# Raw edition

The delivered file set, the content rules and the verification contract of the raw edition, the floor delivery that ships plain public files only. The procedure (tag, artifacts, archive, rollback) is in the [release runbook](../runbooks/release.md#raw-edition-floor); how the files reach the laptop is in the [transfer runbook](../runbooks/transfer.md). Code: `xtask/src/edition_raw.rs` (packaging), `xtask/src/raw_release.rs` (tag check, evidence, archive verifier) and the Raw profile of `xtask/src/allowlist.rs`.

## Commands

| Command | What it does |
|---|---|
| `cargo xtask edition-raw --out DIR [--fieldbook PROGRAM]` | Packages the clean commit into `DIR`. Refuses a dirty tree (including an ignored file under `corpus/`, a tracked file marked skip-worktree or assume-unchanged, and a sparse checkout) and a blank or missing `FIELDBOOK_SECURITY_CONTACT` (control IN-01). Writes nothing to `DIR` until every check has passed |
| `cargo xtask raw-edition-check TAG --base FILE --evidence-out DIR` | Validates an owner-created `edition-NNN` tag whose committed `edition-release.toml` says `kind = "raw"`, a clean checkout at the tagged commit (the same clean-tree rule as `edition-raw`), and a `--base` identical to the committed baseline; collects the public compatibility evidence |

The program run for the check is `fieldbook check --strict --json`; without `--fieldbook`, `edition-raw` starts it through `cargo run`. Nothing builds a site, starts a renderer or opens a browser.

## Delivered files

`edition-raw` writes five files. Four are sent; the fifth is kept.

| File | Sent | Route |
|---|---|---|
| `fieldbook-edition-NNN.zip` | Yes | The approved transfer channel |
| `fieldbook-originals-NNN.zip` | Yes | The approved transfer channel |
| `SHA256SUMS` (two lines, one per zip) | Yes | The separate hash route |
| `corpus-support.sha256` (one line) | Yes | The separate hash route |
| `corpus-support.toml` | No | Retention copy for the independent archive. The catalogue that is checked on the laptop is `edition\evidence\corpus-support.toml`, extracted from the edition zip, against `corpus-support.sha256` |

Each zip is deterministic: entries sorted by path, the commit time, fixed permissions, deflate only. Each holds one top folder (`edition/` or `originals/`) and its own `SHA256SUMS`.

## What the zips may hold

Both trees are checked against the Raw allowlist profile before anything is written (FB0601, FB0602, FB0608).

- **Edition zip.** Only fixed paths: the shipped Corpus (`corpus/corpus.toml`, `corpus/eval/regression.toml`, the five registers, pages, `corpus/sources/text/` when extraction exists), the assistant rules, `edition.toml`, `README.txt`, `TRANSFER-STATEMENT.txt`, `evidence/check.json`, `evidence/regression.json`, `evidence/corpus-support.toml`, and four files under `intake/`. The extensions that result are `.md`, `.toml`, `.csv`, `.json` and `.txt`. No site file, script, markup, state folder or quarantined payload.
- **Originals zip.** Only registered originals whose bytes match their `sources.csv` hash, and only with the extensions `.pdf`, `.txt` and `.csv` (the constant `RAW_ORIGINAL_EXTENSIONS`). HTML, HTM, XHTML and SVG originals ship with an added `.txt`. Any other extension, and any original that begins like a Windows or Linux executable, is refused. A quarantined version ships nothing.
- PDF originals are third-party public documents shipped byte for byte. They are not inspected for embedded content.

## Evidence in the edition zip

- `evidence/check.json`: the strict check report, exactly as the program printed it. Zero errors is required.
- `evidence/regression.json`: `cases` and `critical_cases` (counted from `regression.toml`), `critical_misses`, `misses` (the non-critical `FB0127` warnings, each listed), `passed`, and the edition, commit and date. A miss of a critical case (`FB0128`) or a malformed set (`FB0126`) stops the packaging. A set with no critical case passes that test vacuously; the counts are written so that a reader sees it.
- `evidence/corpus-support.toml`: the support-file catalogue, and its digest is `corpus-support.sha256`. This is evidence of bytes, not proof that any file is public or admitted.

## The generated texts

`README.txt`, `TRANSFER-STATEMENT.txt` and `intake/integrity-guide.md` and `intake/network-silence.md` are written by the build from templates in `edition_raw.rs`, with every figure counted from the packed files. The other two intake files are the repository's, with a raw-edition note first. A placeholder left unfilled stops the run (IN-01). The texts make no network claim, no browser-test claim and no claim about the content of a PDF. Tests pin that the texts agree with the delivered file set above and with the transfer runbook.

## Verification contract

`cargo xtask archive-release --kind raw` and `cargo xtask verify-archive` (see the release runbook) reopen the archived pair and check: the exact provider file set; the zip hashes against the external `SHA256SUMS`; every file against the internal `SHA256SUMS`; the edition identity and commit; the clean `check.json` and `regression.json` for this edition and commit; the Raw allowlist; the catalogue bytes against the sidecar digest; and the predecessor binding for the next edition. No Site provider and no executable runs for a raw archive.

## Not covered

No signing, no browser test, no network-silence test, no inspection of PDF content, and no check by a second party of the strict-check evidence. The comparison of a local pair with a downloaded workflow artifact is a manual step (`compare_delivered_sets` is exercised only by tests).
