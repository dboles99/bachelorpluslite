//! The Windows registration shape, as values, plus the guard ADR-0012 needs.
//!
//! ADR-0012 says: register as a supported handler and guide the user to the
//! operating system's own default-app selection; do not replace, patch, shim
//! or redirect `notepad.exe`. Written down that is a paragraph. Here it is a
//! function -- [`key_objection`] -- that every generated key is passed through
//! and that a test asserts nothing gets past.
//!
//! ## What is written, and what is deliberately not
//!
//! Three places, all of them under `HKEY_CURRENT_USER`:
//!
//! * `HKCU\Software\Classes\BachelorPadPlus.<ext>` -- one ProgID per file
//!   type, holding the friendly name, the icon and the open command. These are
//!   ours; nothing else has a claim on a key with our name in it.
//! * `HKCU\Software\Classes\.<ext>\OpenWithProgids` -- one *value* per
//!   extension, naming the ProgID. This is the additive registration: it puts
//!   the product in the "Open with" list without taking the extension from
//!   whoever has it.
//! * `HKCU\Software\BachelorPad+\Capabilities` and one value under
//!   `HKCU\Software\RegisteredApplications` -- what makes the product appear
//!   in Settings ▸ Default apps at all.
//!
//! And what is **not** written, each for a reason:
//!
//! * the *default* value of `HKCU\Software\Classes\.<ext>`. Setting it is the
//!   old way to seize an extension, it works, and it is precisely the
//!   "bypass" ADR-0012 refuses. The `OpenWithProgids` value beside it offers;
//!   the default value takes.
//! * `HKCU\Software\Classes\.<ext>\UserChoice`. This is where Windows records
//!   what the *user* picked. It is protected by a hash over the extension, the
//!   SID and a timestamp; writing it means forging that hash, Microsoft
//!   documents it as unsupported, and it is the single clearest example of
//!   what ADR-0012 exists to forbid.
//! * anything under `HKEY_LOCAL_MACHINE`, and anything belonging to another
//!   application -- `txtfile`, Notepad's own ProgID, most of all.
//!
//! ## Why nothing here writes to the registry
//!
//! Because it cannot without either a third-party dependency (`winreg`) or a
//! Win32 call, and this crate has neither a dependency nor `unsafe`. That is a
//! real limitation and it is reported as one -- see
//! [`Capability::WindowsFileTypeRegistration`](crate::capabilities::Capability::WindowsFileTypeRegistration).
//! What is here is the complete set of values plus [`registry_script`], which
//! renders them as a `.reg` file the user can inspect and apply themselves.
//! Given ADR-0012's insistence that the user keeps control, a file the user
//! reads and double-clicks is not obviously the worse answer.

use super::AppInfo;
use super::file_type::AssociationSelection;

/// The prefix of every ProgID this product owns.
///
/// A prefix rather than one flat ProgID because Windows shows the friendly
/// name *per ProgID*: one shared ProgID would label every file type "Text
/// document", including the notebooks and the encrypted documents.
pub const PROG_ID_PREFIX: &str = "BachelorPadPlus";

/// The key holding the application's declared capabilities.
pub const CAPABILITIES_KEY: &str = r"HKCU\Software\BachelorPad+\Capabilities";

/// The one shared key this registration touches, and it adds a single value to
/// it under the product's own name.
///
/// Windows discovers registered applications by enumerating the values here;
/// there is no per-application alternative. Adding one named value is the
/// documented mechanism, and it neither reads nor disturbs anybody else's.
pub const REGISTERED_APPLICATIONS_KEY: &str = r"HKCU\Software\RegisteredApplications";

/// Which value of a key is meant.
///
/// A registry key's unnamed "default" value and its named values are different
/// things with different meanings, and a `String` that is empty for one of them
/// is a bug waiting for the day somebody writes a value genuinely named `""`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueName {
    /// The key's unnamed value, written `@=` in a `.reg` file.
    Default,
    /// A named value.
    Named(String),
}

/// One registry value a registration would set.
///
/// Data as a value, rather than a call that sets it, because the whole
/// question "what would this do to my machine" then has an answer a user can
/// read before anything happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryValue {
    /// The full key path, starting `HKCU\`.
    pub key: String,
    /// Which value of it.
    pub name: ValueName,
    /// The string data. Every value this registration writes is a `REG_SZ`;
    /// nothing here needs a binary or numeric type, and keeping it to one type
    /// keeps the `.reg` rendering to one case.
    pub data: String,
}

