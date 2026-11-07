+++
id = "ctf-wide-area-workflow-payment-instructions"
title = "Electronic submission of payment requests: Wide Area WorkFlow payment instructions"
area = "ctf"
subarea = "billing-payment"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-27
handling = ["public"]
terms = ["billing", "contract_financing", "defense_far_supplement"]
questions = ["Q-009"]
links = ["ctf-prompt-payment", "ctf-contract-types"]
+++

## Purpose

Shows how the clause directs a contractor to submit payment requests and receiving reports in Wide Area WorkFlow: which document type to use, how documents are routed and how they may be submitted.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CTF-0451] |
| Codified citation | DFARS 252.232-7006 [@CL-CTF-0451] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CTF-0451] |
| Applies to | The clause as prescribed in DFARS 232.7004(b) [@CL-CTF-0451] |

The clause is prescribed in DFARS 232.7004(b) [@CL-CTF-0451]. The Wide Area WorkFlow (WAWF) system provides the method to electronically process vendor payment requests and receiving reports, as authorized by Defense Federal Acquisition Regulation System (DFARS) 252.232-7003, Electronic Submission of Payment Requests and Receiving Reports [@CL-CTF-0452]. To access WAWF, the contractor shall (1) have a designated electronic business point of contact in the System for Award Management, and (2) be registered to use WAWF following the step-by-step procedures for self-registration available at its Web site [@CL-CTF-0561; @CL-CTF-2216]. Document submissions may be via Web entry, Electronic Data Interchange, or File Transfer Protocol [@CL-CTF-0453].

The contractor shall submit payment requests using the following document types: (i) for cost-type line items, including labor-hour or time-and-materials, submit a cost voucher [@CL-CTF-0454]. For fixed price line items (A) that require shipment of a deliverable, submit the invoice and receiving report specified by the Contracting Officer [@CL-CTF-0562; @CL-CTF-0720], and (B) for services that do not require shipment of a deliverable, submit either the Invoice 2in1, which meets the requirements for the invoice and receiving report, or the applicable invoice and receiving report, as specified by the Contracting Officer [@CL-CTF-0563]. For (iii) customary progress payments based on costs incurred, submit a progress payment request, for (iv) performance based payments, submit a performance based payment request, and for (v) commercial financing, submit a commercial financing request [@CL-CTF-0564]. Fast Pay requests are only permitted when FAR 52.213-1 is included in the contract [@CL-CTF-0455].

## How it works

The contractor shall use the information in the Routing Data Table only to fill in applicable fields in WAWF when creating payment requests and receiving reports in the system [@CL-CTF-0565]. Department of Defense Activity Address Code (DoDAAC) is a six position code that uniquely identifies a unit, activity, or organization [@CL-CTF-0560]. The contractor shall ensure a payment request includes documentation appropriate to the type of payment request in accordance with the payment clause, contract financing clause, or FAR 52.216-7, Allowable Cost and Payment, as applicable [@CL-CTF-0566]. The contractor shall ensure a receiving report meets the requirements of DFARS Appendix F [@CL-CTF-0567].

## Worked example

Synthetic example. The numbers are made up.

A contractor performs a cost-plus-fixed-fee contract and bills monthly. In June it incurs 150,000 in direct costs and 60,000 in indirect costs, plus a fee of 15,000, for a request of 225,000. The line item is cost-type, so the billing specialist creates a cost voucher in Wide Area WorkFlow, fills in the routing fields only from the contract's routing data table, and sends the 225,000 voucher by Web entry.

## Never confuse

Under DFARS 252.232-7006(f)(1), the contractor shall submit a cost voucher for cost-type line items (including labor-hour or time-and-materials), a progress payment request for customary progress payments based on costs incurred, a performance based payment request for performance based payments, and a commercial financing request for commercial financing [@CL-CTF-0454; @CL-CTF-0564]. Under DFARS 252.232-7006(f)(4), the contractor shall ensure a payment request includes documentation appropriate to the type of payment request in accordance with the payment clause, contract financing clause, or FAR 52.216-7, Allowable Cost and Payment, as applicable [@CL-CTF-0566]. For payment timing and contract types, see [Prompt payment](ctf-prompt-payment.md) and [Contract types](ctf-contract-types.md).

## Company calibration

Confirming who holds the electronic business point of contact in the System for Award Management, which routing data table applies to each contract, and how rejected submissions are handled. [@Q-009]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-11-06: Stated the conditions and limits the cited text carries, after a full-source check.
- COMPATIBLE_EXTENSION, 2026-10-31: Added companion claim for DFARS 252.232-7006(c)(2) self-registration procedures, included the Invoice 2in1 qualifier in (f)(1)(ii)(B), restored shall modality, and removed uncited lead-ins.
- COMPATIBLE_EXTENSION, 2026-10-31: Cited the fixed price line item lead-in for the services sentence.
- COMPATIBLE_EXTENSION, 2026-10-31: Removed a claim cited for the wrong line-item case and restored the must and contractor-ensures wording.
- COMPATIBLE_EXTENSION, 2026-10-29: Added the document types, routing, access and documentation rules, and removed statements about invoices, Fast Pay and payment terms that the clause does not make.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for Electronic submission of payment requests (Wide Area WorkFlow) clause.
