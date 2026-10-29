# Dependencies

This is the one list of what Fieldbook depends on: crates, developer tools and CI actions. It is read by the owner and by Codex. IT's view of the dependencies is the SBOM and the licence report (artifact 6 of the [intake pack](../intake/README.md)).

- A crate, tool or action is added together with its row in these tables, in the same pull request: name, exact pin, licence, what it is used for and a sentence on why it is needed. `cargo xtask deps --check` fails a dependency that has no row, and a pin that differs from its row (DEPS-1, DEPS-2, DEPS-4 and DEPS-5).
- Adding one, or refreshing the lock, is in scope for the card that needs it. The card is `auto` when `cargo xtask deps --check`, `cargo deny check` and `just lock-verify` pass, and the owner reads the week's merged work.
- Two additions need more. A direct crate past the 15th of the AI build needs a card whose Review is `owner`. A network crate changes `deny.toml`, which only the card `net-stack-spike` edits.
- The decision behind this document is AD-06 in [ADR 0001](../decisions/0001-foundation.md).
- A change to a pin edits this document, `Cargo.toml` or `.config/tools.toml`, and the workflow files in one pull request.

Words used here: a **direct shipped crate** is a crate named in `[dependencies]`, or in a `[target.'cfg(..)'.dependencies]` table, of a shipped crate (`fieldbook-calc`, `fieldbook-core`, `fieldbook-render`, `fieldbook`, `fieldbook-win`). The **transitive count** is the whole-workspace crate/version count under the target and profile algorithm below. Dev-only paths are excluded. Normal dependencies reachable through `xtask` count toward that cap, but not the direct runtime budget. A crate that only `xtask` uses has its row under Pins, with `xtask` as its user (`object`); a dev-only crate is listed under Test-only and dev-only crates. There are two **profiles**: the silent build (default features, the only build in an edition) and the AI build (feature `net`). A crate that only the AI build enables counts as a direct shipped crate of the AI build only.

## Bootstrap providers

`day1-dependencies` provides complete catalog/workflow reference facts and source locations. `day1-dependencies-inputs` adds workspace/tool/replay configuration and the complete strict input aggregate; the checker waits for that final provider. Neither input provider registers a CLI or partial policy gate. `day1-dependencies-checks` owns the single complete DEPS-1 to DEPS-5 command, graph measurements, baseline row 0 and DP-02. `day1-dependencies-2` adds actual isolated offline replay over immutable bounded fixtures, still without changing production manifests, lock or replay configuration. `day1-dependencies-3` captures the production bundle, adopts the initial dependency set and activates the complete just/CI replay gate and DP-04. Consumers needing initial crates, the workspace path edge or lock recipes wait for `day1-dependencies-3`. No declarations-only part merges before full replay works. Each implementation must still meet the card-size rule; a measured further split preserves the final complete checker and gate.

The source-reference interface `deps_references::read_references(root)` reads only this document and `.github/workflows/` under the explicit root. It returns complete rows and action references with relative paths and one-based lines, or a structured read error. Enumeration is sorted; missing, unreadable or unsupported entries refuse. It performs no Cargo resolution, configuration reading or policy comparison; absent catalog entries, mismatched pins and literal tags, local paths or Docker references remain facts.

The four required tables have the exact headers under Shipped runtime crates/Pins, Test-only and dev-only crates, Developer tools before Agent aids, and CI actions. Tables may be empty; every cell is retained with only delimiter padding trimmed. Widths, required cells, separators and canonical identities are checked. Crate and action identities are the initial complete backticked name; tool identities also allow bare names before parenthetical annotations. Crate identities are unique across runtime and dev tables; tools have a separate namespace. Other prose and tables are outside the input grammar. Backtick/tilde fences and four-space-indented code are data; raw HTML block/comment contexts, comments within cells, multiline cells and escaped pipes are unsupported; other inline cell markup is retained verbatim.

Workflow references occur only at `jobs.<job>.uses` and `jobs.<job>.steps[*].uses` in block mappings with bare keys, empty mapping values at job declarations, and indented sequences with exactly one space after the dash. Metadata remains opaque; established `env`, `with`, `strategy`, `matrix`, `inputs`, `outputs`, `on` and `defaults` contexts are data. One-line plain, single-quoted (including doubled quotes), and JSON-compatible double-quoted scalars are supported; plain `#` begins a comment only after whitespace. Literal/folded block bodies (`|`, `>`, with optional `+` or `-`) are data until dedent, including blank lines. Bare sequence scalars/markers, additional dash whitespace, numeric block indentation indicators, multiline quoted scalars, anchors/aliases, dynamic references, indentless steps, and flow or ambiguous forms that could hide references refuse with a location. This is bounded extraction, not general Markdown/YAML validation or a policy verdict.

The configuration provider reads only root `Cargo.toml` workspace declarations, `.config/tools.toml` and `rust-toolchain.toml`, then reuses the reference provider. `[workspace]` supports `members`, `exclude` and `default-members` string arrays plus a string `resolver`; `[workspace.dependencies]` is optional and accepts string pins or one-line inline tables. No explicit `[workspace]` header is required; a present manifest with no recognized workspace declarations yields empty workspace facts. Cargo owns implicit-workspace semantics. Supported dependency fields are string `version`, `package`, `path`, `registry`, `git`, `branch`, `tag`, `rev`; string-array `features`; and boolean `default-features`, `optional`, `workspace`. Top-level `cargo-features` assignments before any table, plus package/lint/profile/feature and root-package/target dependency sections, remain opaque Cargo-owned data, never claimed as validated; section delimiters cannot hide or invent workspace declarations. Ambiguous multiline/unsupported shapes refuse. Syntactically supported path/git/wildcard and policy-fault declarations remain facts; Cargo owns dependency semantics.

