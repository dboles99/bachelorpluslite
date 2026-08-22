# BachelorPad+ Project Memory

**This file is the *original* brief, kept as written.** It records what was
asked for before anything was built, which is why it is worth not editing:
several of its items have since been decided differently, and the value is in
being able to see that.

**Every decision made since lives in [DECISIONS.md](DECISIONS.md)**, which
indexes 49 ADRs. Where the two disagree, the ADR is current. The most visible
example is in "Suggested implementation style" below: Slint was "a leading
candidate to be benchmarked"; it was benchmarked and chosen
([ADR-0015](docs/decisions/ADR-0015.md)).

[CLAUDE.md](CLAUDE.md) is the entry point for working here.

## Identity

Project: **BachelorPad+**
Repository folder target: `G:\dev2\bachelorpadplus_rust`
Language: Rust
Primary target OS: Windows 10, Windows 11, Linux

## Product intent

BachelorPad+ is not merely a Notepad clone. It is a fast text appliance that remains simple for ordinary note-taking while exposing deeper capabilities progressively.

Core phrase:

> Notepad when you want it. More when you need it.

## Locked user decisions

1. Target Windows 10, Windows 11 and Linux.
2. Use Rust as the primary language.
3. Default filename pattern is:
   `Semantic Title_DDMMMYYYY.ext`.
4. Creation date is the default filename date and does not change on every save.
5. Status bar shows last successful save and full path.
6. Support Markdown, YAML, JSON, JSONL/NDJSON, CSV/TSV, TOML, XML, common source formats and Jupyter notebooks.
7. Support notebook-style execution, including mixed-language cells and run-selection.
8. Support local semantic organization, semantic naming, related notes, duplicate detection and hybrid search.
9. Include all proposed Rust-oriented performance features: rope buffer, memory mapping, huge-file mode, parallel processing, safe/atomic save, file watching, recovery journals, structural undo and inspection tools.
10. Include Light, Dark, Organic and Green themes.
11. Include clipboard-history access in the editor context menu, plus an optional BachelorPad+ clipboard system.
12. Include full security/cryptography feature set: `.bpadx`, passphrases, Argon2id, AEAD, DPAPI/Hello on Windows where appropriate, Linux keyring services where appropriate, encrypted recovery/history, security profiles, privacy mode, secure clipboard, secret scanning, hashing, signatures, redaction, audit history and semantic privacy controls.
13. Allow BachelorPad+ to be set as the user's default text editor on Windows 10/11 and Linux using supported operating-system mechanisms.
14. Do not replace or patch Windows `notepad.exe`.
15. Branding is an original retro-futurist consumer-appliance style: humorous, chunky, industrial, exaggerated convenience, but no copied characters, logos, names, fictional companies, props, typography, dialogue, or distinctive protected assets.
16. Project governance should use:
   `project -> task -> step -> prompts/rosettas -> artifacts -> metadata -> database -> note`
   where logical and useful.

## Architecture guardrails

- Core editing never depends on AI/cloud services.
- Cloud semantic services are optional.
- Security policy constrains recovery, clipboard, history, metadata, embeddings and network use.
- Physical file renames/moves require user approval.
- No custom cryptography.
- Notebook code never auto-runs.
- Cross-platform behavior lives in shared crates; platform-specific behavior is isolated behind traits/adapters.
- Plain files remain standard files unless encryption or a BachelorPad-specific notebook/storage feature requires otherwise.

## Suggested implementation style

- Cargo workspace.
- Slint is a leading UI candidate, but final UI toolkit should be benchmarked/prototyped before irreversible commitment.
- SQLite for local metadata.
- `bp-*` crate naming.
- Strong tests, fuzzing and benchmark baselines from early phases.
