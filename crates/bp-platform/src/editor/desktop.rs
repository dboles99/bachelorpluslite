//! The Linux registration artefacts: a `.desktop` entry and a MIME package.
//!
//! Both are plain text, both are produced here as `String`s, and neither is
//! written anywhere by this module. That separation is the point -- generating
//! the file the desktop *would* read is decision-free and testable, putting it
//! where the desktop reads it is neither, and lives in [`super::install`].
//!
//! ## Escaping, which is the whole difficulty
//!
//! A `.desktop` file is a line-oriented `key=value` format with no quoting at
//! the line level, so a value containing a newline does not produce a broken
//! file -- it produces a *valid* file with an extra key in it. Every string
//! that reaches an artefact here comes from an [`AppInfo`](super::AppInfo)
//! that the shell filled in, so "the application's display name" is
//! attacker-adjacent in exactly the way a filename is, and the escaping is
//! written on that assumption rather than on the assumption that the name is
//! "BachelorPad+".
//!
//! The `Exec` key escapes **twice**, and getting that wrong is the classic
//! defect in this format: its value is first quoted by the Exec argument rules
//! (freedesktop's own reserved-character list) and the result is then a
//! desktop-entry string, where a backslash means an escape. A single Windows
//! -style path with a backslash in it therefore needs four backslashes in the
//! file to survive the round trip, and a single-escaped one launches the wrong
//! program.

use super::AppInfo;
use super::file_type::AssociationSelection;

/// Characters the Exec key treats as reserved, per the Desktop Entry
/// Specification.
///
/// An argument containing any of them must be quoted. Listed rather than
/// approximated with "is this alphanumeric", because the correct set is short,
/// fixed, and published, and an approximation is a quoting rule that differs
/// from the one the desktop environment parses with.
const EXEC_RESERVED: &[char] = &[
    ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(', ')',
    '`',
];

/// The file name a `.desktop` entry for `app` must have.
///
/// The name is not cosmetic: `mimeapps.list` refers to an application *by this
/// file name*, so it is the application's identity on Linux and changing it
/// orphans every association the user already made.
#[must_use]
pub fn desktop_file_name(app: &AppInfo) -> String {
    format!("{}.desktop", app.app_id)
}

/// The file name of the MIME package for `app`.
#[must_use]
pub fn mime_package_file_name(app: &AppInfo) -> String {
    format!("{}.xml", app.app_id)
}

/// A `.desktop` entry claiming `selection`.
///
/// Deterministic: the same inputs give byte-identical output, because the
/// selection iterates in table order and the MIME list is deduplicated in a
/// stable one. An artefact that reorders itself between runs shows up as a
/// change in every backup and every configuration-management diff, and
/// eventually gets ignored.
///
/// An empty selection omits the `MimeType` key entirely rather than emitting
/// an empty one -- the product still belongs in the application menu, it just
/// claims nothing.
#[must_use]
pub fn desktop_entry(app: &AppInfo, selection: &AssociationSelection) -> String {
    let mut out = String::from("[Desktop Entry]\n");
    out.push_str("Type=Application\n");
    out.push_str("Version=1.0\n");
    push_key(&mut out, "Name", &app.display_name);
    out.push_str("GenericName=Text Editor\n");
    push_key(&mut out, "Comment", &app.description);
    // `%F` rather than `%U`: this is an editor for files on a disk, and
    // accepting a URL it cannot open would put a broken entry in "Open With"
    // for every remote location the desktop knows about.
    push_key(
        &mut out,
        "Exec",
        &format!("{} %F", quote_exec_argument(&app.executable)),
    );
    push_key(&mut out, "TryExec", &app.executable);
    push_key(&mut out, "Icon", &app.icon);
    out.push_str("Terminal=false\n");
    out.push_str("StartupNotify=true\n");
    out.push_str("Categories=Utility;TextEditor;\n");
    out.push_str("Keywords=text;editor;notes;markdown;log;\n");
    let mimes = selection.mime_types();
    if !mimes.is_empty() {
        out.push_str("MimeType=");
        for mime in mimes {
            out.push_str(&escape_list_item(mime));
            out.push(';');
        }
        out.push('\n');
    }
    out
}

