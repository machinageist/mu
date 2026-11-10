// Author: machinageist
// Date: 2026-11-09
// Description: Jupyter notebooks (nbformat 4) turned into Markdown for the shared renderer
// Notes: Markdown cells pass through, code cells become fenced blocks in the kernel's
// language, and outputs become fenced text or data: URI images; anything else is None

use crate::data::value::{self, Value};
use crate::image::base64;

const BYTE_ORDER_MARK: char = '\u{feff}';
const SUPPORTED_NBFORMAT: &str = "4";

// Sniffing: a JSON object naming notebook cells
const OBJECT_OPEN: char = '{';
const SNIFF_KEYS: [&str; 2] = ["\"cells\"", "\"cell_type\""];

// Notebook keys and values
const KEY_NBFORMAT: &str = "nbformat";
const KEY_CELLS: &str = "cells";
const KEY_METADATA: &str = "metadata";
const KEY_KERNELSPEC: &str = "kernelspec";
const KEY_LANGUAGE: &str = "language";
const KEY_LANGUAGE_INFO: &str = "language_info";
const KEY_NAME: &str = "name";
const KEY_CELL_TYPE: &str = "cell_type";
const KEY_SOURCE: &str = "source";
const KEY_ATTACHMENTS: &str = "attachments";
const KEY_EXECUTION_COUNT: &str = "execution_count";
const KEY_OUTPUTS: &str = "outputs";
const KEY_OUTPUT_TYPE: &str = "output_type";
const KEY_TEXT: &str = "text";
const KEY_DATA: &str = "data";
const KEY_ENAME: &str = "ename";
const KEY_EVALUE: &str = "evalue";
const KEY_TRACEBACK: &str = "traceback";
const CELL_MARKDOWN: &str = "markdown";
const CELL_CODE: &str = "code";
const OUTPUT_STREAM: &str = "stream";
const OUTPUT_ERROR: &str = "error";
const STREAM_STDERR: &str = "stderr";

// Output data types, most preferred first
const MIME_PNG: &str = "image/png";
const MIME_JPEG: &str = "image/jpeg";
const MIME_SVG: &str = "image/svg+xml";
const MIME_MARKDOWN: &str = "text/markdown";
const MIME_PLAIN: &str = "text/plain";
const MIME_HTML: &str = "text/html";
const LANGUAGE_HTML: &str = "html";

// Markdown pieces
const ATTACHMENT_PREFIX: &str = "attachment:";
const IMAGE_ALT: &str = "output";
const DATA_URI: &str = "data:";
const BASE64_MARK: &str = ";base64,";
const INPUT_LABEL: (&str, &str) = ("*In \\[", "\\]:*");
const UNRUN_COUNT: &str = " ";
const STDERR_LABEL: &str = "*stderr:*";
const ERROR_SEPARATOR: &str = ": ";
const FENCE_CHAR: char = '`';
const MIN_FENCE: usize = 3;
const BLOCK_GAP: &str = "\n\n";

// Terminal escapes in tracebacks
const ESC: char = '\u{1b}';
const CSI_OPEN: char = '[';
const CSI_FINALS: std::ops::RangeInclusive<char> = '@'..='~';

// Check whether the head of an input looks like a notebook
pub fn sniff(head: &str) -> bool {
    head.trim_start_matches(BYTE_ORDER_MARK)
        .trim_start()
        .starts_with(OBJECT_OPEN)
        && SNIFF_KEYS.iter().all(|key| head.contains(key))
}

