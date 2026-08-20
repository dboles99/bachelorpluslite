# Rosetta R012: Scope the Work

First step of the chain. Two minutes, and it is the step that stops a session
ending in a question.

## Answer these before writing anything

1. **What decides this?** Name the ADR or the specs.md section. If neither
   exists and the choice has consequences beyond one file, you are writing an
   ADR, not a feature.
2. **Does it need a decision only Daniel can make?** Check
   `project/DECISIONS_NEEDED.md`. If it does, **ask now, in a batch with
   everything else outstanding** — not when you reach it.
3. **Which editor path?** `TextInput`, `--editor-view`, or both. Anything
   touching the caret, the selection or the clipboard behaves differently in
   each (ADR-0018). "Both" is the answer that costs; "caret only" is the
   answer that greys a menu row.
4. **What is the failure the user sees?** An operation that can fail says so
   in the status bar. Decide the words now; they are part of the feature.
5. **What must this *not* do?** The line that turns Save Copy into Save As, or
   a security control into a lie. Write it down — it usually becomes a test.

## Then

- Pick a free action-id block if there is a menu row (`bp-ui/src/menus.rs`,
  `pub mod action`). Ranges are load-bearing; read the comments.
- Say which crate owns the decision. If the answer is `bp-ui`, think again:
  the shell connects, it does not decide.

## Stop here if

- a question from step 2 is unanswered;
- the work needs a dependency that is not already in the workspace — say so
  explicitly and give the reason, dependencies are ADR material;
- step 5 is empty. Nothing worth building has no constraints, so an empty
  answer means the work is not understood yet.
