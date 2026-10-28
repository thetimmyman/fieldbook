# hook-environment-isolation

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The pre-push hook runs its checks without exporting the pushed repository's Git context into scratch-repository commands.

## Why

Git exports repository-local variables to hooks. The scratch repositories used by the existing guard and denylist tests must remain independent. This restores the existing push check contract and changes no product, policy, format or CI permission.

## Scope

- Files: `.githooks/pre-push`, `xtask/tests/hooks.rs`.
- Registration lines: `docs/plan/PLAN.md` (one item after workspace-skeleton).
- Docs to update: none.

## Out of scope

Product behavior, dependencies, Corpus, seeds, CI permissions and bypassing hooks.

## Preconditions

- Cards merged: `workspace-skeleton`.

## Steps

1. Test the actual hook process in synthetic repositories with injected Git context and a check-command adapter that reads a foreign repository. Assert the selected foreign root and unchanged pushed-repository HEAD/index.
2. After selecting the hook's root and preserving its push protocol, clear the repository-local variable names supplied by `git rev-parse --local-env-vars` before running checks. Preserve every existing check and denylist input.
3. Run the complete checks with hooks enabled and push the branch normally.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test hooks
just check
git push -u origin card/hook-environment-isolation
```

Expected: the regression and all existing checks pass; the normal pre-push hook runs them again without changing HEAD/index.

## Negative control

Remove only the environment clearing: the regression fails because the foreign Git query resolves the pushed repository. Restore it. The regression performs only reads after context injection.

## Rollback

Revert the merge commit. Keep pushes stopped until the isolation regression passes again.

## BLOCKED

The variable inventory cannot be read, the regression changes either repository unexpectedly, or an existing check stops failing its negative control. Preserve the evidence; never bypass the hook.
