//! Seam: this build against files a *previous* build left on somebody's disk.
//!
//! **Every other format test in this workspace writes and then reads back**,
//! which proves the two halves of one build agree with each other and says
//! nothing whatever about what is already on a disk. That is trap 7 in
//! `CLAUDE.md`, and [ADR-0050](../../../docs/decisions/ADR-0050.md) found it
//! for `.bpadx` -- three golden envelopes had been checked in since the corpus
//! was seeded and the harness fed them to `open` and discarded the result.
//!
//! **These are the three files this product leaves behind**, and they are the
//! ones a user actually notices going missing:
//!
//! There were four, with `.bpadx` golden vectors beside them. ADR-0064
//! removed encryption and the security history, so `security.log` and
//! `fuzz/tests/envelope.rs` went with the code that read them. **The fixture
//! was deleted rather than kept or regenerated**: a committed byte string
//! with no reader is evidence about nothing, which is the argument ADR-0058
//! made when it deleted a hash manifest. Trap 7's coverage here is three
//! files now, and saying so is the point of this paragraph.
//!
//! | File | Where | What losing it costs |
//! | --- | --- | --- |
//! | `config.toml` | `DirKind::Config` | Every setting silently back to default |
//! | `recent.toml` | `DirKind::State` | The recent list, silently emptied |
//! | `recovery/*.json` | `DirKind::State` | **Unsaved work**, which is the worst of the four |
//!
//! ## These bytes are fixtures, not expectations
//!
//! Each literal below was produced by the build of 2026-08-23 and is checked
//! in **as a record of what shipped**. Regenerating one to make this file
//! pass is the mistake it exists to prevent: if a literal no longer parses,
//! the format changed, and the question is whether that was meant. A
//! deliberate version bump is fine -- and then this build must still read the
//! old shape, which is what the assertion actually is.
//!
//! ## What this cannot check
//!
//! Only formats that have already shipped once. A field added tomorrow is
//! covered from the day somebody adds a literal here carrying it, and not
//! before -- so the habit that makes this file worth having is adding the new
//! shape *beside* the old one rather than editing the old one.

use bp_config::{Env, Recent, RendererPref, resolve};
use bp_history::{Checkpoint, CheckpointEncoding, CheckpointLineEnding};

/// `config.toml`, as the settings documentation describes it.
///
/// This one is the least at risk and the cheapest to hold still: nothing in
/// the product *writes* it, so there is no round trip to give false comfort
/// in the first place, and `resolve` takes the text as a value. What the
/// literal pins is the **key names** -- `font_size` with an underscore, and
/// not the `font-size` the command line spells it with.
const CONFIG_TOML: &str = "\
theme = \"Green\"
renderer = \"software\"
log = \"warn\"
font_size = 18
tab_width = 8
indent_spaces = true
";

/// `recent.toml`, written by `Recent::to_toml`.
const RECENT_TOML: &str = "\
paths = [
    \"/notes/beta.md\",
    \"/notes/alpha.txt\",
]
";

/// One recovery checkpoint, in the shape this build writes.
///
/// `bp-history` already tests the *older* shape -- the one with no `encoding`
/// and no `line_ending` -- which is the more urgent direction and is why that
/// test exists. This is the other end of the same guarantee: the shape being
/// written today, held still so tomorrow's build cannot stop reading it.
const CHECKPOINT_JSON: &str = r#"{
  "path": "/notes/alpha.txt",
  "name": "alpha.txt",
  "text": "unsaved work\n",
  "written_at": 1700000000,
  "encoding": "Utf8",
  "line_ending": "Lf"
}"#;

#[test]
fn a_config_file_from_an_earlier_build_still_sets_every_setting() {
    let (config, notices) = resolve(Some(CONFIG_TOML), &Env::default(), &[]);

    assert!(
        notices.is_empty(),
        "a config file this product shipped is now reported as a problem: {notices:?}"
    );
    assert_eq!(config.theme.as_deref(), Some("Green"));
    assert_eq!(config.renderer, RendererPref::Software);
    assert_eq!(config.log, "warn");
    assert_eq!(config.font_size, 18);
    assert_eq!(config.tab_width, 8);
    assert!(config.indent_spaces);
}

#[test]
fn a_recent_list_from_an_earlier_build_is_not_silently_emptied() {
    // `Recent::parse` ends in `unwrap_or_default()`, so a format change here
    // does not fail -- it returns an empty list. The user opens the product
    // and their recent files are simply gone, with nothing said and nothing
    // logged. **That silence is the reason this test is worth more than its
    // three lines**, and why it asserts the contents rather than merely that
    // parsing did not panic.
    let recent = Recent::parse(RECENT_TOML);

    let paths: Vec<String> = recent
        .paths()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    assert_eq!(
        paths,
        vec!["/notes/beta.md".to_owned(), "/notes/alpha.txt".to_owned()],
        "a recent list this product wrote no longer reads back. \
         `parse` swallows the failure, so the user sees an empty list rather \
         than an error -- check the format before regenerating this fixture"
    );
}

#[test]
fn a_recovery_checkpoint_from_an_earlier_build_still_holds_the_unsaved_text() {
    // The worst of the four to lose, and the only one that is somebody's work
    // rather than somebody's convenience.
    let checkpoint: Checkpoint = serde_json::from_str(CHECKPOINT_JSON).unwrap_or_else(|e| {
        panic!(
            "a recovery checkpoint written by an earlier build no longer \
             deserialises: {e}. Unsaved work already on disk is unrecoverable"
        )
    });

    assert_eq!(checkpoint.text, "unsaved work\n");
    assert_eq!(checkpoint.name, "alpha.txt");
    assert_eq!(checkpoint.written_at, 1_700_000_000);
    assert_eq!(checkpoint.encoding, CheckpointEncoding::Utf8);
    assert_eq!(checkpoint.line_ending, Some(CheckpointLineEnding::Lf));
    assert_eq!(
        checkpoint.path.as_deref(),
        Some(std::path::Path::new("/notes/alpha.txt"))
    );
}
