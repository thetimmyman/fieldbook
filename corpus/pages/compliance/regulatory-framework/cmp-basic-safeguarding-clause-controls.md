+++
id = "cmp-basic-safeguarding-clause-controls"
title = "Basic safeguarding of covered contractor information systems: controls and flowdown"
area = "cmp"
subarea = "regulatory-framework"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-01-20
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
| RFO citation | RFO FAR 52.240-93 [@CL-CMP-0495; @CL-CMP-0498] |
| Edition date | NOV 2021 [@CL-CMP-10080] |
| Text status | Final [@CL-CMP-3546] |
| Applies to | Contractors with covered contractor information systems [@CL-CMP-3547] |

A covered contractor information system is an information system that is owned or operated by a contractor that processes, stores, or transmits Federal contract information [@CL-CMP-3546]. Federal contract information is information, not intended for public release, that is provided by or generated for the Government under a contract to develop or deliver a product or service to the Government, subject to the exclusions in the same sentence [@CL-CMP-3589]. It does not include information provided by the Government to the public, such as on public Web sites, or simple transactional information, such as necessary to process payments [@CL-CMP-3590]. The contractor shall apply the basic safeguarding requirements and procedures to protect covered contractor information systems, and they shall include, at a minimum, the listed security controls [@CL-CMP-3547].

Each control group below is part of the minimum controls that the basic safeguarding requirements shall include [@CL-CMP-3547]. One group limits information system access to authorized users, processes acting on behalf of authorized users, or devices, and limits access to the types of transactions and functions that authorized users are permitted to execute [@CL-CMP-3548]. Another verifies and controls or limits connections to and use of external information systems, controls information posted or processed on publicly accessible information systems, and identifies information system users, processes acting on behalf of users, or devices [@CL-CMP-3549]. A third authenticates the identities of users, processes or devices as a prerequisite to allowing access, and sanitizes or destroys media containing Federal Contract Information before disposal or release for reuse [@CL-CMP-3550]. Physical controls limit physical access to systems, equipment and operating environments to authorized individuals, and require escorting visitors, monitoring visitor activity, keeping audit logs of physical access and controlling physical access devices [@CL-CMP-3551]. Boundary controls monitor, control and protect organizational communications at the external boundaries and key internal boundaries [@CL-CMP-3552], and implement subnetworks for publicly accessible components that are physically or logically separated from internal networks, and identify, report and correct flaws in a timely manner [@CL-CMP-3553]. Malware controls provide protection from malicious code and update the protection mechanisms when new releases are available [@CL-CMP-3554], and periodic scans of the information system and real-time scans of files from external sources are performed as files are downloaded, opened, or executed [@CL-CMP-3555].

## How it works

The subcontracts paragraph tells the contractor to include the substance of the clause, including that paragraph, in subcontracts under the contract [@CL-CMP-3556]. That direction reaches subcontracts under the contract, including subcontracts for the acquisition of commercial products or commercial services other than commercially available off-the-shelf items, in which the subcontractor may have Federal contract information residing in or transiting through its information system [@CL-CMP-3557]. The clause does not relieve the contractor of any other specific safeguarding requirements specified by Federal agencies and departments relating to covered contractor information systems generally, or other Federal safeguarding requirements for controlled unclassified information [@CL-CMP-3588]. The defense supplement rules on covered defense information are on [Safeguarding covered defense information and cyber incident reporting](../trade-cyber/cmp-safeguarding-covered-defense-information.md) and are not restated here.

Under the RFO text, the DoD class deviation directs DoD contracting officers to use the RFO FAR part 52 text in place of the codified text, effective February 1, 2026 [@CL-CMP-9760] [@CL-CMP-9762]. The RFO text lists FAR 52.204-21 as reserved [@CL-CMP-0494], and prints the basic safeguarding clause at FAR 52.240-93 [@CL-CMP-0495]. It keeps the definition of a covered contractor information system [@CL-CMP-0495; @CL-CMP-3546]. It keeps the definition of Federal contract information and its exclusion for information provided by the Government to the public and for simple transactional information [@CL-CMP-0496; @CL-CMP-0497; @CL-CMP-3589; @CL-CMP-3590]. For the controls, paragraph (b)(1) says the Contractor shall safeguard its covered contractor information systems by implementing, at minimum, the listed security controls, in place of the codified wording about applying basic safeguarding requirements and procedures [@CL-CMP-0498; @CL-CMP-3547]. The RFO list keeps the same controls in groups: access limits and external connections [@CL-CMP-0499; @CL-CMP-0500; @CL-CMP-3548; @CL-CMP-3549], authentication and media sanitization [@CL-CMP-0519; @CL-CMP-3550], physical access [@CL-CMP-0520; @CL-CMP-3551], boundary protection [@CL-CMP-0521; @CL-CMP-3552], subnetworks and flaw correction [@CL-CMP-0522; @CL-CMP-3553], malicious code protection [@CL-CMP-0523; @CL-CMP-3554], and scans [@CL-CMP-0524; @CL-CMP-3555]. It keeps the statement that the clause does not relieve the Contractor of other safeguarding requirements, including Federal safeguarding requirements for controlled unclassified information as established by Executive Order 13556 [@CL-CMP-0525; @CL-CMP-0621; @CL-CMP-3588], and keeps the direction to include the substance of the clause in subcontracts under the contract [@CL-CMP-0526; @CL-CMP-3556]. For the subcontracts that may hold Federal contract information, the RFO wording places the exception for commercially available off-the-shelf items between commercial products and commercial services [@CL-CMP-0527; @CL-CMP-3557].

## Worked example

Synthetic example. The numbers are made up.

A contractor keeps Federal contract information on an internal file server and a public web server. It limits server access to authorized users, keeps the public server on a separate subnetwork from the internal network, and runs scans on files that arrive from outside. It then awards a subcontract to a vendor that will hold the same kind of information, so it includes the substance of the clause in that subcontract.

## Never confuse

Basic safeguarding of a covered contractor information system is not the safeguarding of covered defense information: the clause sets, at a minimum, a basic set of security controls for covered contractor information systems [@CL-CMP-3547], and the flowdown reaches subcontracts in which the subcontractor may have Federal contract information residing in or transiting through its information system [@CL-CMP-3557]. For covered defense information, see [Safeguarding covered defense information and cyber incident reporting](../trade-cyber/cmp-safeguarding-covered-defense-information.md). In the codified clause the exception for commercially available off-the-shelf items is written after both commercial products and commercial services [@CL-CMP-3557], while the RFO clause writes it after commercial products and before commercial services [@CL-CMP-0527]. The clause on privacy or security safeguards, with the drone prohibition, is covered in [Privacy or security safeguards and the drone prohibition](cmp-privacy-and-security-safeguards-and-drone-prohibition.md).

## Company calibration

Which company systems process, store or transmit Federal contract information, and who owns the basic safeguarding controls on them. [@Q-014]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-11-24: Filled the Edition date row with cited clause edition dates.

- COMPATIBLE_EXTENSION, 2026-11-23: Never confuse states a contrast.
- COMPATIBLE_EXTENSION, 2026-10-31: corrected the scan wording and removed uncited contrasts.
- COMPATIBLE_EXTENSION, 2026-10-31: Linked the related card on privacy or security safeguards and the drone prohibition.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the RFO FAR part 52 citation and how DoD's class deviation applies it.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for basic safeguarding of covered contractor information systems.
