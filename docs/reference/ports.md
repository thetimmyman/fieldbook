# Ports
<!-- generated:ports -->
Generated from `fieldbook::adapters::registry::ADAPTERS` and `fieldbook_core::ports::templates::TEMPLATES` by `cargo xtask docs`. Do not edit by hand.

## index

| ID | Default | State | Build | Crate | Needs | Sees | Settings keys | Fixtures | Suite |
|---|---|---|---|---|---|---|---|---|---|
| `bm25f@1` | yes | built | silent | `fieldbook-core` | none | public | - | `tests/fixtures/ports/index/bm25f@1/` | `index_suite` |

### index bm25f@1

Contract test `contract_index_bm25f_1`; model calls: no.

## extractor

| ID | Default | State | Build | Crate | Needs | Sees | Settings keys | Fixtures | Suite |
|---|---|---|---|---|---|---|---|---|---|
| `plain@1` | yes | built | silent | `fieldbook-core` | none | labelled | - | `tests/fixtures/ports/extractor/plain@1/` | `extractor_contract` |
| `html-strip@1` | no | built | silent | `fieldbook-core` | none | labelled | - | `tests/fixtures/ports/extractor/html-strip@1/` | `extractor_contract` |
| `pdftotext-layout@1` | no | built | silent | `xtask` | process | public | - | `tests/fixtures/ports/extractor/pdftotext-layout@1/` | `extractor_contract` |
| `staged@1` | no | built | silent | `fieldbook` | files | public | - | `tests/fixtures/ports/extractor/staged@1/` | `extractor_contract` |

### extractor plain@1

Contract test `contract_extractor_plain_1`; model calls: no.

### extractor html-strip@1

Contract test `contract_extractor_html_strip_1`; model calls: no.

### extractor pdftotext-layout@1

Contract test `contract_extractor_pdftotext_layout_1`; model calls: no.

### extractor staged@1

Contract test `contract_extractor_staged_1`; model calls: no.

## classifier

| ID | Default | State | Build | Crate | Needs | Sees | Settings keys | Fixtures | Suite |
|---|---|---|---|---|---|---|---|---|---|
| `rules@1` | yes | built | silent | `fieldbook-core` | none | labelled | - | `tests/fixtures/ports/classifier/rules@1/` | `classifier_suite` |

### classifier rules@1

Contract test `contract_classifier_rules_1`; model calls: no.

## correlator

| ID | Default | State | Build | Crate | Needs | Sees | Settings keys | Fixtures | Suite |
|---|---|---|---|---|---|---|---|---|---|
| `exact-ids+bm25@1` | yes | built | silent | `fieldbook-core` | none | public | - | `tests/fixtures/ports/correlator/exact-ids+bm25@1/` | `correlator_suite` |

### correlator exact-ids+bm25@1

Contract test `contract_correlator_exact_ids_bm25_1`; model calls: no.

## admit

| ID | Default | State | Build | Crate | Needs | Sees | Settings keys | Fixtures | Suite |
|---|---|---|---|---|---|---|---|---|---|
| `corpus-add@1` | yes | built | silent | `fieldbook-core` | none | public | - | `tests/fixtures/ports/admit/corpus-add@1/` | `admit_suite` |

### admit corpus-add@1

Contract test `contract_admit_corpus_add_1`; model calls: no.

## backup

| ID | Default | State | Build | Crate | Needs | Sees | Settings keys | Fixtures | Suite |
|---|---|---|---|---|---|---|---|---|---|
| `folder@1` | yes | built | silent | `fieldbook` | files | labelled | - | `tests/fixtures/ports/backup/folder@1/` | `backup_target_contract` |

### backup folder@1

Contract test `contract_backup_folder_1`; model calls: no.

## authz

| ID | Default | State | Build | Crate | Needs | Sees | Settings keys | Fixtures | Suite |
|---|---|---|---|---|---|---|---|---|---|
| `policy-readers@1` | yes | built | silent | `fieldbook-core` | none | public | - | `tests/fixtures/ports/authz/policy-readers@1/` | `authz_suite` |

### authz policy-readers@1

Contract test `contract_authz_policy_readers_1`; model calls: no.

## store

| ID | Default | State | Build | Crate | Needs | Sees | Settings keys | Fixtures | Suite |
|---|---|---|---|---|---|---|---|---|---|
| `folder@1` | yes | built | silent | `fieldbook` | files | labelled | - | `tests/fixtures/ports/store/folder@1/` | `store_suite` |

### store folder@1

Contract test `contract_store_folder_1`; model calls: no.

## Templates

| ID | Purpose | Parts | Schema |
|---|---|---|---|
| `ask@1` | ask | rules, history, context, question | none |

## Settings keys

| Table | Key | Type | Default | Adapter |
|---|---|---|---|---|
| [model] | order | list of strings | ["handoff"] | - |
| [model] | retries | integer (1..=10) | 2 | - |
| [model] | max_calls_per_run | integer (>= 1) | 100 | - |
| [model] | max_out_tokens | integer (>= 1) | 1500 | - |
| [model] | keep_calls | boolean | true | - |
| [model] | keep_days | integer (>= 0) | 30 | - |
| [model] | proxy | "env" or "none" | "env" | - |
| [model.handoff] | allow | list of label ids | none (no narrowing) | handoff@1 |
| [model.handoff] | assistant_label | string | none | handoff@1 |
| [model.endpoint.<name>] | adapter | string (Model adapter id) | required | - |
| [model.endpoint.<name>] | base_url | string (URL with host) | required | - |
| [model.endpoint.<name>] | model | string (ModelId) | required | - |
| [model.endpoint.<name>] | allow | list of label ids | none (accepts nothing) | - |
| [model.endpoint.<name>] | timeout_s | integer (1..=600) | 60 | - |
| [model.endpoint.<name>] | key_env | string (env var name) | none | - |
| [model.endpoint.<name>] | auth | "bearer" or "api-key" | "bearer" | - |
| [search] | semantic | boolean | false | semantic@1 |
| [mcp] | allow | list of label ids | none (no narrowing) | - |
| [chat] | idle_minutes | integer (1..=1440) | 30 | - |
<!-- /generated:ports -->
