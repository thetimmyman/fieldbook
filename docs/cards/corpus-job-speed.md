# corpus-job-speed

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The `corpus` job of `ci.yml` finishes in at most half its measured time alone (35 to 43 minutes) on a self-hosted runner, and it still runs every check it runs today on every page and every renderable original.

## Why

- The cost. Alone, the job takes 35 to 43 minutes: the site build about 12 and the browser harness about 23, over 1,894 pages, one browser process after another (`xtask/src/harness.rs`). With two self-hosted runners and one `corpus` job per pull request and per push to `main`, merging a queue of cards one after another takes hours.
- Two places to win, neither dropping a check. The binary the job builds the site with is the debug build of the `rust` job (`cargo build --locked --package fieldbook`); a release build of the same commit renders the same bytes ([AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types), Determinism) in less time. The harness can open pages in several browser processes at once, each with its own profile folder and net-log, and judge each page as it does now.
- `Review: auto`: the card changes job steps and xtask code only. It changes no `permissions`, secret, runner label or action pin, and adds no job; any of those stops it (BLOCKED). [Adding a CI job](README.md#adding-a-ci-job) does not apply.
- Decisions: [AD-08](../decisions/0001-foundation.md#ad-08-ci-design) (the CI design), CI-01 in [controls.md](../controls.md) (self-hosted runners only for trusted code).
- Design: none.

## Scope

- Files: `xtask/src/harness.rs` (its tests are in its `mod tests`), `docs/reference/testing.md` (the harness timing line, if it names one).
- Registration lines: `.github/workflows/ci.yml`, the build step of the `rust` job and the steps of the `corpus` job only.
- Docs to update: `docs/reference/testing.md` as above.

## Out of scope

- Moving any job to a GitHub-hosted runner, or adding a runner.
- Fewer pages, fewer originals, a sample of pages, or a weaker net-log judgement.
- The deploy workflow `hosted.yml`.

## Preconditions

- Cards merged: `runner-contention` (it changes the `corpus` job), `narrow-width-check` (it changes `harness.rs`).

## Steps

1. Measure on `main`, alone on one runner: each step of the `corpus` job from the job log, and in the harness the time per page and the time to start a browser. Record the table in the pull request.
2. In `harness.rs`, open pages in `--jobs N` browser processes (default: half the available cores, at least 1), each with its own temporary profile and net-log. Collect the findings and print them in page order, so the output does not depend on N. Tests in `mod tests`: `findings_are_in_page_order_for_any_job_count` and `one_failing_page_fails_the_run_with_many_jobs`.
3. In `ci.yml`, build the binary of the `fieldbook-dev` artifact with `--release`, or add a release binary beside it if another job needs the debug build. Keep the `Check the Corpus` step first.
4. Measure again as in Step 1, alone and beside another run. If the job is not at most half its measured time alone, say why in the pull request and stop (BLOCKED).

## Acceptance

```sh
cargo nextest run --locked -p xtask harness::
just site
cargo xtask harness --jobs 4
just check
```

Expected: the tests pass; `just site` exits 0; the harness exits 0 and its summary line counts every page and every renderable original, the same counts as a run with `--jobs 1`; `just check` exits 0. The pull request links a `corpus` job run within the target.

## Negative control

Plant a page that loads a remote URL in the copy the harness opens (the harness's negative page): the run with `--jobs 4` must fail and name the page, as the run with `--jobs 1` does. Make the collector print findings in completion order: `findings_are_in_page_order_for_any_job_count` must fail. Restore it.

## Rollback

Revert the merge commit. The harness opens one page at a time and the job builds the site with the debug binary again.

## BLOCKED

The change would need a `permissions` change, a secret, a new action, a new job or another runner, or the target is not met without dropping a check: comment `BLOCKED: <what>` on the work item with the Step 4 table, push the branch and take the next card.
