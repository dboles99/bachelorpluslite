<!--
  Thank you. Two things before you fill this in:

  1. CONTRIBUTING.md explains why a pull request here needs a DCO sign-off
     (`git commit -s`). There is a second, proprietary BachelorPad+, and your
     contribution stays GPL-3.0-only unless you separately say otherwise.
     Nobody will push you to.

  2. If this touches crates/bp-ui/, read
     prompts/rosettas/R011_WIRE_CAPABILITY_INTO_SHELL.md first. Ten traps,
     each of which has cost somebody a session.
-->

## What this changes, and why

<!-- Behaviour, not files. What could somebody do after this that they could not before? -->

## What you ran, and what it said

<!--
  Paste the gate's summary. Do not truncate a failure: its detail is *above*
  the summary, which is why CLAUDE.md forbids piping the gate through `tail`.
-->

```
./scripts/Invoke-LocalCI.ps1 -Linux
```

## What you checked by hand

<!--
  Five defects have lived in the seam between this product and its toolkit,
  including a signing passphrase typed into the open document in plain text.
  1,812 passing tests did not see it. If this change is anywhere near that
  seam, say what you clicked.

  "Nothing -- this is library-only" is a fine answer.
-->

---

- [ ] Commits are signed off (`git commit -s`) — see [CONTRIBUTING.md](../CONTRIBUTING.md)
- [ ] `./scripts/Invoke-LocalCI.ps1 -Linux` is green on **both** legs
- [ ] `cargo fmt --all` was run before the gate
- [ ] The record moved with the code — README, ROADMAP, MENU_MAP or an ADR, whichever this touches
- [ ] Any new claim in a comment has something that would fail if it stopped being true
- [ ] No new dependency, or the reason is stated above and `cargo deny check licenses` passes
- [ ] No new way to execute anything, reach the network, or launch a program
- [ ] Documentation regenerated if `docs/` changed (`Build-Docs.ps1`, `Build-AppHelp.ps1`, `Sync-GitHubWiki.ps1`)
