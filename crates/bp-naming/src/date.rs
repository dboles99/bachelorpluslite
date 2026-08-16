//! The `DDMMMYYYY` date component of the filename grammar.

use time::{Date, Month};

/// Uppercase English month abbreviations, as required by specs.md section 2.
const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

/// Width of a formatted date: two digits, three letters, four digits.
pub const DATE_LEN: usize = 9;

fn month_index(month: Month) -> usize {
    usize::from(u8::from(month)) - 1
}

/// Format a date as `16AUG2026`.
pub fn format_date(date: Date) -> String {
    format!(
        "{:02}{}{:04}",
        date.day(),
        MONTHS[month_index(date.month())],
        date.year()
    )
}

/// Parse a `16AUG2026` date component.
///
/// Month matching is case-insensitive -- we always *emit* uppercase, but a
/// user who typed `16aug2026` by hand still has a conforming filename and
/// should not have it treated as unstructured.
///
/// Returns `None` for calendar-invalid dates such as `31FEB2026`, so callers
/// never receive a date that could not have been a real creation date.
pub fn parse_date(s: &str) -> Option<Date> {
    if s.len() != DATE_LEN || !s.is_ascii() {
        return None;
    }
    let (day, rest) = s.split_at(2);
    let (month, year) = rest.split_at(3);

    if !day.bytes().all(|b| b.is_ascii_digit()) || !year.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let day: u8 = day.parse().ok()?;
    let year: i32 = year.parse().ok()?;

    let upper = month.to_ascii_uppercase();
    let index = MONTHS.iter().position(|m| *m == upper)?;
    let month = Month::try_from(u8::try_from(index).ok()? + 1).ok()?;

    Date::from_calendar_date(year, month, day).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn formats_the_spec_example() {
        assert_eq!(format_date(date!(2026 - 08 - 16)), "16AUG2026");
    }

    #[test]
    fn pads_single_digit_days() {
        assert_eq!(format_date(date!(2026 - 01 - 05)), "05JAN2026");
    }

    #[test]
    fn round_trips() {
        let d = date!(2026 - 12 - 31);
        assert_eq!(parse_date(&format_date(d)), Some(d));
    }

    #[test]
    fn accepts_lowercase_months() {
        assert_eq!(parse_date("16aug2026"), Some(date!(2026 - 08 - 16)));
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(parse_date(""), None);
        assert_eq!(parse_date("16AUG202"), None, "too short");
        assert_eq!(parse_date("16AUG20266"), None, "too long");
        assert_eq!(parse_date("1AUG20266"), None, "day not two digits");
        assert_eq!(parse_date("16XXX2026"), None, "not a month");
        assert_eq!(parse_date("AUG162026"), None, "wrong component order");
    }

    #[test]
    fn rejects_impossible_calendar_dates() {
        assert_eq!(parse_date("31FEB2026"), None);
        assert_eq!(parse_date("00AUG2026"), None);
        assert_eq!(parse_date("32AUG2026"), None);
    }

    #[test]
    fn accepts_leap_day_only_in_leap_years() {
        assert_eq!(parse_date("29FEB2024"), Some(date!(2024 - 02 - 29)));
        assert_eq!(parse_date("29FEB2026"), None);
    }

    #[test]
    fn rejects_non_ascii_of_the_right_byte_length() {
        // "é" is two bytes, so a naive `len() == 9` check could let a
        // multi-byte string through and then panic on `split_at`.
        assert_eq!(parse_date("16AUGé026"), None);
    }
}
