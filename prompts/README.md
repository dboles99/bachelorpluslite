# Prompts and Rosettas

Rosettas are reusable implementation/research instruction packets. They should be specific enough to produce traceable work but generic enough to reuse.

Naming convention:

```text
R<number>_<DOMAIN>_<PURPOSE>.md
```

## Which one to use

- **R001** — designing a feature that does not exist yet.
- **R011** — connecting a capability that already exists in a `bp-*` crate to
  the application. This is the shape most available work has now, and it is
  where the codebase-specific traps live.

`project/WORK_QUEUE.md` lists what is ready to take, and which items can run
in parallel without colliding.
