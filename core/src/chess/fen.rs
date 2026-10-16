use std::fmt;
use std::str::FromStr;

use anyhow::{Result, anyhow, bail, ensure};

use crate::chess::Color;

const STARTING: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// One position in FEN as written, e.g. `rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq
/// e3 0 1`. Only well-formed text is a `Fen`: six fields, eight ranks of eight squares, castling
/// rights in `KQkq` order, an en passant square behind the side that just moved, and counters
/// that fit a `u32`. Whether the position could arise isn't checked.
///
/// FEN (Forsyth–Edwards Notation) writes a position as one line of six fields: the pieces rank
/// by rank (row by row, from Black's side), uppercase for White and digits for runs of empty
/// squares; the side to move, `w` or `b`; castling rights, which castlings are still allowed,
/// `K`/`Q` for White's king or queen side and `k`/`q` for Black's; the en passant square, which a
/// pawn just skipped by moving two squares and where an enemy pawn beside it may capture it on
/// the next move only; and two counters, the halfmove clock (plies since the last capture or
/// pawn move) and the fullmove number (which goes up after each Black move).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fen(pub(crate) String);

impl Fen {
    #[must_use]
    #[expect(clippy::missing_panics_doc, reason = "STARTING is a valid FEN")]
    pub fn starting() -> Fen {
        str::parse(STARTING).expect("STARTING FEN should parse")
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn side_to_move(&self) -> Color {
        match self.0.split(' ').nth(1) {
            Some("w") => Color::White,
            Some("b") => Color::Black,
            _ => unreachable!("a Fen has a validated side-to-move field"),
        }
    }
}

impl FromStr for Fen {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        check(text).map_err(|reason| anyhow!("\"{text}\" is not a FEN: {reason}"))?;
        Ok(Fen(text.to_string()))
    }
}

impl fmt::Display for Fen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Fails with the reason `text` isn't a [FEN](Fen).
fn check(text: &str) -> Result<()> {
    let fields: Vec<&str> = text.split(' ').collect();
    let [board, side, castling, en_passant, halfmove, fullmove] = fields[..] else {
        bail!("it needs 6 fields separated by single spaces");
    };
    let ranks: Vec<&str> = board.split('/').collect();
    ensure!(
        ranks.len() == 8,
        "the board has {} ranks, not 8",
        ranks.len()
    );
    for rank in ranks {
        ensure!(rank_len(rank) == Some(8), "rank {rank:?} isn't 8 squares");
    }
    let en_passant_rank = match side {
        "w" => '6',
        "b" => '3',
        _ => bail!("the side to move is {side:?}, not w or b"),
    };
    ensure!(
        is_castling(castling),
        "castling rights {castling:?} aren't - or KQkq in order"
    );
    ensure!(
        en_passant == "-" || is_square_on(en_passant, en_passant_rank),
        "the en passant square {en_passant:?} isn't - or on rank {en_passant_rank}"
    );
    counter(halfmove)?;
    ensure!(counter(fullmove)? > 0, "the fullmove number is 0");
    Ok(())
}

/// The squares a rank of the board spans, or None if it isn't pieces and digits for runs of
/// empty squares, never two digits in a row. A rank is a row of the board, e.g. `4P3` is four
/// empty squares, a white pawn, then three empty squares.
fn rank_len(rank: &str) -> Option<u32> {
    let mut len = 0;
    let mut after_digit = false;
    for c in rank.chars() {
        match c {
            '1'..='8' if !after_digit => len += c.to_digit(10)?,
            'p' | 'n' | 'b' | 'r' | 'q' | 'k' | 'P' | 'N' | 'B' | 'R' | 'Q' | 'K' => len += 1,
            _ => return None,
        }
        after_digit = c.is_ascii_digit();
    }
    Some(len)
}

