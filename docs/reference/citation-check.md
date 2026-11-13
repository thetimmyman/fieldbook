# Citation checks

## Facts and evidence

Citation checking separates source existence, locator resolution and quotation matching. `FactVerdict` is `NotApplicable`, `Verified`, `Unchecked` or `Mismatch`. A quotation of at least six words directly before a citation uses the shared quote normalizer and matcher. This checks the quotation, not a paraphrase or the correctness of the answer.

`CiteFact` contains metadata only, including an optional `EvidenceRef { source_id, locator }` for a claim's source. `LocatorFacts.available` distinguishes missing extraction facts from a known empty extraction. A missing table entry does not prove that a row or cell exists.

The pure `EvidenceText` trait has `text(&self, source_id, locator) -> Option<&str>`. It returns the complete permitted text at that locator, or `None` when absent, inaccessible or incomplete. `check_facts(facts, evidence, answer, context)` uses this provider for matching and excerpts. `cite_check(view, answer, context)` supplies a provider over the filtered view; it performs no I/O. A citation to a claim uses its evidence reference; an explicit source locator uses that source and locator. No fallback reads the unrestricted model.

A source citation with no locator has locator NotApplicable; with no applicable quotation, a citation of an existing version that no earlier status catches is verified. A claim citation takes its locator fact from its claim's evidence reference, checked against that version's extraction facts as an explicit source locator is; a claim with no evidence reference has locator NotApplicable. A quotation is the text between a pair of double quotation marks, straight or curly. It is directly before a citation when nothing but white space separates its closing mark from the citation's opening bracket, and its words are the runs of non-white-space characters after the shared normalizer.

The browser provider uses already-exported permitted unit text and explicit locator coverage. It returns text only when the units cover the complete locator in order, without gaps or truncation. It fetches no file and exports no extra source text to fill gaps. Identical facts and evidence give identical Rust and browser reports. Missing browser coverage is Unchecked, not a match or a mismatch. Excerpts come only from this provider and are cut to 200 Unicode scalar characters.

## Status and exit

The first applicable status wins: broken, broken locator, outside context, superseded, weak, unchecked locator, unchecked quote, quote mismatch, verified. A question or conflict reference is neutral. `verified` requires every applicable fact to be checked and matched. No applicable quotation means quote is NotApplicable; a quotation needing unavailable text is Unchecked. A citation's primary `code` comes from that precedence verdict (FB0701 broken, FB0702 broken locator, FB0703 superseded, FB0704 weak). Noncanonical form is a separate fact with its own FB0706 warning: it is carried beside a primary code rather than replacing it, and the text printer and the browser show both.

Count a citation once as unresolved if its id is broken or either applicable locator/quote fact is Unchecked or Mismatch, even when a higher-precedence display status applies. An outside-context, superseded or weak citation with valid facts does not itself change the exit code. Exit 1 when any citation is unresolved, or when nothing verifies and the answer is not solely abstentions. Source existence alone does not verify a row, cell, page or quotation. A sentence whose only citations are question or conflict references is not uncited: they are citations, shown and linked, though never counted as evidence. The JSON `verdict` is `refused` when nothing verifies and the answer is not solely abstentions, otherwise `unresolved` when any citation is unresolved, otherwise `ok`.

## Shared cases

`tests/fixtures/handoff/citation-facts.json` holds ordinary, non-seed cases with metadata, evidence text/coverage, answer, optional context and expected fact fields/status/exit. Rust and browser run these same cases: valid and invalid known locators; unavailable page/table facts; a matching quotation; a mismatched quotation; a known locator with missing quote text; truncated browser coverage; unavailable quotation text under a higher-precedence status; and a broken noncanonical citation whose line carries primary `code` FB0701 together with `non_canonical = true` and displays both FB0701 and the FB0706 warning. Existing seed files remain unchanged.
