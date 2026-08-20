// The strategy helpers below are not `#[test]` functions, so
// `clippy.toml`'s `allow-unwrap-in-tests` does not cover them -- but a
// `Date` the generator was just told to build is a fixture, and a fixture
// that cannot be built has no property to check.
#![allow(clippy::unwrap_used)]

//! Property tests for the filename grammar.
//!
//! The unit tests in the crate pin down specific cases from specs.md. These
//! check the invariants that must hold for *every* input, which is where
//! filename bugs actually live: a title someone pastes from a web page, a
//! CJK note name, a 400-character heading.

use bp_naming::{MAX_COMPONENT_BYTES, SemanticName, Stamp, render, sanitize_title};
use proptest::prelude::*;
use time::{Date, Month, Time, UtcOffset};

/// Any real calendar date. Days stop at 28 so every month is valid without
/// the strategy having to know about leap years.
fn any_date() -> impl Strategy<Value = Date> {
    (2000i32..2100, 1u8..=12, 1u8..=28)
        .prop_map(|(y, m, d)| Date::from_calendar_date(y, Month::try_from(m).unwrap(), d).unwrap())
}

/// Any time of day, to the second.
fn any_time() -> impl Strategy<Value = Time> {
    (0u8..24, 0u8..60, 0u8..60).prop_map(|(h, m, s)| Time::from_hms(h, m, s).unwrap())
}

/// Any whole-hour offset from UTC, east and west. Whole hours keep the
/// minutes and seconds components at zero, which trivially satisfies
/// `UtcOffset`'s rule that all three components share one sign.
fn any_offset() -> impl Strategy<Value = UtcOffset> {
    (-23i8..=23).prop_map(|h| UtcOffset::from_hms(h, 0, 0).unwrap())
}

/// Titles without `_`. Underscores are legal in titles and handled by the
/// unit tests, but a title ending in `_v2` is genuinely ambiguous with a
/// revision suffix, so it cannot appear in a round-trip property.
fn any_title() -> impl Strategy<Value = String> {
    "[A-Za-z0-9 .()&,'-]{1,60}"
}

fn any_extension() -> impl Strategy<Value = String> {
    "[a-z]{1,8}"
}

proptest! {
    /// The core guarantee: what we write, we can read back.
    #[test]
    fn render_parse_round_trips(
        title in any_title(),
        date in any_date(),
        ext in any_extension(),
        revision in prop::option::of(2u32..50),
        collision in prop::option::of(2u32..500),
    ) {
        let name = SemanticName::new(&title, date, &ext)
            .with_revision(revision)
            .with_collision(collision);

        let parsed = SemanticName::parse(&name.to_filename());
        prop_assert_eq!(parsed.as_ref(), Some(&name));
    }

    /// Every rendered name is a legal path component on both platforms.
    #[test]
    fn rendered_names_are_always_writable(
        title in ".{0,400}",
        date in any_date(),
        ext in ".{0,40}",
    ) {
        let f = SemanticName::new(&title, date, &ext).to_filename();

        prop_assert!(!f.is_empty());
        prop_assert!(f.len() <= MAX_COMPONENT_BYTES, "{} bytes", f.len());
        prop_assert!(f.contains('.'), "no extension in {f:?}");
        prop_assert!(
            !f.chars().any(|c| "<>:\"/\\|?*".contains(c) || c.is_control()),
            "forbidden character in {f:?}"
        );
        // Windows silently drops these, so a name ending in one is a name we
        // would not get back.
        prop_assert!(!f.ends_with(' ') && !f.ends_with('.'), "{f:?}");
    }

    /// Sanitising is idempotent -- running it twice must not keep eroding the
    /// title, or a repeatedly-renamed note would lose its name over time.
    #[test]
    fn sanitize_is_idempotent(title in ".{0,200}") {
        let once = sanitize_title(&title);
        prop_assert_eq!(sanitize_title(&once), once.clone());
        prop_assert!(!once.is_empty());
    }

    /// Collision resolution must never hand back an occupied name.
    #[test]
    fn resolve_collision_never_returns_a_taken_name(
        title in any_title(),
        date in any_date(),
        taken_count in 0usize..20,
    ) {
        let name = SemanticName::new(&title, date, "txt");

        // Occupy the bare name plus the first `taken_count` suffixes.
        let taken: Vec<String> = std::iter::once(name.to_filename())
            .chain((2..).take(taken_count).map(|n| {
                name.clone().with_collision(Some(n)).to_filename()
            }))
            .collect();

        let resolved = name
            .resolve_collision(|candidate| taken.iter().any(|t| t == candidate))
            .expect("plenty of suffixes remain");

        prop_assert!(!taken.contains(&resolved.to_filename()));
    }

    /// Parsing arbitrary text must not panic. Filenames come from the
    /// filesystem, not from us, so this path is effectively untrusted input.
    #[test]
    fn parse_never_panics(s in ".{0,120}") {
        let _ = SemanticName::parse(&s);
    }

    /// A stamp must render for any date, time and offset the type can
    /// represent -- a document's insertion point does not get to reject a
    /// user's clock, however far from today it is set.
    #[test]
    fn render_never_panics(
        date in any_date(),
        time in any_time(),
        offset in any_offset(),
        stamp in prop::sample::select(Stamp::all()),
    ) {
        let at = date.with_time(time).assume_offset(offset);
        let _ = render(stamp, at);
    }
}
