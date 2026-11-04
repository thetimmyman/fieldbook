+++
id = "cmp-safeguarding-covered-defense-information"
title = "Safeguarding covered defense information and cyber incident reporting"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["defense_far_supplement", "commercial_product"]
questions = ["Q-014"]
links = ["cmp-acquisition-regulations", "cmp-oversight-roles"]
+++

## Purpose

Shows what the clause requires for adequate security on covered contractor information systems, how a cyber incident is reported and its evidence preserved, and what flows down to subcontractors.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CMP-1001] |
| Codified citation | DFARS 252.204-7012 [@CL-CMP-1001] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CMP-1001] |
| Applies to | The clause as prescribed in DFARS 204.7304(c) [@CL-CMP-1001] |

The clause is prescribed in DFARS 204.7304(c) [@CL-CMP-1001]. The contractor shall provide adequate security on all covered contractor information systems [@CL-CMP-1003]. Adequate security means protective measures that are commensurate with the consequences and probability of loss, misuse, or unauthorized access to, or modification of information [@CL-CMP-1420]. A covered contractor information system is an unclassified information system that is owned, or operated by or for, a contractor and that processes, stores, or transmits covered defense information [@CL-CMP-1421]. Where such a system is not part of an information technology service or system operated on behalf of the Government [@CL-CMP-1422], and except as provided in paragraph (b)(2)(ii) of the clause, it is subject to the security requirements in NIST Special Publication 800-171 [@CL-CMP-1423], as in effect at the time the solicitation is issued or as authorized by the Contracting Officer [@CL-CMP-1424].

Rapidly report means within 72 hours of discovery of any cyber incident [@CL-CMP-1002]. The review and reporting duties apply when the contractor discovers a cyber incident that affects a covered contractor information system or the covered defense information residing therein [@CL-CMP-1425], or that affects the contractor's ability to perform contract requirements designated as operationally critical support and identified in the contract [@CL-CMP-1426]. The contractor must conduct a review for evidence of compromise of covered defense information, including, but not limited to, identifying compromised computers, servers, specific data, and user accounts [@CL-CMP-1427], and rapidly report cyber incidents to DoD [@CL-CMP-1428]. When a contractor discovers that a cyber incident has occurred, it must preserve and protect images of all known affected information systems identified in paragraph (c)(1)(i) of the clause [@CL-CMP-1430], and it must preserve and protect the images and all relevant monitoring or packet capture data for at least 90 days from the submission of the cyber incident report, to allow DoD to request the media or decline interest [@CL-CMP-1004]. To report cyber incidents, the contractor or subcontractor must have or acquire a DoD-approved medium assurance certificate [@CL-CMP-1005].

## How it works

When the contractor or a subcontractor discovers and isolates malicious software in connection with a reported cyber incident, it submits the software to the DoD Cyber Crime Center in accordance with instructions from that center or the Contracting Officer [@CL-CMP-1429]. Upon request by DoD, the contractor must give DoD access to additional information or equipment that is necessary to conduct a forensic analysis [@CL-CMP-1431]. The contractor must include the clause, including its subcontracts paragraph, in subcontracts or similar contractual instruments for operationally critical support, or for which subcontract performance will involve covered defense information [@CL-CMP-1433], and the flowdown reaches subcontracts for commercial products or commercial services, without alteration except to identify the parties [@CL-CMP-1434]. The contractor must determine whether the information required for subcontractor performance retains its identity as covered defense information and will require protection under the clause, and, if necessary, consult with the Contracting Officer [@CL-CMP-1435]. The clause does not remove the contractor's responsibility for other safeguarding or cyber incident reporting for its unclassified information systems required by other applicable clauses of the contract, or as a result of other applicable United States Government statutory or regulatory requirements [@CL-CMP-1432; @CL-CMP-7389].

## Worked example

Synthetic example. The numbers are made up.

A contractor discovers an incident that affects an unclassified server holding covered defense information. Twenty hours after discovery, security staff report the incident to DoD using a DoD-approved medium assurance certificate, which is inside the 72-hour limit. The contractor keeps images of the affected server and the relevant packet capture data for 120 days after submitting the report, which is longer than the 90-day minimum. It also finds that a subcontractor will handle the same covered defense information, so it includes the clause in that subcontract without alteration except to identify the parties.

## Never confuse

The 72-hour limit runs from discovery of any cyber incident [@CL-CMP-1002]. The 90-day preservation period runs from the submission of the cyber incident report [@CL-CMP-1004]. For the wider structure of the regulations, see [Acquisition regulations](../regulatory-framework/cmp-acquisition-regulations.md) and [Oversight roles](../audit-retention/cmp-oversight-roles.md).

## Company calibration

Confirming which systems process, store or transmit covered defense information, who holds the medium assurance certificate, and how the reporting clock is tracked. [@Q-014]

## Sources

## Change note

- COMPATIBLE_EXTENSION, 2026-10-31: Added compromise review examples and subcontracts paragraph scope, and removed uncited contrasts.
- COMPATIBLE_EXTENSION, 2026-10-31: Named the actor who submits malicious software.
- COMPATIBLE_EXTENSION, 2026-10-31: Corrected the other-reporting sentence so the clause rule keeps the statutory and regulatory alternative.
- COMPATIBLE_EXTENSION, 2026-10-29: Added the adequate security requirement, the incident review and reporting duties, the flowdown and the related paragraphs, and corrected the worked example and the contrast section.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for Safeguarding Covered Defense Information and Cyber Incident Reporting clause.
