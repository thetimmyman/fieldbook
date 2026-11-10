# Hosted edition runbook

How to deploy, verify, roll back and maintain the hosted public edition of Fieldbook. The decision behind it is [ADR 0007](../decisions/0007-hosted-edition.md), and the company boundary is [ADR 0004](../decisions/0004-company-boundary.md). The site serves public material only and sits behind Cloudflare Access for the reader group.

## 1. What the hosted edition is

The hosted edition serves the public edition (the same static pages `just site` builds, plus the originals folder, which the source pages link as `originals/` one level up, so on the site it sits at the root) as static assets from a Cloudflare Worker named `fieldbook`, behind Cloudflare Access (a login) for the reader group. It holds public material only; never company data ([ADR 0004](../decisions/0004-company-boundary.md) and [ADR 0007](../decisions/0007-hosted-edition.md)).

| Component | Where it lives |
|---|---|
| Worker assets configuration | [`deploy/wrangler.jsonc`](../../deploy/wrangler.jsonc) |
| Hardened response headers | [`deploy/_headers`](../../deploy/_headers) |
| Crawl disallow rules | [`deploy/robots.txt`](../../deploy/robots.txt) |
| Login gate probe (before and after the upload) | [`deploy/check-login-gate.sh`](../../deploy/check-login-gate.sh) |
| Pinned deploy tool and locks | [`deploy/package.json`](../../deploy/package.json), [`deploy/package-lock.json`](../../deploy/package-lock.json) |
| Deploy workflow | [`.github/workflows/hosted.yml`](../../.github/workflows/hosted.yml) |
| Asset limits check | `cargo --locked xtask hosted-limits` |
| Custom domain in the owner's zone | Cloudflare account only |
| Access reader application (the custom domain alone, Allow for the reader group), the blocking application (the `workers.dev` and preview hostnames, Block for everyone) and the reader group | Cloudflare account only |
| Edge-rewriting rule (a hostname-scoped Configuration Rule that turns off Email Obfuscation, Automatic HTTPS Rewrites, Rocket Loader, the Web Analytics (RUM) beacon, Zaraz and Apps for the site's host) | Cloudflare account only |

The three deploy values are secrets of the GitHub environment `hosted`. There are no repository-level secrets or variables.
- `CLOUDFLARE_API_TOKEN`: account-owned token with permission to write Worker scripts and nothing else.
- `CLOUDFLARE_ACCOUNT_ID`: account identifier.
- `FIELDBOOK_HOST`: the custom host. It is a secret, not a variable, so that GitHub masks it in logs.
- The environment's deployment-branch policy allows `main` alone, so a branch or pull-request run never receives the secrets. The owner reads the environment's secrets and branch policy whenever either changes.
- The deploy job runs on the owner's self-hosted runner, with the labels of `ci.yml` (the owner's decision while CI-01 in [controls.md](../controls.md) stands). That runner persists between jobs and also runs branch CI, so the job takes its mitigations from the workflow: secrets only through the environment, a clean checkout with no stored credentials, a fresh `out/`, `cargo --locked`, `npm ci --ignore-scripts` from the committed lock, and no data root, store, overlay, policy file or state folder flag.
- The broader setup token (Access, DNS, routes) never goes to CI and stays in the owner's vault.
- Provider-side setting the owner keeps: the edge-rewriting rule named in the components table. Cloudflare can rewrite HTML at the edge (email obfuscation, HTTPS link rewrites, Rocket Loader) or inject a script into it (the Web Analytics beacon, which Cloudflare adds to a proxied host when Web Analytics is on for the account, and Zaraz), which would change the pages as built and conflict with their content security policy; the rule turns that off for the site's host only. Confirm it after any change of the zone's settings (section 3, step 4).

## 2. Deploy

A merge to `main` runs the deploy workflow ([`.github/workflows/hosted.yml`](../../.github/workflows/hosted.yml)); or trigger it from the Actions page with `rollback_to` left empty.

