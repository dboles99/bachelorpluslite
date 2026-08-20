//! The `SemanticName` type: BachelorPad+'s filename grammar as data.

use time::Date;

use crate::date::{format_date, parse_date};
use crate::sanitize::{
    FALLBACK_TITLE, MAX_COMPONENT_BYTES, sanitize_extension, sanitize_title, truncate_bytes,
};

/// Revision numbering starts at 2 -- the original document is `v1` implicitly
/// and carries no suffix.
pub const FIRST_REVISION: u32 = 2;

/// Collision numbering starts at 2 -- the first file to claim a name carries
/// no suffix, so `_01` is never emitted.
pub const FIRST_COLLISION: u32 = 2;

/// How many collision suffixes to try before giving up.
const MAX_COLLISION: u32 = 9999;

/// A filename in BachelorPad+'s grammar (specs.md section 2):
///
/// ```text
/// <Title>[_v<N>]_<DDMMMYYYY>[_<NN>].<ext>
/// ```
///
/// The spec fixes the `Title_DDMMMYYYY.ext` core and requires both suffixes
/// but does not fix their order relative to the date. The ordering above is
/// this crate's decision, recorded in `docs/decisions/ADR-0003.md`:
///
/// * the revision suffix sits with the title, because `_v2` is part of what
///   the document *is* -- a deliberate new version the user asked for;
/// * the collision suffix sits last, because `_02` is a mechanical
///   disambiguator applied at write time by the filesystem layer and carries
///   no meaning about the document.
///
/// This also makes parsing unambiguous: the date is a fixed-width anchor with
/// at most one numeric segment after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticName {
    title: String,
    revision: Option<u32>,
    date: Date,
    collision: Option<u32>,
    extension: String,
}

