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

### `fieldbook fmt`

Normalize registers and page bodies under `corpus/`

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `--check` | - | - | List each file that would change and exit 1 without writing |

### `fieldbook build`

Build the static site and workbooks into an edition directory

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `--out` | `DIR` | `out` | Parent output directory (`DIR/edition` holds the built site) |
| `--watch` | - | - | Rebuild when the Corpus changes (checks every 500 ms; Ctrl-C stops) |
| `--since` | `EDITION` | - | Path of an earlier edition's folder or `edition.toml`, for `changes.html` |

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

### `fieldbook status`

Show what needs work, and optionally write the desk pages

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `--today` | `YYYY-MM-DD` | - | Calendar date (YYYY-MM-DD) for freshness, instead of the clock |
| `--page` | - | - | Write the desk pages into `pages/` of the state folder |

### `fieldbook verify`

Record that the owner (or lead) verified a claim, at a terminal

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `<CLAIM-ID>` | `CLAIM-ID` | - | The claim to verify (`CL-AAA-nnnn`) |
| `--on` | `YYYY-MM-DD` | - | The calendar date (YYYY-MM-DD) the original was read at the locator |

### `fieldbook backup`

Create, verify, or list backup snapshots

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `--target` | `ID` | - | Backup target identifier in settings.toml |
| `--to` | `DIR` | - | Destination directory for a one-off backup |
| `--dry-run` | - | - | Perform checks and report what would be written without writing anything |

### `fieldbook restore`

Restore a backup snapshot into a new folder or run the restore drill

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `--snapshot` | `N` | - | Snapshot number to restore |
| `--to` | `DIR` | - | Restore into this new or empty folder |
| `--drill` | - | - | Restore into the drill folder under the state folder, check the copy, then delete it |
| `--target` | `ID` | - | Backup target identifier in settings.toml |
| `--from` | `DIR` | - | Read the snapshot from this folder instead of a settings target |

### `fieldbook policy`

Show the active company policy or validate a draft policy file

| Argument / flag | Value | Default | Help |
|---|---|---|---|

### `fieldbook ai`

Inspect AI settings, adapter readiness, policy allow sets and key state

| Argument / flag | Value | Default | Help |
|---|---|---|---|

### `fieldbook context`

Build a plain-text context pack for a question from the permitted view

| Argument / flag | Value | Default | Help |
|---|---|---|---|
| `<QUESTION>` | `QUESTION` | - | Question text used to select and budget the context pack |
| `--scope` | `SCOPE` | - | Restrict selection to one area (`area:<code>`) or lead with one page (`page:<id>`) |
| `--budget` | `N` | - | Maximum Unicode-scalar character budget (defaults to `[assistant] pack_budget_chars`) |
| `--include` | `CLASSES` | - | Comma-separated exclusion overrides (`tier-d`, `superseded`, `contradicted`) |
| `--json` | - | - | Print the context report as a JSON object |

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

### `ai-status`