Workflow execution order:
1. Check out clean, without stored credentials, and remove any `out/` left on the runner.
2. Validate configuration.
3. Search the tree for the values of the host and account id: [`deploy/check-no-identifiers.sh`](../../deploy/check-no-identifiers.sh) searches case-insensitively, prints only the name of a variable that matched, and fails on an empty value or a search error.
4. Build the edition static pages with `just site`.
5. Stage `out/hosted` (edition pages, `originals/` at root, [`deploy/_headers`](../../deploy/_headers) and [`deploy/robots.txt`](../../deploy/robots.txt)).
6. Check limits: `cargo --locked xtask hosted-limits out/hosted` (fails on any file over 25 MiB or total count over 20,000 files, and prints the five largest files; wrangler also refuses files over 25 MiB).
7. Check the build is unreleased: [`deploy/check-unreleased.sh`](../../deploy/check-unreleased.sh) fails if `FIELDBOOK_GIT_SHA` is set (even empty), if `out/hosted/index.html` is missing, or if it does not say "unreleased build". The hosted edition is never a released build.
8. Install the pinned deploy tool: `npm ci --ignore-scripts`.
9. Probe the login before the upload: run [`deploy/check-login-gate.sh`](../../deploy/check-login-gate.sh) against `FIELDBOOK_HOST`.
10. Deploy static assets: `wrangler deploy --env-file=/dev/null` using the commit SHA as the message. The flag stops wrangler loading a `.env` file from the persistent runner's checkout; the same flag is on `wrangler rollback`.
11. Probe again after: run [`deploy/check-login-gate.sh`](../../deploy/check-login-gate.sh) against `FIELDBOOK_HOST`.

Runner notes:
- The job runs on the self-hosted runner (section 1). Moving it to an ephemeral GitHub-hosted runner is the remedy for the residual risk that a persistent runner shared with branch CI could have its tool caches tampered with; do that when CI-01 ends ([ADR 0007](../decisions/0007-hosted-edition.md), risk table).
- The claims register is split: `claims.html` is an index and the claims sit in `claims/<AREA>-<part>.html` pages, so it no longer blocks the limits step. The step still names any file over 25 MiB; read its output on the first deploy.
- The controls that guard this workflow and its configuration are HO-04 to HO-09 in [controls.md](../controls.md) (the secrets and environment, the runner and job hardening, the identifier and unreleased checks, the wrangler configuration, and the triggers, pins and metrics setting of `hosted.yml`).
- The deploy shares the runner's home folder with branch CI, and both use `taiki-e/install-action`, which writes its tools to one folder there. A deploy that starts while a CI job is installing `just` can fail at that step (curl error 23, a write that could not complete). Nothing was deployed; re-run the failed job once the CI job is past its install step.
- Only one deploy runs at a time, and a running deploy is never cancelled. GitHub keeps at most one run waiting in the concurrency group `deploy-hosted`: a newer waiting run replaces an older waiting one. A push to `main` that arrives while a deploy runs therefore replaces a queued rollback dispatch (see section 4).

## 3. Verify after a deploy

Run this checklist after a deploy:
1. **Gate step**: Confirm the workflow gate step passed. It requests `/`, `/index.html` and `/assets/app.css` without a session and requires a redirect to the Access login host, or 401/403, with no edition content in the body.
2. **Reader session**: Signed in as a member of the reader group, open the site at `FIELDBOOK_HOST`. Open a page, a source page and one original PDF (an original opens in the browser's PDF viewer under the same response headers; if a browser refuses to show a PDF because of the site's Content-Security-Policy or X-Frame-Options header, change the headers for `/originals/*` only, in its own pull request, never relax the page rule). Press Ctrl K and use the search page. Note that HTML pages redirect from `/page.html` to `/page` (307) because of the `auto-trailing-slash` setting, and that is expected. If the sign-in ends on a page that says there is nothing here yet, the reader application names a hostname that answers 404: Access signs a reader in to each hostname of an application in turn, so the reader application must name the custom domain alone (section 5).
3. **Unauthenticated probes**: Probe the three hostname kinds from a shell without a session and expect no edition content from any of them:
   - Custom host (redirect to Access login): `curl -sI https://<custom-host>/`
   - The Worker's workers.dev name (expect 404, route is disabled): `curl -sI https://<workers-dev-name>/`
   - Preview URL (expect 404): `curl -sI https://<preview-url>/`
