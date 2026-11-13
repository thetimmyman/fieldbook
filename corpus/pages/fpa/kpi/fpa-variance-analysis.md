+++
id = "fpa-variance-analysis"
title = "Variance analysis"
area = "fpa"
subarea = "kpi"
type = "concept"
stage = 3
readiness = "SYNTHETIC-NOW"
status = "draft"
as_of = 2026-06-24
handling = ["public"]
terms = ["labor_rate_variance", "labor_efficiency_variance", "overhead_volume_variance", "purchase_price_variance", "material_usage_variance", "estimate_at_completion", "budget_at_completion"]
questions = ["Q-017", "Q-024"]
links = ["fpa-annual-budget-build", "fpa-rolling-forecast", "fpa-mdna-results-of-operations", "prg-evms-guideline-17-schedule-cost-variance", "prg-evms-guideline-18-analyze-variances", "prg-evms-guideline-19-indirect-cost-variances", "rte-pool-and-base-structure"]
+++

## Purpose

Explains variance analysis of actual results against the budget and against the forecast, covering price, rate, volume, and mix variances and the thresholds that make an explanation necessary. Distinguishes budget variances from earned-value cost variances and outlines how indirect cost variances are analyzed.

## The rule

Public sources fix the earned value side only: the EVMS guide requires schedule and cost variances at least monthly at the control account, analysis of significant differences with reasons, identification of factors such as efficiency, rate and timing, corrective actions in the analysis, and explanation of variances that exceed the established internal or external thresholds [@CL-PRG-0308; @CL-PRG-0310; @CL-PRG-0705; @CL-PRG-0706; @CL-PRG-0708; @CL-PRG-0707]. Indirect cost variance is the part of total cost variance attributed to indirect rate adjustments [@CL-PRG-0314]. The guide sets no numeric threshold in the claims of this Corpus: numeric thresholds: Not established.

Single source: none for the budget variance formulas. They are contractor practice [@CL-FPA-1321; @CL-FPA-1322; @CL-FPA-1323; @CL-FPA-1324; @CL-FPA-1325; @CL-FPA-1326; @CL-FPA-1327]. In contractor practice, a price variance is the difference between the actual and the budgeted unit price, such as a rate per hour or a price per unit, multiplied by the actual quantity [@CL-FPA-1321]. In contractor practice, a volume variance is the difference between the actual total quantity and the budgeted total quantity, multiplied by the budgeted average price or the budgeted average margin per unit [@CL-FPA-1322]. In contractor practice, a mix variance is the difference between the actual quantity of each product or labor category and the quantity it would have had at the budgeted mix of the actual total quantity, multiplied by the budgeted price or margin of that item and summed over the items [@CL-FPA-1323]. In contractor practice, a labor rate variance is the actual hourly rate less the budget hourly rate, multiplied by the actual hours, and a labor efficiency variance is the actual hours less the hours allowed for the actual output, multiplied by the budget hourly rate [@CL-FPA-1324]. In contractor practice, price, volume and mix variances are calculated in a fixed order so that together they equal the total variance, and any part that cannot be traced to a component is reported as unexplained rather than hidden in another component [@CL-FPA-1325]. In contractor practice, an indirect pool's under-absorption or over-absorption for a period is split into a spending variance, which is the actual pool cost less the budgeted pool cost, and a volume variance, which is the planned rate multiplied by the budgeted base less the actual base, to the extent the pool behaves as a fixed cost [@CL-FPA-1326]. In contractor practice, a variance explanation is required when the variance exceeds both an amount threshold and a percentage threshold set by company policy, and the thresholds differ by level of the reporting structure [@CL-FPA-1327].

## How it works

In practice, price, volume, and mix variances are calculated in a fixed order so that together they equal the total variance, and any part that cannot be traced to a component is reported as unexplained rather than hidden in another component [@CL-FPA-1325]. In practice, a price variance is the difference between the actual and the budgeted unit price, such as a rate per hour or a price per unit, multiplied by the actual quantity [@CL-FPA-1321]. In practice, a volume variance is the difference between the actual total quantity and the budgeted total quantity, multiplied by the budgeted average price or the budgeted average margin per unit [@CL-FPA-1322]. In practice, a mix variance is the difference between the actual quantity of each product or labor category and the quantity it would have had at the budgeted mix of the actual total quantity, multiplied by the budgeted price or margin of that item and summed over the items [@CL-FPA-1323].

In practice, a labor rate variance is the actual hourly rate less the budget hourly rate, multiplied by the actual hours, and a labor efficiency variance is the actual hours less the hours allowed for the actual output, multiplied by the budget hourly rate [@CL-FPA-1324]. In practice, an indirect pool's under-absorption or over-absorption for a period is split into a spending variance, which is the actual pool cost less the budgeted pool cost, and a volume variance, which is the planned rate multiplied by the budgeted base less the actual base, to the extent the pool behaves as a fixed cost [@CL-FPA-1326]. This connects to the indirect pool and base mechanisms in [Fringe, overhead and G&A pools and their bases](../../indirect-rates/pools-bases/rte-pool-and-base-structure.md), where comparing actual indirect expenses with established indirect budgets shows whether absorption under existing documented allocation schemes is on track or whether allocation rates require adjustment [@CL-PRG-0268].

