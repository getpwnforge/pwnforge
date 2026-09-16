# Security Policy

## Reporting a Vulnerability

**PwnForge does not currently offer a paid bug bounty program.** Recognition for a valid report takes the form of credit in the published GitHub Security Advisory (see [Disclosure process](#4-fix--coordinated-disclosure)), unless you'd prefer to stay anonymous. We may revisit this as the project and its funding grow.

**Please do not open a public GitHub issue for security vulnerabilities.**

If you believe you've found a security issue in PwnForge, report it privately:

- **Email**: <security@pwnforge.app>
- **GitHub Private Vulnerability Reporting**: use the "Report a vulnerability" button under this repository's Security tab
- **security.txt**: machine-readable version at [pwnforge.app/.well-known/security.txt](https://pwnforge.app/.well-known/security.txt) ([RFC 9116](https://www.rfc-editor.org/rfc/rfc9116))

Please include:

- A description of the vulnerability and its potential impact
- Steps to reproduce (PoC code or requests are welcome)
- The affected version or commit hash
- Whether the issue is already public or exploited in the wild
- If you're able to, a self-assessed [CVSS v3.1](https://www.first.org/cvss/calculator/3.1) score and vector string (see below); not required, but it speeds up triage

## Disclosure process

### 1. Acknowledgment

We confirm receipt within **72 hours**.

### 2. Severity scoring (CVSS)

Every confirmed vulnerability gets a **CVSS v3.1 score and vector string**, computed or reviewed by a maintainer regardless of whether the reporter supplied one. A self-reported score is a useful starting point, not the final word, since severity depends on deployment context (Cloud vs. self-hosted, Standard vs. Air-Gapped) that only we can fully assess on our own scoring pass. We use the [FIRST.org CVSS v3.1 calculator](https://www.first.org/cvss/calculator/3.1) as the reference tool. The score is recorded in the eventual GitHub Security Advisory alongside the vector string, so anyone can see exactly how it was derived rather than trusting a bare "Critical/High/Medium/Low" label.

### 3. Initial assessment

Within **7 days** of acknowledgment, we share: the CVSS score and vector, confirmed affected versions, and the expected fix timeline. If the issue is already public or being actively exploited, we may accelerate the fix timeline and/or publish a public advisory immediately rather than waiting out the normal disclosure window.

### 4. Fix & coordinated disclosure

We follow coordinated disclosure: the vulnerability stays private until a fix is released, or **90 days** from acknowledgment, whichever comes first, consistent with industry-standard disclosure windows (e.g. Google Project Zero). If a fix genuinely can't land in 90 days, we'll discuss an extension with you directly rather than let the deadline lapse silently.

We publish through **GitHub Security Advisories** (GHSA) rather than a changelog line or a blog post, for reasons that matter specifically because PwnForge is self-hostable:

- GHSA entries mirror into [OSV.dev](https://osv.dev), so container-scanning tools (Trivy, Grype) flag outdated self-hosted images automatically. Air-Gapped operators who never see our website or mailing list still get a signal at their next scan
- Each advisory carries the CVSS score and vector, the affected version range, and the patched version in a structured, machine-readable format, not just prose
- It's what feeds GitHub's own security tab and Dependabot alerts for anyone who forked or vendored the code

Publication is **timed separately from the fix**, not simultaneous with it: publishing full exploit details the same day a patch lands gives Standard and Air-Gapped self-hosted operators (who patch on their own schedule, not automatically) a window where the advisory itself becomes the attacker's roadmap:

| Severity        | Advisory published            |
| --------------- | ----------------------------- |
| Critical / High | 30 days after the fix release |
| Medium / Low    | 7 days after the fix release  |

The fix itself ships as soon as it's ready regardless of this table. Only the public advisory (and CVE details) waits. If the vulnerability is already public or being actively exploited, we publish immediately instead of waiting out the delay.

Once published:

- A CVE is requested through GitHub's advisory workflow when the issue qualifies
- Reporters are credited in the advisory, unless you'd prefer to stay anonymous (tell us your preference when you report)

### 5. Safe harbor

We will not pursue legal action against, or report to law enforcement, anyone who:

- Reports a vulnerability through the channels above in good faith
- Makes a genuine effort to avoid privacy violations, data destruction, and service disruption during testing
- Only interacts with accounts/data they own, or has explicit permission for
- Gives us reasonable time to fix the issue before any public disclosure

This applies to testing self-hosted locally, or the public cloud instance in ways that don't affect other users' workspaces or data.

## Implementation invariants

Rules the codebase already follows, written down because breaking one is
usually invisible in review: the code still compiles, the tests still pass, and
nothing looks wrong until someone goes looking. Referenced from
[AGENTS.md](AGENTS.md).

### JWT claims carry identity, never authority

An access token holds `sub`, `iat` and `exp`, and nothing else. No roles, no
permissions, no email address, no display name.

Two reasons. A JWT is only checked for signature and expiry, so any permission
baked into one stays valid for the rest of its lifetime: demoting someone, or
removing them from a team, would not take effect until their token expired.
And a JWT is not encrypted: it is base64, readable by anyone holding it, so
every claim is published to the client and to anything that logs a request.

Permissions are resolved per request against the database, in the extractor for
the resource being accessed. Suspension is checked the same way, through a
short-lived cache that fails through to the database rather than guessing.

### A 404 hides existence, a 403 admits it

The status code answers "what is this caller allowed to *know*", which is not
the same question as "what are they allowed to do".

- A **member whose role is insufficient** for an action already knows the
  resource exists, since they can see it. Answering `403` tells them nothing new.
- A **non-member**, or anyone who should not know the resource exists at all,
  gets `404`. A `403` here confirms that the slug or id they guessed is real,
  which turns the endpoint into an enumeration oracle.

In practice this means resolving the resource and the caller's membership as a
single step, so "no such team" and "not your team" cannot be told apart. See
`team_service::get_team_for_member`, and `require_role`, which is only reachable
once membership is established.

### Rate limits fail open only where a secret sits underneath

When the limiter itself is unavailable, whether Redis is down or the network
blipped, an endpoint either refuses the request or lets it through. The deciding question
is whether anything else bounds abuse of that route:

| Underneath the endpoint | On limiter failure | Examples |
| --- | --- | --- |
| A password or an opaque token the caller must already hold | Fail **open** | `login`, `password/reset`, `email/resend` |
| Nothing | Fail **closed** (`503`) | `register`, `password/forgot`, `contact`, `setup` |

The second row is the one that matters. Those routes are reachable by anyone
and most of them send mail to an address the caller never proved they own, so
an unlimited one is a free mailer, and for `register`, unbounded account
creation and an Argon2 CPU sink as well.

### The client IP is only as trustworthy as the proxy count

`X-Forwarded-For` is attacker-controlled. `TRUSTED_PROXY_COUNT` says how many
hops to trust, counted from the right: the leftmost entries are whatever the
client chose to prepend.

Set too high, a caller spoofs their own address and dodges every per-IP limit.
Set too low behind a proxy, every caller resolves to the proxy and all per-IP
limits collapse into one shared bucket, at which point a single attacker
exhausting the login window locks out the entire instance. Neither failure is
visible without looking at what the limiter actually keyed on.

## Supported versions

| Version               | Supported           |
| --------------------- | ------------------- |
| Latest stable release | ✅                  |
| Older releases        | ❌ (please upgrade) |

## Scope

In scope:

- The codebase in this repository
- The official Docker images published under `ghcr.io/getpwnforge/*`

Out of scope:

- Third-party dependencies (report upstream, though we appreciate a heads-up)
- Issues requiring physical access to a self-hosted instance's host machine
- Social engineering against maintainers or users

## Our security practices

PwnForge follows secure-by-default principles: strict container hardening (non-root, read-only filesystems, dropped capabilities), no server-side file preview, encrypted secrets at rest, and automated dependency auditing (`cargo audit`, `pnpm audit`, Dependabot).

Thank you for helping keep PwnForge and its community safe.
