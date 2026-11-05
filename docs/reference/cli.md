# Command line
<!-- generated:cli -->
Generated from the command-line parser, the [Check catalogue](corpus-format.md#check-catalogue) and `tests/golden/json/` by `cargo xtask docs`. Do not edit by hand.

Offline finance reference and Corpus toolkit.

## Global options

| Option | Value | Default | Help |
|---|---|---|---|
| `--version` | - | - | Print version and build identity |
| `--state` | `PATH` | - | Path to the state folder (defaults to `.fieldbook/` beside the Corpus) |

## Commands

### `fieldbook check`

Check the Corpus and print findings

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `--strict` | - | - | Treat strict-level rules as errors |
| `--today` | `YYYY-MM-DD` | - | Calendar date (YYYY-MM-DD) for freshness and as_of checks |
| `--json` | - | - | Print the report as a JSON object |
| `<PATHS>` | `PATHS` | - | Optional paths (relative to the working directory) to filter diagnostics |

### `fieldbook audit`

Inspect or verify the audit journal

| Argument / flag | Value | Default | Help |
|---|---|---|---|

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

### `audit-show`

```json
{
  "report_version": 1,
  "verb": "audit show",
  "head": {
    "seq": 12,
    "hash": "333c06e7324ba0462583de262a461c8935ccabd7bbf915ed76d9d7c3989b59bf"
  },
  "count": 12,
  "shown": 3,
  "events": [
    {
      "seq": 10,
      "time": "2026-03-01T12:00:00Z",
      "kind": "write.begin",
      "actor": "local",
      "role": "lead",
      "store": "corpus",
      "via": "cli",
      "verb": "new page",
      "labels": [
        "public"
      ],
      "files": 1,
      "file_hashes": [
        {
          "path_sha256": "5151515151515151515151515151515151515151515151515151515151515151",
          "sha256": "5252525252525252525252525252525252525252525252525252525252525252"
        }
      ],
      "affected": [
        "corpus/fnd-overview"
      ],
      "hash": "078274465a4703aa1876e0e5e5a747dd26dcc1959b3166e7738f01a41f4bb8a2"
    },
    {
      "seq": 11,
      "time": "2026-03-01T12:00:00Z",
      "kind": "write.end",
      "actor": "local",
      "role": "lead",
      "store": "corpus",
      "via": "cli",
      "verb": "new page",
      "labels": [
        "public"
      ],
      "affected": [
        "corpus/fnd-overview"
      ],
      "outcome": "ok",
      "hash": "0d70a231c53782f8f75b9f31ab386c3c75f979492bda7e11e0c8b049d213dc61"
    },
    {
      "seq": 12,
      "time": "2026-03-01T12:00:00Z",
      "kind": "backup.verify",
      "actor": "local",
      "role": "lead",
      "store": "corpus",
      "via": "cli",
      "verification": "snapshot",
      "outcome": "ok",
      "target_sha256": "6161616161616161616161616161616161616161616161616161616161616161",
      "snapshot": 1,
      "manifest_sha256": "6262626262626262626262626262626262626262626262626262626262626262",
      "hash": "333c06e7324ba0462583de262a461c8935ccabd7bbf915ed76d9d7c3989b59bf"
    }
  ],
  "exit": 0
}
```

### `audit-verify`

```json
{
  "report_version": 1,
  "verb": "audit verify",
  "state": "ok",
  "events": 12,
  "head": {
    "seq": 12,
    "hash": "333c06e7324ba0462583de262a461c8935ccabd7bbf915ed76d9d7c3989b59bf"
  },
  "findings": [],
  "unfinished": [],
  "exit": 0
}
```

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
