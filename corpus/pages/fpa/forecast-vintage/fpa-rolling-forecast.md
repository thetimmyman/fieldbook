+++
id = "fpa-rolling-forecast"
title = "Rolling forecast"
area = "fpa"
subarea = "forecast-vintage"
type = "procedure"
stage = 3
readiness = "SYNTHETIC-NOW"
status = "draft"
as_of = 2026-06-24
handling = ["public"]
terms = ["forecast_vintage", "estimate_at_completion", "estimate_to_complete", "backlog_waterfall", "funded_backlog", "booked_backlog"]
questions = ["Q-010", "Q-018", "Q-025"]
links = ["fpa-annual-budget-build", "fpa-variance-analysis", "fpa-critical-accounting-estimates", "fpa-funded-versus-unfunded-policy", "prg-evms-guideline-20-eac-updates", "prg-evms-guideline-23-program-manager-eac", "rte-billing-to-final-rate-cycle"]
+++

## Purpose

Shows how a contractor keeps a rolling forecast: its cadence, what is re-estimated each cycle, how it ties to backlog and to the estimates at completion of the program-finance pages, and how each forecast vintage is kept.

## The rule

Public sources govern specific program-level estimates and external reporting requirements rather than the overarching company forecasting cycle. Under earned value guidance, an estimate at completion is actual cost to date plus the estimate to complete [@CL-PRG-0316]. The earned value management system guide has estimates at completion refreshed at least monthly at control-account level [@CL-PRG-0130], and the program manager's contract-level assessment at least monthly [@CL-PRG-0359], with a comprehensive estimate at least annually [@CL-PRG-0361]. The management estimate includes dollarized risks and opportunities tracked at the program level that have not yet been incorporated into control accounts [@CL-PRG-0360], and the externally reported and internal estimates at completion reconcile with a clear, consistent relationship [@CL-PRG-0362]. Regulation S-K asks registrants for critical accounting estimates [@CL-FPA-0035] and how much an estimate changed over a period to the extent material and reasonably available [@CL-FPA-0038]. Do not treat the EVMS cadence as a company forecast requirement; it is the earned value guide's cadence for program estimates.

Single source: none for the company forecast cycle. The cycle below is contractor practice [@CL-FPA-1311; @CL-FPA-1312].

## How it works

1. Close the latest period's actuals and set the actuals cut-off date [@CL-FPA-1312].
2. Roll the backlog forward in a backlog waterfall: opening booked backlog plus bookings plus modifications minus cancellations minus revenue equals ending backlog; revenue forecast from existing programs stays within the backlog available unless new orders are forecast [@CL-FPA-1313].
3. Re-estimate each program's estimate to complete and update its estimate at completion, using current direct and indirect rates [@CL-PRG-0316; @CL-PRG-0315; @CL-PRG-0712; @CL-PRG-0709; @CL-FPA-1314].
4. Refresh indirect rates by taking year-to-date actuals plus the forecast remainder [@CL-FPA-1316].
5. Spread each program's remaining cost and revenue over the forecast periods by its time-phased estimate to complete [@CL-PRG-0207; @CL-PRG-0208; @CL-FPA-1314].
6. Add new business by its probability and keep funded backlog and unfunded backlog apart [@CL-FPA-1312; @CL-FPA-0372; @CL-FPA-0083].
7. List the bridge between the company forecast and the sum of program estimates at completion: management reserve, unrealized risks and opportunities, and unapproved changes [@CL-FPA-1315; @CL-PRG-0360; @CL-PRG-0362].
8. Store the cycle as a forecast vintage: dated, unchanged, with cut-off date and assumptions; a correction is a new version [@CL-FPA-1317]; measure accuracy of each vintage by horizon [@CL-FPA-1318].

The cycle repeats on the fixed cadence and the horizon moves forward [@CL-FPA-1311].

## Worked example

Synthetic example. The numbers are made up.

An invented contractor maintains a rolling forecast on a quarterly cycle with a four-quarter horizon.

Backlog roll-forward for the quarter:
- Opening backlog: 10,000,000
- Bookings: +2,000,000
- Modifications: +500,000
- Cancellations: -300,000
- Revenue recognized: -2,600,000
- Ending backlog: 9,600,000 (10,000,000 + 2,000,000 + 500,000 - 300,000 - 2,600,000 = 9,600,000)

Program re-estimate:
- Budget at completion: 4,000,000
- Actual cost to date: 1,500,000
- Estimate to complete: 2,700,000
- Estimate at completion: 4,200,000 (1,500,000 + 2,700,000)
- Variance at completion: 4,000,000 - 4,200,000 = -200,000 (unfavorable, the estimate exceeds the budget)
- Remaining 2,700,000 spread over the next three quarters: 1,000,000, 1,000,000 and 700,000 (sum 2,700,000)

Forecast vintages:
- The vintage made at the end of the previous year forecast the second-quarter revenue at 2,400,000 (two quarters ahead).
- The vintage made at the end of the first quarter forecast it at 2,550,000 (one quarter ahead).
- The actual was 2,600,000.
- Forecast error: 2,400,000 - 2,600,000 = -200,000 or -7.7 percent of the actual at a horizon of two quarters.
- Forecast error: 2,550,000 - 2,600,000 = -50,000 or -1.9 percent at a horizon of one quarter.

Known answer: the nearer vintage was more accurate; both vintages are kept unchanged.

## Never confuse

In contractor practice, a forecast is not the budget: the budget is the fixed comparison point, and the forecast moves around it [@CL-FPA-1306]. Baseline operating plans are built in the [Annual budget build](../planning-chain/fpa-annual-budget-build.md), while differences between actuals, budgets, and forecasts are evaluated through [Variance analysis](../kpi/fpa-variance-analysis.md). In earned value management, the estimate at completion is not the budget at completion and the estimate to complete is not the budget left; an estimate at completion is actual cost to date plus the estimate to complete [@CL-PRG-0316], whereas the variance at completion is the difference between budget at completion and estimate at completion [@CL-PRG-0318].

In contractor practice, a forecast vintage is not the current live forecast: it is a stored historical version that remains unchanged at each cycle to track performance and measure accuracy across forward horizons [@CL-FPA-1317]. The company forecast is not the sum of program estimates at completion until the bridge is listed: management reserve, unrealized risks and opportunities, and unapproved changes must be listed explicitly rather than absorbed silently [@CL-FPA-1315; @CL-PRG-0362]. Significant shifts in program estimates and underlying assumptions also link to disclosure evaluations in [Critical accounting estimates in MD&A](fpa-critical-accounting-estimates.md).

Funded backlog is not total backlog: fixed-price contracts shall be fully funded except as permitted by DFARS 232.703-1 [@CL-FPA-0372], and on incrementally funded items the Government will not be obligated in any event to reimburse the contractor in excess of the amount allotted to the contract regardless of anything to the contrary in the termination for convenience clause [@CL-FPA-0083]. Contractors must keep funded backlog and unfunded backlog distinct in demand planning and roll-forwards rather than treating total contract value or unfunded amounts as authorized funding [@CL-FPA-0372; @CL-FPA-0083].

## Company calibration

Company calibration depends on who owns program estimates at completion, when they are refreshed, and how changes are reconciled to accounting and reporting [@Q-010], how backlog, pipeline, funding, and revenue are bridged in the operating plan [@Q-018], and which baseline, contingency, and risk decisions change the current program forecast [@Q-025].

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-11-12: Documents the rolling forecast procedure, backlog roll-forward, rate refresh, and forecast vintage tracking.
