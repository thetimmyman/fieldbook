+++
id = "cmp-cmmc-model-levels-and-scoping"
title = "CMMC model levels and assessment scoping"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
questions = ["Q-016"]
links = ["cmp-safeguarding-covered-defense-information", "cmp-cmmc-program-scope-and-phases", "cmp-cmmc-level-1-self-assessment", "cmp-cmmc-level-2-assessment", "cmp-cmmc-level-3-assessment"]
+++

## Purpose

Shows the three CMMC levels, the security requirements behind each, and which assets fall inside an assessment.

## The rule

| Field | Value |
|---|---|
| Instrument | Not established. |
| Codified citation | Not established. |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Not established. |
| Applies to | Organizations seeking assessment, whose assessment scope is the set of assets assessed against CMMC security requirements [@CL-CMP-2060] |

The CMMC rule is a DoD rule that is not part of the FAR, DFARS or CAS, so the instrument rows read Not established. The safeguarding and security requirements specified for Level 1 are those in 48 CFR 52.204-21, for Level 2 are those in NIST SP 800-171 R2, and for Level 3 are selected requirements from NIST SP 800-172 Feb2021 [@CL-CMP-2059]. The CMMC Assessment Scope is the set of all assets in the OSA's environment that will be assessed against CMMC security requirements, and it must be specified before the assessment [@CL-CMP-2060].

The rule binds the organizations seeking assessment that handle FCI or CUI, as the [program scope and phases](cmp-cmmc-program-scope-and-phases.md) card explains. This card does not restate the DFARS safeguarding clause, which is covered in [Safeguarding covered defense information and cyber incident reporting](cmp-safeguarding-covered-defense-information.md). The source text also covers other asset categories and external service providers, and this card does not state them.

## How it works

For Level 1, OSA information systems that process, store, or transmit FCI are in scope and must be self-assessed against the applicable CMMC security requirements, as the [Level 1 card](cmp-cmmc-level-1-self-assessment.md) describes [@CL-CMP-2061]. For Level 2, assets that process, store, or transmit CUI are in scope as CUI Assets, which the OSA must document in the asset inventory and whose treatment it must document in the SSP, as the [Level 2 card](cmp-cmmc-level-2-assessment.md) describes [@CL-CMP-2062]. Out-of-Scope Assets for Level 2 are assets that cannot process, store, or transmit CUI and do not provide security protections for CUI Assets [@CL-CMP-2063]. The Level 3 assessment scope must be equal to or a subset of the Level 2 assessment scope [@CL-CMP-2064], and the [Level 3 card](cmp-cmmc-level-3-assessment.md) covers that assessment. For Level 2, an asset that can, but is not intended to, process, store, or transmit CUI because of security policy, procedures and practices is a Contractor Risk Managed Asset, not an out-of-scope one [@CL-CMP-2139].

## Worked example

Synthetic example. The numbers are made up.

A contractor keeps CUI on a file server. A separate lab network is physically and logically separated so that it cannot process, store or transmit CUI, and it gives no security protection to the file server. The file server is a CUI Asset in the Level 2 scope, and the lab network is an Out-of-Scope Asset. Had the lab network been able to handle CUI but not intended to, holding none would not have put it out of scope: it would have been a Contractor Risk Managed Asset. If the contractor later seeks Level 3, it picks an enclave inside the Level 2 scope, so the Level 3 scope stays a subset.

## Never confuse

A level is a set of security requirements, and the scope is the set of assets they are assessed against [@CL-CMP-2059; @CL-CMP-2060]. The status a procurement asks for is selected by DoD Program Managers or requiring activities, not by the model [@CL-CMP-2054]. An OSA information system that processes, stores, or transmits FCI is in the Level 1 scope [@CL-CMP-2061], while CUI Assets are one Level 2 in-scope category [@CL-CMP-2062].

## Company calibration

Which systems, storage tools, transfer tools and assistant tools process, store or transmit FCI or CUI, and which of them IT approves. [@Q-016]

## Sources

## Change note

- COMPATIBLE_EXTENSION, 2026-10-31: Removed an added only from the Level 1 scope sentence.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for CMMC model levels and scoping.
