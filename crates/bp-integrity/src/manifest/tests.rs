//! Tests for the hash manifest. Every one of them in a `tempfile`
//! directory; nothing here writes into the checkout.

use super::*;
use proptest::prelude::*;
use std::fs;
use tempfile::{TempDir, tempdir};

/// The published SHA-256 of no bytes at all -- the value `sha256sum` gives an
/// empty file, so a user comparing ours against theirs sees one string.
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn bundle(files: &[(&str, &[u8])]) -> TempDir {
    let dir = tempdir().unwrap();
    for (name, contents) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, contents).unwrap();
    }
    dir
}

fn manifest_over(dir: &TempDir, names: &[&str]) -> Manifest {
    Manifest::build(dir.path(), names, HashAlgorithm::Sha256).unwrap()
}

// --- hashing one file ------------------------------------------------------

#[test]
fn hashing_a_file_gives_the_digest_every_other_tool_gives_it() {
    // If this drifts, a user comparing our checksum against `sha256sum` sees
    // a mismatch that is ours.
    let dir = bundle(&[("abc.txt", b"abc")]);

    assert_eq!(
        hash_file(&dir.path().join("abc.txt"), HashAlgorithm::Sha256)
            .unwrap()
            .to_hex(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn an_empty_file_hashes_to_the_published_empty_digest() {
    let dir = bundle(&[("empty.txt", b"")]);

    assert_eq!(
        hash_file(&dir.path().join("empty.txt"), HashAlgorithm::Sha256)
            .unwrap()
            .to_hex(),
        EMPTY_SHA256
    );
}

#[test]
fn hashing_a_file_that_is_not_there_names_the_file() {
    let dir = tempdir().unwrap();
    let Err(error) = hash_file(&dir.path().join("gone.txt"), HashAlgorithm::Sha256) else {
        panic!("hashed a file that does not exist");
    };
    assert!(error.to_string().contains("gone.txt"));
}

// --- building --------------------------------------------------------------

#[test]
fn a_manifest_records_a_digest_for_every_file_it_was_given() {
    let dir = bundle(&[("a.txt", b"a"), ("b.txt", b"b")]);

    let manifest = manifest_over(&dir, &["a.txt", "b.txt"]);

    assert_eq!(manifest.len(), 2);
    assert_eq!(manifest.algorithm(), HashAlgorithm::Sha256);
    assert_eq!(manifest.entries()[0].path(), "a.txt");
    assert_eq!(
        manifest.entries()[0].digest(),
        hash_document(b"a", HashAlgorithm::Sha256).to_hex()
    );
}

#[test]
fn entries_are_sorted_whatever_order_the_caller_listed_them_in() {
    // The order a caller happened to pass is not information, and a manifest
    // that reorders itself between builds is a diff nobody can read.
    let dir = bundle(&[("a.txt", b"a"), ("b.txt", b"b"), ("c.txt", b"c")]);

    let forwards = manifest_over(&dir, &["a.txt", "b.txt", "c.txt"]);
    let backwards = manifest_over(&dir, &["c.txt", "a.txt", "b.txt"]);

    assert_eq!(forwards, backwards);
    assert_eq!(forwards.to_text(), backwards.to_text());
}

#[test]
fn a_manifest_over_no_files_is_allowed_and_says_it_covers_nothing() {
    // The empty case, and the trap in it: it checks clean, because it
    // asserts nothing. A caller that shows a green tick without asking
    // `is_empty` reports a clean bundle for an empty one.
    let dir = tempdir().unwrap();
    let manifest = Manifest::build(dir.path(), &[] as &[&str], HashAlgorithm::Sha256).unwrap();

    assert!(manifest.is_empty());
    let report = manifest.check(dir.path());
    assert!(report.all_match());
    assert!(report.summarise().contains("no files"));
}

#[test]
fn building_over_a_file_that_cannot_be_read_fails_rather_than_leaving_a_hole() {
    // A manifest with a gap in it would be checked later and pass.
    let dir = bundle(&[("a.txt", b"a")]);

    assert!(Manifest::build(dir.path(), &["a.txt", "missing.txt"], HashAlgorithm::Sha256).is_err());
}

#[test]
fn the_same_file_listed_twice_is_refused() {
    let dir = bundle(&[("a.txt", b"a")]);

    assert!(matches!(
        Manifest::build(dir.path(), &["a.txt", "a.txt"], HashAlgorithm::Sha256),
        Err(IntegrityError::UnusablePath { .. })
    ));
}

#[test]
fn a_path_the_manifest_format_cannot_hold_is_refused_when_it_is_built() {
    // The cases the type permits and the domain does not. `..` in particular
    // is a directory traversal aimed at whatever unpacks the bundle later.
    let dir = bundle(&[("a.txt", b"a")]);

    for path in ["../a.txt", "/etc/passwd", "./a.txt", ""] {
        assert!(
            Manifest::build(dir.path(), &[path], HashAlgorithm::Sha256).is_err(),
            "accepted {path:?} as a manifest path"
        );
    }
}

#[test]
fn a_file_in_a_subdirectory_is_recorded_with_forward_slashes() {
    // So a manifest written on Windows checks on Linux and the other way
    // round.
    let dir = bundle(&[("sub/deep/note.txt", b"note")]);

    let manifest = Manifest::build(
        dir.path(),
        &[Path::new("sub").join("deep").join("note.txt")],
        HashAlgorithm::Sha256,
    )
    .unwrap();

    assert_eq!(manifest.entries()[0].path(), "sub/deep/note.txt");
    assert!(manifest.check(dir.path()).all_match());
}

// --- the format ------------------------------------------------------------

#[test]
fn the_body_lines_are_exactly_sha256sum_format() {
    // The whole reason for the two-space separator: strip the two header
    // lines and `sha256sum -c` reads what is left.
    let dir = bundle(&[("note.txt", b"")]);
    let manifest = manifest_over(&dir, &["note.txt"]);

    let text = manifest.to_text();
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines[0], MANIFEST_MAGIC);
    assert_eq!(lines[1], "algorithm: SHA-256");
    assert_eq!(lines[2], format!("{EMPTY_SHA256}  note.txt"));
    assert!(text.ends_with('\n'));
}

#[test]
fn a_manifest_round_trips_through_its_text() {
    let dir = bundle(&[("a.txt", b"a"), ("sub/b.txt", b"b")]);
    let manifest = manifest_over(&dir, &["a.txt", "sub/b.txt"]);

    let parsed = Manifest::parse(&manifest.to_text()).unwrap();

    assert_eq!(parsed, manifest);
    assert_eq!(parsed.to_text(), manifest.to_text());
}

#[test]
fn a_manifest_round_trips_through_a_file_and_checks_the_same_way() {
    let dir = bundle(&[("a.txt", b"a"), ("b.txt", b"b")]);
    let manifest = manifest_over(&dir, &["a.txt", "b.txt"]);
    let path = dir.path().join("MANIFEST.txt");

    manifest.write(&path).unwrap();

    assert_eq!(Manifest::read(&path).unwrap(), manifest);
    assert!(Manifest::read(&path).unwrap().check(dir.path()).all_match());
}

#[test]
fn windows_line_endings_and_blank_lines_still_read() {
    let dir = bundle(&[("a.txt", b"a")]);
    let text = manifest_over(&dir, &["a.txt"]).to_text();
    let mangled = format!("{}\r\n\r\n", text.replace('\n', "\r\n"));

    assert_eq!(Manifest::parse(&mangled).unwrap().len(), 1);
}

#[test]
fn a_manifest_from_a_future_version_is_refused_by_its_marker() {
    let dir = bundle(&[("a.txt", b"a")]);
    let text = manifest_over(&dir, &["a.txt"])
        .to_text()
        .replace(MANIFEST_MAGIC, "BachelorPad+ manifest v2");

    assert!(matches!(
        Manifest::parse(&text),
        Err(MalformedManifest::WrongMagic { .. })
    ));
}

#[test]
fn an_unknown_algorithm_is_refused_rather_than_guessed() {
    // Checking a SHA-512 manifest with SHA-256 would report every file as
    // altered, which is the worst possible way to be wrong.
    let dir = bundle(&[("a.txt", b"a")]);
    let text = manifest_over(&dir, &["a.txt"])
        .to_text()
        .replace("algorithm: SHA-256", "algorithm: MD5");

    assert_eq!(
        Manifest::parse(&text),
        Err(MalformedManifest::UnknownAlgorithm {
            found: "MD5".to_owned()
        })
    );
}

#[test]
fn a_sha512_manifest_records_and_checks_sha512() {
    let dir = bundle(&[("a.txt", b"a")]);
    let manifest = Manifest::build(dir.path(), &["a.txt"], HashAlgorithm::Sha512).unwrap();

    assert!(manifest.to_text().contains("algorithm: SHA-512"));
    assert_eq!(manifest.entries()[0].digest().len(), 128);
    assert!(manifest.check(dir.path()).all_match());
    assert_eq!(Manifest::parse(&manifest.to_text()).unwrap(), manifest);
}

#[test]
fn a_digest_of_the_wrong_length_is_refused_with_its_line_number() {
    // The boundary and one past it, on the field whose length is fixed by the
    // algorithm.
    let dir = bundle(&[("a.txt", b"a")]);
    let text = manifest_over(&dir, &["a.txt"]).to_text();
    let body = text.lines().nth(2).unwrap();
    let (digest, path) = body.split_once("  ").unwrap();

    let shorter = digest[..digest.len() - 1].to_owned();
    let longer = format!("{digest}0");
    for short in [shorter.as_str(), longer.as_str()] {
        let broken = text.replace(body, &format!("{short}  {path}"));
        assert_eq!(
            Manifest::parse(&broken),
            Err(MalformedManifest::BadDigest {
                line: 3,
                path: path.to_owned(),
                expected: 64
            })
        );
    }
}

#[test]
fn an_uppercase_digest_is_refused_so_that_one_manifest_has_one_spelling() {
    let dir = bundle(&[("a.txt", b"a")]);
    let text = manifest_over(&dir, &["a.txt"]).to_text();
    let body = text.lines().nth(2).unwrap();

    let shouted = text.replace(body, &body.to_uppercase());
    assert!(Manifest::parse(&shouted).is_err());
}

#[test]
fn a_body_line_that_is_not_a_digest_and_a_path_is_refused() {
    let dir = bundle(&[("a.txt", b"a")]);
    let text = format!("{}nonsense\n", manifest_over(&dir, &["a.txt"]).to_text());

    assert_eq!(
        Manifest::parse(&text),
        Err(MalformedManifest::BadLine {
            line: 4,
            found: "nonsense".to_owned()
        })
    );
}

#[test]
fn a_traversal_path_in_a_manifest_that_arrived_from_elsewhere_is_refused() {
    // Paths are validated on the way in as well as on the way out, because a
    // manifest can come from anybody.
    let text = format!("{MANIFEST_MAGIC}\nalgorithm: SHA-256\n{EMPTY_SHA256}  ../../secrets\n");

    assert!(matches!(
        Manifest::parse(&text),
        Err(MalformedManifest::BadPath { line: 3, .. })
    ));
}

#[test]
fn a_manifest_that_contradicts_itself_about_one_file_is_refused() {
    let text = format!(
        "{MANIFEST_MAGIC}\nalgorithm: SHA-256\n{EMPTY_SHA256}  a.txt\n{}  a.txt\n",
        hash_document(b"a", HashAlgorithm::Sha256).to_hex()
    );

    assert_eq!(
        Manifest::parse(&text),
        Err(MalformedManifest::DuplicatePath {
            path: "a.txt".to_owned()
        })
    );
}

#[test]
fn a_manifest_file_that_is_not_text_is_refused_by_name() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("MANIFEST.txt");
    fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();

    assert!(matches!(
        Manifest::read(&path),
        Err(IntegrityError::NotText { .. })
    ));
}

#[test]
fn a_malformed_manifest_file_says_which_file_and_why() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("MANIFEST.txt");
    fs::write(&path, b"this is a shopping list\n").unwrap();

    let Err(error) = Manifest::read(&path) else {
        panic!("a shopping list parsed as a manifest");
    };
    let message = error.to_string();
    assert!(message.contains("MANIFEST.txt"), "{message}");
    assert!(matches!(
        error,
        IntegrityError::MalformedManifestFile {
            source: MalformedManifest::WrongMagic { .. },
            ..
        }
    ));
}