// Turn a notebook into Markdown, or None when it is not an nbformat 4 notebook
pub fn markdown(input: &str) -> Option<String> {
    let notebook = value::parse(input.trim_start_matches(BYTE_ORDER_MARK))?;
    match notebook.get(KEY_NBFORMAT)? {
        Value::Number(version) if version == SUPPORTED_NBFORMAT => {}
        _ => return None,
    }
    let cells = notebook.get(KEY_CELLS)?.items();
    let language = language(&notebook);
    let mut blocks: Vec<String> = Vec::new();
    for cell in cells {
        let source = cell.get(KEY_SOURCE).map(Value::text).unwrap_or_default();
        match cell.get(KEY_CELL_TYPE).and_then(Value::as_str) {
            Some(CELL_MARKDOWN) => blocks.push(with_attachments(&source, cell)),
            Some(CELL_CODE) => {
                blocks.push(input_label(cell));
                if !source.trim().is_empty() {
                    blocks.push(fenced(&source, &language));
                }
                for output in cell.get(KEY_OUTPUTS).map_or(&[][..], Value::items) {
                    blocks.extend(output_blocks(output));
                }
            }
            _ => {
                if !source.trim().is_empty() {
                    blocks.push(fenced(&source, ""));
                }
            }
        }
    }
    let mut out = blocks
        .iter()
        .map(|block| block.trim_end())
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join(BLOCK_GAP);
    out.push('\n');
    Some(out)
}

