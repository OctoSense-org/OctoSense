//! The static check every template and fork passes before it is admitted:
//! OctoScript's canonical syntax, size caps, and **only declared host modules
//! and methods referenced**. The runner additionally installs only the
//! declared methods, so a call the check cannot see (a computed member) still
//! finds nothing to call.

use crate::manifest::{Manifest, MAX_SOURCE_BYTES};
use crate::modules::STD_MODULES;
use crate::{Error, ErrorKind, Result};

fn refuse(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Check, message)
}

/// Checks `source` against `manifest`. Returns the refusal reasons joined, or
/// `Ok` when the template may run.
pub fn check_source(manifest: &Manifest, source: &str) -> Result<()> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(refuse(format!(
            "template.octoscript is {} bytes; the limit is {MAX_SOURCE_BYTES}",
            source.len()
        )));
    }
    let syntax = octoscript_core::check_syntax(source)
        .map_err(|e| refuse(format!("syntax check: {e:?}")))?;
    if !syntax.valid {
        let first = syntax
            .diagnostics
            .first()
            .map(|d| format!("{}:{}: {}", d.line, d.column, d.message))
            .unwrap_or_default();
        return Err(refuse(format!("not canonical OctoScript: {first}")));
    }

    let imports = octoscript_core::module_import_report(source)
        .map_err(|e| refuse(format!("import report: {e:?}")))?;
    if imports.truncated || imports.valid_prefix_end_byte < source.trim_end().len() {
        return Err(refuse("the import report is incomplete"));
    }
    for import in &imports.imports {
        let path: Vec<&str> = import.path.iter().map(String::as_str).collect();
        match path.as_slice() {
            ["mod", "std", name] if STD_MODULES.contains(name) => {}
            ["mod", name] if manifest.modules.iter().any(|d| d.module == *name) => {}
            _ => {
                return Err(refuse(format!(
                    "use {}: not a declared host module",
                    path.join(".")
                )))
            }
        }
    }

    let calls = octoscript_core::imported_module_call_hint_report(source)
        .map_err(|e| refuse(format!("call report: {e:?}")))?;
    if calls.truncated {
        return Err(refuse("the module call report is incomplete"));
    }
    for hint in &calls.hints {
        let path: Vec<&str> = hint.module_path.iter().map(String::as_str).collect();
        if let ["mod", module] = path.as_slice() {
            if !manifest.declares(module, &hint.method) {
                return Err(refuse(format!(
                    "{}:{}: {module}.{} is not declared in template.json",
                    hint.line, hint.column, hint.method
                )));
            }
        }
    }

    // `mod.<x>` outside a `use` line (a direct path expression) is not
    // resolved by the reports above; templates must import what they use.
    for (index, line) in code_lines(source).enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("use ") {
            continue;
        }
        if contains_word(&line, "mod") {
            return Err(refuse(format!(
                "line {}: `mod` may appear only in a `use` declaration",
                index + 1
            )));
        }
    }
    Ok(())
}

/// Source lines with `//` comments and string literal contents removed.
fn code_lines(source: &str) -> impl Iterator<Item = String> + '_ {
    source.lines().map(|line| {
        let mut out = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        let mut in_string = false;
        while let Some(c) = chars.next() {
            if in_string {
                if c == '\\' {
                    chars.next();
                } else if c == '"' {
                    in_string = false;
                    out.push('"');
                }
                continue;
            }
            if c == '"' {
                in_string = true;
                out.push('"');
                continue;
            }
            if c == '/' && chars.peek() == Some(&'/') {
                break;
            }
            out.push(c);
        }
        out
    })
}

fn contains_word(line: &str, word: &str) -> bool {
    let bytes = line.as_bytes();
    let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    line.match_indices(word).any(|(at, _)| {
        let before = at == 0 || !ident(bytes[at - 1]);
        let end = at + word.len();
        let after = end >= bytes.len() || !ident(bytes[end]);
        before && after
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_comments() {
        assert!(contains_word("x = mod.research", "mod"));
        assert!(!contains_word("let model = 1", "mod"));
        let lines: Vec<_> = code_lines("let a = \"mod.tool\" // mod.tool").collect();
        assert_eq!(lines, vec!["let a = \"\" ".to_owned()]);
    }
}