// --- checking, per file ----------------------------------------------------

#[test]
fn an_untouched_set_reports_every_file_as_matching() {
    let dir = bundle(&[("a.txt", b"a"), ("b.txt", b"b"), ("sub/c.txt", b"c")]);
    let manifest = manifest_over(&dir, &["a.txt", "b.txt", "sub/c.txt"]);

    let report = manifest.check(dir.path());

    assert!(report.all_match());
    assert_eq!(report.files().len(), 3);
    assert_eq!(report.failures().count(), 0);
    assert_eq!(report.summarise(), "All 3 files match the manifest.");
}

#[test]
fn only_the_file_that_changed_is_reported_as_changed() {
    // The whole reason a manifest reports per file: a pass/fail over two
    // hundred exhibits tells the user they have a problem and not where.
    let dir = bundle(&[("a.txt", b"a"), ("b.txt", b"b"), ("c.txt", b"c")]);
    let manifest = manifest_over(&dir, &["a.txt", "b.txt", "c.txt"]);

    fs::write(dir.path().join("b.txt"), b"B").unwrap();
    let report = manifest.check(dir.path());

    assert!(!report.all_match());
    let failures: Vec<&FileCheck> = report.failures().collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].path, "b.txt");
    assert_eq!(
        failures[0].outcome,
        FileOutcome::Differs {
            expected: hash_document(b"b", HashAlgorithm::Sha256).to_hex(),
            found: hash_document(b"B", HashAlgorithm::Sha256).to_hex(),
        }
    );
    assert_eq!(
        report.summarise(),
        "1 of 3 files do not match the manifest."
    );
}