4. **Edge rewriting stays off**: In the Cloudflare dashboard, confirm the hostname-scoped Configuration Rule for the site's host still turns off Email Obfuscation, Automatic HTTPS Rewrites, Rocket Loader, the Web Analytics (RUM) beacon, Zaraz and Apps; do this after any change of the zone's settings. In the page source of a signed-in read, no `/cdn-cgi/` script, no `cloudflareinsights` beacon and no rewritten link appears, and the browser console shows no script refused by the content security policy (a refused beacon is the sign that the rule is missing its RUM setting).

A failed gate is an Access fault, not a fault of the files, so rolling the Worker back does not close it: the owner first detaches the custom domain from the Worker or fixes the Access application, then repeats the hostname probes, then re-runs the deploy.

## 4. Roll back

To restore a previous deployment:
1. Find a version id to target:
   ```sh
   cd deploy && npm ci --ignore-scripts --no-audit --no-fund
   WRANGLER_SEND_METRICS=false npx --no-install wrangler deployments list
   WRANGLER_SEND_METRICS=false npx --no-install wrangler versions list
   ```
   Read the deploy token from the owner's vault into the environment of the one command (`CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`); never print it, never write it to a file.
2. Apply rollback via either route:
   - Workflow: run the deploy workflow ([`.github/workflows/hosted.yml`](../../.github/workflows/hosted.yml)) from the Actions page with `rollback_to` set to that version id (skips the build, runs `wrangler rollback`, and runs a probe before and after). The concurrency group `deploy-hosted` keeps one waiting run: if a deploy is running when you dispatch, a push to `main` that arrives afterwards replaces your waiting rollback, and the Actions page shows it cancelled. Open the run after dispatching, confirm it started and was not replaced, and dispatch again if it was. Merge nothing to `main` until the rollback has run.
   - The workflow route was tried at setup with a canary version: the rollback served the canary, and a roll-forward to the version from `main` restored the pages and headers. Re-run that drill after a change to `hosted.yml`.
   - Local: with the same environment as above, run `WRANGLER_SEND_METRICS=false npx --no-install wrangler rollback <version-id> --env-file=/dev/null --message "<reason>"`.
3. Revert git history: a rollback changes what is served, not the repository. Follow it with a revert on `main`, or the next merge deploys the new code again.

## 5. Add a reader

The site has two Access applications. The reader application names the custom domain alone and has one Allow policy whose only include is the reader group. The blocking application names the `workers.dev` hostname and the preview hostnames and has one Block policy for everyone. Never put the custom domain in the blocking application, and never add the `workers.dev` or preview hostnames to the reader application: it would send every sign-in through a hostname that answers 404. After any change to either application, repeat the probes of section 3.

1. Edit the reader group in the owner's Cloudflare Zero Trust dashboard under Access > Access groups.
2. Add an email address, a list or an email domain to the group. Nothing in the repository changes, and no deploy is needed.
3. Widening beyond the owner needs the owner's say-so.
4. Removing a reader: remove the identity from the group, then revoke active sessions in Access (sessions last 24 hours).

## 6. Rotate the deploy token

1. In Cloudflare, create a new account-owned token with only the permission to write Worker scripts.
2. The new token replaces the secret in the `hosted` environment, not the repository (`CLOUDFLARE_API_TOKEN`); put the new token in the owner's vault.
3. Run the workflow once (a dry deploy is not enough; run a `rollback_to` the current version or a normal deploy) to prove the token works.
4. Delete the old token in Cloudflare.
5. Leaked token: delete the compromised token in Cloudflare first, then generate and install the replacement.

