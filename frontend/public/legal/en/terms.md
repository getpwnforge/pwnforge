---
slug: terms
lang: en
title: Terms of Service
description: Rules for accessing and using your PwnForge instance, content, moderation, suspension and liability.
version: "0.1-template"
updated: "[DATE]"
---

> **⚠️ Template, not legal advice.** Starting structure only, adapt every
> `[BRACKETED]` field and every legal reference to your own situation and
> jurisdiction before publishing.

# Terms of Service

State plainly whether the service is free, paid, or something in between —
this single fact changes which sections below actually need teeth (consumer
protection and sale terms become relevant the moment money changes hands).

---

## 1. Purpose

These Terms govern access to and use of your PwnForge instance, operated by
`[YOU / YOUR ORGANISATION]`, identified in the Legal Notice.

---

## 2. Definitions

Keep the core vocabulary, it saves rewriting it every section:

| Term | Definition |
| --- | --- |
| **Operator** | `[You / your organisation]`, who runs this instance. |
| **Service** | This PwnForge instance, at `[YOUR DOMAIN]`. |
| **Account**, **Workspace**, **User Content**, **Flag** | Same meanings as used throughout the platform's own UI. |

---

## 3. Acceptance

State that use requires accepting these Terms via an explicit action at
account creation, and that a person acting for an organisation represents
they have authority to bind it.

---

## 4. Nature of the service

State honestly what stage your instance is at (personal project, hobby,
production service backing a real team/company) and what it does **not**
provide: attack infrastructure, code execution for arbitrary user input,
network relays. This framing is what supports treating you as a host for
user content rather than a publisher of whatever your users organise on the
platform.

---

## 5. Pricing

If free: say so plainly, and if you might introduce pricing later, consider
committing to advance notice and no silent migration to a paid plan, the way
the upstream project does. If already paid: this template does not cover
sale terms, consumer withdrawal rights, or billing — that needs its own
document and is out of scope here.

---

## 6. International scope

State which law these Terms are drafted from (your place of establishment)
and that a more favourable mandatory rule of a user's own country of
residence still prevails where one exists.

---

## 7. Eligibility

State your minimum age (16 is common for this kind of platform, but check
your local rules — some jurisdictions set 13, others higher) and any other
access conditions (e.g. sanctions compliance) relevant to you.

---

## 8. User account

Standard clauses worth keeping: accurate information at creation, security
responsibility for credentials, one account per person, self-service
deletion available at any time.

---

## 9. Workspaces and teams

State that access follows server-resolved roles, and that workspace/team
creators are responsible for their space's composition and content
compliance.

---

## 10. Acceptable use policy

**Keep this section close to the original almost verbatim** — it is the part
doing the real legal work for a platform aimed at an offensive security
audience, and weakening it casually would be a mistake. Adjust only the
named laws to reflect what's actually relevant to your jurisdiction and your
users' likely jurisdictions (the original cites French, US and UK computer
misuse statutes as examples; add or swap in your own):

- No unauthorised system intrusion, and no hosting of ready-to-use malicious
  payloads or C2/phishing infrastructure aimed at real targets.
- No publishing of unlawfully obtained data (breach dumps, real credentials,
  third-party personal data obtained without consent).
- No manifestly illegal content (CSAM, terrorism, hate speech, and so on —
  keep this list, do not soften it).
- No attacking the Service itself outside a defined responsible-disclosure
  scope (cross-reference your security research section).
- A user documenting a security assessment represents that they hold valid
  authorisation for it; you do not verify this and are not liable for its
  absence.

---

## 11. User content

Keep the core structure: users retain ownership, they grant you only the
narrow licence needed to actually host and display their content (never a
licence to exploit it for other purposes or train models on it), and the
licence ends when content is deleted subject to normal backup-purge delay.
If your users may be under an NDA (common for pentest-adjacent writeups),
keep the clause reminding them that private-visibility settings do not
override a confidentiality obligation they're separately bound by.

---

## 12. Flags and sensitive content

Cross-reference your Privacy Policy's description of flag encryption and its
limits. Add a line making clear users must not rely on this mechanism beyond
what it actually protects against.

---

## 13. Third-party integrations

If you enable any CTF platform connectors, state that connecting is
user-initiated, tokens are encrypted and revocable, and you are not
responsible for third-party platform availability or API changes.

---

## 14. Licence and trade mark

State that the core is AGPL v3, restricting none of the freedoms it grants.
For the trade mark point: **only include a policy here if you actually hold
rights over your instance's name/branding.** If you are running unmodified
upstream PwnForge, link to the upstream project's own trade mark policy
instead of restating one you have no authority over.

---

## 15. Reporting and moderation

Keep the escalation ladder as a fair, proportionate structure: warning →
content removal → feature restriction → suspension → termination, applying
the least restrictive measure that resolves the issue. State where reports
go (`[YOUR ABUSE EMAIL]`).

---

## 16. Suspension and termination

This is the section most worth keeping close to the source, since it
reflects a deliberate, defensible design choice: **notify that a measure was
taken and its scope, without disclosing the detailed detection reasoning**,
because in an offensive-security context, explaining exactly how you caught
something teaches people how to evade it next time. Keep:
- a path for a reasoned human review request, with no time bar;
- the effects of termination (data deleted/anonymised per your retention
  periods, an export window before permanent deletion unless the case
  involves illegal content or a security threat);
- a notice period if you ever discontinue the service entirely.

---

## 17. Security research and disclosure

State your responsible-disclosure scope (testing on the researcher's own
account or a local instance, no data exfiltration from other users, no
availability degradation, coordinated disclosure) and where reports go
(`[YOUR SECURITY EMAIL]`). Whether you offer a reward is entirely up to you
and your resources.

---

## 18. Availability and changes

State your actual availability posture honestly (single server, no uptime
commitment, is a very different statement from a monitored redundant setup)
and your notice period for withdrawing a substantial feature.

---

## 19. No warranty / 20. Liability

Standard "as is" disclaimer, plus a liability cap appropriate to your
situation — a nominal cap only makes sense for a genuinely free,
non-commercial service; do not copy a token cap if you are charging money.
Note explicitly that some jurisdictions do not allow excluding all implied
warranties, and that mandatory local law overrides this clause where it
applies.

---

## 21. Personal data

Cross-reference your Privacy Policy as forming part of these Terms.

---

## 22. Instances derived from yours

If your source is public and someone else deploys a further instance from
it, state that these Terms do not apply to that instance, its operator bears
full responsibility for it, and they must comply with AGPL v3 Article 13
(making their modified source available) if they changed anything.

---

## 23. Changes to these terms

State your notice period for material changes and that continued use after
that period constitutes acceptance, while changes required by law may take
effect immediately.

---

## 24. Miscellaneous

Standard boilerplate worth keeping: entire agreement, severability, no
waiver, assignment conditions, force majeure, controlling language if you
publish in more than one, and that your technical/audit logs are admissible
as evidence between the parties.

---

## 25. Governing law and disputes

State your governing law (your place of establishment) and where disputes
are heard, while noting that a user's own country of residence may provide
a mandatory right to sue locally that this clause cannot remove.

---

## 26. Version history

| Version | Date | Changes |
| --- | --- | --- |
| 0.1 | `[DATE]` | Initial version |
