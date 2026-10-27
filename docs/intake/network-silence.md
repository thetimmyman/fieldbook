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

The edition's own Content Security Policy forbids connections and remote scripts, styles and images (`connect-src 'none'`). The log shows what happened. At the standard setting and at the fallback that hashes inline scripts, the policy is what prevents a request, and the two agree or the build fails. If Edge refuses both, the last fallback is a no-scripts edition: pages work without JavaScript under the strictest policy Edge accepts, and search says that it needs scripts. Then no script exists to make a request, and the edition's file list and script budget show that. The chat page of the AI build, served by `fieldbook chat` from `127.0.0.1` and never written to a file, allows connections to its own origin and nothing else.

## 2. What the result proves

- No page, in this browser build and with these options, started a request to anything but a local file.
- The instrument works: it saw and blocked the request on the negative page.
- Every page was loaded: the counts equal the counts of HTML files and renderable originals in the edition.

## 3. What it does not prove

- It says nothing about how Edge is configured on the laptop, about other browsers, or about what Excel does when it opens a workbook. The workbooks contain no external links and no macros, and the build checks that (see `TRANSFER-STATEMENT.txt`).
- It covers the tested edition and the tested interactions only: the pages as loaded, the self-test and the search tests, not every action a reader could take.
- It exercises the pages as loaded, plus the self-test and the search tests. A path in a script that no test reaches is not run. The scripts are plain readable files, they are few, and the page policy blocks a remote connection even if one were attempted.
- It covers the files in this edition. The signed programs of a later release get their own checks on Windows. For the silent build: a capture while the tests run, and a check that the program imports no networking library. For the AI build: a capture while the AI drill runs, which must show connections only to the hosts in the drill's settings, and an import list that holds only the socket and certificate-store imports named in section 5. Those results will join this report then.

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

The permitted HTTP exception for 127.0.0.1, [::1] and localhost connects directly and bypasses every proxy setting, including when NO_PROXY is absent. HTTPS proxy behavior remains separately tested with certificate verification.

Tool versions before 0.2.0, and the silent build in every version, make no connection of their own: Fieldbook opens no socket and starts no process, and the one thing that can cross a network, a read from a company data source on a share, is done by the operating system (below). The AI build (0.2.0 and later) makes one kind of connection of its own: an HTTPS request to an endpoint named in the owner's settings and in the company's policy file, only when the owner runs a command that needs it, and only after the audit line is written. It has no redirects, cannot turn off certificate checks, and starts no process. A build with the further feature `bridge` holds one listener, on 127.0.0.1, only while `fieldbook chat` runs, and the capture and import list in the report cover it. The AI build without `bridge` has no listener and its import list shows none of the listener functions. `fieldbook mcp` uses standard input and output and opens no socket in any build.
Both builds of the installed program have a second kind of read, which is not a connection of Fieldbook's: `sync` reads the files of a company data source, such as a folder that the company's own tools fill. It reads as the owner's account, makes no connection of its own, writes nothing to the source and starts no process. If the folder is on a network share, the connection is the operating system's file-sharing client, and a capture made while a sync runs shows a connection to the share's host that the operating system makes. The report lists the hosts of the sources the owner named in `systems.toml` beside the capture. The import check is unchanged, because `sync` adds no networking function. A reader that connects to a company system itself, with a credential, would be a third kind. It would be built only in the AI build, behind its own gate and its own destination row in the policy, and its hosts would join this report before IT is asked to approve it.

What the report will show for it: the capture lists the hosts connected to, and each is named in both the settings and the policy file; the import list holds the socket and certificate-store functions the TLS stack needs and, in a build with `bridge`, the chat listener, and nothing else (the list is in the report, so IT can compare). What it does not show: what the endpoint does with the text, whether the endpoint is the one IT approved, or what an inspecting gateway does in between.

## 6. Repeating it

On a disposable machine with Edge, the options in the summary can be replayed against any page in the extracted edition, with the network log written to a file. In the log, a request to anything but a local file is a request to a remote address. The harness is a build tool and is not part of the edition. The repository holds its source for review.