/// Why a registry key must not be written.
///
/// Separate variants rather than a bool because they are separate promises,
/// and a refusal that cannot say which promise it is keeping is one nobody
/// can review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyObjection {
    /// Not under `HKEY_CURRENT_USER`. A per-user registration has no business
    /// anywhere else, and a machine-wide one would need elevation the product
    /// never asks for.
    OutsideTheUserHive,
    /// Inside another application's territory: a ProgID that is not ours, or a
    /// key under `Software` belonging to somebody else.
    AnotherApplication,
    /// The extension's default value, or its `UserChoice`. Both are ways of
    /// *taking* an association rather than offering one; ADR-0012 forbids
    /// both, and `UserChoice` additionally requires forging a hash.
    SeizesTheAssociation,
    /// Names Notepad. ADR-0012's sentence, made mechanical.
    TouchesNotepad,
}

impl KeyObjection {
    /// A sentence naming the promise, for a refusal a reviewer can check.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::OutsideTheUserHive => {
                "Only per-user registration is supported; this key is outside HKEY_CURRENT_USER."
            }
            Self::AnotherApplication => {
                "This key belongs to another application. BachelorPad+ registers only its own \
                 ProgIDs and adds itself to the Open With list."
            }
            Self::SeizesTheAssociation => {
                "This would take the file association rather than offer it. BachelorPad+ registers \
                 as a handler and leaves the choice to Windows (ADR-0012)."
            }
            Self::TouchesNotepad => {
                "BachelorPad+ never replaces, patches or redirects Notepad (ADR-0012)."
            }
        }
    }
}

