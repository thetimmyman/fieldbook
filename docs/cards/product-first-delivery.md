# product-first-delivery

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The dependency work order uses standard committed-lock Cargo/cargo-deny checks and required exact-source MAX/JEV pin/profile/release-age review, so application work does not wait for unimplemented custom dependency automation. Complete checker and bounded capture/replay requirements remain deferred, with their controls unpassed.

## Why

- [AD-06](../decisions/0001-foundation.md#ad-06-dependency-allowlist), [AD-21](../decisions/0001-foundation.md#ad-21-codex-execution-plan) and [AD-24](../decisions/0001-foundation.md#ad-24-what-is-not-built-yet-later) own the dependency policy, authority and deferred work. Budgets, the cooldown and security-exception review remain binding.
- [Review evidence](../reference/dependencies.md#review-evidence) defines the concrete replacement evidence; no green parser/prototype test stands in for it.

## Scope

- Files: `docs/cards/product-first-delivery.md`, `docs/cards/day1-dependencies-3.md`, `docs/cards/day1-dependencies-2.md` (defer), `docs/cards/day1-dependencies-checks-4.md` (defer), `docs/cards/drafts/{later-dependency-checks.md,later-lock-replay.md}`.
- Registration lines: `AGENTS.md` (dependency authority and lock Commands only; retain the approved foundation authority and primitive-provider label); `docs/plan/PLAN.md` (dependency work order/Later rows; retain foundation/primitive rows); `docs/decisions/0001-foundation.md` (AD-06/07/21/24 work-order clauses and primitive docs-provider label only); `docs/controls.md` (DP-02/04 and dependency-evidence note); `docs/reference/dependencies.md` (present evidence versus retained deferred contracts, counts and Measurements ownership); `docs/cards/README.md` (dependency recipe); `docs/cards/{check-ci-and-docs.md,docs-checks.md,windows-facts.md,edge-harness.md,core-policy.md,release-workflow.md,scheduled-workflow.md}` and `docs/cards/drafts/{net-stack-spike.md,extract-xlsx-sheets.md,mutation-report.md}` (dependency recipe/prerequisite/acceptance references only); `docs/runbooks/{release.md,toolchain-bump.md}` (dependency recipe only); `.config/tools.toml` (explanatory comments only; parsed values unchanged).
- Docs to update: exactly the work-order, control, provider and consumer references named above.

## Out of scope

Executable code, manifests/lock, configuration values, CI implementation/permissions, dependency adoption, all Corpus/seed/golden data and design files. No source admission, status raise, company authorization, release or readiness claim.

## Preconditions

- Cards merged: `workspace-skeleton`, `kb-foundation-guard`, `kb-foundation-content` and `core-types-and-strings` (retain their approved source/content/provider changes on current main).

## Steps

1. Reconcile Authority, DP-02/04, AD-06/07/21/24 and PLAN to the standard-Cargo work order; retain merged providers, the valid nightly-only lock table and all foundation branches.
2. Defer the complete checker and offline replay/capture/bootstrap to their own Later drafts, retaining their full process/negative requirements and known unmerged policy/record repair obligations. No partial command or substitute verifier is activated.
3. Make actual adoption and downstream consumers require the standard checks and exact-source MAX/JEV pin/profile/release-age evidence. Preserve owner review for exceptions and the AI direct-crate threshold; remove nonexistent command prerequisites.
4. Review every impacted reference and both deferred drafts; check that code, parsed configuration values, Corpus bytes, decision statuses and design files remain unchanged by this amendment.

## Acceptance

```sh
just check
```

Expected: exit 0 on the reconciled current tree. Scope/links/sections and the exact source patch pass review. This documentation check neither adopts crates nor passes the deferred controls.

## Negative control

In a review copy, restore a `deps --check`/`lock-verify` adoption prerequisite, omit actual release-age/profile evidence, or lose a retained process test/foundation branch: the source review must reject the amendment. This is a document-review control, not an invented executable gate.

## Rollback

Revert this amendment and restore the previous complete dependency-work order coherently. Leave already merged providers, accepted foundation content and actual source changes intact; do not use rollback to certify deferred work.

## BLOCKED

An active consumer still assumes a deferred command exists, required evidence or an exception's reviewer is unspecified, or reconciliation would drop a foundation/control requirement. Name and repair the exact contract before merge; do not add a replacement helper.
