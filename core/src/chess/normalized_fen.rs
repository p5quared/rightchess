use std::fmt;
use std::str::FromStr;

use anyhow::{Context, Result, ensure};

use crate::chess::Fen;

/// A position as a key, for finding it again however a game reached it, e.g.
/// `rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq -`. It's a [FEN](Fen)'s first four
/// fields: placement, side to move and [castling rights](Fen) as written, and the
/// [en passant square](Fen) only when a pawn of the side to move stands beside the pawn that just
/// moved two squares (Polyglot's rule; Polyglot is a common format for opening books, whose
/// position hashes the key matches). The move [counters](Fen) are dropped.
///
/// Caveat: that pawn may still be unable to capture, because it's pinned (moving it would expose
/// its own king to attack) or its king is in check and the capture doesn't answer it. FIDE's
/// repetition rule, which decides when the same position has occurred again for a draw, would
/// ignore such a square, but the key keeps it, so one position can have two keys. That costs a
/// repeated engine [eval](crate::Eval), never a wrong one: a key never stands for two different
/// positions. Checking that the capture is legal would need `Board` and could fail on positions
/// that can't be played.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NormalizedFen(String);

impl NormalizedFen {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&Fen> for NormalizedFen {
    fn from(fen: &Fen) -> Self {
        let fields: Vec<&str> = fen.as_str().split(' ').collect();
        let [placement, side, castling, en_passant, ..] = fields[..] else {
            unreachable!("{fen} has six fields");
        };
        let en_passant = if pawn_beside(placement, side, en_passant) {
            en_passant
        } else {
            "-"
        };
        NormalizedFen(format!("{placement} {side} {castling} {en_passant}"))
    }
}

/// The position as a [FEN](Fen), with move [counters](Fen) `0 1`: the key doesn't keep the real
/// ones.
impl From<&NormalizedFen> for Fen {
    fn from(key: &NormalizedFen) -> Self {
        Fen(format!("{key} 0 1"))
    }
}

/// Reads a key back from its text. Only text that `From<&Fen>` could have written is a key.
impl FromStr for NormalizedFen {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let fen: Fen = format!("{text} 0 1")
            .parse()
            .with_context(|| format!("\"{text}\" is not a normalized FEN"))?;
        let key = NormalizedFen::from(&fen);
        ensure!(
            key.0 == text,
            "\"{text}\" is not a normalized FEN: it normalizes to \"{key}\""
        );
        Ok(key)
    }
}

/// Whether a pawn of `side` stands beside the pawn that just passed the `en_passant` square, so
/// it might capture [en passant](Fen).
fn pawn_beside(placement: &str, side: &str, en_passant: &str) -> bool {
    let &[file, _] = en_passant.as_bytes() else {
        return false;
    };
    let file = i32::from(file - b'a');
    let (rank, pawn) = match side {
        "w" => (5, 'P'),
        _ => (4, 'p'),
    };
    [file - 1, file + 1]
        .into_iter()
        .any(|beside| piece_at(placement, beside, rank) == Some(pawn))
}

/// The piece on a square of a [FEN](Fen)'s placement, with files (columns) counted from 0 for a
/// and ranks (rows) from 1. Squares off the board are empty.
fn piece_at(placement: &str, file: i32, rank: usize) -> Option<char> {
    let row = placement.split('/').nth(8usize.checked_sub(rank)?)?;
    let mut at = 0;
    for c in row.chars() {
        match c.to_digit(10) {
            Some(empty) => at += empty as i32,
            None if at == file => return Some(c),
            None => at += 1,
        }
    }
    None
}

impl fmt::Display for NormalizedFen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The contract of [`NormalizedFen`]: positions a game can reach by different move orders get
/// the same key. The key is placement, side to move, [castling rights](Fen), and the
/// [en passant square](Fen) only when a pawn of the side to move stands beside the pawn that
/// just moved two squares. [Counters](Fen) are dropped.
#[cfg(test)]
mod tests {
    use crate::chess::{Fen, NormalizedFen};

    #[track_caller]
    fn normalize(fen: &str) -> NormalizedFen {
        let fen: Fen = fen.parse().unwrap_or_else(|err| panic!("{fen:?}: {err}"));
        NormalizedFen::from(&fen)
    }

