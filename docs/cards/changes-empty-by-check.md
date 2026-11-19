# changes-empty-by-check

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

An empty Changes to review panel on Today and on an area overview says what was observed: it reports no claims, claims not yet checked, or checked claims that have not moved, and says "Nothing upstream has moved." only when every claim in scope has been checked.

## Why

The empty panel said "Nothing upstream has moved." while no claim had been checked, so it stated a result nobody observed. The line is now chosen by the check state of the claims in scope, with counts written in the thousands form the unreleased-build banner uses.

- Design: [HANDOFF.md](../design/HANDOFF.md) revision 26, section 0 (Fixed template text) and section 2 (Area workspaces); [MAP.md](../design/MAP.md) rows `#today` and `#area-<code>`; component `ThreeUp` (`ChangeItem` rows). The line is the panel's existing muted paragraph: no new component, no link, no token.

## Scope

- Files: `crates/fieldbook-render/src/meta.rs`, `crates/fieldbook-render/tests/home.rs`, `crates/fieldbook/tests/build.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/home.rs` (the `use crate::meta` line, one `let` of the claims in scope in `render_three_up` and its `empty` argument for the Changes to review panel; the file is the Files of `decision-package`, which edits other lines of it).
- Docs to update: the changelog. The plan line came with `design-workspace-opening`.

## Out of scope

- The Ready to reuse and Continue working lines, and the panel's header count, which stays `None recorded` when empty.
- A panel that lists items: it is unchanged.
- Views (CPY, CPT, PGM), which have no three-up.
- The design files and the mockup: `design-workspace-opening` brings them.

## Preconditions

- Cards merged: `design-workspace-opening` (design revision 26).

## Steps

1. Pin the lines at the render interface in `crates/fieldbook-render/tests/home.rs`, module `home::empty_changes`: no claims, none checked (one claim, several claims), some checked (the singular forms of the checked and unchecked counts), every claim checked, the thousands separator, Today over the whole permitted view, and a filled panel unchanged. Run them red.
2. In `crates/fieldbook-render/src/meta.rs`, add `no_changes_line(total, unchecked)`: the first matching row of the table in HANDOFF revision 26 (no claims; none checked; some checked; all checked), with its counts through the existing `format_comma_separated`.
3. In `render_three_up` of `crates/fieldbook-render/src/home.rs`, count the claims in scope with `coverage_of` (the count behind the coverage bar) and pass the total and the unchecked count as the empty line of the Changes to review panel.
4. Update the two existing assertions that expected the old line over unchecked claims: `empty_sections_show_their_line` in `tests/home.rs` and `assert_permitted_home_and_areas` in `crates/fieldbook/tests/build.rs`.
5. Add the changelog line.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test home home::empty_changes::
cargo nextest run --locked -p fieldbook --test build build::build_home_and_areas_counts_use_the_permitted_view
just check
```

Expected: each command exits 0; the first runs eight tests, all passing. An area with no claim reads "No claims here yet.", an area of three unchecked claims reads "Not established: none of the 3 claims here has been checked yet.", and a panel that lists items has no empty line.

## Negative control

On the branch, make the Changes to review panel always take the old line "Nothing upstream has moved." in `render_three_up`. `cargo nextest run --locked -p fieldbook-render --test home home::empty_changes::` must then fail six of its eight tests, the first of them with the panel text `<p class="muted">Nothing upstream has moved.</p>` where the new line was expected. Restore the file.

## Rollback

Revert the merge commit. The empty panel says "Nothing upstream has moved." again, whatever the check state of the claims.

## BLOCKED

The design files do not carry the table of lines (`design-workspace-opening` is not merged): comment `BLOCKED: changes-empty-by-check waits on design revision 26`.
