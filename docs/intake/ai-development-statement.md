# Fieldbook AI development statement

Edition {edition}. Tool version {tool_version}.

This statement answers two questions. How were AI assistants used to build Fieldbook? And what part does AI play when Fieldbook is used? It names assistants by kind, not by product.

## 1. What AI the product contains

- No version of Fieldbook contains a model, an embedding service or a model download, and none runs a model itself.
- Tool versions 0.1.x are silent builds: they have no network client and no AI feature that calls out. They have no telemetry, crash upload or update check. Handoff (section 3) works with no network.
- From tool version 0.2.0 the home build can include an AI build: one HTTPS client that sends a prompt to an endpoint named in the owner's settings and in the company's policy file, only when the owner runs a command for it. A build with the further feature `bridge` also holds, while the owner runs `fieldbook chat`, one listener, on 127.0.0.1 only (section 4, IT-32). The build without that client is the silent build. The AI build is not part of the first edition or of `v0.1.0`. Which build is installed on the laptop is IT's decision (IT-27). Section 4 describes the AI build and its limits.
- The Corpus, the checks, the search, the calculations and the editions never use a model, in any build.
- With a loaded policy, Fieldbook enforces each destination’s allowed labels and the owner’s narrower settings. The tool server remains public-only in version 1. A missing or invalid previously loaded policy closes outward access except the audit sink (IT-33).

> **Floor.** Until a policy file has been loaded on this machine, the floor applies. The stores `overlay`, `working` and `state` hold `public` and `unlabelled`, `state` also holds `owner-supplied`, and `corpus` holds `public`. Handoff, exports and backups receive `public` and `owner-supplied`. A model endpoint named in the owner's settings, the tool server, the chat Bridge, print and the clipboard receive `public`. On a machine that holds a company store, the stores `overlay`, `working` and `state` also hold `marked`, no model endpoint receives anything, exports are refused, and a backup to a folder the owner recorded as company storage may also hold `unlabelled`. No endpoint is approved, and nothing marked leaves.

## 2. How assistants were used to build it

| Question | Answer |
|---|---|
| Which assistants | Two kinds of general-purpose large-language-model coding assistant, used through their vendors' hosted services: an interactive assistant that helps draft documents and code, and an autonomous coding agent (the repository calls this role Codex) that executes written task cards |
| On what hardware | The owner's personal development equipment. No employer hardware, network or account was involved |
| What data went in | Public sources and synthetic examples only. No company, customer, employee, personal or controlled data. No credentials are pasted into any assistant. A secret scanner runs before every commit and on every pull request as a second check |
| What the agent may not do | Add a crate, tool or action whose licence, source or pin fails the repository's mechanical checks, or add network or model code anywhere except the one file and the one gate the AI build describes; admit anything to a company store without an accept by the owner or lead at a terminal; raise a verification, applicability or adoption status, or record a check; rewrite a stored original or an existing seed test vector; push tags; skip checks. It may add public sources and draft claims and pages, which stay unchecked until a person verifies them. CI enforces the status, original and seed rules on every commit that carries the agent's author name or an assistant co-author trailer, and the dependency rules by the licence, source, pin and count checks |
| Prompt injection | Text in source documents and in pasted answers is treated as data, never as instructions. The agent's instructions say so, and the tool shows pasted answers as plain text. Model replies are treated the same way |
| Human review | The owner decides every format change after the freeze, tag, release permission and signing step. The owner verifies every consequential claim; the lead verifies the others. Cards that touch none of those may be merged by the agent after CI passes, and the owner reads the week's merged work in a weekly review. Every consequential claim stays marked unchecked until the owner verifies it against the original source |
| Licences | Every dependency's licence is checked against an allowlist on each pull request. A scan for reproduced third-party code snippets is not run |
| Where the code lives | A private repository. IT may review the source |

The agent's author name on commits is "Codex (agent)", and an interactive assistant's commits carry a `Co-authored-by` trailer, so either's work can be told apart from a person's.

## 3. Handoff: AI by copy and paste

The Handoff route has no model and no network in it, in the silent build and in the AI build alike. In tool versions before 0.2.0 the owner does the three steps by hand; from 0.2.0 the command `ask` does them in one run. Each step is by the person at the keyboard:

1. Fieldbook builds a context pack: plain text taken from the Corpus or, in the company store, from the records that the policy's `handoff` row allows (with no policy file, public text only), led by a short set of rules for the assistant. The first line of the pack lists its labels, and the pack ends with a count of the records left out because the policy does not allow them. Pack size is capped (the default is 12,000 characters). Sources of low authority, superseded versions and contradicted claims are left out unless named.
2. The owner copies the pack and the question into an assistant that IT has approved for this laptop. Fieldbook never talks to the assistant.
3. The owner pastes the answer back. Fieldbook checks each citation against the Corpus, flags sentences with no citation, and says "Do not use this answer." when nothing verifies. It never sends the answer anywhere.

