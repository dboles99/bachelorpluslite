# BachelorPad+ Full-Product Roadmap

This is not an MVP roadmap. Each phase contributes to the full v1 target.

Status as of **2026-08-17**. Three words, meaning three different things:

- **Done** — the phase's work is finished.
- **Started** — a real, tested slice exists *and is reachable from the
  application*. Not that the phase is finished.
- **Built, unwired** — the library work exists and is tested, but nothing in
  the application calls it. Used where that is a deliberate decision rather
  than an unfinished job, and the reason is always recorded.

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
| 9 | Organization and related-note graph | **Built, unwired** | [07-semantic](project/tasks/07-semantic/) | `bp-storage` |
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
tested slice reachable in the product. Phase 9 has its foundation and no way
in, on purpose.

**The item that gated three sessions is no longer gating.** The rope is the
editor's storage, `bp-editor` owns caret, motion, undo and the line
operations, and a custom editor view exists behind `--editor-view`
(ADR-0018). Phase 4's large-file work is unblocked either way, because the
storage question is settled independently of which view draws.

What stands between that view and being the default is two parity items —
word wrap and input-method composition — and one thing no amount of code
fixes: **nobody has typed into it yet.** Its rules are covered by 147 tests
in `bp-editor`; the widget has been shown to render a frame without
panicking, and nothing more. Several phase-2 features shipped since depend on
that caret and inherit whatever is wrong with it.

**Security profiles (phase 14) are now the largest structural gap**, and they
have gone from two dependants to three. The recovery journal writes unsaved
text to disk in plaintext; clipboard history holds copied secrets in memory;
and `bp-storage` is built, tested and deliberately unreachable because the
useful thing to record — a document's extracted title — is a summary of what
the user wrote. ADR-0011 says the document's profile governs all three.
None of them is wrong today. The third is *waiting*, which is different from
the first two, which are *running*.
