//! Date/time text for insertion into a document (specs.md section 4,
//! "date/time insertion").
//!
//! Like the rest of this crate, this module is pure: it renders whatever
//! instant it is given and never reads a clock. Whether that instant is
//! local or UTC, and when it was captured, are the caller's decisions --
//! `render` only turns the decision already made into text.

use time::OffsetDateTime;

use crate::date::format_date;

/// A way of writing a date and time into a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stamp {
    Date,
    Time,
    DateAndTime,
    Iso8601,
    FilenameDate,
}

/// English month names in full, for prose. `date.rs`'s uppercase three-letter
/// abbreviations exist for the filename grammar and would read like a
/// shouted abbreviation in a sentence, so this is a separate table rather
/// than a reuse of that one.
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Render `at` in the given format.
pub fn render(stamp: Stamp, at: OffsetDateTime) -> String {
    match stamp {
        Stamp::Date => date(at),
        Stamp::Time => time_of_day(at),
        Stamp::DateAndTime => format!("{}, {}", date(at), time_of_day(at)),
        Stamp::Iso8601 => iso8601(at),
        // Reuse date.rs's formatter rather than reimplementing DDMMMYYYY, so
        // there is exactly one place that grammar is written.
        Stamp::FilenameDate => format_date(at.date()),
    }
}

/// `16 August 2026` -- day before month, matching the day-first order the
/// filename grammar already uses (ADR-0003), so the product never asks a
/// reader to guess whether "the 8th" or "August" comes first. The month is
/// spelled out because this is prose for a document body, not a sortable
/// key -- `Iso8601` and `FilenameDate` cover that need.
fn date(at: OffsetDateTime) -> String {
    format!(
        "{} {} {}",
        at.day(),
        MONTHS[usize::from(u8::from(at.month())) - 1],
        at.year()
    )
}

/// `8:05 PM` -- deliberately identical to `clock()` in
/// `crates/bp-ui/src/state.rs`, which renders the status bar's save times
/// per specs.md section 3. Two different renderings of the same clock in one
/// product is the defect this guards against, so the hour/minute logic here
/// is a direct copy of that function's, not a reinvention of it.
fn time_of_day(at: OffsetDateTime) -> String {
    let (hour, meridiem) = match at.hour() {
        0 => (12, "AM"),
        h @ 1..=11 => (h, "AM"),
        12 => (12, "PM"),
        h => (h - 12, "PM"),
    };
    format!("{hour}:{:02} {meridiem}", at.minute())
}

