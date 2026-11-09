# hosted-deploy

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The repository holds the pinned deploy tool, the host configuration and the response headers for the hosted edition, and `cargo xtask hosted-limits DIR` fails on a staged site the host would refuse: a file over 25 MiB or more than 20,000 files.

## Why

- The hosted edition is [ADR 0007](../decisions/0007-hosted-edition.md), which another branch adds and the owner accepts first. This card is the files-and-limits side of it; the login-gate check is `hosted-gate` and the workflow is `hosted-workflow`. The hosted site holds the public edition only; no company data goes to it ([ADR 0004](../decisions/0004-company-boundary.md)).
- Review is `auto`: the card adds no workflow, secret, token or permission. It adds one tool pin with its row in [dependencies.md](../reference/dependencies.md#deployment-tools), which an `auto` card may do under [AGENTS.md](../../AGENTS.md). The pin is read only by the workflow card.
- The headers are the table of ADR 0007 (Content-Security-Policy as `CSP_STATIC` in `crates/fieldbook-render/src/csp.rs` plus `frame-ancestors 'none'`, which a `<meta>` policy cannot carry). A header can only narrow a page's own policy, so no page, the search page or the Ctrl K palette changes.
- The cooldown and the review evidence are in [dependencies.md](../reference/dependencies.md#the-cooldown). `wrangler` is a deploy-time tool and is never in the product, an edition or a Cargo graph.
- Process code lives only in `xtask` ([AGENTS.md](../../AGENTS.md)), so the size check is `cargo xtask hosted-limits`.
- Controls: the Limits and Headers rows of [controls.md](../controls.md#hosted-edition), HO-01 and HO-02.

## Scope

- Files: `deploy/{package.json,package-lock.json,wrangler.jsonc,_headers,robots.txt}`, `xtask/src/{main.rs,hosted.rs}` (the unit tests are in `mod tests` of `hosted.rs`), `xtask/tests/hosted_config.rs`, `.gitignore`.
- Registration lines: `AGENTS.md` (Commands: the `cargo xtask hosted-limits DIR` entry), `docs/reference/dependencies.md` (the Deployment tools table and its `wrangler` row), `docs/controls.md` (the Hosted edition section with rows HO-01 and HO-02), `docs/plan/PLAN.md` (this card's line).
- Docs to update: the files named under Registration lines.

## Out of scope

- The decision record `docs/decisions/0007-hosted-edition.md`, which is another branch's.
- The login-gate check (`hosted-gate`), the workflow, the runbook and the changelog line (`hosted-workflow`).
- Splitting the claims register (`claims-register-split`), which brings the edition under the per-file limit; until it merges, the limits check fails on the register page.
- Creating the Worker, the custom domain, the login application or the reader group. They exist in the owner's account and no file here names them.

## Preconditions

- Cards merged: `edge-harness`. The ADR branch `hosted-edition-adr` merges before this card.

## Steps

1. `deploy/package.json` and `deploy/package-lock.json`: `wrangler` pinned exactly, lockfile version 3 with integrity hashes. Where a transitive entry was younger than 14 days, `overrides` pin an older compatible release. Add `deploy/node_modules/` and `deploy/.wrangler/` to `.gitignore`.
2. `xtask/src/hosted.rs` and its registration in `main.rs`: `cargo xtask hosted-limits DIR` fails when a file is over 25 MiB or the tree has more than 20,000 files, and prints the five largest files. Tests in `mod tests`; run `cargo nextest run --locked -p xtask hosted::`. Mark the negative control `// control: HO-01`.
3. `deploy/wrangler.jsonc`, `deploy/_headers` and `deploy/robots.txt`: Worker `fieldbook`, assets only, directory `../out/hosted`, `workers_dev` and `preview_urls` false, no routes and no account id; the header table of ADR 0007; `Disallow: /`.
4. `xtask/tests/hosted_config.rs`: reads the committed header file and the configuration; run `cargo nextest run --locked -p xtask --test hosted_config`. Mark the header test `// control: HO-02`.
5. The `AGENTS.md`, `dependencies.md`, `controls.md` and plan lines. Delete this card file in the last commit.

## Acceptance

```sh
cargo nextest run --locked -p xtask hosted::
cargo nextest run --locked -p xtask --test hosted_config
(cd deploy && npm ci --ignore-scripts && npx wrangler --version)
(cd deploy && npx wrangler deploy --dry-run --outdir "$TMPDIR/hosted-deploy/dry")
just check
```

Expected: every command exits 0; `wrangler --version` prints `4.153.0`; the dry run lists the asset count and writes no Worker bundle that holds an account id. A local `wrangler dev` on a staged copy of a small built site shows each header of `deploy/_headers` on a page response.

## Negative control

- Limits (HO-01): a file of 25 MiB plus one byte makes `cargo xtask hosted-limits` exit 1 and name the file; a file of exactly 25 MiB passes.
- Headers (HO-02): remove one header line from `deploy/_headers`; `--test hosted_config` fails and names the header. Restore the line. Separately, `wrangler dev` on a staged site without `_headers` shows none of the headers on `curl -I`; with it they are present.

## Rollback

Revert the merge. Nothing deploys from this card, so nothing in the owner's account changes.

## BLOCKED

Stop if `wrangler` cannot be pinned without a package younger than 14 days that no override can replace, or if a header would break a page of the edition. Comment `BLOCKED: deploy tool pin` or `BLOCKED: header breaks a page`, push the branch and take the next card.
