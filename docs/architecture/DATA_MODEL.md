# Data Model

SQLite is the default local metadata store.

Proposed tables:

- documents
- document_versions
- projects
- tasks
- steps
- prompts
- rosettas
- artifacts
- artifact_metadata
- notes
- topics
- entities
- tags
- relationships
- semantic_metadata
- embeddings
- filename_candidates
- file_events
- recovery_sessions
- security_events
- signatures
- integrity_checks
- settings
- schema_migrations

Sensitive rows must inherit the owning document/project security policy.

**`notebook_cells` and `notebook_outputs` were on this list and are not any
more.** [ADR-0057](../decisions/ADR-0057.md) removed notebooks and execution,
and neither table had ever been migrated into existence -- `bp-storage`'s
schema never held them. They are deleted rather than struck through, because
a proposed table nothing proposes any longer is the trap `CLAUDE.md` numbers
3 written into a data model.
