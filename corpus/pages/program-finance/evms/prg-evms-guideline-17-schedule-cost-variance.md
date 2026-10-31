+++
id = "prg-evms-guideline-17-schedule-cost-variance"
title = "Calculate schedule variance and cost variance"
area = "prg"
subarea = "evms"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["planned_value", "earned_value", "actual_cost"]
questions = ["Q-037"]
links = ["cmp-accounting-system-criteria"]
+++

## Purpose

Shows how the guideline defines schedule variance and cost variance and requires them to be calculated at least monthly [@CL-PRG-0306; @CL-PRG-0308].

## The rule

| Field | Value |
|---|---|
| Instrument | Not established. |
| Codified citation | Not established. |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Not established. |
| Applies to | The control account and higher levels as necessary for management control, at least monthly [@CL-PRG-0308] |

Schedule variance compares the planned budget to the budget earned for work accomplished [@CL-PRG-0306], and cost variance compares the budget earned to the actual (applied where appropriate) direct costs for the same work [@CL-PRG-0307]. In the guideline's formulas, schedule variance equals BCWP minus BCWS [@CL-PRG-0704], and cost variance equals BCWP minus ACWP [@CL-PRG-0713]. This information is generated at least monthly for the control account and higher levels, as necessary for management control, using actual cost data from or reconcilable with the accounting system [@CL-PRG-0308]. A positive value indicates a favorable condition and a negative value an unfavorable condition [@CL-PRG-0309].

## How it works

At least monthly, the system compares the planned budget to the budget earned for the work accomplished, producing schedule variance [@CL-PRG-0306; @CL-PRG-0308]. It then compares the budget earned to the actual (applied where appropriate) direct costs for the same work, producing cost variance [@CL-PRG-0307]. A positive value is favorable and a negative value is unfavorable [@CL-PRG-0309].

## Worked example

Synthetic example. The numbers are made up.

A control account planned a budget of 100,000 and earned 90,000 for the work accomplished. Schedule variance is the budget earned minus the planned budget, so it is 90,000 minus 100,000, which is minus 10,000 and unfavorable. The actual direct costs for that work are 95,000. Cost variance is the budget earned minus the actual cost, so it is 90,000 minus 95,000, which is minus 5,000 and unfavorable.

## Never confuse

Schedule variance and cost variance measure different things; schedule variance compares planned budget to budget earned, while cost variance compares budget earned to actual direct costs for the same work.

The guideline on assessing progress against the plan is covered in [EVMS Guideline 14: assessing progress against the plan](prg-evms-guideline-14-assess-progress.md).

## Company calibration

Whether schedule and cost variances are calculated monthly at the control account level. [@Q-037]

## Sources

## Change note

- COMPATIBLE_EXTENSION, 2026-10-31: Linked the related card on EVMS Guideline 14.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for EVMS Guideline 17 schedule and cost variance.
