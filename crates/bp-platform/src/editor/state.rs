//! Reporting which application currently opens each file type.
//!
//! The half of default-editor integration that changes nothing, and therefore
//! the half a settings screen can call freely: "are we the default for `.txt`?
//! for `.md`?" is a question, and answering it needs no consent, no elevation
//! and no undo.
//!
//! Every function here takes the *already-read* state as a value -- the text
//! of a `mimeapps.list`, or a map of extension to ProgID -- rather than
//! reading it. Two reasons, and the second is the one that matters: the
//! parsing is then testable without a home directory, and the Windows shape
//! can be tested on Linux even though nothing here can read a Windows
//! registry.
//!
//! ## The honest gap
//!
//! On Windows this crate cannot read the current association at all, because
//! it lives in `HKCU\Software\Classes\<ext>\UserChoice` and reading the
//! registry needs a dependency this crate does not take. That is reported as
//! [`AssociationState::Unknown`] with the reason attached, never as
//! [`AssociationState::Unclaimed`] -- a UI that renders "not registered" when
//! it means "could not look" invites the user to fix something that is not
//! broken.

use std::collections::BTreeMap;

use super::AppInfo;
use super::file_type::AssociationSelection;

/// The section of a `mimeapps.list` that decides the default handler.
///
/// `[Added Associations]` is deliberately not consulted: it lists applications
/// that *can* open a type, which is the question "is it in the Open With
/// list", not "is it the default". Conflating them makes a product that has
/// merely registered report itself as the default.
const DEFAULT_APPLICATIONS: &str = "Default Applications";

/// Who opens a file type at the moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssociationState {
    /// This product.
    Ours,
    /// Another application. Named by whatever identifier the platform uses --
    /// a `.desktop` file name on Linux, a ProgID on Windows -- rather than a
    /// pretty name, because resolving one to the other is another lookup and
    /// an unresolvable one would have to be reported as unknown anyway.
    Other { handler: String },
    /// Nothing claims it.
    Unclaimed,
    /// The state could not be read. Distinct from [`Self::Unclaimed`], and the
    /// reason says which obstacle it hit.
    Unknown { reason: &'static str },
}

impl AssociationState {
    /// A sentence for a settings row.
    #[must_use]
    pub fn describe(&self, extension: &str) -> String {
        match self {
            Self::Ours => format!("{} opens .{extension} files.", crate::DISPLAY_NAME),
            Self::Other { handler } => format!("{handler} opens .{extension} files."),
            Self::Unclaimed => format!("Nothing is set to open .{extension} files."),
            Self::Unknown { reason } => {
                format!("Cannot tell what opens .{extension} files. {reason}")
            }
        }
    }
}

/// One row of an [`AssociationReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationEntry {
    /// The extension, without a dot.
    pub extension: &'static str,
    /// The MIME type, which is what the state was actually looked up by on
    /// Linux. Carried so a diagnostic can show why a lookup missed -- an
    /// association against `text/x-markdown` when the table says
    /// `text/markdown` is invisible otherwise.
    pub mime: &'static str,
    /// Who opens it.
    pub state: AssociationState,
}

/// What currently opens each of the selected file types.
///
/// Ordered like the selection, so two reports taken before and after a change
/// line up row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationReport {
    entries: Vec<AssociationEntry>,
}

impl AssociationReport {
    /// The rows.
    #[must_use]
    pub fn entries(&self) -> &[AssociationEntry] {
        &self.entries
    }

    /// The state of one extension, if it was in the selection.
    #[must_use]
    pub fn state_of(&self, extension: &str) -> Option<&AssociationState> {
        let wanted = extension
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase();
        self.entries
            .iter()
            .find(|entry| entry.extension == wanted)
            .map(|entry| &entry.state)
    }

    /// Whether this product opens `extension`.
    ///
    /// False for both "somebody else does" and "cannot tell", which is the
    /// right default for anything that acts on the answer: a caller offering
    /// to register should offer when the answer is unknown.
    #[must_use]
    pub fn is_ours(&self, extension: &str) -> bool {
        matches!(self.state_of(extension), Some(AssociationState::Ours))
    }