Controls on this path:
- A pack holds only the labels that the policy allows for Handoff, because the pack builder is given a view of the store limited to those labels. With no policy file that is public text. A record outside the allowed set is left out whole, never redacted, and the pack says how many were left out.
- A question that carries a marking banner is refused unless the label that the marking maps to is already one of the labels of the request, and the destination must still allow it. The Assistant page cannot read the policy and refuses every banner shape.
- Pasted answers are shown as text and cannot act on the page.
- A saved answer becomes a proposal or a note marked as AI-drafted and unreviewed. It can never support a claim.
- From 0.2.0 a round trip run through `ask` writes two lines to the audit journal, one for the pack and one for the paste. A round trip done on a page opened from a file is not recorded, and the page says so.
- The step is optional. The Corpus, search and workbooks work without it.

## 4. The AI build (tool version 0.2.0 and later)

The AI build is not part of the first edition. This section says what it does from tool version 0.2.0 and what limits it.

| Part | What it does | What limits it |
|---|---|---|
| The gate | Every model call passes one check, whoever started it: the labels of the request against the policy's row for the endpoint, a scan for marking banners, a call budget, then a line written to the audit journal before anything is sent | If the line cannot be written, nothing is sent. If the line after the reply cannot be written, the reply is discarded |
| The endpoint adapter | Sends a prompt (a context pack and a question) over HTTPS to the endpoint named in the owner's settings and, under a loaded policy, in that policy, only on an explicit command and after the audit line is written, and returns text | Under a loaded policy, only labels its endpoint row permits and the owner's settings allow; a missing row permits nothing. Before a policy has ever loaded, the Floor block above applies (IT-27, IT-33). No redirects. Certificate checks cannot be turned off. The key is read from an environment variable, never from a file (IT-28) |
| The chat page (feature `bridge`) | `fieldbook chat` serves one generated page on this computer's 127.0.0.1 only while it runs, behind a per-session secret; the page sends the same previewed request through the same gate | IT-32. A page opened from a file does not answer |
| The audit journal | A hash-chained record of each call: when, which endpoint, the label ids, the hash of the policy in force, hashes and counts, never the text | `audit verify` checks it. It does not stop a person with write access from replacing the file. A copy that IT collects with `audit export`, with its head hash, is what shows a replacement (IT-35) |
| Call records | When enabled, retain the prompt and reply in the data folder; the home default is 30 days, and laptop retention is off until IT answers IT-31 | The owner may disable retention. State must admit the request’s labels; the policy’s model row caps retention, and a zero cap disables it |
| `fieldbook mcp` | A program an approved AI client starts. It answers read-only questions (search, a unit, a context pack, a citation check) over standard input and output | Public text only in version 1: the policy's `mcp` row cannot list any other label (FB0413). The row can limit which clients may start it, if it lists any (IT-29, IT-33). It has no write tool. Every call is audited |
| Proposals | Every AI output is a proposal. A person accepts it, and the acceptance checks the journal line that produced it | An AI never certifies its own output |
| Later adapters | A typed-decision service, a local model and semantic search use the same gate | Each is a code change and is described here when it exists |

What it does not do: bundle a model, call anything that is not named in both the settings and the policy file, open a listener other than the chat page's (127.0.0.1, only while `fieldbook chat` runs, and only in a build with the feature `bridge`), write to a company data source, send telemetry or check for updates, or use a model in `check`, `build`, `search`, `verify` or an edition.

## 5. Questions IT may want to settle

- Which assistant is approved for this laptop, and what is its paste limit?
- Are clipboard history and cloud clipboard disabled?
- Is AI-assisted development of tools for corporate assets permitted, and under what data limits?
- Which endpoint, if any, is approved for the AI build, for which labels, and may a second signed program carry it (IT-27)? Who owns the policy file that names the labels and the endpoints (IT-33), what marks company data (IT-34), and where may the journal be exported (IT-35)? How may its key be held (IT-28)? May an approved client start `fieldbook mcp` (IT-29)? What route do connections take, and is TLS inspected (IT-30)? May call records stay on disk for 30 days (IT-31)? May a build with the chat page (feature `bridge`) open one listener on 127.0.0.1 while `fieldbook chat` runs (IT-32)?

They are asked at the first IT contact. The answers change this statement.
