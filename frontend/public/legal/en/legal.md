---
slug: legal
lang: en
title: Legal Notice
description: Publisher, hosting, licence and content reporting for your PwnForge instance.
version: "0.1-template"
updated: "[DATE]"
---

> **⚠️ Template, not legal advice.** This is a starting structure for operators
> self-hosting PwnForge, built from the document used by the official cloud
> instance under French/EU law. Every `[BRACKETED]` field must be filled in
> with your own facts, and every legal reference must be checked against the
> law that actually applies to you: your country of establishment, not
> France, unless you happen to be established there too. Have this reviewed
> by a local professional before publishing it, especially if your instance
> has real users beyond a small private group.

# Legal Notice

## About this document

Three documents typically govern the use of a self-hosted instance:

| Document | Contents |
| --- | --- |
| **Legal Notice** (this document) | Who operates the instance, where it is hosted, under which licence, how to contact the operator |
| Privacy Policy | What data is processed, why, for how long, and what rights users have |
| Terms of Service | Usage rules, content, moderation, liability |

---

## 1. Status of the service

Describe here whether your instance is personal/non-commercial, run by an
organisation, or a commercial offering. This changes which obligations apply
to you (consumer protection, mandatory business registration, VAT, etc.),
which none of the templates here attempt to cover for a commercial case.

---

## 2. Publisher of the service

| Item | Value |
| --- | --- |
| Operator | `[YOUR NAME OR LEGAL ENTITY]` |
| Capacity | `[e.g. natural person, non-professional publisher / registered company]` |
| Contact address | `[YOUR CONTACT EMAIL]` |
| Full identification details | `[Where held — e.g. provided to your hosting provider, or published here directly if you are a registered business]` |

If you operate as an individual rather than a registered business, check
whether your country's equivalent of France's LCEN (or no equivalent at all)
lets you withhold your personal address from public view provided your host
has it on file. Do not assume the French rule (Article 1-1, II of Law
No. 2004-575) applies to you unless you are actually established in France.

---

## 3. Hosting and infrastructure

### 3.1 Application hosting provider

| Item | Value |
| --- | --- |
| Company | `[YOUR HOSTING PROVIDER]` |
| Registered office | `[ADDRESS]` |
| Server location | `[COUNTRY / REGION]` |

### 3.2 Other infrastructure components

List every third party whose infrastructure your instance's traffic or data
actually passes through — DNS, CDN, email delivery, analytics, object
storage. One row per provider, one true role per row:

| Provider | Role | Location |
| --- | --- | --- |
| `[e.g. your DNS/CDN provider]` | `[e.g. DNS, CDN, WAF]` | `[Location]` |
| `[e.g. your email provider]` | `[e.g. transactional email delivery]` | `[Location]` |

---

## 4. Contact

| Subject | Address |
| --- | --- |
| Support, usage questions | `[YOUR SUPPORT EMAIL]` |
| Reporting illegal or policy-violating content | `[YOUR ABUSE EMAIL]` |
| Security vulnerability reports | `[YOUR SECURITY EMAIL]` |
| Personal data requests | `[YOUR PRIVACY EMAIL]` |

---

## 5. Purpose of the service

PwnForge is a collaborative platform for CTF teams and offensive security
practitioners: challenge tracking, collaborative writeups, workspace
statistics. It provides no attack infrastructure and no execution
environment for user-supplied code — state this clearly, since it is the
basis for treating the operator as a host rather than a publisher of
whatever users do with the information they organise on the platform.

---

## 6. Source code licence

PwnForge core is distributed under **AGPL v3**. If you have not modified the
source, say so and link to the upstream repository. If you run a modified
version, **Article 13 of AGPL v3 requires you to make your modified source
available** to every user of your instance — this is not optional, and it is
the one obligation in this whole document that carries real legal
consequences if ignored.

---

## 7. Trade mark and visual identity

Unless you hold trade mark rights over the PwnForge name and logo yourself,
you cannot grant permissions over it — that policy belongs to the upstream
project. State instead that you are running an instance of PwnForge, an open
source project, and link to the upstream project's own trade mark policy for
anything concerning use of its name and logo. If you have renamed or
rebranded your fork, describe your own name/logo policy here instead.

---

## 8. Editorial content

Content you produce (documentation, announcements) is protected by
copyright as usual. Content posted by users remains theirs — cross-reference
your Terms of Service.

---

## 9. Reporting illegal content

State that you act as a host for user-posted content, with no general
obligation to monitor it, and that you act on reports of manifestly illegal
content sent to `[YOUR ABUSE EMAIL]`. If your jurisdiction has a specific
penalty for bad-faith takedown requests (France's LCEN does, at Article 6, I,
4), check whether an equivalent exists where you operate before citing one.

---

## 10. Complaints

State where complaints go (`[YOUR SUPPORT EMAIL]`) and whether a consumer
mediation scheme applies to you. It typically does not for a free,
non-commercial, personal service, but this depends entirely on your local
consumer protection law, not on French law by default.

---

## 11. Hyperlinks

Standard disclaimer: no control over and no liability for third-party links,
linking to your service is permitted absent a false suggestion of
partnership.

---

## 12. Personal data

Cross-reference your Privacy Policy. Name your applicable data protection
authority here (e.g. your national DPA if you are in the EU/EEA, or the
relevant body for your country) — do not default to the CNIL unless you are
actually established in France.

---

## 13. Accessibility

State your accessibility target (e.g. WCAG 2.1 AA) and whether you have
actually been audited against it. Do not claim a conformity level you have
not verified.

---

## 14. Instances derived from yours

If your source is available (which it must be if you modified AGPL-licensed
code and made it network-accessible, per section 6), state plainly that you
are not responsible for further instances deployed by others from your
code, and that each such operator is the data controller for their own
instance.

---

## 15. Credits

List the major open-source components you build on and their licences.

---

## 16. International scope and governing law

State which country's law this document is drafted from (your place of
establishment, not necessarily France), and that a more favourable mandatory
rule in a user's own country of residence still applies where one exists.

---

## 17. Version history

| Version | Date | Changes |
| --- | --- | --- |
| 0.1 | `[DATE]` | Initial version |
