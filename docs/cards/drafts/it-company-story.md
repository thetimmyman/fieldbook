# it-company-story

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

The documents that go to IT say what the policy file, the labels, the stores and the audit export do, sentence by sentence true of the program as built through `audit-export`. A sentence about a part that is not built yet (`sync`, the company store, records, `label add`) names the release it arrives in. `docs/intake/policy.md` is a guide that IT can follow to write, check, install and verify a policy file, and `docs/intake/policy.example.toml` is a synthetic file that `fieldbook policy check` refuses only because it says `example = true`, and accepts once that one line is deleted. The count of IT questions is 39 in all six places that state it, the floor is stated in one sentence that every place quotes word for word, and one test file keeps the example and the guide from drifting from the program. Nothing else changes.

## Why

- Review is `owner` because the card edits the IT-facing statements in `docs/intake/` and `SECURITY.md`, which the release gate covers ([Gates](../../decisions/0001-foundation.md#gates)). The owner reads each changed sentence before the pack goes to IT, and sends IT-33 through IT-39 with the rest.
- What the pack may claim is bounded by the controls in [controls.md](../../controls.md) and by [ADR 0004](../../decisions/0004-company-boundary.md#3-the-company-policy-file). The pack makes no claim of compliance with any scheme and says that IT and the company decide and operate the rest.
- The card follows `ai-it-story`. That card made the pack true of the first edition and the AI build and states the questions. It cannot check a sentence about the policy file, because `policy show`, `policy check` and `audit export` did not exist when it ran. This card does, and it shares files with it, so it comes after.
- The failures the checks prevent, each by a test below:
  - A stale example. A policy file with one unknown key is invalid as a whole, and an invalid file closes every destination (FB0413). If IT copies an example that the loader no longer accepts, the first thing the company learns is that the program is closed on the day it installs the file. `the_example_is_a_valid_policy_once_the_example_line_is_gone` runs the real checker on a copy of the example with that one line removed.
  - The example installed as it stands. A copy that still says `example = true` would carry the placeholder labels and roots of the example into the company's real file. The loader refuses it (FB0413), so the mistake shows on the first run and not in a year. `the_example_is_refused_as_it_stands` and `the_example_is_never_loaded` keep that true.
  - A real label code or host name pasted into the example. The first policy is drafted from the example, so the draft starts as a copy. `the_example_holds_only_synthetic_values`.
  - The floor stated two ways. IT would read a default that is more permissive than the program's, or a floor for a laptop without a company store where the laptop has one. `it_33_and_the_guide_quote_the_floor_sentence`.
  - A guide that names a command that was renamed or a code that does not exist: `the_guide_names_only_commands_that_exist`, `the_guide_names_only_known_codes`.
  - A flag that is not in the release program. The flags that move the policy, a laptop folder or the date exist only in a test build (control PL-03). A pack that names one tells IT to type something that fails, and a reader may take it for a way round the policy folder. `the_pack_names_no_flag_that_moves_the_policy`.
- The tests do not show that a sentence is true. The pass of step 1 and the owner's reading do that.
- No page, template or string shown in a browser is built here, so there is no Design bullet.

## Scope

- Files: `docs/intake/{README.md,ai-development-statement.md,data-handling.md,it-questions.md,network-silence.md,transfer-statement.txt,policy.md,policy.example.toml}`, `SECURITY.md`, `crates/fieldbook/tests/intake_policy.rs`.
- Registration lines: none.
- Docs to update: the files above, and `CHANGELOG.md` (`- Changed: the IT documents are checked against the program for the company policy file, labels, stores and the audit export, and a test keeps the policy guide and its synthetic example in step with the program.`).

## Out of scope

- Any change to the program, the Corpus, `docs/controls.md`, `docs/reference/` or `docs/design/`. A statement the program cannot support is a finding, not an edit of the program.
- `docs/intake/edition-readme.txt` and `docs/intake/integrity-guide.md` (`edition-zips`), and the runbooks under `docs/runbooks/` (the cards that own the drills).
- A sentence about `sync`, the company store, records and `label add`. They are not built here. The sentences say that they arrive at R2, and `r2-intake-pack` re-reads every one of them against the signed program.
- Asking IT anything. The questions are text; the owner sends them.
- The company's real policy file. It never enters a repository.

## Preconditions

- Cards merged: `docs-checks`, `ai-it-story`, `core-policy` (`Policy::floor_sentence()`), `policy-load` (`policy show`, `policy check` and `write_policy` of `crates/fieldbook/tests/support/policy.rs`), `audit-journal` (`audit verify --repair`, which `data-handling.md` names) and `audit-export` (`audit export`, `audit verify --export`).

## Steps

1. List every sentence of the pack that names the policy file, a label, a store, a destination, the audit export or the company store: `grep -nE 'policy|label|handling|audit export|overlay|company store|IT-3[3-9]' docs/intake/*.md docs/intake/transfer-statement.txt SECURITY.md`. For each, check three things and fix the sentence when one fails. It names the build, the tier or the edition it holds for. It names a command or a control that exists now (`policy show`, `policy check`, `audit export`, `audit verify --export`, `audit verify --repair`, the event `policy.change`, a control id that is a row of `docs/controls.md`) or says that it arrives at R2. A sentence about who may open a store, a source `root` or a model row for a non-public label is a sentence about the signed program, and names it. It quotes the Floor block of the [policy reference](../../reference/policy.md#the-floor-policy) word for word wherever it states the floor, and does not paraphrase it. Quote a canonical form of the network-posture block or name the build; do not write the bare four-part sentence. A sentence that names `--policy`, `--program-data` or `--local-app-data` is removed or reworded: those flags exist only in a test build that the company never receives (control PL-03), so no document of the pack tells IT to use one.
2. `docs/intake/policy.md`: read it against the policy reference. The three states, the fixed rows, the exit codes of `policy check` and `policy show`, the codes FB0412, FB0413, FB0414, FB0418, FB0806, FB0835, FB0901, FB0902, FB0918 and FB0925 and the event `policy.change` must be as the reference and the built commands say. The guide takes the policy folder from the Windows known-folder call and names no environment variable and no flag that moves it, and it has no step that adds a `state` row. The bullet of the guide that begins "The file is not read from a test build's" names the flag it denies. Replace the whole bullet with the code span `- No flag, variable or setting chooses the file or its folder. The program asks Windows for the folder, so a copy anywhere else has no effect ([PL-03](../controls.md#labels-policy-and-egress)).` (the link is written for `docs/intake/policy.md`, one folder below `docs/`) and make sure that link resolves from there. The guide and the example name `require_fresh_export` as the setting that turns the model gate's export requirement off, say that the requirement is on when the key is absent, and agree with [Audit](../../reference/policy.md#audit) in the reference (the key, its type and its default). The guide links the reference for the keys and does not repeat them. It names no internal work item.
3. `docs/intake/policy.example.toml`: run `fieldbook policy check` on it. The one finding is the `example = true` line (FB0413, `example`). Run it again on a copy with that line deleted: it must print `policy ok`. Any other finding is fixed in the example, or is a defect in the reference or the loader and BLOCKED.
4. `crates/fieldbook/tests/intake_policy.rs`, new. It reads each file named below with `include_str!` (for example `include_str!("../../../docs/intake/policy.example.toml")`), reads the whole folder `docs/intake/` from `concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/intake")` for the two tests that take every file of it, writes each copy of the example into a folder of its own under the system temporary folder, and runs the binary with `--state` on a temporary copy of `tests/fixtures/state-mini`. `policy check FILE` skips the writer check, so those tests need no flag. The one test that gives the binary a policy to load by `--policy` (`the_example_is_never_loaded`) carries `#[cfg(feature = "test-hooks")]`, because the flag exists only in that build, and writes its copy with `write_policy` (`#[path = "support/policy.rs"] mod policy_support;`), so that the umask of the machine does not matter; `just check` runs it in its `--all-features` run. The tests:
   - `the_example_is_refused_as_it_stands`: `fieldbook policy check docs/intake/policy.example.toml` exits 1 and prints one `error[FB0413]` line that names `example`.
   - `the_example_is_a_valid_policy_once_the_example_line_is_gone`: the test copies the example, deletes the line `example = true`, and `fieldbook policy check <copy>` exits 0 and its first line starts `policy ok: revision 1, 3 labels`. The copy prints one `warning` line for each value that matches a placeholder form of the [policy reference](../../reference/policy.md#rules-the-loader-enforces) (a host that ends `.example`, a path that starts `C:\Example`, a ref that starts `CHG-EXAMPLE-`). The test counts, with its own pattern, the lines of the copy outside comments that hold such a value, and requires the same number of `warning` lines, and at least one. The count is the test's reading of the rule, so a loader that stops warning is caught.
   - `the_example_is_never_loaded`: `fieldbook policy show --policy <the example>` (the `test-hooks` build) exits 1 and the first line is `policy: closed`; the copy without the line exits 0 and the first line is `policy: loaded`, with a `hash:` line equal to the SHA-256 of the copy, computed in the test. The test reads no line about expiry: `policy show` judges dates against the clock, and the model row of the example ends on 2027-06-30, but the first line, the hash and the exit code do not depend on the date.
   - `the_example_holds_only_synthetic_values`: every `[[label]]` id matches `^[a-z]{2,4}-[0-9]{4}$`; every `url` has a host that ends `.example`; every `paths`, `root` and `sink` value starts `C:\Example` or `\\` followed by a host that ends `.example`; every `ref` starts `CHG-EXAMPLE-`; every `readers` and `changers` value starts `EXAMPLE\`; every `clients` value is the placeholder `0123456789abcdef` repeated four times; no line holds `@` or four numbers joined by dots. A failure names the line.
   - `it_33_and_the_guide_quote_the_floor_sentence`: the default cell of the IT-33 row of `it-questions.md` and the guide `policy.md` each contain `Policy::floor_sentence()` word for word.
   - `the_guide_names_only_commands_that_exist`: each code span of `policy.md` that begins `fieldbook ` names a command, which is the words after `fieldbook` up to the first word that holds a capital letter, a dash, a dot, a slash or a backslash, and at most two (`fieldbook policy check FILE` names `policy check`; `fieldbook audit verify --export` names `audit verify`). The test runs `fieldbook <those words> --help` and requires exit 0; a span that names no word fails and names the line.
   - `the_guide_names_only_known_codes`: each `FB` followed by four digits in `policy.md`, `policy.example.toml`, `it-questions.md` and `data-handling.md` is in `Code::ALL`.
   - `the_export_switch_is_named_and_the_example_accepts_it`: `policy.md`, the IT-35 row of `it-questions.md` and `policy.example.toml` each hold `require_fresh_export`; the example holds the line `# require_fresh_export = true` and no line with that key outside a comment. A copy of the example without the `example = true` line and with the leading `# ` of that line removed, and a second copy with `require_fresh_export = false` in its place, each pass `policy check` and print the same first line as the copy without the key (`policy ok: revision 1, 3 labels`), so IT is never shown a key that the program refuses.
   - `the_intake_documents_cite_no_internal_slug`: no file under `docs/intake/` holds the string `later-`.
   - `the_pack_names_no_flag_that_moves_the_policy`: no file under `docs/intake/` and not `SECURITY.md` holds `--policy`, `--program-data` or `--local-app-data`, and `policy.md` holds no sentence that tells IT to set an environment variable for the policy folder. A failure names the file and the line. It may write the install path `%ProgramData%\Fieldbook\policy.toml`.
   - `it_33_to_it_39_each_have_a_default_and_a_reason`: `it-questions.md` has one table row for each of `IT-33` to `IT-39`, and each has a non-empty `Why it matters` cell and a non-empty `Default assumed until answered` cell, and none holds `@`, `.com` or four numbers joined by dots. A failure names the number. The test holds no count: the run of numbers and the count in every place that states it are checked by `it_count_matches_rows` of `docs-checks`, the one place that does.
5. Run the lints. A finding is fixed in the document, never by changing a lint.
6. List, in the pull request text, the sentences that changed, one line each, for the owner's reading.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --features test-hooks --test intake_policy
cargo nextest run --locked -p xtask docs_lints:: links::
cargo xtask cards --check
cargo build -q --locked -p fieldbook --features test-hooks && B="$PWD/target/debug/fieldbook" && T="$(mktemp -d)" && cp -R tests/fixtures/state-mini "$T/state"
"$B" policy check docs/intake/policy.example.toml --state "$T/state"; echo "exit $?"
grep -v '^example = true' docs/intake/policy.example.toml > "$T/copy.toml" && chmod 644 "$T/copy.toml"
"$B" policy check "$T/copy.toml" --state "$T/state" > "$T/check.out"; echo "exit $?"; head -2 "$T/check.out"; grep -c '^warning' "$T/check.out"
"$B" policy show --policy "$T/copy.toml" --state "$T/state" > "$T/show.out"; echo "exit $?"; head -3 "$T/show.out"
rm -rf "$T"
grep -c '^| IT-' docs/intake/it-questions.md
grep -rnE 'IT-1 +to +IT-3[0-8]\b|all 3[0-8] questions' README.md SECURITY.md docs | grep -v '^docs/design/'; echo "exit $?"
grep -rn 'later-' docs/intake SECURITY.md; echo "exit $?"
just check
```

Expected: the tests pass; `cards --check` prints `cards: ok`; the first `policy check` prints one `error[FB0413]` line that names `example` and then `exit 1`; the check of the copy prints `exit 0`, then a first line that starts `policy ok: revision 1, 3 labels` and a `hash:` line, then a number above 0 (one `warning` line for each placeholder value); `policy show` of the copy prints `exit 0` and then `policy: loaded`, a `path:` line and a `hash:` line; the count prints `39`; each of the two greps prints nothing and `exit 1`; `just check` exits 0. The block removes its temporary folder.

## Negative control

On the branch, each of these must make the named test fail, and the file is restored after each:

- Add a top-level key `colour = "red"` to `docs/intake/policy.example.toml`: `the_example_is_a_valid_policy_once_the_example_line_is_gone` and `the_example_is_never_loaded` must fail, and the first prints the FB0413 line.
- Delete the line `example = true` from the example file itself: `the_example_is_refused_as_it_stands` must fail. (A shipped example that the program accepts would be installed by mistake with its placeholder labels.)
- Add `owner-supplied` to the `holds` of the overlay row in the example: the copy is refused (FB0413) and `the_example_is_a_valid_policy_once_the_example_line_is_gone` must fail.
- Change `ai-gateway.example` to `gateway.corp.com` in the example: `the_example_holds_only_synthetic_values` must fail and name the line.
- Change one word of the floor text in the IT-33 row, or drop its last sentence: `it_33_and_the_guide_quote_the_floor_sentence` must fail.
- Rename `fieldbook policy check` to `fieldbook policy verify` in the guide: `the_guide_names_only_commands_that_exist` must fail.
- Write `FB9999` in the guide: `the_guide_names_only_known_codes` must fail.
- Change `require_fresh_export` to `require_fresh_exports` in the comment of the example: `the_export_switch_is_named_and_the_example_accepts_it` must fail.
- Lower by one the last number of the range sentence in `docs/intake/README.md`: `it_count_matches_rows` must fail and give the row count.
- Remove the row for IT-37 from `it-questions.md`: `it_33_to_it_39_each_have_a_default_and_a_reason` must fail and name the number, and so must `it_count_matches_rows` (run `cargo nextest run --locked -p xtask docs_lints:: links::`).
- Write `fieldbook --policy x policy show` in `docs/intake/README.md`: `the_pack_names_no_flag_that_moves_the_policy` must fail and name that file and line.
- Add a posture sentence to `SECURITY.md` that denies a socket and names neither the silent build, an edition nor the AI build: `posture_wording_names_the_build` must fail and name the line.
- Change `readers` of the overlay store in the example to `ACME\Finance-Readers`: `the_example_holds_only_synthetic_values` must fail and name the line.

One run must change nothing: set `expires` of the model row of the example to `2020-01-01`. Every test of this card still passes and the Acceptance prints the same lines, because the card judges no date (`policy check` does not read the clock, and `policy show` is read for its first line, its path and its hash). Restore the file.

A control that stops failing means the test no longer tests the rule: the card is BLOCKED.

## Rollback

Revert the merge commit. The intake documents return to their earlier text, which is correct for the first edition and less exact about the company policy. A copy already sent to IT keeps the text it was sent with; send the corrected files with the next transfer.

## BLOCKED

- A statement and the program disagree and the document cannot be made true by rewording (for example the document claims a control the program does not have). Set the item to Blocked with `BLOCKED: <the sentence> is not true of the program`, and stop. Do not weaken the statement to fit and do not change the program in this card.
- The example fails `fieldbook policy check`, or its copy without the `example = true` line is Closed under `policy show`, and the finding is about a rule of the reference, not about the example: set the item to Blocked with `BLOCKED: reference`, name the rule, and stop. A change to the reference is an owner decision.
- `policy show`, `policy check` or `audit export` is missing: set the item to Blocked with `BLOCKED: no policy verbs`, and stop.