In practice, a variance explanation is required when the variance exceeds both an amount threshold and a percentage threshold set by company policy, and the thresholds differ by level of the reporting structure [@CL-FPA-1327]. Under the EVMS guide, cost and schedule variances that exceed established internal or external thresholds are considered significant enough to require in-depth analysis and possible management action [@CL-PRG-0708]; numeric thresholds: Not established. In practice, a budget variance is explained against the budget and separately against the latest forecast, and the explanation says what happened, whether the effect is timing or permanent, and what action follows [@CL-FPA-1328]. Schedule and cost variances are identified and analyzed at the control account and summary level, and the analysis identifies the factors causing the variance, such as efficiency, rate and timing, and its potential impacts [@CL-PRG-0705], with corrective actions or mitigation plans identified in the variance analysis [@CL-PRG-0706]. In the EVMS guide, a positive schedule or cost variance value indicates a favorable condition, and a negative value indicates an unfavorable condition [@CL-PRG-0309]; a company must state its own convention for budget variances: Not established.

## Worked example

Synthetic example. The numbers are made up.

Setup: invented contractor, one quarter, revenue from hours billed on two labor categories.

- Budget: Senior engineer 5,000 hours at 150 per hour = 750,000; Technician 5,000 hours at 90 per hour = 450,000; total 10,000 hours, revenue 1,200,000, budget average price 120 per hour, budget mix 50 percent and 50 percent.
- Actual: Senior engineer 4,000 hours at 155 per hour = 620,000; Technician 7,000 hours at 88 per hour = 616,000; total 11,000 hours, revenue 1,236,000.
- Total revenue variance = 1,236,000 - 1,200,000 = +36,000.
- Price variance = (actual price - budget price) x actual hours: Senior (155 - 150) x 4,000 = +20,000; Technician (88 - 90) x 7,000 = -14,000; total +6,000.
- Volume variance = (11,000 - 10,000) x 120 = +120,000.
- Mix variance = sum of (actual hours - actual total hours x budget share) x budget price: at the budget mix the actual 11,000 hours would be 5,500 and 5,500; Senior (4,000 - 5,500) x 150 = -225,000; Technician (7,000 - 5,500) x 90 = +135,000; total -90,000.
- Check: +6,000 + 120,000 - 90,000 = +36,000, equal to the total.
- Threshold example (synthetic policy: explain when a variance is at least 50,000 and at least 5 percent of its budget line, on total revenue budget 1,200,000): price +6,000 is 0.5 percent, no explanation; volume +120,000 is 10.0 percent, explain; mix -90,000 is 7.5 percent, explain.
- Labor cost: budget rate 70 per hour; actual rate 72 per hour; actual hours 11,000; hours allowed for the actual output 10,500. Rate variance = (72 - 70) x 11,000 = 22,000 unfavorable; efficiency variance = (11,000 - 10,500) x 70 = 35,000 unfavorable; total 57,000 unfavorable = actual cost 792,000 less allowed cost 735,000.
- Overhead pool: planned rate 40 percent of a budgeted labor base of 1,000,000 = pool budget 400,000; actual pool 430,000; actual base 900,000; applied overhead = 40 percent x 900,000 = 360,000; under-absorbed 430,000 - 360,000 = 70,000 = spending variance 430,000 - 400,000 = 30,000 plus volume variance 40 percent x (1,000,000 - 900,000) = 40,000.

## Never confuse

A budget variance is not an earned-value cost variance. A budget variance compares actual costs with the budget or forecast for the period. The EVMS cost variance compares the amount of the budget earned for work accomplished with the actual direct costs for the same work: BCWP minus ACWP [@CL-PRG-0713; @CL-PRG-0307; @CL-PRG-0704]; schedule variance is BCWP minus BCWS [@CL-PRG-0704; @CL-PRG-0306]. A program can spend less than its period budget, generating a favorable budget variance, while falling behind on work, generating an unfavorable schedule variance, so the first does not show the second. In a synthetic illustration: period budget 500,000; actual cost 450,000; work scheduled 500,000; work accomplished (earned) 400,000: budget variance 50,000 favorable; cost variance 400,000 - 450,000 = -50,000 unfavorable; schedule variance 400,000 - 500,000 = -100,000 unfavorable.

A variance against the budget is not a variance at completion: the variance at completion is the difference between the budget at completion and the estimate at completion for a control account or summary level planning package [@CL-PRG-0318], where the estimate at completion is the actual cost of work performed to date plus the estimate to complete for the remaining authorized work [@CL-PRG-0316]. While the [Annual budget build](../planning-chain/fpa-annual-budget-build.md) sets the initial baseline operating plan, expected results across the planning horizon are updated through a [Rolling forecast](../forecast-vintage/fpa-rolling-forecast.md).

An indirect rate change is not a rate variance on labor: the program's indirect cost variance is the portion of total cost variance attributed to indirect rate adjustments [@CL-PRG-0314], analyzed by conducting an impact analysis by indirect rate category to determine its contribution to the overall program [@CL-PRG-0710], whereas in contractor practice a labor rate variance is the hourly labor price effect [@CL-FPA-1324]. Furthermore, internal replanning should not be used as an alternative to proper initial planning, nor to mask legitimate variances [@CL-PRG-0760].

## Company calibration

Which reporting measures management uses and how definitions tie to ledger and program data [@Q-017], and how indirect cost pools and allocation bases are defined and reconciled [@Q-024].

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-11-12: Explains variance analysis against budget and forecast, decomposition formulas, and distinctions from earned value cost variances.