#[test]
fn a_deleted_file_is_missing_rather_than_different() {
    // Three failure shapes rather than one: a deletion, an edit and a
    // permissions problem are three different events, and only one of them is
    // evidence of tampering.
    let dir = bundle(&[("a.txt", b"a"), ("b.txt", b"b")]);
    let manifest = manifest_over(&dir, &["a.txt", "b.txt"]);

    fs::remove_file(dir.path().join("a.txt")).unwrap();
    let report = manifest.check(dir.path());

    assert_eq!(report.files()[0].outcome, FileOutcome::Missing);
    assert_eq!(report.files()[1].outcome, FileOutcome::Matches);
}

#[test]
fn every_altered_file_is_reported_rather_than_only_the_first() {
    // A user whose bundle has three altered files needs all three names, not
    // the alphabetically first one and another run.
    let dir = bundle(&[("a.txt", b"a"), ("b.txt", b"b"), ("c.txt", b"c")]);
    let manifest = manifest_over(&dir, &["a.txt", "b.txt", "c.txt"]);

    fs::write(dir.path().join("a.txt"), b"x").unwrap();
    fs::remove_file(dir.path().join("b.txt")).unwrap();
    fs::write(dir.path().join("c.txt"), b"y").unwrap();

    let report = manifest.check(dir.path());
    let names: Vec<&str> = report.failures().map(|f| f.path.as_str()).collect();

    assert_eq!(names, ["a.txt", "b.txt", "c.txt"]);
    assert_eq!(
        report.summarise(),
        "3 of 3 files do not match the manifest."
    );
}

