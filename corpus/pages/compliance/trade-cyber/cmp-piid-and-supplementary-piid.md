+++
id = "cmp-piid-and-supplementary-piid"
title = "Procurement instrument identifiers and supplementary identifiers"
area = "cmp"
subarea = "trade-cyber"
type = "clause-card"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2026-10-29
handling = ["public"]
questions = ["Q-014"]
links = ["mkt-fpds-reporting-responsibilities-and-deadlines"]
+++

## Purpose

Shows how a procurement instrument identifier is built, how a supplementary identifier is built, and which actions each identifies.

## The rule

| Field | Value |
|---|---|
| Instrument | Codified FAR [@CL-CMP-2986] |
| Codified citation | FAR 4.1601 [@CL-CMP-2986]; FAR 4.1603 [@CL-CMP-2987] |
| RFO citation | Not established. |
| Edition date | Not established. |
| Text status | Final [@CL-CMP-2986] |
| Applies to | Solicitation and contract actions of agencies [@CL-CMP-2986] |

The PIID shall be used to identify all solicitation and contract actions [@CL-CMP-2986]. Agencies shall have in place a process that ensures that each PIID used to identify a solicitation or contract action is unique Governmentwide, and will remain so for at least 20 years from the date of contract award [@CL-CMP-2946]. The PIID consists of a combination of thirteen to seventeen alpha and/or numeric characters, and special characters such as hyphens, dashes, or spaces are not used [@CL-CMP-2987].

These listed elements sit under that description of the PIID [@CL-CMP-2987]. The first six positions identify the department/agency and office issuing the instrument, using the AAC assigned to the issuing office [@CL-CMP-2988]. The seventh and eighth positions are the last two digits of the fiscal year in which the instrument is issued or awarded, measured by the date the action is signed rather than the effective date if different [@CL-CMP-2989]. Position nine indicates the type of instrument by one of the listed upper case letters [@CL-CMP-2990].

## How it works

A supplementary PIID is numbered in addition to the PIID for amendments to solicitations and for modifications [@CL-CMP-2991; @CL-CMP-2992]. Amendments to solicitations are numbered sequentially using a four position numeric serial number, beginning with 0001, in addition to the 13-17 character PIID [@CL-CMP-2991]. Modifications to contracts, orders, and agreements are numbered using a six position alpha or numeric, or a combination thereof, in addition to the 13-17 character PIID [@CL-CMP-2992]. This card does not set out the later positions of the PIID, the letter list or the issuing office letter for modifications. Reporting of contract actions is on [FPDS reporting responsibilities and deadlines](../../market/feeds/mkt-fpds-reporting-responsibilities-and-deadlines.md).

## Worked example

Synthetic example. The numbers are made up.

An office with the code AB1234 signs a contract in fiscal year 2027. Positions one to six are AB1234, positions seven and eight are 27, position nine is the contract letter C, and the agency number 4217 fills the rest. The PIID is AB123427C4217, which has thirteen characters. Its first modification could be numbered P00001.

## Never confuse

The PIID identifies the solicitation or contract action [@CL-CMP-2986]. A supplementary PIID is numbered in addition to the PIID, for an amendment to a solicitation or for a modification [@CL-CMP-2991; @CL-CMP-2992].

## Company calibration

How the company's contract records store the PIID and each supplementary number for every award and modification, and who maintains them. [@Q-014]

## Sources

## Change note

- NEW_KNOWLEDGE, 2026-10-29: Drafted the clause card for procurement instrument identifiers.
