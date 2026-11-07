# Changelog

Notes on tool releases (`vX.Y.Z`), newest first. Editions are noted in [corpus/changes.md](corpus/changes.md). The release steps are in [the release runbook](docs/runbooks/release.md).

## Unreleased

- Added: the home page and the area pages.
- Changed: search lists pages first, one row per page, and `--units` keeps the flat list.
- Added: source pages and the originals folder.
- Fixed: the permitted view keeps a source original by its version id, whatever its location says.
- Added: the search command.
- Added: the five register pages and the id list.
- Added: the new command for pages, claims, questions, conflicts and terms.
- Added: add accepts a folder and prints a per-file report.
- Added: briefs, checklists with their timeline, and the other page types.
- Added: page files in the frozen part order with Sources from citations.
- Added: the page frame with section tabs, sidebar and theme toggle.
- Added: the build command.
- Added: the add command.
- Added: the audit journal, audit verify and its repair mode.
- Added: check reports marking, identity, claim-language and formula problems and non-public labels (FB0400 to FB0405, FB0410).
- Added: check reports the citation and review-date rules (FB0117, FB0118, FB0309, FB0310 and FB0320).
- Added: check reads every citation in a page body and reports unreadable, uncanonical, touching and unresolved citations, and locators on claim keys (FB0300 to FB0303 and FB0305).
- Added: check reports required content by page type, raw HTML, images, outside links, long briefs and terms defined on a page (FB0134, FB0135, FB0138, FB0139).
- Added: check reports part headings, blank parts, the Sources and Worked example parts and the Change note (FB0130 to FB0133, FB0137).
- Added: check reports unknown page, term, question and conflict ids, prerequisite cycles, page supersession and broken page links (FB0110 to FB0113, FB0115, FB0136).
- Added: check reports page front matter, ids, areas, types, computed fields and dates (FB0101 to FB0109, FB0114, FB0116).
- Added: check reports broken promotion rules, claim dates, claims on superseded versions and incomplete day-one questions (FB0209, FB0213, FB0220 to FB0222, FB0230).
- Added: check reports broken register references, supersession, original hashes and repeated glossary labels (FB0207, FB0208, FB0211, FB0214).
- Added: check reports a consequential claim without a fine locator or a quote (FB0306, FB0307).
- Added: check reports register cells and the rules across columns (FB0114, FB0204 to FB0206, FB0210).
- Added: check reports register files, CSV syntax, encoding and keys (FB0200 to FB0203).
- Added: the check command (`FB0120`–`FB0125`).