/// `2026-08-16T20:05:00+05:30` (or `...Z` for UTC) -- RFC 3339, the strict
/// profile of ISO 8601: always zero-padded, always to the second, always an
/// explicit offset. This is the format a script or another program reads
/// back, so ambiguity is the one thing it cannot have.
fn iso8601(at: OffsetDateTime) -> String {
    let offset = at.offset();
    let offset_text = if offset.is_utc() {
        "Z".to_owned()
    } else {
        // `as_hms` gives all three components the same sign (time's own
        // guarantee), so the hour alone decides the leading sign and the
        // rest can be printed unsigned.
        let (hours, minutes, _) = offset.as_hms();
        format!(
            "{}{:02}:{:02}",
            if hours < 0 { '-' } else { '+' },
            hours.abs(),
            minutes.abs()
        )
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{offset_text}",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    )
}

impl Stamp {
    /// Menu label, e.g. "Date and Time".
    pub fn label(self) -> &'static str {
        match self {
            Self::Date => "Date",
            Self::Time => "Time",
            Self::DateAndTime => "Date and Time",
            Self::Iso8601 => "ISO 8601",
            Self::FilenameDate => "Filename Date",
        }
    }

    /// Every variant, in menu order.
    ///
    /// Date, Time and Date and Time first, because they are what most users
    /// mean by "insert the date"; the two machine-oriented formats last, as
    /// the specialised choices they are.
    pub fn all() -> &'static [Stamp] {
        &[
            Self::Date,
            Self::Time,
            Self::DateAndTime,
            Self::Iso8601,
            Self::FilenameDate,
        ]
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;
    use crate::parse_date;

    #[test]
    fn the_date_stamp_spells_the_month_and_reads_day_first() {
        assert_eq!(
            render(Stamp::Date, datetime!(2026-08-16 20:05 UTC)),
            "16 August 2026"
        );
    }

    #[test]
    fn the_time_stamp_agrees_with_the_status_bar_clock() {
        // Same fixtures as bp-ui's `clock_uses_twelve_hour_time`, because
        // agreement with that function is the entire point of this variant.
        assert_eq!(
            render(Stamp::Time, datetime!(2026-08-16 20:05 UTC)),
            "8:05 PM",
            "must match bp-ui's clock() exactly"
        );
        assert_eq!(
            render(Stamp::Time, datetime!(2026-08-16 08:05 UTC)),
            "8:05 AM"
        );
    }

    #[test]
    fn midnight_is_twelve_am_not_zero_am() {
        // 12-hour clocks are the classic place an hour-mod-12 shortcut gets
        // this wrong and prints "0:30 AM".
        assert_eq!(
            render(Stamp::Time, datetime!(2026-08-16 00:30 UTC)),
            "12:30 AM"
        );
    }

    #[test]
    fn noon_is_twelve_pm_not_zero_pm() {
        assert_eq!(
            render(Stamp::Time, datetime!(2026-08-16 12:00 UTC)),
            "12:00 PM"
        );
    }

    #[test]
    fn date_and_time_joins_both_renderings_with_a_comma() {
        assert_eq!(
            render(Stamp::DateAndTime, datetime!(2026-08-16 20:05 UTC)),
            "16 August 2026, 8:05 PM"
        );
    }

    #[test]
    fn iso8601_uses_z_for_utc() {
        assert_eq!(
            render(Stamp::Iso8601, datetime!(2026-08-16 20:05:00 UTC)),
            "2026-08-16T20:05:00Z"
        );
    }

    #[test]
    fn iso8601_keeps_a_non_utc_offset_explicit() {
        assert_eq!(
            render(Stamp::Iso8601, datetime!(2026-08-16 20:05:00 +05:30)),
            "2026-08-16T20:05:00+05:30",
            "an offset must survive rendering, or the timestamp silently changes meaning"
        );
        assert_eq!(
            render(Stamp::Iso8601, datetime!(2026-08-16 20:05:00 -08:00)),
            "2026-08-16T20:05:00-08:00"
        );
    }

    #[test]
    fn filename_date_matches_the_filename_grammar_exactly() {
        assert_eq!(
            render(Stamp::FilenameDate, datetime!(2026-08-16 20:05 UTC)),
            "16AUG2026",
            "must be exactly what date.rs writes into a filename"
        );
    }

    #[test]
    fn filename_date_round_trips_through_the_filename_parser() {
        let at = datetime!(2026-01-05 00:00 UTC);
        let rendered = render(Stamp::FilenameDate, at);
        assert_eq!(
            parse_date(&rendered),
            Some(at.date()),
            "a stamp inserted into a document must be readable by the same \
             grammar that reads it out of a filename"
        );
    }

    #[test]
    fn a_single_digit_day_and_month_are_padded_where_the_format_needs_it() {
        // 5 January: the case where a naive `{}` format prints "5:7:3" or
        // "5JAN2026" instead of the fixed-width forms these variants need.
        let at = datetime!(2026-01-05 09:07:03 UTC);
        assert_eq!(render(Stamp::Date, at), "5 January 2026");
        assert_eq!(render(Stamp::Time, at), "9:07 AM");
        assert_eq!(render(Stamp::Iso8601, at), "2026-01-05T09:07:03Z");
        assert_eq!(render(Stamp::FilenameDate, at), "05JAN2026");
    }

    #[test]
    fn every_variant_has_a_non_empty_label() {
        for stamp in Stamp::all() {
            assert!(!stamp.label().is_empty(), "{stamp:?} has no label");
        }
        assert_eq!(
            Stamp::all().len(),
            5,
            "a variant was added or removed without updating `all`"
        );
    }
}
