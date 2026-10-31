+++
id = "cmp-award-and-extension-cyber-checks"
title = "Award, option and extension checks on NIST SP 800-171 assessments and CMMC status"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["applicability_record", "control_evidence"]
questions = ["Q-004"]
links = ["cmp-nist-sp-800-171-dod-assessment-requirements", "cmp-cmmc-level-compliance-clause", "cmp-cmmc-level-2-assessment", "cmp-safeguarding-controls-representation-and-sprs-notice", "cmp-cmmc-affirmation-and-flowdown"]
+++

## Purpose

Shows what the contracting officer checks in the Supplier Performance Risk System (SPRS) before an award, an option exercise or a period of performance extension where the safeguarding clause or a CMMC level requirement applies, and what the contractor must hold for the check to pass.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CMP-7230] |
| Codified citation | DFARS 204.7303 [@CL-CMP-7230]; DFARS 204.7502 [@CL-CMP-7402]; DFARS 204.7503 [@CL-CMP-7235] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CMP-7230] |
| Applies to | Awards, options and extensions with a contractor required to implement NIST SP 800-171 under the clause at DFARS 252.204-7012 [@CL-CMP-7232; @CL-CMP-7233]; awards, options and extensions under a solicitation or contract that requires a CMMC level [@CL-CMP-7401; @CL-CMP-7403] |

Before awarding a contract, task order or delivery order to an offeror required to implement NIST SP 800-171 under the clause at DFARS 252.204-7012, the contracting officer must verify that the summary level score of a current NIST SP 800-171 DoD Assessment is posted in SPRS for each covered contractor information system that is relevant to the offer [@CL-CMP-7230; @CL-CMP-7231; @CL-CMP-7232]. The same verification comes before exercising an option period or extending the period of performance with such a contractor [@CL-CMP-7233]. A current assessment is one not more than 3 years old unless the solicitation specifies a lesser time [@CL-CMP-7230].

For CMMC, the contracting officer must include the required level in the solicitation if the program office or requiring activity provides one, and in the solicitation provision and contract clause that DFARS 204.7504 prescribes [@CL-CMP-7401; @CL-CMP-7234]. The contracting officer must not award to an offeror that lacks a current CMMC status at the level the solicitation requires [@CL-CMP-7402]. The officer may award, or modify a contract to exercise an option or extend its period of performance, when the status is one of the listed statuses and is equal to or higher than the required level [@CL-CMP-7403]. The listed statuses are Final Level 1 (Self), Conditional Level 2 (Self), Final Level 2 (Self), Conditional Level 2 (C3PAO), Final Level 2 (C3PAO), Conditional Level 3 (DIBCAC) and Final Level 3 (DIBCAC) [@CL-CMP-7404].

## How it works

The contracting officer checks SPRS and does not award unless the offeror has a current CMMC status posted there at the required level or higher for each CMMC UID the offeror provides [@CL-CMP-7235]. At award, those UIDs apply to each contractor information system that will process, store or transmit FCI or CUI and will be used in performance [@CL-CMP-7405].

Before an option is exercised or the period of performance is extended, the officer checks SPRS again and does not proceed unless the contractor has a current CMMC status posted there [@CL-CMP-7236]. That status must be at the level the contract, task order or delivery order requires, or higher, for each UID the contractor provides [@CL-CMP-7406]. The UIDs apply to each system that processes, stores or transmits FCI or CUI and is or will be used in performance [@CL-CMP-7407].

If the contractor provides new UIDs during performance, the officer checks in SPRS, using the UIDs that SPRS assigned, that the contractor has a current status at the required level or higher [@CL-CMP-7237]. That check covers each system identified that will process, store or transmit FCI or CUI during performance [@CL-CMP-7408].

The contractor must hold, at time of award, a CMMC status at the level the solicitation specifies or higher for all information systems used in performance that will process, store or transmit FCI or CUI [@CL-CMP-7238]. Where the contract requires it, the contractor must keep a current status at that level or higher throughout the life of the contract, task order or delivery order [@CL-CMP-7239].

