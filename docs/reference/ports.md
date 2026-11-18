# Ports
<!-- generated:ports -->
Generated from `fieldbook::adapters::registry::ADAPTERS` and `fieldbook_core::ports::templates::TEMPLATES` by `cargo xtask docs`. Do not edit by hand.

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
<!-- /generated:ports -->
