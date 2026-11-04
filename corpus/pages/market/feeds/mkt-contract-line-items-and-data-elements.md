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
| Instrument | Codified FAR [@CL-MKT-0344]; RFO model text [@CL-MKT-9275] |
| Codified citation | FAR 4.1004 and FAR 4.1005-1 [@CL-MKT-0344; @CL-MKT-0345; @CL-MKT-0370; @CL-MKT-0371; @CL-MKT-0372]; FAR 2.101 [@CL-MKT-9236; @CL-MKT-1834] |
| RFO citation | RFO FAR 2.101 [@CL-MKT-9275; @CL-MKT-1835] |
| Edition date | Not established. |
| Text status | Final [@CL-MKT-0344; @CL-MKT-9275] |
| Applies to | Each line item or subline item in the schedule or a comparable section of a procurement instrument, except as provided in FAR 4.1005-2 [@CL-MKT-0345] |

Subline items may be used to facilitate tracking of performance, deliverables, payment, and contract funds accounting or for other management purposes [@CL-MKT-0344]. Subline items may be either deliverable or informational [@CL-MKT-0344].

Under FAR 2.101, a subline item means a subset of a line item [@CL-MKT-9236], and the use of the term "line item" includes "subline item," as applicable [@CL-MKT-1834]. Under RFO FAR 2.101, a subline item means a subset of a line item [@CL-MKT-9275], and the use of the term "line item" includes "subline item," as applicable [@CL-MKT-1835].

Except as provided in FAR 4.1005-2, each line item or subline item shall include in the schedule (described at FAR 12.303(b)(4), 14.201-2, or 15.204-2, or in a comparable section of the procurement instrument), at a minimum, the following information as separate, distinct data elements [@CL-MKT-0345]. The first three elements are the line item or subline item number established in accordance with agency procedures, a description of what is being purchased, and the Product or Service Code (PSC) [@CL-MKT-0346]. The fourth is the accounting classification citation: for line items or deliverable subline items, if multiple accounting classifications for a single deliverable apply, the dollar amount for each accounting classification is included in the schedule (or a comparable section of the procurement instrument) [@CL-MKT-0347], and for informational subline items, an accounting classification citation is not required (see FAR 4.1004) [@CL-MKT-0370]. Under the fifth element, fixed-price line items include unit of measure, quantity, unit price, and total price, and cost-reimbursement line items include unit of measure, quantity, estimated cost, fee (if any), and total estimated cost plus any fee [@CL-MKT-0348]. Each deliverable line item or deliverable subline item shall have its own delivery schedule, destination, period of performance, or place of performance expressly stated in the appropriate section of the procurement instrument ("as required" constitutes an expressly stated delivery term) [@CL-MKT-0349]. When a line item has deliverable subline items, the delivery schedule, destination, period of performance, or place of performance shall be identified at the subline item level, rather than the line item level [@CL-MKT-0372]. If a contract contains a combination of fixed-price, time-and-materials, labor-hour, or cost-reimbursable line items, the contract type for each line item must be identified in the schedule (or a comparable section of the procurement instrument) to facilitate payment [@CL-MKT-0371].

## How it works

Subline items may be used to facilitate tracking of performance, deliverables, payment, and contract funds accounting or for other management purposes, and may be either deliverable or informational [@CL-MKT-0344]. For line items or deliverable subline items, if multiple accounting classifications for a single deliverable apply, the dollar amount for each accounting classification is included in the schedule (or a comparable section of the procurement instrument) [@CL-MKT-0347], whereas for informational subline items an accounting classification citation is not required [@CL-MKT-0370]. Fixed-price line items include unit of measure, quantity, unit price, and total price, and cost-reimbursement line items include unit of measure, quantity, estimated cost, fee (if any), and total estimated cost plus any fee [@CL-MKT-0348]. When a line item has deliverable subline items, the delivery schedule, destination, period of performance, or place of performance shall be identified at the subline item level, rather than the line item level [@CL-MKT-0372].

## Worked example

Synthetic example. The numbers are made up.

A contract has one fixed-price line item for supplies. The buyer sets up two deliverable subline items so that payment for each can be tracked. Answer: each subline item carries its own number, description, Product or Service Code, accounting classification, unit of measure, quantity, unit price and total price, and its own delivery schedule or destination.

## Never confuse

Subline items may be either deliverable or informational [@CL-MKT-0344]. For line items or deliverable subline items, the accounting classification citation is a required data element and, if multiple accounting classifications for a single deliverable apply, the dollar amount for each accounting classification is included in the schedule (or a comparable section of the procurement instrument) [@CL-MKT-0345; @CL-MKT-0347], whereas for informational subline items an accounting classification citation is not required [@CL-MKT-0370]. Contract action reports entered in the Federal Procurement Data System are covered on the [Federal Procurement Data System: what is reported](mkt-fpds-reportable-contract-actions.md) card, and reporting responsibilities and deadlines are on the [Federal Procurement Data System: who reports a contract action and when](mkt-fpds-reporting-responsibilities-and-deadlines.md) card.

## Company calibration

Which contracts and line structures are inside the finance remit, and which line numbers and Product or Service Codes the company's systems carry. [@Q-003]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-10-31: Separated codified and RFO FAR 2.101 citations, added FAR 4.1005-2 exception and schedule parentheticals, and removed meta-wording.
- COMPATIBLE_EXTENSION, 2026-10-31: Named the delivery schedule, destination, period or place of performance instead of 'those terms'.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the RFO FAR 2.101 definition of subline item.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the FAR 2.101 definition of subline item.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for contract line items, subline items and required data elements.
