# Security invariants

Rules the codebase already follows, written down because breaking one is
usually invisible in review: the code still compiles, the tests still pass, and
nothing looks wrong until someone goes looking. Referenced from
[AGENTS.md](../AGENTS.md) and [SECURITY.md](../SECURITY.md).

## JWT claims carry identity, never authority

An access token holds `sub`, `iat` and `exp`, and nothing else. No roles, no
permissions, no email address, no display name.

Two reasons. A JWT is only checked for signature and expiry, so any permission
baked into one stays valid for the rest of its lifetime: demoting someone, or
removing them from a team, would not take effect until their token expired.
And a JWT is not encrypted: it is base64, readable by anyone holding it, so
every claim is published to the client and to anything that logs a request.

Permissions are resolved per request against the database, at the moment the
resource is loaded: in its extractor where one exists (`WorkspaceMember`), or in
the service call the handler makes first (`team_service::get_team_for_member`).
Suspension is checked the same way, through a short-lived cache that fails
through to the database rather than guessing. A request fills that cache only
where it is empty, and a change that blocks an account (a suspension, a
deletion) writes the blocked state into it rather than deleting the entry: a
request that read the account just before the change cannot then cache it as
active again.

## A 404 hides existence, a 403 admits it

The status code answers "what is this caller allowed to *know*", which is not
the same question as "what are they allowed to do".

- A **member whose role is insufficient** for an action already knows the
  resource exists, since they can see it. Answering `403` tells them nothing new.
- A **non-member**, or anyone who should not know the resource exists at all,
  gets `404`. A `403` here confirms that the slug or id they guessed is real,
  which turns the endpoint into an enumeration oracle.
- A **team member excluded from a workspace** counts as a non-member of it,
  even though their team can see it: they get `404`, exactly as if the
  workspace did not exist.

In practice this means resolving the resource and the caller's membership as a
single step, so "no such resource" and "not yours" cannot be told apart. See
`team_service::get_team_for_member` and `workspace_service::get_workspace_for_member`.
Role checks (`team_service::require_role`, `Membership::satisfies`) are only
reachable once membership is established.

## One access resolution per resource, whatever the route

Every route that reaches a workspace (its detail, its members, its avatar, and
later its attachments) resolves access through the same function in
`workspace_service`, by slug or by id. No route re-derives the rule locally.

A second copy drifts without anyone noticing: a rule added to one path, such as
"an archived workspace's content is read-only", is missing from the other, which
quietly becomes a side door around it. (Read-only covers the content, not who
reaches it: excluding a member or removing a guest stays possible on an archived
workspace, while a write that can grant or change access is refused.)

## Rate limits fail open only where a secret sits underneath

When the limiter itself is unavailable, whether Redis is down or the network
blipped, an endpoint either refuses the request or lets it through. The
deciding question is whether anything else bounds abuse of that route:

| Underneath the endpoint | On limiter failure | Examples |
| --- | --- | --- |
| A password or an opaque token the caller must already hold | Fail **open** | `login`, `password/reset`, `email/verify`, `email/resend` |
| Nothing | Fail **closed** (`503`) | `register`, `password/forgot`, `contact`, `setup` |
| A session any account can open, and nothing bounding the work | Fail **closed** (`503`) | avatar upload, password change |
| A session on any team, and a mail to an address the caller names | Fail **closed** (`503`) | team and workspace invitations, by username or by address |

The last three rows are the ones that matter. The routes in the second are
reachable by anyone and most of them send mail to an address the caller never
proved they own, so an unlimited one is a free mailer, and for `register`,
unbounded account creation and an Argon2 CPU sink as well. The avatar upload
needs a session, but any account will do, and it hands attacker-chosen bytes
to an image decoder: an unmetered one is a CPU sink. A password change has a
password underneath, but behind a session: whoever holds a stolen one could
otherwise guess the current password as fast as the server runs Argon2, and
make it run Argon2 as often. An invitation needs no more than a team of one's
own, and each one mails the invitee.

The invitation limits are per flow (team or workspace, by username or by
address), each with one window per author and one per team or workspace.
Under them sits one more, on the address mailed: three unsolicited mails a
day, whoever causes them, invitations and sign-up notices counted together.
That one never refuses the request, since a refusal would tell the author
the address was being flooded: past it, or when it cannot be read, the
request is answered as usual and only its mail is not sent. Its key holds
the address hashed.

A window keyed on something a victim owns, an identifier or an address,
counts only the requests that got past the checks that justify it. Anyone
could fill it otherwise, and lock the victim out of signing in or of
resetting their password. So the IP window comes first, then Turnstile, then
that window: a sign-in counts against its identifier only once Turnstile has
passed and the password has failed, and a success clears the count; a reset
request spends its address window after Turnstile; a resend checks its IP
window before the ones kept per token.

## An address is verified only from the mail sent to it

`email/verify` accepts one thing: the link mailed to the address. A sign-in
on an unverified address answers with a token of another kind,
`email_verification_resend`, valid for an hour, which `email/resend` accepts
and `email/verify` never does. Knowing the password proves nothing about the
mailbox: an account registered with someone else's address would otherwise
verify it, and receive what is addressed to it, invitations included. The
page the link opens verifies on a click, not on load, so a mail scanner that
follows links does not spend it.

Every link that carries a token carries it in the fragment (`#token=`),
which the browser never sends, so it stays out of every access log; a page
that reads one drops it from the address bar. An account that verifies no address
within seven days is deleted by the cleanup task, so an address registered
by someone who does not own it is not held for good.

## Registration never says whether an address has an account

`auth/register` answers `202` with no body and no cookie in three cases that
must not be told apart: the address is free, it belongs to a verified
account, or it is held by a registration nobody confirmed. The status, the
headers, the body and the time it takes are the same. Only the username is
an explicit refusal (`username_taken`), since usernames are public, and it
is checked before anything about the address, so a taken username gets the
same answer whatever address comes with it.

The same work runs in every case: the Argon2 hash, then the inserts, where
the unique constraint on the address decides. A free address commits, a
taken one rolls the same statements back, and two registrations racing for
one address end the way one finding it taken does. No address is looked up
beforehand. The mail goes out afterwards in a task of its own, whatever the
case, and only the address learns which case it was: a new account gets its
verification link; a verified holder, a notice with plain links to sign in
and to reset the password; a registration nobody confirmed, the date it is
deleted and the address freed. That last notice never carries the pending
account's verification link: whoever reads it owns the mailbox, not that
account. Both notices count against the daily cap on unsolicited mail.

Two places still tell. Inviting by address answers `AlreadyMember` when a
member of the inviter's own team or workspace has that address verified,
which tells the inviter which address one of their members holds. The setup
wizard, behind its token and before any account can exist, names a taken
address (`admin_email_taken`).

## The client IP is only as trustworthy as the proxy count

`X-Forwarded-For` is attacker-controlled. `TRUSTED_PROXY_COUNT` says how many
hops to trust, counted from the right: the leftmost entries are whatever the
client chose to prepend.

Set too high, a caller spoofs their own address and dodges every per-IP limit.
Set too low behind a proxy, every caller resolves to the proxy and all per-IP
limits collapse into one shared bucket, at which point a single attacker
exhausting the login window locks out the entire instance. Neither failure is
visible without looking at what the limiter actually keyed on.
