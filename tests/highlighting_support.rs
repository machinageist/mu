// Author: machinageist
// Date: 2026-11-12
// Description: Highlighting reaches every document code adapter and preserves plain output

mod common;

use common::{colored, plain, strip_sgr};
use mu::{Format, Options, present};

const PYTHON: &str = "def f(): return None";
const KEYWORD: &str = "\x1b[0;1;35mdef\x1b[0m";

fn assert_python(input: &str, format: Format) {
    for width in [30, 47, 80] {
        let color = colored(input, format, width);
        assert!(color.contains(KEYWORD), "{format:?}: {color:?}");
        assert_eq!(strip_sgr(&color), plain(input, format, width));
        for line in color.lines() {
            if let Some(last) = line.rfind('\x1b') {
                assert!(line[last..].starts_with("\x1b[0m"), "{line:?}");
            }
        }
    }
}

#[test]
fn document_code_adapters_share_highlighting() {
    let cases = [
        (Format::Markdown, format!("```python\n{PYTHON}\n```\n")),
        (
            Format::Html,
            format!("<pre><code class=\"language-python\">{PYTHON}</code></pre>"),
        ),
        (
            Format::Epub,
            format!("<pre><code class=\"language-python\">{PYTHON}</code></pre>"),
        ),
        (
            Format::Rst,
            format!(".. code-block:: python\n\n   {PYTHON}\n"),
        ),
        (
            Format::Org,
            format!("#+BEGIN_SRC python\n{PYTHON}\n#+END_SRC\n"),
        ),
        (
            Format::AsciiDoc,
            format!("[source,python]\n----\n{PYTHON}\n----\n"),
        ),
        (
            Format::Jupyter,
            format!(
                r#"{{"nbformat":4,"metadata":{{"kernelspec":{{"language":"python"}}}},"cells":[{{"cell_type":"code","source":"{PYTHON}","outputs":[]}}]}}"#
            ),
        ),
    ];
    for (format, input) in cases {
        assert_python(&input, format);
    }
}

#[test]
fn html_code_uses_common_language_hints() {
    for input in [
        format!("<pre><code class=\"python\">{PYTHON}</code></pre>"),
        format!("<pre data-lang=\"python\">{PYTHON}</pre>"),
        format!("<pre><code data-language=\"python\">{PYTHON}</code></pre>"),
        format!(
            "<div class=\"language-python highlighter-rouge\"><div class=\"highlight\"><pre><code>{PYTHON}</code></pre></div></div>"
        ),
    ] {
        assert_python(&input, Format::Html);
    }
}

#[test]
fn html_inner_code_language_overrides_wrapper_language() {
    assert_python(
        &format!(
            "<div class=\"language-rust\"><pre class=\"language-rust\"><code class=\"language-python\">{PYTHON}</code></pre></div>"
        ),
        Format::Html,
    );
}

#[test]
fn html_language_hints_do_not_leak_or_override_an_explicit_unknown_language() {
    let input = format!(
        "<div class=\"language-python\"><pre>{PYTHON}</pre><pre><code class=\"language-unknown\">{PYTHON}</code></pre></div><pre>{PYTHON}</pre><div class=\"python\"><pre>{PYTHON}</pre></div>"
    );
    let out = colored(&input, Format::Html, 80);
    assert_eq!(out.matches(KEYWORD).count(), 1, "{out:?}");
    assert_eq!(strip_sgr(&out), plain(&input, Format::Html, 80));
}

#[test]
fn data_language_aliases_also_highlight_verbatim_source() {
    for (names, input) in [
        (&["json", "jsonl", "geojson"][..], "{\"key\":1}"),
        (&["yaml", "yml"][..], "key: 1"),
        (&["toml"][..], "key = 1"),
        (
            &[
                "xml", "plist", "xsd", "xsl", "xslt", "rss", "atom", "svg", "opml", "html", "htm",
                "xhtml",
            ][..],
            "<root key=\"value\"/>",
        ),
    ] {
        for name in names {
            let out = present(
                input,
                Format::Code,
                &Options {
                    language: Some((*name).to_owned()),
                    ..Options::new(80, true)
                },
            );
            assert!(out.contains('\x1b'), "{name}: {out:?}");
            assert_eq!(strip_sgr(&out), format!("{input}\n"), "{name}");
        }
    }
}

