# Mockup to build map

Each mockup route or component, the AD-12 output it becomes, where `HANDOFF.md` specifies it, and the card that builds it. Section numbers (§) refer to `HANDOFF.md`. Cards are named by slug only; their ids and status live in the owner's tracker, never here. "later" marks rows with no card yet; "none" means it is never built.

The Day 30 cards are `handoff-context`, `handoff-cite-check`, `handoff-new-note`, `build-watch`, `assistant-page`, `views-and-day-n`, `decision-package` and `app-map`. Some slugs name cards whose file is not written yet; the slug is final.

## Routes and components

| Mockup route or component | AD-12 output | HANDOFF | Card |
|---|---|---|---|
| `#today`: Hero, Composer, Starter, ThreeUp (ReuseItem, ChangeItem), NextUpRow, AreaIndex / AreaTile | `index.html` | §2 Routes, §5 | `build-home-and-areas`. Continue working fills as work objects land: `decision-package`, then R2 or later. The composer and starters open a thread on the Assistant page: `assistant-page` |
| `#browse`: AreaIndex, CoverageBar per area | `index.html#areas` (canonical Today frame) | §2 Routes, §5 Charts | `build-home-and-areas` |
| `#areas`: the 13 areas and 3 views with their module ladders | `index.html#areas` (canonical Today frame) | §2 Routes | `build-home-and-areas` |
| `#area-<code>`, `#area-<code>/<tab>`: WorkspaceHeader / Tabs, ThreeUp, CoverageBar | `a/<area>.html`, one section per tab, an empty-state line where there is no data | §2 Area workspaces, §5 | `build-home-and-areas` for the 13 areas; `views-and-day-n` for CPY, CPT and PGM |
| ProcessDetails (area Processes and Procedures) | A section of `a/<area>.html` | §2 Work and workspace object types, §5 | later (Release 3) |
| Timeline (area History) | A section of `a/<area>.html` | §5 | later (history is not an object) |
| Charts: ChartPanel, LineChart, BarList, ChartFoot, TableFold, Unavailable (EST and RTE overviews) | Inline SVG in `a/<area>.html`, written by the build; no chart script, no `style=""` | §5 Charts, §12 | `build-charts`; `calc-cases` supplies CALC-02. The escalation chart (S-0371) shows Unavailable until the Corpus has a format for series data (an ADR 0002 amendment, owner gate 1). Bid and rate history stay Unavailable until R2 |
| `#competitor-<id>`: CapLadder, CompareMatrix | MKT pages in subarea `peers`: `p/<page-id>.html`, listed in `a/cpt.html` | §2 Routes, §5 | `build-page-types` for the pages; `views-and-day-n` for the CPT view |
| ProgramsTable (PGM overview) | MKT pages in subarea `programs`, listed in `a/pgm.html` | §5 | `views-and-day-n` |
| `#page-<id>`: PageHeader, MetaRow, PageActions, WordingToggle, RichText, TermLink, PageLink, CitationMarker, RefChip, ClaimMark, PreciseBlock, ConflictBox, EvidenceRail / EvidenceCard, SourcesBlock, ChangeNote, Popover | `p/<page-id>.html`: frozen part order, Sources built from the citations | §2 Page template, §5 Inline token kinds, §7 | `build-page`; `check-citations` lexes and resolves the citations |
| CalcTable (worked example) | A part of `p/<page-id>.html` | §5 | `calc-cases` computes the values; `build-charts` renders them |
| Page context pack (`<details><pre>`) | A part of `p/<page-id>.html` (a Could) | §9 Context pack | `page-context-packs` |
| `#brief-<id>`: BriefSheet | `p/<page-id>.html`, page type `brief` | §5 | `build-page-types`; the print contract is `day-one-binder-and-registers-xlsx` |
| `#plan`: the 90-day plan, PlanTimeline | The 30/60/90 checklist, `p/<page-id>.html`, page type `checklist`; `templates/90-day-plan.xlsx` | §2 Routes, §5 | `build-page-types` for the page and timeline; `views-and-day-n` for the today line; `e1-workbook-templates` for the workbook |
| `#learn`: StageTrack | Curriculum stage pages, `p/<page-id>.html` | §2 Routes | `build-page-types` (no reading progress) |
| Source drawer: SourceDrawer, LocatorView, Excerpt, AnswerMeta | `s/<version-id>.html`, an anchor per locator | §5 | `build-source-pages`; `extract-and-locators` for the locators; `quote-match-and-anchors` for the quotes |
| `#claims`, `#questions`, `#glossary`, `#sources`: RegisterTable, StatusChip | `claims.html`, `questions.html`, `glossary.html`, `sources.html` | §2 Routes, §5, §7 | `build-registers`; `registers-xlsx` for the workbook and CSV copies |
| `#conflicts`, `#conflicts-<id>` | `conflicts.html`, `conflicts.html#XC-14` | §2 Routes, §5 ConflictBox | `build-registers` |
| `#templates`: TemplatesList | Briefs on `index.html`; `day-one.html`; `data/registers.xlsx`; `templates/<name>.xlsx` | §5 | `day-one-binder-and-registers-xlsx` and `registers-xlsx`; `e1-workbook-templates` for the template files |
| Search: CommandPalette, SearchTrigger | `search.html#q=`; the palette on every page; `assets/search-index.js` (edition: page and section units, cap 15 MB, [ADR 0006](../decisions/0006-edition-search-index.md)) | §10, §5 | `search-core-and-cli`, `search-ranking-and-index`, `search-site-index`, `search-page-view`, `search-index-budget`, `search-js`, `search-palette` |
| Id list (no mockup route) | `ids.html` | §2 Routes | `build-registers` |
| `#frame/public`, `#frame/company`, `#frame/company-marked` | Shared frame preview only | §3 Frame; BuildChip, Sidebar and effective-label banner | `build-frame`; company context: `r2-overlay` |
| About (no mockup route) | `about.html` | §2 Routes | `build-about` (compiled profile/posture, no live adapter probe); edition metadata from `edition-manifest` |
| Changes (no mockup route) | `changes.html` | §2 Routes | `changes-page` (`edition-002`) |
| `#desk/import`: ImportReport | state/pages/import.html | Desk pages: ImportReport, ActivityLog and BackupStatus | `status-command`; AddReport from `add-folder-mode` |
| `#desk/activity`: ActivityLog | state/pages/activity.html | Desk pages: ImportReport, ActivityLog and BackupStatus | `status-command`; show and verify reports from `audit-journal` |
| `#desk/backup`: BackupStatus | state/pages/backup.html | Desk pages: ImportReport, ActivityLog and BackupStatus | `status-command`; BackupList from `backup-snapshot` |
| `#queue`: RegisterTable, StatusChip, CommandChip | `pages/queue.html` | Queue review pages | `review-queue` |
| `#queue/P-nnnn`: MetaRow, Panel, Fold, CommandChip | `pages/queue/P-nnnn.html` | Queue review pages | `review-queue` |
| `#assistant`, `#assistant/<id>`: ThreadList / ThreadPick, Message, ThreadNote, AdapterStatus, Composer, CheckedAnswer, VerdictChips, Fold, ReadFold, AnswerActions, ContextPanel, ContextItem, ContextMeter, ContextBar, RelatedPanel | `assistant.html` | §9 Assistant page, Context panel | `assistant-page` (with `assist.js`); `handoff-context` for the items, their reasons and the saved context packs |
| Model adapter: the AdapterStatus states and the capabilities that switch controls | `assist.js`, through the adapter the architecture contract names | §9 Adapters | `model-gate`, `model-adapter-openai` and `chat-bridge`; static Handoff is `assistant-page`; Assistant/drawer cancel() reaches chat-bridge’s existing authenticated cancellation route |
| Ask drawer: AskDrawer, AdapterStatus, Composer, ContextBar and the context sheet, Fold, CheckedAnswer, VerdictChips, CiteCheckChip, AnswerActions, SelectionToolbar | Every page, through `assist.js` | §9 Entry points, Context pack, Cite-check; §6 | `handoff-context`, `handoff-cite-check` (with the outside-context status); `handoff-new-note` for Copy as draft note |
| `#map`: AppMap | `map.html` | §2 Routes, §5 | `app-map` |
| `#decisions`, `#decision-<id>`: DpSummary, KeyFigures (StatStrip), ReadinessGates, Sensitivity, OptionsCompare, Bridge, Trace, OriginTag | `templates/decision-package.xlsx` first; pages with the R2 overlay | §2 Routes, §5 | `decision-package` for the workbook; later (R2) for the pages |
| `#assumptions` | `templates/assumption-register.xlsx`; R2 overlay | §2 Routes | `e1-workbook-templates` for the workbook; later (R2) for the pages |
| `#discovery` | `templates/discovery-log.xlsx`; R2 overlay | §2 Routes | `e1-workbook-templates` for the workbook; later (R2) for the pages |
| `#bids`, `#bids/<id>`: BidPipeline, LifecycleChain | Company store | §2 Routes, §5 | later (R2 or later) |
| `#estimates`, `#estimates/<id>`: EstimatePointer, OriginTag | Company store, authorized originals and evidence | §2 Routes, §5 | later (R2 or later) |
| `#projects`, `#projects/<id>`: ProjectPosition, EacHistory | Company store | §2 Routes, §5 | later (R2 or later) |
| `#audit` | R2 overlay (audit requests) | §2 Routes | later (R2) |
| `#modules`: Ladder, ModuleCard | `modules.csv` | §2 Routes, §5 | later (Release 3) |
| `#feeds`: Dots | `sources.csv` origin rows of the public `Pull` stream `later-feeds` | §2 Routes, §5 | later |
| `#cycles`: DayStrip | Not built (cycle is not an object) | §2 Routes | none |
| `#states` | Not built | §6, §11 | none: the reference for every card's states |
| Base components: Button, Chip, Field, Panel, Eyebrow, Segmented, Box, Kbd, Dialog | Component rules in `assets/app.css`, used by every template | §5 (the first ten rows), §6 | `build-frame`. Later cards reuse them and add a variant only by a §5 change |
| Skeleton | `search.js`, while the index loads and only after 300ms | §5, §11 Index loading | `search-js` |
| AppShell, TopBar, SectionTabs, BuildChip, CrumbsRow, Sidebar, ContextNav, SectionSwitch, NavItem, Banner, Toast, Switch, theme toggle | Every page: the askama base template, `assets/app.css`, `app.js`. The page's section, store context and effective labels come from build metadata, so navigation and public/company handling text render with scripts blocked | §2 Global frame, §3, §5, §6, §8 | `build-frame`; company frame and effective labels: `r2-overlay` |
| ShortcutsDialog | `app.js` on every page | §5, §6 | `search-palette` (`/`, Ctrl K); `assistant-page` adds the Ask shortcut |
| DayChip and "Set your start date" | Header on every page and `index.html`, `app.js` | §5, §6, §11 | `views-and-day-n` |
| CommandChip | Every write, as a command to copy | §5, §6 | `build-about` for the chip and its quoting; the verbs come from `add-new-and-edition`, `new-verbs`, `verify-status-fmt`, `status-command`, `fmt-command` and `search-cli-and-regression`; the Day 30 verbs (`context`, `cite-check`, `new note`) from `handoff-context`, `handoff-cite-check` and `handoff-new-note` |
| CitationChip | Registers, drawers | §5 | `build-page`, `check-citations`, `build-registers` |
| Design tokens | `:root`, dark, print, forced-colors and reduced-motion blocks in `assets/app.css` | §4 | `build-tokens` |
| Frozen strings | `fieldbook-core::strings` and `strings.js` | §0 Frozen strings | `core-types-and-strings` |
| Fixed template text | The templates; lines a page script shows are shared once (§0) | §0 Fixed template text | The card that builds each component; `core-types-and-strings` if the shared lines join the shared browser string table |
| Status display (who may raise a status) | Display only; the UI never writes a status | §7 | `guard-statuses` |
| CSP, no `style=""`, no external URL | Every page | §12 | `build-v0` and `workspace-skeleton`; `build-site-lints` enforces them |
| Print: footnotes, one-page briefs | Print rules in `assets/app.css` | §12 | `day-one-binder-and-registers-xlsx` |
| Edge from `file://`, cmd and PowerShell command chips | Checks, not pages | §13 Not yet verified | `edge-harness` (headless Edge); `transfer-and-windows-drill` (E1) |

## Design tokens

`HANDOFF.md` §4 owns every token value, in its Colour, Type, Space, Shape and line, Size, Motion, Layering, Elevation, Print, Forced colours and Reduced motion tables, and "Using the tokens" says which properties take only tokens. Each row there names one token and gives one value per column, so `build-tokens` compares `assets/app.css` with §4 directly. This map does not copy the values, so there is one place to change them. The mockup's `:root` matches §4, except that its font stacks add fallbacks for preview. The mockup has no raw value left in the token-only properties.

Assistant copy actions on the page and Ask drawer share the existing dialog for full selectable manual text when clipboard permission or browser access refuses the write. `assistant-page` owns independent build-to-render Handoff and clipboard metadata and tests the emitted data. Search record rows retain their labels and facts without an href until `later-record-views` supplies their pages.
