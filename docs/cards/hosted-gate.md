# hosted-gate

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`deploy/check-login-gate.sh BASE_URL PATH...` exits 1 when any given path of a host answers a request without the sign-on: a status other than 302, 401 or 403, a redirect that does not go to the login host, or a body that carries edition content. It is the probe that the deploy workflow runs before and after an upload.

## Why

- The Gate controls of [ADR 0007](../decisions/0007-hosted-edition.md#controls-and-evidence) (before and after a deploy): a removed or widened Access application must fail the run, not pass it silently. This card builds the probe; `hosted-workflow` calls it.
- Review is `auto`: the script reads a public URL, holds no secret and changes nothing; it is called by a workflow that another, owner-reviewed card adds.
- The check is the row HO-03 of [controls.md](../controls.md#hosted-edition). Its output never carries the host name, a port or the redirect target, so a run log names no identifier ([ADR 0007](../decisions/0007-hosted-edition.md#controls-and-evidence), No identifiers).
- The script uses only `curl`, `grep` and `sed`, so it runs on any runner image, whatever `awk` it ships.

## Scope

- Files: `deploy/check-login-gate.sh`, `xtask/tests/hosted_gate.rs`.
- Registration lines: `docs/controls.md` (row HO-03 in the Hosted edition section), `docs/plan/PLAN.md` (this card's line).
- Docs to update: the files named under Registration lines.

## Out of scope

- The workflow that calls the probe, the runbook and the changelog line (`hosted-workflow`).
- Creating or changing the Access application; it is in the owner's account.

## Preconditions

- Cards merged: `hosted-deploy`.

## Steps

1. `deploy/check-login-gate.sh`: for each path, one request without redirects; pass on 302, 401 or 403 whose `Location`, if any, is on the login host and whose body carries neither the product name nor a content security policy; print `ok` or `FAIL` with the path and the reason only.
2. `xtask/tests/hosted_gate.rs`: a local mock server answers recorded responses; run `cargo nextest run --locked -p xtask --test hosted_gate`. Mark each negative control `// control: HO-03`. A missing `bash` or `curl` fails the tests; it never skips them.
3. The controls row and the plan line. Delete this card file in the last commit.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test hosted_gate
just check
```

Expected: both exit 0.

## Negative control

- Gate (HO-03): a recorded 200 response with an edition page body makes the probe exit 1 for every path; a 302 to another host fails; a 302 with no `Location` fails; a 403 whose body names the product fails. Checked against a local server, never the live site.

## Rollback

Revert the merge. Nothing calls the script until `hosted-workflow` merges.

## BLOCKED

Stop if the probe would need the host name or a token in a file. Comment `BLOCKED: gate probe needs an identifier`, push the branch and take the next card.
