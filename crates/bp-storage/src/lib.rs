//! The local metadata store (specs.md section 21).
//!
//! SQLite, because the questions this has to answer later -- which notes are
//! related, which are near-duplicates, what is tagged what -- are queries, and
//! a file format that is not a database answers them by being read entirely
//! into memory first.
//!
//! Two rules shape everything here:
//!
//! * **The store holds metadata, never document content.** A title extracted
//!   from a note is a summary of what the user wrote, and ADR-0011 says the
//!   document's security profile governs where that may be kept. Until phase
//!   14 exists, what goes in stays deliberately thin.
//! * **Time is a parameter, never a clock.** `bp-naming` earns its exhaustive
//!   tests by not reading one, and the same applies here.
//!
//! Nothing in the application writes to this yet. It is the foundation phase 9
//! builds on, not a feature.

#![forbid(unsafe_code)]

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

mod migrations;
pub use migrations::latest_version;

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-storage";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("metadata store: {0}")]
    Database(#[from] rusqlite::Error),

    /// A path that is not valid UTF-8.
    ///
    /// Refused rather than converted lossily: two different files whose names
    /// differ only in the bytes that got replaced would become one row, and
    /// silently merging two documents is worse than declining one.
    #[error("{0} cannot be stored: its name is not valid UTF-8")]
    UnsupportedPath(String),

    /// The file was written by a newer BachelorPad+ than this one.
    #[error(
        "this metadata store is version {found}, but this build understands \
         only up to {supported}; it was written by a newer BachelorPad+"
    )]
    FromTheFuture { found: u32, supported: u32 },
}

/// A document as the store knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRecord {
    pub id: i64,
    pub path: String,
    pub title: Option<String>,
    pub first_seen: i64,
    pub last_seen: i64,
}

/// The local metadata database.
pub struct Store {
    connection: Connection,
}