/// A `shared-mime-info` package defining the types nobody else defines, or
/// `None` when the selection contains none.
///
/// Only the product's own types get a glob here. Shipping one for `.json`
/// would be a disagreement with the distribution's own MIME database about
/// what `.json` is, and the distribution wins that argument on every machine
/// where it is installed second.
#[must_use]
pub fn mime_package(selection: &AssociationSelection) -> Option<String> {
    let own: Vec<_> = selection
        .file_types()
        .filter(|t| t.is_own_mime_type())
        .collect();
    if own.is_empty() {
        return None;
    }
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n");
    // One `mime-type` element per distinct type, with every extension that
    // maps to it as a glob -- three PowerShell extensions are one type, and
    // repeating the element would define it three times.
    let mut done: Vec<&str> = Vec::new();
    for file_type in &own {
        if done.contains(&file_type.mime) {
            continue;
        }
        done.push(file_type.mime);
        out.push_str(&format!(
            "  <mime-type type=\"{}\">\n",
            escape_xml(file_type.mime)
        ));
        out.push_str(&format!(
            "    <comment>{}</comment>\n",
            escape_xml(file_type.description)
        ));
        for sibling in own.iter().filter(|t| t.mime == file_type.mime) {
            out.push_str(&format!(
                "    <glob pattern=\"{}\"/>\n",
                escape_xml(&sibling.glob())
            ));
        }
        out.push_str("  </mime-type>\n");
    }
    out.push_str("</mime-info>\n");
    Some(out)
}

/// Append `key=value` with the value escaped as a desktop-entry string.
fn push_key(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push('=');
    out.push_str(&escape_value(value));
    out.push('\n');
}

/// A string escaped for a desktop-entry value.
///
/// The escape that matters is the newline. A `.desktop` file has no line
/// continuation and no quoting, so an un-escaped newline in a `Name` does not
/// corrupt the file -- it silently adds whatever follows as another key, which
/// is how a display name becomes an `Exec` line. The backslash escape has to
/// come first, or escaping a newline would itself be re-escaped.
#[must_use]
pub fn escape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '\n' => out.push_str(r"\n"),
            '\t' => out.push_str(r"\t"),
            '\r' => out.push_str(r"\r"),
            // Any other control character has no escape in this format and no
            // legitimate use in a name; dropping it is the only option that
            // cannot produce a file the parser reads differently.
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

/// One item of a semicolon-separated desktop-entry list.
///
/// The separator is escapable and must be escaped, or a MIME type containing a
/// semicolon -- which nothing sane has, and an [`AppInfo`] field certainly
/// could -- would split into two claims.
#[must_use]
pub fn escape_list_item(item: &str) -> String {
    escape_value(item).replace(';', r"\;")
}

/// One argument of an `Exec` value, quoted if the specification requires it.
///
/// Quoting only when required, because `Exec=/usr/bin/bachelorpad %F` is what
/// every other entry on the system looks like and an always-quoted value, while
/// legal, makes a hand-inspected file look wrong. The escaping *inside* the
/// quotes is the specification's own short list: `"`, backtick, `$` and `\`.
///
/// Note this returns the Exec-level form. It is escaped a second time by
/// [`escape_value`] when it becomes a line, which is correct and is the step
/// most implementations miss.
#[must_use]
pub fn quote_exec_argument(argument: &str) -> String {
    if !argument.is_empty() && !argument.contains(EXEC_RESERVED) {
        return argument.to_owned();
    }
    let mut out = String::with_capacity(argument.len() + 2);
    out.push('"');
    for c in argument.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Text escaped for an XML attribute or element body.
///
/// Attributes here are always double-quoted, so the apostrophe needs no
/// entity; the other four do. `&` first, or the ampersands introduced by the
/// later replacements would be escaped again.
#[must_use]
pub fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
