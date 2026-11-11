# frame-unbuilt-entries

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

On a build whose navigation has entries with no built target, no page's sidebar or section tabs show an "is not available until" line where revision 22 says to hide or collapse it, and every entry whose target is built still links.

## Why

- The need. Every page of the hosted edition carries about 26 "is not available until" lines in its frame: 20 sidebar entries and the Assistant, Work and Plan tabs. `nav::build` in `crates/fieldbook-render/src/nav.rs` makes each entry a `NavTarget` from `PlannedPaths`, and `templates/frame/sidebar.html` and `templates/frame/topbar.html` render a target that is not planned as `.not-available` text with its landing line (`format_not_available`). The reader can do nothing with these lines.
- The decision is the design's: revision 22 (`design-r22-reader-pages`, step 3) says whether such an entry is hidden or collapsed into one disclosure, with its wording.
- `PlannedPaths` already decides what is built, so the card reads that and adds no new input. A built target keeps its link, so no page can fail `FB0603`.
- `nav.rs` and `frame.rs` are also changed by `unreleased-status-strip`, so that card is a precondition.
- Design: `MAP.md` row AppShell, TopBar, SectionTabs, Sidebar, ContextNav, NavItem; `HANDOFF.md` section 3 (Frame) and section 5 (`Sidebar`, `NavItem`, `SectionTabs`) as revision 22 words them; token groups as the revision names, no new token. No `style=""`, and the frame reads the same with scripts blocked.

## Scope

- Files: `crates/fieldbook-render/src/nav.rs`, `crates/fieldbook-render/src/frame.rs`, `crates/fieldbook-render/templates/frame/sidebar.html`, `crates/fieldbook-render/templates/frame/topbar.html`, `crates/fieldbook-render/tests/frame.rs`, `crates/fieldbook-render/tests/snapshots/`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/assets/app.css`, only if the revision adds a class for the disclosure.
- Docs to update: `CHANGELOG.md` (`- Changed: navigation entries that a build does not hold no longer fill the sidebar and tabs with "not available" lines.`).

## Out of scope

- The landing lines' wording and `format_not_available`: the app map and the about page may still use them.
- The home and area body placeholders: `home-area-placeholders`.
- Building any of the missing targets.

## Preconditions

- Cards merged: `design-r22-reader-pages`, `unreleased-status-strip`, `build-charts-2` (its Scope holds `tests/snapshots/`, which this card's frame change rewrites).

## Steps

1. In `nav.rs`, keep the `NavTarget` model and add what the template needs to apply revision 22 (for example, the unbuilt entries of a group as one list), computed from `PlannedPaths` only.
2. In `sidebar.html` and `topbar.html`, render unbuilt entries and tabs as the revision says; a built entry renders as before.
3. Tests in `tests/frame.rs`: `unbuilt_entries_follow_revision_22` (a fixture whose planned paths omit `map.html` shows no "is not available until" line in the sidebar, or shows it only inside the revision's disclosure), `built_entries_still_link` (every planned target is an `<a href>`), `unbuilt_tab_follows_revision_22`. Update the snapshots under `tests/snapshots/` and read each diff.
4. Run `just site` and count the lines in the pull request: `grep -c 'is not available until' out/edition/index.html` before and after.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test frame
cargo nextest run --locked -p fieldbook-render --test page_types
just check
```

Expected: the tests pass; `just check` exits 0.

## Negative control

Render every entry as before in `sidebar.html`: `unbuilt_entries_follow_revision_22` must fail. Restore it. Drop the `href` of one built entry: `built_entries_still_link` must fail. Restore it.

## Rollback

Revert the merge commit. The frame shows the landing lines again.

## BLOCKED

A precondition has not merged: comment `BLOCKED: frame-unbuilt-entries waits on <slug>`, push the branch and take the next card.
