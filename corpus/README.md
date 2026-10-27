# Corpus

This folder is the library: pages, five tables, and the public documents they cite. Every file is plain text or CSV, so a text editor and Excel are enough to read all of it. Only public information goes here. The exact rules are in [the specification](../docs/reference/corpus-format.md).

## What is where

| Path | Holds |
|---|---|
| `pages/<area-folder>/<subarea>/<page-id>.md` | One page per file |
| `registers/` | Five CSV tables: `sources`, `claims`, `questions`, `conflicts`, `glossary` |
| `sources/public/` | The public originals, byte for byte, named by source-version id |
| `sources/text/<version-id>/` | The text of one original, a file per page (`p0014.txt` is page 14), and `extraction.toml` |
| `templates/` | A blank page for each page type |
| `corpus.toml` | The areas, the views and the settings |
| `changes.md` | What each edition added, changed and verified |
| `AGENTS.md`, `ASSISTANT.md` | Rules for agents that write here, and for an approved assistant that reads |

## Reading a page

- The lines between the two `+++` markers are the page's fields: id, title, area, status, as-of date.
- The body has fixed parts in order: Purpose, The rule, How it works, Worked example, Never confuse, Company calibration, Sources, Change note. A brief has six: Question, Recommendation, Evidence, Options, Unknowns, Next action.
- "Not established." under a heading means the library has nothing there yet.
- A worked example is synthetic. Its numbers are made up.

## Following a citation

| You see | It is | Look in |
|---|---|---|
| `[@CL-CMP-0011]` | A claim | `registers/claims.csv`. The row names the source version, the place in it and a short quote |
| `[@S-0042-v2, p. 14]` | Page 14 of one version of a source | `sources/public/S-0042-v2.pdf`, or `sources/text/S-0042-v2/p0014.txt` |
| `[@Q-027]` | A question the library cannot yet answer | `registers/questions.csv` |
| `[@XC-14]` | Two statements that disagree | `registers/conflicts.csv` |
| `terms = ["material_weakness"]` | A glossary term the page uses | `registers/glossary.csv` |

## How far to trust a claim

- `verification` is `unchecked` until a person has read the original at the place cited. `primary_source_supported` means that person found the original supports the claim.
- `applicability` says whether the claim applies to the company. `pending_company_confirmation` means nobody there has confirmed it.
- `adoption` says whether the method or number has been taken up for use. `proposed` means it is only offered.
- Two sources agree independently only when a person has reviewed their `source_family` and the families differ. A blank family means unknown, never independent.
- These fields are separate. A checked claim is not thereby approved, and a primary source can still be out of date. The rules for who may change each field are in the specification.

## The 13 areas

A page id begins with its area code, and the page sits in the area's folder under `pages/`. Code, then folder: `fnd` foundation, `mkt` market, `est` estimating, `sup` supply-chain, `rte` indirect-rates, `prg` program-finance, `fpa` fpa, `ctf` contract-finance, `cmp` compliance, `cls` close-revrec, `mna` mna, `opm` operating-model, `ind` industrialization. `corpus.toml` gives each area a one-line summary and its subareas. Three views (`CPY`, `CPT`, `PGM`) are filters over `mkt` and have no folder.

## Editing

Use a text editor. Excel saves a register with a byte-order mark and CRLF line endings, so run `fieldbook fmt` after an edit there, then `fieldbook check`. Each edition carries Excel copies of the registers for reading.