    /// `fen` normalizes to `expected`, and the key reads back from its text.
    #[track_caller]
    fn check(fen: &str, expected: &str) {
        let key = normalize(fen);
        assert_eq!(key.as_str(), expected, "{fen}");
        assert_eq!(key.to_string(), expected, "{fen}");
        let read = expected.parse::<NormalizedFen>();
        let read = read.unwrap_or_else(|err| panic!("{expected:?}: {err}"));
        assert_eq!(read, key, "{fen}");
    }

    /// `fen` normalizes to its first four fields.
    #[track_caller]
    fn check_kept(fen: &str) {
        let fields: Vec<&str> = fen.split(' ').collect();
        check(fen, &fields[..4].join(" "));
    }

    /// `fen` normalizes to its first three fields and no en passant square.
    #[track_caller]
    fn check_dropped(fen: &str) {
        let fields: Vec<&str> = fen.split(' ').collect();
        check(fen, &format!("{} -", fields[..3].join(" ")));
    }

    /// `text` isn't a key, and the error names it.
    #[track_caller]
    fn check_err(text: &str) {
        let err = text.parse::<NormalizedFen>().expect_err(text);
        assert!(err.to_string().contains(text), "{text:?}: {err}");
    }

    mod spec {
        use super::{check, check_dropped, check_err, check_kept, normalize};
        use crate::chess::{Fen, NormalizedFen, Pgn};

        #[test]
        fn counters_are_dropped() {
            for (fen, expected) in [
                (
                    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
                ),
                (
                    "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/3P1N2/PPP2PPP/RNBQK2R w KQkq - 1 5",
                    "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/3P1N2/PPP2PPP/RNBQK2R w KQkq -",
                ),
                ("8/8/8/8/8/8/8/K6k w - - 99 150", "8/8/8/8/8/8/8/K6k w - -"),
                (
                    "4k3/8/8/8/8/8/8/4K3 b - - 4294967295 4294967295",
                    "4k3/8/8/8/8/8/8/4K3 b - -",
                ),
            ] {
                check(fen, expected);
            }
        }

        #[test]
        fn side_and_castling_rights_are_kept() {
            for rights in ["-", "K", "Qk", "KQkq"] {
                for side in ["w", "b"] {
                    check_kept(&format!("r3k2r/8/8/8/8/8/8/R3K2R {side} {rights} - 3 20"));
                }
            }
        }

        #[test]
        fn en_passant_is_kept_with_a_pawn_beside_the_pushed_one() {
            for fen in [
                "4k3/8/8/8/3pP3/8/8/4K3 b - e3 0 1",
                "4k3/8/8/8/4Pp2/8/8/4K3 b - e3 0 1",
                "4k3/8/8/8/3pPp2/8/8/4K3 b - e3 0 1",
                "4k3/8/8/3Pp3/8/8/8/4K3 w - e6 0 1",
                "4k3/8/8/4pP2/8/8/8/4K3 w - e6 0 1",
                "4k3/8/8/8/Pp6/8/8/4K3 b - a3 0 1",
                "4k3/8/8/8/6pP/8/8/4K3 b - h3 0 1",
                "4k3/8/8/pP6/8/8/8/4K3 w - a6 0 1",
                "4k3/8/8/6Pp/8/8/8/4K3 w - h6 0 1",
                "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3",
            ] {
                check_kept(fen);
            }
        }

        #[test]
        fn en_passant_is_dropped_without_a_pawn_beside_the_pushed_one() {
            for fen in [
                // Nothing beside it.
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
                "4k3/8/8/4p3/8/8/8/4K3 w - e6 0 1",
                // A pawn two files away.
                "4k3/8/8/8/2p1P3/8/8/4K3 b - e3 0 1",
                "4k3/8/8/4p1P1/8/8/8/4K3 w - e6 0 1",
                // A pawn of the side that just moved.
                "4k3/8/8/8/3PP3/8/8/4K3 b - e3 0 1",
                "4k3/8/8/3pp3/8/8/8/4K3 w - e6 0 1",
                // A pawn on a neighbouring file but the wrong rank.
                "4k3/8/8/8/4P3/3p4/8/4K3 b - e3 0 1",
                "4k3/8/8/3p4/4P3/8/8/4K3 b - e3 0 1",
                "4k3/8/3P4/4p3/8/8/8/4K3 w - e6 0 1",
                // A piece beside it that isn't a pawn.
                "4k3/8/8/8/3nP3/8/8/4K3 b - e3 0 1",
                "4k3/8/8/4pQ2/8/8/8/4K3 w - e6 0 1",
            ] {
                check_dropped(fen);
            }
        }

