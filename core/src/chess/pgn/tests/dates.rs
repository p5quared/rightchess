//! The contract of [`Pgn::date`]: the day a game started, from its `[UTCDate]` [tag](Pgn), else
//! its `[Date]` tag.

use crate::chess::{Date, Pgn};

fn pgn(tags: &[(&str, &str)]) -> Pgn {
    let text = tags
        .iter()
        .map(|(name, value)| format!("[{name} \"{value}\"]"))
        .collect::<Vec<_>>()
        .join("\n");
    text.parse().unwrap_or_else(|err| panic!("{text:?}: {err}"))
}

fn date(year: u16, month: u8, day: u8) -> Date {
    Date { year, month, day }
}

/// The game's only date tag is `[Date "value"]`.
#[track_caller]
fn check(value: &str, expected: Option<Date>) {
    assert_eq!(pgn(&[("Date", value)]).date(), expected, "{value:?}");
}

mod spec {
    use super::{check, date, pgn};

    #[test]
    fn full_dates() {
        check("2024.03.15", Some(date(2024, 3, 15)));
        check("0001.01.01", Some(date(1, 1, 1)));
        check("1851.06.21", Some(date(1851, 6, 21)));
        check("9999.12.31", Some(date(9999, 12, 31)));
    }

    #[test]
    fn the_day_must_exist_in_its_month() {
        check("2024.04.30", Some(date(2024, 4, 30)));
        check("2024.04.31", None);
        check("2024.01.31", Some(date(2024, 1, 31)));
        check("2024.01.32", None);
        check("2024.01.00", None);
        check("2024.00.10", None);
        check("2024.13.01", None);
    }

    #[test]
    fn february_has_a_29th_only_in_leap_years() {
        check("2024.02.29", Some(date(2024, 2, 29)));
        check("2023.02.29", None);
        check("2023.02.28", Some(date(2023, 2, 28)));
        check("1900.02.29", None);
        check("2000.02.29", Some(date(2000, 2, 29)));
    }

    #[test]
    fn unknown_parts_are_none() {
        for value in ["????.??.??", "2024.??.??", "2024.03.??", "????.03.15"] {
            check(value, None);
        }
    }

    #[test]
    fn only_yyyy_mm_dd_is_a_date() {
        for value in [
            "",
            "2024-03-15",
            "2024/03/15",
            "2024.3.15",
            "2024.03.5",
            "24.03.15",
            "02024.03.15",
            " 2024.03.15",
            "2024.03.15 ",
            "2024.03.15.01",
            "2024.03",
            "2024",
            "+024.03.15",
            "2024.+3.15",
            "2024.03.+5",
            "２０２４.03.15",
        ] {
            check(value, None);
        }
    }

    #[test]
    fn utc_date_wins_over_date() {
        let game = pgn(&[("Date", "2024.03.15"), ("UTCDate", "2024.03.16")]);
        assert_eq!(game.date(), Some(date(2024, 3, 16)));
    }

    #[test]
    fn date_stands_in_for_a_missing_or_partial_utc_date() {
        for utc in [
            None,
            Some("????.??.??"),
            Some("2024.??.??"),
            Some(""),
            Some("soon"),
        ] {
            let mut tags = vec![("Date", "2024.03.15")];
            tags.extend(utc.map(|value| ("UTCDate", value)));
            assert_eq!(
                pgn(&tags).date(),
                Some(date(2024, 3, 15)),
                "UTCDate {utc:?}"
            );
        }
    }

    #[test]
    fn utc_date_alone_is_enough() {
        assert_eq!(
            pgn(&[("UTCDate", "2024.03.16")]).date(),
            Some(date(2024, 3, 16))
        );
    }

    #[test]
    fn no_full_date_is_none() {
        assert_eq!(pgn(&[]).date(), None);
        assert_eq!("1. e4 e5 *".parse::<super::Pgn>().unwrap().date(), None);
        assert_eq!(
            pgn(&[("Date", "????.??.??"), ("UTCDate", "????.??.??")]).date(),
            None
        );
        // Names are case-sensitive, as with every tag.
        assert_eq!(pgn(&[("date", "2024.03.15")]).date(), None);
    }
}

mod properties {
    use proptest::prelude::*;
    use proptest::test_runner::FileFailurePersistence;

    use super::{Date, date, pgn};

    /// Every real day from year 0 to 9999, in the proleptic Gregorian calendar.
    fn real_day() -> impl Strategy<Value = Date> {
        (0..=9999u16, 1..=12u8).prop_flat_map(|(year, month)| {
            let leap =
                year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
            let len = match month {
                2 if leap => 29u8,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            (1..=len).prop_map(move |day| date(year, month, day))
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
            ..ProptestConfig::default()
        })]

        #[test]
        fn every_real_day_reads_back(day in real_day()) {
            let text = day.to_string();
            prop_assert_eq!(pgn(&[("Date", &text)]).date(), Some(day), "{}", text);
            prop_assert_eq!(pgn(&[("UTCDate", &text)]).date(), Some(day), "{}", text);
        }

        /// Nothing but the exact text of a date parses, so no two values give the same date.
        #[test]
        fn a_date_is_written_exactly_as_its_value(value in r"[0-9.?+ -]{0,12}|[0-9]{4}\.[0-9]{2}\.[0-9]{2}") {
            if let Some(day) = pgn(&[("Date", &value)]).date() {
                prop_assert_eq!(day.to_string(), value);
            }
        }

        #[test]
        fn dates_order_as_their_text(a in real_day(), b in real_day()) {
            prop_assert_eq!(a.cmp(&b), a.to_string().cmp(&b.to_string()));
        }
    }
}
