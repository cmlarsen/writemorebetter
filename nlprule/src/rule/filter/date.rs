//! Calendar helpers for the LanguageTool date filters.
//!
//! These mirror the small parts of `java.util.Calendar` (with the UK locale) and
//! `org.languagetool.tools.StringTools` that the English date filters use. They
//! are pure computation: no data beyond the Gregorian calendar is needed.
//!
//! LanguageTool's own tests run with a fixed "current date" (January 1st, 2014,
//! see its `TestHackHelper`). Rules whose examples depend on "today" are only
//! meaningful under the same convention, so the rule test harness passes an
//! explicit date here; production uses the real system clock.

use serde::{Deserialize, Serialize};

/// A date in the Gregorian calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

pub(crate) fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Whether the date exists in the Gregorian calendar (mirrors a strict,
/// non-lenient `Calendar.set`).
pub(crate) fn is_valid(year: i32, month: u32, day: u32) -> bool {
    (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)
}

impl Date {
    pub(crate) fn new(year: i32, month: u32, day: u32) -> Option<Date> {
        if is_valid(year, month, day) {
            Some(Date { year, month, day })
        } else {
            None
        }
    }

    /// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
    fn days_from_epoch(self) -> i64 {
        let y = if self.month <= 2 {
            self.year as i64 - 1
        } else {
            self.year as i64
        };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = if self.month > 2 {
            self.month as i64 - 3
        } else {
            self.month as i64 + 9
        };
        let doy = (153 * mp + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    /// The day of the week as `java.util.Calendar` numbers it (Sunday = 1 …
    /// Saturday = 7).
    pub(crate) fn weekday_java(self) -> u32 {
        let idx = self.days_from_epoch().rem_euclid(7);
        // 1970-01-01 is 0 here and was a Thursday (Calendar.THURSDAY = 5).
        ((idx + 4) % 7 + 1) as u32
    }

    pub(crate) fn is_after(self, other: Date) -> bool {
        self.days_from_epoch() > other.days_from_epoch()
    }
}

/// The English long name of a weekday, like `Calendar.getDisplayName(…, LONG,
/// Locale.UK)`.
pub(crate) fn weekday_name_java(day: u32) -> Option<&'static str> {
    Some(match day {
        1 => "Sunday",
        2 => "Monday",
        3 => "Tuesday",
        4 => "Wednesday",
        5 => "Thursday",
        6 => "Friday",
        7 => "Saturday",
        _ => return None,
    })
}

/// `org.languagetool.tools.StringTools.trimSpecialCharacters`: keeps only
/// whitespace, alphanumeric and punctuation characters.
pub(crate) fn trim_special_characters(s: &str) -> String {
    use unicode_categories::UnicodeCategories;
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || c.is_punctuation())
        .collect()
}

/// `StringTools.isNumeric`: `^[\d\s.,]*\d$`.
pub(crate) fn is_numeric(s: &str) -> bool {
    if !s
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_whitespace() || c == '.' || c == ',')
    {
        return false;
    }
    matches!(s.chars().last(), Some(c) if c.is_ascii_digit())
}

/// `StringTools.isCapitalizedWord`.
pub(crate) fn is_capitalized_word(s: &str) -> bool {
    match s.chars().next() {
        Some(first) if first.is_uppercase() => {
            let rest: String = s.chars().skip(1).collect();
            rest == rest.to_lowercase()
        }
        _ => false,
    }
}

/// `StringTools.isAllUppercase`.
pub(crate) fn is_all_uppercase(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic()) && !s.chars().any(|c| c.is_lowercase())
}

/// `StringTools.uppercaseFirstChar`.
fn uppercase_first_char(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        if i == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `StringTools.preserveCase`.
pub(crate) fn preserve_case(input: &str, model: &str) -> String {
    if model.is_empty() {
        return input.to_string();
    }
    if is_capitalized_word(model) {
        return uppercase_first_char(&input.to_lowercase());
    }
    if is_all_uppercase(model) {
        return input.to_uppercase();
    }
    input.to_string()
}

/// `StringTools.isWhitespace`.
pub(crate) fn is_whitespace(s: &str) -> bool {
    if s == "\u{0001}" || s == "\u{0002}" {
        return false;
    }
    if s == "\u{FEFF}" {
        return true;
    }
    s.chars().next().map_or(false, |c| c.is_whitespace())
}

/// A number as an English month (1 = January), from a name or abbreviation.
pub(crate) fn month_from_name(month: &str) -> Option<u32> {
    let mon = trim_special_characters(month).to_lowercase();
    for (prefix, number) in [
        ("jan", 1),
        ("feb", 2),
        ("mar", 3),
        ("apr", 4),
        ("may", 5),
        ("jun", 6),
        ("jul", 7),
        ("aug", 8),
        ("sep", 9),
        ("oct", 10),
        ("nov", 11),
        ("dec", 12),
    ] {
        if mon.starts_with(prefix) {
            return Some(number);
        }
    }
    None
}

/// The `java.util.Calendar` day-of-week number for an English weekday name or
/// abbreviation.
pub(crate) fn day_of_week_from_name(day: &str) -> Option<u32> {
    let day = trim_special_characters(day).to_lowercase();
    for (prefix, number) in [
        ("su", 1),
        ("mo", 2),
        ("tu", 3),
        ("we", 4),
        ("th", 5),
        ("fr", 6),
        ("sa", 7),
    ] {
        if day.starts_with(prefix) {
            return Some(number);
        }
    }
    None
}

/// Today according to the system clock, or a fixed fallback where no clock is
/// available (wasm). Mirrors `Calendar.getInstance(Locale.UK)` closely enough
/// for a "is this date in the future / is it January" check.
pub(crate) fn today() -> Date {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        if let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) {
            return civil_from_days((duration.as_secs() / 86_400) as i64);
        }
    }
    Date {
        year: 2014,
        month: 1,
        day: 1,
    }
}

/// Inverse of `Date::days_from_epoch` (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> Date {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    Date {
        year: (y + i64::from(m <= 2)) as i32,
        month: m,
        day: d,
    }
}

/// Extracts the leading digits of a day-of-month string (`"(\\d+).*"`).
pub(crate) fn leading_number(s: &str) -> Option<i64> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekday_matches_java() {
        // java.util.Calendar: Sunday = 1 … Saturday = 7.
        assert_eq!(Date::new(2014, 10, 7).unwrap().weekday_java(), 3); // Tuesday
        assert_eq!(Date::new(2014, 10, 6).unwrap().weekday_java(), 2); // Monday
        assert_eq!(Date::new(1970, 1, 1).unwrap().weekday_java(), 5); // Thursday
        assert_eq!(Date::new(2000, 1, 1).unwrap().weekday_java(), 7); // Saturday
    }

    #[test]
    fn round_trips_days() {
        for date in [
            Date {
                year: 2014,
                month: 10,
                day: 7,
            },
            Date {
                year: 2000,
                month: 2,
                day: 29,
            },
            Date {
                year: 1999,
                month: 12,
                day: 31,
            },
        ] {
            assert_eq!(civil_from_days(date.days_from_epoch()), date);
        }
    }

    #[test]
    fn preserves_case() {
        assert_eq!(preserve_case("tuesday", "Monday"), "Tuesday");
        assert_eq!(preserve_case("tuesday", "MONDAY"), "TUESDAY");
        assert_eq!(preserve_case("tuesday", "monday"), "tuesday");
    }
}
