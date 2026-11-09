# Plan: open URLs directly (0.4.0)

Gate for every commit:

```sh
cargo fmt --check && cargo test --release --no-fail-fast && cargo clippy --all-targets -- -D warnings
```

## 1. Media types
Add `Format::from_media_type` in the library (format/mod.rs), plus `Format::from_url_path`, which reads the extension of the path part only.
→ verify: unit tests, including that `https://example.org` is not Org

## 2. Fetcher
`src/fetch.rs` in the binary holds `Request` → curl args, response header blocks parsed from `-D -`, the final URL from write-out, the raw rewrite, and the header table as Markdown.
→ verify: unit tests for args, parsing (redirect chains, 100 Continue), rewrites, and `|` escaping

## 3. CLI wiring
Recognise URL arguments, the new flags, the format order, the images default, exit codes and the pager title.
→ verify: integration tests against a local `TcpListener` server: HTML, JSON by content type, a 404 exit code, `-I` headers, a flag used with a file being a usage error

## 4. Docs and release
README section, FEATURE_SPEC, CHANGELOG, release 0.4.0, push and tags.
