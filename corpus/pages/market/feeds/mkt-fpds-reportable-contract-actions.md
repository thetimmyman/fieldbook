+++
id = "mkt-fpds-reportable-contract-actions"
title = "Federal Procurement Data System: what is reported"
area = "mkt"
subarea = "feeds"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["federal_acquisition_regulation", "defense_far_supplement"]
questions = ["Q-020"]
links = ["cmp-acquisition-thresholds"]
+++

## Purpose

Lists the contract actions, indefinite delivery vehicles and orders that agencies must enter in the Federal Procurement Data System, how purchase card data and consolidated reports are handled, and one of the kinds of transactions that are not reported.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified FAR and codified DFARS [@CL-MKT-0016; @CL-MKT-0023] |
| Codified citation | FAR 4.606; DFARS 204.606 [@CL-MKT-0016; @CL-MKT-0023] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-MKT-0016; @CL-MKT-0023] |
| Applies to | Agencies reporting contract actions over the micro-purchase threshold [@CL-MKT-0016] |

As a minimum, agencies must report the following contract actions over the micro-purchase threshold, regardless of solicitation process used, and agencies must report any modification to these contract actions that change previously reported contract action data, regardless of dollar value [@CL-MKT-0016]: (i) definitive contracts, including purchase orders and imprest fund buys over the micro-purchase threshold awarded by a contracting officer [@CL-MKT-0017], (ii) indefinite delivery vehicle (identified as an "IDV" in FPDS) [@CL-MKT-0100], and (iii) all calls and orders awarded under the indefinite delivery vehicles identified in FAR 4.606(a)(1)(ii) [@CL-MKT-0019].

## How it works

Examples of IDVs include task and delivery order contracts (see FAR subpart 16.5), including government-wide acquisition contracts and multi-agency contracts [@CL-MKT-0100], GSA Federal supply schedules, Blanket Purchase Agreements (see FAR 13.303), Basic Ordering Agreements (see FAR 16.703), and any other agreement or contract against which individual orders or purchases may be placed [@CL-MKT-0018]. The GSA Office of Charge Card Management will provide the Government purchase card data, at a minimum annually, and GSA will incorporate that data into FPDS for reports [@CL-MKT-0020]. Agencies may use the FPDS Express Reporting capability for consolidated multiple action reports for a vendor when it would be overly burdensome to report each action individually, and when used, Express Reporting should be done at least monthly [@CL-MKT-0021]. Under the actions not reported paragraph, imprest fund transactions below the micro-purchase threshold, including those made via the Government purchase card, are not to be reported to FPDS unless specific agency procedures prescribe reporting these actions [@CL-MKT-0022]. In addition to FAR 4.606, DFARS 204.606 directs following the procedures at PGI 204.606 for reporting data to FPDS [@CL-MKT-0023].

## Worked example

Synthetic example. The numbers are made up.

An agency makes a purchase card order of 2,000 for office supplies. Separately it awards a 600,000 blanket purchase agreement with a first order of 40,000. The card order is below the micro-purchase threshold, so under the exclusion for imprest fund transactions made via the purchase card it is not reported unless agency procedures say so. The blanket purchase agreement is an indefinite delivery vehicle and is reported, and so is the first order placed under it.

## Never confuse

The GSA Office of Charge Card Management will provide the Government purchase card data, at a minimum annually, and GSA will incorporate that data into FPDS for reports [@CL-MKT-0020], whereas imprest fund transactions below the micro-purchase threshold, including those made via the Government purchase card, are not to be reported to FPDS unless specific agency procedures prescribe reporting these actions [@CL-MKT-0022]. Agencies may use the FPDS Express Reporting capability for consolidated multiple action reports for a vendor when it would be overly burdensome to report each action individually, and when used, Express Reporting should be done at least monthly [@CL-MKT-0021]. For the micro-purchase threshold amount, see [Simplified acquisition and micro-purchase thresholds](../../compliance/regulatory-framework/cmp-acquisition-thresholds.md).

## Company calibration

Confirm which reported contract actions and orders map to the company's own contracts before they are used as market observations. [@Q-020]

## Sources

## Change note

- COMPATIBLE_EXTENSION, 2026-10-31: Added exact FAR cross-references for IDV examples and removed uncited contrast framing.
- COMPATIBLE_EXTENSION, 2026-10-31: Said the card covers one of several kinds of actions not reported.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for what is reported to the Federal Procurement Data System.