`[cargo]` accepts arbitrary bare tool keys with nonempty string pins. Fixed schemas are `[gitleaks]` string `version`/`linux-x64-sha256`, `[poppler]` version, optional `[fuzz]` toolchain, and `[toolchain]` channel/profile with optional components/targets string arrays. Baseline `[lock]` nightly is valid. Complete replay records retain integer `replay_schema = 1` and string mode (`publish-time` or `stable-fallback`), resolver, cutoff, inputs_sha256, snapshot and snapshot_sha256 separately from tool pins; optional nightly remains a tool pin. Ordered `[[lock.adjustment]]` rows contain package/version/reason (`cooldown` or `security`); security rows also contain advisory, approval_commit, approval and approval_sha256. Missing, duplicate, unknown, malformed or unsupported configuration refuses with a relative location. These are shape checks; hash/date/path/approval/resolution semantics remain with [the replay verifier](#the-cooldown).

Unquoted separator whitespace is ASCII space/TAB; line endings are LF or CRLF. Configuration keys and the listed table names use the bounded unquoted forms; relevant assignments, string arrays and dependency inline tables occupy one line. Single-quoted strings retain their literal bytes; double-quoted basic strings support faithful scalar `\uXXXX`, quote/backslash and `\b`, `\t`, `\n`, `\f`, `\r` escapes; other escapes, raw ASCII controls other than TAB, triple-quoted/multiline forms and trailing garbage refuse; unescaped TAB is retained in both supported string forms. Empty arrays and trailing container commas are supported. The aggregate nests the reference facts unchanged and propagates their read errors; replay fields and adjustment order are retained separately, without validating their semantic values.

## Policy

| Rule | Value | Checked by |
|---|---|---|
| Budget for direct shipped crates | Silent build: 12 at Day 1 (`windows-sys` is one, in `fieldbook-win`), 14 once `extract-xlsx-sheets` has merged (`calamine` and `zip`) and still 14 at R2 (`embed-resource` is build-only). AI build (feature `net`): 17, which adds `ureq`, `rustls` and `rustls-graviola`; the feature `bridge` adds no crate. A direct shipped crate of the AI build past the 15th requires an `owner` review card (`extract-xlsx-sheets` is one) | `cargo xtask deps --check` (DEPS-3); the count is recorded under [Measurements](#measurements) |
| Cap on transitive crates | Per profile, counted as described below on the two graph targets: silent 80 (measured 63), AI 100 (measured 87). The count for `--target all` (106 for the AI build) is reported and not capped. A cap is a judgment, not a measurement; revisit it when a count comes within 10 of it. The measured figures are from before `extract-xlsx-sheets`, which adds about nine crates to each count (estimates 72 and 96), and before `windows-facts`, which adds two on the Windows target (`windows-sys` and `windows-link`), and `core-policy`, which adds one (`object`, in `xtask`; its `memchr` is in the graph already); the pull request of each card reports the measured counts. `net-stack-spike` repeats the counts | `cargo xtask deps --check` (DEPS-3), once per profile: `cargo metadata` with default features for the silent build, with `--features net` for the AI build |
| Registry | crates.io only. No git dependencies, no path dependencies outside the workspace, wildcard requirements denied | `cargo deny check sources` on every pull request |
| Licences | MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, Unlicense. Workspace crates use `LicenseRef-Proprietary` with `publish = false`, and `deny.toml` sets `[licenses.private] ignore = true` | `cargo deny check licenses` on every pull request |
| No C code in the build graph | The named bans below | `cargo deny check bans`; the metadata test also fails if a banned crate is in the normal graph |
| Duplicate versions | `multiple-versions = "warn"`. Exactly one `zip` version is allowed | The metadata test fails on a second `zip` |
| Graph targets | `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` | `deny.toml` |
| MSRV | No crate may need a newer Rust than the pinned toolchain (1.99.1; `rust-version = "1.99"`) | Resolver 3 prefers releases that fit `rust-version`; `cargo build` fails otherwise |
| Cooldown | A release is adopted only when it is at least 14 days old, and the newest release that qualifies is the one used. This holds for crates, tools, actions and the Rust toolchain | See [The cooldown](#the-cooldown) |
| Justification | Every direct crate has a row below with why it is needed, the alternatives considered and its maintenance health (NFR-M-02 in ADR 0001) | The row is required in the same pull request (DEPS-1). Whether a crate is useful and its maintainers healthy is the row author's judgement, which the owner reads in the week's merged work |

### Counting crates

The silent direct budget is 14 only when both `calamine` and `zip` are active direct normal dependencies of shipped crates in the silent profile; otherwise it is 12. The AI direct budget is 17. The checker derives this stage from the shipped normal-edge graph, not tracker status, catalog rows, lockfile presence, xtask or dev/build edges. A single reader dependency does not select the higher silent ceiling. The existing DEPS-3 fault test exercises each budget and transitive-cap boundary.

The transitive count is the number of distinct crate-and-version pairs in the normal dependency graph of the whole workspace, `xtask` included, minus the workspace's own crates. It is counted once per profile, on the two graph targets of `deny.toml`. `cargo xtask deps --check` makes that count from `cargo metadata` (DEPS-3). The command below counts the same graph for all targets, which is a superset; its figure is reported next to the capped one and is not capped:

```sh
cargo tree --workspace -e normal --target all --prefix none --no-dedupe --format '{p}' | grep -v ' (/' | grep -v ' ([A-Za-z]:' | sort -u | wc -l
```

Run the command once without and once with `--features net`; both counts go in [Measurements](#measurements). The command counts a superset of what the shipped program contains, because `xtask` is included. Direct runtime crates are counted from the `[dependencies]` tables of the five shipped crates, once per crate name and once per profile; an optional crate counts only in the profile that enables it, and a crate in a `cfg(windows)` table counts in both.

### Where versions are declared

Versions and features are declared once, in `[workspace.dependencies]` of the root `Cargo.toml`, as a caret requirement at the version shown in the tables. Member crates write `crate = { workspace = true }`. `Cargo.lock` pins exact versions.

The version in the tables is therefore the floor. The lock resolves the newest release inside the caret range that clears the cooldown, so `Cargo.lock` may hold a newer compatible release. For 0.x crates the range is the minor version (`rust_xlsxwriter` 0.99.1 allows 0.99.x only), so a minor bump is always an edit to this document.

### Named bans

`deny.toml` bans these crates. The aim is to let IT be told, and to check, that no crate in the build graph compiles C code, that the silent build's graph holds no network crate, and that the only network crates in the AI build are the wrapped ones named below. Only the card `net-stack-spike` edits `deny.toml`.

Today the file and the matching `BANNED` list in `xtask/src/graph.rs` hold one unconditional set of thirteen crates, and `native-tls`, `ureq` and `rustls` are banned outright with the rest. The rows below that say *planned* describe the change that `net-stack-spike` would make in its own pull request; they are not today's state, and nothing here changes `deny.toml`, the graph check, the tool pins or the bootstrap.

| Banned | Why |
|---|---|
| `cc` | Drives a C compiler. Banned today; allowed only as a dependency of `embed-resource`, through a `wrappers` entry, which is a planned change owned by the first R2 lock card, which records here what `cc` is used for |
| `cmake` | Builds C and C++ libraries |
| `bindgen` | Generates bindings to C libraries and needs libclang |
| `pkg-config` | Finds system C libraries, so the build would depend on the host |
| `openssl-sys` | Binds the C OpenSSL library |
| `libsqlite3-sys` | Compiles or binds C SQLite |
| `tokio` | An async runtime. A blocking client and a hand-written loopback listener are enough (AD-01) |
| `reqwest`, `hyper`, `curl` | HTTP clients and servers with an async runtime or C |
| `native-tls` | Binds OpenSSL on Linux. Banned unconditionally today. The planned change would keep the ban as the default and allow it only if the spike falls back to schannel on Windows; that is `net-stack-spike`'s decision, not a current exemption |
| `ring`, `aws-lc-rs`, `aws-lc-sys`, `openssl` | Planned: TLS providers that would build C or assembly through `cc` or CMake, added to the ban when that stack arrives |
| `ureq` | Planned: wrapped, allowed only as a dependency of `fieldbook`, under the feature `net` (and `net-pull` once its card exists). It would be the one HTTPS client: the model gate and, later, a `Pull` adapter over HTTPS through `GuardedPull` use it. A second client, or another feature that reaches it, needs a card whose Review is `owner`, a row here and an edit to `deny.toml` by the network-stack card |
| `rustls` | Planned: wrapped, allowed only through `ureq`, `rustls-graviola`, `rustls-platform-verifier` and `fieldbook` |

A model endpoint needs TLS. The proposed stack uses rustls with the pure-Rust graviola provider and the operating system's trust store; the spike must confirm its build graph, and until it lands the thirteen-entry ban stands unchanged.

Features that bring C code into the build graph are off: never enable `rust_xlsxwriter`'s `zlib` feature, which turns on a C zlib through `zip`.

### Checks and their schedule

| Run | Checks | When |
|---|---|---|
| `cargo xtask deps --check` | DEPS-1: every direct dependency has a row here with the same name and pin. DEPS-2: every pin in `[workspace.dependencies]` equals its row. DEPS-3: direct and transitive crates are within the budget and the cap, once per profile. DEPS-4: every workflow `uses:` is `owner/repo@<40 hex>` and has a row. DEPS-5: every tool in `.config/tools.toml` has a row and a pin | Every pull request, in `just check` |
| `cargo deny check bans licenses sources` | Bans, licences, sources. Offline. Runs twice, with default features and with `--all-features` | Every pull request, in `just check` |
| `just lock-verify` | Replays the recorded resolution mode, bounded registry inputs and approved adjustments at the cutoff in `.config/tools.toml` and fails if the lock differs, or if the cutoff is less than 14 days older than the head commit | Every pull request that changes `Cargo.lock` or a manifest, and weekly |
| `cargo deny check advisories` | Known advisories | Weekly, and before each release |
| Patch deadlines | An advisory on the CISA known-exploited list is patched within 14 days; a high-severity one within 30 | Owner |

### The cooldown

The cooldown is applied when the lockfile is resolved, not by a network check afterwards. `just lock` regenerates `Cargo.lock` so that it contains no release younger than 14 days:

```sh
cargo +<nightly> generate-lockfile -Zunstable-options \
  --publish-time "$(date -u -d '14 days ago' +%Y-%m-%dT%H:%M:%SZ)"
```

- `<nightly>` is the pin in `.config/tools.toml`, section `[lock]`: `nightly-2026-10-08`. The recipe installs it with the minimal profile. `just lock` also writes the cutoff it used into the same section as `cutoff`, and writes the replay record below; `just lock-verify` uses its recorded mode, resolver and cutoff.
- The nightly only resolves the lockfile. It never builds, tests or checks anything.
- The date command is GNU `date`. `just lock` runs at home and in CI, both on Linux.
- Everything else that builds uses the stable toolchain from `rust-toolchain.toml`.
- From Rust 1.100 the stable key `[registry] global-min-publish-age = "14 days"` in `.cargo/config.toml` replaces the nightly. The steps are in [toolchain-bump.md](../runbooks/toolchain-bump.md).

**Tools, actions and the toolchain.** These have no lockfile. Whoever writes the card that adds or bumps one reads the release date from the project's release page and uses the newest release that is at least 14 days old. For Rust the rule applies to the minor release, and the pin is that minor's newest point release (AD-02). Dependabot covers GitHub Actions only, monthly, with its own 14-day `cooldown`. Crates change only through `just lock`.

**If `--publish-time` fails.** The first lock card confirms that the flag and its time format work on the pinned nightly. If either does not, that card:

1. Resolves the lockfile with the stable toolchain (`cargo generate-lockfile`).
2. Reads the publish date of every `Cargo.lock` entry from crates.io, and no other host.
3. Lists every entry younger than 14 days in the table [Young lockfile entries](#young-lockfile-entries), with the date it clears.
4. For each, moves to an older release with `cargo update -p <crate> --precise <version>` where one satisfies the requirements, and otherwise leaves it listed.
5. Puts the result in the pull request.

Each later lock refresh repeats steps 1 to 5 until the flag works.

**The security-fix exception.** A release younger than 14 days may be used to fix a security advisory, with the owner's approval in the pull request. Apply it with `cargo update -p <crate> --precise <version>` after `just lock`, and add a row to [Cooldown exceptions](#cooldown-exceptions). `just lock` regenerates the whole lockfile and would drop the young release again, so the exception is part of that resolution’s ordered replay adjustments. It remains recorded when time advances. A new resolution may omit it only when the release qualifies at the new cutoff; never remove it merely to verify an old resolution.

**Building the lock tool.** Normal `just lock` builds or reuses xtask from an isolated copy of a known consistent committed baseline using that baseline's pinned stable toolchain and `cargo build --frozen -p xtask`. Its tool-source and locked-graph provenance must match before reuse. It runs the resulting helper directly against the invocation workspace; edited target manifests never participate in bootstrapping. Missing baseline inputs, dependencies or local toolchain refuse with exit 2, without implicit online resolution or target mutation. Bootstrap files/cache are separate from the capture cache and never enter its snapshot. While initially introducing the capture command, build the new serde_json/std helper before editing manifests; that ignored binary is preparation evidence, not a delivered artifact or the normal post-merge recipe.

**Recorded resolution and replay.** `[lock]` in `.config/tools.toml` holds `replay_schema = 1`, `mode` (`publish-time` or `stable-fallback`), exact `resolver`, `cutoff`, `inputs_sha256`, `snapshot` (repository-relative path) and `snapshot_sha256`. The input digest covers sorted manifest paths/bytes and Cargo resolution configuration, excluding the replay record itself. `snapshot` names an immutable public registry-cache archive under `.config/lock-inputs/`, containing only the crates.io index entries and candidate crate archives consulted by this resolution, with their original registry identity, checksums and publish dates. Capture the initial stable candidates before precise downgrades. Exclude credentials and all unrelated Cargo-home files. A bounded manifest lists each cache path and full hash; reject traversal, unknown entries and mismatches. The same snapshot supplies both resolution modes, including precise security overrides.

`[[lock.adjustment]]` rows are ordered and contain `package` (name plus old version), `version`, and `reason` (`cooldown` or `security`). A security row also requires the advisory id and the owner-approval commit plus repository-relative approval-record path/hash. The public record says only that the exception was approved for this input digest, candidate/version, cutoff and advisory; it contains no person or tracker address. The owner adds that approval in the resolution pull request. Verification checks the approval bytes and binding; an adjustment without them fails.

`just lock-verify` uses an empty temporary checkout and isolated Cargo cache, installs only the recorded snapshot inputs and exact resolver, and replays the recorded generate-lockfile command offline followed by the ordered precise updates offline. It never seeds the run with the candidate Cargo.lock, uses today's registry, or chooses a mode by trying flags again. Publish-time uses the recorded cutoff; fallback uses its retained initial candidates and downgrade list. Residual young entries retain the existing fallback table rule. Compare the resulting lock byte for byte and check the cutoff is at least 14 days before HEAD's committer time. Missing replay inputs/resolver refuse, never silently re-resolve. A fresh resolution captures a fresh snapshot; PR and weekly checks use the identical replay command. This is dependency automation in xtask, not a product network path.

**The monthly lock refresh.** A `lock-refresh` card runs once a month. It is `auto` when the checks below pass, and the owner reads it in the week's merged work. The card:

1. Runs `just lock` and reads the `Cargo.lock` diff: new crates, jumps in major or minor version, and any new direct crate. A direct crate that is not in this document stops the card.
2. Runs `cargo deny check bans licenses sources` and `cargo deny check advisories`, then `cargo xtask deps --check`, `just lock-verify` and `just check`.
3. Counts direct and transitive crates for each profile and adds a row to [Measurements](#measurements). A transitive count above the cap of its profile stops the card until the owner decides.
4. Opens a pull request. When it also bumps a pin in this document, both change together. A new direct crate past the 15th of the AI build makes the card one whose Review is `owner`.

### Planned replacements

Each of these removes a moving part. The trigger is in AD-24, and the steps are in [toolchain-bump.md](../runbooks/toolchain-bump.md).

| Today | Replaced by | Trigger |
|---|---|---|
| The nightly in `just lock` | `[registry] global-min-publish-age = "14 days"` in `.cargo/config.toml` | Rust 1.100 is pinned |
| `--remap-path-prefix` flags for the workspace, vendor and `CARGO_HOME` paths (R2) | `trim-paths` | Rust 1.101 is pinned |
| `rust_xlsxwriter` 0.99.1, `calamine` 0.36.1 and `zip` 8.6.0 | `rust_xlsxwriter` 0.101.x, `calamine` 0.37.x (the first on `zip` 9) and `zip` 9.0.5 or later, re-verified; all three move in one pull request, because the graph holds one `zip` | A Day-30 card; all are old enough by then |
| `cargo-cyclonedx` | Another SBOM generator | IT requires CycloneDX 1.6, or `cargo-cyclonedx` goes 12 months without a release |
| `rustc` 1.99.1 | The newest minor at least 14 days old | At most every 12 weeks, as its own card. The next candidate is 1.100, after Day 30 |

## Shipped runtime crates

All come from crates.io. The two tables below are one table split for width: the first holds the pins, the second the reasons.

### Pins

| Crate | Version (released) | Features | Used by | Licence | MSRV |
|---|---|---|---|---|---|
| `serde` | 1.0.229 (2026-07-18) | `derive` | calc, core, render, fieldbook | MIT OR Apache-2.0 | 1.56 |
| `toml` | 1.1.8+spec-1.1.0 (2026-10-09) | default | core, xtask | MIT OR Apache-2.0 | 1.85 |
| `csv` | 1.4.0 (2025-10-17) | default | core, render | Unlicense OR MIT | 1.73 |
| `pulldown-cmark` | 0.13.4 (2026-05-20) | `default-features = false`, `html` | core, render | MIT | 1.71.1 |
| `unicode-normalization` | 0.1.25 (2025-10-30) | default | core | MIT OR Apache-2.0 | 1.36 |
| `sha2` | 0.11.0 (2026-03-25) | `default-features = false` | core, xtask | MIT OR Apache-2.0 | 1.85 |
| `rust_decimal` | 1.43.0 (2026-09-02) | `default-features = false`, `std` | calc | MIT | 1.67.1 |
| `askama` | 0.16.1 (2026-09-04) | `default-features = false`, `derive`, `std` | render | MIT OR Apache-2.0 | 1.88 |
| `serde_json` | 1.0.151 (2026-07-20) | default | render, fieldbook, xtask | MIT OR Apache-2.0 | 1.71 |
| `clap` | 4.6.7 (2026-09-14) | `default-features = false`, `std`, `derive`, `help`, `usage`, `error-context`, `suggestions` | fieldbook | MIT OR Apache-2.0 | 1.85 |
| `rust_xlsxwriter` | 0.99.1 (2026-09-14) | none (the default set is empty); never `zlib` | render | MIT OR Apache-2.0 | 1.88 |
| `calamine` (`extract-xlsx-sheets`) | 0.36.1 (2026-07-27) | `default-features = false`; never `chrono`, `dates` or `picture` | core | MIT | 1.88 |
| `zip` (`extract-xlsx-sheets`) | 8.6.0 (2026-04-25) | `default-features = false`, `deflate` | core, xtask; render (tests) | MIT | 1.88 |
| `embed-resource` (R2) | 3.0.12 (2026-10-06) | default | `fieldbook` `build.rs`, Windows only | MIT | 1.76 |
| `windows-sys` (`windows-facts`) | 0.61.2 (2025-10-06) | `default-features = false`, `Win32_Foundation`, `Win32_Security`, `Win32_Security_Authorization`, `Win32_Storage_FileSystem`, `Win32_Storage_CloudFilters`, `Win32_System_Com`, `Win32_System_RemoteDesktop`, `Win32_System_Threading`, `Win32_UI_Shell` | `fieldbook-win`, in `src/sys.rs`; Windows only | MIT OR Apache-2.0 | 1.71 |
| `object` (`core-policy`) | 0.40.0 (2026-08-01) | `default-features = false`, `read_core`, `elf`, `pe` (the build settles the list) | `xtask`, in `binscan`; not shipped | MIT OR Apache-2.0 | 1.85 |
| `ureq` (AI build) | 3.4.2 (proposed; lands with `net-stack-spike`) | `rustls-no-provider`, `platform-verifier`; optional | `fieldbook`, in `io/net`; AI build only | Confirmed by the spike | Confirmed by the spike |
| `rustls` (AI build) | 0.23.45 (proposed; lands with `net-stack-spike`) | `std`, `tls12`, `custom-provider`; optional | `fieldbook`, in `io/net`; AI build only | Confirmed by the spike | Confirmed by the spike |
| `rustls-graviola` (AI build) | 0.4.0 with `graviola` 0.4.1 (proposed; lands with `net-stack-spike`) | optional | `fieldbook`, in `io/net`; AI build only | Confirmed by the spike | Confirmed by the spike |

`sha2` 0.11 output has no `{:x}` formatting: use `fieldbook_core::hex::lower`. `askama` 0.16 custom filters use `#[askama::filter_fn]`. `rust_xlsxwriter` 0.99.1 brings `zip ^8.3` with only `deflate`, and `calamine` 0.36.1 needs `zip ^8.6`, which is why `zip` is pinned at 8.6.0 above and the graph holds one `zip`. `calamine` 0.37.0 is the first release on `zip` 9, which `rust-xlsxwriter-bump` moves to. `object` is the one row here that no shipped crate uses: only `xtask` (`binscan`, from `core-policy`) depends on it, so it is not among the 12, 14 and 17 below and `cargo xtask deps --check` does not count it as a direct shipped crate.

That is 12 direct shipped crates at Day 1 in the silent build, 14 with the two rows marked `extract-xlsx-sheets` and still 14 at R2; one of the 12 is `windows-sys`, which only `fieldbook-win` uses. The AI build adds the three proposed rows: 17, past the review trigger of 15 (a direct crate of the AI build past the 15th needs a card whose Review is `owner`, which `extract-xlsx-sheets` is). The three rows are proposals until `net-stack-spike` has run: the card proves the stack on Linux and Windows, repeats the transitive counts, edits `deny.toml` and moves the rows out of "proposed". Proposed pins: `ureq` 3.4.2, `rustls` 0.23.45, `rustls-graviola` 0.4.0 (`graviola` 0.4.1) in Cargo.toml, plus transitive `rustls-platform-verifier` 0.7.1 via `ureq`, all to be confirmed at the spike; `just lock` decides the real pins.

The source of the Bridge token's random bytes is decided by `net-stack-spike`. A `getrandom` row is added by that card only if it chooses the crate, and increases the measured runtime count beyond the listed 17; it is not already included. `embed-resource` is a build dependency and adds no direct shipped crate.

### Justification

| Crate | Why it is needed | Alternatives considered | Maintainer health |
|---|---|---|---|
| `serde` | The typed schema for front matter, register rows and test cases. The serde types are the schema (AD-09) | No practical alternative; hand-written parsers rejected | An individual and a publish team own it; the most widely used crate in the graph |
| `toml` | Front matter, `corpus.toml`, test cases. Its `display` support lets `new` write valid TOML | `toml_edit` (keeps comments but is larger and not serde-first); `basic-toml` (reduced feature set). `fmt` never rewrites front matter, so losing comments is accepted | A team owns it, under the same organisation as `toml_edit`; frequent releases |
| `csv` | RFC 4180 registers read and written with serde | `csv-core` (lower level); a hand parser (quoting rules are easy to get wrong); `polars` (far larger) | One owner; mature and stable; releases are rare by design |
| `pulldown-cmark` | CommonMark with source offsets and the broken-link callback that reads citations | `comrak` (rejected: larger tree, more dependencies); `markdown` (a different event model) | Three owners; steady releases |
| `unicode-normalization` | NFC for `fmt` and paths, NFKC for the tokenizer and the quote match | `icu_normalizer` (needs far more data and code); no hand-written substitute is safe | Five owners in the `unicode-rs` organisation; slow, steady updates that follow Unicode |
| `sha2` | SHA-256 of originals, editions and allowances, the audit journal chain and the backup manifests | `ring` (C and assembly); a hand-written SHA-256 (an audit burden); the `sha256` wrapper crate | Maintained by the RustCrypto organisation team; active |
| `rust_decimal` | Exact decimal arithmetic and rounding strategies for money (AD-14) | `bigdecimal` (arbitrary precision, heavier); `fastnum`; integer minor units (rejected in AD-14) | One owner; steady releases |
| `askama` | Compile-checked HTML templates with escaping | `maud` (macros instead of template files); `tera` and `minijinja` (templates checked at run time); a generic site generator (AD-12) | An organisation team with several owners; active |
| `serde_json` | Search index and Handoff payloads, `--json` reports, golden vectors; in `fieldbook` also model requests and replies, MCP messages and the chat Bridge's bodies | `miniserde` (fewer features); a hand-written writer (escaping bugs) | The same owners as `serde`; very active |
| `clap` | CLI parsing, help, the generated CLI reference, and the test that parses every command chip | `lexopt` and `pico-args` (no derive, no generated help); `argh`; `bpaf` | Two teams own it; very active |
| `rust_xlsxwriter` | `registers.xlsx` and the templates, written deterministically with a fixed creation time | `umya-spreadsheet` (reads and writes, heavier); hand-written OOXML over `zip` (fidelity risk); `calamine` (a reader, used for `xlsx-sheets`; see its row) | One owner, who also wrote the Python library it follows. The risk is a single maintainer |
| `calamine` (`extract-xlsx-sheets`) | The reader of the `xlsx-sheets` method. It gives the value a `.xlsx` or `.xlsm` workbook stored in each cell, one cell at a time, and says whether the cell has a formula, without evaluating one, opening a macro project or following a link. Pure Rust, no feature on | `umya-spreadsheet` (reads and writes; far larger); a reader written over `zip` (shared strings, date styles and shared formulas are easy to get wrong, and it would be a second parser to review); `rust_xlsxwriter` (writes only) | Three owners; a release every one to two months since mid-2025; first published in 2016. It parses files from outside, so the card bounds the file, what it unpacks and the cells it reads, and fuzzes the extractor |
| `zip` (`extract-xlsx-sheets`) | Opens the package of a workbook to count what it inflates to and to read its calculation setting and its link parts, which `calamine` does not expose. The same crate writes the edition zips (`xtask`) and `rust_xlsxwriter` brings it, so one version serves all three | A hand-written zip reader (an attack surface of its own) | Two owners; major versions arrive often, so the pin is exact in intent |
| `embed-resource` (R2) | VERSIONINFO and the manifest of `fieldbook.exe`. It is the only crate that may bring in `cc` | `winres` and `winresource`; a hand-written `build.rs` calling `rc.exe` | One owner; small and stable. It runs only at build time, only on Windows |
| `windows-sys` (`windows-facts`) | The Windows facts the program reads and no other crate may: the known folders, the logon account, group membership, interactive logons, who can write to a folder, reparse, sync-root and network places, and the not-indexed mark. Raw bindings with no build script and no C. Only `fieldbook-win` uses it, and the first-party `unsafe` code is in one file of that crate, `src/sys.rs` | `windows` (safe wrappers, but a much larger graph); `winapi` (no longer maintained); hand-written `extern` blocks (the same unsafe code, with the link names written by hand); the environment variables `USERNAME` and `ProgramData` (PL-03 forbids that route, and a variable is not the process token) | The `windows-rs` project of Microsoft; several owners; frequent releases. It brings one crate, `windows-link` |
| `object` (`core-policy`) | Reads the imported function names of a built ELF or PE for `xtask binscan`, which fails a build that imports a network, process-start or clipboard call. Only `xtask` uses it; no shipped crate depends on it. With the features of the Pins row its only dependency is `memchr`, which `zip` and `pulldown-cmark` already bring | `goblin` (a larger graph); one crate for each format, such as `elf` and `pelite` (two parsers to review); a hand-written reader of the import tables (a parser for one scan, with faults of its own) | Two owners; a release every few months, 0.40.0 in August 2026. It reads the binary of our own build, not input from outside |
| `ureq` (AI build, proposed) | A blocking HTTPS client for one request and one reply; no async runtime. In `io/net` only, behind feature `net` | `reqwest` and `hyper` (async, larger, C paths); `curl` (C); `minreq` (a smaller feature set, no platform certificate store); a hand-written client (TLS and HTTP framing are not written in-house) | A small team; frequent releases. To be confirmed at the spike |
| `rustls` (AI build, proposed) | TLS in Rust with no C of its own. In `io/net` only, behind feature `net` | `native-tls` (OpenSSL on Linux); `openssl` (C) | A funded team; frequent releases |
| `rustls-graviola` (AI build, proposed) | The crypto provider with no C and no `cc`. In `io/net` only, behind feature `net` | `ring` and `aws-lc-rs` (C and assembly through `cc` or CMake); `rustls-rustcrypto` (experimental) | A single maintainer; young. The spike runs it on Windows MSVC, with schannel on `cfg(windows)` as the fallback |

Maintainer health was read from the crates.io owner lists and release histories when the pins were set. Re-read it at each lock refresh and change a row when it has changed.

### Transitive crates worth knowing

| Crate | Brought by | Note |
|---|---|---|
| `flate2` 1.1.10 | `zip` with `deflate` | Uses the `zlib-rs` backend |
| `zlib-rs` 0.6.8 | `flate2` | Pure Rust; licence Zlib, on the allowlist |
| `zopfli` | `zip` with `deflate` | Pure Rust; Apache-2.0. The lock takes 0.8.3 because 0.8.4 was younger than 14 days when pinned |
| `crc32fast`, `memchr` | `zip` (`memchr` also `pulldown-cmark`, and `object` in `xtask`) | Pure Rust |
| `indexmap` | `zip` | Arrives through `zip`; not a direct dependency (see Rejected) |
| `typed-path` | `zip` | Its licence for the version the lock takes was not verified when the pins were set. `cargo deny check licenses` decides at the first lock card |
| `quick-xml`, `encoding_rs`, `codepage`, `atoi_simd`, `fast-float2`, `byteorder`, `log` | `calamine` | The cell reader's parsers (about nine crates in all, some of them perhaps in the graph already). Pure Rust. `cargo deny check licenses` decides their licences at the lock of `extract-xlsx-sheets` |
| `rustls-platform-verifier` 0.7.1 | `ureq`, through its `platform-verifier` feature; AI build only | Checks server certificates against the operating system's store, so a TLS-inspecting gateway that IT trusts works. Not named in `Cargo.toml`. One of the crates that `deny.toml` lets depend on `rustls` |
| `windows-link` | `windows-sys`; Windows target only | Raw link declarations; pure Rust, no build script |

## Test-only and dev-only crates

None of these is shipped. They are not counted in the budget of 15.

| Crate | Version (released) | Features | Where | Licence | MSRV | Why | Maintainer health |
|---|---|---|---|---|---|---|---|
| `insta` | 1.49.0 (2026-10-03) | `default-features = false` | dev-dependency | Apache-2.0 | 1.66 | Snapshot tests of pages and diagnostics, without `cargo-insta` | Two owners; active |
| `tempfile` | 3.27.0 (2026-03-11) | default | dev-dependency | MIT OR Apache-2.0 | 1.63 | Temporary trees for CLI tests and negative controls | One owner; very widely used, stable |
| `proptest` (Day 30) | 1.12.x: the newest release at least 14 days old (1.12.0 was released 2026-10-10) | `default-features = false`, `std` | dev-dependency | MIT OR Apache-2.0 | 1.88 | Properties for `fieldbook-calc` and the tokenizer; the Bridge `read_request` property test (P5) | An organisation team plus two owners; steady |

`zip` is not in this table: `fieldbook-core` uses it for `xlsx-sheets`, so it has a row under Pins. `xtask` (the deterministic edition zips) and the render tests (the xlsx read-back) use the same version. The default `zip` features pull in many compressors and an encryption stack; `default-features = false` with `deflate` keeps the graph to pure Rust.

## Developer tools

Pins and checksums live in `.config/tools.toml`. `cargo xtask setup` installs them, from outside the repository, with `cargo install --locked`. The one line in the README is `cargo install --locked just@1.58.0`. `gitleaks` is a release binary whose SHA-256 is checked before it is installed. poppler is the exception: it comes from the operating system's package manager and is pinned and checked, not installed, by xtask.

| Tool | Version (released) | Joins | Note |
|---|---|---|---|
| Rust toolchain | 1.99.1 (2026-10-15) | Now | Pinned in `rust-toolchain.toml` with `rustfmt` and `clippy` |
| Rust nightly | nightly-2026-10-08 | Now | `just lock` only; it resolves and never builds. Retired when 1.100 is pinned |
| `just` | 1.58.0 (2026-08-03) | Now | 1.59.0 was younger than 14 days when pinned |
| `cargo-nextest` | 0.9.148 (2026-10-08) | Now | 0.9.149 was younger than 14 days when pinned |
| `cargo-deny` | 0.20.2 (2026-07-09) | Now | |
| `typos-cli` | 1.51.1 (2026-10-06) | Now | |
| `gitleaks` | 8.30.1 (2026-03-21) | Now | Release binary, SHA-256 checked; `install-action` has no manifest for it |
| poppler (`pdftotext`, `pdfinfo`) | 26.10.0 (2026-10-04) | Card `extract-and-locators` | Home only, from the operating system's package manager, never in CI. `pdftotext` makes the page text and `pdfinfo` reads page counts in the home drill. Pinned in `.config/tools.toml` under `[poppler]`; `extract` records the version it ran in `extraction.toml`. On a different version it warns, naming both on standard error, extracts anyway and records the version it used; `check` still fails when a quote no longer matches its page text |
| Headless Microsoft Edge | The version on the GitHub-hosted runner image | Card `edge-harness` | Runs the page harness. Preinstalled on the runners, so not installed by us and not pinned |
| `proptest` | See above | Day 30 | |
| `cargo-cyclonedx` | 0.5.9 (2026-03-19) | Day 60 to 75 | Emits CycloneDX 1.3 to 1.5, so pass `--spec-version 1.5` |
| `cargo-about` | 0.9.2 (2026-08-18) | Day 60 to 75 | Licence notices |
| `cargo-mutants` | 27.1.0 (2026-06-02) | P5 for the gate, Day 90 for the report | A gate on the label, policy, egress and authorisation modules of `fieldbook-core`, with an owner-reviewed skip file for mutants that change no behaviour, and a report only on `fieldbook-calc` (`mutation-report`) |
| `cargo-auditable` | 0.7.7 (2026-10-02) | R2 | Embeds the dependency list in the exe |
| BinSkim | 4.4.9.13 (2026-10-06) | R2 | Checks the Windows binary |
| `cargo-fuzz` | 0.13.2 (2026-06-09) | R2 | Bounded PR smoke and scheduled citation and Bridge parser runs |
| Rust nightly for fuzzing | nightly-2026-10-08 | R2 | Separate `[fuzz] toolchain` pin; builds only the fuzz targets |

axe-core joins at Day 90 as a test-only tool; this document gains its row, with a version, in that card.

### Agent aids outside the repository

Some tools help the coding agent or the owner and never touch the product. They live in the agent's own configuration on the owner's machine, or they are reference for a row of the Later table in AD-24 (the table in [ADR 0001](../decisions/0001-foundation.md#ad-24-what-is-not-built-yet-later)). Nothing about them is installed or committed here, and they are not dependencies: the dependency checks and the "row in the same pull request" rule do not apply to them. Three limits hold. They add no code, crate or file to the product, an edition or CI. A `.serena/` folder is never committed. Nothing an agent aid produces counts as evidence: the repository's checks decide.

Status is one of: **used now** (a dev aid in use at home), **candidate adapter** (a tool that could sit behind a port on its trigger, run as a separate local service or process, never linked), **candidate for work after IT**, **reference only** (ideas, never a dependency) or **replaced**. A candidate names its port and the slug of the Later row that schedules it. Code that a licence bars (GPL, AGPL, noncommercial, unlicensed) is reference only and never a dependency.

| Tool | Status | Role and plug point | At home now / at work after IT |
|---|---|---|---|
| Serena | Used now | Symbol navigation for the coding agent | At home, not shipped. The application is GPL-3.0-or-later and is never linked. AGENTS.md tells the agent to use its symbol tools and never to commit a `.serena/` folder |
| Understand-Anything | Used now, on trial | A repository orientation graph for the agent | At home, on the Rust workspace. Removed if it shows no gain under SM-01 in ADR 0001 |
| OpenUI | Used now | Design time only, through the `openui-design` skill; never shipped | At home, for design |
| The shared agent skills (`codebase-design`, `domain-modeling`, `tdd`, `openui-design`) | Used now | Design, domain modelling, test-first work and interface design, listed in AGENTS.md | At home, in the agent's own configuration |
| A public agent-skills collection | Replaced | The owner's shared skills cover the same ground | At home, as the shared skills |
| Paperclip | Reference only | Agent dispatch | A pilot only when queue wait is the longest part of card time for two weeks |
| OpenViking | Reference only | None. AGPL: never linked, copied or shipped; at most a consumer of `fieldbook mcp` | At home only, as a separate service |
| WeKnora | Candidate adapter | `IndexPort`, `Classifier` and `Correlator`, as a local service over loopback (`later-semantic-search`) | At home now; its six trial checks become the conformance cases. At work after IT approves a runtime |
| System One Adapter | Candidate adapter | `DecidePort` over `ModelPort` (`later-decision-adapters`) | At home once the Model port exists (MIT) |
| Jev/TypeSafe | Candidate adapter | `DecidePort` (`later-decision-adapters`) | After its hosted terms are checked and an endpoint is approved |
| invoice2data, Docling | Candidate adapters | `Extractor` in `xtask`, writing the staging folder (`later-docx-import`, `later-pdf-native-ocr`) | At home after a measured gap, in `xtask`. On a company device: the program's own `Extractor` methods (core, no new crate) or a reviewed adapter under the same port that writes into the company store's `sources\text\`, on a card whose Review is `owner` (`later-pdf-native-ocr`) |
| openpyxl | Replaced | Replaced by the core method `xlsx-sheets` (`extract-xlsx-sheets`), which reads a workbook inside the program, at home and on a company device | Reference |
| Company data-source connectors | Candidate for work after IT | A `Pull` adapter in the port registry (`later-company-source-read`), behind `GuardedPull`, built only with the feature `net-pull`, which its own card adds. A connector that drives a web form and not a supported interface is not used as a base | The company approves each connection and names a service account. Credentials come from the Windows credential store through the `Secrets` port, and are never in a repository, a file or the company store |
| Laravel, Filament | Candidate for work after IT | The workspace application (`later-workspace-ui`) | Reference now; a candidate when the workspace application is scheduled |
| Metabase | Candidate for work after IT | A feed over the owner's own data (`later-dashboards`) | Reference now. AGPL: a separate service on a read-only role |
| XlsxWriter | Replaced | Replaced by `rust_xlsxwriter` | Reference |
| Finance workflow prompts (Apache-2.0) | Reference only | The finance modules (`later-finance-eac-evm`, `later-finance-cash-irr`, `later-finance-ma`) | Deferred until a finance module is scheduled |
| EVM-analysis, M&A-analysis | Reference only | None. No licence; ideas only. The EVM sample's defect is the calculation case CALC-04 | |
| ERPNext, MIS Builder, cost-core | Reference only | None. GPL, AGPL and noncommercial terms bar linking or shipping | |
| awesome-dashboard | Reference only | None. No blanket licence; ideas only | |
| Other finance libraries | Reference only | The finance modules, when scheduled | Deferred until their module |

An adapter that comes from this table meets the conformance suite of its port, keeps the sources of what it returns, and has every model call in the audit journal. AGPL and GPL tools run as separate local services or processes and never as linked code.

## CI actions

Every action is pinned by commit SHA. The workflow line reads `uses: owner/name@<sha> # <version>`, and the SHA must be the commit the version tag points to. The workflow token is read-only.

| Action | Version (released) | Commit SHA | Note |
|---|---|---|---|
| `actions/checkout` | v7.0.1 (2026-07-20) | `3d3c42e5aac5ba805825da76410c181273ba90b1` | |
| `Swatinem/rust-cache` | v2.9.2 (2026-08-06) | `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` | v2.9.3 was younger than 14 days when pinned. Saves the cache on `main` only |
| `taiki-e/install-action` | v2.87.27 (2026-10-09) | `ab6895351a88b908e96a4697860d4ffea5e57dbe` | Later releases were younger than 14 days when pinned |
| `actions/upload-artifact` | v7.0.2 (2026-10-07) | `cf430e030ddbb5b0abf93d22962f4752f3646cd9` | |
| `actions/download-artifact` | v8.0.2 (2026-10-07) | `9000827ccba6bdab643e8b6fd33ac0654aef8333` | v8 fails on a digest mismatch |

There are no artifact attestations: private repositories need Enterprise Cloud for them.

## In-house instead of a crate

Each of these is small, has a test, and removes a review line for IT.

| What | Where | Replaces |
|---|---|---|
| `CivilDate`, `PartialDate`, `EffectivePeriod` and `Duration`: ISO parse, compare, add days, partial dates (`YYYY`, `YYYY-MM`) that never fill in the missing parts, and business days over a calendar that is passed in as data. No clock is read. `fieldbook-calc` owns them, because a calculation function takes a date or a period, and `fieldbook-core` re-exports them. Accounting periods and working-day calendars for EAC and EVM are data and a Later row (`later-calendar`) and widen no type | `fieldbook-calc` | `jiff`, `chrono` |
| A decimal series with a fixed number of terms for a power or a logarithm (a learning curve, fractional-year escalation, a dated rate of return), and the tolerance each case states. No float is used | `fieldbook-calc` | The maths feature of `rust_decimal`, `f64` |
| A diagnostic `Code` enum and hand-written diagnostics (`path:line:col: error[FB0102]: message`) | `fieldbook-core` | `thiserror`, `anyhow`, `miette`, `ariadne` |
| Hand-written matchers for scans, locators and the citation lexer | `fieldbook-core` | `regex` |
| A sorted `read_dir` walk | `fieldbook` (`io` module) | `walkdir` |
| Polling every 500 ms for `build --watch` (Day 30) | `fieldbook` (`io` module) | `notify`, `watchexec` |
| `hex::lower`, lower-case hexadecimal for digests | `fieldbook-core` | `hex` |
| The tokenizer and BM25F ranking in [search.md](search.md) | `fieldbook-core` | `tantivy`, `lunr`, SQLite FTS5 |
| A hash-chained audit journal, about 15 lines on `sha2` (Day 1) | `fieldbook-core` (chain), `fieldbook` (file) | A logging crate |
| A single-writer lock on the state folder, `std::fs::File::lock` (Day 1), behind the `Store` trait (`folder@1`), so a later shared store replaces the lock and not its callers. The journal chain is per writer. Record-level concurrency for several users waits for `later-shared-store` | `fieldbook` | `fs2`, `fd-lock` |
| The chat Bridge: a loopback listener over `std::net` with fixed limits (AI build with `bridge`, Day 90) | `fieldbook`, `io/net/bridge.rs` | `hyper`, `tokio` |
| `fieldbook mcp`: a synchronous JSON-RPC loop over standard input and output, about 400 lines on `serde_json` (Day 30) | `fieldbook`, feature `mcp` | An MCP SDK |

## Rejected crates

| Rejected | Instead | Why |
|---|---|---|
| `jiff`, `chrono` | `CivilDate` | Only civil dates are needed, and none is read from the clock for output |
| `thiserror`, `anyhow`, `miette`, `ariadne` | Hand-written diagnostics | Diagnostics have a fixed format and stable codes |
| `regex` | Hand-written matchers | The shapes are few and fixed; no engine to review |
| `walkdir`; `notify` | Sorted `read_dir`; polling | Output order must be deterministic; no background processes |
| `schemars` | Generated docs | The serde types are the schema until a second implementation reads the Corpus |
| `scraper` | Tests at the source | No HTML parser dependency; axe-core joins at Day 90 |
| `comrak` | `pulldown-cmark` | Smaller tree, source offsets, fewer dependencies |
| `rusqlite`, `tantivy` | The in-memory index | Reopen on the AD-24 trigger `later-sqlite-tantivy`: search p95 above 100 ms on the laptop, an edition index above 3 MB, a company browser working set above 24 MB, building the in-memory company index in more than 5 s, or a full `check` above 60 s. The persistent adapter would sit behind `IndexPort` |
| `tracing`; `rayon` | None | Diagnostics are the output; no measured need |
| `indexmap` as a direct dependency | `BTreeMap` | Ordered output; `indexmap` still arrives through `zip` |
| `tokio` | A blocking client | A blocking client and a hand-written loopback listener are enough |
| An MCP SDK | `fieldbook mcp` | Not used. `fieldbook mcp` is a synchronous JSON-RPC loop of about 400 lines on `serde_json` |
| `reqwest`, `hyper` | `ureq` | Async HTTP stacks that need `tokio` |
| `curl` | `ureq` | Binds the C library libcurl |
| `ring`, `aws-lc-rs` | `rustls-graviola` | TLS providers that build C or assembly through `cc` or CMake. Their build tools (cc and cmake) are banned by name in `deny.toml` |
| `webpki-roots` | The operating system's certificate store | Bundled roots would ignore the inspection root that IT installs |
| `rustls-rustcrypto` | `rustls-graviola` | Experimental |

## Measurements

Card `day1-dependencies-checks` adds measured baseline row 0 without changing the lock. Card `day1-dependencies-3` retains that row and adds expanded-graph row 1 after its first recorded `just lock`; each later `lock-refresh` card adds the next row. The card `net-stack-spike` adds the first AI-profile row, and the card `core-policy` adds one silent-profile row when it adds `object`. A row gives one profile, so a refresh adds one row for the silent build and one for the AI build. The counts follow [Counting crates](#counting-crates). The rows are not edited afterwards.

| Lock refresh | Profile | Features | Direct runtime crates | Transitive crates | Notes |
|---|---|---|---|---|---|

## Young lockfile entries

Used only when `--publish-time` fails (see [The cooldown](#the-cooldown)). Empty while the flag works.

| Crate | Version | Released | Clears on |
|---|---|---|---|

## Cooldown exceptions

One row for each security fix adopted before it was 14 days old. Empty until one is adopted.

| Crate | Version | Advisory | Approved in | Dropped when |
|---|---|---|---|---|
