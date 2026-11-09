# ADR 0007 The hosted read-only edition

Status: Proposed. The owner accepts it in its pull request. In [ADR 0001](0001-foundation.md) it adds one workflow to [AD-08](0001-foundation.md#ad-08-ci-design), and it narrows three rejections for the deploy step of that workflow only: Node in the Rejected column of AD-07 and npm in that of AD-15 in the [decision table](0001-foundation.md#decision-table) (see [AD-07](0001-foundation.md#ad-07-developer-tooling) and [AD-15](0001-foundation.md#ad-15-ui-technology-and-design-system)), and "No npm" in [H3](0001-foundation.md#hypotheses). It decides nothing else in ADR 0001 or [ADR 0004](0004-company-boundary.md).

## Trigger

No row of AD-24 measures this need, so no Later trigger fired. The trigger is an owner need that ADR 0001 did not foresee: the owner wants to read the current library away from home without a transfer, and Tier 0 cannot serve that until IT names a channel, and then only one edition at a time. Under [Reopen triggers](0001-foundation.md#reopen-triggers) a decision of ADR 0001 changes only through a new record, so the AD-07, AD-15 and H3 rejections that the deploy step needs are narrowed here, bounded below, and nowhere else. `later-js-tooling` is a separate row about the site's own JavaScript tooling; it does not fire and this record does not touch it.

## Context

The owner wants to read the Corpus while at work. Tier 0 brings it to the laptop as files, only by the channel IT approves (IT-9), and mail filters strip the scripts from a zip. Until that channel is open, and between editions after it, the owner has no way to read the current library away from home without installing or carrying anything.

`fieldbook build` already writes the whole site as static files (`just site` writes `out/edition`). The site needs no server: every page works from disk, its CSP lets it fetch nothing, and its search runs in the page. A copy of those files behind a sign-on, read in a browser, answers the need without a change to the program.

The infrastructure exists and is the owner's: a static-assets Worker on Cloudflare with a custom domain in the owner's zone, and one Cloudflare Access application in front of it. This record decides what may be put there, who may read it and on what device, how it is put there, what proves it is gated and how it is undone. It names no host, account, zone or reader: those live only in repository variables, environment secrets and the owner's vault.

## Decision

The **hosted edition** is the output of `fieldbook build` over `corpus/` at a commit of `main`, deployed unchanged as static files to Workers static assets by one workflow, `hosted.yml`, and read only through Cloudflare Access.

- **The program is unchanged.** No crate gains a listener, a network call, an authentication step or hosting code. The deterministic verbs stay model-free and network-free ([AD-01](0001-foundation.md#ad-01-architecture), Determinism). The silent build still opens no socket and every edition still makes no outbound connection ([AD-18](0001-foundation.md#ad-18-security-and-confidentiality-guardrails), Network silence). Hosting is a deployment of build output, done by the workflow, like `release.yml` attaching zips to a release.
- **No listener of Fieldbook's.** The Never row "Local server or any listener in the silent build or an edition" of [AD-24](0001-foundation.md#ad-24-what-is-not-built-yet-later) stands. The provider's edge serves the files; nothing of Fieldbook runs there, and the Worker has no script, only assets. A company-hosted service stays the Later row `later-hosted-service`, under its own ADR; this record is not it and is no step toward it.
- **No telemetry, update check or ping.** NG-07 and the Updater row of AD-24 stand. The pages carry the same CSP meta (`CSP_STATIC`, [AD-12](0001-foundation.md#ad-12-site-generation-and-page-types)), with `connect-src 'none'` and `form-action 'none'`, so a hosted page sends nothing either. The provider keeps its own sign-on and request logs; that is the provider's record of who reached the site, not a feature of the program, and no analytics or beacon is enabled on it.
- **Same bytes.** The workflow deploys the files `build` wrote, plus two committed deployment files beside them: a response-header file and `robots.txt`. It adds, rewrites or injects nothing in a page. A built page is byte-identical whether it is opened from disk, a zip or the hosted copy (invariant 3 of [the architecture in one page](0001-foundation.md#the-architecture-in-one-page)).
- **When it deploys.** On each push to `main` and on a manual dispatch from `main`. The deploy job declares the GitHub environment `hosted`, whose deployment-branch policy allows `main` alone, and the two deploy secrets exist only in that environment. A run from any other ref, a pull request's included, is refused the environment and never sees the secrets, whatever its own copy of the workflow says. There are no preview deployments and no deploy from any machine.
- **The runner.** The deploy job runs only on an ephemeral GitHub-hosted runner, which starts clean for the job and is discarded after it. It never runs on the self-hosted runner of CI-01's temporary departure, which persists between jobs and runs the code of every same-repository pull request. While that departure lasts, nothing deploys: `hosted.yml` waits for GitHub-hosted runners.
- **The deploy tool.** It is the provider's command-line tool, an npm package that runs on Node. This is the one place Node and npm run in the repository's work: only in the deploy step of `hosted.yml`, never in `just`, the build, the site, the tests or any shipped file. Its npm tree is installed with `npm ci` from a committed lockfile, pinned to an exact release under the 14-day cooldown, covered by its row in [dependencies.md](../reference/dependencies.md), and run with its metrics off. The site keeps its first-party JavaScript and no build tool.
- **What it is not.** It is not an edition in the sense of [AD-20](0001-foundation.md#ad-20-signing-release-pipeline-and-versioning): it has no `edition-NNN` tag, no zip, no `SHA256SUMS`, no transfer statement and no signature, and it does not replace Tier 0 or the channel of IT-9. It is a read-only view of `main`.

## Boundary

The hosted edition holds what an edition may hold, and nothing more: public content and synthetic examples. That is fixed by the program, not by this record.

- **Labels.** In [ADR 0004](0004-company-boundary.md#6-destinations) the destination `edition` is fixed `public` only, and the build refuses any other label. The hosted edition is that destination's output carried one step further. It is not a new destination kind, it adds no row to `DEST_KINDS`, and no policy file can widen it ([What stays refused](0004-company-boundary.md#13-what-stays-refused)). No company label may ever reach it.
- **What the build reads.** The deploy job builds on its ephemeral GitHub-hosted runner, from a fresh checkout of the `main` commit that started it. It reads only `corpus/` and the program's own templates. There is no data root, no company store, no `overlay\`, no policy file and no state folder on that runner, and the workflow never passes a flag that names one. A site built on the laptop, on the self-hosted runner, or with a company store is never deployed. The one-way flow of invariant 1 holds: public content moves out of GitHub to the host, and nothing comes back.
- **Not a workplace.** The hosted edition is for reading the public library. Company data, customer data, proposals, pricing, estimates and reports are never built, drafted, pasted or stored there. Text pasted into a hosted page (the Assistant page's Handoff box, a search box) stays in that browser tab: the CSP forbids every request, and the page has no form that submits. Company work stays on the laptop, in company storage, under [ADR 0004](0004-company-boundary.md#1-the-boundary).
- **The Assistant page.** On the hosted copy it is Handoff only, as on a file. The Bridge works only on `http://127.0.0.1` and only when `fieldbook chat` serves the page.

## Readers

- **Where it is read.** On the owner's personal devices, over networks that are not the company's. Not on company equipment and not over the company network. The IT-facing statements say that nothing reaches the laptop from Fieldbook except by the channel IT names, list every place data rests, and say no cloud service is added ([intake](../intake/README.md), [data handling](../intake/data-handling.md#2-where-data-rests)). A hosted copy opened in a work browser would be a new channel and a new cloud service, and it would let the site's downloads (`registers.xlsx`, the templates, the CSVs, the originals) reach the laptop outside IT-9 and NFR-P-07. Reading it on company equipment or the company network is therefore an owner decision that changes those statements, and before its first such use the Work-use control below is met.
- **The gate.** One Cloudflare Access self-hosted application covers every hostname that can serve the site: the custom domain, the `workers.dev` hostname and the preview hostnames. The `workers.dev` hostname and preview URLs are also disabled on the Worker (`workers_dev = false`, `preview_urls = false`), so they answer 404 even behind Access. The custom domain is attached to the Worker in Cloudflare and appears in no repository file, and a deploy with no routes leaves it in place.
- **Who.** The application has one Allow policy whose only include is an Access group, called the reader group here. Sign-on is the one-time PIN to the reader's email, with a 24-hour session. Today the group holds the owner alone.
- **Adding a reader** is adding an email, a list or an email domain to the reader group in Zero Trust. Nothing in the repository changes, and no deploy runs. Widening the group beyond the owner is the owner's decision alone, made on the owner's tracker, never by an agent or the lead.
- **Never.** No service token, no Bypass or "Everyone" policy, no include rule outside the reader group, and no second application or hostname that serves the site without Access.
- **Search engines.** Every response carries `X-Robots-Tag: noindex, nofollow`, and `robots.txt` disallows every path. Behind Access a crawler reaches neither; the two are for a gate that has failed.

## Honest status

The hosted edition is an unreleased build of `main`, not a release. Its claims are drafted with AI assistance and are unchecked until a person checks them, and every status on it is the status in the Corpus at that commit. It never presents anything as verified, released or approved: it shows the statuses the build already renders, and a planned card adds a strip to every page that says the build is unreleased and its claims unchecked. No figure or recommendation on it is a basis for company action, and no saving is stated without the baseline SM-01 asks for ([Requirements referenced](0001-foundation.md#requirements-referenced)); the owner checks a claim against its source before relying on it.

## Controls and evidence

A later card with Review `owner` builds `hosted.yml` and these controls, adds their rows to [docs/controls.md](../controls.md), and shows each negative control failing. Until it merges, nothing deploys from the repository.

**Before deploy.** Each runs in pull-request CI or in the deploy job before any file is uploaded. A failure deploys nothing, and the previous version keeps serving.

| Control | What it checks | Evidence | Negative control |
|---|---|---|---|
| Limits | No file over 25 MiB, and at most 20,000 files in the deployed folder | The step prints the largest file's size and the count | A planted file of 26 MiB fails the step and deploys nothing |
| Headers | Every response carries the header table below | A test reads the committed header file; the owner reads the live headers once after the first deploy, signed in | Removing a header from the committed file fails the test |
| Environment | `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` exist only as secrets of the environment `hosted`, never at repository level, and its deployment-branch policy allows `main` alone; only `hosted.yml` declares that environment | The owner reads the environment's secrets and branch policy whenever either is set or changed; a test over `.github/workflows/` checks that only `hosted.yml` declares `hosted` | A planted `environment: hosted` in another workflow fails the test; the owner dispatches a job that uses the environment from a non-`main` ref once, and it is refused |
| Runner | `hosted.yml` runs only on a GitHub-hosted runner label | A test reads the workflow's `runs-on` | The self-hosted labels of `ci.yml` in `hosted.yml` fail the test |
| Deploy token | The token is an account token with Workers Scripts write only | The token's permissions, read by the owner from the provider when it is made or rotated | — (an owner check, not a test) |
| No identifiers | No host name, account or zone id, `workers.dev` subdomain, Access team name or reader email in any repository file, the deployment config included (no `routes`, no `account_id`) | In pull-request CI, a shape check over `.github/` and the deployment config refuses a `workers.dev` label, an Access team domain, a run of exactly 32 hex digits and an email address. In the deploy job, before upload, the tree is searched for the values of `FIELDBOOK_HOST` and `CLOUDFLARE_ACCOUNT_ID` without printing them, and a match fails the job. The personal denylist (D-11) still runs on the owner's own pushes, but is not relied on | A planted value of each shape fails the CI check; a planted copy of the host value fails the deploy job's search |
| Gate (before) | Unauthenticated requests to `/` and to one page of the custom host (the repository variable `FIELDBOOK_HOST`) are redirected to the Access sign-on and never return a page | The probe prints each status code and whether the redirect went to Access, never a host name or the page | The probe's check, given a recorded 200 response with a page body, fails |

**After deploy.** The Gate probe runs again once the new version serves. A failure fails the run; the new files are already live by then.

| Control | What it checks | Evidence | Negative control |
|---|---|---|---|
| Gate (after) | The same requests as Gate (before) | The same output | The same recorded 200 response fails it |

A failed gate is an Access fault, not a fault of the files, so rolling the Worker back does not close it. The owner first detaches the custom domain from the Worker or fixes the Access application, then repeats the hostname controls below, then re-runs the deploy.

The response headers:

| Header | Value |
|---|---|
| `Content-Security-Policy` | `CSP_STATIC` plus `frame-ancestors 'none'` (a meta tag cannot carry it) |
| `Strict-Transport-Security` | `max-age=31536000`; no `preload` |
| `X-Robots-Tag` | `noindex, nofollow` |
| `Cache-Control` | `private`, so no shared cache keeps a copy |
| `X-Content-Type-Options` | `nosniff` |
| `Referrer-Policy` | `no-referrer` |

- **Work use.** Before the hosted edition is first read on company equipment or over the company network, IT has been told of it, and the owner has updated the IT-facing statements in `docs/intake/` (and `SECURITY.md` if its scope is affected) to name it. Those are owner-gated files; until they say so, the hosted edition is read only as Readers says.
- **The setup token** (Access, DNS, routes) never enters CI or the repository. It stays in the owner's vault and is used by hand for setup.
- **Actions** are pinned by SHA and the workflow token stays read-only (AD-08).
- **The workers.dev and preview negative controls** were seen at setup: before Access every hostname returned the canary page; after it the custom domain redirects to sign-on and the other hostnames return 404. The owner repeats them after any change to the Access application or the Worker's settings.
- **First deploy.** `claims.html` is over the 25 MiB limit today, so the Limits control refuses the first deploy until `claims-register-split` merges. No deploy runs at all while CI-01's departure keeps GitHub-hosted runners away.

## Consequences

**What this gives.** The owner reads the current library away from home on a personal device, in any browser, with nothing installed and nothing carried. A merge to `main` reaches the hosted copy in one workflow run. The program and its builds are unchanged.

**What it costs.** One workflow, one environment with two secrets, one repository variable, two committed deployment files, one deploy tool with its npm lockfile, and a provider account the owner administers. Node and npm enter the repository's work for that one step. The edition's per-file sizes are now bound by the provider's limits as well as by its own budgets. Reading the library at work on company equipment costs a change to the IT story, which this record does not make.

**Risks.**

| Risk | What follows | Mitigation |
|---|---|---|
| Access misconfigured (a hostname left out, a Bypass or Everyone rule, the group widened by mistake) | Public, synthetic content becomes readable by others; the harm is the owner's unchecked drafts read as the owner's statements | The gate probe before and after every deploy; `noindex` and `robots.txt`; the owner repeats the hostname controls after any Access change; only public content is ever there |
| Provider outage | No hosted copy for a while | Tier 0 and the local build are unchanged; nothing depends on the hosted copy |
| A branch's workflow reaching the secrets | A pull request or dispatch from another ref runs a workflow file that names the secrets | The secrets exist only in the `hosted` environment, whose branch policy refuses every ref but `main`; no repository-level copy exists |
| Tampered tools or caches on the runner | A build or deploy runs tools another job altered, with the token in its environment | The deploy job runs only on an ephemeral GitHub-hosted runner, never on the self-hosted runner that runs pull-request code |
| Deploy token leaked | Someone can replace the Worker's files or scripts in the account; the content readers see behind Access could be altered | The token holds no Access, DNS or zone permission, so it cannot open the gate; it is reachable only by a `main` job in the `hosted` environment; the owner revokes and rotates it from the vault, and redeploys from `main` |
| Accidental widening | A person the owner did not choose reads the library | Only the owner widens the group; nothing in the repository can |
| Read at work before IT is told | A channel and a cloud service IT has not named reach company equipment | Readers limits reading to personal devices until the Work-use control is met and the owner has changed the IT-facing statements |
| Non-public content in the build | Company data on a host the company does not operate | Impossible by the existing rules: the repository and the edition hold `public` only, and the runner has no store |

**Rollback.** A bad deploy of files is undone by rolling the Worker back to its previous version, or by redeploying a known-good commit of `main`. An open gate is not a bad deploy: it is closed as Controls says, by detaching the custom domain or fixing Access. To withdraw the hosted edition, the owner detaches the custom domain or deletes the Worker and the Access application, and the lead reverts `hosted.yml`; the program and every edition are untouched.

**What this does not change.** The silent and AI builds, every crate, the listener rule, the network-silence statements, the `edition` and `git` destinations, the release pipeline of AD-20, Tier 0, the site's own JavaScript rules, and the rule that nothing company-owned goes to the home side, GitHub or a host the company does not operate.

## Alternatives considered

| Option | Why not |
|---|---|
| Cloudflare Pages | Every branch and commit gets a preview deployment on its own hostname, which is harder to keep behind Access and to switch off; Workers static assets can disable both |
| The local package only | It does not reach the owner at work while mail strips the zip's scripts, and it trails `main` by a release |
| Hosting inside the company | It needs IT's approval of a host and of this content; it is the Later row `later-hosted-service`, for company data under its own ADR, not for reading the public library |
| Deploying from the self-hosted runner | It persists between jobs and runs every same-repository pull request's code, so a deploy there could run tampered tools with the token |