#[test]
fn checking_the_same_set_in_a_different_folder_works() {
    // The point of relative paths: a manifest survives its bundle being
    // moved, renamed or unpacked somewhere else.
    let original = bundle(&[("a.txt", b"a"), ("sub/b.txt", b"b")]);
    let manifest = manifest_over(&original, &["a.txt", "sub/b.txt"]);
    let elsewhere = bundle(&[("a.txt", b"a"), ("sub/b.txt", b"b")]);

    assert!(manifest.check(elsewhere.path()).all_match());
}

#[test]
fn a_manifest_says_nothing_about_a_file_that_was_added_afterwards() {
    // The documented limit, asserted so that nobody later mistakes it for a
    // completeness guarantee.
    let dir = bundle(&[("a.txt", b"a")]);
    let manifest = manifest_over(&dir, &["a.txt"]);

    fs::write(dir.path().join("planted.txt"), b"surprise").unwrap();

    assert!(manifest.check(dir.path()).all_match());
}

#[test]
fn an_empty_file_that_stays_empty_matches() {
    // The empty case at the other end: a zero-byte file has a digest like any
    // other, and must not be skipped or special-cased.
    let dir = bundle(&[("empty.txt", b"")]);
    let manifest = manifest_over(&dir, &["empty.txt"]);

    assert_eq!(manifest.entries()[0].digest(), EMPTY_SHA256);
    assert!(manifest.check(dir.path()).all_match());

    fs::write(dir.path().join("empty.txt"), b"\n").unwrap();
    assert!(!manifest.check(dir.path()).all_match());
}