/// Whether `rights` is `-`, or some of `KQkq` in that order. These are castling rights, which
/// castlings are still allowed: `K`/`Q` for White's king or queen side, `k`/`q` for Black's, `-`
/// for none.
fn is_castling(rights: &str) -> bool {
    let mut rest = "KQkq";
    let in_order = rights.chars().all(|c| match rest.find(c) {
        Some(i) => {
            rest = &rest[i + 1..];
            true
        }
        None => false,
    });
    rights == "-" || (!rights.is_empty() && in_order)
}

fn is_square_on(square: &str, rank: char) -> bool {
    matches!(square.as_bytes(), [b'a'..=b'h', r] if *r as char == rank)
}

/// A counter: digits without leading zeros that fit a `u32`. A [FEN](Fen) ends with two
/// counters, the halfmove clock (plies since the last capture or pawn move) and the fullmove
/// number.
fn counter(text: &str) -> Result<u32> {
    let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    ensure!(
        digits && (text == "0" || !text.starts_with('0')),
        "counter {text:?} isn't a number"
    );
    text.parse()
        .map_err(|_| anyhow!("counter {text} doesn't fit a u32"))
}

/// The contract of [`Fen`]: which text is a position in [FEN](Fen). Its structure is checked:
/// six fields split by single spaces, eight [ranks](Fen) of eight squares,
/// [castling rights](Fen) in `KQkq` order, an [en passant square](Fen) behind the side that just
/// moved, and [counters](Fen) that fit a `u32`. Whether the position could arise isn't; playing
/// from it checks that.
#[cfg(test)]
pub(crate) mod tests {
    use crate::chess::Fen;

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[track_caller]
    fn check(fen: &str) {
        let parsed: Fen = fen.parse().unwrap_or_else(|err| panic!("{fen:?}: {err}"));
        assert_eq!(parsed.as_str(), fen);
        assert_eq!(parsed.to_string(), fen);
    }

    #[track_caller]
    fn check_err(text: &str) {
        let err = text.parse::<Fen>().expect_err(text);
        assert!(err.to_string().contains(text), "{text:?}: {err}");
    }

    mod spec {
        use super::{START, check, check_err};
        use crate::chess::{Color, Fen};

        #[test]
        fn starting_position() {
            check(START);
            assert_eq!(Fen::starting().as_str(), START);
        }

        #[test]
        fn side_to_move_comes_from_the_position() {
            assert_eq!(Fen::starting().side_to_move(), Color::White);
            for (side, expected) in [("w", Color::White), ("b", Color::Black)] {
                for counters in ["0 1", "17 40"] {
                    let fen: Fen = format!("4k3/8/8/8/8/8/8/4K3 {side} - - {counters}")
                        .parse()
                        .expect("valid FEN");
                    assert_eq!(fen.side_to_move(), expected, "{fen}");
                }
            }
        }

        #[test]
        fn positions_from_games() {
            for fen in [
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
                "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
                "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/3P1N2/PPP2PPP/RNBQK2R w KQkq - 1 5",
                "3q3k/p3Q1bp/1p1p2p1/5r2/2rP1P2/2P5/P2B2PP/1R2R1K1 b - - 0 23",
                "8/8/8/8/8/8/8/K6k w - - 99 150",
            ] {
                check(fen);
            }
        }

        #[test]
        fn en_passant_squares() {
            for file in 'a'..='h' {
                check(&format!("4k3/8/8/8/8/8/8/4K3 b - {file}3 0 1"));
                check(&format!("4k3/8/8/8/8/8/8/4K3 w - {file}6 0 1"));
            }
        }

        #[test]
        fn castling_rights() {
            for rights in [
                "-", "K", "Q", "k", "q", "KQ", "Kk", "Kq", "Qk", "Qq", "kq", "KQk", "KQq", "Kkq",
                "Qkq", "KQkq",
            ] {
                check(&format!("r3k2r/8/8/8/8/8/8/R3K2R w {rights} - 0 1"));
            }
        }

        #[test]
        fn counters() {
            for counters in ["0 1", "1 1", "50 100", "4294967295 4294967295"] {
                check(&format!("4k3/8/8/8/8/8/8/4K3 w - - {counters}"));
            }
        }

