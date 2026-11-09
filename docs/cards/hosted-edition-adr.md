# hosted-edition-adr

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

ADR 0007, `docs/decisions/0007-hosted-edition.md`, exists with Status Proposed and decides the hosted read-only edition: what it is, what it may hold, who can read it, its controls and how it is undone. The owner accepts it in the pull request.

## Why

- The owner reads the Corpus at work without the package, so a sign-on-gated hosted copy of the site needs a decision before any workflow deploys it. A new ADR number is reserved in [PLAN.md](../plan/PLAN.md) before its ADR is written.
- It must square with the offline and network rules of [AD-01](../decisions/0001-foundation.md#ad-01-architecture) and [AD-18](../decisions/0001-foundation.md#ad-18-security-and-confidentiality-guardrails), the workflows of [AD-08](../decisions/0001-foundation.md#ad-08-ci-design), the Never table of [AD-24](../decisions/0001-foundation.md#ad-24-what-is-not-built-yet-later) and the `edition` destination of [ADR 0004](../decisions/0004-company-boundary.md#6-destinations).
- Review is `owner`: the record decides a deployment that uses secrets and a new workflow (the release gate), and an ADR changes only by the owner's acceptance ([Reopen triggers](../decisions/0001-foundation.md#reopen-triggers)).

## Scope

- Files: `docs/decisions/0007-hosted-edition.md` (new), `docs/cards/hosted-edition-adr.md`.
- Registration lines: `docs/plan/PLAN.md` (this card's line under P3 and the 0007 sentence of the Reserved ADR numbers paragraph); `docs/decisions/0001-foundation.md` (one `hosted.yml` bullet in the AD-08 specification); `AGENTS.md` (ADR 0007 in the Decisions line of Where things live); `README.md` (ADR 0007 in the `docs/decisions/` row).
- Docs to update: `AGENTS.md`, `README.md` and `docs/decisions/0001-foundation.md`, the lines named above.

## Out of scope

- The deploy workflow, its wrangler pin and lock, the response-header and robots files, the pre-deploy limit check and the post-deploy probe: a later card with Review `owner` builds them under this record.
- The status strip that says on every page that the build is unreleased and its claims unchecked: a planned card of its own.
- Splitting the claims register (`claims-register-split`), which the per-file limit needs before a first deploy.
- Any Cloudflare or GitHub setting. The infrastructure is the owner's and the lead's, and no identifier of it enters the repository.

## Preconditions

- Cards merged: none. The record describes the site that `fieldbook build` already writes.

## Steps

1. Reserve 0007 in the Reserved ADR numbers paragraph of PLAN.md and add this card's line under P3.
2. Write `docs/decisions/0007-hosted-edition.md` in the shape of the existing records: Status Proposed, Context, Decision, Boundary, Readers, Honest status, Controls and evidence, Consequences, Alternatives considered. Cite each AD it squares with by an anchor that exists.
3. Add the one AD-08 bullet in ADR 0001, and ADR 0007 to the decisions lists of `AGENTS.md` and `README.md`.
4. Check that no host name, account or zone id, workers.dev subdomain, Access team name or email address is in the change.
5. The last commit deletes this card file.

## Acceptance

```sh
git diff --name-only origin/main...HEAD
```

Expected: exit 0, and only the files of Scope.

```sh
just check
```

Expected: exit 0, and step 7 prints no FB error.

## Negative control

Write a temporary denylist with one made-up term, stage a copy of the record with that term planted in it, and run `cargo xtask denylist <temporary list>`: it must exit 1 and name the object. Unstage and delete the planted copy, and the same command exits 0.

## Rollback

Revert the merge. The record, its PLAN.md line and the index lines go; 0007 stays reserved, and nothing is deployed by this card.

## BLOCKED

Stop if the record needs an identifier of the owner's infrastructure in a repository file, or a change to a gate it does not name. Comment `BLOCKED: hosted-edition-adr needs <what>` on the work item.
