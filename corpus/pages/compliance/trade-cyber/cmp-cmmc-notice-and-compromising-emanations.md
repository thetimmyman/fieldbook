+++
id = "cmp-cmmc-notice-and-compromising-emanations"
title = "CMMC level notice and protection against compromising emanations"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["defense_far_supplement"]
questions = ["Q-014"]
links = ["cmp-cmmc-level-compliance-clause", "cmp-cmmc-poam-and-scoring", "cmp-cmmc-affirmation-and-flowdown", "cmp-cmmc-program-scope-and-phases", "cmp-cmmc-model-levels-and-scoping", "cmp-cloud-computing-services-clause"]
+++

## Purpose

Shows what an offeror must have before award under the CMMC level notice, and what the emanations clause requires of accredited information technology.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CMP-4008; @CL-CMP-4012] |
| Codified citation | DFARS 252.204-7025 [@CL-CMP-4008]; DFARS 252.239-7000 [@CL-CMP-4012] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CMP-4008; @CL-CMP-4012] |
| Applies to | Offerors, for each contractor information system that will process, store, or transmit Federal contract information or controlled unclassified information during performance of the contract [@CL-CMP-4008]; contractors providing or using information technology as specified by the Government [@CL-CMP-4012] |

The solicitation states the required CMMC level as one of CMMC Level 1 (Self), CMMC Level 2 (Self), CMMC Level 2 (C3PAO), or CMMC Level 3 (DIBCAC), which the Contracting Officer inserts [@CL-CMP-4049]. That level, or higher, is required before award for each contractor information system that will process, store, or transmit Federal contract information or controlled unclassified information during performance of the contract [@CL-CMP-4008]. The offeror will not be eligible for award if it does not have, for each of those systems that will be used in performance of a contract, the items listed in the provision [@CL-CMP-4009]. The two listed items are the current CMMC status entered in the Supplier Performance Risk System at the required CMMC level, and a current affirmation of continuous compliance with the security requirements in the CMMC rule entered in that system [@CL-CMP-4009; @CL-CMP-4010]. If the offeror has a CMMC Status of Conditional, it shall successfully close out a valid plan of action and milestones to achieve a CMMC Status of Final [@CL-CMP-4050]. The offeror shall also provide in its proposal the CMMC unique identifiers issued by that system for each contractor information system that will process, store, or transmit Federal contract information or controlled unclassified information during performance of a resulting contract, task order, or delivery order [@CL-CMP-4011]. It also shall update the list of identifiers when new identifiers are generated in that system [@CL-CMP-4051].

The contractor shall provide or use only information technology, as specified by the Government, that has been accredited to meet the appropriate information assurance requirements of the listed standards, the first of which is the National Security Agency National TEMPEST Standards [@CL-CMP-4012]. The second listed alternative is other standards specified by the contract, including the date through which the required accreditation is current or valid for the contract [@CL-CMP-4052]. Upon request of the Contracting Officer, the contractor shall provide documentation supporting the accreditation [@CL-CMP-4013]. The Government may, as part of its inspection and acceptance, conduct additional tests to ensure that information technology delivered under the contract satisfies the specified information assurance standards [@CL-CMP-4014]. It may conduct those tests at the installation site or contractor's facility, and notwithstanding the existence of valid accreditations of the information technology prior to award [@CL-CMP-4053]. Unless otherwise provided under the Warranty of Supplies or Warranty of Systems and Equipment clause, the contractor shall correct or replace accepted information technology found to be deficient within 1 year after proper installations [@CL-CMP-4015]. The correction or replacement shall be at no cost to the Government [@CL-CMP-4054].

## How it works

The CMMC notice is a before-award test, applied system by system: the required level, the status and affirmation held, a plan of action where the status is Conditional, and the unique identifiers provided in the proposal [@CL-CMP-4049; @CL-CMP-4008; @CL-CMP-4009; @CL-CMP-4010; @CL-CMP-4050; @CL-CMP-4011]. The emanations clause is a delivery test on information technology: accreditation, documentation on request, possible additional Government tests, and a correction duty after installation [@CL-CMP-4012; @CL-CMP-4013; @CL-CMP-4014; @CL-CMP-4015]. The CMMC levels, assessments and flowdown sit on other cards and are not restated here.

## Worked example

Synthetic example. The numbers are made up.

A solicitation states a required CMMC level. An offeror has two systems that will hold controlled unclassified information. One system has the required status and a current affirmation, and its unique identifier is in the proposal. The other has only a lower status than the level required. The offeror is not eligible for award while that second system lacks the required status and affirmation. Separately, the contract buys accredited equipment, and the Government tests it on site after delivery.

## Never confuse

The offeror notice before award is a separate document from the contractor compliance clause; see [Contractor compliance with CMMC level requirements](cmp-cmmc-level-compliance-clause.md). Plan of action and milestones and scoring are on [CMMC POA&M conditions and scoring](cmp-cmmc-poam-and-scoring.md), and affirmation and flowdown are on [CMMC affirmation and subcontractor flowdown](cmp-cmmc-affirmation-and-flowdown.md). The emanations clause concerns accreditation of information technology against TEMPEST or other specified standards, not cybersecurity maturity levels. Cloud services have their own security clause; see [Cloud computing services clause](cmp-cloud-computing-services-clause.md).

## Company calibration

Which information systems will handle Federal contract information or controlled unclassified information on each solicitation, the CMMC status and unique identifier held for each, and who owns open findings. [@Q-014]

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for the CMMC level notice and protection against compromising emanations.
