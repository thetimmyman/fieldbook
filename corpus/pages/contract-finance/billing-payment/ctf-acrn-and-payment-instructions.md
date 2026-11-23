+++
id = "ctf-acrn-and-payment-instructions"
title = "Accounting classification reference numbers and payment instructions"
area = "ctf"
subarea = "billing-payment"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-27
handling = ["public"]
terms = ["defense_far_supplement", "funding"]
questions = ["Q-008"]
links = ["ctf-dfars-line-item-and-subline-item-criteria", "ctf-wide-area-workflow-payment-instructions", "ctf-incremental-funding-policy", "ctf-making-payments"]
+++

## Purpose

Shows what the DFARS says about tracing funds with accounting classification reference numbers and about payment instructions, and where the source text stops.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CTF-1623] |
| Codified citation | DFARS 204.7107 [@CL-CTF-1623]; DFARS 204.7108 [@CL-CTF-1624] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CTF-1623] |
| Applies to | DoD contract actions that trace funds using ACRNs and agency accounting identifiers [@CL-CTF-1623] |

Traceability of funds from accounting systems to contract actions is accomplished using ACRNs and AAIs, and the procedures at PGI govern their use [@CL-CTF-1623]. The DFARS says to follow the procedures at PGI for the inclusion of payment instructions in contracts [@CL-CTF-1624].

The DFARS sends the procedures for using ACRNs and for including payment instructions to PGI [@CL-CTF-1623; @CL-CTF-1624].

At line item level, each line item shall reference a single accounting classification citation, except as provided in the paragraph that authorizes multiple citations [@CL-CTF-1583]. One listed situation for multiple citations is a single, nonseverable deliverable paid for with research and development or other funds properly incrementally obligated over several fiscal years under DoD policy [@CL-CTF-1584]. When multiple citations are authorized, informational subline items are established for each one [@CL-CTF-1585]. Subline items used instead of line items to facilitate payment, delivery tracking, contract funds accounting, or other management purposes shall be used in listed situations [@CL-CTF-1589]. One of them is when items bought under one line item are to be paid for from more than one accounting classification, and a subline item is established for the quantity associated with the single accounting classification citation [@CL-CTF-1590]. A line item rather than a subline item is established if it is likely that a subline item may be assigned additional accounting classification citations at a later date [@CL-CTF-1630]. The tests for line items and subline items are on [DFARS criteria for contract line items and subline items](ctf-dfars-line-item-and-subline-item-criteria.md).

## How it works

ACRNs and AAIs trace funds from the accounting systems to the contract action [@CL-CTF-1623]. At line item level, unless multiple citations are authorized for a line item, each line item references a single accounting classification citation, and when items bought under one line item are paid from more than one accounting classification, a subline item is established for the quantity associated with each single citation [@CL-CTF-1583; @CL-CTF-1590], except that a line item is established instead if a subline item is likely to be assigned additional citations later [@CL-CTF-1630]. The payment instructions are included in the contract under the procedures at PGI [@CL-CTF-1624].

## Worked example

Synthetic example. The numbers are made up.

A contract line item for 100 units is paid from two accounting classifications, 70 units from one and 30 from the other. Neither quantity is likely to receive additional accounting classification citations later. The contract carries a subline item for each quantity.

The answer under the source text: two subline items, each tied to one citation. How a payment is split across the two is not established here and is read from the contract's payment instructions and PGI.

## Never confuse

An ACRN differs from the accounting classification citation on a line item: ACRNs are covered by DFARS 204.7107 [@CL-CTF-1623], and the accounting classification citation on a line item by DFARS 204.7103-1 [@CL-CTF-1583]; traceability of funds is accomplished using ACRNs and AAIs [@CL-CTF-1623], while each line item references a single accounting classification citation except as provided in the paragraph that authorizes multiple citations [@CL-CTF-1583]. The electronic invoice and receiving report instructions in the wide area workflow clause are on [Wide Area WorkFlow payment instructions](ctf-wide-area-workflow-payment-instructions.md), incremental funding is on [Incremental funding policy](../funding/ctf-incremental-funding-policy.md), and how payments are made is on [Making payments](ctf-making-payments.md).

## Company calibration

How the company ties each contract line's funding to its own funds tracking, and who owns funding surveillance. [@Q-008]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-11-23: Never confuse states a contrast.
- COMPATIBLE_EXTENSION, 2026-10-31: Removed uncited negative and contrast clauses; added the later-assignment qualification.
- COMPATIBLE_EXTENSION, 2026-10-31: Named the multiple-citation exception in the summary.
- COMPATIBLE_EXTENSION, 2026-10-31: Named the separately identified subline items and the lead-in that the accounting-classification situation belongs to.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for accounting classification reference numbers and payment instructions.
