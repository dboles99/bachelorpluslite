//! Schema migrations.
//!
//! Version is held in SQLite's own `user_version` pragma rather than in a
//! table of our own: it costs no query to read, it cannot be deleted by
//! accident, and it exists before any schema does -- which is precisely when
//! the first migration needs to ask.
//!
//! **Migrations are append-only.** Editing an applied one changes the schema
//! of new databases and leaves existing ones behind, with nothing to say they
//! disagree. A mistake gets a new migration that corrects it.

/// Every migration, in order. The index plus one is the version it produces.
pub(crate) const MIGRATIONS: &[&str] = &[
    // --- 1: documents, tags, and the join between them ------------------
    r"
    CREATE TABLE documents (
        id          INTEGER PRIMARY KEY,
        -- Absolute, and the identity of the row: the same file opened twice
        -- is one document.
        path        TEXT    NOT NULL UNIQUE,
        -- Semantic title, when something has extracted one. Nullable because
        -- an empty document has no title and pretending otherwise would put
        -- a filename where a title belongs.
        title       TEXT,
        -- Seconds since the epoch. Supplied by the caller rather than read
        -- from a clock here, so tests are not at the mercy of one.
        first_seen  INTEGER NOT NULL,
        last_seen   INTEGER NOT NULL
    );

    CREATE INDEX documents_last_seen ON documents (last_seen DESC);

    CREATE TABLE tags (
        id   INTEGER PRIMARY KEY,
        -- NOCASE so 'Rust' and 'rust' are one tag. A tag list that
        -- distinguishes them is a tag list nobody can use.
        name TEXT NOT NULL UNIQUE COLLATE NOCASE
    );

    CREATE TABLE document_tags (
        document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
        tag_id      INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
        PRIMARY KEY (document_id, tag_id)
    ) WITHOUT ROWID;

    CREATE INDEX document_tags_tag ON document_tags (tag_id);
    ",
];

/// The schema version this build produces.
pub fn latest_version() -> u32 {
    u32::try_from(MIGRATIONS.len()).unwrap_or(u32::MAX)
}
