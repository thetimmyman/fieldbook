+++
id = "est-rate-reference"
title = "Synthetic reference table of pricing rate windows and placeholders"
area = "est"
subarea = "pricing"
type = "reference"
stage = 2
readiness = "SYNTHETIC-NOW"
status = "draft"
as_of = 2026-02-01
handling = ["public"]
terms = ["rate_window", "cost_baseline"]
questions = ["Q-002"]
volatility = "V2"
+++

## Purpose

Provide a synthetic lookup table of pricing rate review parameters.

## The rule

| Parameter | Value | Basis |
|---|---|---|
| Rate review window | 10 days [@CL-EST-0003] | Current synthetic pricing guidance [@CL-EST-0003] |
| Escalation placeholder | 3 percent [@CL-EST-0001] | Synthetic multi-year assumption [@CL-EST-0001] |

## How it works

Look up the synthetic parameter in the table before updating a cost baseline.

## Worked example

Synthetic example. The numbers are made up.

Applying the 3 percent placeholder to a 1,000 USD base adds 30 USD for the synthetic year.

## Never confuse

Do not confuse a synthetic rate window with a binding forward pricing rate agreement.

## Company calibration

Confirm the multi-year escalation index [@Q-002].

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-02-01: Added the synthetic pricing reference page.