        #[test]
        fn positions_that_could_not_arise() {
            for fen in [
                "8/8/8/8/8/8/8/8 w - - 0 1",
                "QQQQQQQQ/QQQQQQQQ/8/8/8/8/kkkkkkkk/KKKKKKKK b - - 0 1",
                "P3k3/8/8/8/8/8/8/p3K3 w - - 0 1",
                "4k3/8/8/8/8/8/8/4K3 w KQkq e6 0 1",
                "4k3/8/8/8/8/8/8/4K2R b - - 0 1",
            ] {
                check(fen);
            }
        }

        #[test]
        fn malformed_fields() {
            for text in [
                "",
                " ",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 0",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 ",
                " rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w  KQkq - 0 1",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR\tw KQkq - 0 1",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1\n",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w  - 0 1",
            ] {
                check_err(text);
            }
        }

        #[test]
        fn malformed_boards() {
            for board in [
                "",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP",
                "rnbqkbnr/pppppppp/8/8/8/8/8/PPPPPPPP/RNBQKBNR",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR/",
                "/rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR",
                "rnbqkbnr/pppppppp/8/8//8/PPPPPPPP/RNBQKBNR",
                "rnbqkbn/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR",
                "rnbqkbnrr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR",
                "rnbqkbnr/pppppppp/7/8/8/8/PPPPPPPP/RNBQKBNR",
                "rnbqkbnr/pppppppp/9/8/8/8/PPPPPPPP/RNBQKBNR",
                "rnbqkbnr/pppppppp/08/8/8/8/PPPPPPPP/RNBQKBNR",
                "rnbqkbnr/pppppppp/44/8/8/8/PPPPPPPP/RNBQKBNR",
                "rnbqkbnr/pppppppp/71/8/8/8/PPPPPPPP/RNBQKBNR",
                "rnbqkbnr/pppppppp/4P21/8/8/8/PPPP1PPP/RNBQKBNR",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNX",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQ.BNR",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQ KBN",
            ] {
                check_err(&format!("{board} w KQkq - 0 1"));
            }
        }

        #[test]
        fn malformed_sides_to_move() {
            for side in ["W", "B", "-", "wb", "white"] {
                check_err(&format!("4k3/8/8/8/8/8/8/4K3 {side} - - 0 1"));
            }
        }

        #[test]
        fn malformed_castling_rights() {
            for rights in [
                "qk", "kK", "QK", "KK", "KQkqK", "kqKQ", "X", "KQ-", "-K", "--", "A", "Hh", "kQ",
            ] {
                check_err(&format!("r3k2r/8/8/8/8/8/8/R3K2R w {rights} - 0 1"));
            }
        }

        #[test]
        fn malformed_en_passant_squares() {
            for (side, square) in [
                ("w", "e3"),
                ("b", "e6"),
                ("w", "e4"),
                ("b", "e5"),
                ("w", "e1"),
                ("b", "e8"),
                ("w", "i6"),
                ("b", "E3"),
                ("w", "e"),
                ("w", "6"),
                ("w", "e66"),
                ("w", "--"),
            ] {
                check_err(&format!("4k3/8/8/8/8/8/8/4K3 {side} - {square} 0 1"));
            }
        }

        #[test]
        fn malformed_counters() {
            for counters in [
                "-1 1",
                "+1 1",
                "01 1",
                "00 1",
                "1.0 1",
                "x 1",
                "0 0",
                "0 -1",
                "0 01",
                "0 x",
                "4294967296 1",
                "0 4294967296",
            ] {
                check_err(&format!("4k3/8/8/8/8/8/8/4K3 w - - {counters}"));
            }
        }
    }

    pub(crate) mod properties {
        use proptest::prelude::*;
        use proptest::sample::Index;
        use proptest::test_runner::FileFailurePersistence;
        use regex::Regex;

        use super::*;

        const PIECE: &str = "[pnbrqkPNBRQK]";
        /// How many digits may follow a counter's first one when generating: few enough to fit a
        /// `u32`. The counters are a [FEN](Fen)'s last two fields, the halfmove clock and the
        /// fullmove number.
        const MORE_DIGITS: &str = "{0,8}";

