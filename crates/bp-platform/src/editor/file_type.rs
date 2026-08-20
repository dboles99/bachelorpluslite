//! Which file types the product would register for, and how each platform
//! names them.
//!
//! The table here is not the same knowledge as `bp-formats`, which is why it
//! is not derived from it. `bp-formats` answers "how do I read this"; this
//! answers "what does the operating system call it" -- a MIME type Linux keys
//! its associations on, and a description Windows shows in the Open With
//! list. A format could gain a parser without anyone wanting the product
//! registered as its handler, and a type could be worth registering that has
//! no parser at all.
//!
//! The two are still required to agree in one direction, and a test enforces
//! it: **every extension offered for registration must be one `bp-formats`
//! can identify.** Claiming a file type the editor cannot open is the one
//! failure mode of registration that the user experiences as a broken
//! machine rather than a missing feature.
//!
//! ## About the MIME types
//!
//! Where `shared-mime-info` has a type, that is the one used, because
//! `mimeapps.list` keys on exactly those strings and a near-miss silently
//! associates nothing. Where it has none -- PowerShell, and the product's own
//! `.bpadx` -- an `x-` type is defined here and shipped in a MIME package (see
//! [`super::desktop::mime_package`]), which is the standards-based way to add
//! one. Getting a MIME name wrong costs nothing on Windows, which associates
//! by extension and never looks at these.

use std::collections::BTreeSet;

/// What kind of thing a file type is, from the point of view of somebody
/// choosing what to associate.
///
/// Coarser than `bp_formats::Profile` on purpose. A user picking file
/// associations is answering "do I want this editor for my spreadsheets", not
/// "is TSV tabular"; a preset built from twenty individual checkboxes is a
/// preset nobody uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeGroup {
    /// The product's own formats. In every preset, because nothing else opens
    /// them and leaving them out means a `.bpadx` the user cannot double-click.
    Own,
    /// What Windows Notepad is the default for: `.txt`, `.log`, `.ini`.
    PlainText,
    /// Markdown.
    Notes,
    /// JSON, YAML, TOML, XML and friends.
    StructuredData,
    /// CSV and TSV. Separate from structured data because most people want a
    /// spreadsheet for these and a text editor for the rest, and a preset that
    /// cannot express that difference is a preset that gets refused whole.
    Tabular,
    /// Source code and markup.
    Code,
    /// Jupyter notebooks.
    Notebook,
}

/// One registerable file type.
///
/// `&'static` throughout because the set is fixed at compile time: a
/// registration table that could be extended at run time is one that could
/// claim a type the editor has no parser for, which is the failure this
/// module is most careful about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileType {
    /// The extension, lowercase and without a leading dot. Stored bare
    /// because Windows wants `.txt` and Linux wants `*.txt`, and a stored
    /// leading dot means one of the two is always doing string surgery.
    pub extension: &'static str,
    /// The MIME type Linux associates by.
    pub mime: &'static str,
    /// What the user sees in Open With, and in the `.desktop` comment.
    pub description: &'static str,
    /// Which preset this type belongs to.
    pub group: TypeGroup,
}

impl FileType {
    /// The extension with its dot, as Windows registry keys spell it.
    #[must_use]
    pub fn dotted(&self) -> String {
        format!(".{}", self.extension)
    }

    /// The glob a MIME package uses.
    #[must_use]
    pub fn glob(&self) -> String {
        format!("*.{}", self.extension)
    }

    /// Whether this type is the product's own invention rather than one the
    /// shared MIME database already knows.
    ///
    /// The dividing line for [`super::desktop::mime_package`]: adding a glob
    /// for a type the distribution already defines is at best redundant and at
    /// worst a fight with the packager over what `.json` means.
    #[must_use]
    pub fn is_own_mime_type(&self) -> bool {
        self.mime.starts_with("application/x-bachelorpad") || self.mime == "text/x-powershell"
    }
}

