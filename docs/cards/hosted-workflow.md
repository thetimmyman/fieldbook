# hosted-workflow

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

A merge to `main`, or a manual run from `main`, builds the edition, checks the host's limits, probes that the hostname asks for a login, uploads the site to the Worker, probes again, and fails on any step that does not hold. A manual run can instead roll the Worker back to an earlier version.

## Why

- This is the workflow of [ADR 0007](../decisions/0007-hosted-edition.md), `hosted.yml`, with the controls the record lists ([Controls and evidence](../decisions/0007-hosted-edition.md#controls-and-evidence)): the deploy job declares the GitHub environment `hosted`, runs only on an ephemeral GitHub-hosted runner, and is the only workflow that names the two deploy secrets.
- Review is `owner` because the card adds a workflow, repository secrets and a CI permission, which are the release gate ([cards README](README.md#the-three-header-fields)). The owner reads the workflow, the environment's secrets and branch policy, and the deploy token's permissions before merging; an agent never merges it.
- The deploy tool and the headers are `hosted-deploy`; the probe is `hosted-gate`. Controls HO-04 to HO-06 of [controls.md](../controls.md#hosted-edition) are built here.
- The deploy needs a GitHub-hosted runner, as the record says. While CI-01's temporary departure keeps those runners away from the repository ([controls.md](../controls.md)), the workflow is merged and does not run; nothing deploys from the self-hosted runner.
- The Node and `actions/setup-node` rows of [dependencies.md](../reference/dependencies.md#deployment-tools) are the exception the record narrows: Node runs only in the deploy step of this workflow.

## Scope

- Files: `.github/workflows/hosted.yml`, `xtask/tests/hosted_workflow.rs`.
- Registration lines: `docs/controls.md` (rows HO-04, HO-05 and HO-06 in the Hosted edition section), `docs/reference/dependencies.md` (the Node row of Deployment tools and the `actions/setup-node` row of CI actions), `docs/plan/PLAN.md` (this card's line).
- Docs to update: the files named under Registration lines.

## Out of scope

- Creating the GitHub environment, its secrets, its branch policy and the repository variable `FIELDBOOK_HOST`; the owner sets them and reads them back.
- The runbook, the `AGENTS.md` link and the changelog line (`hosted-runbook`).
- Creating the Worker, the custom domain, the login application or the reader group.
- Splitting the claims register (`claims-register-split`); until it merges the limits step refuses the first deploy.
- Any change to `ci.yml`.

## Preconditions

- Cards merged: `hosted-deploy`, `hosted-gate`. The ADR branch `hosted-edition-adr` merges first.

## Steps

1. `.github/workflows/hosted.yml`: on push to `main` and on `workflow_dispatch` with an optional `rollback_to` version id. Read-only token; one deploy at a time, never cancelled; the job `if: github.ref == 'refs/heads/main'`, `environment: hosted`, `runs-on: ubuntu-24.04`. Steps in order: checkout (pinned by SHA), rollback-input check, configuration check, a search of the tree for the values of `FIELDBOOK_HOST` and `CLOUDFLARE_ACCOUNT_ID` that prints nothing, build as `just site` does, stage `out/hosted`, `cargo xtask hosted-limits`, install the deploy tool with `npm ci --ignore-scripts`, the login probe, the deploy or the rollback, and the login probe again.
2. `xtask/tests/hosted_workflow.rs`: the workflow is pinned, read-only and serialised; the deploy job declares `hosted` and runs on a GitHub-hosted label (HO-05); no other workflow declares `hosted` or names a deploy secret (HO-04); no file of `.github/` or `deploy/` holds an identifier shape (HO-06). Each has a planted-violation test. Run `cargo nextest run --locked -p xtask --test hosted_workflow`.
3. The controls rows and the dependency rows.
4. Delete this card file in the last commit.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test hosted_workflow
(cd deploy && npm ci --ignore-scripts && npx wrangler --version)
just check
```

Expected: every command exits 0 and `wrangler --version` prints `4.153.0`.

## Negative control

- Secret use and Environment (HO-04): a planted `environment: hosted` or a planted `CLOUDFLARE_API_TOKEN` in another workflow fails the test and names the file.
- Runner (HO-05): the self-hosted labels of `ci.yml` in `hosted.yml` fail the test.
- No identifiers (HO-06): a planted `workers.dev` label, an Access team domain, a run of exactly 32 hex digits and an email address each fail the check.

## Rollback

Revert the merge. Delete the `hosted` environment's secrets in the repository settings if the deploy is withdrawn; the Worker keeps serving its last version until the owner detaches the custom domain or deletes it, as the record says.

## BLOCKED

Stop if the workflow would need a secret at repository level, a permission beyond `contents: read`, or a runner that persists between jobs. Comment `BLOCKED: workflow needs a wider grant` and push the branch.
