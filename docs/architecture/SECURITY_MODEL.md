# Security Model

## Security profiles

- Standard
- Private
- Confidential
- Maximum
- Custom

## Propagation requirement

If a document is protected, the policy must cover:

- primary file
- autosave
- recovery journal
- revision history
- clipboard persistence
- semantic metadata
- embeddings
- temporary files
- audit events
- cloud/network eligibility

## Encrypted format

Extension: `.bpadx`

Portable encryption:
- passphrase
- Argon2id or reviewed equivalent KDF
- established AEAD algorithm/library
- versioned envelope
- authenticated metadata handling

Platform key protection:
- Windows: DPAPI / Windows Hello-assisted workflows where appropriate
- Linux: Secret Service / desktop keyring integrations where appropriate

## Notebook execution

Never auto-run:
- opened notebook
- pasted code
- restored executable cell

Execution UI must show interpreter, environment and working directory.