## 7. Update wrangler

`wrangler` is a deploy-time tool only ([dependencies.md](../reference/dependencies.md), row under Deployment tools).
1. Confirm the candidate release is at least 14 days old: `npm view wrangler@<version> time`.
2. Set the exact version in [`deploy/package.json`](../../deploy/package.json).
3. Regenerate [`deploy/package-lock.json`](../../deploy/package-lock.json) with npm's `--before` set to a date 14 days back, so that no dependency younger than the rule resolves: `npm install --package-lock-only --ignore-scripts --before <date 14 days ago>`.
4. Check every locked package for the 14-day rule: `npm view <package>@<version> time`.
5. Install from the new lock and run the dry-run deploy. The configuration points at `../out/hosted`, which does not exist until a site is staged, so name a small staged folder (as in section 8) with `--assets`:
   ```sh
   cd deploy && npm ci --ignore-scripts --no-audit --no-fund
   WRANGLER_SEND_METRICS=false npx --no-install wrangler deploy --dry-run --env-file=/dev/null --outdir <scratch-out> --assets <scratch-dir>
   ```
   It needs no credentials and uploads nothing; it reads the staged files and exits after printing the upload size.
6. Update the version in the workflow's version check (the version check file is [`.github/workflows/hosted.yml`](../../.github/workflows/hosted.yml)) and the dependencies row in [`docs/reference/dependencies.md`](../reference/dependencies.md) in the same pull request.

## 8. Check the headers locally

Verify response headers locally without deploying and without credentials:
1. Stage a small built site into a scratch directory with [`deploy/_headers`](../../deploy/_headers) and [`deploy/robots.txt`](../../deploy/robots.txt) at its root.
2. Start the local server:
   ```sh
   cd deploy && npm ci --ignore-scripts --no-audit --no-fund
   WRANGLER_SEND_METRICS=false npx --no-install wrangler dev --local --assets <scratch-dir> --port 8792
   ```
3. Probe the local server:
   ```sh
   curl -sI http://127.0.0.1:8792/about
   ```
   Expected: every header of [`deploy/_headers`](../../deploy/_headers) appears; without the `_headers` file in the scratch directory none does.
4. Local state folder: `deploy/.wrangler/` is the local state folder and is ignored in [`.gitignore`](../../.gitignore).

## 9. What never to do

- Never enable `workers_dev` or `preview_urls` in [`deploy/wrangler.jsonc`](../../deploy/wrangler.jsonc).
- Never add `routes` or a custom domain, an account id or a host name to any file in the repository (the custom domain lives in the Cloudflare dashboard and a deploy without routes keeps it).
- Never remove or loosen either Access application, give the blocking application an Allow policy, or drop a hostname from either (the gate check exists to catch this for the custom domain, and a failing gate step is a stop: do not re-run, tell the owner).
- Never deploy company, customer, personal or controlled material, or anything that is not in the public edition.
- Never store the deploy token anywhere but the `hosted` environment secret and the owner's vault.
- Never put the deploy secrets at repository level or in any other workflow.
- Never move the deploy job to a runner other than the one `hosted.yml` names, and never let a branch or pull-request run reach the environment `hosted`: its deployment-branch policy allows `main` alone.
- Never set `FIELDBOOK_GIT_SHA` in the hosted build: the hosted edition is an unreleased build.
- Never run a bare `npx wrangler` (it can fetch a different version from the network); use `npm ci --ignore-scripts` first, then `WRANGLER_SEND_METRICS=false npx --no-install wrangler`.
- Never run the setup token in CI.
- Never cancel a running deploy.
- Never use `wrangler` with a newer version than the one pinned without the update steps above.
- Never edit headers so that the Content-Security-Policy differs from the pages' (a test enforces equality plus frame-ancestors).
