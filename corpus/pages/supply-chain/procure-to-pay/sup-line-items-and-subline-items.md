+++
id = "sup-line-items-and-subline-items"
title = "Separately identified line items and subline items"
area = "sup"
subarea = "procure-to-pay"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["item_identity_layer"]
questions = ["Q-040"]
links = ["cmp-dfars-item-unique-identification", "ctf-dfars-line-item-and-subline-item-criteria", "est-market-research-and-alternative-line-items"]
+++

## Purpose

Shows how a procurement instrument identifies deliverables as line items and subline items, which characteristics call for a separate line item, and which exceptions apply for indefinite-delivery contracts, item identification and pricing.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified FAR [@CL-SUP-3313] |
| Codified citation | FAR 4.1001, 4.1003, 4.1005-2, 4.1007 and 4.1008 [@CL-SUP-3313; @CL-SUP-3315; @CL-SUP-3374; @CL-SUP-4115; @CL-SUP-4116] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-SUP-3313] |
| Applies to | Procurement instruments that identify the supplies or services to be acquired [@CL-SUP-3313] |

To improve the accuracy, traceability and usability of procurement data, procurement instruments must identify the supplies or services to be acquired as separately identified line items and, as needed, subline items [@CL-SUP-3313]. Line items are established to define deliverables or organize information about deliverables, and each line item describes characteristics for the item purchased, such as pricing, delivery and funding information [@CL-SUP-3368]. Each line item may be subdivided into separate unique subsets, called subline items, to ease administration [@CL-SUP-3369]. If a line item has deliverable subline items, the line item is informational [@CL-SUP-3314]. Subline items differentiate between or among certain characteristics of the line item, such as colors or sizes, dates of delivery, destinations or places of performance, and are established to define deliverables or organize information about deliverables [@CL-SUP-3370].

A procurement instrument must establish separate line items for deliverables that have the characteristics listed in the section, except as provided in the exceptions section [@CL-SUP-3315]. The five characteristics are:

- Separately identifiable: a supply is separately identifiable if it has its own identification, such as a national stock number, item description or manufacturer's part number, and services are separately identifiable if they have no more than one statement of work or performance work statement [@CL-SUP-3371]. If the instrument involves a first article, a separate line item is established for each item requiring a separate approval, and a single line item may be used where the first article is a lot of mixed items that will be approved as a single lot [@CL-SUP-3317].
- Single unit price or total price [@CL-SUP-3316].
- Single accounting classification citation [@CL-SUP-3316]. A single deliverable may be funded by multiple accounting classifications when the deliverable effort cannot be otherwise subdivided [@CL-SUP-3372].
- Separate delivery schedule, destination, period of performance or place of performance [@CL-SUP-3373].
- Single contract pricing type, such as fixed-price or cost-reimbursement [@CL-SUP-3373].

The exceptions are:

- Indefinite-delivery contracts: accounting classification, delivery date and destination, or period and place of performance are not known at the time of issuance of the contract but must be provided in each order at the time of issuance [@CL-SUP-3374].
- IDIQ and requirements contracts may omit the quantity at the line item level for the base award, provided that the total contract minimum and maximum, or the estimate, respectively, is stated [@CL-SUP-3375].
- Multiple-award IDIQ contracts awarded using the procedures at FAR 13.106-1(a)(2)(iv)(A) or 15.304(c)(1)(ii)(A) may omit price or cost at the line item or subline item level for the contract award, provided that the total contract minimum and maximum is stated [@CL-SUP-3376].
- Item description and PSC data elements are not required in the line item if there are associated deliverable subline items that include the actual detailed identification, and a general narrative description is then used for the line item [@CL-SUP-3377].
- The requirement for a single unit price or single total price at the line item level does not apply if any listed condition is present: there are associated deliverable subline items that are priced, or the line item or subline item is not separately priced [@CL-SUP-3378]; the supplies or services are being acquired on a cost-reimbursement, time-and-materials or labor-hour basis [@CL-SUP-3379]; or the instrument is for services, firm prices have been established for elements of the total price, and the number of the elements is not known until performance, for example a labor-hour contract for maintenance or repair [@CL-SUP-3380]. For instruments of that last kind, the contracting officer may structure them to reflect a firm or estimated total amount for each line item [@CL-SUP-3381].

