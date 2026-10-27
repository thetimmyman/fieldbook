# Security policy

Fieldbook is the owner's finance tool. It runs inside the company boundary, on the company laptop and network, as an extension of company infrastructure, and it is meant to become a company application. This repository holds code, public sources and synthetic data only; company data lives in company storage. The default (silent) build and every edition are offline; the optional AI build can call a model endpoint that the owner configures and the company's policy lists, through one audited gate. This file says how to report a security problem, which versions receive fixes and what is in scope. Fieldbook claims no certification and no compliance status under CMMC, NIST SP 800-171, FIPS or any other scheme. The controls it does have, and the evidence for each, are in [docs/controls.md](docs/controls.md). What IT receives, and when, is in [docs/intake/README.md](docs/intake/README.md).

## Fields (RFC 9116 names)

```text
Contact: the owner (role); the route is printed in TRANSFER-STATEMENT.txt of every edition
Expires: checked at every tool release; no date is stored in this file
Preferred-Languages: en
Policy: SECURITY.md
```

RFC 9116 asks for a URI in `Contact` and a date in `Expires`. Fieldbook publishes no `security.txt`, because it has no public site and no public address. A published copy would use an address and a date that IT supplies.

## How to report

1. Write to the contact role above. The route is set at intake ([IT-10](docs/intake/it-questions.md)) and passed to packaging as `FIELDBOOK_SECURITY_CONTACT`, never committed here; CI receives the owner-provisioned same-named secret.
2. Do not open a public issue or pull request with details.
3. Say which edition or tool version, which file or page, the steps, and what you expected against what happened.
4. Use synthetic data only. Never put company, customer, personal or controlled data in a report.
5. If you find controlled unclassified information, export-controlled, proprietary third-party or personal data in the repository or in an edition, report it as a security problem. The owner removes it and issues a new edition.

Targets (judgment, not contract):

| Event | Target |
|---|---|
| Acknowledge a report | 5 business days |
| Dependency flaw on the CISA Known Exploited Vulnerabilities list | Patch or mitigate within 14 days of listing |
| Dependency flaw rated high | Within 30 days |
| Anything else | The next planned release |

A fix to the program ships as a new tool release, which IT must approve again. A fix to content ships as a new edition. The owner tells IT about every fix that touches an installed version, through the channel IT names.

## Supported versions

| What | Receives fixes |
|---|---|
| Tool `v0.x`, before R2 | The newest release only |
| Tool `v1.x`, from R2 | The newest minor release; IT may ask for a longer window |
| Editions | The newest edition only; the previous zip is the rollback, not a supported version |

## Scope

Fieldbook:
- reads the Corpus (Markdown, CSV, public originals) and checks it;
- renders a static site, a read-only workbook and Excel-safe CSV copies, and packs them into two zips per edition;
- from Day 30, answers questions with AI: it builds a context pack for an assistant IT has approved (Handoff) or, in the AI build (from `v0.2.0`, not part of the first edition), for an endpoint IT has approved, and checks the citations in the answer;
- from Day 30, serves read-only search to an AI host that IT has approved (`fieldbook mcp`, from `v0.2.0`);
- from R2, runs as one signed program installed by IT, holds company data in company storage under the labels the company's policy lets that store hold, and syncs from the company data sources the owner may access (from folders of exports; [ADR 0004](docs/decisions/0004-company-boundary.md));
- applies one policy file that the company owns and Fieldbook never writes: every path out of a store (a model, the tool server, an export, a backup, print and copy controls) is checked against it, and the program's journal records the decision where IT can export it ([the policy file](docs/reference/policy.md)).

Fieldbook never:
- opens a network connection in the silent build; in the AI build (from `v0.2.0`) opens one only to an endpoint that the company's policy lists and the owner's settings name, on an explicit command; opens a listener other than the loopback chat page (127.0.0.1, only while `fieldbook chat` runs, and only in a build with the Cargo feature `bridge`); or starts another process (shipped code; the dev-only `xtask` crate is never shipped);
- contains a model, telemetry or an updater;
- needs administrator rights, or installs a service, driver, scheduled task or startup entry;
- classifies a document by what it means, strips or edits a marking, or claims compliance with any scheme (it holds controlled unclassified or export-controlled material only in a store that the company has authorised for it, with the owner's own access, and sends it nowhere unless the company's policy says so);
- sends anything from company storage to the source repositories or the home side, or to any destination that the company's policy has not approved for the labels the data carries.

On memory safety the claim is limited. First-party crates forbid unsafe code (a compile-time lint), except one file of the leaf crate `fieldbook-win` that makes the Windows system calls (a test fails on `unsafe` anywhere else). deny.toml bans the native crates it lists. Dependencies, the Rust standard library and the platform C runtime remain trusted components; these lints are not a sandbox. The evidence is CD-01 and NS-01 in [docs/controls.md](docs/controls.md), and a docs lint fails any line that says "`unsafe` code" or "C code" without "first-party" or "build graph".

In scope for reports: this repository, its workflows, editions, tool releases, and any claim in these documents that the evidence does not back. Out of scope: Edge, Excel, Windows, the transfer channel, GitHub and the assistants. Report those to their owners. Do tell us if a file we ship makes such a flaw easier to exploit.
