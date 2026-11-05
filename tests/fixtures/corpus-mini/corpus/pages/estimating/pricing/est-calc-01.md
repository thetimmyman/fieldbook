+++
id = "est-calc-01"
title = "Synthetic landed cost and overhead build-up calculation"
area = "est"
subarea = "pricing"
type = "calc"
stage = 3
readiness = "SYNTHETIC-NOW"
status = "draft"
as_of = 2026-02-01
handling = ["public"]
terms = ["cost_baseline"]
questions = ["Q-002"]
volatility = "V3"
+++

## Purpose

Explain the synthetic landed cost and overhead build-up calculation.

## The rule

Landed cost sums the line extension, freight, duty and setup before applying overhead on the cost base and markup at the final price stage.

## How it works

Each stage rounds half away from zero to cents at the named stage boundaries, and totals sum the rounded extensions.

## Worked example

Synthetic example. The numbers are made up.

Fixture: CALC-01

## Never confuse

Do not confuse a cost baseline build-up with margin pricing on selling price.

## Company calibration

Confirm the rate structure and escalation basis for the cost baseline [@Q-002].

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-02-01: Added the synthetic landed cost calculation fixture page.
