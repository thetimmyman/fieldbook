+++
id = "cmp-dfars-item-unique-identification"
title = "Item unique identification and valuation clause (DFARS)"
area = "cmp"
subarea = "business-systems"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-27
handling = ["public"]
terms = ["defense_far_supplement"]
questions = ["Q-003"]
links = ["cmp-dfars-government-property-management-reporting", "cmp-dfars-safety-notification-and-warranty-tracking"]
+++

## Purpose

Shows which delivered items need a unique item identifier, how it is marked, and how the item's value is reported at delivery.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CMP-3870] |
| Codified citation | DFARS 252.211-7003 [@CL-CMP-3870] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CMP-3870] |
| Applies to | Items the clause lists, delivered under contracts with the clause [@CL-CMP-3870] |

One listed category requiring a unique item identifier is delivered items for which the Government's unit acquisition cost is $5,000 or more, except for the line items the contract lists as exceptions [@CL-CMP-3870]. A second listed category is items for which the Government's unit acquisition cost is less than $5,000 that are identified in the Schedule or in the clause table [@CL-CMP-3890]. A third is subassemblies, components, and parts embedded within delivered items, items with warranty requirements, and DoD serially managed reparables and nonreparables, as specified in an attachment [@CL-CMP-3891]. A fourth is any item of special tooling or special test equipment, as defined in FAR 2.101, that has been designated for preservation and storage for a Major Defense Acquisition Program as specified in an attachment [@CL-CMP-3892]. A fifth is any item not included in the other listed categories for which the contractor creates and marks a unique item identifier for traceability [@CL-CMP-3893]. The identifier's component data elements shall be marked on an item using two dimensional data matrix symbology that complies with ISO/IEC International Standard 16022 and the ECC200 data matrix specification [@CL-CMP-3871].

For each item that requires item unique identification under paragraph (c)(1)(i), (ii), or (iv) of the clause, or when item unique identification is provided under paragraph (c)(1)(v), and in addition to the information provided as part of the Material Inspection and Receiving Report, the Contractor shall report at the time of delivery, as part of that report, the listed information [@CL-CMP-3872; @CL-CMP-3873]. One listed item is the Government's unit acquisition cost [@CL-CMP-3874].

## How it works

The unique item identifier component data elements are marked on an item, and the Contractor reports at the time of delivery, as part of the Material Inspection and Receiving Report, the listed information, including the Government's unit acquisition cost [@CL-CMP-3871; @CL-CMP-3873; @CL-CMP-3874]. For fixed-price type line, subline, or exhibit line items, the Government's unit acquisition cost is the unit price identified in the contract at the time of delivery [@CL-CMP-3875]. For cost-type or undefinitized line, subline, or exhibit line items, it is the Contractor's estimated fully burdened unit cost to the Government at the time of delivery [@CL-CMP-3894]. For items produced under a time-and-materials contract, it is the Contractor's estimated fully burdened unit cost to the Government at the time of delivery [@CL-CMP-3895]. If the Contractor acquires by subcontract any items for which item unique identification is required in accordance with paragraph (c)(1) of the clause, the Contractor shall include the clause, including paragraph (g), in the applicable subcontracts, including subcontracts for commercial products or commercial services [@CL-CMP-3876; @CL-CMP-0178].

## Worked example

Synthetic example. The numbers are made up.

A fixed-price line item has a unit price of 8,000 at delivery. That is above the 5,000 level and the line is not listed as an exception, so the unit must carry a unique item identifier in a data matrix mark. The delivery report shows 8,000 as the Government's unit acquisition cost.

## Never confuse

The unique item identifier component data elements shall be marked on an item using two dimensional data matrix symbology that complies with ISO/IEC International Standard 16022 and the ECC200 data matrix specification, and the Contractor shall report the required information at the time of delivery as part of the Material Inspection and Receiving Report [@CL-CMP-3871; @CL-CMP-3873]. The unit cost basis differs by contract type: the Government's unit acquisition cost is the unit price identified in the contract at the time of delivery for fixed-price type line, subline, or exhibit line items, and the Contractor's estimated fully burdened unit cost to the Government at the time of delivery for cost-type or undefinitized line, subline, or exhibit line items and for items produced under a time-and-materials contract [@CL-CMP-3875; @CL-CMP-3894; @CL-CMP-3895]. Under DFARS 252.245-7005(d)(1), the Contractor assigns the UII and marks the reparable items identified as serially managed in the GFP attachment to the contract with an item unique identification data matrix when the technical drawing for the item is accessible to the Contractor and includes IUID data matrix location and marking method [@CL-CMP-3900], covered on [Management and reporting of Government property clause](cmp-dfars-government-property-management-reporting.md). Under DFARS 252.246-7006(b)(2)(i), the Contractor provides, no later than when the warranted items are presented for receipt and/or acceptance, the unique item identifier for each warranted item required by the attachment entitled Warranty Tracking Information [@CL-CMP-3883], as described on [Notification of potential safety issues and warranty tracking of serialized items](cmp-dfars-safety-notification-and-warranty-tracking.md).

## Company calibration

Which contracts carry this clause and which delivered line items fall in a listed category. [@Q-003]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-11-23: Never confuse states a contrast.
- COMPATIBLE_EXTENSION, 2026-10-31: Completed the subcontract flowdown clause with commercial products and commercial services and matched the warranty tracking timing and attachment title.
- COMPATIBLE_EXTENSION, 2026-10-31: Delivery report scope names the paragraphs; removed unsupported only.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for DFARS item unique identification and valuation.
