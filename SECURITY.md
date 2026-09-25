# Security

## Reporting a vulnerability

**Please do not open a public issue.**

Use [GitHub's private vulnerability reporting](https://github.com/dboles99/bachelorpluslite/security/advisories/new)
— *Security → Report a vulnerability* on this repository. It is private to the
maintainer until an advisory is published.

Please include what you can of:

- what an attacker gets, and what they need first;
- the steps, or a file that triggers it;
- the version, from **Help ▸ Diagnostics** or `bachelorpad --version`;
- your platform.

You will get an acknowledgement within a week. This is a one-person project,
so a fix may take longer than that; you will be told where it stands rather
than left waiting. If you want credit in the advisory, say so and how you want
to be named.

Please give a reasonable window before publishing. There is no bounty.

## What is worth reporting

The threat model is small, because the product refuses most of what would
widen it:

- **it executes nothing** — no scripting, no macros, no cells, no
  interpreters ([ADR-0057](docs/decisions/ADR-0057.md));
- **it makes no network connection** — no telemetry, update check or crash
  reporting ([ADR-0006](docs/decisions/ADR-0006.md));
- **it launches no other program**, not even a browser for its own help;
- **it has no encryption**, since [ADR-0064](docs/decisions/ADR-0064.md)
  removed the `.bpadx` envelope, the audit log and signing.

So the things most worth reporting are:

| | |
| --- | --- |
| **A crafted document that does more than fail to open** | Memory unsafety, a panic that loses unsaved work, unbounded allocation. `fuzz/` exists for this and its coverage is not complete |
| **Anything written outside the expected directories** | Config, data, cache, state, and the file you asked to save. A path in a document that escapes them is a bug |
| **Document text reaching somewhere it should not** | A log line, a temp file, a recovery journal a privacy profile said not to write. There is a gate stage for the log case and it is not exhaustive |
| **Default-editor registration doing more than it showed you** | It writes a `.reg` or a `.desktop` file for you to apply. If the plan on screen and the file on disk differ, that is a serious bug |
| **A dependency advisory** we have not picked up. CI runs `cargo deny check` |

## What is not a vulnerability here

**That the archives are unsigned.** Deliberate and documented
([ADR-0055](docs/decisions/ADR-0055.md)): a certificate has not been bought,
and self-signing was refused outright because it is only satisfied once a user
installs a root certificate they have no reason to trust. Verify with
`SHA256SUMS.txt`, attached to every release.

**That a `.bpadx` file opens as unreadable bytes.** This build has no
decryption in it; the file is encrypted, and what you are seeing is literally
its contents. `.bpadx` is registered by no preset, which was itself a fix
([ADR-0069](docs/decisions/ADR-0069.md)).

**That a recovery journal contains your unsaved text.** That is what makes it
work. Whether one is written at all is a per-document privacy setting, and the
trade is stated in [Privacy](docs/user/11-privacy.md) rather than hidden.

**That old files are left in your profile.** Removing a feature does not
remove what it already wrote, and this product will not delete out of your
profile on your behalf ([ADR-0070](docs/decisions/ADR-0070.md)). They are
named so you can remove them yourself.

**That the notes index records paths and keywords.** Documented, and governed
by the same privacy setting. If it records something the setting said it would
not, *that* is a bug and a serious one.

## Supported versions

The latest release. This is a one-person project and there is no long-term
support branch; a fix goes into the next version.

| Version | Supported |
| --- | --- |
| 0.9.x | Yes |
| earlier | No |

## What this project does about security in general

- **No custom cryptography, ever.** Vetted primitives, composed
  ([ADR-0011](docs/decisions/ADR-0011.md)).
- **`#![forbid(unsafe_code)]`** in `bp-platform`, and no `unsafe` anywhere in
  the workspace's own code.
- **Panic lints on** — `unwrap_used`, `panic`, `todo`, `unimplemented` — as
  errors under the gate.
- **A log-hygiene gate stage**: a `tracing` macro naming an identifier that
  carries document text fails the build. Log *about* a document, never what is
  in it.
- **Hostile-input fuzzing** for the file and format readers, in `fuzz/`.
- **`cargo deny check`** in CI, over licences, advisories and sources.

And the honest part: **the seam between this product and its toolkit is where
the defects have been.** Five have lived there, including a signing passphrase
typed into the open document in plain text, which 1,812 passing tests did not
see. If you find something in that seam, it is exactly the kind of report this
project most needs.