#[test]
fn shebang_highlighting_accepts_bom_and_env_options() {
    for shebang in [
        "\u{feff}#!/usr/bin/env python3",
        "#!/usr/bin/env MODE=test python3",
        "#!/usr/bin/env -i python3",
        "#!/usr/bin/env -S python3 -u",
    ] {
        assert_python(&format!("{shebang}\n{PYTHON}\n"), Format::Code);
    }
}

#[test]
fn unlabelled_document_scripts_use_their_shebang() {
    let script = format!("#!/usr/bin/env python3\n{PYTHON}\n");
    let cases = [
        (Format::Markdown, format!("```\n{script}```\n")),
        (Format::Html, format!("<pre>{script}</pre>")),
        (Format::Epub, format!("<pre>{script}</pre>")),
        (
            Format::Rst,
            format!(
                "Example::\n\n   {}\n   {PYTHON}\n",
                script.lines().next().unwrap()
            ),
        ),
        (
            Format::Org,
            format!("#+BEGIN_EXAMPLE\n{script}#+END_EXAMPLE\n"),
        ),
        (Format::AsciiDoc, format!("----\n{script}----\n")),
        (Format::Man, format!(".TH TEST 1\n.EX\n{script}.EE\n")),
        (
            Format::Jupyter,
            format!(
                r##"{{"nbformat":4,"cells":[{{"cell_type":"code","source":["#!/usr/bin/env python3\n","{PYTHON}\n"],"outputs":[]}}]}}"##
            ),
        ),
    ];
    for (format, input) in cases {
        assert_python(&input, format);
    }
}

#[test]
fn explicitly_unknown_code_languages_stay_plain() {
    let input = format!("```unknown\n#!/usr/bin/env python3\n{PYTHON}\n```\n");
    let out = present(&input, Format::Markdown, &Options::new(80, true));
    assert!(!out.contains(KEYWORD), "{out:?}");
    assert_eq!(strip_sgr(&out), plain(&input, Format::Markdown, 80));
}

#[test]
fn html_nested_code_keeps_its_language() {
    let input = format!("<pre><span><code class=\"language-python\">{PYTHON}</code></span></pre>");
    for format in [Format::Html, Format::Epub] {
        assert_python(&input, format);
    }
}

#[test]
fn asciidoc_source_languages_accept_quotes_and_existing_attributes() {
    for language in ["\"python\"", "'python'", "{lang}", "\"{lang}\""] {
        let input = format!(":lang: python\n\n[source,{language}]\n----\n{PYTHON}\n----\n");
        assert_python(&input, Format::AsciiDoc);
    }
}

#[test]
fn item_language_hints_override_their_list_or_definition_list() {
    for (open, close, item) in [("ul", "ul", "li"), ("dl", "dl", "dd")] {
        let input =
            format!("<{open}><{item} data-lang=\"python\"><pre>{PYTHON}</pre></{item}></{close}>");
        assert_python(&input, Format::Html);
        let input = format!(
            "<{open} class=\"language-python\"><{item} data-lang=\"unknown\"><pre>{PYTHON}</pre></{item}></{close}>"
        );
        let out = colored(&input, Format::Html, 80);
        assert!(!out.contains(KEYWORD), "{out:?}");
        assert_eq!(strip_sgr(&out), plain(&input, Format::Html, 80));
    }
}

#[test]
fn innermost_nested_code_hint_takes_precedence() {
    let input = format!(
        "<pre class=\"language-rust\"><code><span><code data-lang=\"python\">{PYTHON}</code></span></code></pre>"
    );
    assert_python(&input, Format::Html);
}
