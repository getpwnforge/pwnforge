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
