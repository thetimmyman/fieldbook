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

### `fieldbook build`

Build the static site and workbooks into an edition directory

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `--out` | `DIR` | `out` | Parent output directory (`DIR/edition` holds the built site) |

### `fieldbook add`

Add a source or source version to the Corpus

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `<FILE>` | `FILE` | - | Path to the source file or folder to add |
| `--source` | `SOURCE-ID` | - | Existing source ID (`S-nnnn`) when adding a new version of an existing source |
| `--metadata-only` | - | - | Append the register row without storing a copy of the original file |
| `--confirm-public` | - | - | Confirm that the unlabelled file is public and may enter the repository Corpus |
| `--recursive` | - | - | Descend into subfolders when adding a folder |
| `--json` | - | - | Print the report as a JSON object |

### `fieldbook new`

Create a new page or register row in its entry state

| Argument / flag | Value | Default | Help |
|---|---|---|---|

### `fieldbook audit`

Inspect or verify the audit journal

| Argument / flag | Value | Default | Help |
|---|---|---|---|

### `fieldbook search`

Find an id or a word in the Corpus; list the versions of a source

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `--json` | - | - | Print the results as a JSON object |
| `--units` | - | - | List every matching unit in rank order instead of grouping sections into their page |
| `<QUERY>` | `QUERY` | - | The words to search for; they are joined with single spaces, so quoting is optional. Put `--` before a word that starts with `-` |

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

### `add`

```json
{
  "report_version": 1,
  "verb": "add",
  "dir": "inbox",
  "labels": [
    "owner-supplied",
    "unlabelled"
  ],
  "inputs": [
    {
      "store": "corpus",
      "id": "S-0004-v1"
    },
    {
      "store": "corpus",
      "id": "S-0005-v1"
    }
  ],
  "files": [
    {
      "name": "a-note.txt",
      "result": "added",
      "id": "S-0004-v1",
      "sha256": "1aa0592a6ef1b53921b5131ff20cb88ee9762c37b66ecf13f02b69c584a2cf76",
      "bytes": 35,
      "needs_extraction": false
    },
    {
      "name": "b-copy.txt",
      "result": "present",
      "id": "S-0004-v1",
      "sha256": "1aa0592a6ef1b53921b5131ff20cb88ee9762c37b66ecf13f02b69c584a2cf76",
      "bytes": 35,
      "needs_extraction": false
    },
    {
      "name": "c-tool.ps1",
      "result": "refused",
      "sha256": "f3a8b50c420c9dddd9bc1d419dd06cc37f01a302989c2054fbb1503ac7ed1203",
      "bytes": 52,
      "needs_extraction": false,
      "code": "FB0821"
    },
    {
      "name": "d-scan.pdf",
      "result": "added",
      "id": "S-0005-v1",
      "sha256": "d7db14408935826d640466e3c20f37ebdae1b5b06b3b54bd0a6afea34c52bea5",
      "bytes": 95,
      "needs_extraction": true
    }
  ],
  "counts": {
    "added": 2,
    "present": 1,
    "refused": 1,
    "needs_extraction": 1
  },
  "exit": 1
}
```

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

### `search-pages`

```json
{
  "report_version": 1,
  "query": "threshold",
  "tokens": [
    "threshold"
  ],
  "view": "pages",
  "total": 2,
  "results": [
    {
      "rank": 1,
      "id": "cmp-withhold-case",
      "kind": "page",
      "title": "Synthetic case study on business-system review and billing withhold",
      "parent": "",
      "status": "draft",
      "as_of": "2026-02-01",
      "effective_from": "",
      "verified_on": "",
      "labels": [
        "public"
      ],
      "store": "corpus",
      "matched": 1,
      "score": 2223,
      "snippet": "The synthetic review board recorded a 30 days cycle [@CL-CMP-0001] and a 5 percent billing withhold threshold [@CL-CMP-0002].",
      "best_unit": "cmp-withhold-case#how-it-works",
      "matched_units": 1
    },
    {
      "rank": 2,
      "id": "CL-CMP-0002",
      "kind": "claim",
      "title": "CL-CMP-0002",
      "parent": "",
      "status": "unchecked",
      "as_of": "2026-02-01",
      "effective_from": "2026-01-15",
      "verified_on": "",
      "labels": [
        "public"
      ],
      "store": "corpus",
      "matched": 1,
      "score": 3923,
      "snippet": "Synthetic withholding threshold is 5 percent of billed amounts."
    }
  ]
}
```

### `search`

```json
{
  "report_version": 1,
  "query": "S-0002",
  "tokens": [
    "s-0002"
  ],
  "view": "pages",
  "total": 2,
  "results": [
    {
      "rank": 1,
      "id": "S-0002-v1",
      "kind": "source",
      "title": "Synthetic Pricing Guide v1, made up for tests",
      "parent": "",
      "status": "active",
      "as_of": "2026-01-15",
      "effective_from": "2025-11-01",
      "verified_on": "",
      "labels": [
        "public"
      ],
      "store": "corpus",
      "matched": 1,
      "score": 5055,
      "snippet": "Synthetic fixture first version for automated tests."
    },
    {
      "rank": 2,
      "id": "S-0002-v2",
      "kind": "source",
      "title": "Synthetic Pricing Guide v2, made up for tests",
      "parent": "",
      "status": "active",
      "as_of": "2026-02-01",
      "effective_from": "2026-01-20",
      "verified_on": "",
      "labels": [
        "public"
      ],
      "store": "corpus",
      "matched": 1,
      "score": 5751,
      "snippet": "Synthetic fixture second version superseding S-0002-v1."
    }
  ]
}
```
<!-- /generated:cli -->
