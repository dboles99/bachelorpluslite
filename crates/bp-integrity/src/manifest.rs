//! A hash manifest: the digests of a whole set of files, and what changed.
//!
//! A signature answers a question about one document. A manifest answers a
//! different one -- *is this collection still the collection I sent* -- for a
//! folder of exhibits, an export, a case bundle, a backup. It is a list of
//! digests, so it needs no key and anybody can recompute it; that also means
//! it is only as trustworthy as the channel the manifest itself arrived by,
//! exactly as `bp-crypto` says of any bare hash. Sign the manifest to fix
//! that, and one signature then covers the whole set.
//!
//! ## The format
//!
//! ```text
//! BachelorPad+ manifest v1
//! algorithm: SHA-256
//! e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  notes.txt
//! ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  sub/report.md
//! ```
//!
//! Two header lines, then one line per file: **digest, two spaces, path** --
//! which is deliberately, exactly the line format `sha256sum` writes and
//! `sha256sum -c` reads. Strip the two header lines and a user on any Unix
//! box can check our manifest with a tool they already trust, without us. A
//! format that can only be verified by the program that wrote it is a format
//! that asks to be taken on faith, which is the opposite of the point.
//!
//! Paths are relative to a root the caller supplies, always with `/`
//! separators so a manifest written on Windows checks on Linux and back. A
//! path that cannot be written that way -- absolute, containing `..`, holding
//! a control character or a byte that is not UTF-8 -- is refused when the
//! manifest is built rather than escaped, because every escaping scheme is a
//! second thing to get wrong and `..` in a manifest is a directory traversal
//! waiting for a careless extractor.
//!
//! Entries are sorted by path, on the way out *and* on the way back in. The
//! order a caller happened to list files in is not information, and a
//! manifest that reorders itself between builds produces a diff the user has
//! to read to discover that nothing changed.
//!
//! ## Per file, not per set
//!
//! [`Manifest::check`] reports every file separately. A single pass/fail over
//! a set of two hundred exhibits tells the user they have a problem and not
//! where, which is the point at which they stop using the feature.
//!
//! ## What a manifest cannot tell you
//!
//! **That nothing was added.** It lists the files it lists; a file that
//! appears in the folder afterwards is not in it and cannot be missed by it.
//! Answering that would need the folder walked at check time, and the
//! manifest could still not say whether the extra file was always meant to be
//! there. Saying so here is better than a completeness claim that quietly is
//! not one.

use std::path::{Path, PathBuf};

use bp_crypto::{DocumentHash, HashAlgorithm, hash_document};

use crate::atomic::{Restrict, write_atomically};
use crate::error::IntegrityError;

/// The first line of every manifest, version included.
///
/// Exposed for the same reason as the sidecar's: so a caller can recognise
/// one of our manifests without parsing it, and so the version lives in one
/// place.
pub const MANIFEST_MAGIC: &str = "BachelorPad+ manifest v1";

const FIELD_ALGORITHM: &str = "algorithm";

/// The digest of a file on disk.
///
/// The file-level counterpart of `bp-crypto`'s `hash_document`, which takes
/// bytes and has no opinion about where they came from. Public because
/// "check this one file's checksum against the one on the download page" is a
/// whole task on its own, with no manifest and no signature in it.
///
/// Reads the file into memory, as `hash_document` does: a caller with a file
/// larger than memory has a streaming problem this crate does not solve, and
/// an API that looked like it did would be worse than the honest one.
pub fn hash_file(path: &Path, algorithm: HashAlgorithm) -> Result<DocumentHash, IntegrityError> {
    let bytes = std::fs::read(path).map_err(IntegrityError::read(path))?;
    Ok(hash_document(&bytes, algorithm))
}

/// One line of a manifest: a path and the digest it had.
///
/// The digest is kept as its hex spelling rather than as a `DocumentHash`,
/// because a manifest read back from disk only ever has the text -- and a
/// type that could hold either a parsed digest or a string would have two
/// states where the file has one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ManifestEntry {
    path: String,
    digest: String,
}

impl ManifestEntry {
    /// The file's path, relative to the manifest's root, with `/`
    /// separators.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The recorded digest, lowercase hex -- byte-for-byte what `sha256sum`
    /// prints, so a user can compare the two by eye.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// The digests of a set of files, and the algorithm that produced them.
///
/// The algorithm is recorded once for the whole manifest rather than per
/// line, because a set hashed two different ways is a set nobody can check
/// with one command, and because `sha256sum -c` would have nowhere to put the
/// per-line variation anyway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    algorithm: HashAlgorithm,
    entries: Vec<ManifestEntry>,
}

