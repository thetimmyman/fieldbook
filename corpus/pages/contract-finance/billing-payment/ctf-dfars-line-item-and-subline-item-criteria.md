+++
id = "ctf-dfars-line-item-and-subline-item-criteria"
title = "DFARS criteria for contract line items and subline items"
area = "ctf"
subarea = "billing-payment"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-27
handling = ["public"]
terms = ["defense_far_supplement", "billing"]
questions = ["Q-009"]
links = ["mkt-contract-line-items-and-data-elements", "ctf-dod-payment-applicability-and-electronic-submission", "ctf-wide-area-workflow-payment-instructions", "ctf-dfars-line-item-numbering-and-exhibits", "ctf-dfars-modification-line-items-and-order-of-application", "ctf-acrn-and-payment-instructions"]
+++

## Purpose

Shows the tests a DoD contract uses to decide what is a separate line item and what is a subline item, and why that structure matters for billing and payment.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CTF-1575] |
| Codified citation | DFARS 204.7103 [@CL-CTF-1498]; DFARS 204.7103-1 [@CL-CTF-1575]; DFARS 204.7104-1 [@CL-CTF-1587] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CTF-1575] |
| Applies to | Contracts that identify the items or services to be acquired as separate contract line items [@CL-CTF-1575] |

The DFARS says to follow the procedures at PGI 204.7103 for establishing contract line items [@CL-CTF-1498]. Contracts shall identify the items or services to be acquired as separate contract line items unless it is not feasible to do so [@CL-CTF-1575]. Contract line items shall have all four listed characteristics, although exceptions within the characteristics may make a separate line item appropriate even though one characteristic appears to be missing [@CL-CTF-1576].

The first characteristic is a single unit price or a single total price, except in one of the listed cases [@CL-CTF-1577]. One exception is an item that is not separately priced but whose price is included in the unit price of another contract line item, where NSP is entered instead of the unit price [@CL-CTF-1577; @CL-CTF-1578]. The list of exceptions also includes the case of associated subline items established for other than informational reasons and priced under the subline item rules [@CL-CTF-1577; @CL-CTF-1625], items or services acquired on a cost-reimbursement contract [@CL-CTF-1577; @CL-CTF-1579], and a contract line item established to refer to an exhibit or an attachment, where, if management needs dictate that a unit price be entered, the price shall be set forth in the item description block and enclosed in parentheses [@CL-CTF-1577; @CL-CTF-1580]. It also includes a contract for maintenance and repair services where firm prices exist for elements of the total price but the actual number and quantity of the elements are not known until performance [@CL-CTF-1577; @CL-CTF-1626]. The last exception is when the contract is an indefinite delivery type contract and provides that the price of an item shall be determined at the time a delivery order is placed and the price is influenced by such factors as the quantity ordered, the destination, the FOB point, or the type of packaging required [@CL-CTF-1577; @CL-CTF-1843; @CL-CTF-1844].

Supplies are separately identifiable if they have no more than one national stock number, item description or manufacturer's part number, and services if they have no more than one scope of work or description of services [@CL-CTF-1581]. That requirement does not apply if there are associated subline items, established for other than informational reasons, and those subline items include the actual detailed identification in accordance with DFARS 204.7104 [@CL-CTF-1627]. Where that exception applies, a general narrative description is used instead of the contract item description [@CL-CTF-1628]. Each line item or service shall have its own delivery schedule, period of performance or completion date expressly stated, and the words as required count as an expressly stated delivery term [@CL-CTF-1582]. Each line item shall reference a single accounting classification citation, except as provided in the paragraph that authorizes multiple citations [@CL-CTF-1583]. One listed situation for multiple citations is a single, nonseverable deliverable paid for with research and development or other funds properly incrementally obligated over several fiscal years under DoD policy [@CL-CTF-1584]. When the use of multiple citations is authorized for a single contract line item, informational subline items shall be established for each accounting classification citation [@CL-CTF-1585].

