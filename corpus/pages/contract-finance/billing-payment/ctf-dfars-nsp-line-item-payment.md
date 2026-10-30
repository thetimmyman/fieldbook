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

Shows when a contractor may invoice an item whose price includes an item marked not separately priced, and what the clause leaves out.

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

The DFARS directs use of the clause as prescribed in DFARS 204.7109(a) [@CL-CTF-1831]. The printed text puts the NSP item's unit price inside the unit price of the related item, and it gives no separate price for the NSP item itself [@CL-CTF-1803]. The invoice limit is a limit on timing for the item that includes the NSP item in its price [@CL-CTF-1804].

## How it works

For a line or subline item identified as NSP, the unit price is included in the unit price of another, related line or subline item [@CL-CTF-1803]. An invoice for the item that includes the NSP item in its price waits until the Contractor has also delivered the NSP item and the Government has accepted it [@CL-CTF-1804]. Technical data is outside the clause [@CL-CTF-1805]. The line item rules are on [DFARS line item and subline item criteria](ctf-dfars-line-item-and-subline-item-criteria.md), and the billing duty for cost vouchers is on [Billing instructions for cost vouchers](ctf-dfars-billing-instructions-cost-vouchers.md).

## Worked example

Synthetic example. The numbers are made up.

A contract has line item 0001, a test stand priced at 100,000, and line item 0002, a spare tool kit marked NSP and priced inside 0001. The contractor delivers the test stand in March and the tool kit in April, and the Government accepts the tool kit in April.

Answer: the contractor may invoice 0001 only after the tool kit is delivered and accepted, so not in March.

## Never confuse

A not separately priced item is not a free item or an unpriced one: its unit price is included in the unit price of another, related item [@CL-CTF-1803]. NSP is also not a separate kind of item, because the clause applies it to a line or subline item that the schedule identifies as NSP [@CL-CTF-1803]; [DFARS line item and subline item criteria](ctf-dfars-line-item-and-subline-item-criteria.md) explains how line and subline items are set up.

## Company calibration

Which contracts carry NSP items and how the invoice timing they cause affects the cash forecast. [@Q-026]

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for payment for line or subline items not separately priced.
