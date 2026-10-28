# Financial work

## Scope and period

`WorkScope { entity: String, project: Option<String>, object: Option<String> }` uses store-qualified stable record ids. In these financial-work contracts, the spelling `overlay:CM-000001` means the existing `ItemRef { store: Overlay, id: "CM-000001" }`; a version reference has the same spelling with a versioned id, for example `overlay:CM-000040-v1`. The store is a `StoreId` and the id must parse as the required existing id type. This spelling introduces no new ref-column kind: an ordinary `ref` cell keeps its existing version-id form and resolves in its owning store.

`RecordScope { Global, Work(WorkScope) }` distinguishes reference tables from scoped work. A project must belong to its entity and an object to the declared entity/project. A type's scope is a column mapping, for example `scope = { entity = "entity" }`, or the explicit word `"global"`. These are type declarations, not serialized scope values. A serialized value in a load, observation or issue manifest is `scope = { kind = "global" }` or `scope = { kind = "work", entity = "overlay:CM-000001", project = "overlay:CM-000002", object = "overlay:CM-000003" }`; absent optional work fields are omitted and global rejects work fields. A system's display-text scope remains text. A stream's financial scope is the serialized value, never the type-column mapping. Missing type scope is FB0240; missing or inconsistent row ownership is FB0241. No parser guesses one context from another's shape.

`Period { version: ItemRef, calendar: String, id: String, start: CivilDate, end: CivilDate }` is a resolved named financial period, including the immutable record version that supplies it. Its bounds are inclusive and `start` must not exceed `end`. A normal record type named `period` supplies `entity` (a ref to `entity`), `calendar` (text), `period_id` (text), `start` (date) and `end` (date), all required. Its type scope maps `entity`. Its business identity is the stable entity id plus `calendar`/`period_id`, and retained versions of one named period belong to one logical record; a different entity-record metadata version is not a different entity. This contract does not change the generic CSV key-joining rule. The `version` field must resolve to a version of that type in the coherent read generation. The period record's entity must resolve to the same stable entity as `WorkScope.entity`; all four supplied value fields must equal that version's `calendar`, `period_id`, `start` and `end`. The ids are opaque identifiers: a display month does not supply bounds. A correction creates a new period record version; no live-calendar lookup changes an existing binding.

A type's optional `period` key names one existing column of kind `ref` with `to = "period"`. Resolve that row's cell and construct `Period` from that exact version. Stream metadata, load plans and observation/issue manifests retain the resolved value and version. If a stream supplies period metadata, the same strict table shown below is required and checked against that exact version; it must agree with a row's period when the type declares one. Global forbids a period. Missing or malformed type mappings are FB0240; wrong type, unresolved version, inconsistent dates or cross-entity values are FB0241. All period/version inputs participate in effective labels, plan hashing and acceptance replanning.

`RecordWrite` keeps `period: Option<Period>`, and `ObservationWrite` keeps the same type. This is distinct from `fieldbook-calc`'s `EffectivePeriod`, which is an inclusive effective-date interval and may have an open end. Do not replace either type with the other.

Synthetic wire example (the ids are invented):

```toml
scope = { kind = "work", entity = "overlay:CM-000001" }
period = { version = "overlay:CM-000040-v1", calendar = "fiscal", id = "P09", start = "2025-09-01", end = "2025-09-30" }
```

Type declaration example:

```toml
scope = { entity = "entity" }
period = "accounting_period"

[[type.column]]
name = "accounting_period"
kind = "ref"
to = "period"
required = true
```

