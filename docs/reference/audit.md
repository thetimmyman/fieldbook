# Audit events
<!-- generated:audit -->
Generated from `fieldbook_core::audit::Kind` and [audit-contract.md](audit-contract.md) by `cargo xtask docs`. Do not edit by hand.

## Envelope

Every event line carries these envelope keys once:

| Field | Type | Meaning |
|---|---|---|
| `seq` | `Count` | 1-based event sequence number |
| `time` | `Now` | UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`) |
| `kind` | `Kind` | Event kind discriminator |
| `actor` | `Principal` | Operating-system or local principal (`os:<DOMAIN>\<user>` or `local`) |
| `role` | `Role` | Declared role (`owner`, `lead`, `agent`) |
| `store` | `StoreId` | Store identifier (`corpus`, `overlay`, `working`, `state`) |
| `via` | `Via` | Door (`cli`, `chat`, `mcp`) |
| `hash` | `Sha256` | Chain hash `SHA-256(prev_hex || 0x0A || body)` |

## Event kinds

| Kind | Fields (`name: Type`) | Emitted by | Egress |
|---|---|---|---|
| `journal.open` | `writer: Count`, `event_schema: Count`, `tool_version: SemVer`, `stores: Vec<StoreUid>` | Journal::open | - |
| `journal.recover` | `reason: Word`, `last_seq: Count`, `last_hash: Sha256` | Journal::open, audit verify --repair | - |
| `write.begin` | `verb: Verb`, `labels: LabelSet`, `files: Count`, `file_hashes: Option<Vec<FileHash>>`, `affected: Vec<RecordRef>`, `dest: Option<DestKind>`, `path_sha256: Option<Sha256>`, `policy: Option<Sha256>` | add, new, fmt, verify, build, queue accept, decide, migrate, clean, audit export | - |
| `write.end` | `verb: Verb`, `labels: LabelSet`, `affected: Vec<RecordRef>`, `outcome: Word`, `dest: Option<DestKind>`, `path_sha256: Option<Sha256>`, `policy: Option<Sha256>` | add, new, fmt, verify, build, queue accept, decide, migrate, clean, audit export | - |
| `refuse` | `code: Code`, `reason: Word`, `sha256: Option<Sha256>`, `name_sha256: Option<Sha256>`, `labels: Option<LabelSet>`, `dest: Option<DestKind>`, `verb: Option<Verb>`, `system: Option<StreamKey>`, `stream: Option<StreamKey>` | any gated verb | - |
| `queue.propose` | `labels: LabelSet` | add --propose, sync | - |
| `queue.accept` | `labels: LabelSet` | queue accept | - |
| `queue.reject` | `labels: LabelSet` | queue reject | - |
| `ai.begin` | `tx: Count`, `purpose: Word`, `dest: DestKind`, `labels: LabelSet`, `policy: Sha256`, `adapter: Word`, `endpoint: EndpointName`, `model: ModelId`, `template: Word`, `prompt_sha256: Sha256`, `context_ids: Vec<RecordRef>` | ask, decide, ai test, chat, queue ingest | egress |
| `ai.call` | `tx: Count`, `outcome: Word`, `code: Option<Code>`, `attempts: Count`, `output_sha256: Option<Sha256>`, `tokens_in: Count`, `tokens_out: Count` | ask, decide, ai test, chat, queue ingest | - |
| `ai.pack` | `prompt_sha256: Sha256`, `context_ids: Vec<RecordRef>`, `dest: DestKind`, `labels: LabelSet`, `policy: Sha256` | ask, context, decide, queue ingest | egress |
| `ai.paste` | `pack_seq: Option<Count>`, `output_sha256: Sha256`, `labels: LabelSet` | ask --resume, new note --ai-drafted, decide --resume, queue ingest --from | - |
| `ask` | `asked: Option<Id>`, `asked_via: AskedVia`, `question_sha256: Sha256`, `result_count: Count`, `outcome: Word`, `labels: LabelSet` | search, context, ask, chat | - |
| `mcp.session` | `protocol: Word`, `client_sha256: Option<Sha256>`, `allow: LabelAllow` | mcp | - |
| `mcp.call` | `tool: Word`, `args_sha256: Sha256`, `result_ids: Vec<RecordRef>`, `dest: DestKind`, `labels: LabelSet`, `policy: Sha256`, `outcome: Word` | mcp | egress |
| `chat.session` | `phase: Word`, `calls: Count` | chat | - |
| `chat.pack` | `phase: Word`, `name_sha256: Sha256`, `count: Count` | chat | - |
| `backup.begin` | `labels: LabelSet`, `dest: DestKind`, `policy: Sha256`, `snapshot: Count` | backup | egress |
| `backup.end` | `outcome: Word`, `files: Count`, `bytes: Count`, `manifest_sha256: Sha256`, `journal_seq: Count`, `journal_head: Sha256`, `fingerprint: Sha256`, `target_kind: Word` | backup | - |
| `backup.verify` | `verification: Word`, `outcome: Word`, `target_sha256: Option<Sha256>`, `snapshot: Option<Count>`, `manifest_sha256: Option<Sha256>`, `path_sha256: Option<Sha256>`, `public_support_sha256: Option<Sha256>` | backup, backup verify | - |
| `backup.prune` | `snapshots: Vec<Count>`, `reason: Word` | backup | - |
| `restore.begin` | `snapshot: Count`, `mode: Word`, `labels: LabelSet`, `dest: Option<DestKind>`, `policy: Option<Sha256>` | restore | - |
| `restore.end` | `snapshot: Count`, `mode: Word`, `outcome: Word`, `code: Option<Code>` | restore | - |
| `settings.change` | `names: Vec<SettingsGroup>`, `baseline: SettingsBaseline` | Journal::open | - |
| `purge` | `what: Word`, `count: Count` | search, ask, decide, chat | - |
| `policy.change` | `policy: Sha256`, `prev: Sha256`, `revision: Count`, `outcome: Word`, `labels_n: Count`, `rows_n: Count` | Journal::open | - |
| `label.change` | `ids: Vec<RecordRef>`, `added: LabelAllow`, `removed: LabelAllow`, `origin: Word` | label add, label reapply | - |
| `audit.export` | `from_seq: Count`, `to_seq: Count`, `head: Sha256`, `policy: Sha256`, `store_uid: StoreUid` | audit export | - |
| `store.missing` | `store_uid: StoreUid` | Journal::open | - |
| `store.adopted` | `store_uid: StoreUid`, `mode: Word`, `writer: Count` | init --overlay --adopt | - |
| `sync.begin` | `system: StreamKey`, `stream: StreamKey`, `adapter: Word`, `root_sha256: Option<Sha256>`, `policy: Sha256` | sync | - |
| `sync.end` | `system: StreamKey`, `stream: StreamKey`, `scope: Word`, `seen: Count`, `proposed: Count`, `present: Count`, `skipped: Count`, `refused: Count` | sync | - |
<!-- /generated:audit -->