impl Manifest {
    /// Hash every named file, relative to `root`.
    ///
    /// Paths are given relative so the manifest survives the folder being
    /// moved, renamed, or unpacked somewhere else -- which is the normal fate
    /// of a bundle that has a manifest in the first place. An absolute path
    /// would pin the manifest to one machine.
    ///
    /// Fails on the first unreadable file rather than recording it as an
    /// entry: a manifest is a statement about files that were read, and one
    /// with a hole in it would be checked later and pass.
    pub fn build<P: AsRef<Path>>(
        root: &Path,
        relative_paths: &[P],
        algorithm: HashAlgorithm,
    ) -> Result<Self, IntegrityError> {
        let mut entries = Vec::with_capacity(relative_paths.len());

        for relative in relative_paths {
            let path = relative_to_text(relative.as_ref())?;
            let digest = hash_file(&join(root, &path), algorithm)?.to_hex();
            entries.push(ManifestEntry { path, digest });
        }

        entries.sort();
        if let Some(duplicate) = first_duplicate(&entries) {
            return Err(IntegrityError::UnusablePath {
                path: duplicate,
                reason: "this file is listed twice, so the manifest would contradict itself",
            });
        }
        Ok(Self { algorithm, entries })
    }

    /// Which digest these entries are.
    #[must_use]
    pub const fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    /// Every entry, sorted by path.
    #[must_use]
    pub fn entries(&self) -> &[ManifestEntry] {
        &self.entries
    }

    /// How many files the manifest speaks for.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the manifest covers nothing at all.
    ///
    /// Worth asking before showing a user a green tick: an empty manifest
    /// checks successfully because it asserts nothing, and a caller that does
    /// not distinguish "everything matched" from "there was nothing to match"
    /// will report a clean bundle for an empty one.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The exact text of the manifest file.
    ///
    /// Canonical, so rebuilding an unchanged set reproduces the file byte for
    /// byte instead of showing a diff.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::with_capacity(96 + self.entries.len() * 80);
        out.push_str(MANIFEST_MAGIC);
        out.push('\n');
        out.push_str(FIELD_ALGORITHM);
        out.push_str(": ");
        out.push_str(self.algorithm.name());
        out.push('\n');
        for entry in &self.entries {
            out.push_str(&entry.digest);
            // Two spaces: `sha256sum`'s separator, not decoration.
            out.push_str("  ");
            out.push_str(&entry.path);
            out.push('\n');
        }
        out
    }

    /// Read a manifest back.
    ///
    /// As strict as the sidecar parser, and for the same reason: a file with
    /// several spellings is a file whose comparisons cannot be relied on. The
    /// one leniency is line endings and trailing blank lines, because a
    /// manifest is a text file that will be mailed and pasted.
    pub fn parse(text: &str) -> Result<Self, MalformedManifest> {
        let mut lines = text.lines().enumerate();

        let (_, magic) = lines.next().ok_or(MalformedManifest::Truncated {
            missing: "the format marker",
        })?;
        if magic.trim_end() != MANIFEST_MAGIC {
            return Err(MalformedManifest::WrongMagic {
                found: excerpt(magic),
            });
        }

        let (_, algorithm_line) = lines.next().ok_or(MalformedManifest::Truncated {
            missing: FIELD_ALGORITHM,
        })?;
        let algorithm = parse_algorithm(algorithm_line)?;

        let mut entries = Vec::new();
        for (index, line) in lines {
            // Human line numbers, because the user is looking at the file.
            let number = index + 1;
            if line.trim().is_empty() {
                continue;
            }
            entries.push(parse_entry(line, number, algorithm)?);
        }

        entries.sort();
        if let Some(path) = first_duplicate(&entries) {
            return Err(MalformedManifest::DuplicatePath { path });
        }
        Ok(Self { algorithm, entries })
    }

    /// Write the manifest to `path`, atomically.
    ///
    /// Not permission-restricted: a manifest is a list of digests of files the
    /// reader already has, so there is nothing in it to keep from them.
    pub fn write(&self, path: &Path) -> Result<(), IntegrityError> {
        write_atomically(path, self.to_text().as_bytes(), Restrict::No)
            .map_err(IntegrityError::write(path))
    }

    /// Read a manifest from `path`.
    pub fn read(path: &Path) -> Result<Self, IntegrityError> {
        let bytes = std::fs::read(path).map_err(IntegrityError::read(path))?;
        let text = String::from_utf8(bytes).map_err(|_| IntegrityError::NotText {
            path: path.to_owned(),
        })?;
        Self::parse(&text).map_err(|source| IntegrityError::MalformedManifestFile {
            path: path.to_owned(),
            source,
        })
    }

    /// Check every file under `root` against its recorded digest.
    ///
    /// Returns a report rather than a `Result`, and never stops at the first
    /// failure: a user whose bundle has three altered files needs all three
    /// names, not the alphabetically first one and another run.
    ///
    /// An unreadable file is a per-file outcome here, unlike in
    /// [`Self::build`]. The asymmetry is the point. When building, an
    /// unreadable file means the manifest would be silently incomplete;
    /// when checking, it *is* the finding, and abandoning the run would hide
    /// every other file's result behind it.
    #[must_use]
    pub fn check(&self, root: &Path) -> ManifestReport {
        let files = self
            .entries
            .iter()
            .map(|entry| FileCheck {
                path: entry.path.clone(),
                outcome: check_one(&join(root, &entry.path), &entry.digest, self.algorithm),
            })
            .collect();
        ManifestReport { files }
    }
}

