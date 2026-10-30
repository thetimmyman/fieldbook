+++
id = "mkt-contract-line-items-and-data-elements"
title = "Contract line items: subline items and required data elements"
area = "mkt"
subarea = "feeds"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["federal_acquisition_regulation"]
questions = ["Q-003"]
links = ["mkt-fpds-reportable-contract-actions", "mkt-fpds-reporting-responsibilities-and-deadlines"]
+++

## Purpose

Shows when subline items are used and what data each line item or subline item must carry in a contract schedule.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified FAR [@CL-MKT-0344] |
| Codified citation | FAR 4.1004 and FAR 4.1005-1 [@CL-MKT-0344; @CL-MKT-0345; @CL-MKT-0370; @CL-MKT-0371; @CL-MKT-0372] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-MKT-0344] |
| Applies to | Each line item or subline item in the schedule or a comparable section of a procurement instrument, except as the section provides [@CL-MKT-0345] |

Subline items may be used to facilitate tracking of performance, deliverables, payment, and contract funds accounting or for other management purposes [@CL-MKT-0344]. Subline items may be either deliverable or informational [@CL-MKT-0344].

Except as the section provides, each line item or subline item shall include in the schedule, at a minimum, the listed information as separate, distinct data elements [@CL-MKT-0345]. The listed elements start with the line item or subline item number established in accordance with agency procedures, a description of what is being purchased, and the Product or Service Code [@CL-MKT-0346]. The next is the accounting classification citation for line items or deliverable subline items, and if multiple accounting classifications for a single deliverable apply, the dollar amount for each is included [@CL-MKT-0347]. For informational subline items, an accounting classification citation is not required [@CL-MKT-0370]. Fixed-price line items show unit of measure, quantity, unit price and total price [@CL-MKT-0348]. Cost-reimbursement line items show unit of measure, quantity, estimated cost, fee if any, and total estimated cost plus any fee [@CL-MKT-0348]. Each deliverable line item or deliverable subline item shall have its own delivery schedule, destination, period of performance, or place of performance expressly stated in the appropriate section of the procurement instrument [@CL-MKT-0349]. When a line item has deliverable subline items, those terms shall be identified at the subline item level, rather than the line item level [@CL-MKT-0372]. If a contract contains a combination of fixed-price, time-and-materials, labor-hour, or cost-reimbursable line items, the contract type for each line item must be identified in the schedule or a comparable section of the procurement instrument to facilitate payment [@CL-MKT-0371].

## How it works

Subline items may be used for tracking performance, deliverables, payment and contract funds accounting [@CL-MKT-0344]. A line item or subline item carries the same basic data elements [@CL-MKT-0345]. The accounting classification citation is listed for line items and deliverable subline items but is not required for informational subline items [@CL-MKT-0347; @CL-MKT-0370]. The price fields depend on whether the line is fixed-price or cost-reimbursement [@CL-MKT-0348]. Delivery terms sit at the subline item level when a line item has deliverable subline items [@CL-MKT-0372]. The sections also say what a line item with subline items contains and which contract type its subline items take, and this card does not restate those rules.

## Worked example

Synthetic example. The numbers are made up.

A contract has one fixed-price line item for supplies. The buyer sets up two deliverable subline items so that payment for each can be tracked. Answer: each subline item carries its own number, description, Product or Service Code, accounting classification, unit of measure, quantity, unit price and total price, and its own delivery schedule or destination.

## Never confuse

Line items and subline items in a contract schedule are not the contract action reports that agencies enter in the Federal Procurement Data System. Those reports are on the [Federal Procurement Data System: what is reported](mkt-fpds-reportable-contract-actions.md) card, and who reports and when is on the [Federal Procurement Data System: who reports a contract action and when](mkt-fpds-reporting-responsibilities-and-deadlines.md) card. A deliverable subline item is also not an informational one, and the accounting classification citation is listed for line items or deliverable subline items [@CL-MKT-0344; @CL-MKT-0347; @CL-MKT-0370].

## Company calibration

Which contracts and line structures are inside the finance remit, and which line numbers and Product or Service Codes the company's systems carry. [@Q-003]

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for contract line items, subline items and required data elements.
