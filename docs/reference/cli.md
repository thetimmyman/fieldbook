# Command line
<!-- generated:cli -->
Generated from the command-line parser, the [Check catalogue](corpus-format.md#check-catalogue) and `tests/golden/json/` by `cargo xtask docs`. Do not edit by hand.

Offline finance reference and Corpus toolkit.

## Global options

| Option | Value | Default | Help |
|---|---|---|---|
| `--version` | - | - | Print version and build identity |

## Commands

### `fieldbook check`

Check the Corpus and print findings

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `--strict` | - | - | Treat strict-level rules as errors |
| `--today` | `YYYY-MM-DD` | - | Calendar date (YYYY-MM-DD) for freshness and as_of checks |
| `--json` | - | - | Print the report as a JSON object |
| `<PATHS>` | `PATHS` | - | Optional paths (relative to the working directory) to filter diagnostics |

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Clean (zero errors; warnings do not change the exit code) |
| `1` | Findings (at least one error) |
| `2` | Usage error (invalid command-line arguments or configuration refusal) |
| `3` | I/O or operational failure |

## Strict rules (`--strict`)

| Code | Check |
|---|---|
| `FB0118` | A page's `as_of` is later than the oldest `as_of` among the claims it cites |
| `FB0230` | A question with `day_one` set to `true` has an empty cell among `owner_role`, `blocks`, `why_it_matters`, `default_assumption` and `work_can_continue` |
| `FB0320` | A `stable` page cites a consequential claim whose `verification` is not `primary_source_supported` |

## `check --json` layout

| Key | Type |
|---|---|
| `report_version` | `integer` |
| `diagnostics` | `array` |
| `errors` | `integer` |
| `notes` | `array` |
| `strict` | `boolean` |
| `today` | `string` |
| `warnings` | `integer` |

## JSON reports

### `check`

```json
{
  "report_version": 1,
  "strict": false,
  "today": "2026-03-01",
  "errors": 1,
  "warnings": 0,
  "notes": [
    "originals check skipped (no corpus/sources/public/)"
  ],
  "diagnostics": [
    {
      "code": "FB0100",
      "severity": "error",
      "path": "corpus/corpus.toml",
      "line": 1,
      "col": 1,
      "message": "missing corpus/corpus.toml",
      "help": "Fix corpus/corpus.toml (or store.toml in a company store) so it is valid TOML at a supported schema_version with well-formed areas and views."
    }
  ]
}
```
<!-- /generated:cli -->
