+++
id = "cmp-basic-safeguarding-clause-controls"
title = "Basic safeguarding of covered contractor information systems: controls and flowdown"
area = "cmp"
subarea = "regulatory-framework"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
questions = ["Q-014"]
links = ["cmp-safeguarding-covered-defense-information"]
+++

## Purpose

Shows what a covered contractor information system is, the minimum security controls the basic safeguarding clause lists, and which subcontracts receive the clause.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified FAR [@CL-CMP-3546] |
| Codified citation | FAR 52.204-21 [@CL-CMP-3546; @CL-CMP-3547] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CMP-3546] |
| Applies to | Contractors with covered contractor information systems [@CL-CMP-3547] |

A covered contractor information system is an information system that is owned or operated by a contractor that processes, stores, or transmits Federal contract information [@CL-CMP-3546]. Federal contract information is information, not intended for public release, that is provided by or generated for the Government under a contract to develop or deliver a product or service to the Government, subject to the exclusions in the same sentence [@CL-CMP-3589]. It does not include information provided by the Government to the public, such as on public Web sites, or simple transactional information, such as necessary to process payments [@CL-CMP-3590]. The contractor shall apply the basic safeguarding requirements and procedures to protect covered contractor information systems, and they shall include, at a minimum, the listed security controls [@CL-CMP-3547].

Each control group below is part of the minimum controls that the basic safeguarding requirements shall include [@CL-CMP-3547]. One group limits information system access to authorized users, processes acting on behalf of authorized users, or devices, and limits access to the types of transactions and functions that authorized users are permitted to execute [@CL-CMP-3548]. Another verifies and controls or limits connections to and use of external information systems, controls information posted or processed on publicly accessible information systems, and identifies information system users, processes acting on behalf of users, or devices [@CL-CMP-3549]. A third authenticates the identities of users, processes or devices as a prerequisite to allowing access, and sanitizes or destroys media containing Federal Contract Information before disposal or release for reuse [@CL-CMP-3550]. Physical controls limit physical access to systems, equipment and operating environments to authorized individuals, and require escorting visitors, monitoring visitor activity, keeping audit logs of physical access and controlling physical access devices [@CL-CMP-3551]. Boundary controls monitor, control and protect organizational communications at the external boundaries and key internal boundaries [@CL-CMP-3552], and implement subnetworks for publicly accessible components that are physically or logically separated from internal networks, and identify, report and correct flaws in a timely manner [@CL-CMP-3553]. Malware controls provide protection from malicious code and update the protection mechanisms when new releases are available [@CL-CMP-3554], and periodic and real-time scans cover files from external sources as they are downloaded, opened, or executed [@CL-CMP-3555].

## How it works

The subcontracts paragraph tells the contractor to include the substance of the clause, including that paragraph, in subcontracts under the contract [@CL-CMP-3556]. That direction is limited to subcontracts, including those for commercial products or commercial services other than commercially available off-the-shelf items, in which the subcontractor may have Federal contract information residing in or transiting through its information system [@CL-CMP-3557]. The clause does not relieve the contractor of any other specific safeguarding requirements specified by Federal agencies and departments relating to covered contractor information systems generally, or other Federal safeguarding requirements for controlled unclassified information [@CL-CMP-3588]. The defense supplement rules on covered defense information are on [Safeguarding covered defense information and cyber incident reporting](../trade-cyber/cmp-safeguarding-covered-defense-information.md) and are not restated here.

## Worked example

Synthetic example. The numbers are made up.

A contractor keeps Federal contract information on an internal file server and a public web server. It limits server access to authorized users, keeps the public server on a separate subnetwork from the internal network, and runs scans on files that arrive from outside. It then awards a subcontract to a vendor that will hold the same kind of information, so it includes the substance of the clause in that subcontract.

## Never confuse

The clause sets a basic set of controls for covered contractor information systems [@CL-CMP-3547]; it is not the defense supplement clause on covered defense information, which is on [Safeguarding covered defense information and cyber incident reporting](../trade-cyber/cmp-safeguarding-covered-defense-information.md). The flowdown is tied to subcontractors that may have Federal contract information in their systems [@CL-CMP-3557], so it is not a blanket duty for every subcontract.

## Company calibration

Which company systems process, store or transmit Federal contract information, and who owns the basic safeguarding controls on them. [@Q-014]

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for basic safeguarding of covered contractor information systems.
