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
through to the database rather than guessing.

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
"an archived workspace is read-only", is missing from the other, which quietly
becomes a side door around it.

## Rate limits fail open only where a secret sits underneath

When the limiter itself is unavailable, whether Redis is down or the network
blipped, an endpoint either refuses the request or lets it through. The
deciding question is whether anything else bounds abuse of that route:

| Underneath the endpoint | On limiter failure | Examples |
| --- | --- | --- |
| A password or an opaque token the caller must already hold | Fail **open** | `login`, `password/reset`, `email/resend` |
| Nothing | Fail **closed** (`503`) | `register`, `password/forgot`, `contact`, `setup` |
| A session any account can open, and nothing bounding the work | Fail **closed** (`503`) | avatar upload |

The last two rows are the ones that matter. The routes in the second are
reachable by anyone and most of them send mail to an address the caller never
proved they own, so an unlimited one is a free mailer, and for `register`,
unbounded account creation and an Argon2 CPU sink as well. The avatar upload
needs a session, but any account will do, and it hands attacker-chosen bytes
to an image decoder: an unmetered one is a CPU sink.

## The client IP is only as trustworthy as the proxy count

`X-Forwarded-For` is attacker-controlled. `TRUSTED_PROXY_COUNT` says how many
hops to trust, counted from the right: the leftmost entries are whatever the
client chose to prepend.

Set too high, a caller spoofs their own address and dodges every per-IP limit.
Set too low behind a proxy, every caller resolves to the proxy and all per-IP
limits collapse into one shared bucket, at which point a single attacker
exhausting the login window locks out the entire instance. Neither failure is
visible without looking at what the limiter actually keyed on.
