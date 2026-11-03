+++
id = "ctf-prompt-payment"
title = "Prompt payment for invoices"
area = "ctf"
subarea = "billing-payment"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["billing", "contract_financing", "cash_receipt"]
questions = ["Q-009"]
links = ["ctf-progress-payments"]
+++

## Purpose

Explains when an invoice payment is due, when a late payment interest penalty arises, and why that treatment does not extend to contract financing.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified FAR [@CL-CTF-0129] |
| Codified citation | FAR 52.232-25 [@CL-CTF-0129] |
| RFO citation | RFO FAR 52.232-25 [@CL-CTF-0642; @CL-CTF-0643; @CL-CTF-0644; @CL-CTF-0645; @CL-CTF-0646; @CL-CTF-0647; @CL-CTF-0648; @CL-CTF-0649; @CL-CTF-0650; @CL-CTF-0651; @CL-CTF-0652; @CL-CTF-9132] |
| Edition date | Not established. |
| Text status | Final [@CL-CTF-0129] |
| Applies to | Contracts in which the Prompt Payment clause is inserted as prescribed in FAR 32.908(c) [@CL-CTF-0129] |

Except as indicated in paragraph (a)(2), which covers certain food products and other payments [@CL-CTF-9634], and in paragraph (c), which covers fast payment procedure contracts [@CL-CTF-0135], invoice payment is due on the later of two events [@CL-CTF-0130], each measured as the 30th day after the event: receipt of a proper invoice from the contractor by the designated billing office (except as provided for a missing receipt annotation), or Government acceptance of supplies delivered or services performed [@CL-CTF-9633].

Where the billing office fails to annotate the invoice with the actual receipt date, the due date is the 30th day after the contractor's invoice date [@CL-CTF-0485], provided the billing office receives a proper invoice and there is no disagreement over quantity, quality, or contractor compliance with contract requirements [@CL-CTF-0486]. An interest penalty is paid automatically, without a contractor request, when payment is not made by the due date and the listed conditions are met, if applicable [@CL-CTF-0131]. When the due date falls on a Saturday, Sunday or legal holiday, the designated payment office may make payment on the following working day without incurring a late payment interest penalty [@CL-CTF-0484].

## How it works

All days referred to in the clause are calendar days unless otherwise specified [@CL-CTF-0133]. The designated billing office returns an invoice that does not comply with the required content within 7 days after receipt, with the reasons why it is not a proper invoice, and shorter return periods apply to meat, meat food products, or fish and to perishable agricultural commodities, dairy products, edible fats or oils, and related food products [@CL-CTF-0132; @CL-CTF-0279].

The designated payment office pays an additional penalty only if three listed conditions are all met [@CL-CTF-9525]: the Government owes an interest penalty of $1 or more [@CL-CTF-9523], the office does not pay that interest penalty within 10 days after the date the invoice amount is paid [@CL-CTF-9524], and the contractor makes a written demand postmarked not later than 40 days after the invoice amount is paid [@CL-CTF-0134]. Where the contract carries the fast payment procedure clause, payment is due within 15 days after the invoice is received [@CL-CTF-0135].

Under the RFO text, DoD contracting officers use the RFO FAR part 52 text in place of the codified text from February 1, 2026 [@CL-CMP-9760] [@CL-CMP-9762]. The RFO prescription line for FAR 52.232-25 still names FAR 32.908(c) [@CL-CTF-0642; @CL-CTF-0129]. RFO FAR 52.232-25 keeps the rule that, except as indicated in paragraphs (a)(2) and (c) of the clause, invoice payment is due on the later of the 30th day after the designated billing office receives a proper invoice, subject to the exception provided in paragraph (a)(1)(ii) of the clause, and the 30th day after Government acceptance [@CL-CTF-0643; @CL-CTF-9132; @CL-CTF-0644; @CL-CTF-0130]. It keeps the fallback to the 30th day after the date of the contractor's invoice when the billing office does not annotate the actual date of receipt, provided the office receives a proper invoice and there is no disagreement over quantity, quality, or contractor compliance with contract requirements [@CL-CTF-0645; @CL-CTF-0646; @CL-CTF-0485; @CL-CTF-0486]. It keeps the automatic interest penalty when payment is not made by the due date and the listed conditions are met, if applicable [@CL-CTF-0647; @CL-CTF-0131], and the rule that, when the due date falls on a Saturday, Sunday, or legal holiday, the designated payment office may make payment on the following working day without incurring a late payment interest penalty [@CL-CTF-0648; @CL-CTF-0484]. It keeps the 7-day return of a non-complying invoice, with the shorter periods for the listed food products [@CL-CTF-0649; @CL-CTF-0650; @CL-CTF-0132; @CL-CTF-0279]. It keeps the written-demand condition for the additional penalty, postmarked not later than 40 days after the invoice amount is paid [@CL-CTF-0651; @CL-CTF-0134], and the fast payment procedure rule that, if the contract contains the clause at 52.213-1, Fast Payment Procedure, payments are made within 15 days after the date of receipt of the invoice [@CL-CTF-0652; @CL-CTF-0135].

## Worked example

Synthetic example. The numbers are made up.

A proper invoice for 60,000 is received on the first day of a month and no acceptance question arises, so payment is due on the 30th day after receipt, which is day 31 counting the day of receipt as day 1. Payment is made on day 41, and an interest penalty for the 10 late days is computed automatically and paid without any request.

## Never confuse

An invoice payment carries an automatic interest penalty when it is not paid by the due date and the listed conditions are met, while contract financing payments are not subject to the interest penalty provisions of the Prompt Payment Act [@CL-CTF-0131; @CL-CTF-0136].

Interim payments made before final payment under the allowable cost and payment clause are contract financing payments, except interim payments where the contract contains Alternate I to the Prompt Payment clause [@CL-CTF-0488]. Except as provided in the Termination for Convenience of the Government clause, all progress payments are liquidated by deducting from any payment under the contract, other than advance or progress payments, the unliquidated progress payments, or 80 percent of the amount invoiced, whichever is less [@CL-CTF-0104; @CL-CTF-0491]; see [Progress payments](../financing/ctf-progress-payments.md).

## Company calibration

Confirm the billing-to-collection workflow, who tracks invoice due dates, and who resolves rejected or overdue invoices. [@Q-009]

## Sources

## Change note

- COMPATIBLE_EXTENSION, 2026-10-31: Carried the exceptions to the invoice due date (new companions CL-CTF-9633 and CL-CTF-9634), the may wording for holiday payment, the billing-office actor on invoice returns and the liquidation terms of progress payments.
- COMPATIBLE_EXTENSION, 2026-10-31: Cited the text of each additional penalty condition; cited the lead-in of those conditions.
- COMPATIBLE_EXTENSION, 2026-10-31: Corrected the day count in the worked example.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the RFO FAR part 52 citation and how DoD's class deviation applies it.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for prompt payment for invoices.
