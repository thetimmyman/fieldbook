# context-edits-conflict-anchor

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`assist::apply_edits` labels the opponents of a conflict by position, not by which claim brought them in: the earliest included claim of a conflict keeps its own status, and every later included claim of that conflict is the one shown as contradicted. Adding, suggesting or pinning the opposing claim therefore no longer relabels the claim the pack was built around, and re-applying the same edits gives the same pack.

## Why

- Control HG-02 in [controls.md](../controls.md): a selected claim brings its conflict row and the opposing claims, and a claim in a conflict never appears in a pack without its conflict. The rule that a claim keeps its own status line is the "shown as contradicted" half of [Dates, supersession, handling and labels](../reference/corpus-format.md#dates-supersession-handling-and-labels): only the opponent is marked, not both sides.
- Defect: `apply_edits` recorded an opponent's conflict id while it walked the edit lists, so a claim that arrived later in the walk stamped `contradicted (conflict <id>)` on a claim that was already in the pack with its own status. Adding `CL-CMP-0002` to a pack built around `CL-CMP-0001` marked both claims contradicted.
- The same defect the card `context-first-claim-conflicts` left out of scope in its pull request: it fixed the first-time opponent's own closure in `pack.rs` and said that the opponent's status line belongs to a change of its own.
- The positional pass at the end of `apply_edits` already recomputed `included` for every active claim's conflict row; this card makes it the only place that decides opponent styling, so the walk order cannot change a label.
- Design: none. This card changes no page, template, string or token.

## Scope

- Files: `crates/fieldbook-core/src/assist/edits.rs`, `crates/fieldbook-core/tests/context_edits.rs`.

## Out of scope

- The context pack builder's own first-time opponent closure and conflicts (`context-first-claim-conflicts`, in flight).
- A claim that is in two conflicts at once: the pack builder records one conflict id per opponent and this card keeps that shape, so a second conflict does not add a second status line.

## Preconditions

- Cards merged: `handoff-context` (it makes `apply_edits`, `Edits` and the item list this card rebuilds).

## Steps

1. `apply_edits`: stop recording `conflict_opponents` inside `push_conflict_deps` and inside the pass that preserves an existing closure item's styling; record it only in the positional pass over `active_claims`, where the item order is final. The earliest included claim of a conflict is its anchor and keeps its own status; a later included claim of that conflict is the opponent and gets `contradicted (conflict <id>)`.
2. `tests/context_edits.rs`: `claim_status(&ContextPack, &str)` reads a claim block's `Status:` line. `an_added_opponent_does_not_relabel_the_claim_it_opposes` adds and suggests `CL-CMP-0002` on a pack built around `CL-CMP-0001` and requires `CL-CMP-0001` to keep `primary_source_supported` and `CL-CMP-0002` to read `contradicted (conflict XC-01)`. `a_pinned_opponent_takes_the_status_leaving_the_claim_it_opposes_clean` pins `CL-CMP-0002` and requires it first with `unchecked` and `CL-CMP-0001` shown as contradicted, because a pin moves the anchor.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core --test context_edits
cargo nextest run --locked -p fieldbook-core --test context --test pack
just check
```

Expected: the `context_edits` tests pass, the two new ones among them; the `context` and `pack` tests pass with the goldens unchanged; `just check` exits 0.

## Negative control

On the branch, restore the `conflict_opponents` record inside `push_conflict_deps`: `an_added_opponent_does_not_relabel_the_claim_it_opposes` fails with `left: "contradicted (conflict XC-01)"`, `right: "primary_source_supported"` at the `added CL-CMP-0002 must not relabel CL-CMP-0001` assertion. Restore the fix.

## Rollback

Revert the merge commit. `apply_edits` goes back to recording opponent styling during the walk, and the two tests go; nothing else reads the label.

## BLOCKED

A golden case expects both claims of a conflict to read contradicted, or the positional pass cannot see every included claim of a conflict because an edit can put a claim in the list after the pass runs.