// The kernel's language, for code block info strings
fn language(notebook: &Value) -> String {
    let metadata = notebook.get(KEY_METADATA);
    metadata
        .and_then(|meta| meta.get(KEY_KERNELSPEC))
        .and_then(|spec| spec.get(KEY_LANGUAGE))
        .or_else(|| {
            metadata
                .and_then(|meta| meta.get(KEY_LANGUAGE_INFO))
                .and_then(|info| info.get(KEY_NAME))
        })
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

// `In [n]:` for a code cell, blank when it never ran
fn input_label(cell: &Value) -> String {
    let count = match cell.get(KEY_EXECUTION_COUNT) {
        Some(Value::Number(count)) => count.clone(),
        _ => UNRUN_COUNT.to_owned(),
    };
    format!("{}{count}{}", INPUT_LABEL.0, INPUT_LABEL.1)
}

// Swap `attachment:name` links for data: URIs from the cell's attachments
fn with_attachments(source: &str, cell: &Value) -> String {
    let Some(Value::Object(attachments)) = cell.get(KEY_ATTACHMENTS) else {
        return source.to_owned();
    };
    let mut out = source.to_owned();
    for (name, bundle) in attachments {
        let Value::Object(types) = bundle else {
            continue;
        };
        if let Some((mime, data)) = types.first() {
            let uri = data_uri(mime, &data.text());
            out = out.replace(&format!("{ATTACHMENT_PREFIX}{name}"), &uri);
        }
    }
    out
}

// A data: URI for base64 text, with any line breaks removed
fn data_uri(mime: &str, base64_text: &str) -> String {
    let compact: String = base64_text.split_whitespace().collect();
    format!("{DATA_URI}{mime}{BASE64_MARK}{compact}")
}

// Markdown blocks for one output
fn output_blocks(output: &Value) -> Vec<String> {
    let kind = output.get(KEY_OUTPUT_TYPE).and_then(Value::as_str);
    match kind {
        Some(OUTPUT_STREAM) => {
            let text = strip_escapes(&output.get(KEY_TEXT).map(Value::text).unwrap_or_default());
            let mut blocks = Vec::new();
            if output.get(KEY_NAME).and_then(Value::as_str) == Some(STREAM_STDERR) {
                blocks.push(STDERR_LABEL.to_owned());
            }
            blocks.push(fenced(&text, ""));
            blocks
        }
        Some(OUTPUT_ERROR) => {
            let name = output
                .get(KEY_ENAME)
                .and_then(Value::as_str)
                .unwrap_or_default();
            let message = output
                .get(KEY_EVALUE)
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mut text = format!("{name}{ERROR_SEPARATOR}{message}\n");
            for line in output.get(KEY_TRACEBACK).map_or(&[][..], Value::items) {
                if let Some(line) = line.as_str() {
                    text.push_str(&strip_escapes(line));
                    text.push('\n');
                }
            }
            vec![fenced(&text, "")]
        }
        _ => output
            .get(KEY_DATA)
            .and_then(rich_block)
            .into_iter()
            .collect(),
    }
}

// The preferred representation of a rich output's data bundle
fn rich_block(data: &Value) -> Option<String> {
    let text = |mime: &str| data.get(mime).map(Value::text);
    if let Some(image) = text(MIME_PNG) {
        return Some(image_block(MIME_PNG, &image));
    }
    if let Some(image) = text(MIME_JPEG) {
        return Some(image_block(MIME_JPEG, &image));
    }
    if let Some(svg) = text(MIME_SVG) {
        return Some(image_block(MIME_SVG, &base64::encode(svg.as_bytes())));
    }
    if let Some(markdown) = text(MIME_MARKDOWN) {
        return Some(markdown);
    }
    if let Some(plain) = text(MIME_PLAIN) {
        return Some(fenced(&strip_escapes(&plain), ""));
    }
    text(MIME_HTML).map(|html| fenced(&html, LANGUAGE_HTML))
}

// A Markdown image of a data: URI
fn image_block(mime: &str, base64_text: &str) -> String {
    format!("![{IMAGE_ALT}]({})", data_uri(mime, base64_text))
}

// A fenced code block whose fence is longer than any backtick run inside it
fn fenced(text: &str, language: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for ch in text.chars() {
        run = if ch == FENCE_CHAR { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let fence = FENCE_CHAR.to_string().repeat((longest + 1).max(MIN_FENCE));
    let body = text.trim_end_matches('\n');
    format!("{fence}{language}\n{body}\n{fence}")
}

// Remove CSI escape sequences, as colored tracebacks carry
fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == ESC && chars.next_if_eq(&CSI_OPEN).is_some() {
            for inner in chars.by_ref() {
                if CSI_FINALS.contains(&inner) {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_outgrows_backtick_runs() {
        assert_eq!(fenced("a", "py"), "```py\na\n```");
        assert_eq!(fenced("x ```` y\n", ""), "`````\nx ```` y\n`````");
    }

    #[test]
    fn escapes_are_stripped() {
        assert_eq!(
            strip_escapes("\u{1b}[0;31mValueError\u{1b}[0m: x"),
            "ValueError: x"
        );
    }

    #[test]
    fn sniff_needs_cells() {
        assert!(sniff("{\n \"cells\": [\n  {\n   \"cell_type\": \"code\""));
        assert!(!sniff("{\"cells\": 1}"));
        assert!(!sniff("# \"cells\" \"cell_type\""));
    }

    #[test]
    fn other_versions_are_none() {
        assert_eq!(markdown(r#"{"nbformat": 3, "cells": []}"#), None);
        assert_eq!(markdown("{not json"), None);
        assert_eq!(
            markdown(r#"{"nbformat": 4, "cells": []}"#),
            Some("\n".into())
        );
    }

    #[test]
    fn cells_and_outputs_become_markdown() {
        let notebook = r##"{"nbformat": 4, "metadata": {"kernelspec": {"language": "python"}},
          "cells": [
            {"cell_type": "markdown", "source": ["# Title\n", "![a](attachment:a.png)"],
             "attachments": {"a.png": {"image/png": "iVBO\nRw=="}}},
            {"cell_type": "code", "execution_count": 2, "source": "print(1)",
             "outputs": [{"output_type": "stream", "name": "stdout", "text": ["1\n"]},
                         {"output_type": "error", "ename": "E", "evalue": "v", "traceback": ["\u001b[31mtb\u001b[0m"]}]},
            {"cell_type": "code", "execution_count": null, "source": "", "outputs": []}
          ]}"##;
        assert_eq!(
            markdown(notebook).unwrap(),
            "# Title\n![a](data:image/png;base64,iVBORw==)\n\n*In \\[2\\]:*\n\n```python\nprint(1)\n```\n\n```\n1\n```\n\n```\nE: v\ntb\n```\n\n*In \\[ \\]:*\n"
        );
    }
}
