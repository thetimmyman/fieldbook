# Testing

How Fieldbook is tested. This file says where a test lives, when each check runs, how a control is proved and how test data is made. It links to the rules and does not repeat them. It is read by the owner and by Codex.

## Where tests live

- Unit tests sit beside the code in a `#[cfg(test)]` module.
- Integration tests are `crates/<crate>/tests/*.rs`, with their helpers in `crates/<crate>/tests/support/` (a `support/mod.rs`). A helper that more than one crate needs lives in the repository's `tests/support/`, and each test file includes it with `#[path]`. A crate that is not a row of `SHIPPED` in `xtask/src/graph.rs` fails the graph test, so helpers live in the crate that uses them or in `tests/support/`.
- The shared chip case table is `tests/support/chip_cases.rs`, one pure, non-shipped file keyed by the exact chip string of `strings::chips` before substitution together with its execution context (`Home` or `Laptop`). It holds each case's placeholder samples, fixture, setup and expected outcome; `crates/fieldbook/tests/chips.rs`, `xtask/src/drill.rs` and, once it exists, `crates/fieldbook/tests/drill5.rs` include it with `#[path]`. Parsing the complete chip with the real `Cli` decides whether a case exists in a build; the exact-string `PENDING` test stays the authority for what is not built, and the table is not a second verb or feature registry. A bad substituted sample is a setup failure, never a pending chip.
- Fixtures are in `tests/fixtures/`: the synthetic Corpus `corpus-mini`, the synthetic store `company-mini` and the `state-*` folders.
- Golden vectors are in `tests/golden/`; see [the golden README](../../tests/golden/README.md).
- Known-bad files are in `tests/known-bad/`, one example for each check code ([Known-bad files](corpus-format.md#known-bad-files)).

## When each runs

| When | What runs | Defined in |
|---|---|---|
| Every pull request | `just check`: formatting, clippy and nextest for the workspace and for each feature set, doc tests, `cargo deny` and the docs checks. The `windows` job builds and tests on Windows. The pull request that closes a card also runs `cargo xtask scope`, which fails a changed file that the card's Files and Registration lines do not list, under the card scope rule of [Writing a card](../cards/README.md#writing-a-card) | AD-07 and AD-08 in [ADR 0001](../decisions/0001-foundation.md); `.github/workflows/ci.yml` |
| Every pull request, in the `corpus` job | The edition is built with the release binary that the `rust` job uploads, then `cargo xtask harness` opens every page and every renderable original in `--jobs N` browser processes at once (default: half the available cores, at least 1 and at most 8), each with its own profile folder and net-log. The findings print in page order and the summary line counts are the same for any `N`; `--jobs 1` opens one page at a time. The `rust` job also scans the release binary's imports. | AD-08 in [ADR 0001](../decisions/0001-foundation.md); `.github/workflows/ci.yml`; `xtask/src/harness.rs` |
| Weekly | `scheduled.yml`: advisories, the secrets scan, `lint-controls`, the lock, the edition build and `cargo xtask scale`, which is advisory and fails nothing; once card `mutation-report` has merged, also `cargo xtask mutants --gate`; once `r2-binskim-fuzz-pe` has merged, also `cargo xtask release-evidence --fuzz-scheduled` on the citation and Bridge parsers. The `#[ignore]` tests that need the network or an outside tool run here with `--run-ignored only` where `scheduled.yml` names them: `advisories_fixture_fails_deny`, `advisories_fixture_git_source_fails_deny`, `gitleaks_reports_a_planted_token` and `lint_bans_fail_clippy`. The other `#[ignore]` tests (the poppler tests of `extract::` and `stage::`, `scale_runs_end_to_end` and `record_sums`) are run at home when their card says so, and no weekly job runs them | [Control register](../controls.md); card `scheduled-workflow` |
| Day 30 | Property tests (`proptest`) for `fieldbook-calc` (`props_decimal.rs`, `props_ops.rs`), the tokenizer (`props_tokenize.rs`) and the Bridge `read_request` | [dependencies.md](dependencies.md); [the plan](../plan/PLAN.md) |
| P5 and Day 90 | `cargo-mutants` through `cargo xtask mutants` (card `mutation-report`): a gate, with no surviving mutant outside the owner-reviewed skip file, on the label, policy, egress and authorisation modules of `fieldbook-core` (P5; it runs on a pull request that changes them and weekly; a run on a release tag is an owner decision, card `mutation-report`, out of scope), and a report on `fieldbook-calc` that fails nothing (Day 90) | [dependencies.md](dependencies.md); [the plan](../plan/PLAN.md) |

## Controls

A row of the [control register](../controls.md) marked with a dagger has a negative control: a test that plants the fault the control exists to catch and expects the control to fail. The test carries `// control: <ID>` within three lines above its `fn`. A known-bad case carries `control = "<ID>"` in `tests/known-bad/cases.toml` instead. The docs-checks lint `controls_have_tests` fails a dagger row of a merged card that has neither. The weekly `lint-controls` job runs `lint_bans_fail_clippy`, `advisories_fixture_git_source_fails_deny` and `gitleaks_reports_a_planted_token`.

## Test data

- Seeded random input comes from `tests/support/rng.rs`, a SplitMix64 generator with `Rng::new(seed)`, `next_u64()` and `bytes(n)`. No test uses the `rand` crate directly, and no test writes its own generator (`one_rng_exists` checks). The `proptest` properties draw their cases from `proptest`'s own ChaCha generator, which brings `rand` into the test builds of `fieldbook-calc` and `fieldbook-core`; each property seeds it with `Rng::new(1).next_u64()`.
- Every parser has a test that feeds 200 seeded byte strings and single-byte mutations of the card's valid example, and requires a typed error or a value and never a panic. `proptest` adds properties beside them at Day 30; the seeded tests stay.
- A fixture run of the program takes its date from a file and never from the clock: `tests/fixtures/corpus-mini/TODAY` for the Corpus fixtures and `tests/fixtures/COMPANY_TODAY` for the company-store fixtures.
- A `state-*` fixture holds the journal events that `support::state::build_state(dir, events)` writes, and any typed file that its owning writer produces (the question log, a hand-added file of a card, a policy the card builds): the card names the writer. Each has one `<name>_matches_the_writer` test that rebuilds the fixture in a temporary folder, with `build_state` for the journal and the owning writer for each typed file; on a difference it leaves the rebuilt folder in `target/state-rebuilt/<name>` and fails with a diff. A file that only the owner edits (`settings.toml`) has no writer: the test does not rebuild it, and the card parses it in a test of its own.
- A timing test prints its figure and fails only above ten times the card's target. The sizes the product promises are measured by `cargo xtask scale`, not by a unit test.

## Changing a golden or snapshot

Follow the recipe [Changing a golden or snapshot](../cards/README.md#changing-a-golden-or-snapshot) and [the golden README](../../tests/golden/README.md).

## Recurring changes

Every recurring change has its recipe in [the cards README](../cards/README.md#recipes-for-recurring-changes), with its drift check.

## Contract seams

Before an interface is frozen, its producing card defines the signature, fixture and negative control its first consumer needs. Pure decision tests do not substitute for tests through the real output handler and sink. The output recipe registers those tests once. Shared vectors cover Rust/browser citation facts and request budgets; published compatibility is checked against the previous release, not rewritten expected output in the same change. Store tests pause multi-file writes and require an old or new complete generation, or a recovery refusal. The financial-work fixtures cover field evidence, period observations, scope locks and exact issued-snapshot restore before modules depend on them.

Windows execution ownership: policy-load adds the fieldbook --all-features nextest run alongside the default-feature run, through its xtask policy-tests wrapper that provisions Windows owner/DACL fixtures and restores them on failure. r2-state-and-journal adds a separate `cargo xtask policy-tests --drill5` invocation when that target exists; it selects --features test-hooks --test drill5 inside the same provisioning and cleanup lifecycle. Zero matching drill tests fails; both Windows steps feed ci-ok and have workflow-drift checks. The ordinary Windows drill runs the Home chip cases in cmd and Constrained Language Mode; Laptop cases run only through the provisioned `--drill5` wrapper, in both shells and with a fresh layout per case. Neither context proves the other.
