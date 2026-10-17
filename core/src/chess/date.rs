use std::fmt;

/// A calendar day, e.g. `2024.03.15` in [PGN](crate::Pgn). Its fields aren't checked, so it can
/// name a day that doesn't exist. Fields are declared largest first, so the derived order is
/// chronological.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

/// Written as [PGN](crate::Pgn) writes a full date, `YYYY.MM.DD`.
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}.{:02}.{:02}", self.year, self.month, self.day)
    }
}

/// Reading dates back is covered by the [`Pgn::date`](crate::Pgn::date) properties.
#[cfg(test)]
mod tests {
    use super::Date;

    #[track_caller]
    fn check(year: u16, month: u8, day: u8, expected: &str) {
        let date = Date { year, month, day };
        assert_eq!(date.to_string(), expected, "{date:?}");
    }

    #[test]
    fn writes_pgn_notation() {
        check(2024, 3, 15, "2024.03.15");
        check(1851, 11, 21, "1851.11.21");
        check(9999, 12, 31, "9999.12.31");
    }

    #[test]
    fn pads_every_field() {
        check(2024, 1, 5, "2024.01.05");
        check(999, 1, 1, "0999.01.01");
        check(1, 1, 1, "0001.01.01");
        check(0, 1, 1, "0000.01.01");
    }

    #[test]
    fn writes_days_that_do_not_exist() {
        check(2023, 2, 29, "2023.02.29");
    }
}