/// Every file type the product offers to register for.
///
/// Ordered by group and then alphabetically, so a generated artefact is
/// byte-stable across runs -- a `.desktop` file whose `MimeType=` line
/// reorders itself is a file that shows up in every diff and every backup.
pub const FILE_TYPES: &[FileType] = &[
    // --- the product's own ------------------------------------------------
    FileType {
        extension: "bpadx",
        mime: "application/x-bachelorpad-encrypted",
        description: "BachelorPad+ encrypted document",
        group: TypeGroup::Own,
    },
    // --- plain text: what Notepad is the default for ----------------------
    FileType {
        extension: "cfg",
        mime: "text/plain",
        description: "Configuration file",
        group: TypeGroup::PlainText,
    },
    FileType {
        extension: "conf",
        mime: "text/plain",
        description: "Configuration file",
        group: TypeGroup::PlainText,
    },
    FileType {
        extension: "ini",
        mime: "text/plain",
        description: "Configuration settings",
        group: TypeGroup::PlainText,
    },
    FileType {
        extension: "log",
        mime: "text/x-log",
        description: "Log file",
        group: TypeGroup::PlainText,
    },
    FileType {
        extension: "text",
        mime: "text/plain",
        description: "Text document",
        group: TypeGroup::PlainText,
    },
    FileType {
        extension: "txt",
        mime: "text/plain",
        description: "Text document",
        group: TypeGroup::PlainText,
    },
    // --- notes ------------------------------------------------------------
    FileType {
        extension: "markdown",
        mime: "text/markdown",
        description: "Markdown document",
        group: TypeGroup::Notes,
    },
    FileType {
        extension: "md",
        mime: "text/markdown",
        description: "Markdown document",
        group: TypeGroup::Notes,
    },
    // --- structured data --------------------------------------------------
    FileType {
        extension: "json",
        mime: "application/json",
        description: "JSON document",
        group: TypeGroup::StructuredData,
    },
    FileType {
        extension: "jsonl",
        mime: "application/x-ndjson",
        description: "JSON Lines document",
        group: TypeGroup::StructuredData,
    },
    FileType {
        extension: "ndjson",
        mime: "application/x-ndjson",
        description: "JSON Lines document",
        group: TypeGroup::StructuredData,
    },
    FileType {
        extension: "toml",
        mime: "application/toml",
        description: "TOML document",
        group: TypeGroup::StructuredData,
    },
    FileType {
        extension: "xml",
        mime: "application/xml",
        description: "XML document",
        group: TypeGroup::StructuredData,
    },
    FileType {
        extension: "yaml",
        mime: "application/yaml",
        description: "YAML document",
        group: TypeGroup::StructuredData,
    },
    FileType {
        extension: "yml",
        mime: "application/yaml",
        description: "YAML document",
        group: TypeGroup::StructuredData,
    },
    // --- tabular ----------------------------------------------------------
    FileType {
        extension: "csv",
        mime: "text/csv",
        description: "Comma-separated values",
        group: TypeGroup::Tabular,
    },
    FileType {
        extension: "tsv",
        mime: "text/tab-separated-values",
        description: "Tab-separated values",
        group: TypeGroup::Tabular,
    },
    // --- code and markup --------------------------------------------------
    FileType {
        extension: "cjs",
        mime: "text/javascript",
        description: "JavaScript source",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "css",
        mime: "text/css",
        description: "Style sheet",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "htm",
        mime: "text/html",
        description: "HTML document",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "html",
        mime: "text/html",
        description: "HTML document",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "js",
        mime: "text/javascript",
        description: "JavaScript source",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "mjs",
        mime: "text/javascript",
        description: "JavaScript source",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "ps1",
        mime: "text/x-powershell",
        description: "PowerShell script",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "psd1",
        mime: "text/x-powershell",
        description: "PowerShell data file",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "psm1",
        mime: "text/x-powershell",
        description: "PowerShell module",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "py",
        mime: "text/x-python",
        description: "Python source",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "rs",
        mime: "text/rust",
        description: "Rust source",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "sh",
        mime: "application/x-shellscript",
        description: "Shell script",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "sql",
        mime: "application/sql",
        description: "SQL script",
        group: TypeGroup::Code,
    },
    FileType {
        extension: "ts",
        mime: "text/x-typescript",
        description: "TypeScript source",
        group: TypeGroup::Code,
    },
    // --- notebooks --------------------------------------------------------
    FileType {
        extension: "ipynb",
        mime: "application/x-ipynb+json",
        description: "Jupyter notebook",
        group: TypeGroup::Notebook,
    },
];

