//! Calendar months for the "since" slider. Steps count from [`FIRST`], the earliest month the
//! slider offers.

use chrono::Datelike;
use rightchess::YearMonth;

/// The oldest month the slider offers.
const FIRST: YearMonth = YearMonth {
    year: 2025,
    month: 1,
};

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

/// The current month according to the system clock.
#[must_use]
pub fn current() -> YearMonth {
    let now = chrono::Local::now();
    let year = u16::try_from(now.year()).unwrap_or(FIRST.year);
    let month = u8::try_from(now.month()).unwrap_or(FIRST.month);
    YearMonth { year, month }
}

/// The month's position from [`FIRST`], which is `0`.
#[must_use]
pub fn index(month: YearMonth) -> i32 {
    (i32::from(month.year) - i32::from(FIRST.year)) * 12
        + (i32::from(month.month) - i32::from(FIRST.month))
}

/// The month `step` months after [`FIRST`].
#[must_use]
pub fn at(step: i32) -> YearMonth {
    let total = i32::from(FIRST.year) * 12 + (i32::from(FIRST.month) - 1) + step;
    let year = u16::try_from(total.div_euclid(12)).unwrap_or(FIRST.year);
    let month = u8::try_from(total.rem_euclid(12) + 1).unwrap_or(FIRST.month);
    YearMonth { year, month }
}

/// A month as a person reads it, e.g. `July 2026`.
#[must_use]
pub fn label(month: YearMonth) -> String {
    let name = MONTHS
        .get(usize::from(month.month).saturating_sub(1))
        .copied()
        .unwrap_or("?");
    format!("{name} {}", month.year)
}
