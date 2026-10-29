# kb-foundation-content

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

The exact source-free foundation bundle is present: one draft onboarding page, 32 open unanswered questions and 51 pending definitions. No claim or original is admitted or verified.

## Why

The [temporary authority](../../AGENTS.md#authority) bounds this initial drafting to the [native page and register formats](../reference/corpus-format.md). The existing checklist template applies.

## Scope

- Files: `corpus/registers/questions.csv`, `corpus/registers/glossary.csv`, `corpus/pages/foundation/onboarding/fnd-onboarding-plan.md`.
- Docs to update: none.

## Out of scope

Every other content path, sources/originals, claims, company answers, status promotion and later edits. GS-01 remains active; GS-02 is still pending.

## Preconditions

- Cards merged: `kb-foundation-guard`.

## Steps

1. Confirm the two register files are the exact header-only baselines and the page is absent. Refuse allocation drift or existing rows; never overwrite them.
2. Copy the three reviewed literal payloads from `tests/fixtures/foundation/{questions.csv,glossary.csv,onboarding.md}` to their scoped targets. Their regular-file blobs must equal the guard's pinned bundle. The glossary excludes budget_at_completion and its unadmitted source reference; rough_order_of_magnitude has no unsupported accuracy threshold.
3. Keep questions open with empty answer/date/adoption, definitions pending_company_calibration and the page draft. Check its four question references resolve; no source or claim reference is introduced.
4. Review the exact diff through the delegated decision workflow. This is own-authored public drafting, not a source-admission or verification act.

## Acceptance

```sh
cargo xtask guard-statuses "$(git merge-base origin/main HEAD)" HEAD
just check
```

Expected: the guard accepts only the exact three-file transition. Run the native Corpus check when available; until then compare with the frozen templates and field tables. No app-readiness claim follows from these checks.

## Negative control

In a scratch repository, change a question answer or page status: the guard must refuse. Do not mutate a real status to run this control.

## Rollback

Revert the content through an authorized reviewed change; the agent cannot make later Corpus edits under this exception.

## BLOCKED

The baseline, allocated IDs or pinned payload differs. Preserve existing rows and use the delegated review flow; do not widen the exception or admit a source.
