+++
id = "cmp-cmmc-poam-and-scoring"
title = "CMMC POA&M conditions and scoring"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
questions = ["Q-014"]
links = ["cmp-safeguarding-covered-defense-information", "cmp-cmmc-level-1-self-assessment", "cmp-cmmc-level-2-assessment", "cmp-cmmc-level-3-assessment"]
+++

## Purpose

Shows the minimum score and point-value limits for a Conditional status, the requirements that may not be on a POA&M, and how points are subtracted when scoring.

## The rule

| Field | Value |
|---|---|
| Instrument | Not established. |
| Codified citation | Not established. |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Not established. |
| Applies to | OSAs seeking Conditional Level 2 or Conditional Level 3 status [@CL-CMP-2081; @CL-CMP-2085] |

The CMMC rule is a DoD rule that is not part of the FAR, DFARS or CAS, so the instrument rows read Not established. An OSA may achieve Conditional Level 2 (Self) or Conditional Level 2 (C3PAO) status only if, among the listed conditions, the assessment score divided by the total number of Level 2 security requirements is greater than or equal to 0.8 [@CL-CMP-2081]. Another listed condition for Conditional Level 2 status is that no requirement on the POA&M has a point value greater than 1, except that CUI encryption may be included if encryption is employed but is not FIPS-validated [@CL-CMP-2082]. Among the requirements that may not be on a Level 2 POA&M are external connections, control of public information and the system security plan [@CL-CMP-2083]. Under the same list, one group of requirements, escort visitors, physical access logs and manage physical access, is also barred from a Level 2 POA&M [@CL-CMP-2084]. The [Level 2 assessment](cmp-cmmc-level-2-assessment.md) card covers how a Conditional Level 2 status is reached and closed out [@CL-CMP-2070; @CL-CMP-2071].

An OSC may achieve Conditional Level 3 (DIBCAC) status only if, among the listed conditions, the assessment score divided by the total number of Level 3 security requirements is greater than or equal to 0.8 [@CL-CMP-2085]. Among the requirements that may not be on a Level 3 POA&M are the security operations center, the cyber incident response team, threat-informed risk assessment and supply chain risk response [@CL-CMP-2086]. The list continues with the supply chain risk plan, the security solution rationale and specialized asset security [@CL-CMP-2128]. The [Level 3 assessment](cmp-cmmc-level-3-assessment.md) card covers who performs the Level 3 certification assessment [@CL-CMP-2079]. No POA&Ms are permitted for Level 1 [@CL-CMP-2066], as the [Level 1 self-assessment](cmp-cmmc-level-1-self-assessment.md) card explains.

The closing of a POA&M must be confirmed by a closeout assessment within 180 days of the Conditional status date, and if it is not, the Conditional status for the information system will expire [@CL-CMP-2087]. Two derived Level 2 requirements can be partially effective, and the points deducted for them may be adjusted depending on how the requirement is implemented [@CL-CMP-2131]. This card does not state those adjustments.

## How it works

Under the Level 2 scoring method the maximum score equals the total number of Level 2 security requirements [@CL-CMP-2130]. An OSA with all Level 2 requirements MET is awarded the maximum score, and for each requirement NOT MET the associated value is subtracted from the maximum score, which may result in a negative score [@CL-CMP-2088]. Five points are subtracted for a basic or derived NIST SP 800-171 R2 requirement that, if not implemented, could lead to significant exploitation of the network or exfiltration of CUI [@CL-CMP-2089]. Three points are subtracted for a basic or derived requirement that, if not implemented, has a specific and confined effect on the security of the network and its data [@CL-CMP-2090]. One point is subtracted for each remaining derived requirement, other than the noted exceptions, that if not implemented has a limited or indirect effect on the security of the network and its data [@CL-CMP-2091]. The Level 3 certification assessment score equals the number of Level 3 security requirements that are assessed as MET [@CL-CMP-2092].

## Worked example

Synthetic example. The numbers are made up.

Suppose a Level 2 assessment has a maximum score of 110. Two requirements are NOT MET, one worth 1 point and one worth 5 points, so the score is 104 and the ratio is about 0.95. The ratio clears the 0.8 line, but the 5-point requirement cannot be on a POA&M because its value is above 1. The OSA cannot reach Conditional status until that requirement is MET.

## Never confuse

Clearing the minimum ratio is not enough on its own, because the point-value limit and the list of barred requirements also apply [@CL-CMP-2081; @CL-CMP-2082; @CL-CMP-2083; @CL-CMP-2084]. A Conditional status is not a Final status, because the closeout must be confirmed within 180 days [@CL-CMP-2087]. The Level 3 score counts requirements MET [@CL-CMP-2092], while the Level 2 score subtracts values from a maximum [@CL-CMP-2088].

## Company calibration

Which NOT MET requirements sit on a POA&M, who owns each fix and when each closeout is due. [@Q-014]

## Sources

## Change note

- COMPATIBLE_EXTENSION, 2026-10-31: Cited the rest of the barred requirements list.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for CMMC POA&M conditions and scoring.