    /// How many of the selected types this product opens.
    #[must_use]
    pub fn ours(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.state == AssociationState::Ours)
            .count()
    }

    /// A one-line summary for a settings screen.
    ///
    /// Says "of the types you chose", not "of all types", because a user who
    /// picked Notepad Replacement and sees "6 of 34" will reasonably think
    /// something failed.
    #[must_use]
    pub fn summary(&self) -> String {
        let total = self.entries.len();
        if total == 0 {
            return "No file types are selected.".to_owned();
        }
        let unknown = self
            .entries
            .iter()
            .filter(|entry| matches!(entry.state, AssociationState::Unknown { .. }))
            .count();
        if unknown == total {
            return format!("Cannot tell which application opens the {total} selected file types.");
        }
        format!(
            "{} opens {} of the {total} selected file types.",
            crate::DISPLAY_NAME,
            self.ours()
        )
    }
}

/// The report a Linux `mimeapps.list` implies.
///
/// `mimeapps` is the file's contents. An absent file is an empty string and
/// yields [`AssociationState::Unclaimed`] throughout, which is correct: a user
/// who has never chosen a handler has none recorded.
#[must_use]
pub fn report_from_mimeapps(
    mimeapps: &str,
    app: &AppInfo,
    selection: &AssociationSelection,
) -> AssociationReport {
    let defaults = default_applications(mimeapps);
    let ours = super::desktop::desktop_file_name(app);
    AssociationReport {
        entries: selection
            .file_types()
            .map(|file_type| AssociationEntry {
                extension: file_type.extension,
                mime: file_type.mime,
                state: match defaults.get(file_type.mime).and_then(|v| v.first()) {
                    None => AssociationState::Unclaimed,
                    Some(handler) if *handler == ours => AssociationState::Ours,
                    Some(handler) => AssociationState::Other {
                        handler: handler.clone(),
                    },
                },
            })
            .collect(),
    }
}

/// The report a map of extension to ProgID implies.
///
/// The Windows shape, taking the values a caller read rather than reading
/// them, so the logic is exercised by the Linux leg of CI too. Keys may be
/// given with or without a leading dot and in any case.
///
/// Takes no [`AppInfo`], unlike its Linux counterpart, and the asymmetry is
/// the platforms' rather than an oversight: on Linux the product is identified
/// by the name of a file it installs, which only the caller knows, while on
/// Windows it is identified by a ProgID this crate owns outright.
#[must_use]
pub fn report_from_prog_ids(
    prog_ids: &BTreeMap<String, String>,
    selection: &AssociationSelection,
) -> AssociationReport {
    let normalised: BTreeMap<String, &str> = prog_ids
        .iter()
        .map(|(key, value)| {
            (
                key.trim().trim_start_matches('.').to_ascii_lowercase(),
                value.as_str(),
            )
        })
        .collect();
    AssociationReport {
        entries: selection
            .file_types()
            .map(|file_type| AssociationEntry {
                extension: file_type.extension,
                mime: file_type.mime,
                state: match normalised.get(file_type.extension) {
                    None => AssociationState::Unclaimed,
                    Some(id)
                        if super::windows::prog_id(file_type.extension)
                            .eq_ignore_ascii_case(id) =>
                    {
                        AssociationState::Ours
                    }
                    Some(id) => AssociationState::Other {
                        handler: (*id).to_owned(),
                    },
                },
            })
            .collect(),
    }
}

/// A report that says, for every selected type, that the state could not be
/// read.
///
/// The honest answer on Windows as this crate stands. A function rather than
/// an ad-hoc construction at the call site so that the shape of "we do not
/// know" is identical everywhere and a UI can special-case it once.
#[must_use]
pub fn unknown_report(reason: &'static str, selection: &AssociationSelection) -> AssociationReport {
    AssociationReport {
        entries: selection
            .file_types()
            .map(|file_type| AssociationEntry {
                extension: file_type.extension,
                mime: file_type.mime,
                state: AssociationState::Unknown { reason },
            })
            .collect(),
    }
}

/// The `[Default Applications]` section of a `mimeapps.list`, as a map from
/// MIME type to the desktop files listed for it, in order.
///
/// Public because it is the useful half on its own -- a diagnostic wants to
/// print what the file says, not just what it implies about us.
///
/// The format is the desktop-entry one, and the tolerances here match what
/// real files contain: comment lines beginning `#`, blank lines, whitespace
/// around `=`, a trailing `;` on the list, and a first-wins rule for a key
/// that appears twice, which is what the desktop-entry specification says
/// about duplicate keys.
#[must_use]
pub fn default_applications(mimeapps: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut in_section = false;
    for line in mimeapps.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_section = name.trim() == DEFAULT_APPLICATIONS;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let handlers: Vec<String> = value
            .split(';')
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .map(str::to_owned)
            .collect();
        if handlers.is_empty() {
            continue;
        }
        out.entry(key.trim().to_owned()).or_insert(handlers);
    }
    out
}
