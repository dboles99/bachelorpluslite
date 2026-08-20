# Security Model

**This file is the intent.** What actually ships against it is in the ADRs
named below and in `docs/product/MENU_MAP.md`; where the two differ, the ADR
is what the code does and this is what it is aiming at. Sections that have
been built say so.

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

**Built (ADR-0021, ADR-0022).** The envelope ships: Argon2id at the OWASP
baseline recorded per document, XChaCha20-Poly1305 or AES-256-GCM versioned in
the header, chunked from the start with the chunk index bound into the AAD.
The recovery journal is sealed with the document's own passphrase and read
back at unlock, so a strict profile keeps recovery rather than losing it.

Platform key protection is **not** built and is phase 18. Nothing in the
product uses DPAPI or Secret Service today.

## Signing keys

A signing key is the one file in this product that must stay secret and that
the product itself creates.

**Built (ADR-0031).** The key is an Ed25519 seed, sealed in the same `.bpadx`
envelope under a passphrase the user chooses, and kept in `DirKind::Data` —
the directory for things the product made and would rather not remake.

**Its contents are protected, not its permissions**, and that is the decision
rather than an implementation detail. ADR-0026 measured what permissions can
buy: `0600` on Linux, and on Windows nothing at all, because narrowing a DACL
needs `SetNamedSecurityInfo` and the `unsafe` that `bp-platform` forbids — so
the honest protection indicator reads "unknown" on half the supported
platforms. Sealing is identical on both. The file mode is still narrowed where
the platform allows it, because two locks are not worse than one.

What this does **not** protect against, unchanged from ADR-0026:

- the machine's administrator, on either platform;
- the key in memory once loaded — it is in zeroizing types and wiped on drop,
  but pinning it out of the page file needs `mlock`/`VirtualLock` and
  therefore `unsafe`;
- the previous key's blocks surviving a rotation on a journalling,
  copy-on-write or flash-backed filesystem. Those blocks now hold ciphertext,
  which is why this matters less than it did.

**A forgotten passphrase is a lost key.** Everything already signed still
verifies forever; nothing new can ever join it. That is a property of the
decision, and it belongs in front of the user when the key is created rather
than being discovered later.

## Notebook execution

Never auto-run:
- opened notebook
- pasted code
- restored executable cell

Execution UI must show interpreter, environment and working directory.
