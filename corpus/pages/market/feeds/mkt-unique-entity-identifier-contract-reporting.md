+++
id = "mkt-unique-entity-identifier-contract-reporting"
title = "Unique entity identifier in contract reporting"
area = "mkt"
subarea = "feeds"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
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
| Instrument | Codified FAR [@CL-MKT-0024] |
| Codified citation | FAR 4.605 [@CL-MKT-0024]; FAR 2.101 [@CL-MKT-9227] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-MKT-0024] |
| Applies to | Contract actions with a successful offeror [@CL-MKT-0024] |

The contracting officer shall identify and report a unique entity identifier for the successful offeror on a contract action [@CL-MKT-0024]. The identifier shall correspond to the successful offeror's name and address as stated in the offer and resultant contract, and as registered in the System for Award Management in accordance with the provision on that system [@CL-MKT-0025]. The FAR definitions section describes a unique entity identifier as a number or other identifier used to identify a specific commercial, nonprofit, or Government entity [@CL-MKT-9227].

## How it works

A generic entity identifier is a number or other identifier assigned to a category of vendors and not specific to any individual or entity [@CL-MKT-0026]. Under the paragraph that authorizes generic entity identifiers, an identifier may be used to report contracts in lieu of the contractor's actual unique entity identifier only for listed cases [@CL-MKT-0027]. One listed case is a contract action valued at or below $40,000 that is awarded to a contractor that is a student [@CL-MKT-0027]. Another is a case in which specific public identification of the contracted party could endanger the mission, contractor, or recipients of the acquired goods or services, and the contracting officer must include a written determination in the contract file [@CL-MKT-0028]. Use of a generic identifier does not supersede the requirements of provisions 52.204-6 or 52.204-7, where present in the solicitation, for the contractor to have its own unique entity identifier assigned [@CL-MKT-0156].

## Worked example

Synthetic example. The numbers are made up.

A contracting officer awards a 25,000 contract to a student. A generic identifier may be used to report the award, because the value is at or below the limit for that case. That only changes what is reported: if the solicitation contains the identifier or registration provision, the student must still have a unique entity identifier assigned. A 25,000 contract to an ordinary domestic firm is different, because the identifier reported must be the firm's own, as registered.

## Never confuse

A generic entity identifier is assigned to a category of vendors and is not specific to any individual or entity [@CL-MKT-0026] and changes only what is reported, not whether the contractor needs its own identifier [@CL-MKT-0156], while the unique entity identifier is the one that corresponds to the successful offeror's name and address as registered [@CL-MKT-0025]. The CAGE code is a different identifier again [@CL-MKT-0101]. For CAGE codes, see the [Commercial and Government Entity Code Reporting provision](../../supply-chain/supplier-identity/sup-cage-code-reporting.md) card.

## Company calibration

Confirm how supplier and customer identifiers in reported contract data are reconciled to the company's own records. [@Q-021]

## Sources

## Change note

- COMPATIBLE_EXTENSION, 2026-10-31: Added the FAR 2.101 definition of unique entity identifier.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for the unique entity identifier in contract reporting.
