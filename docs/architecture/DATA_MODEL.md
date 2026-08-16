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
- clipboard_items
- notebook_cells
- notebook_outputs
- security_events
- signatures
- integrity_checks
- settings
- schema_migrations

Sensitive rows must inherit the owning document/project security policy.
