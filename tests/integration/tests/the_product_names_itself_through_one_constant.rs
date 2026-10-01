//! The product's name is written once, in `bp_platform::DISPLAY_NAME`, and
//! read everywhere else.
//!
//! **This test was cited before it existed.** `DISPLAY_NAME`'s doc comment
//! named `every_place_the_product_names_itself_must_read_display_name` as
//! the thing that asks, from the commit that wrote the comment until 1.0, and
//! no test of that name had ever been written. Meanwhile four user-visible
//! messages went on saying "BachelorPad+" -- the full product's name -- in the
//! Lite edition's registration dialogs. Trap 3 in the comment written to warn
//! about it; this is the test, under a name that says what it checks.
//!
//! What it owns: string literals in shipped Rust and Slint source. What it
//! deliberately does not: comments, which may name the full product when
//! that is what they mean; tests, which assert what the product prints; and
//! the generator scripts, which are PowerShell and cannot read a Rust
//! constant -- they are the rename checklist's job, and ADR-0074 lists them.

use std::path::{Path, PathBuf};

/// Spellings the product has had or is related to. A literal holding any of
/// them names the product.
const NAMES: &[&str] = &["BachelorPad+", "BachelorPlusLite", "BachelorPadPlus"];

/// The constants allowed to hold a name, each for a reason the constant's own
/// doc comment gives, and each pinned to `DISPLAY_NAME` by a test where it
/// can be.
const ALLOWED: &[&str] = &[
    "pub const DISPLAY_NAME",
    "pub const SOFTWARE_KEY",
    "pub const CAPABILITIES_KEY",
    "pub const PROG_ID_PREFIX",
    "pub const LEGACY_PROG_ID_PREFIX",
    "pub const LEGACY_SOFTWARE_KEY",
    "pub const LEGACY_REGISTERED_NAMES",
];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tests/integration sits two levels down")
        .to_path_buf()
}

fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            sources(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs" || e == "slint")
            && path.file_name().is_some_and(|n| n != "tests.rs")
        {
            found.push(path);
        }
    }
}

/// The string literals on a line of code, ignoring anything after `//`
/// outside a literal. Rough on purpose: a literal spanning lines is read a
/// line at a time, which is enough for a name.
fn literals(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (&mut current, c) {
            (None, '/') if chars.peek() == Some(&'/') => break,
            (None, '"') => current = Some(String::new()),
            (Some(text), '"') => {
                out.push(std::mem::take(text));
                current = None;
            }
            (Some(text), '\\') => {
                text.push('\\');
                if let Some(next) = chars.next() {
                    text.push(next);
                }
            }
            (Some(text), c) => text.push(c),
            (None, _) => {}
        }
    }
    out.extend(current);
    out
}

#[test]
fn no_shipped_string_names_the_product_except_the_constants_that_hold_it() {
    let root = workspace();
    let mut files = Vec::new();
    sources(&root.join("crates"), &mut files);
    sources(&root.join("apps"), &mut files);
    assert!(
        files.len() > 50,
        "found only {} source files under {} -- the walk is wrong, not the code",
        files.len(),
        root.display()
    );

    let mut offences = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("readable source");
        for (number, line) in text.lines().enumerate() {
            // Tests sit at the end of a file in this workspace, behind this
            // attribute; what they assert about output is not output.
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            if line.trim_start().starts_with("//") || ALLOWED.iter().any(|a| line.contains(a)) {
                continue;
            }
            for literal in literals(line) {
                if NAMES.iter().any(|name| literal.contains(name)) {
                    offences.push(format!(
                        "{}:{}: {}",
                        file.strip_prefix(&root).unwrap_or(file).display(),
                        number + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    assert!(
        offences.is_empty(),
        "the product's name is spelled out instead of read from \
         bp_platform::DISPLAY_NAME:\n{}",
        offences.join("\n")
    );
}

#[test]
fn the_reader_of_literals_finds_what_it_is_for() {
    // The test above is only as good as this, so it is asked directly.
    assert_eq!(
        literals(r#"let a = "BachelorPad+ opens"; // "not this""#),
        ["BachelorPad+ opens"]
    );
    assert_eq!(literals(r#"x("a\"b", "c")"#), [r#"a\"b"#, "c"]);
    assert!(literals("// \"BachelorPad+\" in a comment").is_empty());
}