        /// A rank of `n` squares that doesn't start with a digit if `after_digit`: digits for runs of
        /// empty squares, never two in a row. A rank is a row of the board, e.g. `4P3`.
        fn rank(n: u32, after_digit: bool) -> String {
            if n == 0 {
                return String::new();
            }
            let mut ways = vec![format!("{PIECE}{}", rank(n - 1, false))];
            if !after_digit {
                ways.extend((1..=n).map(|d| format!("{d}{}", rank(n - d, true))));
            }
            format!("({})", ways.join("|"))
        }

        /// [FEN](Fen) as a regex, with `more_digits` after each counter's first digit: the oracle
        /// the parser must agree with, apart from the `u32` bound. The fullmove number, the last
        /// field, isn't 0.
        fn fen(more_digits: &str) -> String {
            let rank = rank(8, false);
            let board = format!("{rank}(/{rank}){{7}}");
            let castling = "(-|KQ?k?q?|Qk?q?|kq?|q)";
            format!(
                "{board} (w {castling} (-|[a-h]6)|b {castling} (-|[a-h]3)) (0|[1-9][0-9]{more_digits}) [1-9][0-9]{more_digits}"
            )
        }

        /// Characters that [FEN](Fen) uses, so that near misses come up often.
        const SOUP: &str = r"[pnbrqkPNBRQKXx0-9/ wbWB\-a-iA-H.\t]";

        /// A [FEN](Fen) with one character added, removed or replaced.
        fn near_miss() -> impl Strategy<Value = String> {
            let edit = prop_oneof![
                (any::<Index>(), SOUP).prop_map(|(at, c)| (at, 0, c)),
                any::<Index>().prop_map(|at| (at, 1, String::new())),
                (any::<Index>(), SOUP).prop_map(|(at, c)| (at, 1, c)),
            ];
            (string_regex(&fen(MORE_DIGITS)), edit).prop_map(|(fen, (at, removed, added))| {
                let mut chars: Vec<char> = fen.chars().collect();
                let at = at.index(chars.len() + 1).min(chars.len());
                let end = (at + removed).min(chars.len());
                chars.splice(at..end, added.chars());
                chars.into_iter().collect()
            })
        }

        /// Whether `text` is a [FEN](Fen): it matches the regex, and its counters (the halfmove
        /// clock and fullmove number, the last two fields) fit a `u32`.
        fn is_fen(text: &str) -> bool {
            let oracle = Regex::new(&format!(r"\A(?:{})\z", fen("*"))).unwrap();
            oracle.is_match(text)
                && text
                    .rsplit(' ')
                    .take(2)
                    .all(|counter| counter.parse::<u32>().is_ok())
        }

        fn string_regex(regex: &str) -> impl Strategy<Value = String> + use<> {
            proptest::string::string_regex(regex).unwrap()
        }

        /// Any [FEN](Fen) whose counters (the halfmove clock and fullmove number, the last two
        /// fields) fit a `u32`.
        pub(crate) fn fens() -> impl Strategy<Value = Fen> {
            string_regex(&fen(MORE_DIGITS)).prop_map(|text| text.parse().unwrap())
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
                ..ProptestConfig::default()
            })]

            #[test]
            fn fen_parses_as_written(text in string_regex(&fen(MORE_DIGITS))) {
                let fen: Fen = text.parse().map_err(|err: anyhow::Error| TestCaseError::fail(err.to_string()))?;
                prop_assert_eq!(fen.as_str(), &text);
                prop_assert_eq!(fen.to_string(), text);
            }

            #[test]
            fn text_parses_exactly_when_it_is_fen(
                text in prop_oneof![near_miss(), string_regex(&format!("{SOUP}{{0,80}}")), any::<String>()]
            ) {
                prop_assert_eq!(text.parse::<Fen>().is_ok(), is_fen(&text), "{:?}", text);
            }
        }
    }
}
