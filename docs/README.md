# Documentation

**This directory is the source. Everything else is generated from it**
([ADR-0075](decisions/ADR-0075.md)).

```
docs/user/  docs/tutorials/  docs/reference/  docs/developer/
                          |
        +-----------------+-----------------+------------------+
        |                 |                 |                  |
  docs/generated/     app-help/           wiki/              site/
   one-file manual   in-app help       GitHub Wiki       the website
```

Do not edit `docs/generated/`, `app-help/` or `wiki/`. The gate regenerates
them and fails if they have drifted, so an edit in any of the three is a
change that will be reverted rather than kept.

| Build | With |
| --- | --- |
| `docs/generated/` | `./scripts/Build-Docs.ps1` |
| `app-help/` | `./scripts/Build-AppHelp.ps1` |
| `wiki/` | `./scripts/Sync-GitHubWiki.ps1` |
| all three, check only | `./scripts/Build-Docs.ps1 -Check` etc., or the gate |

## What lives where

| Directory | Holds | Audience |
| --- | --- | --- |
| `user/` | How to use the product, one topic per file | Somebody who has downloaded it |
| `tutorials/` | Start-to-finish walkthroughs | Somebody who has just opened it |
| `reference/` | Look-up material **that has no other home** | Somebody who knows what they want |
| `developer/` | Building, testing, releasing, architecture | Somebody changing the code |
| `generated/` | Built output. Not source | Nobody, directly |

The other documentation directories are not part of this pipeline and keep
their own jobs: `architecture/` owns crates, modules and seams; `decisions/`
owns one decision each; `product/MENU_MAP.md` owns every menu row and its
state; `governance/` owns the gate and the definition of done.

## The rule that shapes `reference/`

**Several facts a manual needs already have a home, and it is not prose.**
Those pages are generated from that home rather than retyped:

| Fact | Its one home | Page |
| --- | --- | --- |
| Every flag the binary accepts | `bp_config::cli::FLAGS` | `generated/reference/cli.md` |
| Every keyboard shortcut | `bp_ui::dispatch::SHORTCUTS` | `generated/reference/shortcuts.md` |
| Every menu row and its state | `docs/product/MENU_MAP.md` | `generated/reference/menus.md` |

A shortcut table typed into `reference/` would be a second spelling of a fact
the product already holds, and it would be wrong within a session.

**The question to ask before adding a page here: if this page and the code
disagreed, what would fail?** If the answer is *nothing*, and the code holds
the same fact, the page has to be generated instead.
