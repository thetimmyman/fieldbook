# Citation checks

## Facts and evidence

Citation checking separates source existence, locator resolution and quotation matching. `FactVerdict` is `NotApplicable`, `Verified`, `Unchecked` or `Mismatch`. A quotation of at least six words directly before a citation uses the shared quote normalizer and matcher. This checks the quotation, not a paraphrase or the correctness of the answer.

`CiteFact` contains metadata only, including an optional `EvidenceRef { source_id, locator }` for a claim's source. `LocatorFacts.available` distinguishes missing extraction facts from a known empty extraction. A missing table entry does not prove that a row or cell exists.

The pure `EvidenceText` trait has `text(&self, source_id, locator) -> Option<&str>`. It returns the complete permitted text at that locator, or `None` when absent, inaccessible or incomplete. `check_facts(facts, evidence, answer, context)` uses this provider for matching and excerpts. `cite_check(view, answer, context)` supplies a provider over the filtered view; it performs no I/O. A citation to a claim uses its evidence reference; an explicit source locator uses that source and locator. No fallback reads the unrestricted model.

The browser provider uses already-exported permitted unit text and explicit locator coverage. It returns text only when the units cover the complete locator in order, without gaps or truncation. It fetches no file and exports no extra source text to fill gaps. Identical facts and evidence give identical Rust and browser reports. Missing browser coverage is Unchecked, not a match or a mismatch. Excerpts come only from this provider and are cut to 200 Unicode scalar characters.

## Status and exit

The first applicable status wins: broken, broken locator, outside context, superseded, weak, unchecked locator, unchecked quote, quote mismatch, verified. A question or conflict reference is neutral. `verified` requires every applicable fact to be checked and matched. No applicable quotation means quote is NotApplicable; a quotation needing unavailable text is Unchecked.

Count a citation once as unresolved if its id is broken or either applicable locator/quote fact is Unchecked or Mismatch, even when a higher-precedence display status applies. An outside-context, superseded or weak citation with valid facts does not itself change the exit code. Exit 1 when any citation is unresolved, or when nothing verifies and the answer is not solely abstentions. Source existence alone does not verify a row, cell, page or quotation.

## Shared cases

`tests/fixtures/handoff/citation-facts.json` holds ordinary, non-seed cases with metadata, evidence text/coverage, answer, optional context and expected fact fields/status/exit. Rust and browser run these same cases: valid and invalid known locators; unavailable page/table facts; a matching quotation; a mismatched quotation; a known locator with missing quote text; truncated browser coverage; and unavailable quotation text under a higher-precedence status. Existing seed files remain unchanged.