impl SemanticName {
    /// Build a name, sanitising the title and extension.
    pub fn new(title: &str, date: Date, extension: &str) -> Self {
        Self {
            title: sanitize_title(title),
            revision: None,
            date,
            collision: None,
            extension: sanitize_extension(extension),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub const fn date(&self) -> Date {
        self.date
    }

    pub const fn revision(&self) -> Option<u32> {
        self.revision
    }

    pub const fn collision(&self) -> Option<u32> {
        self.collision
    }

    pub fn extension(&self) -> &str {
        &self.extension
    }

    /// Set the revision suffix. Values below [`FIRST_REVISION`] clear it,
    /// since `v1` is the unsuffixed original.
    #[must_use]
    pub fn with_revision(mut self, revision: Option<u32>) -> Self {
        self.revision = revision.filter(|n| *n >= FIRST_REVISION);
        self
    }

    /// Set the collision suffix. Values below [`FIRST_COLLISION`] clear it.
    #[must_use]
    pub fn with_collision(mut self, collision: Option<u32>) -> Self {
        self.collision = collision.filter(|n| *n >= FIRST_COLLISION);
        self
    }

    /// Render the filename.
    ///
    /// The result is always a legal path component of at most
    /// [`MAX_COMPONENT_BYTES`] bytes: the title is truncated to fit around
    /// the suffixes rather than the whole name being cut, which would
    /// otherwise destroy the extension and make the file unopenable.
    pub fn to_filename(&self) -> String {
        let mut suffix = String::new();
        if let Some(r) = self.revision {
            suffix.push_str(&format!("_v{r}"));
        }
        suffix.push('_');
        suffix.push_str(&format_date(self.date));
        if let Some(c) = self.collision {
            suffix.push_str(&format!("_{c:02}"));
        }
        suffix.push('.');
        suffix.push_str(&self.extension);

        let budget = MAX_COMPONENT_BYTES.saturating_sub(suffix.len());
        let title = truncate_bytes(&self.title, budget).trim_end_matches([' ', '.', '_', '-']);
        let title = if title.is_empty() {
            FALLBACK_TITLE
        } else {
            title
        };

        format!("{title}{suffix}")
    }

    /// Parse a filename in the grammar. Returns `None` for names that do not
    /// conform -- an ordinary `notes.txt` is not an error, it simply has no
    /// semantic structure to read.
    pub fn parse(filename: &str) -> Option<Self> {
        let (stem, extension) = filename.rsplit_once('.')?;
        if stem.is_empty() || extension.is_empty() {
            return None;
        }

        let segments: Vec<&str> = stem.split('_').collect();
        let last = segments.len().checked_sub(1)?;

        // The date is a fixed-width anchor: it is either the final segment,
        // or the one before a canonical collision suffix.
        let (date_index, date, collision) = match parse_date(segments[last]) {
            Some(date) => (last, date, None),
            None => {
                let index = last.checked_sub(1)?;
                let collision = parse_collision(segments[last])?;
                (index, parse_date(segments[index])?, Some(collision))
            }
        };

        // Consume a revision suffix only if a title still remains after it.
        let (revision, title_end) = match date_index.checked_sub(1) {
            Some(i) if i >= 1 => match parse_revision(segments[i]) {
                Some(r) => (Some(r), i),
                None => (None, date_index),
            },
            _ => (None, date_index),
        };

        if title_end == 0 {
            return None;
        }
        let title = segments[..title_end].join("_");

        Some(Self {
            title,
            revision,
            date,
            collision,
            extension: extension.to_owned(),
        })
    }

    /// Find the first free filename, adding a collision suffix if needed.
    ///
    /// `exists` is injected rather than hitting the filesystem here so the
    /// rule stays pure and testable; `bp-files` supplies the real predicate.
    /// Returns `None` if every suffix up to [`MAX_COLLISION`] is taken, which
    /// the caller must surface rather than overwrite a user's file.
    pub fn resolve_collision(&self, mut exists: impl FnMut(&str) -> bool) -> Option<Self> {
        let bare = self.clone().with_collision(None);
        if !exists(&bare.to_filename()) {
            return Some(bare);
        }
        (FIRST_COLLISION..=MAX_COLLISION)
            .map(|n| self.clone().with_collision(Some(n)))
            .find(|candidate| !exists(&candidate.to_filename()))
    }
}

/// Parse a canonical collision suffix (`02`, `03`, ... `100`).
///
/// Requires canonical formatting, so `002` is rejected -- accepting it would
/// mean a parse/render round trip silently rewrote the user's filename.
fn parse_collision(s: &str) -> Option<u32> {
    let n: u32 = s.parse().ok()?;
    (n >= FIRST_COLLISION && format!("{n:02}") == s).then_some(n)
}

/// Parse a canonical revision suffix (`v2`, `v3`, ...).
fn parse_revision(s: &str) -> Option<u32> {
    let n: u32 = s.strip_prefix('v')?.parse().ok()?;
    (n >= FIRST_REVISION && format!("v{n}") == s).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    const D: Date = date!(2026 - 08 - 16);

    #[test]
    fn renders_the_spec_example() {
        let n = SemanticName::new("Rust Migration Notes - UE AI Editor Suite", D, "txt");
        assert_eq!(
            n.to_filename(),
            "Rust Migration Notes - UE AI Editor Suite_16AUG2026.txt"
        );
    }

    #[test]
    fn renders_both_suffixes_in_the_documented_order() {
        let n = SemanticName::new("Report", D, "md")
            .with_revision(Some(2))
            .with_collision(Some(3));
        assert_eq!(n.to_filename(), "Report_v2_16AUG2026_03.md");
    }

    #[test]
    fn suffix_values_below_the_first_are_dropped() {
        let n = SemanticName::new("Report", D, "txt")
            .with_revision(Some(1))
            .with_collision(Some(1));
        assert_eq!(n.to_filename(), "Report_16AUG2026.txt");
    }

    #[test]
    fn parses_a_plain_name() {
        let n = SemanticName::parse("Report_16AUG2026.txt").unwrap();
        assert_eq!(n.title(), "Report");
        assert_eq!(n.date(), D);
        assert_eq!(n.revision(), None);
        assert_eq!(n.collision(), None);
        assert_eq!(n.extension(), "txt");
    }

    #[test]
    fn parses_both_suffixes() {
        let n = SemanticName::parse("Report_v2_16AUG2026_03.md").unwrap();
        assert_eq!(n.title(), "Report");
        assert_eq!(n.revision(), Some(2));
        assert_eq!(n.collision(), Some(3));
    }

    #[test]
    fn titles_may_contain_underscores_and_the_date_still_anchors() {
        let n = SemanticName::parse("Backup_of_notes_16AUG2026.txt").unwrap();
        assert_eq!(n.title(), "Backup_of_notes");
    }

    #[test]
    fn the_rightmost_date_wins() {
        // A title that itself looks like a dated name must not shadow the
        // real date component.
        let n = SemanticName::parse("Backup_16AUG2026_notes_17AUG2026.txt").unwrap();
        assert_eq!(n.title(), "Backup_16AUG2026_notes");
        assert_eq!(n.date(), date!(2026 - 08 - 17));
    }

    #[test]
    fn rejects_non_conforming_names() {
        assert!(SemanticName::parse("notes.txt").is_none());
        assert!(SemanticName::parse("16AUG2026.txt").is_none(), "no title");
        assert!(SemanticName::parse("Report_16AUG2026").is_none(), "no ext");
        assert!(SemanticName::parse("Report_31FEB2026.txt").is_none());
        assert!(SemanticName::parse("").is_none());
        assert!(SemanticName::parse(".txt").is_none());
    }

    #[test]
    fn rejects_non_canonical_suffixes() {
        // `_002` and `_01` are not things we emit; treating them as
        // structure would make a round trip rewrite the user's filename.
        assert!(SemanticName::parse("Report_16AUG2026_002.txt").is_none());
        assert!(SemanticName::parse("Report_16AUG2026_01.txt").is_none());
        // A non-numeric trailing segment simply is not the grammar.
        assert!(SemanticName::parse("Report_16AUG2026_draft.txt").is_none());
    }

    #[test]
    fn a_bare_revision_segment_is_treated_as_the_title() {
        // `v2_16AUG2026.txt` has no title to speak of; consuming `v2` as a
        // revision would leave an empty title, so it stays the title.
        let n = SemanticName::parse("v2_16AUG2026.txt").unwrap();
        assert_eq!(n.title(), "v2");
        assert_eq!(n.revision(), None);
    }

    #[test]
    fn a_title_ending_in_a_revision_segment_is_read_as_a_revision() {
        // Known, accepted ambiguity in the grammar: `Report_v2` as a *title*
        // is indistinguishable from `Report` at revision 2, because that is
        // exactly what the suffix means. Recorded here so the behaviour is a
        // decision rather than a surprise; the property test excludes such
        // titles from the round-trip invariant for the same reason.
        let n = SemanticName::parse("Report_v2_16AUG2026.txt").unwrap();
        assert_eq!(n.title(), "Report");
        assert_eq!(n.revision(), Some(2));
    }

    #[test]
    fn long_titles_are_truncated_but_the_extension_survives() {
        let n = SemanticName::new(&"A".repeat(400), D, "markdown");
        let f = n.to_filename();
        assert!(f.len() <= MAX_COMPONENT_BYTES, "got {} bytes", f.len());
        assert!(f.ends_with("_16AUG2026.markdown"));
    }

    #[test]
    fn resolve_collision_returns_the_bare_name_when_free() {
        let n = SemanticName::new("Report", D, "txt");
        let r = n.resolve_collision(|_| false).unwrap();
        assert_eq!(r.to_filename(), "Report_16AUG2026.txt");
    }

    #[test]
    fn resolve_collision_skips_taken_names() {
        let n = SemanticName::new("Report", D, "txt");
        let taken = ["Report_16AUG2026.txt", "Report_16AUG2026_02.txt"];
        let r = n.resolve_collision(|f| taken.contains(&f)).unwrap();
        assert_eq!(r.to_filename(), "Report_16AUG2026_03.txt");
    }

    #[test]
    fn resolve_collision_gives_up_rather_than_overwriting() {
        let n = SemanticName::new("Report", D, "txt");
        assert!(
            n.resolve_collision(|_| true).is_none(),
            "must never hand back a name that is already taken"
        );
    }

    #[test]
    fn resolve_collision_preserves_the_revision() {
        let n = SemanticName::new("Report", D, "txt").with_revision(Some(2));
        let r = n
            .resolve_collision(|f| f == "Report_v2_16AUG2026.txt")
            .unwrap();
        assert_eq!(r.to_filename(), "Report_v2_16AUG2026_02.txt");
    }
}
