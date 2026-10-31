+++
id = "cmp-cmmc-affirmation-and-flowdown"
title = "CMMC affirmation and subcontractor flowdown"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
questions = ["Q-004"]
links = ["cmp-safeguarding-covered-defense-information", "cmp-cmmc-program-scope-and-phases", "cmp-cmmc-level-1-self-assessment", "cmp-cmmc-level-2-assessment", "cmp-cmmc-level-3-assessment"]
+++

## Purpose

Shows who the Affirming Official is, when affirmation is due and the minimum CMMC status flowed down to subcontractors.

## The rule

| Field | Value |
|---|---|
| Instrument | Not established. |
| Codified citation | Not established. |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Not established. |
| Applies to | Affirmation: each OSA, whether prime or subcontractor [@CL-CMP-2094]. Flowdown: prime contractors and subcontractors at all tiers that will process, store, or transmit FCI or CUI [@CL-CMP-2095] |

The CMMC rule is a DoD rule that is not part of the FAR, DFARS or CAS, so the instrument rows read Not established. An Affirming Official from each OSA, whether prime or subcontractor, must affirm continuing compliance after every assessment, including POA&M closeout, and annually thereafter [@CL-CMP-2094]. The Affirming Official is the senior level representative within each OSA who is responsible for ensuring the OSA's compliance with the CMMC Program requirements and has the authority to affirm its continuing compliance with the specified security requirements for their respective organizations [@CL-CMP-2093; @CL-CMP-8128]. OSCs must annually affirm their Level 2 (C3PAO) status in addition to their Level 3 (DIBCAC) status to maintain eligibility for contracts requiring compliance with Level 3 [@CL-CMP-2134]. CMMC requirements apply to prime contractors and to subcontractors at all tiers of the supply chain that will process, store, or transmit any FCI or CUI on contractor information systems in performance of the DoD contract or subcontract [@CL-CMP-2095].

Prime contractors must comply with CMMC requirements and must require subcontractors to comply with and flow down those requirements, so that compliance is required at all tiers with the applicable level and assessment type for each subcontract [@CL-CMP-2135]. The minimum status a subcontractor needs depends on what it handles and on the prime contract. A subcontractor that will only process, store, or transmit FCI and not CUI in performance of the subcontract must have [Level 1 (Self)](cmp-cmmc-level-1-self-assessment.md) status [@CL-CMP-2096]. A subcontractor that will process, store, or transmit CUI in performance of the subcontract must have at least [Level 2 (Self)](cmp-cmmc-level-2-assessment.md) status [@CL-CMP-2097]. A subcontractor that will process, store, or transmit CUI, under a prime contract that requires Level 2 (C3PAO) status, must have at least Level 2 (C3PAO) status [@CL-CMP-2098]. A subcontractor that will process, store, or transmit CUI, under a prime contract that requires [Level 3 (DIBCAC)](cmp-cmmc-level-3-assessment.md) status, must have at least Level 2 (C3PAO) status [@CL-CMP-2099]. How DoD selects the status for the prime contract is on the [program scope and phases](cmp-cmmc-program-scope-and-phases.md) card.

## How it works

The Affirming Official signs for the organization, so each prime and each subcontractor needs its own Affirming Official [@CL-CMP-2094; @CL-CMP-2093]. Affirmation follows every assessment, including POA&M closeout, and recurs annually [@CL-CMP-2094]. The prime contractor must require its subcontractors to comply and flow the requirements down [@CL-CMP-2135], looking at what each subcontractor will handle [@CL-CMP-2096; @CL-CMP-2097]. Where the prime contract requires Level 2 (C3PAO) or Level 3 (DIBCAC) and the subcontractor handles CUI, the minimum subcontractor status is Level 2 (C3PAO) [@CL-CMP-2098; @CL-CMP-2099].

## Worked example

Synthetic example. The numbers are made up.

A prime holds a contract that requires Level 3 (DIBCAC). Subcontractor A will receive CUI, and subcontractor B will receive only FCI. The minimum for A is Level 2 (C3PAO) and the minimum for B is Level 1 (Self). The Affirming Official of each subcontractor affirms after each assessment and annually after that.

## Never confuse

The flowdown minimum under a Level 3 prime contract is Level 2 (C3PAO), not Level 3 (DIBCAC) [@CL-CMP-2099]. Affirmation is separate from the assessment: it follows every assessment and recurs annually [@CL-CMP-2094], and before award of a contract or subcontract with a Level 1 (Self) requirement, the OSA must both achieve that status and submit an affirmation of compliance into SPRS for all information systems within the CMMC Assessment Scope [@CL-CMP-2068].

## Company calibration

Which subcontracts pass FCI or CUI, who signs as Affirming Official for each entity, and where the flowdown clauses are kept. [@Q-004]

## Sources

## Change note

- COMPATIBLE_EXTENSION, 2026-10-31: Aligned the Level 1 pre-award sentence with its quote.
- COMPATIBLE_EXTENSION, 2026-10-31: Corrected the Affirming Official definition so the rule keeps the specified security requirements for the respective organizations.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for CMMC affirmation and subcontractor flowdown.