impl Store {
    /// Open (or create) the store at `path`, bringing the schema up to date.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::from_connection(Connection::open(path)?)
    }

    /// A store that exists only for the life of the process. For tests.
    pub fn in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, StoreError> {
        // SQLite disables foreign keys by default, per connection, for
        // backwards compatibility. Every `ON DELETE CASCADE` in the schema is
        // inert without this, so deleting a document would leave its tag rows
        // pointing at nothing.
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        // Survives a crash without the risk of a corrupt file, and lets a
        // reader run while a writer works -- which matters the moment
        // indexing happens off the UI thread.
        connection.execute_batch("PRAGMA journal_mode = WAL;")?;

        let mut store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    /// Apply every migration this build has that the file does not.
    fn migrate(&mut self) -> Result<(), StoreError> {
        let found: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let supported = latest_version();

        // Opening a newer file read-only and hoping is how metadata gets
        // quietly corrupted by an older build. Say so instead.
        if found > supported {
            return Err(StoreError::FromTheFuture { found, supported });
        }

        for (index, sql) in migrations::MIGRATIONS.iter().enumerate() {
            let version = u32::try_from(index + 1).unwrap_or(u32::MAX);
            if version <= found {
                continue;
            }
            // One transaction per migration, with the version bumped inside
            // it: a migration that fails half way leaves the file on the
            // version it started at rather than in between two schemas.
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(sql)?;
            transaction.pragma_update(None, "user_version", version)?;
            transaction.commit()?;
        }
        Ok(())
    }

    /// The schema version of the open file.
    pub fn version(&self) -> Result<u32, StoreError> {
        Ok(self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    /// Record that a document was seen, inserting it if it is new.
    ///
    /// `first_seen` is kept from the original row: the point of it is when
    /// the document entered the workspace, which a later visit does not
    /// change. A `None` title leaves any existing one alone rather than
    /// erasing it, so opening a note without extracting a title does not
    /// discard the one already known.
    pub fn record_document(
        &self,
        path: &Path,
        title: Option<&str>,
        seen_at: i64,
    ) -> Result<i64, StoreError> {
        let path = path_str(path)?;

        self.connection.execute(
            "INSERT INTO documents (path, title, first_seen, last_seen)
                  VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT (path) DO UPDATE SET
                 last_seen = excluded.last_seen,
                 title     = COALESCE(excluded.title, documents.title)",
            params![path, title, seen_at],
        )?;

        Ok(self.connection.query_row(
            "SELECT id FROM documents WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )?)
    }

    pub fn document(&self, path: &Path) -> Result<Option<DocumentRecord>, StoreError> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, path, title, first_seen, last_seen
                   FROM documents WHERE path = ?1",
                params![path_str(path)?],
                document_from_row,
            )
            .optional()?)
    }

    /// Documents, most recently seen first.
    pub fn recent_documents(&self, limit: usize) -> Result<Vec<DocumentRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, path, title, first_seen, last_seen
               FROM documents ORDER BY last_seen DESC, id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit as i64], document_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Forget a document, and with it any tags that were only on it.
    pub fn forget_document(&self, path: &Path) -> Result<bool, StoreError> {
        let removed = self.connection.execute(
            "DELETE FROM documents WHERE path = ?1",
            params![path_str(path)?],
        )?;
        Ok(removed > 0)
    }

    /// Attach a tag, creating it if this is the first use.
    ///
    /// Idempotent: tagging something twice is not an error, because the user
    /// asked for a state rather than for an action.
    pub fn tag_document(&self, document: i64, tag: &str) -> Result<(), StoreError> {
        let tag = tag.trim();
        if tag.is_empty() {
            return Ok(());
        }
        self.connection.execute(
            "INSERT INTO tags (name) VALUES (?1) ON CONFLICT (name) DO NOTHING",
            params![tag],
        )?;
        self.connection.execute(
            "INSERT INTO document_tags (document_id, tag_id)
                  SELECT ?1, id FROM tags WHERE name = ?2
             ON CONFLICT DO NOTHING",
            params![document, tag],
        )?;
        Ok(())
    }

    pub fn untag_document(&self, document: i64, tag: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "DELETE FROM document_tags
              WHERE document_id = ?1
                AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
            params![document, tag],
        )?;
        Ok(())
    }

    /// A document's tags, in alphabetical order.
    pub fn tags_of(&self, document: i64) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT t.name FROM tags t
               JOIN document_tags dt ON dt.tag_id = t.id
              WHERE dt.document_id = ?1
              ORDER BY t.name COLLATE NOCASE",
        )?;
        let rows = statement.query_map(params![document], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Every document carrying `tag`, most recently seen first.
    pub fn documents_tagged(&self, tag: &str) -> Result<Vec<DocumentRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT d.id, d.path, d.title, d.first_seen, d.last_seen
               FROM documents d
               JOIN document_tags dt ON dt.document_id = d.id
               JOIN tags t ON t.id = dt.tag_id
              WHERE t.name = ?1
              ORDER BY d.last_seen DESC, d.id DESC",
        )?;
        let rows = statement.query_map(params![tag], document_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

fn document_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DocumentRecord> {
    Ok(DocumentRecord {
        id: row.get(0)?,
        path: row.get(1)?,
        title: row.get(2)?,
        first_seen: row.get(3)?,
        last_seen: row.get(4)?,
    })
}

fn path_str(path: &Path) -> Result<&str, StoreError> {
    path.to_str()
        .ok_or_else(|| StoreError::UnsupportedPath(path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn store() -> Store {
        Store::in_memory().unwrap()
    }

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("/notes/{name}"))
    }

    #[test]
    fn a_new_store_is_at_the_latest_version() {
        assert_eq!(store().version().unwrap(), latest_version());
        assert!(latest_version() >= 1, "there is at least one migration");
    }

    #[test]
    fn opening_an_existing_store_does_not_reapply_migrations() {
        // Re-running a migration would fail on `CREATE TABLE`, so this also
        // proves the version check is what stops it rather than luck.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("meta.db");

        let first = Store::open(&file).unwrap();
        first
            .record_document(&path("a.md"), Some("A"), 100)
            .unwrap();
        drop(first);

        let second = Store::open(&file).unwrap();
        assert_eq!(second.version().unwrap(), latest_version());
        assert_eq!(
            second.document(&path("a.md")).unwrap().unwrap().title,
            Some("A".to_owned()),
            "the data survived the reopen"
        );
    }

    #[test]
    fn a_store_from_a_newer_build_is_refused_rather_than_migrated() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("meta.db");
        {
            let connection = Connection::open(&file).unwrap();
            connection
                .pragma_update(None, "user_version", latest_version() + 5)
                .unwrap();
        }

        match Store::open(&file) {
            Err(StoreError::FromTheFuture { found, supported }) => {
                assert_eq!(found, latest_version() + 5);
                assert_eq!(supported, latest_version());
            }
            other => panic!("expected a refusal, got {other:?}", other = other.err()),
        }
    }

    #[test]
    fn foreign_keys_are_actually_on() {
        // SQLite disables them per connection by default, so every
        // ON DELETE CASCADE in the schema is inert unless the pragma ran.
        let store = store();
        let on: i64 = store
            .connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(on, 1);
    }

    #[test]
    fn recording_a_document_twice_updates_rather_than_duplicates() {
        let store = store();
        let first = store
            .record_document(&path("note.md"), Some("Note"), 100)
            .unwrap();
        let second = store.record_document(&path("note.md"), None, 200).unwrap();

        assert_eq!(first, second, "the same file is one document");

        let record = store.document(&path("note.md")).unwrap().unwrap();
        assert_eq!(record.first_seen, 100, "when it entered the workspace");
        assert_eq!(record.last_seen, 200);
        assert_eq!(
            record.title,
            Some("Note".to_owned()),
            "a missing title must not erase the known one"
        );
    }

    #[test]
    fn a_later_title_replaces_an_earlier_one() {
        let store = store();
        store
            .record_document(&path("n.md"), Some("Draft"), 1)
            .unwrap();
        store
            .record_document(&path("n.md"), Some("Final"), 2)
            .unwrap();

        assert_eq!(
            store.document(&path("n.md")).unwrap().unwrap().title,
            Some("Final".to_owned())
        );
    }

    #[test]
    fn an_unknown_document_is_none_rather_than_an_error() {
        assert!(store().document(&path("never.md")).unwrap().is_none());
    }

    #[test]
    fn recent_documents_are_newest_first_and_bounded() {
        let store = store();
        for (index, name) in ["a.md", "b.md", "c.md"].iter().enumerate() {
            store
                .record_document(&path(name), None, i64::try_from(index).unwrap())
                .unwrap();
        }

        let recent = store.recent_documents(2).unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent[0].path.ends_with("c.md"));
        assert!(recent[1].path.ends_with("b.md"));
    }

    #[test]
    fn tagging_is_idempotent_and_case_insensitive() {
        let store = store();
        let id = store.record_document(&path("n.md"), None, 1).unwrap();

        store.tag_document(id, "Rust").unwrap();
        store.tag_document(id, "rust").unwrap();
        store.tag_document(id, "Rust").unwrap();

        assert_eq!(
            store.tags_of(id).unwrap(),
            vec!["Rust".to_owned()],
            "one tag, under the name it was first given"
        );
    }

    #[test]
    fn a_blank_tag_is_ignored_rather_than_stored() {
        let store = store();
        let id = store.record_document(&path("n.md"), None, 1).unwrap();
        store.tag_document(id, "   ").unwrap();
        store.tag_document(id, "").unwrap();

        assert!(store.tags_of(id).unwrap().is_empty());
    }

    #[test]
    fn tags_are_trimmed_so_the_same_word_is_the_same_tag() {
        let store = store();
        let id = store.record_document(&path("n.md"), None, 1).unwrap();
        store.tag_document(id, " rust ").unwrap();
        store.tag_document(id, "rust").unwrap();

        assert_eq!(store.tags_of(id).unwrap().len(), 1);
    }

    #[test]
    fn untagging_removes_only_that_tag() {
        let store = store();
        let id = store.record_document(&path("n.md"), None, 1).unwrap();
        store.tag_document(id, "rust").unwrap();
        store.tag_document(id, "notes").unwrap();

        store.untag_document(id, "rust").unwrap();
        assert_eq!(store.tags_of(id).unwrap(), vec!["notes".to_owned()]);
    }

    #[test]
    fn documents_can_be_found_by_tag() {
        let store = store();
        let a = store.record_document(&path("a.md"), None, 1).unwrap();
        let b = store.record_document(&path("b.md"), None, 2).unwrap();
        let c = store.record_document(&path("c.md"), None, 3).unwrap();
        store.tag_document(a, "rust").unwrap();
        store.tag_document(c, "rust").unwrap();
        store.tag_document(b, "prose").unwrap();

        let tagged = store.documents_tagged("rust").unwrap();
        assert_eq!(tagged.len(), 2);
        assert!(tagged[0].path.ends_with("c.md"), "newest first");
        assert!(tagged[1].path.ends_with("a.md"));
    }

    #[test]
    fn forgetting_a_document_takes_its_tag_links_with_it() {
        // Which only happens because foreign keys are on.
        let store = store();
        let id = store.record_document(&path("n.md"), None, 1).unwrap();
        store.tag_document(id, "rust").unwrap();

        assert!(store.forget_document(&path("n.md")).unwrap());
        assert!(store.document(&path("n.md")).unwrap().is_none());

        let orphans: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM document_tags", [], |row| row.get(0))
            .unwrap();
        assert_eq!(orphans, 0, "the join rows went with the document");
        assert!(
            store.documents_tagged("rust").unwrap().is_empty(),
            "the tag itself survives; nothing carries it"
        );
    }

    #[test]
    fn forgetting_something_unknown_says_so_rather_than_failing() {
        assert!(!store().forget_document(&path("never.md")).unwrap());
    }

    #[test]
    fn unicode_paths_and_titles_round_trip() {
        let store = store();
        let file = PathBuf::from("/notes/日本語のノート.md");
        store.record_document(&file, Some("日本語"), 1).unwrap();

        let record = store.document(&file).unwrap().unwrap();
        assert_eq!(record.title, Some("日本語".to_owned()));
        assert!(record.path.ends_with("日本語のノート.md"));
    }

    #[test]
    fn a_path_that_is_not_utf8_is_refused_rather_than_mangled() {
        // Converting lossily would let two different files collide on one
        // row, which silently merges two documents.
        #[cfg(unix)]
        {
            use std::ffi::OsStr;
            use std::os::unix::ffi::OsStrExt;
            let bad = PathBuf::from(OsStr::from_bytes(b"/notes/\xff\xfe.md"));
            assert!(matches!(
                store().record_document(&bad, None, 1),
                Err(StoreError::UnsupportedPath(_))
            ));
        }
        // Windows paths are UTF-16 and always convert, so there is nothing to
        // reject there; the guard exists for the platform that can produce one.
    }

    #[test]
    fn the_store_holds_no_document_content() {
        // ADR-0011: a title is a summary of what the user wrote, and even
        // that waits on a security profile. Nothing here takes a body.
        let store = store();
        let id = store
            .record_document(&path("secret.md"), Some("Title"), 1)
            .unwrap();
        store.tag_document(id, "tag").unwrap();

        let mut names = store
            .connection
            .prepare("SELECT name FROM pragma_table_info('documents')")
            .unwrap();
        let columns: Vec<String> = names
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        for forbidden in ["body", "content", "text"] {
            assert!(
                !columns.iter().any(|c| c == forbidden),
                "documents.{forbidden} would hold document content"
            );
        }
    }
}
