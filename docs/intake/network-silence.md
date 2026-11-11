# Fieldbook network-silence report

Edition {edition}. Tool version {tool_version}.

This file explains one check: that the pages in this edition make no network request. The result for this edition is in `network-silence-summary.txt`, beside this file. A test harness runs the check on every build; no edition is released before it has run.

## 1. What the check does

1. The harness checks the completed edition tree before packaging, using a separate instrumented copy, the same files you receive.
2. For every HTML page in it, and for every text-renamed original (the HTML and SVG documents shipped as `.txt`), it starts Microsoft Edge in headless mode, loads the file from disk, and records Edge's network log.
3. It starts Edge with background networking, component updates and sync turned off. It never passes the option that lets a local file read other local files.
4. It reads the log and counts the requests those page loads started. Requests Edge makes for itself are ignored, because they are not the pages'.
5. Each page runs a short self-test that writes any blocked-content reports and resource loads into the page. The harness reads them back.
6. A negative page asks for `https://example.invalid/x.js`. The load must be blocked by the page's Content Security Policy and reported. If it were not, the harness could not see a real leak, and the check fails.

The edition's own Content Security Policy forbids connections and remote scripts, styles and images (`connect-src 'none'`). The log shows what happened. At the standard setting and at the fallback that hashes inline scripts, the policy is what prevents a request, and the two agree or the build fails. If Edge refuses both, the last fallback is a no-scripts edition: pages work without JavaScript under the strictest policy Edge accepts, and search says that it needs scripts. Then no script exists to make a request, and the edition's file list and script budget show that. The chat page of the AI build, built with `bridge` from tool version 0.3.0 and not part of the first edition, served by `fieldbook chat` from `127.0.0.1` and never written to a file, allows connections to its own origin and nothing else.

## 2. What the result proves

- No page, in this browser build and with these options, started a request to anything but a local file.
- The instrument works: it saw and blocked the request on the negative page.
- Every page was loaded: the counts equal the counts of HTML files and renderable originals in the edition.

## 3. What it does not prove

- It says nothing about how Edge is configured on the laptop, about other browsers, or about what Excel does when it opens a workbook. The workbooks contain no external links and no macros, and the build checks that (see `TRANSFER-STATEMENT.txt`).
- It covers the tested edition and the tested interactions only: the pages as loaded, the self-test and the search tests, not every action a reader could take.
- It exercises the pages as loaded, plus the self-test and the search tests. A path in a script that no test reaches is not run. The scripts are plain readable files, they are few, and the page policy blocks a remote connection even if one were attempted.
- The first edition and the silent build make no connection and open no socket; this report's result covers the edition's pages only, and no program is in the edition. The signed silent program of a later release (R2) gets its own checks on Windows: a capture while the tests run, and a check that the program imports no networking library. The AI build is a separate program from tool version 0.2.0 and is not covered by this edition's result: its own capture while the AI drill runs must show connections only to the hosts in the drill's settings, and its own import list must hold only the socket and certificate-store imports named in section 5; those results join this report when that build is released.

## 4. How to read `network-silence-summary.txt`

The file is plain text, one `name: value` per line. This example is **synthetic**; it is not a result.

```text
SYNTHETIC EXAMPLE, NOT A RESULT
edition: edition-000
browser: Microsoft Edge 0.0.0.0
options: --headless=new --dump-dom --disable-background-networking --disable-component-update --disable-sync ...
pages loaded: 100
text-renamed originals loaded: 10
requests started by those loads: 0
requests to file: URLs: 0
requests to any other scheme or host: 0
self-test: pass
negative page, requests blocked: 1
negative page, requests reported: 1
verdict: pass
```

| Line | Read it as |
|---|---|
| `edition`, `browser`, `options` | What was tested. `options` lists every command-line option used |
| `pages loaded` | Must equal the HTML count in `TRANSFER-STATEMENT.txt` |
| `text-renamed originals loaded` | Must equal the number of originals shipped as `.txt` |
| `requests started by those loads` | Requests started by the page loads. Ordinary local loads of style sheets and scripts count here, so a nonzero number is normal |
| `requests to file: URLs` | Local requests |
| `requests to any other scheme or host` | **Must be 0**. Anything else fails the build |
| `self-test` | `pass` means the in-page probe ran and reported |
| `negative page, requests blocked` and `reported` | **Each must be at least 1**. This proves the check can see a remote request and that the policy stops one |
| `verdict` | `pass` only if every rule above holds |

If a count is lower than expected, or the negative page shows 0, treat the result as void even if the verdict says `pass`.

## 5. The AI build

The AI build is a separate program from tool version 0.2.0; it is not part of the first edition or of v0.1.0. This section says what a capture and an import list of that program will show and what they cannot show; it is not a statement about the edition. In the AI build, a request to 127.0.0.1, [::1] or localhost connects directly and bypasses every proxy setting, including when NO_PROXY is absent, and a request to any other address needs HTTPS with certificate verification, tested separately with a proxy.

The first edition and the silent build make no connection of their own and open no socket. The one thing that can cross a network in the silent build, a read from a company data source on a share (from R2, `sync`), is done by the operating system's file-sharing client.

Channels the company approves for the AI build:

- The one HTTPS endpoint that both the owner's settings and the company's policy file name (IT-27, IT-33), used only on an explicit command and after the audit line is written, no redirects, certificate checks cannot be turned off.
- The loopback Bridge page: in a build with feature `bridge` (from tool version 0.3.0), one listener on 127.0.0.1 only while `fieldbook chat` runs (IT-32); the AI build without `bridge` has no listener and its import list shows none of the listener functions.
- A sync from a company source (from R2): a folder read in either build (a share is opened by the operating system; the program makes no connection of its own; it writes nothing to the source); a read over a network by the program itself would be built only in the AI build, behind its own gate and its own destination row in the policy, and its hosts would join this report before IT is asked to approve it; it is not in v0.2.0.

From tool version 0.2.0, `fieldbook mcp` uses standard input and output and opens no socket in any build.

What a capture and an import list will show: the capture lists the hosts connected to and each is named in both the settings and the policy file; the import list holds the socket and certificate-store functions the TLS stack needs and, in a build with `bridge`, the listener functions, and nothing else. What they cannot show: what the endpoint does with the text, whether the endpoint is the one IT approved, or what an inspecting gateway does in between. The list of hosts and of imports is not written in this document: it is in the report that joins this one at that release, so IT can compare.

## 6. Repeating it

On a disposable machine with Edge, the options in the summary can be replayed against any page in the extracted edition, with the network log written to a file. In the log, a request to anything but a local file is a request to a remote address. The harness is a build tool and is not part of the edition. The repository holds its source for review.
