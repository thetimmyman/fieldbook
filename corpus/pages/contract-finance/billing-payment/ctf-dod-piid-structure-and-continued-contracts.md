+++
id = "ctf-dod-piid-structure-and-continued-contracts"
title = "DoD procurement instrument identifiers and continued contracts"
area = "ctf"
subarea = "billing-payment"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
terms = ["defense_far_supplement"]
questions = ["Q-009"]
links = ["cmp-piid-and-supplementary-piid", "ctf-dfars-modification-line-items-and-order-of-application", "ctf-dfars-contract-closeout-and-file-disposal"]
+++

## Purpose

Shows how a DoD procurement instrument identifier is built and kept unique, and what happens to payment when the identifier is changed after award.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified DFARS [@CL-CTF-1607] |
| Codified citation | DFARS 204.1601 [@CL-CTF-1606]; DFARS 204.1603 [@CL-CTF-1607] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CTF-1607] |
| Applies to | DoD-issued PIIDs [@CL-CTF-1607] |

The DFARS says not to reuse a PIID once it has been assigned and not to assign the same PIID to more than one task or delivery order, even if they are issued under different base contracts or agreements [@CL-CTF-1606]. DoD-issued PIIDs are thirteen characters in length, use only alpha-numeric characters, and do not use the letter I or O in any part of the PIID [@CL-CTF-1607].

DoD-issued PIIDs shall only use positions 10 through 13 to complete the PIID, and the serial number of the instrument is entered in those positions [@CL-CTF-1637]. DoD does not use other letters identified in the FAR as reserved for future Federal Governmentwide use or reserved for departmental or agency use in position 9 of the PIID [@CL-CTF-1638]. The DFARS also says not to use the letter C or H for contracts or agreements with provisions for orders or calls [@CL-CTF-1639]. The same section assigns further letters in the ninth position and further rules for particular positions, which this card does not set out.

For modifications, DoD-issued supplementary PIIDs shall, for positions 2 through 6, comply with the rules the DFARS lists [@CL-CTF-1641]. One of them says to use Z only in position 2, and only to identify a modification which definitizes a letter contract or a previously issued undefinitized modification [@CL-CTF-1640]. This card does not set out the other supplementary rules. The FAR rules that the DFARS builds on are on [Procurement instrument identifiers and supplementary identifiers](../../compliance/trade-cyber/cmp-piid-and-supplementary-piid.md).

When a PIID is changed after contract award, the new PIID is known as a continued contract, which does not constitute a new procurement [@CL-CTF-1608]. A continued contract shall not be used to evade competition requirements, expand the scope of work, or extend the period of performance beyond that of the predecessor contract [@CL-CTF-1608; @CL-CTF-1635]. When issuing a continued contract, the contracting officer shall issue an administrative modification to the predecessor contract to clearly state the listed matters [@CL-CTF-1609]. One listed matter is that any future awards provided for under the terms of the predecessor contract, for example issuance of orders or exercise of options, will be accomplished under the continued contract [@CL-CTF-1609; @CL-CTF-1636]. Another is that supplies and services already acquired under the predecessor contract remain solely under that contract for purposes of Government inspection, acceptance, payment, and closeout [@CL-CTF-1609; @CL-CTF-1610].

## How it works

An identifier is assigned once and is not reused [@CL-CTF-1606]. If the identifier changes after award, the result is a continued contract rather than a new procurement [@CL-CTF-1608]. The administrative modification to the predecessor contract states the listed matters [@CL-CTF-1609]. Future awards under the predecessor contract's terms are accomplished under the continued contract [@CL-CTF-1636], and what was already acquired stays under the predecessor contract for inspection, acceptance, payment and closeout [@CL-CTF-1610].

## Worked example

Synthetic example. The numbers are made up.

A contract with one identifier is replaced after award by a continued contract with a new identifier. Forty units were already delivered and accepted under the predecessor contract. Sixty more units are to be ordered later by exercising an option in the predecessor contract.

The answer: the 40 delivered units stay under the predecessor contract for acceptance, payment and closeout, as the administrative modification states. The option exercise is a future award provided for under the predecessor contract's terms, so it is accomplished under the continued contract, as the administrative modification states.

## Never confuse

A continued contract is not a new procurement [@CL-CTF-1608]. It is also not the same as a supplementary PIID, which applies to modifications to contracts and agreements [@CL-CTF-1641] and is covered on the FAR identifier card linked above. The payment treatment of already acquired items follows the predecessor contract [@CL-CTF-1610]; how modifications are sequenced is on [Line items in DFARS contract modifications and the order modifications apply](ctf-dfars-modification-line-items-and-order-of-application.md), and closeout is on [DFARS contract closeout and disposal of contract files](ctf-dfars-contract-closeout-and-file-disposal.md).

## Company calibration

How the company's contract and billing systems handle a changed identifier, so that invoices stay with the contract that holds the delivery. [@Q-009]

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for DoD procurement instrument identifiers.