/// What happened to one file when a manifest was checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCheck {
    /// The path as the manifest spells it, so the user can find the line.
    pub path: String,
    /// What was found there.
    pub outcome: FileOutcome,
}

/// The per-file verdict.
///
/// Three ways to fail rather than one, because they are three different
/// events: somebody edited the file, somebody moved or deleted it, or the
/// storage is refusing. Only the first is evidence of tampering, and
/// reporting all three as "failed" is how a permissions problem gets
/// escalated as a security incident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOutcome {
    /// The file is byte-for-byte what it was.
    Matches,

    /// The file is there and is different. Both digests are carried so the
    /// user can compare them against a third source rather than take our
    /// word for it.
    Differs { expected: String, found: String },

    /// Nothing at that path. A deletion, a rename, or a bundle that was
    /// unpacked incompletely.
    Missing,

    /// The file exists and could not be read -- permissions, a broken link,
    /// a disconnected share. Says nothing about its contents, and must not be
    /// shown as though it did.
    Unreadable { reason: String },
}

impl FileOutcome {
    /// Whether this file is intact. Everything that is not [`Self::Matches`]
    /// is a failure, including the two that are not about tampering: a file
    /// that cannot be read has not been shown to be unchanged.
    #[must_use]
    pub const fn is_match(&self) -> bool {
        matches!(self, Self::Matches)
    }
}

/// The result of checking a whole set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestReport {
    files: Vec<FileCheck>,
}

impl ManifestReport {
    /// Every file's result, in manifest order.
    #[must_use]
    pub fn files(&self) -> &[FileCheck] {
        &self.files
    }

    /// Just the ones the user has to do something about.
    ///
    /// The realistic report is two hundred matches and one failure, and a
    /// caller that has to filter for itself will eventually filter wrongly.
    pub fn failures(&self) -> impl Iterator<Item = &FileCheck> {
        self.files.iter().filter(|file| !file.outcome.is_match())
    }

    /// Whether every file in the manifest was found and unchanged.
    ///
    /// True for an empty manifest, which asserts nothing -- see
    /// [`Manifest::is_empty`] before showing this as a clean bill of health.
    #[must_use]
    pub fn all_match(&self) -> bool {
        self.files.iter().all(|file| file.outcome.is_match())
    }

    /// A one-line summary for a status bar, with the counts a user acts on.
    #[must_use]
    pub fn summarise(&self) -> String {
        let total = self.files.len();
        let failed = self.failures().count();
        if total == 0 {
            return "This manifest lists no files, so nothing was checked.".to_owned();
        }
        if failed == 0 {
            return format!("All {total} files match the manifest.");
        }
        format!("{failed} of {total} files do not match the manifest.")
    }
}

/// Why a manifest file could not be read as one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MalformedManifest {
    #[error("this does not start with \"{MANIFEST_MAGIC}\"; it starts with {found:?}")]
    WrongMagic { found: String },

    #[error("this manifest stops early -- {missing} is missing")]
    Truncated { missing: &'static str },

    #[error("expected an \"{FIELD_ALGORITHM}\" line here, found {found:?}")]
    WrongField { found: String },

    /// Refused rather than guessed at: checking a SHA-512 manifest with
    /// SHA-256 would report every single file as altered.
    #[error("this manifest uses {found:?}, which this version cannot check")]
    UnknownAlgorithm { found: String },

    #[error("line {line} is not a digest and a path: {found:?}")]
    BadLine { line: usize, found: String },

    #[error("the digest on line {line}, for {path:?}, is not a {expected}-digit lowercase hex")]
    BadDigest {
        line: usize,
        path: String,
        expected: usize,
    },

    #[error("the path on line {line} cannot be used: {reason} ({path:?})")]
    BadPath {
        line: usize,
        path: String,
        reason: &'static str,
    },

    /// Two lines claiming different things about one file is a manifest that
    /// contradicts itself, and picking one of them would be inventing an
    /// answer.
    #[error("{path:?} appears more than once in this manifest")]
    DuplicatePath { path: String },
}

