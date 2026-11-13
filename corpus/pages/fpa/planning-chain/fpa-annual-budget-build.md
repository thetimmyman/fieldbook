+++
id = "fpa-annual-budget-build"
title = "Annual budget build"
area = "fpa"
subarea = "planning-chain"
type = "procedure"
stage = 3
readiness = "SYNTHETIC-NOW"
status = "draft"
as_of = 2026-06-24
handling = ["public"]
terms = ["funded_backlog", "unfunded_backlog", "booked_backlog", "indirect_cost", "forward_pricing_rate_agreement", "provisional_billing_rate"]
questions = ["Q-018", "Q-024"]
links = ["rte-pool-and-base-structure", "rte-wrap-rate-build-up", "rte-forward-pricing-rate-agreements", "rte-billing-to-final-rate-cycle", "prg-evms-guideline-11-indirect-budgets", "fpa-funded-versus-unfunded-policy", "fpa-rolling-forecast", "fpa-variance-analysis", "ctf-contract-types"]
+++

## Purpose

Describes how a defence contractor builds its annual budget, how that budget rolls up from programs and indirect pools, and how it sets the planned indirect rates. It details the operational build sequence from backlog conversion through pool accumulation, rate computation, program indirect budgeting, and comparison with regulatory rate sets.

## The rule

Public sources fix specific rate and funding constraints: indirect budgets of an earned value baseline are built from indirect rates applied to direct resource budgets [@CL-PRG-0262; @CL-PRG-0263; @CL-PRG-0266]; the Government obligates only funded amounts [@CL-FPA-0050], fixed-price contracts are fully funded except where incremental funding is permitted by regulation [@CL-FPA-0372; @CL-FPA-0371], and an incrementally funded contract limits government reimbursement to the amount allotted [@CL-FPA-0083]; billing rates are temporary interim rates that should be as close as possible to anticipated final rates [@CL-RTE-0300; @CL-CTF-0124]; and forward pricing rates are projections in proposals or negotiated agreements used to price contracts and modifications [@CL-RTE-9076; @CL-RTE-0328; @CL-RTE-0402; @CL-RTE-0403].

Single source: none. The build order below is contractor practice, not a requirement [@CL-FPA-1301; @CL-FPA-1302; @CL-FPA-1303; @CL-FPA-1304; @CL-FPA-1305; @CL-FPA-1306; @CL-FPA-1307].

## How it works

1. In contractor practice, fix the budget frame by defining the fiscal year, participating programs, and indirect cost pools, keeping the budget unchanged once approved as the fixed comparison point for variance reporting [@CL-FPA-1306].
2. In contractor practice, start from booked backlog and its expected conversion to revenue by period, treating funded backlog as firm while keeping unfunded backlog, unexercised options, and probability-weighted new business as separate categories that are never added to funded backlog [@CL-FPA-1301; @CL-FPA-1302], reflecting statutory and regulatory limits where the Government creates obligations only within available funds [@CL-FPA-0050], requires fixed-price contracts to be fully funded except where an exception applies [@CL-FPA-0372], and limits reimbursement on incrementally funded items to the allotted amount [@CL-FPA-0083; @CL-FPA-0371].
3. In contractor practice, build the direct cost plan by program, establishing planned resource requirements across direct labor, direct material, and subcontract cost elements.
4. In contractor practice, collect each pool owner's pool budget by cost element within logical cost groupings [@CL-FPA-1303; @CL-RTE-0209], and set each pool's planned allocation base from the direct and intermediate budgets of the programs that the pool serves to ensure a common base across benefiting cost objectives [@CL-FPA-1303; @CL-RTE-0210].
5. In contractor practice, build the rate layers in sequence—direct program plan first, then the fringe pool, then the overhead pool, and finally the general and administrative pool—because each successive layer's allocation base includes the layers before it [@CL-FPA-1304].
6. In contractor practice, compute the planned indirect rate for each layer as that layer's budgeted pool divided by its budgeted base [@CL-FPA-1305].
7. Apply the planned indirect rates to program direct resource budgets at the applicable level to establish program indirect budgets in the performance measurement baseline across pools and cost sub-elements [@CL-PRG-0262; @CL-PRG-0263; @CL-PRG-0265; @CL-PRG-0266; @CL-PRG-0267], and in contractor practice tie out total applied indirect costs against each pool budget, showing any difference as planned over-absorption or under-absorption [@CL-FPA-1307].
8. Compare the planned budget rates with the rate sets used by the Government: forward pricing rates are projections detailed in formal proposals or negotiated in agreements used by contracting officers to price contracts and modifications [@CL-RTE-9076; @CL-RTE-0328; @CL-RTE-0402; @CL-RTE-0403], while billing rates are temporary interim rates that should be as close as possible to anticipated final rates [@CL-RTE-0300; @CL-CTF-0124]; in contractor practice, planned budget rates are the company's internal planning rates and do not replace either rate set [@CL-FPA-1305], and the budget is approved once for the fiscal year and kept unchanged as the fixed comparison point [@CL-FPA-1306].

