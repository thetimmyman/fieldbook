# hosted-runbook

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

[docs/runbooks/hosted-edition.md](../runbooks/hosted-edition.md) tells the owner how to deploy the hosted edition, verify it, roll it back, add a reader and rotate the deploy token, and what never to do. `AGENTS.md` links it, and the changelog says that the owner can now publish the edition behind a login.

## Why

- The procedures of [ADR 0007](../decisions/0007-hosted-edition.md): the owner reads the live headers once after the first deploy, repeats the hostname controls after an Access change, and rolls back or detaches the custom domain by hand when a gate fails. They belong in one runbook, not in the workflow.
- Review is `auto`: the card changes documentation only and grants nothing. It describes the workflow of `hosted-workflow`, which is owner-reviewed.
- The runbook names no host, account, zone, team, email or date; it refers to the environment secrets `FIELDBOOK_HOST`, `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`, "the owner's zone" and "the reader group".

## Scope

- Files: `docs/runbooks/hosted-edition.md`.
- Registration lines: `AGENTS.md` (Commands: the link to the runbook), `CHANGELOG.md` (one line under `## Unreleased`), `docs/plan/PLAN.md` (this card's line).
- Docs to update: the files named under Registration lines.

## Out of scope

- Any change to the workflow, the headers or the controls register.
- Creating the Worker, the custom domain, the login application or the reader group.

## Preconditions

- Cards merged: `hosted-workflow`.

## Steps

1. Write the runbook: what the hosted edition is and where each part lives (the three secrets of the environment `hosted`, the self-hosted runner and its mitigations, the edge-rewriting rule as a provider setting the owner keeps), deploy, verify (the gate step, a signed-in read, the three unauthenticated probes, the edge-rewriting rule after any change of the zone's settings), roll back (with the note that a push to `main` can replace a queued rollback dispatch in the concurrency group), add a reader, rotate the token, update `wrangler` (lock regeneration with npm's `--before` set 14 days back), check the headers locally, and what never to do. Every `wrangler` command is `npm ci --ignore-scripts` first, then `WRANGLER_SEND_METRICS=false npx --no-install wrangler`, never a bare `npx`.
2. Add the `AGENTS.md` link and the `CHANGELOG.md` line.
3. Delete this card file in the last commit.

## Acceptance

```sh
just check
```

Expected: exit 0. Every link of the runbook resolves to a file of the repository.

## Negative control

None for a runbook: the documentation changes are checked by `just check` (typos, the docs check) and read by the owner. Say so in the pull request.

## Rollback

Revert the merge. Nothing else depends on the runbook.

## BLOCKED

Stop if a step of the runbook needs an identifier to be written into the repository. Comment `BLOCKED: runbook needs an identifier` and push the branch.
