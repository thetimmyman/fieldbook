+++
id = "ctf-prompt-payment-due-dates"
title = "Prompt payment: determining invoice payment due dates"
area = "ctf"
subarea = "billing-payment"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["billing", "cash_receipt"]
questions = ["Q-026"]
links = ["ctf-prompt-payment", "ctf-proper-invoice-content", "ctf-prompt-payment-interest-and-clauses"]
+++

## Purpose

Names the periods that fix an invoice payment due date and what starts each one, so the cash forecast can place a receipt. This card covers the general invoice due date, the cost-reimbursement interim rule, construction progress payments and retained amounts, and mixed invoices only. The Prompt Payment clause itself is on the linked card.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified FAR [@CL-CTF-1337] |
| Codified citation | FAR 32.904 [@CL-CTF-1337] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CTF-1337] |
| Applies to | The general invoice payment due date; the section prescribes separate due dates for other cases, and this card covers only some of them [@CL-CTF-1337] |

Except as prescribed in paragraphs (c) through (f) of the section, or as authorized in the clause-prescription section, the due date for making an invoice payment is the later of two listed events [@CL-CTF-1337]. The two events are the 30th day after the designated billing office receives a proper invoice, except as provided in paragraph (b)(3), and the 30th day after Government acceptance of supplies delivered or services performed [@CL-CTF-1338]. Agency policies and procedures may authorize amendment of the clause to insert a period shorter than 30 days, but not less than 7 days, for making contract invoice payments [@CL-CTF-1391].

For the purpose of computing late payment interest penalties that may apply, the due date for making interim payments on cost-reimbursement contracts for services is 30 days after the date of receipt of a proper invoice [@CL-CTF-1341].

Construction contracts have their own due dates [@CL-CTF-1394]. Among them, the due date for progress payments based on contracting officer approval of the estimated amount and value of work or services performed, including payments for reaching milestones in any project, is 14 days after the designated billing office receives a proper payment request [@CL-CTF-1394; @CL-CTF-1382]. The due date for payment of any amounts retained under the clause at 52.232-5 is as specified in the contract or, if not specified, 30 days after approval by the contracting officer for release to the contractor [@CL-CTF-1394; @CL-CTF-1383].

When an invoice contains items with different payment due dates, the payment office will, subject to agency policy, either pay the entire invoice on the earliest due date or split invoice payments and pay by the applicable due dates [@CL-CTF-1384].

This card does not state the due dates for architect-engineer contracts, for final payments on construction contracts, for contracts that do not require an invoice, or for food and specified items. Do not use the general rule above for those cases.

## How it works

Receipt of a proper invoice starts the first period, and Government acceptance starts the second [@CL-CTF-1338]. A listed rule applies for the sole purpose of computing an interest penalty that might be due the contractor [@CL-CTF-1339]. Under it, Government acceptance is deemed to occur constructively on the 7th day after the contractor delivers supplies or performs services in accordance with the contract, unless there is a disagreement over quantity, quality, or contractor compliance with a contract requirement [@CL-CTF-1339; @CL-CTF-1340]. If actual acceptance occurs within the constructive acceptance period, the Government must base the determination of an interest penalty on the actual date of acceptance [@CL-CTF-1392]. The section also has rules on a final invoice subject to settlement, longer constructive periods and missing receipt dates; this card does not state them. See [Prompt payment for invoices](ctf-prompt-payment.md) for the clause, [Proper invoice content](ctf-proper-invoice-content.md) for what makes an invoice proper and [Prompt payment interest and clauses](ctf-prompt-payment-interest-and-clauses.md) for when an interest penalty is owed.

## Worked example

Synthetic example. The numbers are made up.

A contractor delivers supplies on day 1, and a proper invoice is received on day 2. The Government has not yet accepted the supplies. For interest purposes only, acceptance is deemed on day 8.

Answer: the later of day 32 and day 38 is day 38, so a payment made after day 38 is late for interest purposes. If the Government disagreed with the contractor on quantity, the deemed acceptance on day 8 would not apply. If the Government actually accepted on day 5, the actual date would be used instead.

## Never confuse

Constructive acceptance is not actual acceptance: the rule is stated for the sole purpose of computing an interest penalty, and actual acceptance within the constructive period replaces it [@CL-CTF-1339; @CL-CTF-1340; @CL-CTF-1392]. The general due date is the later of two events, each measured as the 30th day after the event [@CL-CTF-1338], while for computing late payment interest, interim payments on cost-reimbursement contracts for services are due 30 days after receipt of a proper invoice [@CL-CTF-1341].

## Company calibration

Confirm which acceptance, receipt and invoicing terms drive the cash forecast and who tracks them. [@Q-026]

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for prompt payment due dates.