There are only two kinds of subline items, informational ones and ones that consist of more than one item requiring separate identification [@CL-CTF-1587]. Subline items that identify information that relates directly to the contract line item and is an integral part of it (for example, parts of an assembly or parts of a kit) shall not be scheduled separately for delivery, identified separately for shipment or performance, or priced separately for payment [@CL-CTF-1588]. Separately identified subline items will be used instead of contract line items to facilitate payment, delivery tracking, contract funds accounting, or other management purposes, and shall be used when items bought under one contract line item number meet one of the listed situations [@CL-CTF-1589]. One listed situation is items that are to be paid for from more than one accounting classification [@CL-CTF-1589; @CL-CTF-1590]. A line item rather than a subline item is established if it is likely that a subline item may be assigned additional accounting classification citations at a later date [@CL-CTF-1630]. Each separately identified subline item shall have its own delivery schedule, period of performance or completion date, as the first of its listed required elements [@CL-CTF-1649].

## How it works

Under the listed characteristics, a line item carries a single price, is separately identifiable, has its own delivery or performance terms and references a single accounting classification citation, subject to the listed exceptions [@CL-CTF-1577; @CL-CTF-1581; @CL-CTF-1582; @CL-CTF-1583]. Separately identified subline items will be used instead of contract line items to facilitate payment, delivery tracking, contract funds accounting, or other management purposes [@CL-CTF-1589], and when items bought under one contract line item number are to be paid for from more than one accounting classification, a subline item shall be established for the quantity associated with the single accounting classification citation [@CL-CTF-1590]. Where the price of items not separately priced is included in the price of another contract line or subline item, it may be necessary to withhold payment on the priced line until the included items are delivered [@CL-CTF-1591]. In a contract that contains a combination of fixed-price, time-and-materials/labor-hour and/or cost-reimbursement line items, the contract type of each line is shown in Section B to facilitate appropriate payment [@CL-CTF-1586].

## Worked example

Synthetic example. The numbers are made up.

A contract buys 100 pumps at a single unit price of 500, all due on one date. The pumps are funded from one accounting classification, so one line item fits.

Now suppose 60 pumps are paid from one accounting classification and 40 from another. Items under one line item paid from more than one classification need a subline item for each quantity, so the contract carries a subline item for 60 and another for 40, each tied to its single citation. Assume neither quantity is likely to receive additional accounting classification citations later.

## Never confuse

Informational subline items identify information that relates directly to the contract line item and is an integral part of it, and shall not be scheduled separately for delivery, identified separately for shipment or performance, or priced separately for payment purposes [@CL-CTF-1588]. Separately identified subline items will be used instead of contract line items to facilitate payment, delivery tracking, contract funds accounting, or other management purposes [@CL-CTF-1589]. Which data elements each line must show is on [Contract line items and data elements](../../market/feeds/mkt-contract-line-items-and-data-elements.md), and how a line is invoiced is on [Wide Area WorkFlow payment instructions](ctf-wide-area-workflow-payment-instructions.md). Numbering is on [DFARS line item and subline item numbering, exhibits and attachments](ctf-dfars-line-item-numbering-and-exhibits.md), changes by modification are on [Line items in DFARS contract modifications and the order modifications apply](ctf-dfars-modification-line-items-and-order-of-application.md), and the tracing of funds is on [Accounting classification reference numbers and payment instructions](ctf-acrn-and-payment-instructions.md).

## Company calibration

Which line item and subline item structure the company's contracts use, and how each line maps to invoices and receiving reports. [@Q-009]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-10-31: Added the last price exception; restored exhibit and integral-part qualifiers; cut meta and uncited contrasts.
- COMPATIBLE_EXTENSION, 2026-10-31: NSP exception condition; and/or combination of line types.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the DFARS pointer to PGI for establishing contract line items.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for DFARS line item and subline item criteria.
