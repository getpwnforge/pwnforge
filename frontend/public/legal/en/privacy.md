---
slug: privacy
lang: en
title: Privacy Policy
description: Data processed by your PwnForge instance, purposes, retention periods, recipients and user rights.
version: "0.1-template"
updated: "[DATE]"
---

> **⚠️ Template, not legal advice.** Starting structure only. If you have any
> EU/EEA or UK users, GDPR/UK-GDPR obligations likely apply to you regardless
> of where you are established — do not assume they don't just because your
> server isn't in Europe. Have this reviewed against the law that actually
> applies to your instance and its users before publishing it.

# Privacy Policy

This policy complements the Legal Notice and Terms of Service.

---

## 1. Data controller

| Item | Value |
| --- | --- |
| Controller | `[YOU / YOUR ORGANISATION]` |
| Country of establishment | `[YOUR COUNTRY]` |
| Data protection contact | `[YOUR PRIVACY EMAIL]` |

State whether you have appointed a Data Protection Officer. Most small,
single-operator instances do not need one under GDPR Article 37, but check
against your actual scale and the nature of what you process, not just by
assumption.

---

## 2. Scope

State clearly that this policy covers your instance only, and explicitly
does **not** cover any further instance someone else may run from your
(AGPL-licensed) source code.

---

## 3. Guiding principles

Worth stating even briefly: minimisation, no data resale, no behavioural
advertising, where data is actually hosted, and — importantly, given the
flag encryption architecture this platform uses — transparency about the
**limits** of your technical safeguards, not just their existence.

---

## 4. Data you process

Copy this table structure and adjust to what your instance actually
collects — do not just copy the cloud instance's list wholesale if you have
disabled features (e.g. no third-party CTF integrations, no marketing
emails):

| Category | Examples |
| --- | --- |
| Account data | Username, email, password hash, optional profile fields |
| Authentication data | Access/refresh tokens, session metadata (IP, user agent) |
| Usage data and logs | Server logs, audit logs of sensitive actions |
| User-generated content | Writeups, comments, challenge data, attachments |
| Support data | Contents of messages sent to your contact addresses |
| Third-party integration data | If you enable CTF platform connectors: public identifiers, tokens |

State plainly what you do **not** collect: payment data (if the instance is
free), special category data under GDPR Article 9, precise geolocation,
unless one of these genuinely applies to your deployment.

---

## 5. Purposes and legal bases

If GDPR applies to you, this table is the load-bearing one — every purpose
needs an actual legal basis, not just a purpose:

| Purpose | Legal basis (GDPR Art. 6) |
| --- | --- |
| Account creation and management | Contract performance, Art. 6(1)(b) |
| Security, fraud and abuse prevention | Legitimate interests, Art. 6(1)(f) |
| Audit logging | Legitimate interests, Art. 6(1)(f) |
| `[Any marketing communications you send]` | Consent, Art. 6(1)(a) |

---

## 6. Retention periods

Another table worth keeping as a skeleton, filled in with your actual
numbers rather than copied from elsewhere:

| Category | Retention period |
| --- | --- |
| Active account | Duration of use |
| Deleted account | `[Your grace period, if any]` |
| Access tokens | `[TTL]` |
| Refresh tokens | `[TTL / rotation policy]` |
| Audit logs | `[Retention period]` |
| Backups | `[Retention period, rotation policy]` |

---

## 7. Recipients and processors

**The single most important table to get right, and the one most likely to
drift out of date.** List every provider whose infrastructure your data
actually transits or rests on — not a copy of anyone else's list:

| Provider | Role | Data involved | Location |
| --- | --- | --- | --- |
| `[Your hosting provider]` | Server hosting, database | All service data at rest | `[Location]` |
| `[Your CDN/DNS provider, if any]` | CDN, DNS, WAF | IP, headers, and — if it proxies your whole app, not just DNS — application content in transit | `[Location]` |
| `[Your email provider, if any]` | Transactional email | Email address, message content | `[Location]` |

If any provider sits outside your users' legal region (e.g. outside the
EU/EEA for European users), add a section on international transfers naming
the actual safeguard in place (Standard Contractual Clauses, an adequacy
decision, a certification) — do not claim a safeguard you have not verified
the provider actually offers.

---

## 8. Cookies and trackers

If your instance sets no tracker beyond what is strictly necessary to
function (session cookies, CSRF protection, a bot-check like Turnstile/
hCaptcha if you use one), you likely do not need a consent banner under
EU/UK law — but this is a factual claim about what your deployment actually
sets, not a default you can assume without checking your own configuration.

| Name | Type | Purpose | Duration |
| --- | --- | --- | --- |
| `[session cookie name]` | `httpOnly` cookie | Session access | `[TTL]` |
| `[refresh cookie name]` | `httpOnly` cookie | Session renewal | `[TTL]` |

---

## 9. Data security

Worth listing honestly, not aspirationally: password hashing algorithm,
encryption in transit, whether backups are encrypted, and — specific to this
platform — the flag encryption mechanism and **its stated limits** (it
protects against a database-only compromise, not against compromise of the
instance administrator or a permission bug). State your actual operational
reality too: single VPS with no redundancy is a very different security
posture from a monitored, redundant deployment, and pretending otherwise
does not help anyone relying on this document.

---

## 10. Your rights

If GDPR or an equivalent regime applies to your users, list the applicable
rights (access, rectification, erasure, portability, objection) and where
requests go (`[YOUR PRIVACY EMAIL]`). Name self-service features your
instance actually has (export from settings, self-service account deletion)
rather than assuming they exist — cross-reference whichever of these are
actually wired up in your deployment.

---

## 11. Complaints

Name the supervisory authority actually competent for you — your national
data protection authority if you're in the EU/EEA, or the relevant
equivalent for your country. Do not default to a specific country's
authority unless you're established there.

---

## 12. Users outside your primary jurisdiction

If your instance has an international user base, briefly note which other
regimes you voluntarily extend equivalent rights to (UK GDPR, Swiss FADP,
LGPD, PIPEDA, CCPA/CPRA are common ones to consider), or state plainly that
you only commit to your home jurisdiction's standard if that is the honest
answer.

---

## 13. Data breaches

State your actual process: who you'd notify, within what timeframe, and
under which law (GDPR's 72-hour rule to your supervisory authority is the
common EU baseline, but check what applies to you).

---

## 14. Instances derived from yours

If your source is public, state that you are not the controller for further
instances others deploy from it, and collect no telemetry from them unless
you have actually built an opt-in mechanism for that.

---

## 15. Changes

State your notice period for material changes (30 days is common practice
in this space) and where changes are announced (email, in-app banner).

---

## 16. Version history

| Version | Date | Changes |
| --- | --- | --- |
| 0.1 | `[DATE]` | Initial version |