/// The registerable type for `extension`, if there is one.
///
/// Accepts `txt`, `.txt` and `.TXT` alike, because the string arrives from a
/// command line, a config file or a file dialog and normalising at every call
/// site is how one of them ends up not normalising.
#[must_use]
pub fn file_type(extension: &str) -> Option<&'static FileType> {
    let wanted = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    FILE_TYPES.iter().find(|t| t.extension == wanted)
}

/// The association presets specs.md section 19 names.
///
/// A preset is a named set of file types and nothing more -- ADR-0012 is
/// explicit that they are "a UI convenience over the supported mechanism, not
/// a bypass of it". Choosing one still produces the same artefacts and still
/// ends at the operating system's own default-app selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssociationPreset {
    /// The types Windows Notepad is the default for. The smallest useful
    /// answer, and the one that surprises nobody.
    NotepadReplacement,
    /// ...and Markdown.
    TextAndNotes,
    /// ...and JSON, YAML, TOML, XML, CSV and TSV.
    TextAndStructuredData,
    /// Everything except the tabular formats, which most people would rather
    /// open in a spreadsheet. Deliberately not a superset of
    /// [`Self::TextAndStructuredData`]: a developer who wants `.rs` and `.py`
    /// usually does *not* want to have taken `.csv` away from their
    /// spreadsheet, and a preset list that only escalates cannot say that.
    Developer,
    /// Every type in [`FILE_TYPES`]. Includes types the user may well prefer
    /// another application for -- `.html` belongs to a browser for most people
    /// -- which is safe precisely because registration only *offers*; the
    /// operating system's own default-app UI still decides.
    EverythingSupported,
}

impl AssociationPreset {
    /// Every preset, in the order specs.md lists them.
    pub const ALL: &'static [AssociationPreset] = &[
        AssociationPreset::NotepadReplacement,
        AssociationPreset::TextAndNotes,
        AssociationPreset::TextAndStructuredData,
        AssociationPreset::Developer,
        AssociationPreset::EverythingSupported,
    ];

    /// The label for the UI.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotepadReplacement => "Notepad Replacement",
            Self::TextAndNotes => "Text + Notes",
            Self::TextAndStructuredData => "Text + Structured Data",
            Self::Developer => "Developer",
            Self::EverythingSupported => "Everything Supported",
        }
    }

    /// A stable token for a config file or a command line.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::NotepadReplacement => "notepad-replacement",
            Self::TextAndNotes => "text-and-notes",
            Self::TextAndStructuredData => "text-and-structured-data",
            Self::Developer => "developer",
            Self::EverythingSupported => "everything-supported",
        }
    }

    /// Read back what [`Self::token`] wrote.
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        let wanted = token.trim().to_ascii_lowercase();
        Self::ALL.iter().copied().find(|p| p.token() == wanted)
    }

    /// The groups this preset covers.
    #[must_use]
    pub fn groups(self) -> &'static [TypeGroup] {
        use TypeGroup::{Code, Notebook, Notes, Own, PlainText, StructuredData, Tabular};
        match self {
            Self::NotepadReplacement => &[Own, PlainText],
            Self::TextAndNotes => &[Own, PlainText, Notes],
            Self::TextAndStructuredData => &[Own, PlainText, Notes, StructuredData, Tabular],
            Self::Developer => &[Own, PlainText, Notes, StructuredData, Code, Notebook],
            Self::EverythingSupported => &[
                Own,
                PlainText,
                Notes,
                StructuredData,
                Tabular,
                Code,
                Notebook,
            ],
        }
    }
}

