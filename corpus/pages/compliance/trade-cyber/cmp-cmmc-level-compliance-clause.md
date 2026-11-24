+++
id = "cmp-cmmc-level-compliance-clause"
title = "Contractor compliance with CMMC level requirements"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-27
handling = ["public"]
terms = ["defense_far_supplement"]
questions = ["Q-014"]
links = ["cmp-cmmc-affirmation-and-flowdown", "cmp-nist-sp-800-171-dod-assessment-requirements"]
+++

## Purpose

Shows what the contractor must have, maintain and report under the CMMC level clause, and what it must insert in subcontracts.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CMP-2834] |
| Codified citation | DFARS 252.204-7021(d)(1)(i) [@CL-CMP-2834] |
| RFO citation | Not established. |
| Edition date | NOV 2025 [@CL-CMP-10295] |
| Text status | Final [@CL-CMP-2834] |
| Applies to | Contracts with the clause, for all information systems used in performance that process, store, or transmit FCI or CUI [@CL-CMP-2835] |

The contractor must have and maintain for the duration of the contract a current CMMC status at the level the contracting officer inserts from the four listed levels, or higher [@CL-CMP-2834]. The required status applies to all information systems used in performance of the contract, task order, or delivery order that process, store, or transmit FCI or CUI [@CL-CMP-2835]. The contractor shall only process, store, or transmit FCI or CUI on contractor information systems that have a CMMC status at the required level, or higher [@CL-CMP-2836].

A contractor with a CMMC Status of Conditional must successfully close out a valid plan of action and milestones to achieve a CMMC Status of Final [@CL-CMP-2837]. The contractor must also submit to the contracting officer the CMMC UIDs issued by SPRS for contractor information systems that will process, store, or transmit FCI or CUI during performance of the contract [@CL-CMP-2838].

## How it works

The clause sets the contractor's own status duties and its subcontract duties. The word current has its own meaning in the clause. For a Conditional Level 2 (Self) or Conditional Level 2 (C3PAO) status, current means, among other listed conditions, not older than 180 days [@CL-CMP-2907]. For a Final Level 2 (Self) or Final Level 2 (C3PAO) status, current means, among other listed conditions, not older than 3 years with no changes in compliance with the requirements of the CMMC rule since the Final CMMC Status date [@CL-CMP-2908]. The contractor must insert the substance of the clause, including paragraph (f) and excluding paragraph (e)(1), in subcontracts and other contractual instruments [@CL-CMP-2839; @CL-CMP-2840]. That insertion reaches commercial product and commercial service subcontracts, excludes commercially available off-the-shelf items, and applies if the subcontract will contain a requirement to process, store, or transmit FCI or CUI [@CL-CMP-2840]. Prior to awarding a subcontract or other contractual instrument, the contractor must ensure that the subcontractor has a current CMMC certificate or current CMMC status at the level appropriate for the information being flowed down [@CL-CMP-2841]. The affirmation duties and the minimum subcontractor levels are on the [CMMC affirmation and subcontractor flowdown](cmp-cmmc-affirmation-and-flowdown.md) card and are not restated here.

## Worked example

Synthetic example. The numbers are made up.

A contract requires Level 2 (C3PAO). The contractor holds Final Level 2 (C3PAO) status for the one system that handles CUI, so CUI may stay on that system. A second system holds only Level 1 (Self) status, so no CUI may be processed on it. The contractor sends the UID for the first system to the contracting officer. Before it awards a subcontract that will carry FCI, it checks that the subcontractor holds a current status.

## Never confuse

The prime's own status duty and the flowdown duty differ: the contractor must have and maintain a current CMMC status at the level the contracting officer inserts, or higher [@CL-CMP-2834], for all information systems used in performance that process, store, or transmit FCI or CUI [@CL-CMP-2835], while prior to awarding a subcontract or other contractual instrument, the contractor must ensure that the subcontractor has a current CMMC certificate or current CMMC status at the level appropriate for the information being flowed down [@CL-CMP-2841]. For the DFARS assessment notice and clause for the NIST standard, see the [DoD assessment notice and requirements](cmp-nist-sp-800-171-dod-assessment-requirements.md) card.

## Company calibration

Which information systems process, store, or transmit FCI or CUI, the CMMC status and UID held for each, and who owns open findings. [@Q-014]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-11-24: Filled the Edition date row with cited clause edition dates.
- COMPATIBLE_EXTENSION, 2026-11-23: Never confuse states a contrast.
- COMPATIBLE_EXTENSION, 2026-10-31: Replaced the uncited contrast with the cited status duties.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for contractor compliance with the CMMC level requirements.
