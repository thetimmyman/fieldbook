+++
id = "cmp-cmmc-level-1-self-assessment"
title = "CMMC Level 1 self-assessment"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
questions = ["Q-003"]
links = ["cmp-safeguarding-covered-defense-information", "cmp-cmmc-program-scope-and-phases", "cmp-cmmc-model-levels-and-scoping", "cmp-cmmc-level-2-assessment", "cmp-cmmc-affirmation-and-flowdown"]
+++

## Purpose

Shows how often a Level 1 self-assessment is due, why no POA&M is allowed, where results go and what that means for contract eligibility [@CL-CMP-2065; @CL-CMP-2066; @CL-CMP-2068].

## The rule

| Field | Value |
|---|---|
| Instrument | Not established. |
| Codified citation | Not established. |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Not established. |
| Applies to | OSAs with a contract or subcontract that requires Level 1 (Self) status [@CL-CMP-2068] |

The CMMC rule is a DoD rule that is not part of the FAR, DFARS or CAS, so the instrument rows read Not established. It binds the OSA under a contract or subcontract that requires Level 1 (Self) status [@CL-CMP-2068]. To achieve Final Level 1 (Self) status the OSA must achieve a MET result for all security requirements specified in the model section for Level 1, and no POA&Ms are permitted for Level 1 [@CL-CMP-2066].

Three deadlines apply. Before award of any contract or subcontract that requires Level 1 (Self) status, the OSA must both achieve that status and have submitted an affirmation of compliance into SPRS for all information systems within the assessment scope [@CL-CMP-2068]. To maintain Final Level 1 (Self) status, the OSA must conduct a Level 1 self-assessment on an annual basis and submit the results in SPRS or its successor capability [@CL-CMP-2065]. The Affirming Official must affirm continuing compliance after every assessment and annually thereafter [@CL-CMP-2094], as the [affirmation and flowdown](cmp-cmmc-affirmation-and-flowdown.md) card explains. The printed text fixes no calendar date inside the year for either annual step.

## How it works

The OSA first specifies the assessment scope before the assessment [@CL-CMP-2060], and OSA information systems that process, store, or transmit FCI are in scope for Level 1, as the [model levels and scoping](cmp-cmmc-model-levels-and-scoping.md) card explains [@CL-CMP-2061]. It then conducts the self-assessment itself and enters the results in SPRS [@CL-CMP-2065]. Affirmation of the Level 1 (Self) status is required for all Level 1 self-assessments [@CL-CMP-2067]. Because no POA&M is allowed, a requirement that is not MET means Final Level 1 (Self) status is not achieved [@CL-CMP-2066].

## Worked example

Synthetic example. The numbers are made up.

A contractor with a Level 1 contract finishes its self-assessment at month 0 and finds that 14 of its 15 requirements are MET. A POA&M is not allowed, so it cannot achieve the status yet and is not eligible for award. It fixes the last requirement, repeats the self-assessment, enters the results in SPRS and submits the affirmation. The next annual self-assessment is due 12 months later.

## Never confuse

A Level 1 self-assessment allows no POA&M, so a missed requirement cannot be carried on a plan of action [@CL-CMP-2066]. A Level 2 self-assessment reaches Conditional Level 2 (Self) status if it results in a POA&M and the POA&M meets all the Level 2 POA&M requirements [@CL-CMP-2070], which is covered on the [Level 2 assessment](cmp-cmmc-level-2-assessment.md) card. Level 1 scope covers systems that handle FCI [@CL-CMP-2061], while Level 2 scope covers CUI Assets [@CL-CMP-2062].

## Company calibration

Who owns the annual self-assessment and affirmation, and which entities and contracts need one. [@Q-003]

## Sources

## Change note

- COMPATIBLE_EXTENSION, 2026-10-31: Aligned the Level 2 conditional status sentence with its quote.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for CMMC Level 1 self-assessment.
