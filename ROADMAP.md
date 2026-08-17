# BachelorPad+ Full-Product Roadmap

This is not an MVP roadmap. Each phase contributes to the full v1 target.

Status as of **2026-08-17**. "Started" means a real, tested slice exists and
is wired into the application — not that the phase is finished.

| # | Phase | Status | Tasks | Crates |
| --- | --- | --- | --- | --- |
| 1 | Foundation and workspace | **Done** | [01-foundation](project/tasks/01-foundation/) | `bp-core`, `bp-config`, `bp-theme`, `bp-ui` |
| 2 | Core editor | **Started** | [02-core-editor](project/tasks/02-core-editor/) | `bp-buffer`, `bp-editor` |
| 3 | File safety and recovery | **Done** | [03-file-safety](project/tasks/03-file-safety/) | `bp-files`, `bp-history` |
| 4 | Large-file engine | Not started | [04-large-files](project/tasks/04-large-files/) | — |
| 5 | Format registry and parser framework | **Started** | [05-formats](project/tasks/05-formats/) | `bp-formats` |
| 6 | Structured formats | **Started** | [05-formats](project/tasks/05-formats/) | `bp-data` |
| 7 | Search engine | **Started** | [06-search](project/tasks/06-search/) | `bp-search` |
| 8 | Semantic foundation and naming | **Started** | [07-semantic](project/tasks/07-semantic/) | `bp-semantic`, `bp-naming` |
| 9 | Organization and related-note graph | Not started | [07-semantic](project/tasks/07-semantic/) | — |
| 10 | Semantic/hybrid search | Not started | [07-semantic](project/tasks/07-semantic/) | — |
| 11 | Clipboard system | **Started** | [08-clipboard](project/tasks/08-clipboard/) | `bp-clipboard` |
| 12 | Notebook/execution system | Not started | [09-notebook](project/tasks/09-notebook/) | — |
| 13 | Research mode | Not started | [10-research](project/tasks/10-research/) | — |
| 14 | Security foundation | Not started | [11-security](project/tasks/11-security/) | — |
| 15 | Encrypted `.bpadx` documents | Not started | [11-security](project/tasks/11-security/) | — |
| 16 | Advanced security | Not started | [11-security](project/tasks/11-security/) | — |
| 17 | Themes/personality/accessibility | **Started** | [12-themes-brand](project/tasks/12-themes-brand/) | `bp-theme` |
| 18 | Windows/Linux platform integration | Not started | [13-platform](project/tasks/13-platform/) | — |
| 19 | Hardening, fuzzing and benchmarks | Not started | [14-hardening](project/tasks/14-hardening/) | — |
| 20 | Packaging, signing, release and upgrade testing | Not started | [14-hardening](project/tasks/14-hardening/) | — |

## Two numbering schemes, reconciled

The phase numbers above are canonical — they are what `specs.md`, the ADRs and
the application's own "not implemented yet (phase N)" menu rows refer to.

`project/tasks/` uses fourteen directories rather than twenty, because several
roadmap phases share a task folder (all three security phases live under
`11-security`, for example). The mapping is the **Tasks** column. The
directories are deliberately *not* renumbered: they are a coarser grouping of
the same plan, not a competing one, and renaming them would break every link
that points at them for no gain.

## Where the work actually is

Phases 1 and 3 are complete. Phases 2, 5, 6, 7, 8, 11 and 17 each have a
tested slice in the product.

One thing gates more than anything else: **the custom editor view**. Slint's
`TextInput` owns its own text, caret and undo stack, which simultaneously
blocks the rope buffer becoming storage (phase 2), our own undo (phase 2), and
Ln/Col in the status bar. It is also the prerequisite for the large-file work
in phase 4. See `docs/architecture/ARCHITECTURE.md`.

The other structural gap is **security profiles** (phase 14). Two shipped
features already hold sensitive data — the recovery journal writes unsaved
text to disk in plaintext, and clipboard history keeps copied secrets in
memory — and ADR-0011 says the document's security profile should govern both.
Neither is wrong today; both are waiting on a subsystem that does not exist.
