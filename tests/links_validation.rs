// Author: machinageist
// Description: Validate external targets at the public presentation and CLI boundaries

use mu::{Format, Options, present, render};
use std::io::Write;
use std::process::{Command, Stdio};

fn options(links: bool) -> Options {
    Options {
        links,
        ..Options::new(240, false)
    }
}

fn html_link(target: &str) -> String {
    let attribute = target
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<p><a href=\"{attribute}\">label</a></p>")
}

#[test]
fn malformed_web_authorities_retain_the_exact_plain_fallback() {
    for target in [
        "https://[",
        "https://[::1/",
        "https://::1/",
        "https://[not-an-ip]/",
        "https://[]/",
        "https://[2001:db8::1]extra/",
        "https://[2001:db8::1]]/",
        "https://example.org]/",
        "https://exa[mple.org/",
        "https://example.org:garbage/",
        "https://example.org:/",
        "https://example.org:-1/",
        "https://example.org:+443/",
        "https://example.org:65536/",
        "https://example.org:443:80/",
        "https://[::1]:garbage/",
        "https://[::1]:/",
        "https://[::1]:65536/",
        "https://user@@example.org/",
        "https://user:pass@/",
        "https://exa^mple.org/",
        "https://exa{mple.org/",
        "https://exa|mple.org/",
        "https://exa mple.org/",
        "https://exa%GGmple.org/",
    ] {
        let source = html_link(target);
        let plain = present(&source, Format::Html, &options(false));
        assert_eq!(plain, format!("label ({target})\n"), "{target:?}");
        assert_eq!(
            present(&source, Format::Html, &options(true)),
            plain,
            "rejected authority became actionable: {target:?}"
        );
    }
    for target in ["https://[", "https://example.org:garbage/"] {
        assert_eq!(
            render(&format!("[label]({target})"), &options(true)),
            format!("label ({target})\n"),
            "{target:?}"
        );
    }
}

#[test]
fn org_file_links_preserve_their_scheme_and_never_become_external_links() {
    for target in [
        "file:https://example.org/path",
        "file:http://example.org:443/path",
        "file:mailto:reader@example.org",
        "file:///tmp/notes.org",
        "file:notes.org",
    ] {
        for (source, expected) in [
            (
                format!("[[{target}][label]]"),
                format!("label ({target})\n"),
            ),
            (format!("[[{target}]]"), format!("{target}\n")),
        ] {
            for links in [true, false] {
                assert_eq!(
                    present(&source, Format::Org, &options(links)),
                    expected,
                    "{source:?}, links {links}"
                );
            }
        }
    }
    assert_eq!(
        present("[[file:img/cat.png]]", Format::Org, &options(true)),
        "[image: img/cat.png]\n"
    );
    assert_eq!(
        present(
            "[[https://example.org/path][label]]",
            Format::Org,
            &options(true)
        ),
        "\x1b]8;;https://example.org/path\x1b\\label\x1b]8;;\x1b\\\n"
    );
}

#[test]
fn nul_entities_and_replacement_characters_cannot_become_repaired_uri_targets() {
    for source in [
        "[label](https://example.org/a&#0;b)",
        "[label](https://example.org/a&#x0;b)",
        "[label](https://example.org/a\0b)",
        "[label](https://example.org/a\u{fffd}b)",
        "[label](mailto:a\u{fffd}b@example.org)",
    ] {
        let plain = render(source, &options(false));
        assert!(plain.contains("label"), "{source:?}");
        assert_eq!(render(source, &options(true)), plain, "{source:?}");
        assert!(!plain.contains('\0'), "source NUL was replayed");
        assert!(!plain.contains('\x1b'), "source control was replayed");
    }
    for target in [
        "https://example.org/a\0b",
        "https://example.org/a\u{fffd}b",
        "https://exa\u{fffd}mple.org/",
        "https://us\u{fffd}er@example.org/",
        "mailto:a\u{fffd}b@example.org",
    ] {
        let source = html_link(target);
        let plain = present(&source, Format::Html, &options(false));
        assert!(plain.contains("label"), "{target:?}");
        assert_eq!(
            present(&source, Format::Html, &options(true)),
            plain,
            "{target:?}"
        );
        assert!(!plain.contains('\0'), "source NUL was replayed");
        assert!(!plain.contains('\x1b'), "source control was replayed");
    }
    // Replacement characters in visible labels do not invalidate a sound target
    assert_eq!(
        render("[a\u{fffd}b](https://example.org/)", &options(true)),
        "\x1b]8;;https://example.org/\x1b\\a\u{fffd}b\x1b]8;;\x1b\\\n"
    );
}

#[test]
fn forced_cli_links_keep_invalid_and_ambiguous_targets_plain() {
    for (format, source, expected) in [
        (
            "markdown",
            "[label](https://example.org/a&#0;b)",
            "label (https://example.org/a\u{fffd}b)\n",
        ),
        (
            "markdown",
            "[label](https://example.org/a\0b)",
            "[label](https://example.org/a\u{fffd}b)\n",
        ),
        ("markdown", "[label](https://[)", "label (https://[)\n"),
        (
            "html",
            "<a href='https://example.org:garbage/'>label</a>",
            "label (https://example.org:garbage/)\n",
        ),
        (
            "org",
            "[[file:https://example.org/path][label]]",
            "label (file:https://example.org/path)\n",
        ),
        (
            "org",
            "[[file:mailto:reader@example.org][label]]",
            "label (file:mailto:reader@example.org)\n",
        ),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mu"))
            .env_clear()
            .args([
                "--paging=never",
                "--images=never",
                "--links=always",
                "--color=never",
                "--width=240",
                "--format",
                format,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn mu");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(source.as_bytes())
            .expect("write document");
        let output = child.wait_with_output().expect("wait for mu");
        assert!(
            output.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).expect("UTF-8 output"),
            expected,
            "{format}: {source:?}"
        );
    }
}

#[test]
fn valid_ipv6_ports_userinfo_and_reg_names_keep_the_original_uri_spelling() {
    for (target, encoded) in [
        ("https://[::1]/", "https://[::1]/"),
        (
            "https://[2001:DB8::1]:8443/path",
            "https://[2001:DB8::1]:8443/path",
        ),
        (
            "http://[::ffff:192.0.2.1]:80/",
            "http://[::ffff:192.0.2.1]:80/",
        ),
        ("https://example.org:0/", "https://example.org:0/"),
        ("https://example.org:65535/", "https://example.org:65535/"),
        ("https://example.org:000443/", "https://example.org:000443/"),
        (
            "https://user:secret@example.org:443/path?query#part",
            "https://user:secret@example.org:443/path?query#part",
        ),
        (
            "https://user%40name:p%3Ass@[::1]:443/",
            "https://user%40name:p%3Ass@[::1]:443/",
        ),
        ("https://***@example.org/", "https://***@example.org/"),
        (
            "https://usér@bücher.example/",
            "https://us%C3%A9r@b%C3%BCcher.example/",
        ),
        (
            "https://service_name.example/",
            "https://service_name.example/",
        ),
        (
            "https://reg!$&'()*+,;=name.example/",
            "https://reg!$&'()*+,;=name.example/",
        ),
        ("https://%65xample.org/", "https://%65xample.org/"),
    ] {
        assert_eq!(
            present(&html_link(target), Format::Html, &options(true)),
            format!("\x1b]8;;{encoded}\x1b\\label\x1b]8;;\x1b\\\n"),
            "{target:?}"
        );
    }
}