/// The set of file types a registration is for.
///
/// A set rather than a preset, so that "Custom" from specs.md section 19 needs
/// no special case anywhere downstream: a custom choice is an
/// [`AssociationSelection`] built from extensions, a preset is one built from
/// a name, and everything that consumes a selection sees the same thing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssociationSelection {
    extensions: BTreeSet<&'static str>,
}

impl AssociationSelection {
    /// The types a named preset covers.
    #[must_use]
    pub fn preset(preset: AssociationPreset) -> Self {
        let groups = preset.groups();
        Self {
            extensions: FILE_TYPES
                .iter()
                .filter(|t| groups.contains(&t.group))
                .map(|t| t.extension)
                .collect(),
        }
    }

    /// An arbitrary set, for specs.md's "Custom".
    ///
    /// Extensions this crate does not know are **dropped, not rejected**. The
    /// rule behind that is the one thing registration must never get wrong: we
    /// may only claim types the editor can actually open, so an unrecognised
    /// entry cannot be honoured whatever the caller intended. Use
    /// [`unknown_extensions`] first to tell the user which ones went, rather
    /// than discovering it from a shorter list.
    pub fn custom<'a>(extensions: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            extensions: extensions
                .into_iter()
                .filter_map(|e| file_type(e).map(|t| t.extension))
                .collect(),
        }
    }

    /// The extensions, without dots, in a stable order.
    pub fn extensions(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.extensions.iter().copied()
    }

    /// The file types, in [`FILE_TYPES`] order rather than alphabetical, so
    /// generated artefacts group related types together.
    pub fn file_types(&self) -> impl Iterator<Item = &'static FileType> + '_ {
        FILE_TYPES
            .iter()
            .filter(|t| self.extensions.contains(t.extension))
    }

    /// The distinct MIME types, deduplicated and in a stable order.
    ///
    /// Deduplicated because six extensions map to `text/plain` and a
    /// `MimeType=` line that repeats it six times is a line that fails
    /// `desktop-file-validate`.
    #[must_use]
    pub fn mime_types(&self) -> Vec<&'static str> {
        let mut seen = BTreeSet::new();
        self.file_types()
            .map(|t| t.mime)
            .filter(|mime| seen.insert(*mime))
            .collect()
    }

    /// Whether `extension` is in the selection. Tolerates a leading dot and
    /// any casing, like [`file_type`].
    #[must_use]
    pub fn contains(&self, extension: &str) -> bool {
        file_type(extension).is_some_and(|t| self.extensions.contains(t.extension))
    }

    /// Whether nothing is selected. Worth asking before building a plan: a
    /// registration for no file types is an artefact that claims nothing, and
    /// writing one is a confusing no-op rather than an error.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
    }

    /// How many types are selected.
    #[must_use]
    pub fn len(&self) -> usize {
        self.extensions.len()
    }

    /// The preset this selection happens to be, if it is one.
    ///
    /// So a settings screen can show "Developer" rather than "Custom" for a
    /// selection that was restored from a config file, without the config file
    /// having to store which preset it came from -- a stored name and a stored
    /// list can disagree, and then one of them is a lie.
    #[must_use]
    pub fn matching_preset(&self) -> Option<AssociationPreset> {
        AssociationPreset::ALL
            .iter()
            .copied()
            .find(|&preset| Self::preset(preset) == *self)
    }
}

/// The entries of `extensions` this crate cannot register for.
///
/// Returned separately from [`AssociationSelection::custom`] so a caller can
/// say "`.docx` is not a text format" instead of silently registering for one
/// fewer type than the user asked for.
#[must_use]
pub fn unknown_extensions<'a>(extensions: &[&'a str]) -> Vec<&'a str> {
    extensions
        .iter()
        .copied()
        .filter(|e| file_type(e).is_none())
        .collect()
}
