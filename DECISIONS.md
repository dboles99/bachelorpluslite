# BachelorPad+ Decision Log

This file is the human-readable decision register. Every row has a formal ADR
in `docs/decisions/` under the same number: `BP-ADR-0007` is `ADR-0007.md`.
Adding a decision means adding both.

| ID | Date | Decision | Status | ADR |
| --- | --- | --- | --- | --- |
| BP-ADR-0001 | 2026-08-16 | Target Windows 10, Windows 11 and Linux | Accepted | [ADR-0001](docs/decisions/ADR-0001.md) |
| BP-ADR-0002 | 2026-08-16 | Rust is the primary implementation language | Accepted | [ADR-0002](docs/decisions/ADR-0002.md) |
| BP-ADR-0003 | 2026-08-16 | Default semantic filename uses `Title_DDMMMYYYY.ext` | Accepted | [ADR-0003](docs/decisions/ADR-0003.md) |
| BP-ADR-0004 | 2026-08-16 | Creation date is the default filename date | Accepted | [ADR-0004](docs/decisions/ADR-0004.md) |
| BP-ADR-0005 | 2026-08-16 | Status bar exposes last successful disk save and full path | Accepted | [ADR-0005](docs/decisions/ADR-0005.md) |
| BP-ADR-0006 | 2026-08-16 | Core editing must work without semantic/cloud features | Accepted | [ADR-0006](docs/decisions/ADR-0006.md) |
| BP-ADR-0007 | 2026-08-16 | Add full Rust performance/safety feature set | Accepted | [ADR-0007](docs/decisions/ADR-0007.md) |
| BP-ADR-0008 | 2026-08-16 | Add structured formats and Jupyter-style notebook capability | Accepted | [ADR-0008](docs/decisions/ADR-0008.md) |
| BP-ADR-0009 | 2026-08-16 | Add Light, Dark, Organic and Green themes | Accepted | [ADR-0009](docs/decisions/ADR-0009.md) |
| BP-ADR-0010 | 2026-08-16 | Add clipboard history and format-aware clipboard behavior | Accepted | [ADR-0010](docs/decisions/ADR-0010.md) |
| BP-ADR-0011 | 2026-08-16 | Add comprehensive security and cryptography subsystem | Accepted | [ADR-0011](docs/decisions/ADR-0011.md) |
| BP-ADR-0012 | 2026-08-16 | Use OS-supported default-editor registration, never patch Notepad | Accepted | [ADR-0012](docs/decisions/ADR-0012.md) |
| BP-ADR-0013 | 2026-08-16 | Use original retro-futurist appliance branding without copying existing IP | Accepted | [ADR-0013](docs/decisions/ADR-0013.md) |
| BP-ADR-0014 | 2026-08-16 | Use project→task→step→prompts/rosettas→artifacts→metadata→database→note governance | Accepted | [ADR-0014](docs/decisions/ADR-0014.md) |
| BP-ADR-0015 | 2026-08-17 | Adopt Slint as the UI toolkit | Accepted | [ADR-0015](docs/decisions/ADR-0015.md) |
| BP-ADR-0016 | 2026-08-17 | CI is local-first; GitHub Actions is dormant | Accepted | [ADR-0016](docs/decisions/ADR-0016.md) |
| BP-ADR-0017 | 2026-08-17 | Default to the software renderer; split the startup target | Accepted | [ADR-0017](docs/decisions/ADR-0017.md) |
| BP-ADR-0018 | 2026-08-17 | Write a custom editor view; make the rope the storage; ship the view opt-in | Accepted | [ADR-0018](docs/decisions/ADR-0018.md) |
| BP-ADR-0019 | 2026-08-17 | SQLite via bundled `rusqlite`; append-only migrations; metadata never content | Accepted | [ADR-0019](docs/decisions/ADR-0019.md) |
| BP-ADR-0020 | 2026-08-19 | Security profiles resolve to a policy; named profiles are monotonic | Accepted | [ADR-0020](docs/decisions/ADR-0020.md) |
| BP-ADR-0021 | 2026-08-19 | `.bpadx`: two AEADs versioned in the envelope, chunked, position authenticated | Accepted | [ADR-0021](docs/decisions/ADR-0021.md) |
| BP-ADR-0022 | 2026-08-19 | The recovery journal is sealed with the document's passphrase and recovered at unlock | Accepted | [ADR-0022](docs/decisions/ADR-0022.md) |
| BP-ADR-0023 | 2026-08-19 | YAML uses `saphyr`; nesting, alias expansion and duplicate keys are bounded before a tree exists | Accepted | [ADR-0023](docs/decisions/ADR-0023.md) |
| BP-ADR-0024 | 2026-08-19 | An audit event is `Copy`, so it cannot carry a secret; the log's destination resolves from the existing policy axes | Accepted | [ADR-0024](docs/decisions/ADR-0024.md) |
| BP-ADR-0025 | 2026-08-19 | Notebook kinds ride in namespaced `.ipynb` metadata; a run needs a `UserGesture` no parsed file can produce | Accepted | [ADR-0025](docs/decisions/ADR-0025.md) |
| BP-ADR-0026 | 2026-08-19 | A `.sig` sidecar appends to the whole file name and records the signer's key; Windows enforces no key-file permission | Accepted | [ADR-0026](docs/decisions/ADR-0026.md) |
| BP-ADR-0027 | 2026-08-19 | Large-file thresholds are measured; the line index is sparse; memory mapping is declined with reasons | Accepted | [ADR-0027](docs/decisions/ADR-0027.md) |
| BP-ADR-0028 | 2026-08-19 | Redaction merges overlapping and adjacent spans, resolves every offset against the original, and states what verification cannot prove | Accepted | [ADR-0028](docs/decisions/ADR-0028.md) |
| BP-ADR-0029 | 2026-08-20 | A line break is `
` or `
`; the rope drops `unicode_lines` | Accepted | [ADR-0029](docs/decisions/ADR-0029.md) |
| BP-ADR-0030 | 2026-08-20 | A huge document opens in the custom surface, read-only, in every build | Accepted, shipped | [ADR-0030](docs/decisions/ADR-0030.md) |
| BP-ADR-0031 | 2026-08-20 | A signing key lives in a sealed `.bpadx` key file, not a permission-protected one | Accepted, shipped | [ADR-0031](docs/decisions/ADR-0031.md) |
| BP-ADR-0032 | 2026-08-20 | The application id is reverse-DNS, and is not the directory name | Accepted, shipped | [ADR-0032](docs/decisions/ADR-0032.md) |
| BP-ADR-0033 | 2026-08-21 | OpenAI's API is the vetted destination for content that leaves the machine | Accepted | [ADR-0033](docs/decisions/ADR-0033.md) |
| BP-ADR-0034 | 2026-08-21 | Default-editor registration stays a `.reg` handoff, never a `winreg` dependency | Accepted, shipped | [ADR-0034](docs/decisions/ADR-0034.md) |
| BP-ADR-0035 | 2026-08-21 | The `.bpadx` validation ceiling is lowered to 256 MiB / 16 iterations | Accepted | [ADR-0035](docs/decisions/ADR-0035.md) |
| BP-ADR-0036 | 2026-08-21 | A long security history stays usable by reading it lazily, not by a faster key | Accepted | [ADR-0036](docs/decisions/ADR-0036.md) |
| BP-ADR-0037 | 2026-08-21 | Phase 9 is a related-notes panel and duplicate detection | Accepted | [ADR-0037](docs/decisions/ADR-0037.md) |
| BP-ADR-0038 | 2026-08-21 | Notebook mode is a lightweight, one-cell-at-a-time utility | Accepted | [ADR-0038](docs/decisions/ADR-0038.md) |
| BP-ADR-0039 | 2026-08-21 | Research mode turns gathered data into design recommendations, offline, on its own data model | Accepted | [ADR-0039](docs/decisions/ADR-0039.md) |
| BP-ADR-0040 | 2026-08-21 | `bp-execution` runs a cell as a fresh subprocess, three languages first | Accepted, shipped | [ADR-0040](docs/decisions/ADR-0040.md) |
| BP-ADR-0041 | 2026-08-21 | Research mode's data is `bp-storage`'s, not a new store | Accepted, shipped | [ADR-0041](docs/decisions/ADR-0041.md) |
| BP-ADR-0042 | 2026-08-21 | Find in a document served from disk is resumable, not threaded | Accepted, shipped | [ADR-0042](docs/decisions/ADR-0042.md) |
| BP-ADR-0043 | 2026-08-21 | Notebook mode arrives as a Run menu over a literate document, not a cell view | Accepted, shipped | [ADR-0043](docs/decisions/ADR-0043.md) |
| BP-ADR-0044 | 2026-08-21 | The Research menu is what a citation crate can honestly do offline | Accepted, shipped | [ADR-0044](docs/decisions/ADR-0044.md) |
| BP-ADR-0045 | 2026-08-21 | A Markdown file is a notebook, and the panel that lists things is one component | Accepted, shipped | [ADR-0045](docs/decisions/ADR-0045.md) |
| BP-ADR-0046 | 2026-08-22 | Research mode's last five planned rows were a paper's structure; two fold into the report, one becomes Open Questions, one becomes What the Store Holds, one is dropped | Accepted, shipped | [ADR-0046](docs/decisions/ADR-0046.md) |
| BP-ADR-0047 | 2026-08-22 | An agent gets standing authorities; one home per kind of fact; pre-push checks for a full gate rather than repeating it; nextest with a doctest stage | Accepted, shipped | [ADR-0047](docs/decisions/ADR-0047.md) |
| BP-ADR-0048 | 2026-08-22 | Every menu row either works or is a readout; ten built, eleven deleted, `planned_menu` and `arrives` removed | Accepted, shipped | [ADR-0048](docs/decisions/ADR-0048.md) |
| BP-ADR-0049 | 2026-08-22 | The window is driven by a script; a clipped menu label is elided and shortened; a temp path a recycled pid can reuse is not unique | Accepted, shipped | [ADR-0049](docs/decisions/ADR-0049.md) |
| BP-ADR-0050 | 2026-08-22 | D4's envelope review is prepared as a brief; the format had no test that a document written by an earlier build still opens | Accepted, shipped | [ADR-0050](docs/decisions/ADR-0050.md) |
| BP-ADR-0051 | 2026-08-22 | The Run menu denied there was anything to run while offering to run it; `every_menu()` never held the Run menu; the window driver photographed the wrong window again | Accepted, shipped | [ADR-0051](docs/decisions/ADR-0051.md) |
| BP-ADR-0052 | 2026-08-22 | Enter twice in the find box replaced the match with a line break; `preview_match` becomes `reveal` and is the default | Accepted, shipped, confirmed at the keyboard | [ADR-0052](docs/decisions/ADR-0052.md) |
| BP-ADR-0053 | 2026-08-22 | `main` advances by pull request; opening one is an agent's job, merging is not. D13 closed after six sessions | Accepted | [ADR-0053](docs/decisions/ADR-0053.md) |
| BP-ADR-0054 | 2026-08-23 | The command line becomes an interface: `--help`, `--version`, `--line=N`, a reported typo, and the licences every manifest already claimed | Accepted, shipped, confirmed at the window | [ADR-0054](docs/decisions/ADR-0054.md) |
| BP-ADR-0055 | 2026-08-23 | Signing is deferred and self-signing refused outright; the archive, the checksums and a one-command signing step are built anyway. D15 closed | Accepted | [ADR-0055](docs/decisions/ADR-0055.md) |
| BP-ADR-0056 | 2026-08-23 | All three embedding sources, as choices, with the profile as a ceiling and `Cloud` behind a per-use gesture — and the axis for them had existed since ADR-0020. D16 closed | Superseded by [ADR-0082](docs/decisions/ADR-0082.md) | [ADR-0056](docs/decisions/ADR-0056.md) |
| BP-ADR-0057 | 2026-08-29 | Executing anything is removed, and notebooks with it. Neither crate depended on the other; one shell module was the whole seam. ADR-0011's "never auto-runs" becomes vacuous rather than enforced | Accepted | [ADR-0057](docs/decisions/ADR-0057.md) |
| BP-ADR-0058 | 2026-08-29 | `metadata/repository_manifest.json` is deleted rather than regenerated: 102 of its 181 hashes were wrong, 275 tracked files were never in it, and nothing read it. Git already content-addresses the tree | Accepted | [ADR-0058](docs/decisions/ADR-0058.md) |
| BP-ADR-0059 | 2026-08-29 | The reduction is scoped: eight crates and huge-file mode leave in five ADRs, least-entangled first, 67,316 lines to ~40,000. `bp-security` is **kept and narrowed** -- it is the store's off switch, and four of its seven policy axes turned out to have no enforcing reader at all | Accepted | [ADR-0059](docs/decisions/ADR-0059.md) |
| BP-ADR-0060 | 2026-08-29 | R1: `bp-research` leaves with the three Research rows that read the document. The menu keeps its name and its three store-reading rows -- ADR-0044's refusal to merge them is what made this a deletion rather than a rewrite | Accepted | [ADR-0060](docs/decisions/ADR-0060.md) |
| BP-ADR-0061 | 2026-08-29 | R2: `bp-clipboard` leaves, and the `Clipboard` policy axis goes with it rather than waiting for R5 -- an axis outlives its subject by nothing. Four tests rewritten; one would have passed while asserting nothing | Accepted | [ADR-0061](docs/decisions/ADR-0061.md) |
| BP-ADR-0062 | 2026-08-29 | R3: `bp-data` and the Data menu leave, superseding ADR-0023. `sniff` survives and loses the only thing that could contradict it -- the cross-crate agreement test that once caught it calling a pretty-printed array "JSON Lines" | Accepted | [ADR-0062](docs/decisions/ADR-0062.md) |
| BP-ADR-0063 | 2026-08-30 | R4: huge-file mode leaves -- the engine, the viewer and `StreamSearch`, superseding ADR-0027, ADR-0030 and ADR-0042. No directory and no id block: a *mode* comes out as branches. D11 becomes inert and D2's own re-ask metric moves | Accepted | [ADR-0063](docs/decisions/ADR-0063.md) |
| BP-ADR-0064 | 2026-08-30 | R5, the last: five security crates leave and `bp-security` stays as Privacy. `Recovery::Encrypted` is deleted rather than degraded to plaintext; `.bpadx` documents become unopenable with no migration; the Security menu is renamed for what four profiles and a toggle actually are | Accepted, amended | [ADR-0064](docs/decisions/ADR-0064.md) |
| BP-ADR-0068 | 2026-08-30 | P2: the icon ships beside the executable and everything points at it there -- which removes the build-time resource dependency the row was sized around rather than adding it. Both halves of ADR-0012's registration had been naming an icon that did not exist | Accepted | [ADR-0068](docs/decisions/ADR-0068.md) |
| BP-ADR-0067 | 2026-08-30 | P5: there is no installer. An unsigned one is worse than none (ADR-0055), winget needs a public URL a private repo has not, and ADR-0012 forbids the one job left -- registering file types. The archive is the delivery mechanism | Accepted | [ADR-0067](docs/decisions/ADR-0067.md) |
| BP-ADR-0069 | 2026-08-30 | `.bpadx` leaves the registration table, `TypeGroup::Own` leaves the enum, and the test exemption that let an unopenable type be registered leaves with them. Every preset -- including Notepad Replacement -- was claiming a file type this build opens as ciphertext | Accepted | [ADR-0069](docs/decisions/ADR-0069.md) |
| BP-ADR-0070 | 2026-08-30 | Removing the code that writes a file does not remove the file. Leftover state -- a frozen `security-history.log`, a `recent.toml` at a location abandoned when the list stopped roaming -- stays where it is and is named rather than swept, because a product that will not seize a file association does not get to delete out of `%APPDATA%` either | Accepted | [ADR-0070](docs/decisions/ADR-0070.md) |
| BP-ADR-0066 | 2026-08-30 | R1 answered: read-only is the save refusing, and always was. `bp_buffer::Access` guarded every edit against a condition nothing has ever set -- one production call site, in the crate ADR-0063 deleted. The refusal now stats the file rather than listing three candidate causes | Accepted | [ADR-0066](docs/decisions/ADR-0066.md) |
| BP-ADR-0065 | 2026-08-30 | Two of the reduction's three costs were *concurrent* rather than inherent, and are repaired: `sniff` gets a test-only parser that can contradict it again, and Private keeps an honestly-labelled plaintext journal. The third -- ADR-0050's golden vectors -- is inherent and stays paid | Accepted | [ADR-0065](docs/decisions/ADR-0065.md) |
| BP-ADR-0071 | 2026-09-10 | **GPL-3.0-only**, and the `-only` is the decision. Every manifest claimed `MIT OR Apache-2.0` while the binary statically links fourteen `i-slint-*` crates whose tri-licence this repository had never elected -- ADR-0054 asked whether there was a *text* behind the claim and nobody asked whether the claim was *true*. Slint grants version 3 and no other, so `-or-later` would offer terms this project has not been granted. Four readers now: `--version`, About, `deny.toml`, and a generated `THIRD-PARTY-NOTICES.md` the gate checks | Accepted | [ADR-0071](docs/decisions/ADR-0071.md) |
| BP-ADR-0072 | 2026-09-10 | **macOS is declined**, with what it would cost written down. It would *compile* today -- `Platform::HOST` reports anything non-Windows as Linux -- and would then put config in `~/.config` and write `.desktop` files nothing reads. A third platform rather than a third build, plus an Apple certificate before Gatekeeper would open it, plus no Mac here to check any of it on | Accepted | [ADR-0072](docs/decisions/ADR-0072.md) |
| BP-ADR-0073 | 2026-09-10 | **Hosted CI is restored.** ADR-0016 rested on "GitHub Actions is not available to this project" and one `gh api` call disproved it -- trap 6, and the second time. The local gate stays authoritative; Actions judges pull requests from strangers, builds a release on a machine that has never built it (trap 7), and runs the advisory database the local leg cannot assume a network for | Accepted, amends ADR-0016 | [ADR-0073](docs/decisions/ADR-0073.md) |
| BP-ADR-0074 | 2026-09-10 | **The product is `BachelorPad+ Lite`**, and the rename found the name spelled out at *eight* sites while `DISPLAY_NAME`'s doc comment said three and a green test proved a pair agreed. The window title was a `.slint` literal no Rust test could see -- the exact site the previous rename broke. All eight read the constant now, and three tests ask at three of the sites rather than at the constant | Accepted | [ADR-0074](docs/decisions/ADR-0074.md) |
| BP-ADR-0075 | 2026-09-10 | **One documentation source, four destinations, nothing retyped.** `docs/` is the source; `docs/generated/`, `app-help/` and `wiki/` are generated and gate-checked for staleness. The half worth the ADR: three reference pages are generated from *code*, because the flag list, the shortcuts and the menu map already have a home there. In-app help opens **as a document**, never a browser, because this product launches no programs | Accepted | [ADR-0075](docs/decisions/ADR-0075.md) |
| BP-ADR-0076 | 2026-09-10 | **The website is static and collects nothing except the waitlist.** No analytics, no cookies, no third-party scripts -- enforced by a CSP of `self` and a build step that greps for a tracker, because a promise on a web page is trap 3 exactly as a comment in Rust is. The waitlist promises features and no dates and no prices, from the list ADR-0057 and ADR-0059--0064 removed | Accepted | [ADR-0076](docs/decisions/ADR-0076.md) |
| BP-ADR-0077 | 2026-09-10 | **Reporting a problem composes a document**, because this product cannot open a browser (ADR-0057) or reach the network (ADR-0006). Help > Report a Problem pre-fills the issue template with the diagnostics already in it and puts the URL on the clipboard. The constraint produced the *better* feature: the fields that make a report fixable are the ones somebody filing in a browser would have gone back for and mostly would not | Accepted | [ADR-0077](docs/decisions/ADR-0077.md) |
| BP-ADR-0078 | 2026-09-10 | **Three unmaintained dependencies accepted by name, never as a category.** The first hosted CI run failed on three RustSec advisories the local gate had never been in a position to see, which is the case ADR-0073 made for hosted CI, demonstrated the same day. All three are `unmaintained` rather than `vulnerability`, and all three arrive through Slint. Ignored by id with a reason each, because `unmaintained = "warn"` would silence the next one too, and the next one is the one nobody has looked at | Accepted | [ADR-0078](docs/decisions/ADR-0078.md) |
| BP-ADR-0079 | 2026-09-10 | **The fuzz harness silences the panic hook, because the hang budget was timing it.** `catch_unwind` does not return until the hook has finished, so with `RUST_BACKTRACE=1` a hosted Windows runner spent the whole 20 second budget symbolising a backtrace and reported an instant panic as a hang, discarding the message. A quiet hook for probe threads only; every other thread keeps the one it had. Raising the budget was rejected -- twenty seconds is not too short, the budget was measuring the wrong thing | Accepted | [ADR-0079](docs/decisions/ADR-0079.md) |
| BP-ADR-0080 | 2026-09-25 | **The Insert key overtypes, and two surfaces stop mishandling named keys.** Found by driving the window under Xvfb with a marker after every key: Page Up and Page Down did nothing in the default surface, because `TextInput` pages by a `page-height` that defaults to zero and nothing set it; and in `--editor-view` F5, Insert and every unnamed key typed a private-use character into the document. Overtype's rule lives in `bp-editor` and both surfaces ask it -- the line break is never overtyped, a run undoes in one step. Conceded: two Ctrl+Z per character under `TextInput`, and caret offsets Slint marks internal | Accepted | [ADR-0080](docs/decisions/ADR-0080.md) |
| BP-ADR-0081 | 2026-09-25 | **Two of the three Slint blockers were never blocked.** Checked against both 1.17.1 and 1.18.1 source: input-method composition is still refused by `FocusScope`. But `StyledText` has been public since Slint 1.15, with `StyledText::from_markdown` at runtime, so D14's premise was false the day it was answered; and a dropped file reaches the application through `slint::winit_030::WinitWindowAccessor::on_winit_window_event` without the backend, on Windows and X11 though not native Wayland. The record is corrected, drag and drop is queued, and D14 goes back to Daniel as a question | Accepted | [ADR-0081](docs/decisions/ADR-0081.md) |
| BP-ADR-0018 | amended 2026-09-26 | Typing from `TextInput` is mirrored into the rope without history: each report was an undo step holding the document twice, 200 KB a key at 100 KB, on a stack no key could reach. Replace All and the line operations -- Sort Lines and the rest -- are undone from the rope instead, which the widget could not do once Slint began clearing its history on an outside change (W1-05) | Accepted, amended | [ADR-0018](docs/decisions/ADR-0018.md) |
| BP-ADR-0048 | amended 2026-09-25 | Its answer to D14 rested on *"Slint 1.17.1 has no rich-text item"*, which was false -- `StyledText` shipped in 1.15. The answer stands withdrawn and the question is open again ([ADR-0081](docs/decisions/ADR-0081.md)) | Accepted | [ADR-0048](docs/decisions/ADR-0048.md) |
| BP-ADR-0082 | 2026-09-25 | **Phase 10 is declined, and four policy axes nothing enforced leave with it.** `Cloud` embeddings contradict a product published as having no network, a `Local` model puts weights on a Notepad's startup path, and the need is met by Find ▸ In Folder and Related Notes. Removing `embeddings` meant counting readers: `network`, `temporary_files` and `zeroise` had none but the Security Inspector, which told a Confidential document *"Temporary files: never written"* while every save wrote one. ADR-0059 had decided all four would go; ADR-0064 deferred it on the ground that a doc comment admitted it. `Policy` is two axes, and `zeroize` leaves the graph. Confidential and Maximum are now visibly the same policy, which they always were -- Daniel's question | Accepted | [ADR-0082](docs/decisions/ADR-0082.md) |
| BP-ADR-0083 | 2026-09-25 | **Slint 1.18.1, because 1.17.1 could not draw past line 2,000.** Answers D21. A debug build overflowed in euclid past about 2,100 lines, or 1,100 at scale 2, under the default software renderer; a release build would have wrapped the coordinate and drawn the document in the wrong place, silently. 1.18.1 passes at 5,000 lines and scale 2, checked by frame as well as log. The software renderer stays the default, ADR-0080's internal caret properties still compile, and the Xvfb harness now fails any run whose log contains a panic | Accepted | [ADR-0083](docs/decisions/ADR-0083.md) |
| BP-ADR-0084 | 2026-09-25 | **Questions are asked in the window, and a question nobody answered keeps the work.** Answers D22. On Linux every rfd message dialog runs `zenity`, and without it returns `Cancel` -- which the recovery prompt read as "discard the journal", and the close handler as "keep the window" forever. Message dialogs move to Slint on both platforms; only an explicit No discards; `rfd`'s file pickers stay, with their `zenity` fallback named as the one program the product can start. Not yet built (W1-02) | Accepted | [ADR-0084](docs/decisions/ADR-0084.md) |
| BP-ADR-0085 | 2026-09-25 | **Legacy encodings are read, and saved back as they came.** Answers D23. The manual called UTF-8-only "a decision", and no ADR had made it; a refused file is not protected, it is opened in Notepad. BOM, then UTF-8, then a `chardetng` guess shown in the status bar; `encoding_rs` to decode and encode; save writes the encoding the file came in; UTF-16 joins Format > Encoding. Two new dependencies named. Not yet built (W2-04) | Accepted | [ADR-0085](docs/decisions/ADR-0085.md) |
| BP-ADR-0086 | 2026-09-25 | **The release candidate ships unsigned, and 1.0 is signed through SignPath or the Store.** Answers D24. EV stopped buying SmartScreen reputation in 2024, before ADR-0055 said it did -- the third accepted ADR found on a false external fact. The repository is public and winget takes a portable ZIP, so ADR-0067's winget premise has gone too. Every signature now starts a reputation clock; only the Store makes the first download silent | Accepted | [ADR-0086](docs/decisions/ADR-0086.md) |
| BP-ADR-0087 | 2026-09-25 | **The Linux build runs on glibc 2.35 and later.** Answers D25. `ubuntu-latest` linked a hard GLIBC_2.39 requirement from std's weak `pidfd_*` references, so the binary would not start on Ubuntu 22.04, Debian 12 or Mint 21. Build on `ubuntu-22.04` and fail the release above 2.35; RHEL and Debian 11 declined until somebody asks. Not yet built (W3-01) | Accepted | [ADR-0087](docs/decisions/ADR-0087.md) |
| BP-ADR-0088 | 2026-09-25 | **The release candidate is reachable by keyboard and names its controls; 1.0 is tested with a screen reader.** Answers D26. Zero `accessible-*` properties and a menu bar no keyboard can reach. RC: Alt/F10 into the menus, and roles and labels on menus, tabs and the find and go-to inputs. 1.0: an NVDA and Orca pass, a high-contrast palette, and a contrast test over every theme. Not yet built (W4-02) | Accepted | [ADR-0088](docs/decisions/ADR-0088.md) |
| BP-ADR-0089 | 2026-09-25 | **The website claims only what ships, and names no date.** Answers D27 and D28. "Everything Notepad does", "nothing is written outside that folder" and "uninstalling is deleting the folder" were false, and the installation page contradicted its own table; all corrected in six languages and at the source. The 10 October countdown is removed rather than moved: a later date would be the same promise with the same lack of a plan | Accepted | [ADR-0089](docs/decisions/ADR-0089.md) |
| BP-ADR-0090 | 2026-10-01 | **A document is saved in its own line ending, unless its file arrived mixed (W1-06).** Save wrote every break through unchanged, so Enter -- which the text widget always types as `\n` -- left every edited CRLF file mixed, and Format > LF / CRLF converted nothing while a comment said it did. A uniform, new or restored document is now saved in its declared convention; a file mixed when opened is preserved until a convention is chosen, and choosing one converts. Save a Copy agrees. Six tests on byte vectors committed first | Accepted | [ADR-0090](docs/decisions/ADR-0090.md) |
| BP-ADR-0087 | built 2026-10-01 | **Built (W3-01).** Releases and CI build Linux on `ubuntu-22.04`, and a release fails when `objdump -T` finds a glibc symbol above 2.35. The check first passed when `objdump` failed -- a pipeline exits with its last stage, and an empty version sorts below every floor -- and now refuses an empty answer. The image is already deprecated: unsupported from 2027-04-17, so the choice is re-made before then | Accepted | [ADR-0087](docs/decisions/ADR-0087.md) |
| BP-ADR-0091 | 2026-10-01 | **The Lite edition takes its own identity before 1.0.** Answers ADR-0074's deferral: `APP_ID` becomes `io.github.dboles99.bachelorpluslite` (Flathub maps it to this repository), ProgIDs `BachelorPadPlusLite.*`, key `Software\BachelorPad+ Lite`; the old names are the full product's, and the guard now refuses them. Registration gains a UTF-16 removal script that also clears the 0.9.5 names, and the window sets its xdg app id so a Linux dock finds its `.desktop` file. Building it found that the test `DISPLAY_NAME`'s doc comment cited had never existed, while four dialogs said BachelorPad+; it exists now and failed against the old code | Accepted | [ADR-0091](docs/decisions/ADR-0091.md) |
| BP-ADR-0032 | amended 2026-10-01 | The application id is `io.github.dboles99.bachelorpluslite`, not the full product's name; the reverse-DNS reasoning stands ([ADR-0091](docs/decisions/ADR-0091.md)) | Accepted, amended | [ADR-0032](docs/decisions/ADR-0032.md) |
| BP-ADR-0092 | 2026-10-01 | **A large file says typing may lag, and names the view that does not.** Answers D29. `TextInput` re-lays out the whole document per key -- about 80 ms at 100 KB -- and `--editor-view`, which does not, cannot be the default without input-method composition. From 512 KiB the status bar says so at open and names the flag; nothing is refused. Found in passing: command-line files were opened before `--editor-view` was applied | Accepted | [ADR-0092](docs/decisions/ADR-0092.md) |
| BP-ADR-0085 | built 2026-10-01 | **Built (W2-04).** Windows-1252, Shift-JIS, GBK and unmarked UTF-16 open, and save back as they came; a character the encoding cannot hold refuses the save by name and line. Format > Reopen As shipped too, because the first short file tried was guessed as Windows-1250 -- honestly. Not foreseen: code pages with two codes for one character (Shift-JIS's NEC and IBM kanji) would have saved back changed, so a legacy file whose decode does not re-encode to itself is refused. Five vectors from Python's codecs, all failing first | Accepted | [ADR-0085](docs/decisions/ADR-0085.md) |
| BP-ADR-0093 | 2026-10-01 | **An installer for Windows, and a package for each kind of Linux.** Supersedes ADR-0067, whose case was against a *per-machine* installer: Inno Setup's per-user mode never elevates, so unsigned it costs what the zip already does. The installer's optional Open with task is generated from `bp-platform`'s own registration plan, and a test fails when they differ. .deb and .rpm are built with `dpkg-deb` and `rpmbuild` because the binary dlopens almost everything it needs -- the list was read from it with `strings` -- and an AppImage tool fetched at build time is refused unless its digest matches. Flatpak and the AUR are written, not published | Accepted | [ADR-0093](docs/decisions/ADR-0093.md) |
| BP-ADR-0094 | 2026-10-01 | **1.0 ships unsigned, and untested with a screen reader.** Daniel asked for 1.0.0 rather than a release candidate; signing (SignPath, the Store) and the NVDA/Orca pass move to a 1.x release, and accessibility is still claimed nowhere | Accepted | [ADR-0094](docs/decisions/ADR-0094.md) |
| BP-ADR-0067 | superseded 2026-10-01 | There is an installer after all: its premises were a per-machine install and an unanswered Linux question ([ADR-0093](docs/decisions/ADR-0093.md)) | Superseded | [ADR-0067](docs/decisions/ADR-0067.md) |
| BP-ADR-0086 | amended 2026-10-01 | 1.0.0 ships unsigned; the routes stand and wait for a 1.x release ([ADR-0094](docs/decisions/ADR-0094.md)) | Accepted, amended | [ADR-0086](docs/decisions/ADR-0086.md) |
| BP-ADR-0088 | amended 2026-10-01 | The screen-reader pass moves to a 1.x release; the claim stays unmade until it happens ([ADR-0094](docs/decisions/ADR-0094.md)) | Accepted, amended | [ADR-0088](docs/decisions/ADR-0088.md) |
| BP-ADR-0084 | built 2026-09-25 | **Built (W1-02).** Every question is drawn in the window and answered through a continuation, since a Slint dialog cannot block; only an explicit Discard or Don't Save destroys anything, and that rule is three tested functions. Building it found the journal keyed by document ids that restart at 1 in every run, so each launch's clean Untitled deleted the last run's `1.json` within five seconds -- hidden while `rfd` blocked, fatal once the question waited. The journal now writes per run. `scripts/drive-recovery-xvfb.sh` passes on both surfaces and fails against the build before | Accepted | [ADR-0084](docs/decisions/ADR-0084.md) |
| BP-ADR-0084 | amended 2026-10-01 | **A session still running is not left behind (W1-03).** `left_behind` offered every other session, a live one included, so a second window asked to recover the first one's unsaved work. Each journal now holds an exclusive lock on `<session>.lock` from its first checkpoint, and a session is offered only once its lock can be taken -- the operating system releases it however the process ends, so no process id is compared and none can be reused. `std`'s `File::try_lock`; no dependency. A restored document is stamped, so Save asks before overwriting a file changed since the crash; the journal is `0700`/`0600` on Unix | Accepted, amended | [ADR-0084](docs/decisions/ADR-0084.md) |
| BP-ADR-0088 | built 2026-09-26 | **Release-candidate half built (W4-02).** F10 opens the menus and the arrows, Enter and Esc work them, from a click as well; menus, rows, tabs, the find bar, Go to Line and the dialogs are named. F10 rather than a bare Alt, because on X11 an Alt+Tab away arrives as Alt pressed and released alone. AccessKit 0.19 publishes no expanded or disabled state over AT-SPI, so greyed rows say "unavailable" in words. The AT-SPI tree is read in a container by `scripts/check-accessibility-xvfb.sh`; nothing has been spoken yet, which is the 1.0 half | Accepted | [ADR-0088](docs/decisions/ADR-0088.md) |
| BP-ADR-0055 | amended 2026-09-25 | Its EV row -- reputation *immediately* -- was false when written; Microsoft removed the bypass in 2024. The deferral stands; EV is off the table ([ADR-0086](docs/decisions/ADR-0086.md)) | Accepted, amended | [ADR-0055](docs/decisions/ADR-0055.md) |
| BP-ADR-0057 | amended 2026-09-25 | "Executes nothing" had an unrecorded exception: `rfd` runs `zenity` on Linux. The message dialogs are moving into the window; the file pickers' fallback is named ([ADR-0084](docs/decisions/ADR-0084.md)) | Accepted, amended | [ADR-0057](docs/decisions/ADR-0057.md) |
| BP-ADR-0067 | amended 2026-09-25 | Its winget premise -- a private repository, no release artefacts -- is gone, and winget needs no installer. No installer still stands ([ADR-0086](docs/decisions/ADR-0086.md)) | Accepted, amended | [ADR-0067](docs/decisions/ADR-0067.md) |
| BP-ADR-0076 | amended 2026-09-25 | The countdown and the 10 October date are withdrawn; the feature list stays ([ADR-0089](docs/decisions/ADR-0089.md)) | Accepted, amended | [ADR-0076](docs/decisions/ADR-0076.md) |
| BP-ADR-0076 | amended 2026-09-25 | Only `main` deploys the site. Pull requests each got a staging environment nothing closed, the Free tier's quota filled by PR #6, and every later PR touching `site/` or `docs/` went red at a step its diff could not fix. PRs are still built and checked | Accepted, amended | [ADR-0076](docs/decisions/ADR-0076.md) |
| BP-ADR-0081 | amended 2026-09-25 | Its "no upgrade in this change" was overtaken the same day by the long-document overflow ([ADR-0083](docs/decisions/ADR-0083.md)); its three answers stand | Accepted, amended | [ADR-0081](docs/decisions/ADR-0081.md) |
| BP-ADR-0017 | amended 2026-08-22 | Half of the renderer revert condition is now a number rather than a feeling: per-frame row building, and its independence from document size | Accepted, amended | [ADR-0017](docs/decisions/ADR-0017.md) |

## Decisions needed before the work they block

- ~~**Where a signing key lives**~~ (phase 16). **Answered and built:
  ADR-0031.** The key is stored *sealed*, in the `.bpadx` envelope ADR-0021
  already ships, under a passphrase the user chooses -- contents protected
  rather than permissions. That closes the asymmetry ADR-0026 measured and
  could not fix: `0600` on Linux and nothing on Windows, where narrowing a
  DACL needs Win32 and `unsafe`. The platform keyring stays specs.md section
  15's third route and becomes a second *source of the passphrase* rather than
  a second key format. Security ▸ Sign Document acts.

  **It was the last row greyed for a missing decision rather than a missing
  prerequisite**, and it is worth recording how long that took: three
  sessions, with the reason in its own label the whole time. Nothing about the
  work was hard once the question was answered. That is the argument
  `project/DECISIONS_NEEDED.md` exists to make.
- **What may leave the machine** (phase 8 layer three, phase 10). Generative
  providers and embeddings both imply sending document content somewhere.
  ADR-0006 keeps them optional; ADR-0011 governs what is permitted. Neither
  says which providers are acceptable.
- **What `bp-storage` records, and what the user sees for it** (phase 9).
  Narrowed twice and no longer a yes/no. ADR-0020 settled the security half --
  `record_document` honours the policy, dropping the title under `PathOnly`
  and recording nothing under `Disabled` -- and the product half is answered:
  it is wired **after** a design pass, not before. ADR-0019 stands exactly as
  written. What is still open is the design, which is a question about the
  product rather than about the crate: a related-notes panel, duplicate
  detection, or something else. Recording something before deciding what it is
  for is how a metadata store becomes a liability.
- **specs.md section 22's warm-start target** (75 ms) is still unverified —
  the software renderer's time to first interaction cannot be measured, so
  half of ADR-0017's target has no number behind it.

## What the undo stacks taught, 2026-09-26

Four things, from W1-05, which set out to stop the widget's undo stack
growing and to fix a panic on Ctrl+Z, and found that neither was quite what
the plan said.

### Half a defect can be fixed by an upgrade nobody connected to it

The plan asked for the widget's history to be cleared whenever Rust replaced
its text. **Slint 1.18.1 already does it** -- `TextInput::align_to_text`,
upstream issue 9024 -- and W1-01 took that version for a different defect.
Driven at the window, the build before W1-01 panics on Ctrl+Z in a new tab
and the build after does not. The record went on listing the defect because
nothing asked. **"Verify first" is the whole of the instruction**, and it
cost two drive runs; the fix it would have replaced was a workaround for
something no longer broken.

And the upgrade had a price the record did not name either: with the
widget's history cleared, **Replace All and every line operation could not
be undone at all** on the default surface, while the manual said Replace
All "is a single undo entry". A fix changes what else is true, and the
sentences about the neighbours are the ones nobody rereads.

### The stack that grew was the one nobody read

ROADMAP named "the widget's own undo stack growing without bound". The
widget's stack is small; it keeps what was typed. **What grew was
`bp-editor`'s**, recording every report as an undo step holding the document
twice -- on the default surface, where Ctrl+Z never reaches it. Memory spent
on something nothing reads fails no test, because every test reads. **Ask of
each thing that is kept: what reads it?** If nothing does, it is either dead
or wrong, and here it was both: an undo from that stack would have replayed
offsets from a document the widget had since changed.

### A guard that declines must say who goes instead

`focus-editor-soon` declines while the find bar is open, so the caret never
leaves a find box -- correct, and tested. Closing Replace All's question
called it, it declined, and nothing else took the caret: every key after the
answer went nowhere, Ctrl+S included, until a click. **A function that says
"not me" has handed the decision to nobody** unless it says who. The
question's close now names the bar behind it. Found only because a drive
case needed a key after Replace All.

### A probe must prove its input arrived

The first memory probe passed. It typed with `xdotool type`, which never
reached the window, and measured an editor that had been sent nothing. The
second typed with `xdotool key` and still passed, because a debug build takes
about a second per key at 100 KB and the reading was taken while the keys
were still queued. **A measurement of nothing looks exactly like a good
result.** The probe now saves and waits until the file holds every key it
sent, and only then reads the memory -- so the probe that passes is also the
proof that the typing happened.

## What the offsets taught, 2026-09-26

One thing, from W1-04, where Find and Go to Line selected the wrong text in
any non-ASCII document and the next key typed replaced it.

### A unit mismatch hides in every fixture that is all ASCII

Every range in this workspace is in characters; `TextInput` counts bytes.
In ASCII the two are the same number, so every test, every fixture and
every manual pass agreed with the code -- and `bp-search`'s own header said
its offsets were "handed straight to the editor", which read as the reason
they were right. A Japanese fixture broke it with one keystroke. **Where two
components meet, find out what unit each side counts in, and test with data
on which the units disagree**: for text, one multi-byte character before the
thing selected. `drive-window-xvfb.sh` takes `FIXTURE_LINE` for exactly this.

## What the screen reader's tree taught, 2026-09-26

Two things, from reading the accessibility tree instead of the markup
([ADR-0088](docs/decisions/ADR-0088.md)).

### Read what the reader is handed, not what you wrote

Every `accessible-*` property was reviewed before the tree was read, and the
tree still found four defects: line numbers announced before the first word,
"vertical line" before every status readout, each button's own text read a
second time, and a dialog whose focus was the window rather than its default
button. None was a wrong property; each was a *missing* one on an element
nobody thought of as speaking. **The markup says what was labelled; only the
tree says what is heard.** `at-spi2-core`, a private bus and forty lines of
`pyatspi` produce it anywhere Xvfb runs -- no screen reader, no person.

### A toolkit's mapping is a second author

Slint accepted `accessible-expanded` and `accessible-enabled`, and AccessKit's
AT-SPI layer then dropped both. Nothing warned. **A property the code sets is
a request, and the platform tree is the answer** -- which is trap 3 in
another form, and the same test answers it: read the far end. Where a state
cannot survive the crossing, say it in words that can.

## What the dialogs taught, 2026-09-25

Three things, from taking the product's questions away from `rfd`
([ADR-0084](docs/decisions/ADR-0084.md)).

### A failure that comes back looking like an answer is a decision somebody made

`rfd` without `zenity` returns `Cancel`. The recovery prompt matched `Yes`
and put the discard in the `else`, so a dialog that never appeared answered
*delete the work* -- and nothing between the two had done anything wrong on
its own terms. **Where a branch destroys something, match the answer that
asks for it and nothing else.** The `else` of a destructive `if` is where a
failure goes to be read as consent.

### Make something stop blocking, and ask what it used to prevent

The dialog had to become non-blocking, and that let the rest of the program
run while a question waited. The rest of the program had been relying on not
running: the five-second checkpoint pass discarded the journal of every clean
document, and every launch opens a clean Untitled as document 1 -- the same
name as the last run's unsaved document 1. The collision had been there all
along; blocking was the only thing keeping it from running first. **A
blocking call guards more than its caller, and nothing records what.** Before
making one asynchronous, list what used to be impossible while it waited.

### An id that restarts every run is not a name for anything that outlives it

Document ids were fine as keys inside a run, and the journal, which exists
precisely to outlive a run, used them anyway. Two instances open at once had
the same defect from the other side. The fix was not a better id; it was
naming the run. Anything written to disk under a counter should say whose
counter it was.

## What the keyboard taught, 2026-09-25

Three things, all from pressing keys nobody had listed as worth pressing
([ADR-0080](docs/decisions/ADR-0080.md)).

### A default of zero is a feature switched off, and nothing reports it

Page Up and Page Down did nothing in the default surface for the whole life of
the product. `TextInput` pages by `page-height`, returns `false` when that is
not taller than a line, and defaults it to zero. A refusal from inside a
toolkit looks exactly like a key the product never claimed, so no test and no
log could see it. **Five manual passes missed it because none of them pressed
Page Down** -- a checklist of features does not list the keys everybody
assumes work. When a toolkit property has a default, ask what the feature does
at that default.

### A comment that names a failure mode is not a test for it

The test beside `translate_key` said a mismatch "would type invisible glyphs
into the document", and asserted only the keys already on its list. F5 was
not on it, and F5 typed U+F708 into the saved file. The comment was right
about the failure and the test asked about the wrong keys: **it checked the
ones that were handled, and the defect lived in the ones that were not.** The
replacement sweeps the whole private-use block Slint borrows, which is the
question the comment was actually asking. This is trap 3 from a new side: the
claim was true, and still nothing would have failed if it stopped being.

### Suspect the probe, and here is a new way the probe lies

The first undo probe typed a marker straight after Ctrl+Z and read as data
loss. Slint re-selects what an undo restores; the marker replaced it. The
defect vanished when the marker moved to the end of the line, because it had
never been one. `project/NEXT_SESSION.md` already said to suspect a manual pass
before the product; the same holds for a scripted one, and **a probe that
types is an edit, with every consequence an edit has.**

## What re-checking the toolkit taught, 2026-09-25

### A check answers the question it was asked

Drag and drop and Markdown preview were each recorded as *blocked on Slint,
checked in its source*, and both checks were honest. The drag-and-drop one
proved that the winit **backend** had no file-drop plumbing, which was true,
and concluded that the **application** had no route, which was not -- winit's
events reach it directly ([ADR-0081](docs/decisions/ADR-0081.md)). The preview
one looked for rich text where `Text` lives and did not find it, while
`StyledText` had been public for seven months. **Each check looked where the
author expected the feature to be.** The habit that catches both is cheap:
before recording a dependency as blocking something, grep its public API for
the noun -- `StyledText`, `winit_window_event` -- rather than for the
mechanism you had in mind. And recheck against the *old* version as well as
the new one, or a blocker that never existed reads as one that lifted.

## What declining phase 10 taught, 2026-09-25

### A true comment does not make a false screen honest

ADR-0064 had the facts: it named the four policy axes nothing enforced, and
left them in place because *"the doc comment on `security_inspector_report`
names which three govern nothing, so the readout is not a claim the code fails
to keep."* The readout was exactly such a claim. A Confidential document's
owner opened Tools ▸ Security Inspector and read *"Temporary files: never
written"* and *"buffers are overwritten when a document closes"*, and neither
was true. **A claim is kept or broken where it is read**, and a doc comment is
read by a developer while a readout is read by the person the claim is about.
Trap 3 is about a comment claiming something of the code; this is its mirror,
a comment conceding something the product still claims.

### Take the inventory before the recommendation, too

The plan that opened this session recommended *lexical search over the note
store* as phase 10's replacement. One `CREATE TABLE` would have shown that the
store holds paths, titles and tags and never content, so it could not have
searched what the recommendation assumed. `WORK_QUEUE.md`'s rule --
*take the inventory before the estimate* -- applies to a recommendation as
much as to a queue item. The need turned out to be met already.

## What the gate taught, 2026-09-25

### A check that reads the clock expires

`Build-Site.ps1` wrote `(Get-Date)` into every sitemap's `<lastmod>`, and the
gate's `site` stage passes only when a fresh build equals the committed one.
So the stage passed on 2026-09-10, the day the site was generated, and **failed
on every day after it** -- on every branch, whatever the change. Nothing about
the site had moved; the date had. The rule `bp-storage` states for itself --
*time is a parameter, never a clock* -- applies to a generator whose output is
checked, and applies harder: a test that reads the clock fails once, while a
generator that does makes the check it feeds expire.

The value was also false. A `lastmod` of *today* on every build claims that
every page changed on every build, which is the one date that is never true.
It is gone rather than derived from `git log`, because the commit that
regenerates a sitemap is the commit that moves that date.

### A script that falls off its end reports someone else's exit code

`Test-Site.ps1` printed *"site/ passes all three checks"* and was reported as
failed, because it ended without `exit 0` and `$LASTEXITCODE` still held the
failure of the stage before it. One red became two, and the second one lied
about the site. A PowerShell script the gate reads through `$LASTEXITCODE`
must say how it ended.

### And a host leg that names `.exe` is a Windows leg

The `launch` stage could not start on a Linux host at all -- it named
`bachelorpad.exe` unconditionally -- and `-Linux` on a Linux host called
`wsl`, which does not exist there, as a terminating error *after* every stage
had passed: thirteen passes and exit 1. Both were true for as long as the gate
only ever ran on Windows, and stopped being true the first time a session ran
it from Linux, which is what this repository's cloud sessions do. On a Linux
host the native stages are the Linux leg, and the other leg is hosted CI.

### A rename reaches the parsers, and nothing ran the parsers

When the product became *BachelorPad+ Lite* ([ADR-0074](docs/decisions/ADR-0074.md)),
the first line of `--version` gained a space. `release.yml`, `release-linux.sh`
and `New-Release.ps1` all read that line as *name, then version* by taking
the second field -- so every release script read the version as `Lite`, refused
it as not semver, and **no release of 0.9.5 could be built on either
platform**. ADR-0074 hunted the name through eight sites that *spell* it and
missed three that *parse* it, and no stage of any gate runs a release script,
so nothing failed until an outside review of PR #4 read the regex. The shape
the scripts rely on -- last field the version, everything before it the name
-- is now a test in `bp_config::cli`, because a contract that only a script
reads is one that nothing checks.

### And a comment promised the manual's links worked

`Build-Docs.ps1` said "links between pages become links within the
document". Nothing did it: every page was pasted in with its relative links
untouched, and the manual published at `/docs` on the website sent 97 ADR
links and every cross-page link to a 404. Trap 3, in a generator -- and a
generator's claim is worse than a function's, because its output is checked
against itself and so cannot disagree with it. The rewrite now happens, and
the build refuses a manual with any relative file link left in it.

## What going public taught, 2026-09-10

Six decisions in one session, and **five of the six were found by preparing to
publish rather than by anything that could fail**. That pattern is worth more
than any of the individual findings.

### A licence claim with a text behind it is not a licence claim that is true

[ADR-0054](docs/decisions/ADR-0054.md) found `MIT OR Apache-2.0` in every
manifest with neither licence file present, and fixed it properly: correct
SPDX expression, both texts committed, the string threaded into `--version`.
The repository then had a licence claim that looked thoroughly handled.

**Looking thoroughly handled is what kept anybody from noticing it described a
fraction of the binary.** Slint is tri-licensed, the archives shipped no Slint
notice at all, and the election had never been made anywhere.

> **"Is there a text behind this claim?" and "is this claim true?" are
> different questions, and answering the first one well is what stops anybody
> asking the second.**

The check is not about licences. **Name the thing the claim is about, then go
and look at that thing.** The claim was about a 21 MB executable, and nobody
had looked inside it.

### A test that proves two things agree is evidence about two things

`DISPLAY_NAME`'s doc comment says the product name "must not be spelled out at
the three places that show it", and records that the previous rename broke
exactly that. The test written in response --
`version_names_the_product_the_desktop_registration_names` -- has passed every
run since, and the comment says why: *both halves of that pair read this*.

**Which is the whole defect.** Renaming to BachelorPad+ Lite found the name
written out at **eight** sites, including the window title in `app.slint` --
the site the previous rename actually broke, and the one no Rust test can see.

> **Before trusting a constant as the one home for a value, grep for the
> value. If the count is higher than the number of readers, the constant is a
> convention rather than a mechanism.**

Counting is the check. Testing was not: the test was green throughout.

### An accepted ADR is where a false premise goes to be safe

[ADR-0016](docs/decisions/ADR-0016.md) opens *"GitHub Actions is not available
to this project"*, and everything in it follows from that sentence. One
`gh api` call returns `{"enabled": true}`.

It was almost certainly true when written. It became false silently, and a
decision document has no mechanism that would ever say so. **This is trap 6,
and it is the second time** -- D13 sat five sessions on a premise one
`git merge-base` disproved. The habit was installed for questions in
`DECISIONS_NEEDED.md` and not for premises inside decisions already accepted,
which read as settled and are therefore never re-read looking for something to
disprove.

> **An ADR whose reasoning rests on an external fact should name the command
> that would test it.**

### A rule does not follow the product onto a new surface

[ADR-0006](docs/decisions/ADR-0006.md) is about the application, and every
crate honours it. A website is a different artefact, on different
infrastructure, in a different language -- and the default for that artefact,
the thing every template ships with, is an analytics tag.

Nothing would have failed. No test covers a website and the gate never sees
one.

> **Ask of each new surface which of this project's existing rules it is now
> the exception to, and answer in writing before the surface exists -- because
> afterwards it is a change rather than a decision.**

The answer here was a CSP of `self` plus a build step that greps for trackers,
so that the promise has a reader.

### The dangerous unsupported platform compiles

macOS was declined ([ADR-0072](docs/decisions/ADR-0072.md)), and the reason is
not effort. `Platform::HOST` reports any non-Windows target as Linux -- a
deliberate, well-argued fallback for a target nobody ships.

Ship one, and it becomes a working editor that puts files in the wrong place
and reports a successful file-type registration that has done nothing. Both
findable only by somebody sitting at a Mac.

> **A target that refuses to build announces itself. One that compiles and
> answers as something else does not, and nothing in the code marks the
> boundary between those two states.**

### A gate pinned to one machine ages with that machine

Hosted CI failed on its first run for two reasons, and the second was not one
ADR-0073 had anticipated. `clippy::chunks_exact_to_as_chunks` fired in
`bp-files` on the runners and fires nowhere on this workstation, because the
local toolchain is 1.97.1 and `dtolnay/rust-toolchain@stable` takes whatever
stable is that morning.

The local gate is thorough, and it had been green on both legs minutes
earlier. **It cannot report a lint it does not have.**

> **A gate that runs only where it was written cannot tell you the world has
> moved.** New lints, new advisories, and a dependency that stops compiling on
> a newer compiler all arrive from outside the repository, and nothing inside
> it changes when they do.

The fix was one line and better code -- `as_chunks::<2>()` hands back real
arrays instead of slices that must be indexed back into one -- which is the
usual shape of a new lint. That is not the point. The point is that nobody
here would have seen it until a contributor did, and a contributor's first
experience of the project would have been a red build they did not cause.

### Two failures in the same red are not the same failure

`cargo deny` reports an unmaintained crate and a vulnerable one identically:
same `error`, same colour, same exit code. Both symmetric responses are wrong.
Treat them all as blocking and you cannot ship over a crate that works and
simply has nobody watching it; treat them all as noise and one day you ship
over a real one.

Of the three this project accepted
([ADR-0078](docs/decisions/ADR-0078.md)), two have no runtime surface worth
worrying about and the third parses fonts, which are untrusted input. That is
not a distinction a category setting can make.

> **Ask what would have to be true for this to hurt somebody, and answer it
> per item rather than per category.**

And the smaller, more repeatable half: **when a tool offers to silence a
category, enumerate the instances instead.** The category setting is a
statement about things nobody has seen yet, and there are no grounds for one.

### A constraint can be a design brief

"In-app feedback" almost always means a button that opens a browser. Two
existing decisions made that impossible: this product launches no programs
([ADR-0057](docs/decisions/ADR-0057.md)) and makes no network connection
([ADR-0006](docs/decisions/ADR-0006.md)). The natural response is to record
the feature as blocked.

What the refusal actually forced was the question *what makes a bug report
useful?* The answer is the diagnostics, which were already a menu row away and
which most people filing in a browser would never have gone back for. So the
version this product is allowed to build delivers better reports than the one
it is not.

> **A constraint that forecloses the obvious implementation is worth reading
> as a design brief rather than as an obstacle** -- and the check is to ask
> what the feature is *for*, once its usual shape is unavailable.

### An absence here is not evidence the thing does not exist

The website's signup was designed from scratch, with a page of reasoning about
why holding no personal data is safest. `dboles99/af-site` sits on the same
domain, is owned by the same person, and had solved the identical problem
three days earlier: Azure Function, Table Storage, Kit, GDPR consent
versioning this version had not thought about. It also had the locale
directory shape, the sitemap naming, `llms.txt`, the PayPal handle and the
deploy configuration.

The tell was available and ignored: **the domain was already in use.**
`bpad.prompt-forge.dev` is a subdomain of a site that exists, and asking what
the parent site does would have produced the whole answer.

> **Trap 6 says a claim in the record is not a property of the repository.
> This is its neighbour: an absence in *this* repository is not evidence that
> the thing does not exist.** Before designing a mechanism, look for the one
> already running next door.

### And the documentation version of trap 3

The first design of the documentation pipeline had a
`docs/reference/shortcuts.md` in it: a table of keyboard shortcuts typed out
beside a constant that already held them, with nothing to make the two agree.
It would have been correct on the day it was written.

> **If this page and the code disagreed, what would fail?** For a page
> describing what a tab is *for* -- nothing, and prose is the right home. For
> a page listing the flags -- nothing either, which is precisely why that page
> has to be generated instead.

Three pages are generated from code now. Asking the question of the existing
documentation found `MENU_MAP.md`'s Help section carrying **two rows for one
menu**, one a subset of the other, coexisting in the file whose entire job is
to be the one home for what each menu holds.

### And the stopwatch version of it

The fuzz harness gave every input a twenty second budget and called anything
past it a hang. On a hosted Windows runner it reported `Hang { budget: 20s }`
for a probe whose entire body is `panic!("the message")`.

`catch_unwind` does not return until the panic hook has finished. The hook
runs inside the window the parent is timing, and with `RUST_BACKTRACE=1` it
was symbolising a backtrace against the PDBs of a large debug binary. So the
harness was timing itself reporting on the input, and calling the number a
property of the input ([ADR-0079](docs/decisions/ADR-0079.md)).

> **A timeout measures everything inside it, including the code that reports
> the result.** The comment above the constant said "a slow machine under a
> cold cache is not a defect. Anything past this is not slow, it is stuck."
> Nothing about reading it would have found the error, because the sentence is
> about the input and the budget was not.

The fix that was *available* was to raise the number, and it would have turned
the run green. It was rejected for the reason worth keeping: twenty seconds is
not too short. **The budget was measuring the wrong thing, and a bigger number
measures the wrong thing for longer.**

And the half that keeps recurring: **the local gate is not the hosted gate.**
Twice in one day the difference was the whole point --
[ADR-0078](docs/decisions/ADR-0078.md) found three advisories a machine with
no network can never see, and this found a cost that only appears on a machine
with cold symbols. [ADR-0073](docs/decisions/ADR-0073.md) argued for hosted CI
on the grounds that it would see things the local gate cannot. It has now done
so twice before anyone downloaded a release.

## Recently closed, and what each one cost to learn

Moved here from `README.md` on 2026-08-22. It was 3,586 words -- roughly
two thirds of that file -- and every word of it was a *lesson*, which this
file owns. README now carries the one-line facts and links here, per the
ownership table in `CLAUDE.md`.

**Why the split matters more than the tidiness:** the same lesson was being
written into three or four files per session and read out of all of them the
next, which is exactly how a fact gets updated in three places and left stale
in the fourth.

Kept rather than deleted, because every one of these went stale the same
way — a fix landing without the record moving — and because the lesson in each
is worth more than the fact.

- **A decision is often already made by the decisions around it, and an
  estimate is often already invalidated by them.** Two queue rows fell in one
  afternoon and neither needed new information.

  P5 asked for *"an installer, or a decision that there is not one"*. The
  answer came from two ADRs that were already in the record:
  [ADR-0055](docs/decisions/ADR-0055.md) refused an unsigned trust gesture,
  and [ADR-0012](docs/decisions/ADR-0012.md) refused a silent file
  association. Between them an installer is left with one Start Menu shortcut
  to justify itself, so there is not one
  ([ADR-0067](docs/decisions/ADR-0067.md)).

  P2 then read: *"Windows takes the icon from the executable, which needs a
  build-time resource -- a new dependency, so an ADR."* True when written.
  **ADR-0067 invalidated it an hour before anyone took the row**: with no
  installer to place an icon in a theme, the icon travels in the archive, and
  a registry `DefaultIcon` takes any path. The dependency was not needed at
  all ([ADR-0068](docs/decisions/ADR-0068.md)).

  **The row still read as authoritative because it was written down in a
  table**, which is the same tell as the five research rows ADR-0046 deleted
  and the 38 "planned" menu rows ADR-0048 found. The habit: before taking a
  sized row, check whether anything decided since has moved the constraint it
  names.

- **Driving the window takes the keyboard from whoever is at the machine, and
  the contaminated run looks exactly like a clean one.** `Drive-Window.ps1`
  already guarded the case it knew about -- `SetForegroundWindow` failing
  silently, so keys land somewhere else -- and that guard is useless against
  the reverse: the *right* window is in front, and the keystrokes arriving are
  a person's rather than the script's.

  On 2026-08-30 a manual pass ran seven launches while Daniel was using the
  desktop. Nine characters he typed were captured into the document under
  test, reached the recovery journal, and **a defect was nearly reported out of
  that evidence** -- "saving does not clear the journal", which was wrong twice
  over, because `-Kill` also makes every run look like a crash and a journal
  surviving a crash is correct.

  Two habits, and the second is the one that generalises. Say before driving
  the window and let the person say when. And **when a manual pass produces a
  surprising result, suspect the pass before the product** -- an automated test
  that is wrong usually fails, while a manual one that is wrong quietly
  produces a finding.

- **Deleting the code that writes a file does not delete the file, and the
  record has to say which of those two happened.** Ten crates of features left
  this session; the removal ADRs describe the code, because the code is what
  the commits touched and what the tests cover. **The bytes already on a disk
  have no compiler and no test**, so they went unmentioned -- and they are the
  part a user actually meets.

  Two were still there: a `security-history.log` frozen at the moment
  [ADR-0064](docs/decisions/ADR-0064.md) took the audit log, and a `recent.toml`
  at the location the recent list left when it stopped roaming. The first is
  the one that matters, because **it does not rebuild and it looks
  maintained** -- trap 3 in a file rather than a comment.

  They stay. A product that hands the user a `.reg` to read rather than
  seizing a file association ([ADR-0012](docs/decisions/ADR-0012.md)) does not
  get to delete out of `%APPDATA%` on its own judgement
  ([ADR-0070](docs/decisions/ADR-0070.md)).

  The check is one question at the end of every removal: **what did this
  feature leave behind, and where?** For nine of the ten the answer was
  nothing. It only takes one.

  A first draft of that ADR said the whole `%APPDATA%\bachelorpad` directory
  was orphaned. It is not -- it is `DirKind::Config` and holds `config.toml`
  as soon as configuration is saved. **The directory is live and two files in it
  are dead**, which is the difference between removing a folder and not, and
  one `grep` for `DirKind::Config` told it.

- **An exemption outlives the reason for it, because an exemption is a
  `filter`, and a `filter` cannot notice.** The one agreement between the
  registration table and the parser -- *claiming a file type the editor cannot
  open is the failure a user experiences as a broken machine* -- was asserted
  by a test that skipped `TypeGroup::Own`. That skip was correct when written:
  `.bpadx` was genuinely unparseable and genuinely openable. ADR-0064 removed
  the second half and could not remove the first, so the test went on passing
  while **every preset, including Notepad Replacement, registered an extension
  that double-clicks into a window of ciphertext**
  ([ADR-0069](docs/decisions/ADR-0069.md)).

  The failure arrives dressed as success, in Explorer, which is worse than
  Windows saying it cannot open the file at all.

  Trap 3 says a claim in a comment is not a property of the code; this is trap
  3 with a `filter` in place of the comment, and it is worse, because **a
  stale comment is read by a person who may doubt it and a stale `filter` is
  read by nothing.** The habit: when a decision removes a capability, search
  the *tests* for what stopped being checked, not only the code for what
  stopped being called. It is the same `grep` either way -- who produces this,
  and who reads it?

- **Two more claims nothing kept, and both were user-visible.** ADR-0068's
  inventory found that File ▸ Set as Default Editor had *always* registered
  file types to an icon that did not exist: Windows wrote `DefaultIcon` as
  `"<exe>",0` and the executable has never carried an icon resource, and Linux
  wrote a theme name nothing had ever installed. Every registered type
  rendered blank in Explorer.

  This is the "type with no producer" pattern again, and the difference is
  worth noting: those were internal and cost nothing until somebody read them.
  **This one shipped, and a user could see it.** The lesson is the same and
  the stakes are not, so the same `grep` -- *who produces this, and who reads
  it* -- is worth running against anything the product writes *outward*: a
  registry value, a `.desktop` entry, a file on somebody else's disk.

- **A type with no producer is not an implementation, it is a claim** -- and
  three of them turned up in one sequence, in three different hiding places.

  - Four of `Policy`'s seven axes are computed, stored and printed by the
    Security Inspector as *the policy in force*, and consulted by nothing
    ([ADR-0059](docs/decisions/ADR-0059.md)).
  - `Format::has_data_operations` answered a question whose only asker was a
    menu that had left ([ADR-0062](docs/decisions/ADR-0062.md)).
  - `bp_buffer::Access` guarded `insert` and `remove` against a condition
    **nothing has ever set** ([ADR-0066](docs/decisions/ADR-0066.md)). Not a
    casualty of the reduction either: before any of it, its one production
    call site was `LargeFile::open`, and a huge document had no `Buffer` at
    all -- so the guard on the rope has never once fired.

  **None would fail a build, a test, or a review of the file it lives in.**
  Each one reads perfectly well locally; what is missing is somewhere else.
  All three fall out of one question asked of the *workspace* rather than of a
  file: **who produces this, and who reads it?** That is a `grep`, and it is
  the same `grep` that would have caught both of this session's own trap-6
  mistakes.

- **Ask whether a cost is *inherent* to a change or merely *concurrent* with
  it.** [ADR-0059](docs/decisions/ADR-0059.md)'s five removals reported three
  things as materially weaker, and all three were recorded honestly as
  consequences. [ADR-0065](docs/decisions/ADR-0065.md) found that only one of
  them had to be paid.

  - **Inherent**: ADR-0050's golden envelope vectors. No format, no vector.
    Nothing can restore them and pretending otherwise would be worse.
  - **Concurrent**: `bp_formats::sniff` lost the only thing that could
    contradict it -- but a *test* does not need a menu, and `serde_json` was
    already in the tree, so the claim came back for the price of one file and
    no dependency.
  - **Concurrent**: Private lost its recovery journal, on ADR-0020's rule that
    a control which quietly weakens itself is worse than an absent one. **The
    qualifier was doing more work than it got credit for**: the Privacy menu
    has always printed what the journal actually is, so the failure that rule
    forbids could not occur. Private keeps a plaintext journal and the row
    says "on, unencrypted".

  **Recording a cost is not the same as accepting it**, and a sequence with
  momentum will file both the same way. The check is cheap and belongs after
  the change lands rather than during it: *would this still be true if the
  removal had happened on its own?*

- **A rule with a qualifier should be read with the qualifier.** ADR-0020 says
  a control that **quietly** weakens itself is worse than an absent one.
  ADR-0064 read that as "weakens itself" and disabled the journal; ADR-0065
  read the whole sentence and kept it, with the readout as the licence -- and
  a test that says in as many words that if the readout stops naming the
  journal's form, the journal goes away again.

- **The last removal in a sequence is the one that changes what stays.**
  R1 to R4 of [ADR-0059](docs/decisions/ADR-0059.md) were deletions: a crate
  left, its callers left with it, and the survivors were untouched. R5 could
  not be, because `bp-security` was never a security *feature* -- it was the
  governor of features, and four of the things it governed had already gone.
  What was left was a governor with two subjects and a name describing its
  origins rather than its job.

  **A thing that decides on behalf of other things outlives them, and it
  should be renamed when it does.** Keeping "Security" over four profiles and
  a toggle would have been free, and would have left every future reader
  believing this product does something it does not.

- **A deleted match arm is not a compile failure, it is a menu row that stops
  working.** [ADR-0064](docs/decisions/ADR-0064.md) removed nine security
  dispatch arms as one block, and took `SET_DEFAULT_EDITOR` with them by
  accident. File ▸ Set as Default Editor would have shipped doing nothing.

  **It surfaced as a dead-code *warning* on a constant in another module**,
  not as an error, and `no_menu_offers_a_row_that_does_nothing` does not catch
  it either -- the row still has an action id, and the id still exists. The
  habit that saves this is the one R4 already named: delete the *callee*
  first, so the compiler audits the callers. Here the callee was `rfd` and
  `bp-platform`, which are staying, so nothing was owed an error.

- **A test that checks a label's length catches its first new offender.**
  ADR-0049 found a clipped menu label by driving the window and mechanised it.
  The Privacy menu's new readout -- "Recorded about this document: path, title
  and tags" -- was 50 characters and would have been elided. This is the first
  time that test has failed for a label written *after* it existed, which is
  the whole point of mechanising a manual finding: the second occurrence costs
  nothing to find.

- **A grep before a question, and the person who wrote the rule broke it.**
  A survey during ADR-0064 reported that `bp_crypto::stable_name` named
  *every* recovery journal file, so removing crypto would orphan unsaved work
  -- and a whole redesign of journal naming was scoped on that premise.
  It was wrong: `stable_name` named only the *sealed* journal, and plaintext
  journals are `{document_id}.json`. One `grep` settled it.

  That is trap 6, committed while warning about trap 6, in the same session
  that had already read a pull-request listing from the wrong repository.
  **Both were confident readings of something adjacent to the answer.** The
  cheap check is not "is this plausible" but "which line of code says so".

- **A capability with no directory of its own is removed by deleting
  *branches*, and a gate that is always open is a gate a reader has to
  check.** [ADR-0063](docs/decisions/ADR-0063.md) took out huge-file mode:
  2,391 of `bp-buffer`'s 2,854 lines, `bp-search`'s `StreamSearch`, and nine
  files in `bp-ui`. Unlike R1 to R3 there was no crate to `git rm` and no
  action-id block to free -- it was reached by *opening a file*, so every
  trace of it was a condition inside code that stays.

  Every one of those nine files had the same shape: **`if this document is
  served from disk, do the other thing`.** Deleting the other thing is easy.
  The risk is the `if`, because a condition pinned to `true` and a condition
  deleted compile identically and read differently to everyone afterwards. The
  File menu is the clearest case: `let writable = !served_from_disk` gated four
  rows, and setting it to `true` would have left them all asking a question
  with one possible answer forever.

  **The tell that you have pinned rather than removed** is a local variable, a
  parameter or a field whose value is now a constant. `menus::file` lost a
  `bool` parameter, `AppState` lost `drawn_rows`, and `uses_custom_surface`
  lost a term -- each found by the compiler only because the thing behind it
  was deleted first. Delete the callee before simplifying the caller and the
  compiler does the audit; simplify first and it cannot.

- **A test that asserted the branch you did not change becomes the whole
  statement when you delete the other one.**
  `an_ordinary_document_still_follows_the_flag` existed because ADR-0030 made
  `uses_custom_surface` two-branched, and something had to hold the branch that
  ADR *did not* touch. With the huge-document branch gone it is the whole of
  that function, so it was renamed rather than deleted -- and it is now
  load-bearing in a way it never was, because a flag that started claiming
  every document would retire `TextInput` by accident and take input-method
  composition with it.

- **A decision not to unify two things is worth as much as a decision to
  unify them, and it is only visible when one of them leaves.**
  [ADR-0044](docs/decisions/ADR-0044.md) declined to merge citation reading
  with the store's synthesis, on the grounds that they answer different
  questions about different subjects -- one reads the document in front of
  you, the other reads what you have written over time. At the time that read
  as a refusal to tidy up two things that shared a menu.

  It is what made [ADR-0060](docs/decisions/ADR-0060.md) a deletion instead of
  a rewrite. `bp-research` was 5,208 lines with **one dependant and one file
  in it** -- the seam was a `mod` line and a `use`. Had "research mode" been
  built as one thing, removing citations would have meant unpicking it from
  the survivor.

  **The general form:** a refusal to merge is a cheap thing to argue against
  and an expensive thing to undo, and its payoff arrives only if one half is
  ever removed. That payoff is invisible while both halves are alive, which is
  why the argument for merging always sounds better than it is.

- **The same crate borrowed a caller's reason twice, in two consecutive
  removals.** `bp_semantic::is_fence` said it recognised both fence characters
  *"because `bp-notebook` reads both"* (ADR-0057). `bp_semantic::questions`
  explained itself by citing *"the same split `bp-research` draws between
  finding a DOI and resolving one"* (ADR-0060). Both rules were correct and
  unchanged; both justifications named a crate that had just been deleted.

  **Twice in the same crate is a pattern, and the pattern has a cause.**
  `bp-semantic` is a crate of *rules*, and a rule is most tempting to justify
  by naming the caller that wanted it -- which is the weakest reason
  available, because the caller is the one thing guaranteed to change. A rule
  worth keeping has a reason that survives its callers; if the only reason to
  hand is "X needs it", that is worth noticing before X leaves rather than
  after.

- **A record that is 44% right is more dangerous than one that is wholly
  stale.** `metadata/repository_manifest.json` sat in this repository from the
  scaffold commit: 181 paths with byte counts and sha256 hashes, no producer,
  no reader, and no script that could rebuild it. Measured before deleting it
  ([ADR-0058](docs/decisions/ADR-0058.md)): 79 hashes still matched, 86 were
  wrong, 16 named files that no longer exist, and 275 of the repository's 456
  tracked files were never listed at all.

  **The 79 correct rows are what made it a hazard.** A manifest that was
  wholly wrong would be dismissed at a glance; one that is nearly half right
  reads as a record with some drift, and invites somebody to trust a row
  rather than the tree. That is trap 3 with an artefact in place of a comment
  -- 181 checkable claims, nothing checking any of them.

  **Regeneration was the tempting wrong fix**, and naming why is the
  transferable part: a script plus a gate stage would have made the file true
  and left it answering no question, at a cost on every run. Compare
  ADR-0050's golden vectors, which earn their cost precisely because the bytes
  were committed *before* the change they guard against. **A hash of a file,
  stored in the same commit as the file, is evidence of nothing.**

  It was found only because ADR-0057's sweep gave somebody a reason to open
  it, which is the same way ADR-0054 found four manifests claiming a licence
  the repository did not contain. Both are the same shape: **a claim nothing
  reads survives until something makes a person read it.**

- **A constraint that cannot fail is worth removing, not keeping.**
  ADR-0011 and ADR-0025 required that notebook content never auto-runs, and
  the mechanism was a good one: a `UserGesture` no parsed file could
  construct, demanded by value, called in exactly one place in the whole
  shell. [ADR-0057](docs/decisions/ADR-0057.md) removed execution, and the
  rule survived the thing it constrained -- it was still written in
  `specs.md`, in `R011`'s constraint list and in `R001`'s, reading like a live
  guarantee about a product that now runs nothing.

  **A vacuous constraint is trap 3 in its most convincing form**, because it
  is *true*. Nothing would fail if it stopped being enforced, and nothing
  would fail if it were enforced twice; it costs a reader a paragraph and
  costs the next person to touch consent a false sense that a pattern is
  still in the tree. `project/WORK_QUEUE.md`'s E3 said as much until this
  session: it planned to unify two `UserGesture` types that no longer exist.
  Where the pattern was worth keeping, it is now described rather than
  pointed at.

- **Removing a capability is mostly a documentation change, and the ratio is
  the surprising part.** ADR-0057 deleted two crates, two menus, a shell
  module, a Slint panel, an integration test and a fuzz target -- about 7,100
  lines -- and the code came out in one pass because neither crate depended on
  the other and one file was the whole seam. What took the rest of the session
  was **nineteen files of record that still described it**: a phase table, a
  spec section, a data model listing two tables that had never been migrated
  into existence, a rosetta for runners, a fuzz README numbering its targets,
  a gate comment justifying a stage by five doctests that had gone with the
  crates, a dormant GitHub workflow whose own header promises it is kept
  correct, and a manual checklist telling a person to drive two rows that no
  longer exist.

  None of that would have failed a build. **The count that matters is not how
  entangled the code was, it is how many places had made a claim about it** --
  and this repository already knew that: it is why `CLAUDE.md` gives each kind
  of fact one home. The homes held; what drifted was every file that had
  quoted one.

- **A performance claim in a comment is trap 3 with worse consequences.**
  `bp_editor::view::Anchor` explains itself by saying a global row index
  "would have to be computed by laying out every line above it -- an
  O(document) cost on every frame", and that anchoring instead means
  "scrolling costs only what is on screen". True, well-reasoned, and until
  2026-08-22 **nothing would have failed if it stopped being true**: a change
  making `visible_rows` count from line 0 passes every test and both gate
  legs, and surfaces only as an editor that gets slower the further down you
  go -- the kind of regression that arrives as a vague complaint months later.

  `bp-editor/benches/scroll.rs` makes it fail instead. The useful threshold
  turned out not to be a time but a **ratio between two columns of one run**:
  a document a thousand times larger costs **1.19x** as much per frame, which
  is the rope's logarithmic indexing and nothing else. Times are worthless
  across machines; a ratio inside one run is not. The same trick made the
  other half of [ADR-0017](docs/decisions/ADR-0017.md)'s revert condition
  honest by leaving it alone -- rasterisation still needs a capture rig, and
  the bench prints that rather than letting a green run imply it.

- **Trap 7 was not only about `.bpadx`, and the other four files are the ones
  a user notices.** ADR-0050 found that nothing tested whether a document
  sealed by an earlier build still opens, and fixed it for the envelope. The
  same hole was open for **every other file this product leaves on a disk** --
  `config.toml`, `recent.toml`, the security history and the recovery journal
  -- each covered by a write-then-read-back test proving only that one build
  agrees with itself.

  `tests/integration/tests/what_an_earlier_build_wrote.rs` holds committed
  literals for all four. **The worst of them is `recent.toml`**, because
  `Recent::parse` ends in `unwrap_or_default()`: a format change there does
  not fail, it returns an empty list, and the user opens the product to find
  their recent files simply gone with nothing said and nothing logged. A
  silent failure needs a test *more* than a loud one, and gets one less often
  -- there is no error to notice and therefore nothing to write a test about
  until somebody goes looking.

  Every guard was watched failing, by mutating all four fixtures at once,
  before the file was called done. That is ADR-0051's habit -- *put a
  known-bad value in and watch the guard fire* -- and it is the difference
  between four passing tests and four tests that pass **for the right
  reason**.

- **When a quoted string needs a second quoted string inside a third, write a
  file.** The release script's Linux leg was a PowerShell string, passed to
  `bash -c`, containing a bash `$(...)`, containing an `awk '{print $2}'`.
  Every layer has its own escape character and one of them is a backtick. What
  it produced was not a syntax error -- it was a version check that compared
  an empty string against the real one and reported *"the Linux leg failed"*,
  with the actual cause invisible because nothing in the chain had printed.

  It is now `scripts/release-linux.sh`, which is readable, runnable on a real
  Linux box, and debuggable by looking at it. **The tell is countable**: if
  the nesting depth of quoting reaches three, the cost of a file is already
  lower than the cost of the next bug.

- **A call that succeeds and changes nothing is worse than one that fails.**
  Staging the Linux archive on the `/mnt` mount and calling `chmod` produced a
  tarball with world-writable licence files. DrvFs reports 0777 for everything
  and **ignores `chmod` silently** -- exit 0, no warning, no change. The fix is
  to stage in the distro's own filesystem and let only the finished archive
  cross the mount. The general shape is trap 3 in the shell: *what would fail
  if this call stopped working?* Nothing did, and nothing would have, until
  somebody extracted the archive and looked. ADR-0055's script carries both
  reasons in its own comments.

- **A question can be answered in the repository and open in the queue at the
  same time.** D16 -- what should compute an embedding -- was described for
  two sessions as "the largest unasked question in the project" and "the last
  genuinely large unscoped thing". `bp_security::Embeddings` had carried its
  three answers since [ADR-0020](docs/decisions/ADR-0020.md), with a mapping
  for all four named profiles and a doc comment reading *"Phase 10 reads
  this"*. One `grep` found it; two sessions of describing it did not.

  This is trap 6 pointed the other way. That trap says a claim in the record
  is not a property of the repository; this is its mirror -- **a decision in
  the repository is not a row in the record**, and the kind least likely to be
  re-read is the kind recorded in a *type*, because nothing about a queue of
  questions suggests looking in `src/`. The habit is one line: **before asking
  a question, grep for its answer.**
  [ADR-0056](docs/decisions/ADR-0056.md).

- **A deferred decision and a blocked one look identical until you ask what
  actually stops.** D15 -- buy a code-signing certificate? -- read like a gate
  on phase 20 and gated one sentence in a README. Name the thing that cannot
  proceed; if nothing can be named, the answer is "build it and leave the
  step". The refusal inside it is worth as much as the deferral: **a
  self-signed Authenticode certificate is worse than shipping unsigned**,
  because it is only satisfied once the user installs a root certificate they
  have no reason to trust -- teaching, in order to run a text editor, exactly
  the habit that makes signing worth having.
  [ADR-0055](docs/decisions/ADR-0055.md).

- **An interface nobody has typed at is not an interface, it is a parser.**
  Nineteen phases in, `bachelorpad --version` printed nothing and opened a
  window; `--help` did the same. Twelve flags were accepted and not one was
  discoverable from the product -- the only inventory was `main.rs` and the
  middle of a `match`. A thirteenth spelling, `--font_size=20`, was dropped in
  silence *by the crate that already reported the identical mistake in the
  config file*: one end of one precedence chain told the user and the other
  swallowed it, and nobody had chosen that. Three more flags were documented
  in `specs.md` and absent, one of which made `--line 427 server.log` try to
  open **a file called `427`**.

  Every one of those is invisible from inside the code, where the arguments
  are a `Vec<String>` that gets read correctly. They appear the moment
  somebody asks what a *stranger* typing `--help` would see -- which is the
  question packaging asks about everything, and is why phase 20's inventory
  found four defects in a product 1,972 passing tests deep. **Take the
  inventory before the estimate**, again.

  The fix that generalises is that `bp_config::cli::FLAGS` holds flags the
  crate does not act on. A list of only the six it applies could not tell a
  typo from a flag the shell owns, and would have called `--self-check` a
  mistake on every gate run. [ADR-0054](docs/decisions/ADR-0054.md).

- **A manifest claim is not a file.** Every crate has said
  `license = "MIT OR Apache-2.0"` since the scaffold commit and the repository
  contained neither licence text. Nothing checks, because nothing in a build
  reads it -- `cargo` takes the string on trust and so does everybody reading
  the manifest. It is trap 3 in its purest form: *what would fail if this
  stopped being true?* Nothing would, right up until the first release
  archive shipped a claim with nothing behind it. ADR-0054.

- **An unanswerable question and an unasked one look identical in a queue.**
  D13 sat for six sessions and was neither: it was a question whose *premise*
  had gone stale, which reads like a hard decision and is really a stale fact
  wearing one. The record said "111 commits live on the branch; `main` has
  none of them"; `git merge-base` disproved it in one command, and what was
  actually being asked turned out to be small enough to answer in a message.
  **The tell is that nobody could say what would change if it were answered
  either way.** Both remaining questions were answered the same day the
  premise was corrected. [ADR-0053](docs/decisions/ADR-0053.md).

- **A comment that enumerates its cases is asserting something about every
  one of them.** `dispatch::select` said Find Next/Previous, Go to Line and a
  cross-file result "all move focus into the editor on purpose, because each
  of those is a single deliberate jump the user makes once". That reads as an
  explanation and is three claims; one was true. Both bars stay open **on
  purpose** -- Go to Line's call site says so three lines from the call that
  contradicted it -- so the caret moved into the document while a box was
  still on screen.

  **What it cost: pressing Enter twice in the find box replaced the match with
  a line break**, and went on inserting one per press, announced by nothing
  but the dirty dot and a line count rising off-screen. Ctrl+S would have
  saved it. The fix was one function away the whole time: `preview_match`
  already selected without taking focus, and its comment already explained the
  hazard -- *for the typing case only*. The same sentence was true of Find
  Next and had never been asked of it. It is now `reveal`, it is the default,
  and `select` is the exception for panel clicks.

  A list in prose looks like description rather than specification, which is
  how it survived; and the call site that disproved it was close enough that
  anyone reading either had already stopped reading the other.
  [ADR-0052](docs/decisions/ADR-0052.md).

- **A guard's own comment describing its failure mode is not a defence
  against it.** `menus::tests::every_menu()` exists so that "a menu added
  later cannot quietly escape the check below", and says so in a doc comment
  that names the failure mode exactly: *the menu nobody added to it.* The Run
  menu was missing from that list from the day the sentence was written, so
  neither the readout sweep nor `LABEL_BUDGET` had ever seen it -- and it was
  carrying a **52-character** readout, and two around 70, while a test
  asserting 42 passed on every run.

  What that let through was not cosmetic. On a `.py`, Run's first row read
  *"Nothing to run: open an .ipynb, or …"* three lines above an **enabled Run
  Document that ran the file correctly**. A readout contradicting a working
  row is worse than no readout, because a user believes it and stops looking.
  The information needed to tell the truth -- `document_runnable` -- was
  already a parameter of the function and unused.

  The cheapest form of "is this list complete?" is putting a known-bad value
  in and watching the guard fire. Doing that here printed
  `Run (inert) ▸ ... is 52 characters`, which is the evidence a passing test
  could not give. [ADR-0051](docs/decisions/ADR-0051.md).

- **The same 1x1 capture, from the fix that was written for it.** ADR-0049
  established that `MainWindowHandle` is not reliably this application's
  window and added an enumeration to avoid it -- but kept *waiting* on
  `MainWindowHandle -eq 0`, a condition satisfied while the handle still names
  the 0x0 `Winit Thread Event Target` and the real window has no title yet.
  The enumeration then matched nothing, fell back to `MainWindowHandle`, and
  photographed the very window it existed to avoid. **The wait condition and
  the selection rule have to be the same question**, and there is now no
  fallback: a capture of the wrong window reads as "the row did nothing",
  which is the one conclusion a capture must never invite by accident.
  ADR-0051.

- **A round trip through one build is not evidence about another build.**
  Every round-trip test in this workspace -- `bp-crypto`'s own, the
  integration tests, and a property test over arbitrary plaintexts and
  arbitrary options -- **sealed and opened with the same build**. That proves
  the writer and the reader agree with each other; it cannot prove either
  agrees with what is already on somebody's disk. A change altering the header
  layout, the additional authenticated data, the nonce construction or the
  Argon2id invocation *coherently on both sides* would have passed the entire
  suite and both gate legs and silently orphaned every document ever written.

  ADR-0021 opens by saying a document written today has to open in ten years.
  Asked what would fail if that stopped being true -- trap 3, aimed at an
  ADR's first sentence rather than at a comment -- the answer was **nothing**.
  Three golden vectors had been sitting in `fuzz/corpus/envelope/` since the
  corpus was seeded, real envelopes under a known passphrase over known
  plaintext, and the survival harness fed all three to `open` and *discarded
  the result*, because a survival harness asserts survival. The assertion was
  one line away and nobody had written it. [ADR-0050](docs/decisions/ADR-0050.md).

- **A task that cannot be finished can usually be prepared.** D4 -- an outside
  review of the `.bpadx` envelope -- was answered *yes* five sessions ago and
  moved not at all, and the reason was neither laziness nor difficulty. **An
  agent cannot discharge it**: a reviewer inside the session is not an outside
  one. So every session correctly declined to do the thing and then did
  nothing else about it either. The question that unstuck it was not "can this
  session review the envelope" but "what would make a reviewer's hour cheap?",
  which has an answer:
  [BPADX_ENVELOPE_REVIEW.md](docs/architecture/BPADX_ENVELOPE_REVIEW.md).

- **A stale figure that is a real measurement of the wrong thing is the
  hardest kind to notice.** ADR-0035 lowered the pre-authentication Argon2id
  ceiling from 1 GiB and 64 iterations to 256 MiB and 16; `fuzz/` kept
  measuring the old one. Three rows of its cost table -- 512 MiB, 1 GiB, and
  64 passes -- were past the bound, so they were timing an *instant refusal*
  rather than the work, and the README's headline of "roughly 75 seconds and a
  gigabyte of resident memory" described a bound the code had stopped
  enforcing. Nothing looked wrong: the numbers were genuine, the test passed,
  and the units were right. The control has moved from 4 GiB -- sixteen times
  past the ceiling -- to **one KiB past it**, because a bound is only
  demonstrated at its edge. ADR-0050.

- **Research mode's last five planned rows turned out to be a paper's
  structure, and none of them survived as a row.** `MENU_MAP.md` had named
  research question, evidence, findings, methods and datasets since the
  scaffold commit. They are IMRaD — the sections of a research *paper* — and
  [ADR-0039](docs/decisions/ADR-0039.md) defines the mode as synthesis over
  your own notes, which is not the same thing.
  [ADR-0046](docs/decisions/ADR-0046.md) scoped each against what
  `bp-storage` actually holds: evidence and methods became part of Research
  Report, datasets became **What the Store Holds**, findings was dropped as a
  second name for the report itself, and research question became **Open
  Questions**.

  **The lesson is the one this repository keeps relearning from the other
  side.** Four sessions were lost to sizing modes before the question of what
  they were *for* had been answered — "undecided reads like large". This is
  the same mistake at a smaller scale and in the opposite direction: five
  names sitting in a plan long enough to look like a specification. Nobody
  wrote them as one. They were a sentence in the scaffold commit, and asking
  what each was actually *for* resolved all five in an afternoon — two of them
  by deleting them.

  **And scoping them found something.** Research Report looked for
  under-connected documents among the five hundred most recently seen and said
  so nowhere the reader could see. A store of nine hundred documents got a
  report drawn from five hundred of them and looked complete. The rule was
  inspectable in `research.rs`; it was not inspectable from the window, which
  is where the reader is. ADR-0041 had promised "a fixed, inspectable rule" —
  it turns out that is a claim about the product, not about the source.

- **A doc comment promised something the code had never done.**
  `Store::forget_document` said "and with it any tags that were only on it".
  `ON DELETE CASCADE` took the join rows; the `tags` row was left orphaned,
  and its own test *asserted the survival* in a comment while the doc comment
  above it promised the opposite. Neither reader was wrong about what they
  were looking at. It survived because every tag-shaped query in `bp-storage`
  joins `document_tags` and so cannot see an orphan — until `Store::summary`
  wanted to count tags and had to pick which number was true. The third time
  this repository has caught a comment asserting a property the code lacks,
  and the first where a test agreed with the code and disagreed with the
  comment without anybody noticing the two were in the same file.

- **`bp-notebook` and `bp-execution` are reachable.** Two crates and about
  9,500 lines had no route to a person for three sessions — not for want of
  effort, but for want of an answer to *what the mode is for*, because
  ADR-0038's no-persistent-session model makes some uses honest and one
  dishonest. [ADR-0043](docs/decisions/ADR-0043.md) answers it: a literate
  document, whose examples are self-contained by intent, so the limitation
  stops being one.

  **The surface is a Run menu, not a cell view**, and that is the whole
  reason it exists today rather than after a redesign: a cell-sequence view is
  a second document kind, with its own editing, selection, undo and save
  story, none of which the rope-plus-two-surfaces design has a place for — and
  all of which would have to be settled before a single cell could run. The
  notebook stays its own JSON in the ordinary editor, which `bp-notebook`
  anticipated: `raw_json_view` exists because the JSON "is the thing the user
  might want to hand-edit".

- **Find did not scroll to its match**, in the surface most people use.
  On a 165-line document, searching for text at line 143 reported `1 of 1`
  — correct — and left the reader looking at line 1, with the match selected
  off screen. It affected Find, Find Next and Previous, and opening a
  cross-file result.

  **Every earlier test of Find used a document that fitted on one screen**,
  which is why four manual passes and three of the same session's own runs
  did not see it. It surfaced only because a new feature — clicking a row of
  the cell outline — jumped to a line a hundred below the fold and visibly
  did nothing.

  `set-selection-offsets` moves the caret and nothing moves the viewport.
  Slint exposes `cursor-position-changed` precisely so a scroll container can
  follow the caret, its own `TextEdit` uses it that way, and this editor was
  already inside a `Flickable` — so the fix is the toolkit's own pattern,
  clamps included. **Watched working in both directions**, because it sits on
  the typing path: the view scrolls to a match a hundred lines down, and
  typing mid-document leaves the viewport exactly where it was.

- **Research mode, and the last unreachable crate.** `bp-research` had 5,208
  lines and 161 tests no user could reach. What kept it there was reading two
  decisions as a contradiction — ADR-0039/0041 say research mode is synthesis
  over `bp-storage` and *not* built on the bibliography types, while
  `MENU_MAP.md` names citation metadata and DOI lookup. They are two features
  sharing a menu: one reads the store and says what you have been writing
  about, the other reads the document in front of you and says what it cites.
  [ADR-0044](docs/decisions/ADR-0044.md).

  **"DOI Lookup" is renamed, not implemented**, and that is the substance:
  finding an identifier and resolving one are different acts, and only the
  first works offline (ADR-0006). A row called Lookup would be a promise this
  product cannot keep.

- **The status bar went stale after every jump, in all five places that
  jump.** Go to Line moved the caret and the view and the readout still said
  where the caret used to be; so did Find Next and Previous, opening a
  cross-file result, and stepping a scan of a huge document. `refresh` sets
  that readout, and every one of those paths deliberately does *not* refresh
  — they move and then draw the surface directly. Nothing corrected it
  afterwards either: both background pollers refresh only when they
  themselves found a change, so it stayed wrong until the next keystroke.

  Fixed in `draw_editor_view` rather than at any of the five call sites,
  because a sixth would have been wrong too — **drawing the surface and
  saying where it is are one act, so they are now one function.**

- **Nothing held the keyboard until you clicked, in every `--editor-view`
  launch and every huge document.** Ctrl+F on a fresh window did nothing;
  click anywhere in the document first and it worked. Found on 2026-08-21
  while fixing the defect below, which had been hiding it.

  `AppWindow`'s `forward-focus: editor` names the `TextInput`, and the
  `TextInput` is `visible: !use-editor-view` — invisible for any document
  served from disk ([ADR-0030](docs/decisions/ADR-0030.md)) and for *every*
  document under `--editor-view`. So the window opened with the keyboard held
  by an element nobody could see.

  **`forward-focus` cannot be made to follow the flag, and that is the part
  worth writing down.** It takes an element rather than an expression, and
  `i-slint-compiler`'s `focus_handling` pass resolves it at *compile* time
  into the component's init code — so it is one fixed element, chosen once,
  and no binding will ever move it. Checked in the toolkit's source, like the
  other Slint findings here, rather than inferred from the symptom. The fix is
  therefore a hand-over rather than a binding: `focus-editor-soon()` gives the
  caret to whichever surface is drawing, at startup and whenever the surface
  changes under the active document.

  **The guard on it is the whole difference between a fix and a new defect**,
  and it is the same lesson as the passphrase leak two entries down: switching
  to a tab drawn by the other surface must not take the caret out of an open
  find box. Confirmed by driving the window — with a query typed and the huge
  tab active, clicking across to a small file leaves the next keystrokes in
  the find box.

- **A huge document had no keyboard shortcuts at all, and nobody had noticed.**
  Not Ctrl+F, not Ctrl+S, not Ctrl+O — nothing. Found on 2026-08-21 by
  building Find for exactly those documents and discovering the feature could
  not be reached, because Ctrl+F is its only route: there is no Find menu row.

  **The cause is a function saying it handled something it did not.**
  `apply_editor_command` sends every key in a viewer to `scroll_for_command`,
  which returned `true` for everything it was given — including the
  `Command::Ignore` that `bp_editor::keys::command_for` produces for the keys
  its own comment calls out as belonging to the window: "Ctrl+S, Ctrl+F and
  the rest." `EditorSurface`'s `FocusScope` accepts whatever that reports as
  handled, and an accepted key never bubbles to the `KeyBinding` waiting for
  it. It had been that way since the viewer shipped (ADR-0030).

  **The lesson is about the boolean, not the keyboard.** "Handled" is a claim
  with a consequence somewhere else, and `scroll_for_command`'s two `return
  true`s were written to mean "there is nothing to do here" — which is the
  opposite of what the caller does with it. Its deliberate swallowing of
  *editing* commands was and is right; it was the catch-all that was too wide.

- **Find works in a document the rope does not hold.** The last piece of the
  large-file story, and the third of the three things
  [ADR-0030](docs/decisions/ADR-0030.md) named as deliberately unbuilt.
  [ADR-0042](docs/decisions/ADR-0042.md) has the design; what is worth
  repeating here is that the scan is **resumable rather than threaded** —
  `advance(windows)` does a few megabytes and returns, so cancelling is the
  absence of the next call rather than a flag another thread has to notice,
  and a scan cannot outlive its document because it lives on the `HugeView`.
  Driven against a 213.5 MiB log: `searching 86%` while it ran, `1 of 1` at
  line 4,800,001 when it finished, the match highlighted on its own
  characters.

- **A signing passphrase no longer types itself into the document.** The
  fourth manual pass, on 2026-08-21, found the worst of the four:
  `Security ▸ Sign Document`, then type the passphrase the bar has just asked
  for, and every character went **into the open document, in plain text**,
  while the passphrase field stayed empty. Saving after that would have
  written the passphrase to disk in the clear. Reproduced against the
  unfixed binary while confirming the fix: `secret` typed at 1.6 s per
  keystroke turned `alpha beta gamma` into `siecalpha beta gamma` and marked
  the tab dirty.

  **The cause was two focus-stealers, both running *after* the callback that
  asked for the caret**, and the reason `PassphraseBar`, `focus-input()` and
  the `SIGN_DOCUMENT` arm all read as correct in isolation is that none of
  them is either one. First, `dispatch` in `ui/app.slint` ends every menu
  action with `root.focus-editor()`, which is right for every row that is not
  opening a bar. Second — and this one no amount of reading this repository
  would have found — **closing a `MenuPopup` restores the focus the popup took
  when it opened**: `i-slint-core`'s `process_mouse_input` computes which
  popup to close *before* it dispatches the click, and calls `close_popup`
  *after* the row's callback has returned, and `close_popup_impl` then hands
  the caret back to whatever had it before the menu opened. There is no way
  to opt a popup out of it.

  **The fix is one tick of patience**: `focus-find`, `focus-goto` and
  `focus-passphrase` now record *which* bar wants the caret and let a 1 ms
  `Timer` hand it over on the next turn of the event loop, once both
  focus-stealers have had theirs. `Edit ▸ Go to Line` had the identical
  defect — confirmed against the same unfixed binary, where a typed `9`
  landed in the document rather than the box — and the two encrypted-document
  paths reach `focus-passphrase` the same way. Confirmed by driving the
  window: the passphrase now arrives as six dots with the document untouched,
  the confirm step keeps the caret too, and Find still behaves in both
  surfaces.

- **Find no longer edits the document while you type the query.** The third
  manual pass, on 2026-08-21, found the worst defect any of the three found:
  Ctrl+F, then type `replicas`, and the `r` reached the find box correctly
  (`1 of 3`), but every character after it went **into the document**, over
  the match the `r` had just selected -- `alpha replicas beta` became
  `alpha eeplicas beta`, the tab went dirty, and the find box still read `r`.
  Reproduced with 1.5 seconds between keystrokes, so it was not a race.

  The cause was one line with a comment that said what it was doing.
  `on_find_changed` ([lib.rs](crates/bp-ui/src/lib.rs)) runs on **every**
  keystroke in the query box and called `dispatch::select`, which ends in
  `editor.focus()` -- "Just take the focus back from the find box", says
  `select-range` in `ui/app.slint`. That is right for Find Next, the other
  caller, where the user does want the caret in the document afterwards. It
  was wrong for the preview that runs while they are still typing the query,
  and the two shared one function.

  **The fix is the split the toolkit's own comment implied it needed**: a
  second, focus-preserving path -- `dispatch::preview_match` in Rust,
  `preview-range` in `ui/app.slint` -- used only by the live-typing preview,
  leaving the three deliberate "jump to it" callers (Find Next/Previous, Go to
  Line, a cross-file search result) on the original focus-stealing one. Both
  the `TextInput` and `--editor-view` branches got the same treatment; the
  first pass's diagnosis of the `--editor-view` half was by reading the code,
  not by driving it, and driving it afterward found nothing further wrong.

  Two agents built this in parallel with an unrelated fix below, in files
  that never overlapped; one of them hit the exact file-collision this
  repository's own working notes warn about mid-run, from the other agent's
  concurrent edits, and caught it itself with an isolated worktree rather
  than reporting a false pass. Confirmed by driving the window again
  afterward: four clean runs against the exact sequence that broke it, at
  both typing speeds.

- **Crash recovery restores the text now, and the encoding and line ending
  with it.** Found in the same pass. `AppState::restore` opened the document
  and inserted the checkpoint's text, but never called `set_line_ending` or
  `set_encoding` the way `open` does, so a recovered document fell back to
  `LineEnding::default()` -- CRLF on Windows. An LF file recovered on Windows
  was rewritten CRLF throughout on the next save, and a UTF-16 document came
  back as UTF-8 -- silently contradicting a test that was already green,
  `saving_a_mixed_document_does_not_rewrite_the_minority_line_break`.

  `bp_history::Checkpoint` now carries `encoding` and `line_ending`,
  `#[serde(default)]` so a journal already on disk still deserializes.
  Recovery re-detects a missing line ending from the text itself, the way the
  rest of the codebase guesses when certainty is not available; a missing
  encoding cannot be recovered the same way, because the checkpoint holds
  already-decoded text, so it falls back to a documented guess rather than a
  claim.

- **A 2 GB file opens, and costs 0.8 MiB.** The last piece of phase 4
  ([ADR-0030](docs/decisions/ADR-0030.md)). A document past the huge threshold
  is served from disk as you scroll: arrow keys, the page keys, Ctrl+Home and
  the wheel move the window, the gutter numbers the *document* rather than the
  screen, and the status bar says which lines are on screen. Measured rather
  than asserted — a few lines of text peaked at 31.3 MiB and a 192 MiB log at
  32.1 MiB.

  It draws in the custom surface in **every** build, not only under
  `--editor-view`. `TextInput` owns its own text and cannot be handed a window
  of a file it does not have; the flag stays a statement about which surface
  *edits* a document the rope holds, and the one reason it is opt-in —
  input-method composition — has nothing to say about a surface that accepts
  no text.

  Save, Save As, Save a Copy and Reload grey with the reason. That is not
  politeness: `text_of` such a document is the empty string, so a Ctrl+S that
  merely did nothing special would write an empty file over two gigabytes and
  report success. Four paths refuse by name, and a test asserts the file's size
  is unchanged rather than trusting the return value.

  **Two things are deliberately not built**, and each is a refusal with a
  reason rather than a gap: Ctrl+End, and a total line count. Both need the
  whole file indexed, which for these documents means reading two gigabytes to
  answer one question while the window is frozen.

- **Signing works, and the key is sealed rather than protected.**
  [ADR-0031](docs/decisions/ADR-0031.md). ADR-0026 had measured a hole it
  could not close: `0600` on Linux, nothing at all on Windows, where narrowing
  a DACL needs Win32 and `unsafe` that `bp-platform` forbids — so
  `is_confirmed_private()` honestly reported "unknown" on half the supported
  platforms. Putting the key inside the envelope encrypted documents already
  use protects the *contents* instead, identically on both platforms, and
  designs nothing new.

  **One ceremony, not one per signature.** The first signature creates the
  key, because what was asked for was a signature; the second finds it and
  asks only to unlock it. The row says which the click will do before you
  click it.

  It still greys, for two reasons that are not about key storage: a signature
  is over the bytes **on disk**, so a document that has never been saved has
  nothing to sign, and one with unsaved changes would get a valid signature
  over the *previous* version — worse than a refusal, because it verifies.

- **Somebody has now typed into the custom editor view, and it works.**
  `--editor-view` gives Ln/Col, our own undo, and a view that draws only the
  lines on screen. A person drove it on 2026-08-20: keys arrive, Ctrl+S saves
  (checked by reading the file back off the disk), and **the caret tracks the
  pointer** -- a click at the end of `line 30` reported `Ln 30, Col 8`,
  exactly its length plus one. The wheel direction, drag-to-select and
  resize behaviour are still unchecked. See [ADR-0018](docs/decisions/ADR-0018.md).

  **That pass found a defect 177 tests could not**, and it is fixed: a tab
  was drawn as a single glyph while every column in `bp-editor` was computed
  as though it reached the next tab stop, so the caret on any tab-bearing
  line sat where the character was not. `view::expand_tabs` and
  `VisualRow::display_text` close it -- the row keeps its characters for the
  arithmetic, and the toolkit is handed the appearance.

- **Word wrap now works under `--editor-view`.** A document line can occupy
  several visual rows: `bp_editor::wrap` decides where they break, the view
  maps rows to characters, Up and Down move by row rather than by line, and
  scrolling is anchored to a line *and* a row within it so a line taller than
  the window can be scrolled through.

- **The crates are tested where they meet, and that is where the defects
  are.** Eleven files of cross-crate tests join load/rope/atomic save, the
  `.bpadx` envelope over a real file, the security profile against all three
  of its dependants, format detection against the parser that then handles
  the file, search against the buffer, journal recovery, the notebook through
  the file layer, the audit log, redaction, signing, and the filename grammar
  against the platform's own rules. Between them they have found seven
  defects that no unit test did. **All seven are fixed**, and there is no
  `#[ignore]`d test left anywhere in the tree. The most serious was
  `bp-naming`, which asked whether a *whole* sanitised title was a Windows
  device name, while Win32 asks only about the stem before the first dot --
  so `con.txt`, `aux.log` and `NUL.dat` passed untouched, and on Windows
  saving to one of those writes **the device**, reports success, and the
  document is gone. The sanitiser now tests the stem, its list gained
  `CONIN$` and `CONOUT$`, and a property compares its verdict against
  `bp-platform`'s stem by stem in both directions so the two lists cannot
  drift apart quietly.

  **The other half of that hole was a name nobody sanitised.** `bp-naming`
  defends the name the product *suggests*; the one a user types over the top
  of it in a Save dialog had never been checked by anything. `bp-files`
  depends on `bp-platform` now and `atomic_write` refuses a device name
  outright -- the only refusal in that module that is checked rather than
  attempted, because attempting it does not fail: the open succeeds, the
  read-back verifies against the console, and the status bar reports a
  document that is nowhere. The platform is a parameter rather than a `cfg`,
  so `con.txt` stays an ordinary file on Linux and both CI legs execute both
  rule sets.

  **There is no `#[ignore]`d test left anywhere in the tree.** All seven are
  closed. The last was a decision rather than a defect and is now
  [ADR-0029](docs/decisions/ADR-0029.md): a line break in this product is
  `
` or `
` and nothing else, so `bp-buffer` builds its rope without
  ropey's `unicode_lines` -- a break set that was never chosen, only
  inherited from writing `ropey = "1"`, and that disagreed with
  `bp-core::LineEnding`, `bp-files::encode`, `bp-search` and two of
  `bp-buffer`'s own helpers. Find-in-files and the caret now agree on every
  document this product can save, and the status bar reports one line count
  rather than a different one per editor view.

[ROADMAP.md](ROADMAP.md) has per-phase status;
[MENU_MAP.md](docs/product/MENU_MAP.md) says which menu rows are real;
[ARCHITECTURE.md](docs/architecture/ARCHITECTURE.md) has the crate map and the
editor-view constraint; `project/WORK_QUEUE.md` lists what is
ready to take and what cannot run in parallel;
`project/NEXT_SESSION.md` is the plan for picking this up
again.

## What the tiers taught

Moved here from `project/WORK_QUEUE.md` on 2026-08-22, for the reason the
section above was moved out of `README.md`: a queue should say what is ready,
and a lesson has one home. These three tiers were empty of work and full of
prose, in the one file an agent reads to find something to do.

## Tier 0.5 — what the cross-crate tests found

Eleven files of cross-crate tests found seven defects. **All seven are fixed,
the decision behind the last of them is made (ADR-0029), and there is no
`#[ignore]`d test left anywhere in the tree.** The two latent items that
remained -- neither wrong today, both wrong the moment anything depended on
them -- **are fixed too, and this tier is empty.**

| Item | Landed as |
| --- | --- |
| The recent-files list, the recovery journal and the security history all roamed on Windows | All three resolve through `DirKind::State`; `bp-config` and `bp-ui` in one change, because splitting it would have scattered the product's files |
| `Profile::is_data` was true for `Ini` and `Xml` | `Profile::is_data_class` for the class, `Format::has_data_operations` for the menu, `Format::ALL` and a test holding the second against `menus::data` |

What that pair is worth remembering for:

- **A path built by subtracting another path's last component moves when that
  one does.** The security history was `recovery_dir()` with `with_file_name`.
  Renaming the recovery folder would have moved it silently, and a history
  that moves starts again at sequence one.
- **Separate the rule from the edge that reads the environment.**
  `recovery_dir_under` and `audit_path_under` take the state directory as an
  argument, so both are assertable without a real profile -- the same split
  `bp_config::config_path` and `config_path_in` already made.
- **A predicate on the wrong type is a trap with no name.** `is_data` was
  asked of the *profile*, and `Profile::StructuredData` holds six formats of
  which `bp-data` parses four. The fix was not a better `match`; it was
  asking the question of the format.

## Tier 1 — pure wiring (the capability exists and is tested)

**All three original Tier 1 items are done** (W4, W6, W7 — see the table
above). What was learned doing them, for whoever wires the next row:

- `EditorSurface` had **two** constants that had to start following
  `font-size`, not one: `line-height` and `gutter-width`. Both feed
  `row-at` / `column-at`, so a constant left behind does not merely look
  wrong — it moves the caret away from the pointer, and further the further
  down or across the click is. Anything else that scales has the same trap.
- `menus::view` now takes the font size, so **every menu builder whose rows
  depend on a value must be handed that value**; there is no ambient state
  in `menus.rs` and there should not be.
- A row that exists but cannot act right now uses `row_enabled(.., false)`,
  which is deliberately *not* `planned()`. `planned` means "does not exist
  yet"; greying for a reason the user can act on is a different statement.

## Tier 2 — done

All of it, in one pass. What is worth carrying forward:

- **`Command::Indent` is not `Command::Insert("	")`.** What a soft tab
  inserts depends on the caret's visual column -- at column 3 with width 4 it
  is one space, not four -- so `command_for` cannot decide it and
  `Editor::apply` expands it instead. Any future key whose text depends on
  where the caret is has the same shape.
- **Save Copy shares nothing with `save_document` except the encoder.** The
  three things it must not do -- move the path, clear the dirty flag, touch
  Open Recent -- are the three that one does. Calling it and subtracting three
  behaviours is how Save Copy becomes Save As; there is a test for each.
- **A `PopupWindow`'s `x` cannot be assigned from outside it.** The tab
  context menu positions itself through a property the popup binds to. The
  compiler catches this, but only once you try.
- **`menus::insert` is rebuilt on every refresh**, because its row hints are
  rendered from the clock. A menu built once at startup would offer this
  morning's time all afternoon.
- **The tab menu's target is stored, not passed.** Opening the menu and
  clicking a row are separate events, and by the second one the pointer has
  moved.

## Open items

- **A claim in the record is not a property of the repository.** D13 sat
  unanswered for five sessions as "111 commits live on
  `feat/phase-01-foundation-ui`; `main` has none of them", and every
  housekeeping pass repeated it, several of them calling it "the item most
  likely to hurt and least likely to be noticed".

  It was false. `main` is at `31f6858`, *"Merge phases 1-19 into main
  (PR #1)"* — most of the branch's history is already in `main`, and the
  branch is 48 commits past that merge rather than 111 commits past nothing.
  One `git merge-base --is-ancestor` settles it, and it was never run.

  **This is trap 3 with the record in place of a comment**, and the mechanism
  is the one that makes trap 3 expensive: nothing depended on the claim, so
  nothing could contradict it. A question nobody can act on gets re-read
  rather than re-checked, and re-reading is what preserved it.

  The generalisation, and it is cheap: **a question that has waited several
  sessions should have its premise checked before it is asked again.** The
  reason it is still open may be that it was never the right question.


- **A flake that gets worse the more you run the tests.**
  `signing_reaches_the_security_history` failed one run in five *in
  isolation*. The cause was a temp path built from the process id and a
  counter -- unique within a run, and never cleaned up, so with Windows
  recycling pids a test process eventually inherited an earlier run's
  security history or signing key. Measured when found: 5,862 leftover files
  across 495 distinct pids.

  **The shape is what makes it worth writing down.** The probability of
  failure rises with the number of times the suite has been run, so the
  machine that runs the tests most is the one that trusts them least -- and it
  degrades slowly enough that every individual session reasonably concludes it
  saw a one-off. Two sessions had.

  The doc comment on the function was not wrong. It said the pid is there "so
  two runs at once cannot collide", which is true. **It answered the question
  it had thought of.** The dangerous case was two runs that were *not* at
  once, and nothing in the comment, the name or the tests pointed at it.

  Fixed in `crate::testpaths`, one home for both callers -- the same trap
  `bp_platform::paths::reserved_device_name` exists for, in a different
  costume: two places inventing the same scheme and getting it wrong the same
  way.

- **A clipped label is not a cosmetic defect when the label is a reason.**
  `MenuPopup` is a fixed width and clips, so `Sign Document — this document
  has never b` had been shipping since the row was built. Every label it
  happened to was a *greyed row's reason* -- the one text on screen whose
  whole job is to explain why something cannot be clicked.

  Found by driving the window, and it needed both halves of a fix: `overflow:
  elide` so a truncation looks like one, and shorter wording so it does not
  happen. **A stylesheet cannot decide what the words should be**, and a
  shorter word is not a rule anything enforces -- so there is now a test with
  a deliberately generous budget, which is a smoke alarm rather than a ruler.


- **A backlog that was mostly finished work, and nobody had looked.**
  `menus::planned_menu` listed 38 greyed rows across eight menus, each under a
  "not implemented yet (phase N)" line. An inventory taken before building
  anything found most had already shipped under another name: six of the
  Security menu's seven, five of the Data menu's six, three of Insert's five.
  The genuine gap was 21 rows, and eleven of *those* were better deleted than
  built.

  **The mechanism is worth naming, because it is not laziness.** Each row was
  correct when written. A feature would land under a name somebody chose at
  the time -- "Hash Document (SHA-256)" -- while the planned list kept the
  name from the original sketch -- "Hash and Sign". Nothing connected them, so
  nothing ever said the row was done. The list did not rot; it was never
  attached to anything that could tell it the truth.

  This is [ADR-0046](docs/decisions/ADR-0046.md)'s "a name that has sat in a
  plan long enough starts to read like a specification" at ten times the
  scale, and with a second edge: **the Data menu contradicted itself in one
  popup.** For a `.txt` it said "nothing for TXT documents" and then listed
  six things underneath. It had done since the menu was built, and nobody
  reading either half had read the other.

  The fix that will hold is not the pruning, it is
  `no_menu_offers_a_row_that_does_nothing` -- a test over a single list of
  every menu. **A plan that no test can contradict is a plan that will drift.**

- **Ten built, eleven deleted, and the deletions carry more information.**
  Every removal in ADR-0048 records *why* a row could not honestly exist:
  Run All against ADR-0038's no-persistent-session model, Benchmarks against
  an empty `benches/`, Semantic Search against embeddings nothing computes,
  Multi-cursor against a `TextInput` that can draw one caret. Each of those is
  a sentence somebody would otherwise have had to rediscover by starting the
  work.


- **A comment sat above the wrong rule, and a whole chain of correct reasoning
  hung off it.** `.gitignore` reads:

  ```
  # Per-run local CI records. Benchmark results under artifacts/benchmarks/ are
  # deliberately NOT ignored -- they are the evidence behind ADR-0015.
  artifacts/test-evidence/*.json
  ```

  The "deliberately NOT ignored" is about `artifacts/benchmarks/`, named one
  clause earlier. The line underneath carries no `!` and ignores the records
  outright. Reading the comment as describing the line below it -- which is
  what a comment above a line normally does -- produced the confident and
  false conclusion that the gate's run records are tracked, and that
  conclusion was written into ADR-0047, two code comments and a commit message
  before `git check-ignore` was run on it. It takes a second.

  **This is trap 3 -- "a claim in a comment is not a property of the code" --
  sprung inside the ADR that names trap 3**, which is worth keeping precisely
  because knowing the trap plainly does not prevent it.

  The generalisation, and it is narrower and more useful than trap 3 alone:
  **when a comment sits above a rule, check which rule it is about.** A
  comment describing one thing and physically adjacent to another is not a
  wrong comment -- it is a correct comment in a place that invites a wrong
  reading, which is why nobody has ever fixed it. The reasoning built on top
  can be sound end to end and still be about the wrong line.

- **"A flipped bit anywhere in a `.sig` never verifies" is false, correctly.**
  `Sidecar::parse` tolerates trailing whitespace so a signature survives being
  mailed and pasted, so flipping bit 0 of the closing newline yields a
  vertical tab and the file still verifies -- the document really is
  unaltered and the signature really does hold. The property with teeth, and
  the one now tested over every byte and every bit, is that a damaged sidecar
  verifies *if and only if* it parses back to the identical key and
  signature. Recorded because the naive version is the one somebody will
  write next.

- ~~**The recent-files list roams on Windows, and it is full of absolute
  paths.**~~ **Fixed**, and all three files moved together, which is why it
  waited for one change owning `bp-config` and `bp-ui`. `recent.toml`, the
  recovery journal and the security history all hung off the config
  directory, which on Windows is `%APPDATA%` and roams between machines --
  and all three hold machine-specific absolute paths, which is exactly what
  `bp_platform::DirKind::roams` warns against. All three now resolve through
  `DirKind::State`.

  Three things worth carrying forward. **A path derived by subtracting
  another path's last component moves when that one does**: the security
  history was `recovery_dir()` with `with_file_name`, so renaming the
  recovery folder would have moved the history silently, and a history that
  moves starts again at sequence one. It asks for the state directory
  directly now. **The rule is separated from the edge that reads the
  environment** -- `recovery_dir_under` and `audit_path_under` take the state
  directory as an argument, the same split `config_path` and `config_path_in`
  make, so both are assertable without a real profile. And **`cargo test` no
  longer creates a recovery directory in the developer's own profile**;
  `recovery_dir()` is `cfg(test)`-redirected to a unique temp path, which is
  the fix the security history already took after it wrote a live history
  into `%APPDATA%` once.

  Nothing migrates the old locations. A recent list of ten paths rebuilds
  itself on the first open; a recovery journal is by definition transient.
  The files left behind in `%APPDATA%\bachelorpad\` are orphaned and can be
  deleted.

- ~~**`Profile::is_data` answers a question it cannot.**~~ **Fixed.** It
  documented itself as "whether the Data menu's operations apply" and said
  yes for `Ini` and `Xml`, which are structured data with no parser in
  `bp-data` -- so the first caller to gate a menu on it would have got a menu
  whose every row failed. The class question is `Profile::is_data_class` and
  says outright that it is not the other one; the menu question is
  `Format::has_data_operations`, asked of the *format*. `Format::ALL` came
  with it, and a test walks it against `bp_ui::menus::data` so the predicate
  and the menu are one truth rather than two that can drift.

  Found by the cross-crate tests and latent for a session, which is the
  argument for the whole tier: nothing was wrong today, and the row that
  would have gone wrong had not been written yet.
- **Neither `bp-platform-windows` nor `bp-platform-linux` was created**, and
  that is a decision rather than an omission. specs §20 and the crate map
  name them. Everything they would hold is either already a parameterised
  function in `bp-platform` -- tested on both legs -- or blocked on D8 and
  the signing-key question. ADR-0001's own warning about `cfg`-gated code
  argues against two crates whose contents each CI leg never compiles.
  Revisit when something genuinely platform-specific has to be linked.

- **Time to first interaction is unmeasurable under the software renderer.**
  Slint exposes no rendering notifier there, so half of the ADR-0017 startup
  target has no measurement behind it. A hole, not a pass.
- **Scroll smoothness under CPU rasterisation is unmeasured**, and needs a
  capture rig. This is the standing revert condition on ADR-0017. The
  application's own typing-path latency *is* now measured and is not a
  problem: 376 µs p50 at 1 MB against a ~16 ms frame budget.
- **Comparative startup timing on Linux is unmeasured.** The shell builds and
  runs there, but WSLg's compositor makes timing unrepresentative. Needs a
  native Linux machine.
- ~~Keyboard shortcut delivery is unverified.~~ **Confirmed working**
  (2026-08-17, manual test). `KeyBinding` in a wrapping `FocusScope` matches
  during the capture phase, so the focused `TextInput` no longer swallows
  Ctrl+S.
- **The three dependants now read the security profile.** The recovery
  journal refuses rather than writing plaintext, and deletes what a looser
  profile already wrote; clipboard history stops recording and is cleared;
  and `bp-storage`'s `record_document` drops the title under `PathOnly` and
  records nothing under `Disabled`. Privacy Mode clamps all of it for the
  session without replacing the document's profile, so leaving it restores
  what the document had rather than the default.

  What ADR-0020 required to fail loudly no longer has to. The two profiles
  wanting an encrypted recovery journal are honoured, because `bp-crypto`
  ships and ADR-0022 seals the journal with the document's own passphrase.
  The remaining hole is narrower, and is the next item: a *plaintext*
  document under those profiles has no key, so it gets no journal at all.
- **The custom editor view has now been typed into, and most of it works.**
  A person drove both views on 2026-08-20. Keys arrive, Ctrl+S saves, the
  recovery journal fires on its own, the Security menu reaches its own bottom
  row, and **the caret tracks the pointer**: a click on a line reported that
  line, and a click at the end of `line 30` reported `Ln 30, Col 8`, which is
  exactly its length plus one. Ln/Col -- the whole payoff of the view -- is
  live in the status bar.
  
  What that pass found was **tabs**, and it is fixed: see below. Still
  unchecked are the wheel direction, drag-to-select and behaviour on resize.
  `project/NEXT_SESSION.md` has what is left.

  This used to carry "nothing depends on the answers yet". **That is no
  longer true.** Duplicate Line and Move Line Up/Down are enabled only under
  `--editor-view`, and Go to Line will be, so features now inherit whatever
  that caret does. The default is still `TextInput`, so nothing is *broken*
  by the answer being bad -- but a growing set of the product is unusable
  until somebody gives it.
- **A clipboard menu row can act on a different entry than the one it
  names.** Row ids are decoded against the clipboard history as it is at the
  moment of the click, not as it was when the menu was built, and a 1.2 s
  poll timer can add an entry in between. Predates the paste-transformation
  rows and applies equally to plain paste. Fixing it properly means freezing
  a snapshot of the history while a menu is open, which is a design question
  rather than a patch.
- **A file just over the huge threshold reads as past itself.**
  `Access::ReadOnlyBySize`'s message renders both the file's size and the limit
  through `format_bytes`, which rounds to whole MiB above 1 MiB -- so a
  192 MiB + 1 byte document produces *"Read-only: 192 MiB is past the 192 MiB
  editing limit."* Every word of that is true and the sentence looks
  self-contradictory. Only files within rounding distance of the threshold are
  affected, which is why nothing caught it: the tests assert the message names
  the size and blames neither permissions nor a missing viewer, and it does all
  three.

  Found while writing a manual-pass checklist, which is a second argument for
  writing one. Fixing it means either a decimal place at MiB (`192.0 MiB` past
  `192.0 MiB` -- no better), or the exact byte count in parentheses, or
  rounding the *limit* down and the *size* up so the two can never print equal.
  The third is the only one that reads correctly at every size, and it is a
  change to `bp-buffer`'s formatter rather than to the message.

- **The status bar says "Never saved" about a file that was opened from
  disk.** ADR-0005 means "no save has happened in this session", and that is
  what the field holds; what it *reads* as is "this file has never been
  saved", which is false for every document loaded from a path. Found by
  looking at it. A wording change, not a behaviour one.
- **Under `TextInput` a tab is still drawn as one glyph**, because Slint owns
  that rendering and we do not. It has no visible consequence today: that
  view reports a line count rather than Ln/Col, so there is no column readout
  to disagree with the glyphs. It becomes one the moment anything
  column-shaped is offered there.
- **Most of the product has still not been used, and the count keeps making
  the same point.** 1,754 tests cover the pieces, and every manual pass so far
  has found something none of them could. The first found a menu bar where
  twelve of fourteen menus swallowed clicks, and Save As defaulting to the
  process working directory — which wrote real documents into a git checkout.
  The second found a tab drawn as one glyph while every column was computed as
  though it reached the next stop, so the caret on any tab-bearing line sat
  where the character was not, and the error grew with every tab further
  along.

  **Both defects lived in the same seam, and it is the seam a test in this
  repository cannot see: what a toolkit does with what it is handed.** More
  unit tests will not find the third one.

  What is unchecked now is listed in `project/NEXT_SESSION.md`. The wheel and
  the resize moved up that list when the huge-file viewer shipped, because
  scrolling is the whole of what a viewer does.

- **A signing key now exists on the machine of anyone who signs, and
  `cargo test` nearly put one on the developer''s.** It did, once, during the
  change that added the feature — a real Ed25519 private key in
  `%LOCALAPPDATA%`, written by a test run before the guard landed. Deleted,
  and the guard is `AppState`''s `signing_key` field.

  **This is the third time this crate has needed that fix**, after the
  security history (which wrote a live log into `%APPDATA%`) and the recovery
  journal. The shape is always the same: a function that reads the real
  profile directory, called from `AppState::new()` or from an operation any
  test can trigger. The fix is a field rather than a global — and here a
  `cfg(test)` redirect on the *function* would not have been enough, because
  "does a key exist" and the signing that follows must agree on one path.

  Worth a gate stage? Measured rather than assumed, and the answer is not
  obvious: a check that no test writes outside the temp directory would want
  to run the suite under a redirected `%LOCALAPPDATA%`, which is a stage that
  passes for the wrong reason if the redirection fails. Recorded here instead,
  because three occurrences is a pattern and the fourth will not announce
  itself either.

- **Three modes were sized as large and were only undecided**, and that is the
  most expensive mistake in this log. Phases 9, 12 and 13 -- storage,
  notebooks, research -- sat as "built with no way in" for four sessions,
  described each time as *modes* rather than menu rows, with `NEXT_SESSION.md`
  calling a notebook view "the largest unscoped thing in the product" three
  sessions running.

  None of them was large. Each was waiting on a question:

  - Notebooks needed to know what the mode is *for*. ADR-0038's
    no-persistent-session model makes exploratory analysis dishonest and a
    literate document free, because a runbook's examples are self-contained by
    intent. Choose that and no cell view is needed at all -- the notebook
    stays its own JSON in the ordinary editor and the surface is a menu.
  - Research needed two decisions read as two features rather than one
    contradiction: ADR-0039/0041 define synthesis over `bp-storage` and rule
    out the bibliography types, while `MENU_MAP.md` names citation metadata.
    Both are right; they are different features sharing a menu.
  - Storage was the same shape and was settled first, by ADR-0037.

  **Undecided reads like large**, and an estimate made before the question is
  answered measures the question rather than the work. Worth remembering the
  next time something is described as a mode.

- **A refusal outlived the only evidence that could have contradicted it.**
  Go to Line was recorded as needing "the caret `TextInput` does not expose",
  alongside four features that genuinely do. It needed the caret *moved*,
  which `set-selection-offsets` does and which Find Next had always relied on.

  What kept the error alive is the part worth keeping: the move used to be
  *invisible* -- nothing scrolled the viewport to the caret -- so even if
  somebody had removed the guard, a correct jump would have looked like
  nothing happening. The claim only became checkable once that was fixed,
  which happened for an unrelated reason a day earlier.

  Reading a caret and moving one had been filed under one word. So had
  "going somewhere" and "saying where you are" -- the second still does need
  the caret read, which is why the status bar cannot report `Ln`/`Col` under
  `TextInput` and Go to Line now can work anyway.

- **Find never scrolled to its match, and every test of it used a document
  that fitted on one screen.** Four manual passes and three of the same
  session's own runs confirmed Find "worked" on fixtures where the match was
  already visible -- which tested the search and nothing about the viewport.
  It surfaced only because a new feature jumped to a line a hundred below the
  fold and visibly did nothing.

  The methodology finding outlives the fix: **a fixture that fits on one
  screen cannot test anything about scrolling**, and most of this product's
  readouts are about where you are. It is now a row on the manual-pass
  checklist, phrased about the fixture rather than the feature.

- **Two keyboard defects masked each other in the custom surface.**
  `scroll_for_command` returned "handled" for every key it was given,
  including the `Command::Ignore` that `bp_editor::keys::command_for` produces
  for exactly the keys its own comment calls the window's -- so a huge
  document had no shortcuts at all, Ctrl+F among them, since the viewer
  shipped. Behind it, `forward-focus: editor` named the `TextInput`, invisible
  whenever `use-editor-view` is true, so nothing held the keyboard until you
  clicked.

  Fixing the first is what made the second visible: until keys arrived at all,
  there was no way to notice nothing was holding them. **"Handled" is a claim
  with a consequence somewhere else**, and the two `return true`s that caused
  it had been written to mean "there is nothing to do here".

- **A property test found a Windows device name four sessions after it was
  written.** `bp-integrity`'s manifest proptest generates file names from
  `[a-z]{1,8}`; a fresh seed produced `nul`, which Windows will not create as
  a file *or* a directory, so the fixture could not be built and the property
  was never reached.

  The generator now asks `bp_platform::paths::reserved_device_name` rather
  than carrying a list -- a second list is the exact failure `bp-naming`'s
  two-way agreement test exists to stop. **This is the fourth time this
  workspace has been bitten by Win32 device names**, after the sanitiser,
  `atomic_write`, and the env-root property test. The rule has one home; the
  recurring mistake is not asking it.

- **A name that has sat in a plan long enough starts to read like a
  specification.** `docs/product/MENU_MAP.md` listed five Research rows --
  research question, evidence, findings, methods, datasets -- from `9f98b8a`,
  the scaffold commit, where they were part of one sentence describing a menu
  nobody had designed. Four sessions of planning treated them as a backlog.
  They are the section headings of a research *paper*, and ADR-0039 had
  already decided the mode was something else; scoping them took an afternoon
  and resolved all five, two of them by deletion.

  **This is the mirror of "undecided reads like large", and it is the more
  dangerous of the two.** An unscoped mode at least *looks* unscoped, so
  somebody eventually asks. A named row in a table looks decided. Nothing
  about it invites the question, and it can therefore sit in a plan
  indefinitely, being counted.

  The tell, when it comes round again: **ask where the name came from.** If
  the answer is a commit that scaffolded the repository rather than a decision
  that chose it, it is a sketch, and it has been read as a plan ever since.

- **"Inspectable" is a claim about the product, not about the source.**
  ADR-0041 committed Research Report to "a plain aggregate query with a fixed,
  inspectable rule", and every rule was indeed inspectable -- in
  `state/research.rs`. Meanwhile the report scanned the five hundred
  most-recently-seen documents and told the reader nothing about it, so a
  store of nine hundred produced a report that read as complete. Both things
  were true at once for a session: the rule was written down, and the person
  it was written for could not see it.

  The fix was six lines saying what the thresholds are and when the truncation
  bit. The lesson is the same shape as the comment-versus-code trap that
  precedes it: **ask of any honesty commitment, honest to whom?** If the answer
  is "to whoever reads this file", it has not been kept.

- **A comment and a test disagreed inside the same file, and both were read
  as right.** `Store::forget_document`'s doc comment promised "and with it any
  tags that were only on it"; forty lines below, its own test asserted
  `"the tag itself survives; nothing carries it"`. The code did the second.
  Nobody noticed, because every tag-shaped query in `bp-storage` joins
  `document_tags` and so cannot see an orphaned tag -- there was no query whose
  answer depended on which of the two was true, until `Store::summary` had to
  count tags and pick one.

  **Two claims about the same function can coexist for as long as nothing
  asks.** That is a different failure from a comment that is merely wrong: a
  wrong comment is one mistake, and this was two readers each correctly
  describing what they were looking at. The generalisation for this repository
  is that the question "what would fail if this stopped being true?" has a
  second edge -- when the answer is "nothing", the comment is a wish *and* the
  test beside it is free to say the opposite.