#[test]
fn only_a_match_counts_as_a_match() {
    assert!(FileOutcome::Matches.is_match());
    for outcome in [
        FileOutcome::Missing,
        FileOutcome::Differs {
            expected: EMPTY_SHA256.to_owned(),
            found: EMPTY_SHA256.to_owned(),
        },
        FileOutcome::Unreadable {
            reason: "permission denied".to_owned(),
        },
    ] {
        assert!(!outcome.is_match(), "{outcome:?} counted as a match");
    }
}

// --- properties ------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Whatever a set of files contains, a manifest just built over it
    /// matches it -- and its text round-trips.
    #[test]
    fn any_set_matches_the_manifest_just_built_over_it(
        contents in prop::collection::vec(
            prop::collection::vec(any::<u8>(), 0..256),
            0..6,
        ),
    ) {
        let dir = tempdir().unwrap();
        let names: Vec<String> = (0..contents.len()).map(|i| format!("f{i}.bin")).collect();
        for (name, bytes) in names.iter().zip(&contents) {
            fs::write(dir.path().join(name), bytes).unwrap();
        }

        let manifest = Manifest::build(dir.path(), names.as_slice(), HashAlgorithm::Sha256).unwrap();

        prop_assert!(manifest.check(dir.path()).all_match());
        prop_assert_eq!(Manifest::parse(&manifest.to_text()), Ok(manifest));
    }

    /// Changing any one file makes exactly that file, and no other, report as
    /// differing. The "no other" half is what a per-file report is for: a
    /// check that flagged its neighbours would be useless for finding which
    /// file moved.
    #[test]
    fn changing_one_file_flags_that_file_and_only_that_file(
        contents in prop::collection::vec(
            prop::collection::vec(any::<u8>(), 0..128),
            1..6,
        ),
        victim in any::<prop::sample::Index>(),
        flip in 1u8..=255,
    ) {
        let dir = tempdir().unwrap();
        let names: Vec<String> = (0..contents.len()).map(|i| format!("f{i}.bin")).collect();
        for (name, bytes) in names.iter().zip(&contents) {
            fs::write(dir.path().join(name), bytes).unwrap();
        }
        let manifest = Manifest::build(dir.path(), names.as_slice(), HashAlgorithm::Sha256).unwrap();

        let index = victim.index(names.len());
        let mut altered = contents[index].clone();
        altered.push(flip);
        fs::write(dir.path().join(&names[index]), &altered).unwrap();

        let report = manifest.check(dir.path());
        let flagged: Vec<&str> = report.failures().map(|f| f.path.as_str()).collect();

        prop_assert_eq!(flagged, vec![names[index].as_str()]);
    }

    /// No manifest this crate can build holds a path that would escape the
    /// root it is checked against.
    #[test]
    fn no_built_manifest_holds_a_path_that_escapes_its_root(
        depth in 1usize..4,
        stem in "[a-z]{1,8}",
    ) {
        let dir = tempdir().unwrap();
        let relative: String = std::iter::repeat_n(stem.as_str(), depth)
            .collect::<Vec<_>>()
            .join("/");
        let path = dir.path().join(&relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"x").unwrap();

        let manifest = Manifest::build(dir.path(), &[&relative], HashAlgorithm::Sha256).unwrap();

        for entry in manifest.entries() {
            prop_assert!(!entry.path().contains(".."));
            prop_assert!(!entry.path().starts_with('/'));
            prop_assert!(join(dir.path(), entry.path()).starts_with(dir.path()));
        }
    }
}
