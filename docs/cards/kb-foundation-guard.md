# kb-foundation-guard

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

GS-01 permits only the exact reviewed initial source-free foundation transition: one own-authored draft onboarding page, 32 open unanswered questions and 51 pending definitions. Every other protected edit stays blocked; full GS-02 remains pending.

## Why

The first usable knowledge base needs these existing drafts. This bounded authority exception preserves the [status gate](../decisions/0001-foundation.md#gates), without waiting for general field-level upkeep or external-source admission.

## Scope

- Files: `xtask/src/guard.rs`, `xtask/src/guard_foundation.rs`, `tests/fixtures/foundation/`.
- Registration lines: `xtask/src/main.rs` (module if needed); `AGENTS.md` (the exact temporary foundation authority); `corpus/AGENTS.md` (the matching own-authored drafting exception); `docs/controls.md` (GS-01 temporary bound, GS-02 remains pending); `docs/cards/kb-foundation-content.md`, `docs/plan/PLAN.md` (initial content ordering only).
- Docs to update: both authority files, controls and plan.

## Out of scope

Claims, source rows/originals, company answers, verification, stable pages, other Corpus edits, seed/evaluation edits and general field-level GS-02 behavior. No content is admitted by this implementation card.

## Preconditions

- Cards merged: `workspace-skeleton`, `hook-environment-isolation`.

## Steps

1. Bind two immutable regular-file transitions: the exact corpus/AGENTS authority amendment as its commit’s sole protected path, and the whole three-file foundation bundle with header-only baselines and absent page. The document carries no self-hash. Do not parse a second schema or authorize arbitrary drafts.
2. Keep every other Corpus/seed/evaluation path protected. Reject baseline drift, modified bytes, later edits, partial bundles, deletions, type changes and renames. Agent merges with protected differences against any parent fail closed; disable replacement-object interpretation.
3. Replace the old guessed-path scratch pre-deletion with atomic ownership before executing guard tests.
4. Prove the exact transition passes and the named refusals fail, through actual Git and the native guard. Keep GS-01 active and its controls; do not call this GS-02.
5. State the exact exception in both authority files and introduce the isolated three-file content card. Consequential review uses the actual delegated frontier/JEV council receipt plus normal source/check/CI review.

## Acceptance

```sh
cargo nextest run --locked -p xtask guard::
just check
```

Expected: all exact transition and refusal controls pass; no product Corpus content changes in this card.

## Negative control

Remove an exact payload/mode/baseline check or admit an agent protected merge: its native refusal case must fail. Restore every plant.

## Rollback

Revert this merge; the unconditional GS-01 Corpus path rule returns.

## BLOCKED

The actual payload or baseline differs, or a proposed action needs source admission/status authority outside the exact envelope. Preserve the evidence and do not widen the exception.
