---
title: URLs
group: Guide
order: 4
summary: Fetching pages with curl-like options
---

`mu <url>` fetches an `http://` or `https://` URL with the system `curl` and presents the response like a file. Added in 0.4.0.

## How fetching works {#fetching}

- mu itself contains no HTTP or TLS code. It runs `curl` directly, without a shell, with the URL as a single argument after `--`.
- Protocols are limited to http(s), including on redirect. Redirects are followed.
- Connecting always times out after 10 s. Responses are requested compressed.
- Response headers come back on stdout ahead of the body, and the final URL comes back on stderr. Redirect, 1xx and proxy-tunnel header blocks are skipped, so only the final response counts.
- The pager title is the URL as typed. A pager resize re-renders without fetching again.

```sh
mu https://example.com/
mu https://api.github.com/repos/rust-lang/rust
mu -I https://example.com/
mu -H 'Accept: application/json' -X POST -d '{"q":1}' https://httpbin.org/post
```

## URL options {#url-options}

| Flag | Meaning | Sent to curl as |
|---|---|---|
| `-H`, `--header <H>` | Add a request header (repeatable) | `-H` |
| `-X`, `--request <M>` | Use another request method | `-X` |
| `-d`, `--data <D>` | Send data in the request body (repeatable) | `--data` |
| `-A`, `--user-agent <UA>` | Set the User-Agent | `-A`, default `mu/VERSION` |
| `-u`, `--user <U:P>` | Send credentials | `-u` |
| `-k`, `--insecure` | Skip TLS certificate checks | `-k` |
| `-i`, `--include` | Show the response status and headers as a table, then the body | nothing: mu handles it |
| `-I`, `--head` | Show the response status and headers only | `-I -o /dev/null` |
| `--timeout <SECS>` | Give up after this long | `--max-time` |
| `--curl <ARG>` | Pass ARG straight to curl (repeatable), e.g. `--curl --http1.1` | ARG verbatim, before `--` |
| `--no-rewrite` | Fetch the URL as typed | nothing: mu handles it |

> [!WARNING]
> These options are a usage error (exit 2) without a URL. mu's `-w` is still the wrap width, not curl's write-out.

## Format of a response {#url-format}

1. `-f`, if given.
2. The `Content-Type`, without its parameters and in any case. `text/plain` and `application/octet-stream` are skipped. The table below lists the types mu knows.
3. The URL path's extension. The host is never used, so `example.md` as a domain isn't treated as Markdown.
4. The content, via [sniffing](/wiki/formats#sniffing).
5. Markdown.

| Format | Media types |
|---|---|
| Markdown | `text/markdown` `text/x-markdown` |
| HTML | `text/html` `application/xhtml+xml` |
| reStructuredText | `text/x-rst` `text/prs.fallenstein.rst` |
| Org | `text/org` `text/x-org` |
| AsciiDoc | `text/asciidoc` `text/x-asciidoc` |
| JSON | `application/json` `application/x-ndjson` `application/ld+json`, and any type ending in `+json` |
| YAML | `application/yaml` `application/x-yaml` `text/yaml` `text/x-yaml` |
| TOML | `application/toml` `text/x-toml` |
| XML | `application/xml` `text/xml`, and any other type ending in `+xml`, such as `image/svg+xml` or `application/rss+xml` |
| CSV | `text/csv` |
| TSV | `text/tab-separated-values` |
| Source code | `text/x-c` `text/x-csrc` `text/x-chdr` `text/x-c++src` `text/x-c++hdr` `text/x-python` `text/x-rust` `text/x-go` `text/x-java` `text/x-java-source` `text/javascript` `application/javascript` `application/x-javascript` `text/x-sh` `application/x-sh` `text/x-shellscript` `text/x-diff` `text/x-patch` `text/css` `application/sql` |

`--toc` works for URLs too: `mu --toc https://example.com/` prints the page's headings.

## Raw file rewrites {#raw-files}

A GitHub `…/blob/REF/PATH`, GitLab `…/-/blob/REF/PATH` or Gist page URL is fetched as the raw file it shows. `mu` then presents the document, not the site's HTML around it. The query and fragment are dropped. Pass `--no-rewrite` to fetch the page as typed.

```sh
mu https://github.com/machinageist/mu/blob/main/README.md   # renders README.md
mu --no-rewrite https://github.com/machinageist/mu/blob/main/README.md   # renders GitHub's HTML
```

A repository's front page is fetched as HTML. Point at the README's blob URL instead.

## Images in fetched pages {#url-images}

A fetched page loads its remote images as if `--images fetch` were given, with relative paths resolved against the final URL after redirects. Pass `--images auto` or `never` to stop that.

## Links in fetched pages {#page-links}

Fetching a page is separate from displaying its links. Semantic links to validated absolute HTTP(S)/mailto targets follow [the OSC 8 policy](/wiki/usage#links): enabled links show a clickable label without a redundant URL suffix; otherwise their URLs remain printed. mu does not fetch or open those targets merely by rendering them; the terminal emulator handles activation.

Relative document links and fragments outside EPUB are not resolved against the final URL or the image `--base`; they retain the existing non-actionable representation. URLs appearing in fetched source code or data are not auto-linkified. EPUB chapter links instead use [in-pager keyboard navigation](/wiki/pager#chapter-links), without network access or an external opener.

## Errors {#http-errors}

| Situation | What happens | Exit |
|---|---|---|
| HTTP status 400 or more | The body is still shown, then `mu: <url>: HTTP <code>` goes to stderr | 1 |
| curl not installed | “opening URLs needs curl” | 1 |
| Connection or transport failure | curl's own message, prefixed with the URL | 1 |

Other limits: there's no caching between runs, and charsets other than UTF-8 are shown with replacement characters. The full flow is [logic map 8](/wiki/logic-maps#map-8).
