# source-location-links

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

Each `s/` page shows its version's official `location` as a link, and each section locator on it as a link to that section at the official publisher, for the publishers a rule table in `source.rs` names.

## Why

- The need. 28,234 of 28,857 claims (98%) cite eCFR HTML originals, which ship as raw markup with a `.txt` name ([intake README](../intake/README.md), the HTML originals paragraph), and every `s/` page says "Excerpt is not available until this version is extracted" until `extract-and-locators` and `quote-match-and-anchors` merge behind the release gate. Today the reader of `s/S-0003-v1.html` sees 79 § locators as plain `<span class="loc">` text and the register's `location` as plain text, with no way to read the section itself except by copying an address.
- This is against a settled rule. [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) (Links) and decision D-20 in ADR 0001 say external URLs are text with a copy button, "a lint fails any `href=\"http`" (`FB0604`, `crates/fieldbook-render/src/lint.rs`), and `HANDOFF.md` section 11 (External URL) says "never a link". NFR-S-12 promises an intake pack stating the edition holds no external links, and [docs/intake/network-silence.md](../intake/network-silence.md) tells IT that the edition makes no outbound connection. A link does not make the page connect, but a click sends the laptop's browser to the publisher, and IT reads these statements. Changing D-20 is an ADR amendment, and any change to the IT-facing statements is the release gate, both the owner's ([AGENTS.md](../../AGENTS.md#authority)); hence `Review: owner`.
- The hosted CSP (`deploy/_headers`, equal to `CSP_STATIC` plus `frame-ancestors 'none'`) sets no navigation directive, so it neither blocks nor needs changing for a link; [ADR 0007](../decisions/0007-hosted-edition.md) keeps the hosted pages byte-identical to every other build of the commit, so a link cannot be hosted-only.
- If the owner keeps D-20, the same need is met within the rule: the location and each section's address as text with a copy button. The card is then rewritten to that outcome before any step runs.
- Design: `MAP.md` row Source drawer (`LocatorView`); `HANDOFF.md` section 5 (`LocatorView`) and section 11 (External URL), as the revision that follows the owner's decision words them.

## Scope

- Files: `crates/fieldbook-render/src/source.rs`, `crates/fieldbook-render/templates/source.html`, `crates/fieldbook-render/tests/source.rs`, `crates/fieldbook-render/src/lint.rs`, `crates/fieldbook-render/tests/site_lints.rs`, `docs/decisions/0001-foundation.md` (the AD-12 Links paragraph and the D-20 row, as the owner amends them), `CHANGELOG.md`.
- Registration lines: `docs/reference/corpus-format.md`, the Content security bullet; `crates/fieldbook-core/src/check/code.rs`, the `FB0604` text, with `docs/reference/errors.md` regenerated.
- Docs to update: those two; `CHANGELOG.md` (`- Added: source pages link to the official location and to each cited section.`).

## Out of scope

- `SECURITY.md` and `docs/intake/`: the owner's own edit, if the decision needs one.
- Links anywhere but `s/` pages; links in the Corpus text (`FB0135` stays).
- The natural order of § locators (31.205-1, 31.205-2, … 31.205-10, today sorted as text) and extraction itself (`extract-and-locators`).

## Preconditions

- The owner's decision on D-20 for `s/` pages, and whether the IT-facing statements need a line, recorded on this card's work item or pull request.
- A design revision after that decision that replaces the External URL row for `s/` pages.
- Cards merged: `design-r22-reader-pages` (that revision follows it), `quote-match-and-anchors` (it changes `source.rs`, `source.html` and `tests/source.rs`).

## Steps

1. Stop here until both preconditions above are met (see BLOCKED).
2. Amend the AD-12 Links paragraph and D-20 in ADR 0001 as the owner decided, and narrow `FB0604` to an `https:` link with the attributes the revision names, on an `s/` page only, to a host in the rule table; every other `href="http` still fails.
3. In `source.rs`, a rule table: for each publisher, the host and how a section locator becomes a section address (the eCFR first). A version whose `location` is on no listed host, or is `metadata-only: `, shows text as today.
4. In `source.html`, render the location link and each locator link with the locator text beside it, as the revision says.
5. Tests in `tests/source.rs`: `ecfr_location_is_a_link`, `ecfr_section_locator_links_to_its_section`, `unlisted_host_stays_text`, `metadata_only_location_stays_text`; in `tests/site_lints.rs`, `fb0604_still_fails_outside_source_pages`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test source
cargo nextest run --locked -p fieldbook-render --test site_lints
just check
```

Expected: the tests pass; `just check` exits 0.

## Negative control

Put an `https:` link on a `p/` page of a fixture build: `fb0604_still_fails_outside_source_pages` must fail with `FB0604`. Restore it. Remove the host check from the rule: `unlisted_host_stays_text` must fail. Restore it.

## Rollback

Revert the merge commit, including the ADR text. `s/` pages show text again and `FB0604` fails every external link.

## BLOCKED

Until the owner decides D-20 and the IT-facing statements: comment `BLOCKED: source-location-links needs the owner's decision on D-20 and the intake statements`, push nothing and take the next card. If the decision needs an edit of `SECURITY.md` or `docs/intake/`, the card stays blocked until the owner has made it.
