# scheduled-workflow

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

`.github/workflows/scheduled.yml` runs once a week and on `workflow_dispatch`, with a read-only token. It runs `cargo deny check advisories`, standard committed-lock Cargo/`just check`, gitleaks over the full history, the lint controls, the scale run and, when `corpus/changes.md` has an `## edition-` heading, an edition build whose hash lists it uploads as an artifact for the cross-OS compare. The lint controls plant, one at a time, a `std::fs::read` call, a call of each of `f64::ln`, `powf`, `powi` and `exp`, a `HashMap` type, a float multiply, an index into a slice and an `unsafe` block, and clippy must fail on each, and a test of `clippy.toml` names any ban that is gone; a dependency from a git source must fail `cargo deny check sources`; a planted token must be reported by gitleaks. The scale run (`cargo xtask scale`) is advisory and fails nothing. A synthetic lockfile with an advised crate version fails the advisory check, and a test proves it; each lint control has a test of the same kind.

## Why

- Owner review covers binding the packaging contact secret; no secret value is stored in the repository.

- The weekly jobs are in [AD-08](../decisions/0001-foundation.md#ad-08-ci-design) (`scheduled.yml`). The actions, their SHAs and the tool pins are in [CI actions](../reference/dependencies.md#ci-actions) and [Developer tools](../reference/dependencies.md#developer-tools); the advisory schedule is in [Checks and their schedule](../reference/dependencies.md#checks-and-their-schedule).
- The weekly run checks the committed lock with stable Cargo and the existing checks; it does not resolve a new lock or claim recorded-cutoff/snapshot replay ([Dependencies and the lock file](README.md#dependencies-and-the-lock-file)).
- Controls brought here: SC-01 (the weekly full-history run and the planted-token test), DP-01 (a git-source dependency fails cargo-deny), DP-05 (advisories and the fixture lockfile), NS-02, CD-01 and DT-04 (the weekly lint controls) and the Linux half of DT-02, all in [controls.md](../controls.md). NS-02, CD-01 and DP-01 are built by `workspace-skeleton` and had no test that plants their fault; this card builds it, and their rows name this card in Brought by.
- The full Windows run and the cross-OS compare itself join with `transfer-and-windows-drill`. The freshness report waits for `status-command`: [AD-08](../decisions/0001-foundation.md#ad-08-ci-design) defines it as the stale-items list of `fieldbook status`, and that card adds the step to `scheduled.yml`.
- The `scale` job runs `cargo xtask scale` ([scale-fixture](scale-fixture.md)) and uploads its table. It is advisory: the card that builds the command says why, and a figure never fails the job.
- Trade-off: the lint controls run weekly, not on every pull request, because each plant is a full clippy run. A ban deleted from `clippy.toml` or from a lint table is found within a week and shows in the diff of the pull request that deleted it.

## Scope

- Files: `.github/workflows/scheduled.yml`, `xtask/tests/advisories_fixture/` (a standalone `Cargo.toml` with its own empty `[workspace]`, `Cargo.lock` and `src/lib.rs`; never built), `xtask/tests/scheduled_controls.rs`.
- Docs to update: none. The rows SC-01, DP-01, DP-05, NS-02, CD-01, DT-04 and DT-02 in `docs/controls.md` already name this card.

## Out of scope

- The `windows` job, the full Windows run and the DT-02 compare (`transfer-and-windows-drill`).
- The freshness report (`status-command`).
- Any change to `ci.yml`, `deny.toml` or the lock.
- Any change to `clippy.toml` or to the lint tables of the crates: the controls read them and fail when a ban is gone.
- The figures and the measuring code of the scale run (`scale-fixture`).

## Preconditions

- Cards merged: `day1-dependencies-3` (the committed dependency declarations/lock), `calc-day1` (the calc crate the lint controls plant into), `scale-fixture` (`cargo xtask scale`), `edition-zips` (`cargo xtask edition`), `edge-harness` (`cargo xtask harness` writes the network-silence summary that `cargo xtask edition` needs).
- May run beside the critical path, including `search-js`, `search-palette` and `guard-statuses`: its paths are disjoint from theirs.

## Steps

1. Write `scheduled.yml`: `on: schedule` (one weekly cron), `workflow_dispatch`, and `pull_request` with a `paths` filter on `scheduled.yml`, the fixture folder, `xtask/tests/scheduled_controls.rs` and `clippy.toml`, so a pull request that changes any of them gets a run, this card's included. `permissions: contents: read`. Only the actions and SHAs of dependencies.md; `persist-credentials: false`; `ubuntu-24.04`.
2. Job `advisories`: `timeout-minutes: 20`, checkout with `fetch-depth: 0` and `persist-credentials: false`, install the pinned Rust toolchain, then install `cargo-deny` and `cargo-nextest` at their pins, run `cargo deny --locked check advisories`, then `cargo nextest run --locked -p xtask --test scheduled_controls --run-ignored only advisories_fixture_fails_deny advisories_fixture_git_source_fails_deny`.
3. In `xtask/tests/advisories_fixture/`, pin one crate at a version with a published RustSec advisory, chosen so nothing builds a C library. The lockfile is synthetic and marked so in a comment in `Cargo.toml`.
4. In `xtask/tests/scheduled_controls.rs`, add the `#[ignore]` test `advisories_fixture_fails_deny`. It runs `cargo deny --manifest-path xtask/tests/advisories_fixture/Cargo.toml check advisories`, expects a non-zero exit and an advisory id in the output, and names the crate. It is ignored because it needs the network; `just check` stays offline. Put the line `// control: DP-05` above its `fn`. Add three more `#[ignore]` tests to the file, each with its own marker line above the `fn`. `advisories_fixture_git_source_fails_deny` (`// control: DP-01`) makes a temporary git repository that holds a one-file crate, writes a temporary manifest that depends on it with `git = "file://<temporary path>"`, runs `cargo deny --manifest-path <that manifest> --config deny.toml check sources` from the repository root, and expects a non-zero exit and the git source in the output; it is ignored because it spawns `cargo` and `cargo-deny`, which the weekly job has. `gitleaks_reports_a_planted_token` (`// control: SC-01`) makes a temporary git repository with one committed file that holds a synthetic token of a form the rules of `.gitleaks.toml` match (a GitHub personal access token shape), assembled at run time from two halves so that no file of this repository holds it whole, runs gitleaks with the arguments of the `_gitleaks` recipe over that repository (read from the `justfile`, so one copy exists), and expects a non-zero exit and the rule id in the output. `lint_bans_fail_clippy` is step 6.
5. Job `secrets`: checkout with `fetch-depth: 0`, `cargo xtask setup --no-hooks gitleaks`, then `FIELDBOOK_BASE=0000000000000000000000000000000000000000 just _gitleaks`, which scans every commit, then `cargo-nextest` at its pin and `cargo nextest run --locked -p xtask --run-ignored only gitleaks_reports_a_planted_token`.
6. Job `lint-controls`: checkout, the pinned toolchain, `cargo-nextest` at its pin, then `cargo nextest run --locked -p xtask --run-ignored only lint_bans_fail_clippy`. In `xtask/tests/scheduled_controls.rs` add that `#[ignore]` test, with the line `// control: NS-02, CD-01, DT-04` above its `fn`. For each plant below it appends the text to `crates/fieldbook-calc/src/lib.rs` (a drop guard restores the file even on a panic), runs `cargo clippy --locked -p fieldbook-calc --message-format=json -- -D warnings` once, and requires a non-zero exit and a diagnostic that carries the lint code. The plants, each appended exactly as written, with the lint it must give: `pub fn plant_fs_read() { let _ = std::fs::read("x"); }`, `pub fn plant_ln(x: f64) -> f64 { x.ln() }`, `pub fn plant_powf(x: f64, y: f64) -> f64 { x.powf(y) }`, `pub fn plant_powi(x: f64) -> f64 { x.powi(2) }` and `pub fn plant_exp(x: f64) -> f64 { x.exp() }` give `clippy::disallowed_methods` (the last four are the bans of `search-core-and-cli`); `pub fn plant_map(m: Option<std::collections::HashMap<u8, u8>>) -> usize { m.map_or(0, |m| m.len()) }` gives `clippy::disallowed_types`; `pub fn plant_float(a: f64, b: f64) -> f64 { a * b }` gives `clippy::float_arithmetic`; `pub fn plant_index(v: &[u8]) -> u8 { v[0] }` gives `clippy::indexing_slicing`; `pub fn plant_unsafe() { unsafe {} }` gives `unsafe_code`. A plant that passes fails the test with "the <name> plant passed clippy", where `<name>` is the function name without `plant_`. The test also reads `clippy.toml` and fails, naming the entry, when it lacks any of `std::fs::read`, `std::process::Command`, `std::net::TcpStream`, `std::env::var`, `std::time::SystemTime::now`, `std::process::exit`, `std::collections::HashMap`, `f64::ln`, `f64::powf`, `f64::powi`, `f64::exp`, `rust_decimal::Decimal::round` and `rust_decimal::Decimal::round_dp`.
7. Job `lock`: checkout with `fetch-depth: 0`, install the pinned stable toolchain and the ordinary tools, run `cargo build --workspace --locked` and `just check`, and fail on a nonzero exit. It neither updates the lock nor installs a custom verifier; exact snapshot replay remains deferred.
8. Job `edition-linux` runs only when `grep -q "^## edition-" corpus/changes.md` succeeds; otherwise the job ends green with the notice `no edition to build` and uploads nothing. Its first step is the checkout of `ci.yml` (the same pinned action, with `fetch-depth: 0` and `persist-credentials: false`, and no `if`), because the next step reads `corpus/changes.md`. Its second step has `id: heading`, runs the `grep`, writes `run=true` to `$GITHUB_OUTPUT` when the `grep` succeeds and otherwise prints `::notice::no edition to build`; every step after the second has `if: steps.heading.outputs.run == 'true'`. When it runs: run `cargo xtask edition`, which owns the stamped build, harness and packaging in that order, then upload both complete edition/originals zips and their published `out/dist/SHA256SUMS` in the one artifact `edition-hashes-linux` (`retention-days: 14`). The comparison must reopen and verify package bytes; hash lists alone are insufficient. The `windows` job of `transfer-and-windows-drill` uploads `edition-hashes-windows` in the same layout.
9. Job `scale`: `timeout-minutes: 30`, checkout, the pinned toolchain, a step with `shell: bash` (so a pipe fails when its first command fails) that runs `cargo xtask scale | tee scale-table.txt`, and an upload of `scale-table.txt` as the artifact `scale-table` (`retention-days: 14`). The job fails only when the command cannot run, never on a figure.
10. In `xtask/tests/scheduled_controls.rs` add `edition_job_is_guarded_by_the_heading` (not ignored; it reads the workflow text): in every job of `scheduled.yml` that has a step with `id: heading` (`edition-linux`, and the `windows` and `compare` jobs that `transfer-and-windows-drill` adds) the first step is the checkout of `ci.yml` with no `if`, the second step is `heading` (it runs `grep -q "^## edition-" corpus/changes.md` and prints `no edition to build` otherwise), and every step after the second has `if: steps.heading.outputs.run == 'true'`. It also runs that `grep` over two temporary `changes.md` files, one with an `## edition-001` heading (exit 0) and one without (exit 1). Add `edition_artifact_contains_both_packages_and_sums` over the workflow upload definition; a hash-list-only artifact fails because comparison must verify its bytes.

11. In each of the separate `secrets` and `lock` jobs, install `just@1.58.0` through the already SHA-pinned `taiki-e/install-action` before its first `just` invocation. The filtered gitleaks setup does not install it. A nonignored workflow test checks both jobs, exact version and ordering; removing either installation fails.

12. Extend the edition workflow test to require one edition operation and no separate site build/harness invocation, while retaining the checkout and heading guard. Inserting either extra invocation must fail it.

13. Bind `FIELDBOOK_SECURITY_CONTACT` from the owner-provisioned same-named repository secret in every packaging step this card adds, including scheduled, release and Windows calls as applicable. Forward the environment through child edition/intake commands. Tests use synthetic contact text, verify the workflow binding and require IN-01 for missing/blank input; do not print or commit the real value. No token permission changes are needed.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test scheduled_controls --run-ignored only advisories_fixture_fails_deny advisories_fixture_git_source_fails_deny lint_bans_fail_clippy gitleaks_reports_a_planted_token
cargo nextest run --locked -p xtask --test scheduled_controls edition_job_is_guarded_by_the_heading
gh run list --workflow scheduled.yml --branch card/scheduled-workflow --limit 1
just check
```

Expected: the ignored tests run at home with the network and pass, because `cargo deny` fails on the fixture and on the git source, clippy fails on each plant and gitleaks reports the planted token; the pull request's `scheduled.yml` run is `completed success` with all six jobs green and the artifact `scale-table` attached (while `corpus/changes.md` has no `## edition-` heading the `edition-linux` job ends green with the notice `no edition to build` and attaches nothing; with a heading it builds that edition and attaches `edition-hashes-linux`); `edition_job_is_guarded_by_the_heading` passes; `just check` exits 0 and still runs no test that makes a request. With a `changes.md` that has no `## edition-` heading the workflow ends green and publishes nothing; with one it builds that edition.

## Negative control

Change the fixture's pin to the first version the advisory names as patched. The ignored test must fail with "cargo deny passed on the advisory fixture". Restore the pin. Then delete `std::fs::read` from `clippy.toml` on the branch and push: the `lint-controls` job must fail and name the missing entry. Restore it. Then delete `f64::exp` from `clippy.toml` the same way: the job must fail and name that entry. Restore it. Then remove `float_arithmetic` from the calc crate's lint table and push: the job must fail with "the float plant passed clippy". Restore the lint. Then add the temporary repository's `file://` URL to `sources.allow-git` in `deny.toml` in a scratch run: `advisories_fixture_git_source_fails_deny` must fail with "cargo deny passed on a git source". Restore `deny.toml`. Last, shorten the planted token by one character in a scratch run: `gitleaks_reports_a_planted_token` must fail with "gitleaks did not report the planted token". Restore the test. Then remove the `if:` line from the `cargo xtask edition` step of `edition-linux`: `edition_job_is_guarded_by_the_heading` must fail. Restore it. Then move the checkout step of `edition-linux` after its `heading` step: `edition_job_is_guarded_by_the_heading` must fail. Restore it.

Remove either zip from the upload definition: `edition_artifact_contains_both_packages_and_sums` must fail. Restore it.

## Rollback

Revert the merge commit. The weekly run stops; SC-01, DP-01, DP-05, NS-02, CD-01 and DT-04 fall back to their PR CI parts until it returns, and the `scale` table is not produced.

## BLOCKED

`cargo deny check advisories` reports an advisory against the real lock. Write the advisory id and the crate that pulls it in, and stop; the lead decides whether to bump the crate or record an exception. A plant passes clippy on the first run because `clippy.toml` or a lint table already lacks the ban: write which plant and stop; the lead restores the ban, because it is a control.
