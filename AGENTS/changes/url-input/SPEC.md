# Change: open URLs directly

Ships as **0.4.0**.

## Outcome

`mu <url>` fetches the URL and presents it, with curl-like options, instead of needing `curl -s <url> | mu`.

## Decisions (machinageist, 2026-11-09)

- **Transport:** the system `curl`, as the image loader already uses. No HTTP or TLS crates
- **Options:** common curl flags natively, plus `--curl <arg>` (repeatable) passed straight to curl
- **Images:** `mu <url>` implies `--images fetch`. An explicit `--images` overrides that, and files and pipes keep the no-network default
- **Raw rewrite:** GitHub, GitLab and Gist page URLs are rewritten to their raw source. `--no-rewrite` turns this off

## Behaviour

- **What counts as a URL:** a FILE argument starting with `http://` or `https://` (case-insensitive). Anything else is a path, so `./http:x` is still a file.
- **Flags:**

  | Flag | curl equivalent | Notes |
  |---|---|---|
  | `-H, --header <H>` | `-H` | Repeatable |
  | `-X, --request <M>` | `-X` | |
  | `-d, --data <D>` | `--data` | Repeatable |
  | `-A, --user-agent <UA>` | `-A` | Default `mu/<version>` |
  | `-u, --user <U:P>` | `-u` | |
  | `-k, --insecure` | `-k` | |
  | `-i, --include` | | Show the response status and headers as a table, then the body |
  | `-I, --head` | `-I` | Status and headers only |
  | `--timeout <SECS>` | `--max-time` | The connect timeout is always 10 s |
  | `--curl <ARG>` | anything | Repeatable, passed verbatim before the URL |
  | `--no-rewrite` | | Fetch exactly the URL typed |

  These flags are only valid with a URL, and using one with a file is a usage error. mu's own `-w` stays the width flag, unlike curl's `-w`.
- **curl invocation:** `curl -sS -L --proto =http,https --proto-redir =http,https --connect-timeout 10 --compressed -D -`, then the user's flags, then `--` and the URL. A write-out line on stderr carries the final URL. No shell is involved, and the URL is a single argument.
- **Format:** the first that applies wins:
  1. `-f`
  2. The response `Content-Type`
  3. The extension of the URL path (the path only, never the host)
  4. Content sniffing
  5. Markdown

  `text/plain` and `application/octet-stream` skip straight to the extension and sniffing.
- **Raw rewrite:**

  | Page URL | Raw URL |
  |---|---|
  | `github.com/O/R/blob/REF/PATH` | `raw.githubusercontent.com/O/R/REF/PATH` |
  | `gitlab.com/NS/…/-/blob/REF/PATH` | `gitlab.com/NS/…/-/raw/REF/PATH` |
  | `gist.github.com/U/ID` | `gist.githubusercontent.com/U/ID/raw` |

  Queries and fragments are dropped from rewritten URLs.
- **Images:** relative images resolve against the final URL, after redirects and rewrites.
- **HTTP errors:** for status 400 and above, mu still renders the body, prints `mu: <url>: HTTP <code>` to stderr and exits 1, like curl's `--fail-with-body`. Transport failures print curl's message and exit 1. A missing curl gives "mu: opening URLs needs curl" and exits 1.
- **Pager:** the title is the URL as typed. A resize re-renders without fetching again.

## Out of scope

- Following a repository's root page to its README
- Charsets other than UTF-8, whose invalid bytes become U+FFFD as for files
- Caching responses between runs
