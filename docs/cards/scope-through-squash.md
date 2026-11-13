# scope-through-squash

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`cargo xtask scope` names the way through when a path changed after the card's last deleting commit, the cards README states where the declaration is read and when the check runs, and `mod tests` of `xtask/src/scope.rs` pins the restore, declare and delete sequence and the squashed range.

## Why

- The need. Builders of the print cards hit this: `cargo xtask scope BASE HEAD` reads the card text at the parent of the newest commit that deletes it (`<deleting commit>^1:docs/cards/<slug>.md`), so a path that a review round touches after the deleting commit is not declared by any card the check can read. The message, `scope: <path> is not in the Files or Registration lines of <card>`, does not say what to do, and the README's "listing the path in the card in the same pull request is the way through" does not say that the listing has to be committed before a deleting commit.
- The code is correct and this card changes no verdict. What a squash loses is the branch's intermediate edits to the card, the only copy of the final declaration; on `main` the squash commit's parent holds the original card, so a range of a push to main cannot be judged. That is why `ci.yml` runs `scope` on `pull_request` only (see the comment above the step in the `ci-ok` job). There is no defect in squash handling, only a habit that is not written down.
- The way through, which the message and the README will state: restore the card from the deleting commit's parent (`git show <deleting>^:docs/cards/<slug>.md > docs/cards/<slug>.md`), list the path under `Files:` or `Registration lines:`, commit that, and delete the card again in a later commit. The newest deleting commit is then the later one, and its parent holds the extended declaration.
- Lead decision: no behaviour change in `scope.rs` beyond the text of the failure message. Reading the declaration from a later commit, or from the card on the base, would change what the check means; that is a design question this card does not take up.
- Lead decision: the durable rule goes in the README's "Closing a card" and "Writing a card" paragraphs; `AGENTS.md` is not changed (it is at its line limit and its Commands line already names `scope`).
- Design: none. No page, string or token is built.

## Scope

- Files: `xtask/src/scope.rs`, `docs/cards/README.md`, `CHANGELOG.md`.
- Docs to update: `docs/cards/README.md` (the "Writing a card" Scope paragraph and "Closing a card" step 1), and `CHANGELOG.md` only if the owner-visible text of the command changes: the failure message is shown to the owner, so one line `- Changed: scope names the way through when a path is not in the card.`.

## Out of scope

- The `docs-checks` verdict rules, the merge-base logic and the exempt paths.
- `ci.yml`: the `scope` step stays on `pull_request` only.
- Any card or PLAN.md change beyond this card's own line.

## Preconditions

- Cards merged: `docs-checks` (it built `xtask/src/scope.rs`; merged on origin/main).

## Steps

1. In `scope.rs`, extend the failure message for a changed path outside the declaration: after the existing text add that the declaration is read at the parent of the newest commit that deletes the card, naming that commit's short hash and the card's slug, and the three steps (restore, list, delete again). Keep the leading `scope: <path> is not in the Files or Registration lines of <card_file>` text unchanged, so existing assertions pass.
2. Add to `mod tests` of `scope.rs`: `later_fix_needs_the_card_restored` (a repository where a later commit changes an undeclared path and the card was deleted before it: `run` fails, and the message holds the deleting commit and the word `restore`), `restore_declare_delete_passes` (the same repository, then a commit that restores the card with the path listed, then a commit that deletes it: `run` passes), and `squashed_range_reads_the_original_card` (one commit that adds the path and deletes the card, over the original card without it: `run` fails, documenting why CI runs the check on pull requests only). Build the repositories with the helper functions `reads_the_card_before_the_deleting_commit` already uses.
3. In `docs/cards/README.md`, add to the Writing a card Scope bullet one sentence, and to Closing a card step 1 one sentence: the declaration is read at the parent of the newest deleting commit; a path a later fix commit touches is declared by restoring the card, listing the path and deleting the card again; `scope` runs on pull requests only.
4. Run `cargo xtask cards --check` and `just check`.

## Acceptance

```sh
cargo nextest run --locked -p xtask scope::tests::
cargo xtask cards --check
just check
```

Expected: the first command passes (the three new tests are listed); the second exits 0 with its usual `ok` line; `just check` exits 0.

## Negative control

In a temporary edit make `run` read the card at the deleting commit instead of its parent: `reads_the_card_before_the_deleting_commit` and `restore_declare_delete_passes` must fail. Restore the file. Remove the way-through text from the message: `later_fix_needs_the_card_restored` must fail. Restore it.

## Rollback

Revert the merge commit. The message and the README go back to their earlier text; no verdict changes either way.

## BLOCKED

A precondition has not merged: comment `BLOCKED: scope-through-squash waits on <slug>`, push the branch and take the next card. A test helper cannot build the three histories: comment `BLOCKED: scope-through-squash needs a repository helper` and leave `scope.rs` unchanged.
