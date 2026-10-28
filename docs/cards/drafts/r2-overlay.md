# r2-overlay

- **Runs where:** home, CI Linux, CI Windows
- **Schema impact:** none
- **Review:** owner

## Outcome

The laptop can initialize an empty company store and build its filtered local site through the existing Layout, labeller and journal. The public edition remains a read-only reference store.

## Why

- Company data needs a concrete storage and rendering entry point before intake and record adapters use it; the roots, overlay folders and checks are in [the company store](../../reference/company-store.md#what-it-is). The failure to prevent is a build writing company text into a public edition or a checkout ([ADR 0004](../../decisions/0004-company-boundary.md)).
- The interface lane supplies the approved label banner and copy-control design before the native work begins.

## Scope

- Files: `crates/fieldbook/src/overlay.rs`, `crates/fieldbook/tests/overlay.rs`, `tests/fixtures/overlay-empty/`, `CHANGELOG.md`, `tests/golden/json/init.json`, `crates/fieldbook-core/src/company_render.rs`.
- Registration lines: `crates/fieldbook/tests/cli.rs` (remove init from NOT_BUILT), `crates/fieldbook/src/verbs.rs` (init in DETERMINISTIC_VERBS), `crates/fieldbook/tests/deterministic_verbs.rs` (init entry), `crates/fieldbook/tests/chips.rs` (init in NO_CHIP and the `fieldbook build --overlay` PENDING entry), `crates/fieldbook/tests/drill5.rs` (init, check and the overlay-build chip case), `tests/support/chip_cases.rs` (the `fieldbook build --overlay` Laptop case, this card's entry only); `crates/fieldbook/src/{lib.rs,cli.rs,build.rs,new.rs}` (overlay dispatch and root selection only); `crates/fieldbook-render/src/lib.rs` (labelled local-site entry only); `crates/fieldbook/tests/leak_outputs.rs` (overlay site surface); `crates/fieldbook/tests/support/world.rs` (layout use only).; `crates/fieldbook-core/src/egress/view.rs` (local constructor and tests); `crates/fieldbook-render/src/frame.rs`, `crates/fieldbook-render/templates/frame/` and `crates/fieldbook-render/templates/base.html` (store context, BuildChip/sidebar and effective-label banner); `crates/fieldbook-render/tests/frame.rs` (public/company assertions); `crates/fieldbook-render/src/about.rs` and `crates/fieldbook-render/templates/about.html` (company metadata, counts and handling statement); `crates/fieldbook-core/src/{lib.rs,report.rs}` (CompanyRenderInfo module and InitReport); `crates/fieldbook/tests/json_reports.rs` (init row); `docs/runbooks/restore-drill.md` (Drill 5 only, at the existing R2 position, after this card has merged); `crates/fieldbook-render/src/{site.rs,frame.rs,about.rs}` (render context and conditional planned paths); `crates/fieldbook-render/tests/frame.rs` (company/public metadata); `crates/fieldbook-core/src/edition.rs` (Company variant of existing RenderContext); `crates/fieldbook/src/io/{store.rs,journal.rs}` (init pending lookup before store-open validation).
- Docs to update: `docs/runbooks/restore-drill.md` (Drill 5), `docs/reference/company-store.md` (Cards that build it); generated cli.md through cargo xtask docs, CHANGELOG.md; read the interface lane design without editing it.

## Out of scope

- Company-id allocation, writer adoption and the company search union are r2-company-ids. This card creates an empty store only; it admits no company source.
- No new label rule, network route or writer. The local site never becomes an export destination.

## Preconditions

- Cards merged: `r2-state-and-journal`, `build-site-lints`, `core-labels`, `core-policy`, `policy-load`, `assistant-page`.

## Steps

1. `overlay::init(world, layout, journal, policy)` requires the Laptop layout and checks matching pending recovery before any fresh-store existence test. Without a matching pending intent it requires absent overlay/store.toml and an empty recognized initialization target. Plan `store.toml` with the existing StoreToml parser/render: writer 0, uid from World::fresh_uid, no restored_from. Write `schema_version` at the supported value in `overlay/corpus.toml`, never in StoreToml; empty registers use the existing registered headers. Create only the fixed overlay and state roots through audited helpers. Refuse a nonempty unrecognized overlay, under-git or invalid placement before begin. Never replace a completed store; matching recovery resumes its original plan and UUID. The later --writer extension uses this planner, rather than a second init.
2. Follow the verb recipe for `init`: remove it from NOT_BUILT, add it to DETERMINISTIC_VERBS with its ENTRIES test on an empty synthetic layout, and to NO_CHIP. `init --overlay` and `build --overlay` use Layout.content_root; `check` on the laptop reads that same root. Without --overlay, build continues to create the public edition from Corpus only. TestWorld supplies layout and placement; no path-name inference is added. Remove the exact `fieldbook build --overlay` chip from `PENDING` once that complete variant parses, and register its Laptop case in `tests/support/chip_cases.rs`.
3. `overlay::build` takes ReadableStores, selects an effective LabelledView and an owned coherent generation. Before writing, require the State store to hold every rendered label; a store with readers restrictions is not written as a static site. Write only state/site/overlay through RootPath and Audited. The renderer receives the filtered view and CompanyRenderInfo defined below; public EditionInfo is never constructed for company content. Strip copy-context controls when the destination is not allowed, and render the effective label banner by the approved interface design. No company text goes into the public edition tree.
4. Use the existing CSV bridge planner and row serializers for empty headers and planned rows; do not introduce a second CSV parser. The company-id card extends the store initialization and index after this card. The local site's company search shards follow search.md once company units exist.
5. Tests in overlay.rs: `init_makes_one_empty_store`, `second_init_preserves_every_byte`, `foreign_folder_is_refused_before_begin`, `git_or_sync_root_is_refused`, `state_rule_refuses_site_without_writes`, `reader_restricted_store_has_no_static_site`, `company_canary_never_reaches_public_edition`, `overlay_build_uses_one_generation`. Register the actual site sink in leak_outputs and compare permitted/forbidden canaries. Run native interface checks from the approved design study; record missing design contracts as an interface-lane block.
6. Extend the laptop drill to run init, strict check and build --overlay on a synthetic temporary layout, and run the `fieldbook build --overlay` chip case through the existing drill5 shim in cmd and Constrained Language Mode, with a fresh layout per shell. Its manifest lists state/site/overlay only. Regenerate CLI documentation. The deployment flow is the release/intake gate; this card does not deploy.

7. Add an explicit local `LabelledView` constructor taking the authorized `ReadableStores` set and a coherent model. Preserve all effective labels and construct no `Allowed`. Keep State admission and the ban on static sites for reader-restricted stores. The real renderer test includes marked content locally under floor B, refuses inaccessible stores and State admission failures, and keeps that canary absent from public editions and refused at outward sinks. Widening a destination row or dropping local marked content must fail.

8. Pass store context and effective labels from the selected view to the shared frame. Public editions retain both public-only strings. Company views show Company storage and the existing label banner/foot, including locally permitted marked content, and neither public-only assertion. Tests `public_frame_keeps_public_context` and `company_frame_uses_effective_labels` render actual public/company frames, including scripts-blocked output; the native check covers wide/narrow layouts and light/dark themes. Keep the same tokens and component geometry.

9. About uses the same selected store context for its handling statement, while its compiled profile/posture remains static. Extend company/public renderer cases to About; it does not probe live adapters, policy or connected company data sources.

10. Initialization uses OperationKey verb init with subject_sha256 from the canonical overlay-root identity, usable before store.toml exists. Normalize schema target, canonical overlay/State target hashes, writer (0 here), overlay flag and initialization mode; --json changes presentation only. Acquire the existing operation lease and call pending_intent before loading the incomplete content store. Fresh planning mints one World::fresh_uid and persists it in allocated_store_uid; recovery uses that exact UID and original transaction, never a second allocation. Fault-test before/after begin, each content write, end and intent cleanup with `init_recovery_preserves_uid_and_transaction`. Completed stores, changed requests and unrelated pending operations refuse. Preserve safe discard for an intent with no begin and unchanged files; after such discard the next fresh init is a new operation.

11. Follow Adding a JSON report for `init --overlay --json`: ordered `InitReport { report_version: u32, store_uid: String }`, version 1 and the validated canonical UUID. Emit one success object only after durable completion, including successful recovery. Refusal emits no success object, uses the existing diagnostic on stderr and preserves its nonzero exit. Register report.rs/json_reports.rs/init.json and regenerate cli.md. Test fresh and recovered success, refusal with empty stdout and exact field order. The later writer/adoption card extends this producer.

12. Define pure `CompanyRenderInfo::from(view: &LabelledView, info: &BuildInfo, tool_version: &str, schema_version: u32) -> CompanyRenderInfo` with the same counts consumed by About and the view’s effective labels. Extend edition-manifest’s existing RenderContext with Company(CompanyRenderInfo), keeping Public(EditionInfo); BuildInfo remains the one compiled profile/date/commit source. Company rendering requires no edition heading and emits no edition.toml; planned paths, header, About and output dispatch branch on this context. Public EditionInfo validation and required shipped selection stay unchanged. `overlay_build_has_local_metadata_without_edition_heading` invokes real build --overlay with permitted marked content and valid release metadata, no company edition heading, and asserts successful delivery, correct About counts/labels and absent edition.toml. Retain the public missing-heading refusal. Pure renderer tests cover both contexts.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --test overlay --test leak_outputs --test cli --test chips --test deterministic_verbs
cargo xtask docs --check
cargo nextest run --locked -p fieldbook-render --test frame
cargo nextest run --locked -p fieldbook --test json_reports
# Windows only; provisions and restores policy fixtures:
cargo xtask policy-tests --drill5
just check
```

Expected: each command exits 0; the named positive cases pass and the planted negative cases fail before restoration.

## Negative control

Route overlay output to the public edition: company_canary_never_reaches_public_edition must fail. Skip State holds or placement: the matching refusal test must fail. Restore each.

Use the public-only strings for a company frame, hide a permitted marked label or replace the public frame with company text: the paired frame tests must fail. Restore each.

Mint a second UID during recovery, emit success JSON before durable completion, or construct public EditionInfo for a company view: the init/recovery/report or real overlay-build tests must fail. Rename an InitReport field: json_goldens_match_reports must fail. Remove drill feature/provisioning or select zero tests: the policy-tests wrapper must fail. Restore each.

## Rollback

Revert the implementation; retain company data and backups. A previous executable must refuse an unsupported store, never rewrite it. Remove only the generated local site through clean --site when that verb exists.

## BLOCKED

An approved interface contract is missing, or placement/store admission cannot be enforced: record the concrete missing contract in the owner's tracker and stop. Do not weaken the company boundary.