FAR 2.101 defines a line item as the basic structural element in a procurement instrument that describes and organizes the required product or service for pricing, delivery, inspection, acceptance, invoicing, and payment [@CL-SUP-9551]. The first sentence of the RFO model text definition of a line item in FAR 2.101 reads the same as the codified first sentence [@CL-SUP-9597; @CL-SUP-9551], and the RFO text goes on to say that the use of the term "line item" includes "subline item," as applicable [@CL-SUP-9597; @CL-SUP-9641]. A line item number is either a numeric or alphanumeric format to identify a line item [@CL-SUP-9552].

## How it works

Each deliverable with the listed characteristics gets its own line item, and subline items are available where colors, sizes, delivery dates, destinations or places of performance differ within a line item [@CL-SUP-3315; @CL-SUP-3370]. A line item that has deliverable subline items is informational [@CL-SUP-3314], and where the subline items carry the actual detailed identification, or are priced, the identification and the price may sit one level down [@CL-SUP-3377; @CL-SUP-3378].

The exceptions keep the structure workable where data are not known at award: an indefinite-delivery contract leaves certain data to each order, quantity and price may be omitted at line item level in the stated cases, and the single price requirement drops away if any one of the four listed conditions is present [@CL-SUP-3374; @CL-SUP-3375; @CL-SUP-3376; @CL-SUP-3378; @CL-SUP-3379; @CL-SUP-3380].

Solicitations should be structured to allow offerors to propose alternative line items, and the text gives as an example that when soliciting certain items using units of measure such as kit, set, or lot, the offeror may not be able to group and deliver all items in a single shipment [@CL-SUP-4115]. The text also says to insert the provision at 52.204-22, Alternative Line Item Proposal, in all solicitations [@CL-SUP-4116]. How an alternative line item proposal is made and treated is on [Market research and alternative line item proposals](../../estimating/pricing/est-market-research-and-alternative-line-items.md).

The related DoD rules are on [Item unique identification](../../compliance/business-systems/cmp-dfars-item-unique-identification.md) and [DFARS line item and subline item criteria](../../contract-finance/billing-payment/ctf-dfars-line-item-and-subline-item-criteria.md).

## Worked example

Synthetic example. The numbers are made up.

A fixed-price order buys 4,000 parts that share one manufacturer's part number, come in two colors, and are delivered to two destinations. The parts are separately identifiable [@CL-SUP-3371], and each destination has its own delivery schedule and destination, so the order sets up one line item for each destination [@CL-SUP-3373]. Within each line item, subline items distinguish the two colors [@CL-SUP-3370]. Because each line item has deliverable subline items, each line item is informational [@CL-SUP-3314]. The subline items carry the detailed identification, so the line item uses a general narrative description [@CL-SUP-3377], and the subline items are priced, so no single unit price is needed at the line item level [@CL-SUP-3378].

## Never confuse

A subline item is not a second contract line: it is a subset of a line item used to ease administration, and the presence of deliverable subline items makes the parent line item informational [@CL-SUP-3369; @CL-SUP-3314]. Having subline items does not by itself mean nothing is priced: the single price requirement at line item level does not apply where associated deliverable subline items are priced, and the price sits on them [@CL-SUP-3378].

## Company calibration

Confirm how each instrument in force splits deliverables into line items and subline items, and which of them are informational only. [@Q-040]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-10-31: Stated the single-price conditions as any one of four.
- COMPATIBLE_EXTENSION, 2026-10-31: Reworded the informational line item contrast.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the qualifying sentence of the line item definition.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the RFO FAR 2.101 definition of line item.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the FAR 2.101 definitions of line item and line item number.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the rules on structuring solicitations for alternative line items and the provision to insert.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for separately identified line items and subline items.
