# Database

BachelorPad+ plans to use SQLite for local metadata.

Rules:
- migrations are append-only and numbered
- no schema changes without a migration
- sensitive data inherits security profile
- no plaintext secret fixtures
- tests use disposable databases
