+++
id = "cmp-cmmc-level-2-assessment"
title = "CMMC Level 2 assessment, certification and POA&M closeout"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
questions = ["Q-014"]
links = ["cmp-safeguarding-covered-defense-information", "cmp-cmmc-poam-and-scoring", "cmp-cmmc-level-3-assessment", "cmp-cmmc-affirmation-and-flowdown"]
+++

## Purpose

Shows how a Level 2 status is earned by self-assessment or C3PAO certification, what Conditional and Final status mean, and the deadlines that follow [@CL-CMP-2069; @CL-CMP-2074].

## The rule

| Field | Value |
|---|---|
| Instrument | Not established. |
| Codified citation | Not established. |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Not established. |
| Applies to | OSAs with a contract or subcontract that requires Level 2 (Self) or Level 2 (C3PAO) status [@CL-CMP-2069; @CL-CMP-2074] |

The CMMC rule is a DoD rule that is not part of the FAR, DFARS or CAS, so the instrument rows read Not established. To maintain compliance with Level 2 (Self) status, the OSA must conduct a Level 2 self-assessment every three years and submit the results in SPRS within three years of the status date of the Conditional Level 2 (Self) status [@CL-CMP-2069]. The OSA achieves Conditional Level 2 (Self) status if its Level 2 self-assessment results in a POA&M that meets all the Level 2 POA&M requirements in the POA&M section of the part [@CL-CMP-2070], and Final Level 2 (Self) status if the self-assessment results in a passing score, whether upon initial self-assessment or as the result of a POA&M closeout self-assessment [@CL-CMP-2073]. To achieve Level 2 (C3PAO) status the OSC must achieve a MET result for all security requirements specified in the model section for Level 2 and must obtain a Level 2 certification assessment from an authorized or accredited C3PAO [@CL-CMP-2074]. To maintain Level 2 (C3PAO) status, the Level 2 certification assessment must be completed within three years of the status date of the Conditional Level 2 (C3PAO) status [@CL-CMP-2075].

Each Conditional status carries a 180-day deadline [@CL-CMP-2087]. For a Conditional Level 2 (Self) status the OSA must remediate any NOT MET requirements, perform a POA&M closeout self-assessment and post compliance results to SPRS within 180 days of the status date [@CL-CMP-2071]. For a Conditional Level 2 (C3PAO) status the OSC must remediate any NOT MET requirements and undergo a POA&M closeout certification assessment from a C3PAO, which must post compliance results into eMASS within 180 days of the status date [@CL-CMP-2076]. If the POA&M is not closed out in time, the Conditional status for the information system expires [@CL-CMP-2087]. If a Conditional Level 2 (Self) status expires within the period of performance of a contract, standard contractual remedies apply [@CL-CMP-2072]. The same sentence of the text also makes the OSA ineligible for additional awards requiring Level 2 (Self) or higher for the information system within the assessment scope until a new status is achieved [@CL-CMP-2077]. In a self-assessment the OSA itself performs the closeout, and in a certification the C3PAO performs the closeout assessment and posts the results [@CL-CMP-2071; @CL-CMP-2076]. The printed text does not state who applies the standard contractual remedies, and it does not give a numeric passing score.

## How it works

The OSA either assesses itself or, for a certification status, obtains an assessment from an authorized or accredited C3PAO [@CL-CMP-2074]. A Conditional status results when the assessment produces a POA&M that meets the POA&M requirements, and a Final status results from a passing score, either at the first assessment or after a closeout assessment [@CL-CMP-2070; @CL-CMP-2073]. The closing of the POA&M must be confirmed within 180 days, or the Conditional status expires [@CL-CMP-2087]. The status date of the Conditional status also anchors the three-year cycle for repeating the assessment [@CL-CMP-2069; @CL-CMP-2075]. The conditions a POA&M must meet, and how points are subtracted, are on the [POA&M conditions and scoring](cmp-cmmc-poam-and-scoring.md) card. The OSA affirms continuing compliance after every assessment, including POA&M closeout, as the [affirmation and flowdown](cmp-cmmc-affirmation-and-flowdown.md) card explains [@CL-CMP-2094].

## Worked example

Synthetic example. The numbers are made up.

A contractor completes a Level 2 self-assessment at month 0, and the result is a POA&M that meets every POA&M requirement, so it holds Conditional Level 2 (Self) status. It remediates the NOT MET items, performs the closeout self-assessment and posts the results to SPRS on day 150. That is inside the 180-day limit, so Final status can follow. Had it posted on day 200, the Conditional status would have expired.

## Never confuse

The 180-day closeout limit runs from the status date of the Conditional status [@CL-CMP-2071], and the three-year cycle also counts from the status date of the Conditional status [@CL-CMP-2069]. A self-assessment and a C3PAO certification are different routes: the OSA closes out its own POA&M in the first [@CL-CMP-2071], and a C3PAO performs the closeout in the second [@CL-CMP-2076]. A Conditional status is not a Final status, because the closeout must still be confirmed [@CL-CMP-2087]. A Level 3 certification assessment has a Final Level 2 (C3PAO) status as a prerequisite [@CL-CMP-2078], as the [Level 3 assessment](cmp-cmmc-level-3-assessment.md) card explains.

## Company calibration

Which POA&M items and closeout dates are open, and who owns each response. [@Q-014]

## Sources

## Change note

- COMPATIBLE_EXTENSION, 2026-10-31: Restored the maintain-compliance lead-in.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for CMMC Level 2 assessment and certification.
