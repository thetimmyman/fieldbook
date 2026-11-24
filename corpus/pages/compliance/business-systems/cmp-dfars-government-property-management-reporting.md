+++
id = "cmp-dfars-government-property-management-reporting"
title = "Management and reporting of Government property clause (DFARS)"
area = "cmp"
subarea = "business-systems"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-27
handling = ["public"]
terms = ["defense_far_supplement"]
questions = ["Q-034"]
links = ["cls-government-property-clause", "cls-property-management-system", "cmp-dfars-item-unique-identification", "cmp-contractor-business-systems"]
+++

## Purpose

Shows what the contractor must report about Government property, how fast, and which data elements its property records must be able to record.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CMP-3867] |
| Codified citation | DFARS 252.245-7005 [@CL-CMP-3867] |
| RFO citation | Not established. |
| Edition date | Jan 2024 [@CL-CMP-10055] |
| Text status | Final [@CL-CMP-3867] |
| Applies to | Contractors holding Government property under contracts with the clause [@CL-CMP-3867] |

The Contractor shall use the Government Furnished Property (GFP) module of the Procurement Integrated Enterprise Environment to report receipt of GFP and the transfer of GFP to another DoD contract [@CL-CMP-3864]. The module is also used to report the shipment of GFP to the Government or to a contractor [@CL-CMP-3896], to report when serially-managed items of GFP are incorporated into a higher-level component, assembly, or end item, and to report the loss of Government property in accordance with paragraph (f)(1)(vii) of the FAR 52.245-1 clause of the contract [@CL-CMP-3897]. It is further used to complete the plant clearance inventory schedule in accordance with paragraph (j)(2) of the FAR 52.245-1 clause of the contract, unless disposition instructions are otherwise included in the contract [@CL-CMP-3898], and to submit a request to buy back or to convert to GFP items of Contractor-acquired property [@CL-CMP-3899]. In complying with paragraphs (b)(1)(i) through (iv) of the clause, the Contractor shall report the updated status of the property to the GFP module within 7 business days of the date the change in status occurs, unless otherwise specified in the contract [@CL-CMP-3865]. When Government property is in the possession of subcontractors, the Contractor shall ensure that reporting is accomplished using the data elements required in paragraph (c) of the clause [@CL-CMP-3866].

To facilitate reporting of Government property to the GFP module, the Contractor's property records, in addition to the requirements of paragraph (f)(1)(iii) of the FAR 52.245-1 clause of the contract, shall enable recording of the listed data elements [@CL-CMP-3867]. Two listed elements are the national stock number (NSN), using either the combination of the manufacturer's CAGE code and part number, or model number, if an NSN is not available, and the CAGE code on the accountable Government contract [@CL-CMP-3868]. Four others are the received or sent (shipped) date, the accountable Government contract number, the serial number (for serially-managed items that do not have a unique item identifier (UII)), and the supply condition code (only required for reporting of reparable items) [@CL-CMP-3869].

Under paragraph (d)(1), the Contractor shall assign the UII and mark the reparable items identified as serially managed in the GFP attachment to the contract with an item unique identification (IUID) data matrix, when the technical drawing for the item is accessible to the Contractor and includes IUID data matrix location and marking method [@CL-CMP-3900]. The clause for item unique identification generally is on [Item unique identification and valuation clause](cmp-dfars-item-unique-identification.md).

## How it works

The Contractor uses the GFP module of the Procurement Integrated Enterprise Environment to report Government property, and the Contractor's property records enable recording of the data elements that facilitate reporting to the GFP module [@CL-CMP-3864; @CL-CMP-3867]. In complying with paragraphs (b)(1)(i) through (iv) of the clause, the Contractor reports the updated status of the property to the GFP module within 7 business days of the date the change in status occurs, unless otherwise specified in the contract [@CL-CMP-3865]. When Government property is in the possession of subcontractors, the Contractor ensures that reporting is accomplished using the data elements required in paragraph (c) of the clause [@CL-CMP-3866]. For property management system administration, see [Contractor Property Management System Administration clause](../../close-revrec/fixed-assets/cls-property-management-system.md). The definition of contractor business systems lists the property management system, if the contract includes the clause at DFARS 252.245-7003, Contractor Property Management System Administration [@CL-CMP-1202]. See [Contractor business systems](cmp-contractor-business-systems.md).

## Worked example

Synthetic example. The numbers are made up.

A contractor receives a reparable item as Government-furnished property on a Monday. The change in status must reach the Government Furnished Property module within 7 business days unless the contract says otherwise. The contractor's record for the item must be able to hold a national stock number, the accountable contract number, the received date, the serial number if the item has no UII, and the supply condition code.

## Never confuse

The DFARS data elements come in addition to, and differ from, the FAR clause requirements: to facilitate reporting of Government property to the GFP module, the Contractor's property records, in addition to the requirements of paragraph (f)(1)(iii) of the FAR 52.245-1 clause of the contract, shall enable recording of the required data elements [@CL-CMP-3867]. See [Government property clause](../../close-revrec/fixed-assets/cls-government-property-clause.md).

## Company calibration

Whether the company's property management system is currently approved and whether its property records can hold every listed data element. [@Q-034]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-11-24: Filled the Edition date row with cited clause edition dates.
- COMPATIBLE_EXTENSION, 2026-11-23: Never confuse states a contrast.
- COMPATIBLE_EXTENSION, 2026-10-31: Aligned the GFP reporting items, 7-business-day scope, and data-element wording with the cited quotes and removed meta-wording and unquoted contrast phrasing.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for DFARS management and reporting of Government property.