CMMC levels 2 and 3 can be at a conditional level for no more than 180 days from the CMMC status date, and award can occur with a conditional level, while level 1 requires a final level for award [@CL-CMP-7240]. A status counts as current only inside the limits that the definition at DFARS 204.7501 sets [@CL-CMP-7480]:

- A Conditional Level 2 (Self) or Conditional Level 2 (C3PAO) status is current if it is not older than 180 days [@CL-CMP-7480], with no changes in compliance with the requirements at 32 CFR part 170 since the Conditional CMMC Status date and a corresponding affirmation of continuous compliance by an affirming official [@CL-CMP-7481].
- A Conditional Level 3 (DIBCAC) status is current if it is not older than 180 days with no such changes [@CL-CMP-7482] and has a corresponding affirmation of continuous compliance by an affirming official [@CL-CMP-7483].
- A Final Level 1 (Self) status is current if it is not older than 1 year [@CL-CMP-7483], with no changes in compliance since the Final CMMC Status date and a corresponding affirmation of continuous compliance, not older than 1 year, by an affirming official [@CL-CMP-7484].
- A Final Level 2 (Self) or Final Level 2 (C3PAO) status is current if it is not older than 3 years with no such changes since the Final CMMC Status date [@CL-CMP-7485], and has a corresponding affirmation, not older than 1 year [@CL-CMP-7486].
- A Final Level 3 (DIBCAC) status is current if it is not older than 3 years [@CL-CMP-7486], with no such changes and a corresponding affirmation, not older than 1 year [@CL-CMP-7487].
- An affirmation of continuous compliance is itself current when it is not older than 1 year with no changes in compliance [@CL-CMP-7488].

The contractor's own duties under the CMMC clause are on the [CMMC level compliance clause card](../trade-cyber/cmp-cmmc-level-compliance-clause.md) and are not restated here.

## Worked example

Synthetic example. The numbers are made up.

A solicitation requires a Level 2 (C3PAO) status and sets no assessment age shorter than the standard one. The offeror is required to implement NIST SP 800-171 under the clause at DFARS 252.204-7012 [@CL-CMP-7232]. It provides two CMMC UIDs, one for each system that will process CUI, and SPRS shows a Conditional Level 2 (C3PAO) status dated 40 days earlier for both, with no changes in compliance since that date and a corresponding affirmation of continuous compliance, so the status is current because it is under 180 days old [@CL-CMP-7480; @CL-CMP-7481]. The summary level score of its NIST SP 800-171 DoD Assessment, dated 1 year earlier, is also posted in SPRS. The contracting officer verifies the score, which is under 3 years old [@CL-CMP-7230; @CL-CMP-7231], checks SPRS for both UIDs [@CL-CMP-7235], and may award because levels 2 and 3 can be conditional and award can occur with a conditional level [@CL-CMP-7240]. Before the first option, SPRS shows no current status posted for the contractor's UIDs, so the officer does not exercise the option [@CL-CMP-7236].

## Never confuse

The verification of a NIST SP 800-171 assessment score under DFARS 204.7303 and the check of a CMMC status under DFARS 204.7503 are separate checks of separate SPRS postings: the first looks for the summary level score of a current assessment, and the second looks for a current CMMC status at the required level for each CMMC UID [@CL-CMP-7230; @CL-CMP-7235].

A conditional status is not a final status. Levels 2 and 3 can be conditional at award, but level 1 requires a final level [@CL-CMP-7240]. The assessment requirements card explains how an assessment is used in an offer; see [the assessment requirements card](../trade-cyber/cmp-nist-sp-800-171-dod-assessment-requirements.md).

## Company calibration

Which information systems are in scope, what assessment score and CMMC UID is posted for each, and who confirms the posting before an award or option decision [@Q-004].

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for award, option and extension checks on NIST SP 800-171 assessments and CMMC status.
