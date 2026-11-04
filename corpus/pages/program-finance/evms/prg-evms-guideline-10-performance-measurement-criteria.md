+++
id = "prg-evms-guideline-10-performance-measurement-criteria"
title = "EVMS Guideline 10: performance measurement criteria for each work package"
area = "prg"
subarea = "evms"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["earned_value", "earned_value_management_system", "control_account", "performance_measurement_baseline"]
questions = ["Q-037"]
+++

## Purpose

Shows how the guide expects a contractor to pick a performance measurement technique for each work package, so that progress is measured objectively [@CL-PRG-0252].

## The rule

| Field | Value |
|---|---|
| Instrument | DoD EVMS Interpretation Guide, Guideline 10 [@CL-PRG-0251] |
| Codified citation | Not established. |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Not established. |
| Applies to | Programs that operate an earned value management system [@CL-PRG-0252] |

Each work package carries an objective measurement criterion chosen to fit the way the work is expected to be accomplished, and only effort that cannot be measured is treated as level of effort [@CL-PRG-0252]. Level of effort is confined to activities with no practicable, measurable output or product associated with technical effort that can be discretely planned and objectively measured at the work package level [@CL-PRG-0253].

Where a long-duration work package is unavoidable, interim milestones that model measurable technical accomplishment are required [@CL-PRG-0255]. Apportioned effort should be used for supporting work that ties directly to a discrete technical activity [@CL-PRG-0256]. If the contractor chooses to use it, the system documentation must cover the requirements for using the technique and the need to document the factor used to establish the relationship, and it must also point out that the progress identified in the base account (percent complete) provides the basis for the progress percentage of the apportioned account [@CL-PRG-0257; @CL-PRG-9149].

## How it works

The control account manager assesses each assigned work package and determines whether there is a discrete way to measure progress towards its completion [@CL-PRG-0254]. The criterion is objective and set per work package, consistent with how the work is expected to be accomplished [@CL-PRG-0252]. Level of effort earns value only as time passes, so it never shows a schedule variance and always equals the budgeted cost for work scheduled [@CL-PRG-0258]. Earned value for discrete material items may be claimed on receipt, inspection and acceptance, provided the items go into use within a reasonable time or are specifically identified to a serially numbered end item [@CL-PRG-0260]. The Program Evaluation and Review Technique (PERT) Cost Formula is a non-discrete earned value technique [@CL-PRG-0764]. It computes BCWP as (ACWP/EAC) x BAC and requires the estimate at completion to be evaluated and updated every month [@CL-PRG-0764; @CL-PRG-0259]. PERT is only appropriate for high-quantity, low-value and low-risk material items where measurement is impracticable, such as consumable material like bolts, fasteners, and welding rods [@CL-PRG-0763; @CL-PRG-0096].

## Worked example

Synthetic example. The numbers are made up.

A work package to build a test fixture is budgeted at 40,000 and planned to finish in three months. The manager finds a discrete measure, because the fixture is a definable product accepted at a single milestone, so the package is planned as discrete rather than level of effort. The fixture is accepted in month three and the full 40,000 is earned in that period.

## Never confuse

The nature of LOE is such that the BCWP reflects only the passage of time during which LOE activities are planned, so there is never a Schedule Variance (BCWP always equals BCWS), while the concept of apportioned effort should be used when work of a supporting nature ties directly to a discrete technical activity and, if the contractor chooses to use this technique, the EVMS documentation must cover the requirements for the use of this measurement technique and the need to document the factor used to establish the relationship, and must also point out that the progress identified in the base account (percent complete) provides the basis for the progress percentage of the apportioned account [@CL-PRG-0258; @CL-PRG-0256; @CL-PRG-0257; @CL-PRG-9149].

The guideline on planning work packages and planning packages is covered in [plan work packages and planning packages](prg-evms-guideline-9-work-and-planning-packages.md), and the guideline on assessing progress against the plan is covered in [assessing progress against the plan](prg-evms-guideline-14-assess-progress.md).

## Company calibration

Which work packages the company measures as level of effort, and whether the system description documents each technique and the evidence behind it. [@Q-037]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-11-04: Aligned the level-of-effort restriction wording with CL-PRG-0253.
- COMPATIBLE_EXTENSION, 2026-10-31: Added consumable-material examples for PERT and full apportioned-effort documentation rule in Never confuse.
- COMPATIBLE_EXTENSION, 2026-10-31: Apportioned-effort documentation, control account manager and PERT wording aligned to the guide.
- COMPATIBLE_EXTENSION, 2026-10-31: Cited the PERT claim for the BCWP formula and stated the apportioned-effort documentation condition.
- COMPATIBLE_EXTENSION, 2026-10-31: Removed an exclusive "only" the cited text does not state.
- COMPATIBLE_EXTENSION, 2026-10-31: Named the related guideline pages by topic in Never confuse.
- COMPATIBLE_EXTENSION, 2026-10-31: Linked the related card on neighbouring EVMS guidelines.
- COMPATIBLE_EXTENSION, 2026-10-29: Quote check: restated the material and apportioned effort sentences to match the guide.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for EVMS Guideline 10 performance measurement criteria.