Period TOML is a strict value table, with exactly `version`, `calendar`, `id`, `start` and `end`, printed in that order; dates are quoted full `CivilDate` strings. Unknown or duplicate keys refuse. Absence is omission of the `period` field, never an empty table. Its canonical bytes reuse the `S(x)` framing defined for [Stored calculation runs](#stored-calculation-runs). `None` is the exact UTF-8 string `period none\n`. `Some` is the following sequence, with one LF after each record. `S(version)` is the canonical store-qualified version spelling. Canonical date strings are `YYYY-MM-DD`. Feed these exact bytes as the named period value to the existing canonical plan encoder; do not hash TOML or debug output. Changing only the bound version changes the hash even when its displayed dates agree.

```text
period 1
version S(version)
calendar S(calendar)
id S(id)
start S(start)
end S(end)
```

This adds one necessary field to the already-planned `Period` value. It does not add a new period port or calculation module. Resolve it before the append/observation lock check, and retain the same value during replanning. Preserve existing backdated/intersecting-date lock behavior.

`Locks::check(&RecordWrite) -> Result<(), LockRefusal>` and `Locks::check_observation(&ObservationWrite) -> Result<(), LockRefusal>` tests all applicable entity, project and object scopes. An entity lock includes its projects; a project lock does not lock another project. A period lock blocks intersecting effective dates and writes explicitly naming that period, including backdated rows. `NoLocks` is only for stores with no lock records; the real append path loads `StoreLocks` as soon as those records exist. `ObservationWrite { scope: RecordScope, period: Option<Period>, as_of: PartialDate }` is checked through RecordRegister::plan_append for every observation in BatchEvidence, even with writes = []. Global writes and observations still invoke the shared lock checker; unrelated entity/project/object locks do not apply to them. Unknown or unreadable lock state refuses every append, including Global and observation-only batches. A reopen record names the lock, exact scope, reason and approval; only its authorized committed form releases that lock. The next issue retains the old lock/reopen history.

## Evidence and calculation runs

Every nonempty `money`, `decimal` or `int` field is a figure. `evidence/<version-id>.toml` has `schema = 1`, `version_id` and a `[field.<column>]` table for each figure. Each table has `basis` (`quoted`, `computed`, `judgment`), `unit`, optional `currency`, and its evidence. Units use the calculation core's unit vocabulary; a count declares its counted unit. Currency is required for money. The record's `basis` is a default for authoring; the sidecar is authoritative per field and must agree with the field's value and dimension.

Quoted evidence has one typed `Pointer { target, locator }`. `target` is a source or record VersionId; `locator` is `None`, `CsvRow { row }`, `CsvCell { row, column }`, `SheetCell { sheet, row, column }` `RecordField { column }` or `TextQuote { locator, quote }`. `TextQuote.locator` uses the existing page, section or paragraph locator grammar. Resolve it to extracted source text, match the quote with the citation normalizer, and require the quoted number, unit and currency to agree with the field. CsvRow serializes as `VERSION rec N`; CsvCell as `VERSION rec N col NAME`. Row-only lineage never satisfies numeric evidence. Numeric evidence needs a resolvable cell, record field or located document quotation; a title alone is insufficient. Workbook rows and columns are 1-based and use extracted stored values, never formula evaluation. Missing facts are unresolved evidence (FB0381); an invalid locator is FB0311. Computed evidence has `run` and `output`, plus its input pointers. Judgment has a nonempty rationale. All input references feed effective labels and `rests_on`. Record-backed claim promotion uses [Promotion rules](corpus-format.md#promotion-rules): computed support is a result under named inputs, judgment support is a documented choice, and neither certifies an estimate as observed fact or approval.

## Load evidence input

A record load pairs its CSV with `<input filename>.evidence.toml`. The strict TOML envelope has `load_evidence_schema = 1`, `source_sha256` (the CSV’s full hash) and `[[field]]` entries sorted by `(row, column)`. `row` is the 1-based logical data-row number (the header is not counted); `column` is the exact declared numeric column name, case-sensitive. Each entry carries the existing field-evidence shape: basis, unit, currency when required, and exactly the quoted pointer, computed run/output/input pointers, or judgment rationale of [Evidence and calculation runs](#evidence-and-calculation-runs). Unknown keys, duplicate row/column pairs, out-of-range rows, unknown/non-numeric columns and mismatched source hashes refuse with FB0241 before staging. No CSV header extension or implicit evidence from a display hash is used.

Load pointers use `LoadTarget::Incoming` (TOML target `incoming`) or an existing store-qualified source/record VersionId. Their locator uses the existing typed locator. Incoming means this CSV’s source hash and never another input; numeric evidence needs its exact CsvCell row and column. Planning validates against the incoming extracted cells before an ID exists. Materialization substitutes the allocated source VersionId in every Incoming pointer, preserving the locator, and validates the resulting immutable evidence. Existing pointers resolve through the coherent source/record view. `LoadEvidence::parse(&str) -> Result<LoadEvidence, LoadEvidenceError>` and `build_batch(&Type, &LoadFile, &RecordIndex, &LoadEvidence, &LoadEvidenceContext) -> Result<Batch, BatchRefusal>` are pure; the context supplies the current RunCatalog, existing pointer facts and incoming cell facts. A computed field resolves a retained valid run and named output; it never creates a run from a hash.

Every nonempty numeric field supplied by the CSV requires exactly one evidence entry, including equal-value rows. A numeric column omitted from a revision keeps the base value and its evidence; an omitted field on a new record is empty and has no entry. A load with no supplied figures may omit the companion, represented explicitly as an empty LoadEvidence bound to the CSV hash. A companion cannot override evidence for an omitted column.

Discover companions before key-template matching, as for observation companions, and never treat them as standalone inputs. Include CSV, observation companion and evidence companion presence/hashes in cursor/fetch consistency. Classify all companion text and evidence inputs, retain the exact bytes and parsed evidence in labelled proposal state, and bind them plus the resolved run identities to plan_sha256. Replan with the same retained inputs and current facts at acceptance; missing, ambiguous or changed evidence refuses before staging or, if changed while waiting, as stale before writing. The resulting BatchEvidence, observation-member evidence and new-version per-field sidecars are written in the same content transaction. Omitted-field evidence and old versions remain unchanged.

## Stored calculation runs

A durable run lives at `runs/<full-sha256>.toml`. `StoredRun { schema: 1, canonical: String, inputs: Vec<Pointer>, outputs: Vec<RunOutput> }` is parsed by the core; `RunOutput` is the calculation core's `{ name: String, value: Value<Dec>, unit: String, currency: Option<Currency> }`. `Run::canonical()` contains function id and version, calculation epoch, stage/completeness, ordered inputs and outputs with units/currencies, and the named rounding policy with scale and mode. Hash exactly its UTF-8 bytes. The existing `<function>@<version>#<16 hex>` cell is a display/reference prefix; `RunCatalog::resolve` requires one matching full hash, verifies the canonical bytes, function/version and output name, value, dimension and rounding. Missing, ambiguous, corrupt or mismatched runs are FB0380. Computed evidence is required even if the type has no display `calc_run` column. A load may reference an existing valid run but cannot manufacture one from a hash. Runs and evidence are planned and journaled with records before any record that references them becomes readable.

The canonical format is UTF-8. `S(x)` below means the decimal UTF-8 byte length, a colon, then exactly those bytes; the length has no leading zero unless it is zero. `N` is an unsigned decimal integer with the same rule. Fixed tags and separators are ASCII; each record ends with LF. Text may contain a newline because its length, rather than the next newline, delimits it. No locale, time or platform path spelling enters these bytes.

```text
run 1
fn S(func)
epoch N(calculation_epoch)
rounding S(policy) N(scale) S(mode)
stage S(stage)
complete true|false
inputs N(input_count)
in S(name) S(value) S(origin) S(pointer) S(unit) S(currency)
outputs N(output_count)
out S(name) S(value) S(unit) S(currency)
```

Repeat `in` and `out` records exactly the declared count in the function's declared order. `value` is `known:` plus the core decimal's canonical display, or `unknown:` plus its reason. Currency is its validated code, or the empty string when absent; unit is the core's unit word. Policy and mode use their stable ids. The stored structured inputs and outputs must equal the decoded canonical fields; an inconsistent duplicate is refused. Hash only the canonical bytes, not TOML serialization.

## Observations and revisions

A load stream declares `mode = "snapshot" | "event" | "upsert"` (default `upsert` for existing configurations). A snapshot also declares its scope and period; an event declares its event-key columns. Each distinct accepted input creates an immutable `observations/<system>/<stream>/<source-version>.toml`: schema, mode, as_of, scope, period, source id/hash, declared event-key columns and event identities, optional supersedes observation id, ordered members `(origin_key, version_id, row, field_evidence)`, row count and declared control totals. A control total declares a numeric column, unit/currency and expected value; sum with the calculation core and refuse a mismatch before staging. An identical source hash for the same observation identity is idempotent.

Snapshot membership is the complete population for that scope and period. An omitted key is absent from that observation, without withdrawing its historical versions. Event streams append immutable events, deduplicated by event key; reuse with changed values is a structural error. Upserts change only keys present and do not infer withdrawal. Unchanged values reuse their version but still get observation membership and source evidence. Each member’s strict `field_evidence` map is keyed by exact supplied numeric column and reuses the version-sidecar field-evidence shape. Resolve Incoming targets to this observation’s source version at materialization. Observation-context lookup uses that member’s supplied evidence, falling back to the immutable version sidecar only for omitted fields. Unknown/duplicate/non-numeric columns, missing evidence for a supplied figure and unresolved/mismatched evidence refuse before acceptance. Unchanged never edits the old version sidecar. The manifest retains companion labels and input references; effective labels union them with current evidence and run-input labels, including after relabelling. Late observations are retained; as-of selection sorts by observation date then stable source id, not arrival or version number, and never promotes a late older observation to current. Two different snapshots at one as-of require explicit supersession; no silent tie-break. The optional `<input>.observation.toml` sidecar has `schema = 1` and `supersedes = "<observation-id>"`; it supplies no values or labels. LoadFile and BatchBody carry that reference, and plan_sha256 covers it, the event-key declaration and identities. A correction must name the current unsuperseded observation with the same system/stream, scope, period and as-of; missing references for a tie, mismatches, forks and cycles refuse. Select through the validated chain, preserving every predecessor manifest and source row. Imports validate the entire batch, including totals, before proposing it. The waiting plan hash covers all source/observation facts, type bytes, row outcomes and evidence/run inputs; changing scope, period or evidence without changing values still requires a new proposal.

A type revision is immutable. `[[type]]` has `name` and `revision` (default 1); `(name, revision)` is unique. Revision 1 reads `records/<type>.csv`; later revisions read `records/<type>@<revision>.csv`, each against its exact header. Adding an optional column creates the next revision, retaining all old definitions and files. Existing columns keep names, kinds and meanings; removing, renaming or changing meaning creates a new type. `RecordRegister::read(type)` joins revisions in logical id/version order; `plan_append` targets the newest revision and returns `PlannedBatch { files: Vec<PlannedFile> }`, including evidence, runs and observations. Duplicate logical versions across revisions are invalid. Serialized old rows and definitions stay byte-for-byte unchanged. Old values do not acquire a new meaning because a later schema exists.

## Approval and issue

The common control records are `approval`, `period_lock`, `reopen` and `issued_snapshot`. They use ordinary `types.toml` records and the existing ref/list kinds. An approval has one or more distinct target `VersionId`s, represented by its required `targets` list; each list item is a store-qualified source or record version and the record-specific validator resolves every target. A generic list remains text unless this common record's validator gives it that role. `source_ref` remains evidence lineage and is not a substitute for approval targets.

An approval also has a required `method` ref, `evidence_hashes` list, `decision` and `rationale`. The method ref points to an exact ordinary method-definition record version. A module owns that record's definition and prerequisites through the existing type recipe; this adds no interpreter or generic method-execution port. The synthetic foundation fixtures define a method type with required `method_id` text and definition provenance through `source_ref`. A mathematical method may name an existing `CALC_FNS` id/version, which must resolve; an approval or reopening procedure need not pretend to be a calculation. The method record `VersionId`, not a mutable title or latest method, is the approval's method version. Each `evidence_hashes` item is a full lowercase SHA-256 and must match an actual retained evidence, run or original in the declared approval's dependency closure. The decision and rationale are retained record values; a typed decision or actor string never establishes independent authorization. A required authorization still follows the existing policy/source and module rules.

A `period_lock` names its `RecordScope` and exact `Period` plus a required approval ref to the authorizing approval version. It does not point to an arbitrary priced row as its lock subject. A `reopen` names the exact `period_lock` version, the same scope and period, a new authorizing approval version and a nonempty reason. An effective approved reopen releases only that lock and preserves its history. Ordinary holds and version approvals retain their covered-version references. Missing, inconsistent or unresolved common-control fields refuse with FB0241; ordinary malformed type declarations remain FB0240. No new verification or adoption status is introduced.

### Issued snapshot carrier and canonical identity

An `issued_snapshot` is an ordinary immutable record with a required `manifest_sha256` text column containing exactly 64 lowercase hex digits. Its manifest is `snapshots/<manifest_sha256>.toml` in the same company store. The directory is a known labelled content kind, not derived State, an original, a public Corpus file or a text-free control. The row's scope and period must equal the manifest's. A corrective issue creates a new record version and a new manifest; its ordinary `supersedes` identifies the prior issued record and the manifest's optional `supersedes` is that prior record's manifest hash. They must agree. The manifest never includes the new `issued_snapshot` record or its own bytes in its dependency closure, avoiding a self-hash. Existing predecessors remain retained; dangling predecessors and cycles refuse.

`IssuedManifest` is strict schema 1. Its required fields are `issued_schema = 1`, `scope`, `targets`, `versions`, `runs`, `observations`, `files`, `methods` and `approvals`. `period` and `supersedes` are optional. `targets` is the nonempty set of exact source/record versions being issued. `versions` is the complete source/record version closure of those targets, selected observations, period, methods and approvals through their persisted input references; it is never a latest/as-of search. A method or approval list entry must also occur in `versions` and resolve to its corresponding record type. These lists are sets: parsing accepts their presentation order but rejects duplicate identities; canonicalization and the writer sort them as specified below. Unknown or duplicate fields, wrong namespaces and missing closure members refuse.

Field shapes (all examples are synthetic; hash literals here illustrate the wire shape and are not a valid delivered issue):

```toml
issued_schema = 1
scope = { kind = "work", entity = "overlay:CM-000001" }
period = { version = "overlay:CM-000040-v1", calendar = "fiscal", id = "P09", start = "2025-09-01", end = "2025-09-30" }
targets = ["overlay:CM-000050-v1"]
methods = ["overlay:CM-000030-v2"]
approvals = ["overlay:CM-000060-v1"]
supersedes = "1111111111111111111111111111111111111111111111111111111111111111"

[[versions]]
ref = "overlay:CM-000050-v1"
kind = "record"
sha256 = "2222222222222222222222222222222222222222222222222222222222222222"

[[runs]]
store = "overlay"
sha256 = "3333333333333333333333333333333333333333333333333333333333333333"

[[observations]]
store = "overlay"
system = "finance-exports"
stream = "commitments"
source_version = "SO-000020-v1"
sha256 = "4444444444444444444444444444444444444444444444444444444444444444"

[[files]]
store = "overlay"
path = "evidence/CM-000050-v1.toml"
sha256 = "5555555555555555555555555555555555555555555555555555555555555555"
```

The complete example in a future executable fixture must contain every referenced version and real matching producer-built hash; the shortened example above deliberately does not claim closure validation.

A `versions` entry has `ref`, `kind` (`source` or `record`) and `sha256`. A source binding is the registered byte-exact original hash; its original must be available in the retained closure. A record binding hashes only that exact version's canonical row and immutable type revision, never the whole appendable CSV or `types.toml`. Use the existing canonical plan encoder with the ordered named values `kind="record-version-1"`, `type`, `revision`, parsed immutable type definition and the ordered `(header, canonical stored cell)` pairs for that row. This includes the original handling cell; current effective labels still come from the ledger and input graph. Retaining a new row or type later cannot change that binding. Add a literal canonical binding vector; do not derive its expected bytes with the function under test.

A `runs` entry is `store` plus the full canonical `StoredRun` hash. An `observations` entry identifies its exact store/system/stream/source-version manifest and pins that immutable file's full hash. `files` contains the exact-path/full-hash auxiliary evidence and extraction files needed by those inputs: per-field evidence and the referenced source versions' extraction metadata and text. Paths are canonical store-relative paths, must be known evidence/extraction paths belonging to this closure, and reject traversal, links, undeclared files and duplicates. Original bytes, runs and observation manifests are already bound by their typed entries and are not duplicated here. Type definitions and rows use the immutable version projection above, avoiding hashes of appendable containers. No site, index, journal, settings or arbitrary path is admitted by this list.

The resolver loads all inputs in one coherent read generation, checks every typed reference, hash and locator and recomputes the complete declared closure. For inputs newly created by this same transaction it may read only the exact validated planned rows and `BatchEvidence` overlaid on that generation, not arbitrary uncommitted files. Acceptance recreates and validates that planned view. Missing, ambiguous, cyclic, malformed or mismatched inputs refuse before planning an issue. It uses exact record/type/method/approval versions, stored runs and selected observation manifests, never newer records or an inferred observation. Ordinary existing source/run/evidence failures retain their existing diagnostic codes; malformed issue structure or a missing or mismatched issue binding is FB0241. Required method/approval prerequisites still apply; an empty list is not proof they were met. An issue record or copied manifest cannot confer authority or lower a label.

Register/Admit plans the new manifest, issue record and any newly supplied evidence, runs or observations as one existing `WriteSet`. Under the same coherent lease it validates the entire closure, resolves all effective labels, performs store admission and lock checks, and binds every planned path and hash before begin. The existing committed generation marker remains last; readers never observe a manifest without its issue record or the reverse. Matching existing content-addressed bytes may be reused; a hash-path collision refuses rather than overwrites. The write's affected list includes the changed issue/control record `VersionId`s in existing `audit::RecordRef` form. Journal events contain no manifest text, rationale or paths: the existing hashed-file list and affected ids suffice.

Model, `check`, `effective_item` and `rests_on` recognize the manifest, its typed references and auxiliary bindings. Effective issue labels union the issue's own labels with the current effective labels of every dependency, including period/method/approval inputs; an unresolved dependency stays unresolved and never grants public provenance. Backup recognizes the manifest as labelled content and retains its company-held closure through existing root copying. It never treats it as a stray file or text-free control. If the closure also uses Corpus inputs, the independent recovery set must retain the exact public edition/originals pair that supplies them, through the existing verified release archive; a company-store-only backup does not establish that those public inputs were copied. Nothing company-owned returns to the public archive or home side. Restore verifies the manifest identity, exact row/type projections, file hashes, stored runs and complete closure before reopening the issue. Later record appends, type revisions, runs or observations cannot substitute for a selected version. Delivery still requires the complete closure in a verified independent recovery copy under the existing recovery gate; same-disk backup alone remains insufficient.

Canonical identity uses UTF-8 `S(x)` and `N` from [Stored calculation runs](#stored-calculation-runs). Fixed tags and separators are ASCII and every record ends in LF. Lists sort by their typed identity tuples in UTF-8 byte order: qualified ref; `(store, hash)`; `(store, system, stream, source_version)`; `(store, path)`. Identity duplicates refuse even when hashes agree. The optional scope fields encode the empty string when absent, and global encodes all three as empty. Empty predecessor encodes no predecessor. Period bytes are exactly [Scope and period](#scope-and-period)'s value encoding, inside `S`. TOML key order, quoting, comments and presentation array order do not define identity; parse, validate and emit the canonical sequence below.

```text
issued 1
scope S(kind) S(entity) S(project) S(object)
period S(period_canonical_bytes)
supersedes S(previous_manifest_hash_or_empty)
targets N(count)
target S(qualified_version)
versions N(count)
version S(qualified_version) S(kind) S(sha256)
runs N(count)
run S(store) S(full_run_hash)
observations N(count)
observation S(store) S(system) S(stream) S(source_version) S(sha256)
files N(count)
file S(store) S(relative_path) S(sha256)
methods N(count)
method S(qualified_version)
approvals N(count)
approval S(qualified_version)
```

Repeat each singular record exactly its preceding count; the canonical empty list consists only of its plural count record. Hash exactly these canonical bytes with SHA-256. The full lowercase result must equal the filename and `issued_snapshot.manifest_sha256`. There is no hash field inside the hashed manifest and no self-reference. The serializer emits the validated arrays in canonical order, but the hash is never a TOML serialization hash. `S` and `N` helpers are shared with existing canonical encodings, not a second generic serialization framework.

## Foundation fixtures

`tests/fixtures/finance-workflows/` contains only synthetic data. The records suite reads three folders through the real Register/Admit path, with no new finance-module UI.

| Case | Positive result | Negative plant |
|---|---|---|
| Pricing | A workbook quote supplies integer quantity and unit price from different cells; a judged allowance has a rationale; a versioned build-up run supplies the computed extension. The issued bid resolves every field and run | Missing run, wrong output/unit, ambiguous run prefix, missing workbook cell or changing the quote restricts/fails the dependent bid as specified |
| Estimate | Three monthly snapshots retain an unchanged commitment, an omitted commitment and a changed value; an older snapshot arriving last stays historical. Exact period totals and membership reproduce the estimate | Treat Unchanged as no observation, ignore a missing member, promote a late vintage or accept a failed control total: the expected period population/totals fail |
| Close | Two entities and two projects share a type. Closing one scope blocks its backdated write but allows the other. A reasoned approved reopen permits a new version; the earlier issued snapshot restores identically | Use type/date alone, ignore entity ownership, bypass a lock, omit an approval, or substitute a newer run: the close/restore contract fails |

For progress and revenue, fixtures declare two distinct named methods and different prerequisites. They assert Unknown for no method, missing approval and a zero denominator, and compare each valid result to that method's calculation case. No generic completion or revenue percentage is inferred from cost, EAC and price alone.

The foundation workflows run in `crates/fieldbook/tests/finance_workflows.rs` with `test-hooks`, through real admission, journal, backup and restore handlers. Pure model and calculation tests remain in core.

Every content WriteSet supplies sorted distinct affected `audit::RecordRef` values for the records it changes. Both write events retain them, so audit ID filtering finds direct and queued writes. Store-level configuration and derived artifacts without a record subject may use an empty list; no filenames or content enter these event fields.