        /// The board's edges don't wrap: a pawn on the far side of the board, on this rank (row)
        /// or the ones above and below, isn't beside the pawn that was pushed two squares.
        #[test]
        fn edges_do_not_wrap() {
            for fen in [
                "4k3/8/8/8/P6p/8/8/4K3 b - a3 0 1",
                "4k3/8/8/7p/P7/8/8/4K3 b - a3 0 1",
                "4k3/8/8/8/P7/7p/8/4K3 b - a3 0 1",
                "4k3/8/8/8/p6P/8/8/4K3 b - h3 0 1",
                "4k3/8/8/p7/7P/8/8/4K3 b - h3 0 1",
                "4k3/8/8/8/7P/p7/8/4K3 b - h3 0 1",
                "4k3/8/8/p6P/8/8/8/4K3 w - a6 0 1",
                "4k3/8/8/P6p/8/8/8/4K3 w - h6 0 1",
            ] {
                check_dropped(fen);
            }
        }

        /// The caveat on [`NormalizedFen`]: the rule only looks for a pawn beside the pushed one,
        /// so the [en passant square](Fen) is kept even when that pawn can't legally capture.
        /// Here a pinned pawn (one that can't move without exposing its king) and a king in check
        /// each keep a square FIDE's repetition rule would ignore.
        #[test]
        fn en_passant_is_kept_even_when_the_capture_is_illegal() {
            for fen in [
                // dxe6 would leave the rook on h5 attacking the king on a5.
                "4k3/8/8/K2Pp2r/8/8/8/8 w - e6 0 1",
                // The bishop on e1 checks the king on a5, and dxe6 doesn't answer it.
                "4k3/8/8/K2Pp3/8/8/8/4b3 w - e6 0 1",
            ] {
                check_kept(fen);
            }
        }

        /// The last position of each game, normalized.
        fn last_keys(games: [&str; 2]) -> [String; 2] {
            games.map(|movetext| {
                let pgn: Pgn = movetext.parse().unwrap();
                let fen = pgn.fens().last().unwrap().unwrap();
                NormalizedFen::from(&fen).to_string()
            })
        }

        #[test]
        fn transpositions_get_the_same_key() {
            for (games, expected) in [
                // One side's double push sets en passant, the other's doesn't.
                (
                    ["1. e4 e5 2. Nf3", "1. Nf3 e5 2. e4"],
                    "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq -",
                ),
                // Back to the start, with other counters.
                (
                    ["1. Nf3 Nf6 2. Ng1 Ng8", ""],
                    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
                ),
                (
                    ["1. d4 Nf6 2. c4 e6", "1. c4 e6 2. d4 Nf6"],
                    "rnbqkb1r/pppp1ppp/4pn2/8/2PP4/8/PP2PPPP/RNBQKBNR w KQkq -",
                ),
            ] {
                assert_eq!(last_keys(games), [expected; 2], "{games:?}");
            }
        }

        /// Only one of these move orders leaves exd6, an [en passant](Fen) capture, possible, so
        /// the positions differ.
        #[test]
        fn a_possible_en_passant_capture_keeps_positions_apart() {
            let [possible, not] = last_keys(["1. e4 Nf6 2. e5 d5", "1. e4 d5 2. e5 Nf6"]);
            assert_eq!(
                possible,
                "rnbqkb1r/ppp1pppp/5n2/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6"
            );
            assert_eq!(
                not,
                "rnbqkb1r/ppp1pppp/5n2/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq -"
            );
        }

        #[test]
        fn as_a_fen_the_counters_are_0_and_1() {
            for key in [
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
                "4k3/8/8/8/3pP3/8/8/4K3 b - e3",
                "8/8/8/8/8/8/8/K6k w - -",
            ] {
                let fen = Fen::from(&normalize(&format!("{key} 0 1")));
                assert_eq!(fen.as_str(), format!("{key} 0 1"));
            }
            let fen = Fen::from(&normalize("8/8/8/8/8/8/8/K6k w - - 99 150"));
            assert_eq!(fen.as_str(), "8/8/8/8/8/8/8/K6k w - - 0 1");
        }

