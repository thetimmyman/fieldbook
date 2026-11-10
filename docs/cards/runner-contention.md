# runner-contention

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

A deploy and a CI run that start together on the self-hosted runners both finish: the deploy job installs no tool into the runner's shared home folder, and the `corpus` job of `ci.yml` has room to finish when two of them run at once.

## Why

- The race. The two self-hosted runners share one home folder. `taiki-e/install-action` writes its tools to one folder there, and both `ci.yml` (job `rust`) and `hosted.yml` call it. A deploy that started while a CI job was installing `just` failed at that step with curl error 23 (a write that could not complete); nothing was deployed, and a re-run passed. The runbook ([hosted-edition.md](../runbooks/hosted-edition.md), section 2) records the re-run as the remedy until this card merges.
- What the deploy needs from `just`. `hosted.yml` uses `just` for `just site` alone, whose recipe in the `justfile` is one command: `SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)" cargo run -q --locked -p fieldbook -- build --out out`. The deploy can run that command itself and install nothing.
- The margin. The `corpus` job takes 35 to 42 minutes on its own against a 45-minute `timeout-minutes`. When two run at once (a pull request and a push to `main`, or two pull requests) both have timed out, and `ci-ok` failed on a pull request whose change was sound.
- Decisions: [AD-08](../decisions/0001-foundation.md#ad-08-ci-design) (the CI design), [ADR 0007](../decisions/0007-hosted-edition.md) (the deploy workflow and its controls HO-04 to HO-09 in [controls.md](../controls.md)). The card changes two workflows, so its Review is `owner`.

## Scope

- Files: `.github/workflows/hosted.yml`, `.github/workflows/ci.yml`, `xtask/tests/hosted_workflow.rs`, `docs/runbooks/hosted-edition.md`.
- Docs to update: the runbook's runner notes (section 2) lose the re-run note and say the deploy installs no tool.

## Out of scope

- Moving any job to a GitHub-hosted runner: CI-01 stands.
- A third runner, or a runner with its own home folder: that is a host change, not a repository change.
- Making the `corpus` job faster.

## Preconditions

- Cards merged: `hosted-workflow`, `hosted-runbook`.

## Steps

1. In `hosted.yml`, remove the `taiki-e/install-action` step and replace `just site` with the recipe's command. A test in `hosted_workflow.rs`, `the_deploy_builds_with_the_site_recipe`, reads the `site` recipe from the `justfile` and the build step from `hosted.yml` and fails when the two commands differ; a second test, `the_deploy_installs_no_tool`, fails when `hosted.yml` names `install-action` or `cargo install`.
2. In `ci.yml`, raise the `corpus` job's `timeout-minutes` from 45 to 75. Record in the pull request the job's duration on a run alone and on a run beside another.
3. Update the runbook as Scope says.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test hosted_workflow
just check
```

Expected: the tests pass and `just check` exits 0. The first deploy from `main` after the merge passes; the pull request names the run.

## Negative control

Put the `install-action` step back in `hosted.yml`: `the_deploy_installs_no_tool` must fail. Change the build step's `--out out` to `--out site`: `the_deploy_builds_with_the_site_recipe` must fail. Restore both.

## Rollback

Revert the merge commit. The deploy installs `just` again and the race returns; the `corpus` job's limit returns to 45 minutes.