## Worked example

Synthetic example. The numbers are made up.

An invented contractor prepares its annual budget for a budget year covering two programs, Program A and Program B.

- Budgeted direct labor 2,000,000 (Program A 1,200,000; Program B 800,000).
- Budgeted material and subcontract 1,500,000 (A 900,000; B 600,000).
- Fringe pool budget 500,000 on direct labor 2,000,000 = 25 percent.
- Labor cost base for overhead = 2,000,000 + 500,000 = 2,500,000.
- Overhead pool budget 1,050,000 on 2,500,000 = 42 percent.
- Total cost input base for G&A = 2,500,000 + 1,050,000 + 1,500,000 = 5,050,000.
- G&A pool budget 404,000 on 5,050,000 = 8.0 percent.
- Program A: fringe 300,000; labor cost 1,500,000; overhead 630,000; total cost input 1,500,000 + 630,000 + 900,000 = 3,030,000; G&A 242,400; budgeted cost 3,272,400.
- Program B: fringe 200,000; labor cost 1,000,000; overhead 420,000; total cost input 1,000,000 + 420,000 + 600,000 = 2,020,000; G&A 161,600; budgeted cost 2,181,600.
- Check: applied G&A 242,400 + 161,600 = 404,000 equals the pool budget; total budgeted cost 3,272,400 + 2,181,600 = 5,454,000 equals direct 3,500,000 plus the three pools 500,000 + 1,050,000 + 404,000 = 1,954,000.
- Known answer: planned rates 25 percent, 42 percent and 8.0 percent; no unabsorbed amount.

In contractor practice, because the annual budget starts from backlog [@CL-FPA-1301], a change in the volume of any base changes the planned rates, so the budget is rerun when the backlog plan changes.

## Never confuse

A budget is not a forecast: in contractor practice, the annual budget is approved once for the fiscal year and kept unchanged as the fixed comparison point for variance reporting, while a [Rolling forecast](../forecast-vintage/fpa-rolling-forecast.md) is continuously revised to reflect operational changes [@CL-FPA-1306].

Planned budget rates are not forward pricing rates, billing rates, or final rates: forward pricing rates are projections used for pricing contracts or modifications [@CL-RTE-9076], and billing rates are temporary interim rates established for interim reimbursement until final rates are settled [@CL-RTE-0300], whereas in contractor practice planned budget rates are internal planning rates that do not replace either [@CL-FPA-1305].

Funded backlog is not total backlog: under federal rules, fixed-price contracts must be fully funded except where an exception applies [@CL-FPA-0372], and on incrementally funded contracts the Government is not obligated to reimburse the contractor beyond the allotted amount [@CL-FPA-0083], while in contractor practice an annual budget treats funded backlog as firm and segregates unfunded backlog, options, and new business as separate categories [@CL-FPA-1302].

## Company calibration

Settling how this procedure applies to the company depends on [@Q-018] for how backlog, pipeline, funding, and revenue are bridged, and [@Q-024] for how pools and bases are defined.

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-11-12: Documents the procedure for building the annual budget, rolling up program and pool budgets, and setting planned indirect rates.