```json
{
  "report_version": 1,
  "policy": "floor",
  "adapters": [
    {
      "port": "index",
      "adapter": "bm25f@1",
      "state": {
        "kind": "Ready"
      },
      "allow": [],
      "key": "NotNeeded",
      "caps": {
        "network": false,
        "structured": false,
        "embeddings": false,
        "streaming": false,
        "max_prompt_chars": 0
      }
    },
    {
      "port": "extractor",
      "adapter": "plain@1",
      "state": {
        "kind": "Ready"
      },
      "allow": [],
      "key": "NotNeeded",
      "caps": {
        "network": false,
        "structured": false,
        "embeddings": false,
        "streaming": false,
        "max_prompt_chars": 0
      }
    },
    {
      "port": "classifier",
      "adapter": "rules@1",
      "state": {
        "kind": "Ready"
      },
      "allow": [],
      "key": "NotNeeded",
      "caps": {
        "network": false,
        "structured": false,
        "embeddings": false,
        "streaming": false,
        "max_prompt_chars": 0
      }
    },
    {
      "port": "correlator",
      "adapter": "exact-ids+bm25@1",
      "state": {
        "kind": "Ready"
      },
      "allow": [],
      "key": "NotNeeded",
      "caps": {
        "network": false,
        "structured": false,
        "embeddings": false,
        "streaming": false,
        "max_prompt_chars": 0
      }
    },
    {
      "port": "admit",
      "adapter": "corpus-add@1",
      "state": {
        "kind": "Ready"
      },
      "allow": [],
      "key": "NotNeeded",
      "caps": {
        "network": false,
        "structured": false,
        "embeddings": false,
        "streaming": false,
        "max_prompt_chars": 0
      }
    },
    {
      "port": "backup",
      "adapter": "folder@1",
      "state": {
        "kind": "Ready"
      },
      "allow": [],
      "key": "NotNeeded",
      "caps": {
        "network": false,
        "structured": false,
        "embeddings": false,
        "streaming": false,
        "max_prompt_chars": 0
      }
    },
    {
      "port": "authz",
      "adapter": "policy-readers@1",
      "state": {
        "kind": "Ready"
      },
      "allow": [],
      "key": "NotNeeded",
      "caps": {
        "network": false,
        "structured": false,
        "embeddings": false,
        "streaming": false,
        "max_prompt_chars": 0
      }
    },
    {
      "port": "store",
      "adapter": "folder@1",
      "state": {
        "kind": "Ready"
      },
      "allow": [],
      "key": "NotNeeded",
      "caps": {
        "network": false,
        "structured": false,
        "embeddings": false,
        "streaming": false,
        "max_prompt_chars": 0
      }
    }
  ],
  "exit": 0
}
```

### `audit-export`

