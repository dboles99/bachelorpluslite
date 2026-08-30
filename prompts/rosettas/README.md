# Rosettas

Prompts for the kinds of work this repository actually contains.

## The chain

Most work is one pass through the same four steps. Each has a short rosetta;
run them in order and stop at the first one that raises a question.

```text
R012 SCOPE      what is being built, and what it may not do
  -> R013 LIBRARY   the decision, in a bp-* crate, tested without a window
    -> R011 WIRE      connect it to the shell
      -> R014 LAND      gate, commit, record
```

**R012 and R014 are the ones that get skipped and should not be.** Scope
failures show up as a feature that quietly needed a decision nobody made;
landing failures show up as a green gate and a stale README.

Skip a step only when it is genuinely empty — R013 has nothing to do for pure
wiring, and R011 has nothing to do for a library-only change.

## The rest

Written for a kind of work rather than a step in the chain. Reach for these
when the task matches; otherwise use the chain.

| Rosetta | For |
| --- | --- |
| R001 | Designing a feature from scratch, where the architecture is in question |
| R002 | Semantic naming (`bp-naming`, ADR-0003) |
| R003 | Reviewing something for security consequences |
| R004 | Adding a format handler (`bp-formats`, `bp-data`) |
| R005 | **Deleted.** Notebook runners, phase 12 — both removed by [ADR-0057](../../docs/decisions/ADR-0057.md). The number is not reused: R006 to R011 keep theirs, because every ADR and task file that points at one points by number |
| R006 | Anything whose behaviour differs by platform |
| R007 | Research mode (phase 13) |
| R008 | Performance work with a measured baseline |
| R009 | Default-editor integration (phase 18, ADR-0012) |
| R010 | Brand and UI surface (ADR-0013) |

## Why they are short

R011 is 160 lines because every line of it was paid for by a debugging
session. The others are short because they are checklists, and a checklist
nobody finishes is worse than a shorter one they do.

If a rosetta grows past about 40 lines, the traps in it have become
documentation. Move them into the crate they belong to, where the person
changing that code will actually see them.