/// Why `key` must not be written, if it must not.
///
/// The guard rather than the generator is the security boundary, because the
/// generator will grow: a preset, a file type, a new capability key. Passing
/// every value through one predicate means the next person to add a key gets a
/// failing test rather than a code review.
///
/// Comparison is case-insensitive throughout. Registry paths are, and a check
/// that is not is a check spelled around by changing one letter's case.
#[must_use]
pub fn key_objection(key: &str) -> Option<KeyObjection> {
    let lower = key.to_ascii_lowercase();

    if lower.contains("notepad") {
        return Some(KeyObjection::TouchesNotepad);
    }
    let Some(rest) = lower
        .strip_prefix(r"hkcu\")
        .or_else(|| lower.strip_prefix(r"hkey_current_user\"))
    else {
        return Some(KeyObjection::OutsideTheUserHive);
    };

    if let Some(under_classes) = rest.strip_prefix(r"software\classes\") {
        let mut parts = under_classes.split('\\');
        let first = parts.next().unwrap_or_default();
        if let Some(extension) = first.strip_prefix('.') {
            if extension.is_empty() {
                return Some(KeyObjection::AnotherApplication);
            }
            // Under an extension key, exactly one sub-key is ours to write.
            // The extension key *itself* is not: its default value is the
            // association, and `UserChoice` is the user's own record of it.
            return match parts.next() {
                Some("openwithprogids") => None,
                _ => Some(KeyObjection::SeizesTheAssociation),
            };
        }
        // Not an extension, so it is a ProgID. Only ours.
        let prefix = format!("{}.", PROG_ID_PREFIX.to_ascii_lowercase());
        return if first.starts_with(&prefix) {
            None
        } else {
            Some(KeyObjection::AnotherApplication)
        };
    }

    if rest == "software\\registeredapplications" {
        return None;
    }
    if rest.starts_with("software\\bachelorpad+") {
        return None;
    }
    Some(KeyObjection::AnotherApplication)
}

/// The ProgID for one extension.
#[must_use]
pub fn prog_id(extension: &str) -> String {
    format!("{PROG_ID_PREFIX}.{}", extension.trim_start_matches('.'))
}

/// Every registry value a registration for `selection` would set.
///
/// Every key in the result satisfies [`key_objection`] -- a test asserts it
/// over every preset -- so a caller that applies these without re-checking is
/// still safe, and a caller that re-checks (as an installer should) finds
/// nothing.
#[must_use]
pub fn registry_values(app: &AppInfo, selection: &AssociationSelection) -> Vec<RegistryValue> {
    let mut values = Vec::new();
    let command = format!("\"{}\" \"%1\"", app.executable);

    for file_type in selection.file_types() {
        let id = prog_id(file_type.extension);
        let key = format!(r"HKCU\Software\Classes\{id}");
        values.push(RegistryValue {
            key: key.clone(),
            name: ValueName::Default,
            data: format!("{} ({})", file_type.description, app.display_name),
        });
        values.push(RegistryValue {
            key: key.clone(),
            name: ValueName::Named("FriendlyTypeName".to_owned()),
            data: format!("{} ({})", file_type.description, app.display_name),
        });
        values.push(RegistryValue {
            key: format!(r"{key}\DefaultIcon"),
            name: ValueName::Default,
            // **The `.ico` beside the executable, not the executable itself**
            // (ADR-0068). This was `"<exe>",0` -- icon index 0, the program's
            // own first icon resource -- and the program has never had one,
            // so every type registered through here rendered as a blank page
            // in Explorer. Embedding one needs a build-time resource compiler
            // and therefore a new dependency; ADR-0067 removed the installer
            // that would otherwise have placed an icon, so the file ships in
            // the archive and is referenced where it lands.
            //
            // A per-type icon would need an icon per type, and a missing one
            // renders as that same blank page.
            data: format!("\"{}\",0", app.icon),
        });
        values.push(RegistryValue {
            key: format!(r"{key}\shell\open\command"),
            name: ValueName::Default,
            data: command.clone(),
        });
        // The additive half: offer, never take. There is deliberately no
        // value written for the extension key's own default.
        values.push(RegistryValue {
            key: format!(
                r"HKCU\Software\Classes\{}\OpenWithProgids",
                file_type.dotted()
            ),
            name: ValueName::Named(id),
            // Windows wants this value present and empty; the value's *name*
            // is the ProgID and its data is ignored.
            data: String::new(),
        });
    }

    values.push(RegistryValue {
        key: CAPABILITIES_KEY.to_owned(),
        name: ValueName::Named("ApplicationName".to_owned()),
        data: app.display_name.clone(),
    });
    values.push(RegistryValue {
        key: CAPABILITIES_KEY.to_owned(),
        name: ValueName::Named("ApplicationDescription".to_owned()),
        data: app.description.clone(),
    });
    for file_type in selection.file_types() {
        values.push(RegistryValue {
            key: format!(r"{CAPABILITIES_KEY}\FileAssociations"),
            name: ValueName::Named(file_type.dotted()),
            data: prog_id(file_type.extension),
        });
    }
    values.push(RegistryValue {
        key: REGISTERED_APPLICATIONS_KEY.to_owned(),
        name: ValueName::Named(app.display_name.clone()),
        data: r"Software\BachelorPad+\Capabilities".to_owned(),
    });

    values
}

/// Any of `values` whose key this crate refuses to write.
///
/// Exists so the mutating step can re-check rather than trust the plan it was
/// handed. A plan is a value and values travel: through a config file, across
/// a version, into a test fixture. The check is cheap and the promise it keeps
/// is ADR-0012's.
#[must_use]
pub fn objections(values: &[RegistryValue]) -> Vec<(String, KeyObjection)> {
    values
        .iter()
        .filter_map(|value| key_objection(&value.key).map(|why| (value.key.clone(), why)))
        .collect()
}

/// `values` rendered as a `.reg` file.
///
/// The artefact that makes the Windows half usable despite nothing here being
/// able to write a registry key: the user can read every line of it before
/// anything happens, which is more than an installer that writes the same keys
/// silently offers them.
///
/// CRLF line endings and the `Windows Registry Editor Version 5.00` header are
/// both required -- `regedit` refuses a file without the header, and treats a
/// UTF-8 file with LF endings as suspect.
#[must_use]
pub fn registry_script(values: &[RegistryValue]) -> String {
    let mut out = String::from("Windows Registry Editor Version 5.00\r\n");
    let mut current: Option<&str> = None;
    for value in values {
        if current != Some(value.key.as_str()) {
            out.push_str("\r\n[");
            out.push_str(&full_hive(&value.key));
            out.push_str("]\r\n");
            current = Some(&value.key);
        }
        match &value.name {
            ValueName::Default => out.push('@'),
            ValueName::Named(name) => {
                out.push('"');
                out.push_str(&escape_reg(name));
                out.push('"');
            }
        }
        out.push_str("=\"");
        out.push_str(&escape_reg(&value.data));
        out.push_str("\"\r\n");
    }
    out
}

/// `HKCU\` expanded, because `.reg` files require the long hive name.
fn full_hive(key: &str) -> String {
    match key.strip_prefix(r"HKCU\") {
        Some(rest) => format!(r"HKEY_CURRENT_USER\{rest}"),
        None => key.to_owned(),
    }
}

/// A string escaped for a `.reg` file's quoted value syntax.
///
/// Only two characters matter, and both matter a great deal: an unescaped
/// backslash in a path turns `C:\temp` into `C:<tab>emp` in some readers and
/// an unescaped quote ends the value early, which is how the rest of a
/// program's path becomes a new key.
fn escape_reg(text: &str) -> String {
    text.replace('\\', r"\\").replace('"', "\\\"")
}
