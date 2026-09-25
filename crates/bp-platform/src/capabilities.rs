//! What a caller may rely on, per platform, and why not when not.
//!
//! `docs/architecture/PLATFORM_MATRIX.md` is a table in a document, which is
//! the right place for a human to read it and the wrong place for code to
//! consult it. This module is the same table as values, so the shell can grey
//! out a menu item, a diagnostic can print the row, and a test can assert that
//! the answer matches what the rest of this crate actually does.
//!
//! ## Three distinctions worth keeping apart
//!
//! A UI that shows one "not available" makes three different situations look
//! identical, and they call for three different responses from the user:
//!
//! * [`Availability::Unsupported`] -- the platform cannot. Nothing the user or
//!   we can do; the honest UI removes the option rather than disabling it.
//! * [`Availability::Conditional`] -- the platform can, this machine might
//!   not, and only a run-time probe can tell. The user may be able to fix it.
//! * [`Availability::NotImplemented`] and [`Availability::AwaitingDecision`]
//!   -- the platform can and we do not. The first is work; the second is
//!   blocked on a human, and conflating the two is how a blocked decision
//!   quietly becomes a decided one.
//!
//! ## Availability is about this build, not about the operating system
//!
//! "Windows can write registry keys" is true and useless. What a caller needs
//! is whether *this product, as built,* will do it, and the answer is
//! frequently no for a reason worth stating -- `#![forbid(unsafe_code)]`, or a
//! dependency nobody has approved. Those reasons are in the strings.

use crate::Platform;

