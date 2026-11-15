+++
id = "est-cpif-fee-adjustment"
title = "Cost-plus-incentive-fee contracts: fee adjustment, minimum fee and maximum fee"
area = "est"
subarea = "pricing"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-27
handling = ["public"]
questions = ["Q-023"]
links = ["ctf-incentive-fee", "cls-cost-reimbursement-contracts", "est-profit-analysis", "ctf-contract-types"]
+++

## Purpose

Shows how the fee in a cost-plus-incentive-fee contract moves with cost against target, and how the minimum and maximum fee limit that movement.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified FAR and codified DFARS [@CL-EST-1090; @CL-EST-9221] |
| Codified citation | FAR 16.304, 16.405 and 16.405-1 [@CL-EST-1090; @CL-EST-1091; @CL-EST-2147]; DFARS 216.405-1 [@CL-EST-9221] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-EST-1090] |
| Applies to | Cost-plus-incentive-fee contracts, a type of cost-reimbursement contract [@CL-EST-1090] |

A cost-plus-incentive-fee contract is a cost-reimbursement contract that provides for an initially negotiated fee to be adjusted later by a formula based on the relationship of total allowable costs to total target costs [@CL-EST-1090]. It specifies a target cost, a target fee, minimum and maximum fees, and a fee adjustment formula [@CL-EST-1091].

The contract type is appropriate for services or development and test programs when a cost-reimbursement contract is necessary and a target cost and fee adjustment formula can be negotiated that are likely to motivate the contractor to manage effectively [@CL-EST-1094].

## How it works

Section 16.405, on cost-reimbursement incentive contracts, refers the reader to 16.301 for requirements applicable to all cost-reimbursement contracts, for use in conjunction with the subsections that follow it [@CL-EST-2147]. The DFARS section 216.405-1 says to see PGI 216.405-1 for guidance on the use of cost-plus-incentive-fee contracts [@CL-EST-9221].

The formula provides, within limits, for fee above target fee when total allowable costs are less than target costs, and fee below target fee when total allowable costs exceed target costs [@CL-EST-1092]. When total allowable cost is outside the range within which the formula operates, the contractor is paid total allowable costs plus the minimum or maximum fee [@CL-EST-1093]. If a high maximum fee is negotiated, the contract shall also provide for a low minimum fee, which may be a zero fee or, in rare cases, a negative fee [@CL-EST-1095]. Cost reimbursement and the billing of fee are covered on the close and contract-finance cards linked here.

## Worked example

Synthetic example. The numbers are made up.

A contract has a target cost of 1,000,000, a target fee of 80,000, a minimum fee of 40,000 and a maximum fee of 120,000. The invented formula moves fee by 10 cents for each dollar that total allowable cost differs from target. At a total allowable cost of 900,000 the fee is 90,000. At a total allowable cost of 1,500,000 the formula would give 30,000, which is below the minimum, so the fee is 40,000 and the contractor is paid 1,540,000.

## Never confuse

A cost-plus-incentive-fee contract reimburses allowable incurred costs and adjusts target fee by formula between negotiated minimum and maximum fee limits [@CL-EST-1090; @CL-EST-1091; @CL-CLS-0567], unlike a fixed-price incentive (firm target) contract, which specifies a price ceiling but not a profit ceiling or floor, the ceiling being the maximum that may be paid [@CL-EST-1076; @CL-EST-1077].

## Company calibration

How the target cost and the fee limits in a proposal are frozen and later compared with actuals. [@Q-023]

## Sources

## Change note
- COMPATIBLE_EXTENSION, 2026-11-15: Never confuse states a contrast with supporting citations.
- COMPATIBLE_EXTENSION, 2026-10-31: Never-confuse contrast restated from the quotes; added award-amount companion.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the DFARS pointer to PGI guidance on cost-plus-incentive-fee contracts.
- COMPATIBLE_EXTENSION, 2026-10-31: Added the pointer in section 16.405 to the general cost-reimbursement requirements.
- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for cost-plus-incentive-fee contracts.
