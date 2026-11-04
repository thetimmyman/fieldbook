+++
id = "ctf-dfars-nsp-line-item-payment"
title = "Payment for line or subline items not separately priced"
area = "ctf"
subarea = "billing-payment"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["defense_far_supplement", "billing"]
questions = ["Q-026"]
links = ["ctf-dfars-line-item-and-subline-item-criteria", "ctf-dfars-modification-line-items-and-order-of-application", "ctf-proper-invoice-content", "ctf-dfars-billing-instructions-cost-vouchers"]
+++

## Purpose

Shows when a contractor may invoice an item whose price includes an item marked not separately priced.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CTF-1803] |
| Codified citation | DFARS 252.204-7002 [@CL-CTF-1803] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CTF-1803] |
| Applies to | Contracts whose schedule contains a line or subline item identified as not separately priced [@CL-CTF-1803] |

If the schedule contains any line or subline items identified as not separately priced (NSP), the unit price for the NSP item is included in the unit price of another, related line or subline item [@CL-CTF-1803]. The Contractor shall not invoice the Government for an item that includes an NSP item in its price until the Contractor has also delivered the NSP item and the Government has accepted it [@CL-CTF-1804]. The clause does not apply to technical data [@CL-CTF-1805].

The DFARS directs use of the clause as prescribed in DFARS 204.7109(a) [@CL-CTF-1831].

## How it works

For a line or subline item identified as NSP, the unit price is included in the unit price of another, related line or subline item [@CL-CTF-1803]. The Contractor shall not invoice the Government for an item that includes in its price an NSP item until the Contractor has also delivered the NSP item included in the price of the item being invoiced and the Government has accepted the NSP item [@CL-CTF-1804]. The clause does not apply to technical data [@CL-CTF-1805]. The line item rules are on [DFARS line item and subline item criteria](ctf-dfars-line-item-and-subline-item-criteria.md), and the billing duty for cost vouchers is on [Billing instructions for cost vouchers](ctf-dfars-billing-instructions-cost-vouchers.md).

## Worked example

Synthetic example. The numbers are made up.

A contract has line item 0001, a test stand priced at 100,000, and line item 0002, a spare tool kit marked NSP and priced inside 0001. The contractor delivers the test stand in March and the tool kit in April, and the Government accepts the tool kit in April.

Answer: the contractor may invoice 0001 only after the tool kit is delivered and accepted, so not in March.

## Never confuse

For a line or subline item identified as not separately priced, the unit price for the NSP line or subline item is included in the unit price of another, related line or subline item [@CL-CTF-1803]. How line and subline items are set up is on [DFARS line item and subline item criteria](ctf-dfars-line-item-and-subline-item-criteria.md).

## Company calibration

Which contracts carry NSP items and how the invoice timing they cause affects the cash forecast. [@Q-026]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-10-31: Cut meta wording, uncited negatives and contrasts; restated the invoice condition.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for payment for line or subline items not separately priced.