        #[test]
        fn text_that_is_not_a_key() {
            for text in [
                "",
                " ",
                "hello",
                // A whole FEN, counters included.
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                // Too few fields, or stray spaces.
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - ",
                " rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w  KQkq -",
                // Malformed fields.
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBN w KQkq -",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq -",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w kqKQ -",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq e3",
                // An en passant square normalizing would have dropped.
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3",
                "4k3/8/8/4p3/8/8/8/4K3 w - e6",
            ] {
                check_err(text);
            }
        }
    }

    mod properties {
        use proptest::prelude::*;
        use proptest::test_runner::FileFailurePersistence;

        use crate::chess::fen::tests::properties::fens;
        use crate::chess::{Fen, NormalizedFen};

        /// The squares of a FEN's placement, rank 8 first, `.` for empty: `grid[0][0]` is a8.
        fn grid(placement: &str) -> Vec<Vec<char>> {
            placement
                .split('/')
                .map(|rank| {
                    rank.chars()
                        .flat_map(|c| match c.to_digit(10) {
                            Some(empty) => vec!['.'; empty as usize],
                            None => vec![c],
                        })
                        .collect()
                })
                .collect()
        }

        /// Whether `fen` has an [en passant square](Fen) with a pawn of the side to move beside
        /// the pawn that just moved two squares.
        fn pawn_beside(fen: &Fen) -> bool {
            let fields: Vec<&str> = fen.as_str().split(' ').collect();
            let [placement, side, _, en_passant, ..] = fields[..] else {
                unreachable!("{fen} has six fields");
            };
            if en_passant == "-" {
                return false;
            }
            let file = (en_passant.as_bytes()[0] - b'a') as i32;
            // The pushed pawn is on rank 5 (row 3) or rank 4 (row 4).
            let (row, pawn) = match side {
                "w" => (3, 'P'),
                _ => (4, 'p'),
            };
            let grid = grid(placement);
            [file - 1, file + 1]
                .into_iter()
                .filter(|file| (0..8).contains(file))
                .any(|file| grid[row][file as usize] == pawn)
        }

        /// `fen` with other [counters](Fen).
        fn with_counters(fen: &Fen, halfmove: u32, fullmove: u32) -> Fen {
            let fields: Vec<&str> = fen.as_str().split(' ').collect();
            let text = format!("{} {halfmove} {fullmove}", fields[..4].join(" "));
            text.parse().unwrap()
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
                ..ProptestConfig::default()
            })]

            #[test]
            fn the_key_is_the_first_four_fields_minus_a_square_no_pawn_is_beside(fen in fens()) {
                let fields: Vec<&str> = fen.as_str().split(' ').collect();
                let en_passant = if pawn_beside(&fen) { fields[3] } else { "-" };
                let expected = format!("{} {en_passant}", fields[..3].join(" "));
                prop_assert_eq!(NormalizedFen::from(&fen).to_string(), expected);
            }

            #[test]
            fn counters_do_not_change_the_key(
                fen in fens(),
                halfmove in any::<u32>(),
                fullmove in 1..=u32::MAX,
            ) {
                let other = with_counters(&fen, halfmove, fullmove);
                prop_assert_eq!(NormalizedFen::from(&fen), NormalizedFen::from(&other));
            }

            #[test]
            fn normalizing_twice_changes_nothing(fen in fens()) {
                let key = NormalizedFen::from(&fen);
                prop_assert_eq!(NormalizedFen::from(&Fen::from(&key)), key);
            }

            #[test]
            fn keys_read_back_from_their_text(fen in fens()) {
                let key = NormalizedFen::from(&fen);
                let read = key.to_string().parse::<NormalizedFen>();
                let read = read.map_err(|err| TestCaseError::fail(err.to_string()))?;
                prop_assert_eq!(read, key);
            }

            /// Text reads as a [key](NormalizedFen) exactly when it's some [FEN](Fen)'s key.
            #[test]
            fn text_reads_as_a_key_exactly_when_it_is_one(fen in fens(), cut in 0..2usize) {
                let fields: Vec<&str> = fen.as_str().split(' ').collect();
                let text = fields[..4 + cut].join(" ");
                let is_key = cut == 0 && NormalizedFen::from(&fen).as_str() == text;
                prop_assert_eq!(text.parse::<NormalizedFen>().is_ok(), is_key, "{:?}", text);
            }
        }
    }
}
