+++
id = "mkt-unique-entity-identifier-contract-reporting"
title = "Unique entity identifier in contract reporting"
area = "mkt"
subarea = "feeds"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-27
handling = ["public"]
terms = ["federal_acquisition_regulation", "item_identity_layer"]
questions = ["Q-021"]
links = ["sup-cage-code-reporting"]
+++

## Purpose

Shows which identifier the contracting officer reports for the successful offeror, when a generic identifier may stand in for it, and where the registered name and address come from.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified FAR [@CL-MKT-0024]; RFO model text [@CL-MKT-9277] |
| Codified citation | FAR 4.601 [@CL-MKT-0026]; FAR 4.605 [@CL-MKT-0024; @CL-MKT-1888]; FAR 2.101 [@CL-MKT-9227] |
| RFO citation | RFO FAR 2.101 [@CL-MKT-9277] |
| Edition date | Not established. |
| Text status | Final [@CL-MKT-0024; @CL-MKT-9277] |
| Applies to | Contract actions with a successful offeror [@CL-MKT-0024] |

The contracting officer shall identify and report a unique entity identifier for the successful offeror on a contract action [@CL-MKT-0024]. The unique entity identifier shall correspond to the successful offeror's name and address as stated in the offer and resultant contract, and as registered in the System for Award Management in accordance with the provision at FAR 52.204-7, System for Award Management [@CL-MKT-0025]. Under FAR 2.101, unique entity identifier means a number or other identifier used to identify a specific commercial, nonprofit, or Government entity [@CL-MKT-9227]. Under RFO FAR 2.101, unique entity identifier means a number or other identifier used to identify a specific commercial, nonprofit, or Government entity [@CL-MKT-9277].

## How it works

Generic entity identifier means a number or other identifier assigned to a category of vendors and not specific to any individual or entity [@CL-MKT-0026]. Under FAR 4.605(c)(2), authorized generic entity identifiers, maintained by the Integrated Award Environment (IAE) program office (http://www.gsa.gov/portal/content/105036), may be used to report contracts in lieu of the contractor's actual unique entity identifier only for the listed cases [@CL-MKT-9394; @CL-MKT-0027]: (i) contract actions valued at or below $40,000 that are awarded to a contractor that is (A) a student [@CL-MKT-0027], (B) a dependent of either a veteran, foreign service officer, or military member assigned outside the United States and its outlying areas (as defined in FAR 2.101) [@CL-MKT-9395], or (C) located outside the United States and its outlying areas for work to be performed outside the United States and its outlying areas and the contractor does not otherwise have a unique entity identifier [@CL-MKT-9396], (ii) contracts valued above $40,000 awarded to individuals located outside the United States and its outlying areas for work to be performed outside the United States and its outlying areas [@CL-MKT-1888], or (iii) contracts when specific public identification of the contracted party could endanger the mission, contractor, or recipients of the acquired goods or services, and the contracting officer must include a written determination in the contract file of a decision applicable to authority under FAR 4.605(c)(2)(iii) [@CL-MKT-1888; @CL-MKT-0028]. Use of a generic entity identifier does not supersede the requirements of provisions 52.204-6, Unique Entity Identifier or 52.204-7, System for Award Management (if present in the solicitation) for the contractor to have a unique entity identifier assigned [@CL-MKT-0156].

## Worked example

Synthetic example. The numbers are made up.

A contracting officer awards a 25,000 contract to a student. A generic identifier may be used to report the award, because the value is at or below the limit for that case. That only changes what is reported: if the solicitation contains the identifier or registration provision, the student must still have a unique entity identifier assigned. A 25,000 contract to an ordinary domestic firm is different, because the identifier reported must be the firm's own, as registered.

## Never confuse

A generic entity identifier means a number or other identifier assigned to a category of vendors and not specific to any individual or entity [@CL-MKT-0026], and use of a generic entity identifier does not supersede the requirements of provisions 52.204-6, Unique Entity Identifier or 52.204-7, System for Award Management (if present in the solicitation) for the contractor to have a unique entity identifier assigned [@CL-MKT-0156], whereas the unique entity identifier reported under FAR 4.605(b) shall correspond to the successful offeror's name and address as stated in the offer and resultant contract, and as registered in the System for Award Management in accordance with the provision at FAR 52.204-7, System for Award Management [@CL-MKT-0025]. FAR 4.605(b) states that the Commercial and Government Entity (CAGE) Code is a different identifier and refers to FAR subpart 4.18 for a discussion of it [@CL-MKT-0101]. For CAGE codes, see the [Commercial and Government Entity Code Reporting provision](../../supply-chain/supplier-identity/sup-cage-code-reporting.md) card.

## Company calibration

Confirm how supplier and customer identifiers in reported contract data are reconciled to the company's own records. [@Q-021]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-10-31: Added second generic-identifier case in FAR 4.605(c)(2)(ii), separated RFO and codified citations, and removed meta-wording.
- COMPATIBLE_EXTENSION, 2026-10-31: Added who maintains generic identifiers and the other contractor kinds in the first listed case.
- COMPATIBLE_EXTENSION, 2026-10-31: Removed an exclusive "only" the cited text does not state.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the RFO FAR 2.101 definition of unique entity identifier.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the FAR 2.101 definition of unique entity identifier.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for the unique entity identifier in contract reporting.
