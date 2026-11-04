+++
id = "cmp-cmmc-level-3-assessment"
title = "CMMC Level 3 DIBCAC assessment"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["defense_contract_management_agency"]
questions = ["Q-014"]
links = ["cmp-safeguarding-covered-defense-information", "cmp-cmmc-model-levels-and-scoping", "cmp-cmmc-level-2-assessment", "cmp-cmmc-poam-and-scoring", "cmp-cmmc-affirmation-and-flowdown"]
+++

## Purpose

Shows what must be in place before a Level 3 assessment, who performs it and how often it must be repeated [@CL-CMP-2078; @CL-CMP-2079; @CL-CMP-2080].

## The rule

| Field | Value |
|---|---|
| Instrument | Not established. |
| Codified citation | Not established. |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Not established. |
| Applies to | OSCs that undergo a Level 3 certification assessment [@CL-CMP-2078] |

A Final Level 2 (C3PAO) status for the information systems within the Level 3 assessment scope is a prerequisite to undergo a Level 3 certification assessment, and Level 3 recertification also has a prerequisite of a new Level 2 assessment [@CL-CMP-2078]. A Level 3 certification assessment is performed by DCMA DIBCAC on behalf of DoD, after the OSC has achieved Final Level 2 (C3PAO) status on the Level 3 assessment scope [@CL-CMP-2079]. To maintain Level 3 (DIBCAC) status, the Level 3 certification assessment must be performed every three years for all information systems within the Level 3 assessment scope [@CL-CMP-2080]. It must be completed within three years of the status date of the Final Level 3 (DIBCAC) status or, if there was a POA&M, within three years of the status date of the Conditional Level 3 (DIBCAC) status [@CL-CMP-2132]. A Level 2 (C3PAO) certification assessment must also be conducted every three years to maintain Level 3 (DIBCAC) status [@CL-CMP-2133].

The Level 3 assessment scope must be equal to or a subset of the Level 2 assessment scope, as the [model levels and scoping](cmp-cmmc-model-levels-and-scoping.md) card explains [@CL-CMP-2064]. Any Level 2 POA&M items must be closed before the Level 3 certification assessment is initiated [@CL-CMP-2129]. DoD Program Managers or requiring activities are responsible for selecting the CMMC Status that applies to a particular procurement or contract, based on the type of information, FCI or CUI, that will be processed on, stored on, or transmitted through a contractor information system [@CL-CMP-2054]. The Conditional Level 3 (DIBCAC) conditions and the Level 3 scoring are on the [POA&M conditions and scoring](cmp-cmmc-poam-and-scoring.md) card [@CL-CMP-2085; @CL-CMP-2092]. Affirmation for a Level 3 status is on the [affirmation and flowdown](cmp-cmmc-affirmation-and-flowdown.md) card [@CL-CMP-2134].

## How it works

The OSC first holds Final Level 2 (C3PAO) status on the Level 3 assessment scope [@CL-CMP-2078]. DCMA DIBCAC then performs the Level 3 certification assessment on behalf of DoD [@CL-CMP-2079]. The Level 3 certification assessment score equals the number of Level 3 security requirements that are assessed as MET [@CL-CMP-2092]. A Conditional Level 3 (DIBCAC) status needs, among its listed conditions, an assessment score divided by the total number of Level 3 security requirements that is greater than or equal to 0.8 [@CL-CMP-2085]. The Level 3 assessment must then be repeated every three years [@CL-CMP-2080].

## Worked example

Synthetic example. The numbers are made up.

A contractor holds Final Level 2 (C3PAO) status for a data enclave at month 0, and its last Level 2 POA&M item is closed by month 2. It asks DCMA DIBCAC for a Level 3 assessment at month 3, and the Level 3 status is dated month 6. The next Level 3 assessment is due within 36 months of that status date, and a Level 2 (C3PAO) assessment must also be repeated every three years.

## Never confuse

A Level 3 certification assessment is performed by DCMA DIBCAC on behalf of DoD [@CL-CMP-2079], and the OSC must obtain a Level 2 certification assessment from an authorized or accredited C3PAO [@CL-CMP-2074]. A Final Level 2 (C3PAO) status for the information systems within the Level 3 assessment scope is a prerequisite to undergo a Level 3 certification assessment, and Level 3 recertification also has a prerequisite of a new Level 2 assessment [@CL-CMP-2078], as the [Level 2 assessment](cmp-cmmc-level-2-assessment.md) card describes for that status.

## Company calibration

Whether any contract requires this status, and who owns the readiness work and each open finding. [@Q-014]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-10-31: Removed uncited inference and replaced the Level 2 contrast with the cited prerequisite.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the information-type basis to the status selection sentence.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for CMMC Level 3 assessment.