/// Hash one file and compare it with what the manifest recorded.
fn check_one(path: &Path, expected: &str, algorithm: HashAlgorithm) -> FileOutcome {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return FileOutcome::Missing,
        Err(e) => {
            return FileOutcome::Unreadable {
                reason: e.to_string(),
            };
        }
    };
    let found = hash_document(&bytes, algorithm).to_hex();
    if found == expected {
        FileOutcome::Matches
    } else {
        FileOutcome::Differs {
            expected: expected.to_owned(),
            found,
        }
    }
}

/// Join a manifest path onto a root one component at a time.
///
/// Component by component rather than `root.join(text)`, so that the `/`
/// separators in a manifest mean the same thing on both platforms without
/// relying on Windows happening to accept them.
fn join(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

/// Turn a relative path into the one spelling a manifest may contain.
fn relative_to_text(path: &Path) -> Result<String, IntegrityError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => {
                let part = part.to_str().ok_or_else(|| IntegrityError::UnusablePath {
                    path: path.display().to_string(),
                    reason: "a manifest path must be valid UTF-8",
                })?;
                parts.push(part);
            }
            _ => {
                return Err(IntegrityError::UnusablePath {
                    path: path.display().to_string(),
                    reason: "a manifest path must be relative and free of \".\" and \"..\"",
                });
            }
        }
    }

    let text = parts.join("/");
    check_path(&text).map_err(|reason| IntegrityError::UnusablePath {
        path: text.clone(),
        reason,
    })?;
    Ok(text)
}

/// The rules a manifest path must obey, applied on the way in and on the way
/// out.
///
/// Applied to text read back as well as text written, because a manifest can
/// arrive from anywhere and `../../etc/passwd` in one is a directory
/// traversal aimed at whatever unpacks it.
fn check_path(path: &str) -> Result<(), &'static str> {
    if path.is_empty() {
        return Err("a manifest path cannot be empty");
    }
    if path.contains(|c: char| c.is_control()) {
        return Err("a manifest path cannot contain control characters");
    }
    if path.contains('\\') {
        return Err("a manifest path must use \"/\" as its separator");
    }
    if path.contains(':') {
        return Err("a manifest path cannot name a drive");
    }
    if path.starts_with('/') {
        return Err("a manifest path must be relative");
    }
    for part in path.split('/') {
        if part.is_empty() {
            return Err("a manifest path cannot contain an empty component");
        }
        if part == "." || part == ".." {
            return Err("a manifest path cannot contain \".\" or \"..\"");
        }
    }
    Ok(())
}

/// Read the `algorithm:` header.
fn parse_algorithm(line: &str) -> Result<HashAlgorithm, MalformedManifest> {
    let (name, value) = line
        .split_once(':')
        .ok_or_else(|| MalformedManifest::WrongField {
            found: excerpt(line),
        })?;
    if name.trim() != FIELD_ALGORITHM {
        return Err(MalformedManifest::WrongField {
            found: excerpt(name),
        });
    }
    // Matched against the algorithm's own `name()` so the two spellings
    // cannot drift apart.
    let value = value.trim();
    [HashAlgorithm::Sha256, HashAlgorithm::Sha512]
        .into_iter()
        .find(|algorithm| algorithm.name() == value)
        .ok_or_else(|| MalformedManifest::UnknownAlgorithm {
            found: excerpt(value),
        })
}

/// Read one `digest  path` line.
fn parse_entry(
    line: &str,
    number: usize,
    algorithm: HashAlgorithm,
) -> Result<ManifestEntry, MalformedManifest> {
    let line = line.trim_end_matches([' ', '\t']);
    let (digest, path) = line
        .split_once("  ")
        .ok_or_else(|| MalformedManifest::BadLine {
            line: number,
            found: excerpt(line),
        })?;

    check_path(path).map_err(|reason| MalformedManifest::BadPath {
        line: number,
        path: path.to_owned(),
        reason,
    })?;

    let expected = algorithm.digest_len() * 2;
    if digest.len() != expected
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(MalformedManifest::BadDigest {
            line: number,
            path: path.to_owned(),
            expected,
        });
    }

    Ok(ManifestEntry {
        path: path.to_owned(),
        digest: digest.to_owned(),
    })
}

/// The first path that appears twice in a sorted entry list.
fn first_duplicate(sorted: &[ManifestEntry]) -> Option<String> {
    sorted
        .windows(2)
        .find(|pair| pair[0].path == pair[1].path)
        .map(|pair| pair[0].path.clone())
}

/// Enough of an offending line to recognise it, and no more -- a manifest
/// that failed to parse may be any file at all, including a private one.
fn excerpt(line: &str) -> String {
    const LIMIT: usize = 40;
    let trimmed = line.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_owned();
    }
    let mut out: String = trimmed.chars().take(LIMIT).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests;