```json
{
  "report_version": 1,
  "verb": "audit export",
  "written": true,
  "events": 13,
  "first_seq": 1,
  "last_seq": 13,
  "head": "6f0f368f4aea5c2d06d4ff3d6bd532cc47890bb88219457baf5fd1075950ad39",
  "file": "journal-w0-5d524e23e0cd-000001-13.jsonl",
  "manifest": "journal-w0-5d524e23e0cd-000001-13.manifest.toml",
  "exit": 0
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

### `audit-verify-export`

```json
{
  "report_version": 1,
  "verb": "audit verify",
  "manifest": "journal-w0-5d524e23e0cd-000001-13.manifest.toml",
  "state": "ok",
  "events": 13,
  "first_seq": 1,
  "last_seq": 13,
  "head": "6f0f368f4aea5c2d06d4ff3d6bd532cc47890bb88219457baf5fd1075950ad39",
  "findings": [],
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

### `backup-list`

```json
{
  "report_version": 1,
  "verb": "backup list",
  "target": {
    "id": "it",
    "kind": "it-location",
    "path": "backups",
    "company_storage": false
  },
  "warn_after_days": 7,
  "keep_last": 7,
  "keep_weekly": 4,
  "snapshots": [
    {
      "snapshot": 3,
      "backup_version": 1,
      "tool_version": "0.1.0",
      "created_utc": "2026-01-01T09:00:00Z",
      "labels": [
        "public"
      ],
      "roots": [
        "corpus",
        "state"
      ],
      "files": 412,
      "bytes": 9120344,
      "fingerprint": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
      "journal_seq": 118,
      "journal_head": "b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3",
      "journal_state": "ok",
      "manifest_sha256": "c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4",
      "verified": true,
      "verified_utc": "2026-01-01T09:05:00Z",
      "latest_verification": {
        "outcome": "failed",
        "utc": "2026-01-01T10:00:00Z"
      }
    }
  ],
  "verified_utc": "2026-01-01T09:05:00Z",
  "drill": {
    "mode": "drill",
    "outcome": "ok",
    "utc": "2026-01-02T10:00:00Z"
  },
  "findings": [],
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

### `context`

```json
{
  "report_version": 1,
  "verb": "context",
  "pack": {
    "request_budget": {
      "limit": 12000,
      "rules": 303,
      "question": 21,
      "history": 0,
      "context": 1935,
      "total": 2259,
      "sendable": true,
      "over_by": 0
    },
    "question": "synthetic",
    "scope": "area:cmp",
    "items": [
      {
        "unit": "XC-01",
        "origin": "auto",
        "why": "hit",
        "included": true,
        "stale": false
      },
      {
        "unit": "Q-001",
        "origin": "auto",
        "why": "hit",
        "included": true,
        "stale": false
      },
      {
        "unit": "CL-CMP-0002",
        "origin": "auto",
        "why": "hit",
        "included": true,
        "stale": false
      },
      {
        "unit": "CL-CMP-0001",
        "origin": "auto",
        "why": "closure",
        "via": "CL-CMP-0002",
        "included": true,
        "stale": false
      },
      {
        "unit": "S-0001-v1",
        "origin": "auto",
        "why": "closure",
        "via": "CL-CMP-0002",
        "included": true,
        "stale": false
      },
      {
        "unit": "billing_withhold",
        "origin": "auto",
        "why": "closure",
        "via": "CL-CMP-0002",
        "included": true,
        "stale": false
      },
      {
        "unit": "cmp-system-review#purpose",
        "origin": "auto",
        "why": "hit",
        "included": true,
        "stale": false
      },
      {
        "unit": "system_review",
        "origin": "auto",
        "why": "closure",
        "via": "cmp-system-review#purpose",
        "included": true,
        "stale": false
      },
      {
        "unit": "cmp-onboarding-checklist#how-it-works",
        "origin": "auto",
        "why": "hit",
        "included": true,
        "stale": false
      },
      {
        "unit": "cmp-review-steps#how-it-works",
        "origin": "auto",
        "why": "hit",
        "included": true,
        "stale": false
      }
    ],
    "chars": 2259,
    "budget": 12000,
    "labels": [
      "owner-supplied"
    ],
    "left_out_line": "Left out (policy): 0 records",
    "view_fingerprint": "8b1eb3bd631f020960ee65e04fb9712cf057d069f00399f493912b06b6668f7d"
  },
  "pack_text": "rules_version = 1\n\nSynthetic assistant rules for the mini Corpus fixture.\nUse only the supplied context pack and cite every factual statement exactly as shown.\nIf the context pack does not establish an answer, reply: Not established in the library.\n\nLabels: owner-supplied\n\nQuestion: synthetic\n\n--- conflict XC-01 ---\nTitle: Synthetic review interval versus withholding check timing\nAs of: unknown\nStatus: unresolved\nPosition: Synthetic review runs every 30 days\nPosition: Synthetic withholding check runs at each billing cycle\nClaims: CL-CMP-0001, CL-CMP-0002\ncite as: [@XC-01]\n\n--- question Q-001 ---\nTitle: Which synthetic business-system control owner approves the monthly compliance review?\nAs of: unknown\nStatus: open\nText: Synthetic withholding exposure depends on timely review sign-off.\ncite as: [@Q-001]\n\n--- claim CL-CMP-0002 ---\nTitle: Synthetic withholding threshold is 5 percent of billed amounts.\nAs of: 2026-02-01\nStatus: unchecked\nSource: S-0001-v1, sec. 2\nQuote: \"Synthetic withholding threshold is 5 percent.\"\ncite as: [@CL-CMP-0002]\n\n--- claim CL-CMP-0001 ---\nTitle: Synthetic compliance reviews occur every 30 days under the synthetic rule.\nAs of: 2026-02-01\nStatus: contradicted (conflict XC-01)\nSource: S-0001-v1, sec. 1\nQuote: \"Synthetic compliance reviews occur every 30 days.\"\ncite as: [@CL-CMP-0001]\n\n--- source S-0001-v1 ---\nTitle: Synthetic Cost Rule, made up for tests\nAs of: 2026-02-01\nStatus: active\nText: Synthetic fixture source for automated tests.\ncite as: [@S-0001-v1]\n\n--- term billing_withhold ---\nTitle: billing withhold\nAs of: unknown\nStatus: public_reference\nText: Synthetic retention of a percentage of interim payments pending system correction.\n\n--- page cmp-system-review#purpose ---\nTitle: Purpose\nAs of: 2026-02-01\nStatus: draft\nText: Explain how a synthetic system review interacts with a synthetic billing withhold.\n\n--- term system_review ---\nTitle: system review\nAs of: unknown\nStatus: public_reference\nText: Synthetic periodic assessment of business-system control operation.\n\n--- page cmp-onboarding-checklist#how-it-works ---\nTitle: How it works\nAs of: 2026-02-01\nStatus: draft\n\n--- page cmp-review-steps#how-it-works ---\nTitle: How it works\nAs of: 2026-02-01\nStatus: draft\n\nLeft out (policy): 0 records\n"
}
```

### `policy-show`

```json
{
  "report_version": 1,
  "verb": "policy show",
  "state": "floor",
  "path": null,
  "hash": "4727fa3d5f222e1f9cc57a61475885429d9bfde29b756c08f9f0fb2e59ca57c3",
  "writer_check": "ok",
  "revision": 0,
  "recorded": null,
  "labels": [],
  "stores": [
    {
      "id": "corpus",
      "holds": [
        "public"
      ],
      "default": [
        "public"
      ],
      "hold": false
    },
    {
      "id": "overlay",
      "holds": [
        "public",
        "unlabelled"
      ],
      "default": [
        "unlabelled"
      ],
      "hold": false
    },
    {
      "id": "working",
      "holds": [
        "public",
        "unlabelled"
      ],
      "default": [
        "unlabelled"
      ],
      "hold": false
    },
    {
      "id": "state",
      "holds": [
        "owner-supplied",
        "public",
        "unlabelled"
      ],
      "default": [
        "unlabelled"
      ],
      "hold": false
    }
  ],
  "destinations": [
    {
      "kind": "handoff",
      "allow": [
        "owner-supplied",
        "public"
      ],
      "built_in": false,
      "advisory": true
    },
    {
      "kind": "model",
      "allow": [
        "public"
      ],
      "built_in": false,
      "advisory": false
    },
    {
      "kind": "mcp",
      "allow": [
        "public"
      ],
      "built_in": false,
      "advisory": false
    },
    {
      "kind": "bridge",
      "allow": [
        "public"
      ],
      "built_in": false,
      "advisory": false
    },
    {
      "kind": "print",
      "allow": [
        "public"
      ],
      "built_in": false,
      "advisory": true
    },
    {
      "kind": "clipboard",
      "allow": [
        "public"
      ],
      "built_in": false,
      "advisory": true
    },
    {
      "kind": "export",
      "allow": [
        "owner-supplied",
        "public"
      ],
      "built_in": false,
      "advisory": false
    },
    {
      "kind": "backup",
      "allow": [
        "owner-supplied",
        "public"
      ],
      "built_in": false,
      "advisory": false
    },
    {
      "kind": "audit",
      "allow": [
        "public"
      ],
      "built_in": true,
      "advisory": false
    },
    {
      "kind": "git",
      "allow": [
        "public"
      ],
      "built_in": true,
      "advisory": false
    },
    {
      "kind": "edition",
      "allow": [
        "public"
      ],
      "built_in": true,
      "advisory": false
    }
  ],
  "effective": [
    {
      "kind": "handoff",
      "allow": [
        "owner-supplied",
        "public"
      ],
      "cuts": []
    },
    {
      "kind": "model",
      "allow": [
        "public"
      ],
      "cuts": [
        {
          "label": "owner-supplied",
          "by": "policy"
        }
      ]
    },
    {
      "kind": "mcp",
      "allow": [
        "public"
      ],
      "cuts": [
        {
          "label": "owner-supplied",
          "by": "policy"
        }
      ]
    },
    {
      "kind": "bridge",
      "allow": [
        "public"
      ],
      "cuts": [
        {
          "label": "owner-supplied",
          "by": "policy"
        }
      ]
    },
    {
      "kind": "print",
      "allow": [
        "public"
      ],
      "cuts": [
        {
          "label": "owner-supplied",
          "by": "policy"
        }
      ]
    },
    {
      "kind": "clipboard",
      "allow": [
        "public"
      ],
      "cuts": [
        {
          "label": "owner-supplied",
          "by": "policy"
        }
      ]
    },
    {
      "kind": "export",
      "allow": [
        "owner-supplied",
        "public"
      ],
      "cuts": []
    },
    {
      "kind": "backup",
      "allow": [
        "owner-supplied",
        "public"
      ],
      "cuts": []
    }
  ],
  "exit": 0
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