/// Whether a capability can be used, and if not, what kind of "not".
///
/// The reason is `&'static str` rather than a formatted `String` because these
/// are fixed facts about a platform and a build, not about a run: a reason
/// that needed run-time data would be a diagnostic, and belongs with whatever
/// probe produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Usable now, on every machine running this platform.
    Available,

    /// The platform supports it; whether this machine does cannot be known
    /// without asking the machine. The desktop-environment answers on Linux
    /// live here, and so does anything Windows gates on a per-machine setting.
    Conditional { reason: &'static str },

    /// The platform supports it and this build does not do it. Work, not a
    /// question -- the reason names the obstacle.
    NotImplemented { reason: &'static str },

    /// Deliberately not built, because building it would settle a question a
    /// human has not answered. The reason names the question.
    AwaitingDecision { reason: &'static str },

    /// The platform does not have it at all.
    Unsupported { reason: &'static str },
}

impl Availability {
    /// Whether a caller may go ahead without asking anything else.
    ///
    /// [`Self::Conditional`] is deliberately false. A capability that might be
    /// there is not one to build a code path on top of unconditionally; the
    /// caller either probes or offers the user a fallback, and treating "maybe"
    /// as "yes" is how a feature works on the maintainer's desktop and nowhere
    /// else.
    #[must_use]
    pub const fn is_usable(self) -> bool {
        matches!(self, Self::Available)
    }

    /// Whether a human has to decide something before this can move.
    ///
    /// Separated from [`Self::NotImplemented`] so a roadmap can tell the two
    /// apart without reading prose: one is a backlog item, the other is a
    /// question with somebody's name on it.
    #[must_use]
    pub const fn is_blocked_on_a_decision(self) -> bool {
        matches!(self, Self::AwaitingDecision { .. })
    }

    /// The reason, where there is one.
    #[must_use]
    pub const fn reason(self) -> Option<&'static str> {
        match self {
            Self::Available => None,
            Self::Conditional { reason }
            | Self::NotImplemented { reason }
            | Self::AwaitingDecision { reason }
            | Self::Unsupported { reason } => Some(reason),
        }
    }
}

/// Something the product might do that depends on the operating system.
///
/// The list is closed on purpose. A capability register that callers can
/// extend is a register nobody can enumerate, and enumerating it -- for a
/// diagnostics panel, for a test that every entry has a reason -- is most of
/// what it is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// Install a `.desktop` entry so the product appears in the application
    /// menu and in "Open With".
    DesktopEntry,

    /// Claim MIME types through the shared MIME database and `mimeapps.list`.
    MimeAssociation,

    /// Register file types and application capabilities under
    /// `HKCU\Software\Classes`, so the product appears in Open With and in
    /// Default Apps.
    WindowsFileTypeRegistration,

    /// Send the user to the operating system's own default-application UI.
    /// The supported route ADR-0012 requires, as opposed to setting the
    /// default ourselves.
    DefaultAppHandoff,

    /// Report which application currently opens a given file type.
    AssociationQuery,

    /// Actually install the registration artefacts where the desktop reads
    /// them. The only capability here that changes the user's machine.
    AssociationInstall,

    /// Store a secret in the platform's credential vault -- DPAPI on Windows,
    /// Secret Service on Linux.
    CredentialStore,

    /// Gate an action behind the platform's biometric or PIN prompt.
    BiometricUnlock,

    /// Narrow a file so that only the owning account can read it.
    FilePermissionNarrowing,

    /// Work with paths longer than the platform's default limit.
    ExtendedLengthPaths,

    /// Treat two names differing only in case as two files.
    CaseSensitivePaths,

    /// Read the operating system's own clipboard history rather than the
    /// product's.
    NativeClipboardHistory,
}

impl Capability {
    /// Every capability, so a diagnostic or a test covers the register without
    /// a second list to keep in step.
    pub const ALL: &'static [Capability] = &[
        Capability::DesktopEntry,
        Capability::MimeAssociation,
        Capability::WindowsFileTypeRegistration,
        Capability::DefaultAppHandoff,
        Capability::AssociationQuery,
        Capability::AssociationInstall,
        Capability::CredentialStore,
        Capability::BiometricUnlock,
        Capability::FilePermissionNarrowing,
        Capability::ExtendedLengthPaths,
        Capability::CaseSensitivePaths,
        Capability::NativeClipboardHistory,
    ];

    /// A stable token, for a `--capabilities` dump and for a bug report.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::DesktopEntry => "desktop-entry",
            Self::MimeAssociation => "mime-association",
            Self::WindowsFileTypeRegistration => "windows-file-type-registration",
            Self::DefaultAppHandoff => "default-app-handoff",
            Self::AssociationQuery => "association-query",
            Self::AssociationInstall => "association-install",
            Self::CredentialStore => "credential-store",
            Self::BiometricUnlock => "biometric-unlock",
            Self::FilePermissionNarrowing => "file-permission-narrowing",
            Self::ExtendedLengthPaths => "extended-length-paths",
            Self::CaseSensitivePaths => "case-sensitive-paths",
            Self::NativeClipboardHistory => "native-clipboard-history",
        }
    }

    /// Whether `platform` offers this, and what kind of "no" it is otherwise.
    ///
    /// One exhaustive match over both axes, which is the point: adding a
    /// platform or a capability fails to compile until somebody has an answer
    /// for every pair, and no pair can be silently absent.
    #[must_use]
    pub const fn availability(self, platform: Platform) -> Availability {
        use Availability::{Available, AwaitingDecision, Conditional, NotImplemented, Unsupported};
        match (self, platform) {
            // --- registration artefacts ------------------------------------
            (Self::DesktopEntry, Platform::Linux) | (Self::MimeAssociation, Platform::Linux) => {
                Available
            }
            (Self::DesktopEntry, Platform::Windows) => Unsupported {
                reason: "Windows has no `.desktop` entry; file types are registered under \
                         HKCU\\Software\\Classes instead.",
            },
            (Self::MimeAssociation, Platform::Windows) => Unsupported {
                reason: "Windows associates by file extension, not by MIME type.",
            },
            (Self::WindowsFileTypeRegistration, Platform::Windows) => NotImplemented {
                reason: "The registry values are generated and can be written to a `.reg` file, \
                         but nothing here applies them: writing to the registry needs either the \
                         `winreg` dependency or a Win32 call, and this crate has neither a \
                         third-party dependency nor `unsafe`.",
            },
            (Self::WindowsFileTypeRegistration, Platform::Linux) => Unsupported {
                reason: "There is no registry on Linux.",
            },
            (Self::DefaultAppHandoff, Platform::Windows) => Available,
            (Self::DefaultAppHandoff, Platform::Linux) => Unsupported {
                reason: "Linux has no single default-apps settings page to hand the user to; the \
                         association is set directly through `xdg-mime`.",
            },

            // --- reading and changing the association ----------------------
            (Self::AssociationQuery, Platform::Linux) => Available,
            (Self::AssociationQuery, Platform::Windows) => NotImplemented {
                reason: "The current handler lives in HKCU\\Software\\Classes\\<ext>\\UserChoice, \
                         and reading the registry needs a dependency this crate does not take. \
                         The report is produced from values a caller supplies instead.",
            },
            (Self::AssociationInstall, Platform::Linux) => Available,
            (Self::AssociationInstall, Platform::Windows) => NotImplemented {
                reason: "Same obstacle as the registration itself: the values are generated, \
                         applying them is not.",
            },

            // --- secrets: blocked on a human -------------------------------
            (Self::CredentialStore, _) => AwaitingDecision {
                reason: "DPAPI and Secret Service are both in specs.md. They were blocked on \
                         where a signing key should live; ADR-0064 removed signing, so there \
                         is now no secret for a platform vault to hold. This stays \
                         AwaitingDecision rather than becoming Unsupported, because the open \
                         question is what would *want* one.",
            },
            (Self::BiometricUnlock, Platform::Windows) => AwaitingDecision {
                reason: "Windows Hello only protects something once there is somewhere for that \
                         something to live; it waits on the same open question as the credential \
                         store. It is additionally per-machine -- Windows 10 may have no \
                         enrolled biometric at all.",
            },
            (Self::BiometricUnlock, Platform::Linux) => Unsupported {
                reason: "There is no desktop-wide biometric prompt on Linux to call.",
            },

            // --- filesystem behaviour --------------------------------------
            (Self::FilePermissionNarrowing, Platform::Linux) => Available,
            (Self::FilePermissionNarrowing, Platform::Windows) => Unsupported {
                reason: "`std::fs::Permissions` on Windows exposes one bit, the read-only \
                         attribute, which controls writing rather than reading. Narrowing the \
                         access control list properly is a Win32 call, and this crate forbids \
                         `unsafe`. A file's permissions are whatever its directory passed down.",
            },
            (Self::ExtendedLengthPaths, Platform::Windows) => Conditional {
                reason: "Beyond 259 characters a path needs either the `\\\\?\\` prefix -- which \
                         `paths::to_extended_length` produces, and which disables every \
                         normalisation Win32 usually applies -- or the machine-wide \
                         LongPathsEnabled setting together with an application manifest.",
            },
            (Self::ExtendedLengthPaths, Platform::Linux) => Available,
            (Self::CaseSensitivePaths, Platform::Linux) => Available,
            (Self::CaseSensitivePaths, Platform::Windows) => Unsupported {
                reason: "Windows treats `Notes.txt` and `notes.txt` as one file. NTFS can be told \
                         otherwise per directory, so this is what to assume rather than what is \
                         certainly true of any given folder.",
            },

            // --- clipboard --------------------------------------------------
            (Self::NativeClipboardHistory, Platform::Windows) => NotImplemented {
                reason: "Windows keeps a clipboard history the product could read; doing so is a \
                         Win32 call and no adapter exists yet. The product's own history in \
                         `bp-clipboard` works on both platforms in the meantime.",
            },
            (Self::NativeClipboardHistory, Platform::Linux) => Unsupported {
                reason: "There is no clipboard history in X11 or Wayland to read; what a user \
                         has is whatever their clipboard manager kept.",
            },
        }
    }

    /// Whether this is usable on `platform` without further questions.
    #[must_use]
    pub const fn is_available_on(self, platform: Platform) -> bool {
        self.availability(platform).is_usable()
    }
}

/// The whole register for one platform, in declaration order.
///
/// For a diagnostics panel and for a bug report: the first question about any
/// platform-shaped defect is which of these the user's machine had, and asking
/// them to read a document is asking them to guess.
#[must_use]
pub fn report(platform: Platform) -> Vec<(Capability, Availability)> {
    Capability::ALL
        .iter()
        .map(|&capability| (capability, capability.availability(platform)))
        .collect()
}

/// Everything usable on `platform`, and nothing conditional.
///
/// The list a caller may branch on directly. Anything absent from it needs
/// either a probe or a fallback, which is why the conditional entries are
/// excluded rather than included with a caveat nobody reads.
#[must_use]
pub fn available_on(platform: Platform) -> Vec<Capability> {
    Capability::ALL
        .iter()
        .copied()
        .filter(|capability| capability.is_available_on(platform))
        .collect()
}

#[cfg(test)]
mod tests;
