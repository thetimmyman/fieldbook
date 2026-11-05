+++
id = "ctf-dfars-line-item-numbering-and-exhibits"
title = "DFARS line item and subline item numbering, exhibits and attachments"
area = "ctf"
subarea = "billing-payment"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-27
handling = ["public"]
terms = ["defense_far_supplement"]
questions = ["Q-009"]
links = ["mkt-contract-line-items-and-data-elements", "ctf-dfars-line-item-and-subline-item-criteria", "ctf-dfars-modification-line-items-and-order-of-application"]
+++

## Purpose

Shows where the numbering rules for line items, subline items and exhibits sit, and when an exhibit replaces a long list of line items.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CTF-1592] |
| Codified citation | DFARS 204.7103-2 [@CL-CTF-1592]; DFARS 204.7104-2 [@CL-CTF-1593]; DFARS 204.7105 [@CL-CTF-1594]; DFARS 204.7100 [@CL-CTF-1399] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CTF-1592] |
| Applies to | Contract line items, subline items, exhibits and attachments in DoD contracts [@CL-CTF-1592; @CL-CTF-1593; @CL-CTF-1594] |

The subpart prescribes policies and procedures for assigning contract line item numbers [@CL-CTF-1399]. The DFARS says to follow the procedures at PGI 204.7103-2 for numbering contract line items [@CL-CTF-1592], the procedures at PGI 204.7104-2 for numbering contract subline items [@CL-CTF-1593], and the procedures at PGI 204.7105 for use and numbering of contract exhibits and attachments [@CL-CTF-1594].

Exhibits may be used as an alternative to putting a long list of contract line items in the schedule, and if exhibits are used a contract line item is created citing the exhibit's identifier [@CL-CTF-1595]. One exception to the single unit price or single total price requirement is when the contract line item is established to refer to an exhibit or an attachment [@CL-CTF-1577; @CL-CTF-1580], and if management needs dictate that a unit price be entered, the price shall be set forth in the item description block and enclosed in parentheses [@CL-CTF-1580]. All subline items and exhibit line items under one contract line item shall be the same contract type as the contract line item [@CL-CTF-1629].

## How it works

Exhibits may be used as an alternative to putting a long list of contract line items in the schedule [@CL-CTF-1595]. If exhibits are used, a contract line item is created citing the exhibit's identifier [@CL-CTF-1595]. A contract line item established to refer to an exhibit is one of the exceptions to the single unit price or single total price requirement [@CL-CTF-1577; @CL-CTF-1580]. If new items are added, new contract line or subline item numbers or exhibit line item numbers are assigned in accordance with the procedures established at DFARS 204.7103, 204.7104 and 204.7105 [@CL-CTF-1596].

## Worked example

Synthetic example. The numbers are made up.

A schedule would otherwise list 80 spare parts one by one. The contract instead puts them in one exhibit and adds a single line item that cites the exhibit's identifier. If management needs dictate that a unit price be entered for that line, it is set out in parentheses in the item description block.

The answer: one line item points to the exhibit, and the numbering of the exhibit and its lines follows the procedures at PGI, which this source text does not set out.

## Never confuse

Contracts shall identify the items or services to be acquired as separate contract line items unless it is not feasible to do so [@CL-CTF-1575]. The tests for a separate line item are in [DFARS criteria for contract line items and subline items](ctf-dfars-line-item-and-subline-item-criteria.md). The data elements each line must show are on [Contract line items and data elements](../../market/feeds/mkt-contract-line-items-and-data-elements.md). How a modification changes items is on [Line items in DFARS contract modifications and the order modifications apply](ctf-dfars-modification-line-items-and-order-of-application.md).

## Company calibration

How the company's billing system carries exhibit line items and subline items so that invoices match the contract schedule. [@Q-009]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-10-31: Cut uncited negatives and contrasts; stated PGI references and the exhibit price exception as written.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the scope of the line item numbering subpart.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for DFARS line item numbering and exhibits.
