use std::fmt;
use std::str::FromStr;

use anyhow::{Result, ensure};

/// One half-move in SAN as written, e.g. `Nf3+`. Only text that is SAN can be a `Ply`.
///
/// A half-move, or ply, is one side's move: `1. e4 e5` is two plies. SAN (Standard Algebraic
/// Notation) is the usual way to write a move: the piece's letter and the square it goes to,
/// e.g. `Nf3` for a knight to f3 and `exd5` for the pawn on the e-file taking on d5. Files are
/// the board's columns, a to h, and ranks its rows, 1 to 8 from White's side.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ply(String);

impl Ply {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Ply {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        ensure!(is_san(text.as_bytes()), "{text:?} is not a SAN move");
        Ok(Ply(text.to_string()))
    }
}

/// Whether `text` is a move, castling, drop or null move, with an optional `+` or `#` after all
/// but the null move. Castling is the king and rook moving together, written `O-O` or `O-O-O`.
/// A null move is a pass, written `--`. `+` marks a check (an attack on the king) and `#`
/// checkmate.
fn is_san(text: &[u8]) -> bool {
    let body = text
        .strip_suffix(b"+")
        .or(text.strip_suffix(b"#"))
        .unwrap_or(text);
    text == b"--" || is_castling(body) || is_drop(body) || is_piece_move(body) || is_pawn_move(body)
}

fn is_castling(body: &[u8]) -> bool {
    [&b"O-O"[..], b"O-O-O", b"0-0", b"0-0-0"].contains(&body)
}

/// A drop such as `N@f3`. Pawns can't be dropped on the first or last rank. A drop places a
/// captured piece back on the board, in variants such as Crazyhouse. Ranks are the board's rows,
/// 1 to 8 from White's side.
fn is_drop(body: &[u8]) -> bool {
    match *body {
        [b'P', b'@', file, rank] => is_file(file) && is_pawn_rank(rank, false),
        [b'N' | b'B' | b'R' | b'Q', b'@', file, rank] => is_square(file, rank),
        _ => false,
    }
}

/// A piece move such as `Nf3`, `Nbd7`, `R1e2` or `Qh4xe1`: the piece's letter, an optional file
/// (column a to h), rank (row 1 to 8) or square saying which of two such pieces moves, `x` for
/// a capture, then the target square.
fn is_piece_move(body: &[u8]) -> bool {
    let [b'K' | b'Q' | b'R' | b'B' | b'N', ref rest @ ..] = *body else {
        return false;
    };
    let Some((from, &[file, rank])) = rest.split_last_chunk() else {
        return false;
    };
    let from = from.strip_suffix(b"x").unwrap_or(from);
    let from_ok = match *from {
        [] => true,
        [c] => is_file(c) || is_rank(c),
        [file, rank] => is_square(file, rank),
        _ => false,
    };
    from_ok && is_square(file, rank)
}

/// A pawn move such as `e4`, `exd5` or `e8=Q`. A pawn captures onto a neighbouring file (column),
/// and promotes exactly when it reaches the first or last rank (row): it becomes the piece
/// after `=`, e.g. a queen in `e8=Q`.
fn is_pawn_move(body: &[u8]) -> bool {
    let (body, promotes) = match *body {
        [ref rest @ .., b'=', b'Q' | b'R' | b'B' | b'N' | b'K'] => (rest, true),
        _ => (body, false),
    };
    match *body {
        [file, rank] => is_file(file) && is_pawn_rank(rank, promotes),
        [from, b'x', file, rank] => {
            is_file(from)
                && is_file(file)
                && from.abs_diff(file) == 1
                && is_pawn_rank(rank, promotes)
        }
        _ => false,
    }
}

/// Whether a pawn may arrive on `rank`: the first or last rank if it promotes, any other if not.
/// Promoting is a pawn turning into another piece when it reaches the far row.
fn is_pawn_rank(rank: u8, promotes: bool) -> bool {
    is_rank(rank) && matches!(rank, b'1' | b'8') == promotes
}

fn is_square(file: u8, rank: u8) -> bool {
    is_file(file) && is_rank(rank)
}

fn is_file(c: u8) -> bool {
    matches!(c, b'a'..=b'h')
}

fn is_rank(c: u8) -> bool {
    matches!(c, b'1'..=b'8')
}

impl fmt::Display for Ply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The contract of [`Ply`]: which text is a [SAN](Ply) move. Legality in a position isn't
/// checked, but rules that hold in every position are: a pawn captures onto a neighbouring file,
/// promotes exactly when it reaches the first or last rank, and is never dropped there.
///
/// Promoting turns a pawn into another piece, and a drop places a captured piece back on the
/// board, as in Crazyhouse.
#[cfg(test)]
mod tests {
    use crate::chess::Ply;

    /// `san` parses, and the ply keeps it as written.
    #[track_caller]
    fn check(san: &str) {
        let ply: Ply = san.parse().unwrap_or_else(|err| panic!("{san:?}: {err}"));
        assert_eq!(ply.as_str(), san);
        assert_eq!(ply.to_string(), san);
    }

    /// `text` doesn't parse, and the error names it.
    #[track_caller]
    fn check_err(text: &str) {
        let err = text.parse::<Ply>().expect_err(text);
        assert!(err.to_string().contains(text), "{text:?}: {err}");
    }

    mod spec {
        use super::{check, check_err};

        #[test]
        fn pawn_moves() {
            for san in [
                "e4", "a2", "h7", "e3", "a6", "exd5", "axb6", "hxg3", "bxa7", "gxh2",
            ] {
                check(san);
            }
        }

        #[test]
        fn promotions() {
            for san in ["e8=Q", "a1=N", "h8=R", "c1=B", "dxe8=Q", "bxa1=R", "gxh8=N"] {
                check(san);
            }
            // Antichess promotes to a king.
            check("h8=K");
        }

        #[test]
        fn piece_moves() {
            for san in [
                "Nf3", "Bb5", "Rd1", "Qh4", "Ke2", "Kxe2", "Bxc6", "Qxf7", "Nxe5",
            ] {
                check(san);
            }
        }

        #[test]
        fn disambiguation() {
            for san in ["Nbd7", "R1e2", "Qh4e1", "Nbxd7", "R8xa2", "Qh4xe1", "Kbd2"] {
                check(san);
            }
        }

        #[test]
        fn castling() {
            for san in ["O-O", "O-O-O", "0-0", "0-0-0"] {
                check(san);
            }
        }

        #[test]
        fn drops() {
            for san in ["N@f3", "B@a1", "R@d4", "Q@h8", "P@e6", "P@a2", "P@h7"] {
                check(san);
            }
        }

        #[test]
        fn null_move() {
            check("--");
        }

        #[test]
        fn check_and_mate() {
            for san in [
                "e4+", "Qh4#", "exd8=Q+", "Nbxd7#", "O-O+", "O-O-O#", "0-0+", "N@f7+", "P@g7#",
            ] {
                check(san);
            }
        }

        #[test]
        fn malformed_squares() {
            for text in [
                "", "e", "4", "e9", "e0", "i4", "E4", "Nf", "N3", "Nf9", "Ni3", "e44",
            ] {
                check_err(text);
            }
        }

        #[test]
        fn malformed_pawn_moves() {
            for text in [
                "Pe4", "xe5", "ex5", "exe5", "exg5", "axh3", "hxa3", "e4xd5", "e2e4", "ed5", "e4x",
            ] {
                check_err(text);
            }
        }

        #[test]
        fn promotion_on_the_wrong_rank_or_to_the_wrong_piece() {
            for text in [
                "e8", "e1", "dxe8", "bxa1", "e5=Q", "e2=Q", "e8=P", "e8=", "e8Q", "e8=q", "e8==Q",
            ] {
                check_err(text);
            }
        }

        #[test]
        fn malformed_piece_moves() {
            for text in [
                "nf3", "Zf3", "Nxf", "Nfx3", "Nf3x", "Nxxf3", "xNf3", "N1bd2", "Nb1d2d3",
            ] {
                check_err(text);
            }
        }

        #[test]
        fn malformed_castling() {
            for text in [
                "O", "O-", "OO", "O-O-", "O-O-O-O", "O-0", "0-O", "o-o", "O_O", "O-O=Q",
            ] {
                check_err(text);
            }
        }

        #[test]
        fn malformed_drops() {
            for text in [
                "@e4", "N@", "K@e4", "N@e9", "N@xe4", "Nx@e4", "P@e1", "P@e8", "p@e4",
            ] {
                check_err(text);
            }
        }

        #[test]
        fn malformed_null_moves() {
            for text in ["-", "---", "--+", "--#"] {
                check_err(text);
            }
        }

        #[test]
        fn malformed_suffixes() {
            for text in [
                "+", "#", "e4++", "e4+#", "e4#+", "e4!", "e4?", "e4!?", "e4$1", "e4 ",
            ] {
                check_err(text);
            }
        }

        #[test]
        fn other_pgn_words_are_not_moves() {
            for text in [
                "1.", "1.e4", "1...", "1-0", "0-1", "1/2-1/2", "*", "$14", "{e4}", " e4",
            ] {
                check_err(text);
            }
        }
    }

    mod properties {
        use proptest::prelude::*;
        use proptest::sample::Index;
        use proptest::test_runner::FileFailurePersistence;
        use regex::Regex;

        use super::*;

        /// [SAN](Ply) as a regex: the oracle the parser must agree with.
        fn san() -> String {
            let files: Vec<char> = ('a'..='h').collect();
            let captures: Vec<String> = files
                .windows(2)
                .flat_map(|pair| {
                    [
                        format!("{}x{}", pair[0], pair[1]),
                        format!("{}x{}", pair[1], pair[0]),
                    ]
                })
                .collect();
            let pawn = format!("([a-h]|{})([2-7]|[18]=[QRBNK])", captures.join("|"));
            let piece = "[KQRBN][a-h]?[1-8]?x?[a-h][1-8]";
            let drop = "[NBRQ]@[a-h][1-8]|P@[a-h][2-7]";
            let castle = "O-O(-O)?|0-0(-0)?";
            format!("({pawn}|{piece}|{drop}|{castle})[+#]?|--")
        }

        /// Characters that [SAN](Ply) and the rest of [PGN](crate::Pgn) use, so that near misses
        /// come up often.
        const SOUP: &str = r"[KQRBNPZnpa-ijx0-9=+#@Oo\-.!?$* ]{0,8}";

        /// A [SAN](Ply) move with one character added, removed or replaced.
        fn near_miss() -> impl Strategy<Value = String> {
            let edit = prop_oneof![
                (any::<Index>(), SOUP).prop_map(|(at, c)| (at, 0, c)),
                any::<Index>().prop_map(|at| (at, 1, String::new())),
                (any::<Index>(), SOUP).prop_map(|(at, c)| (at, 1, c)),
            ];
            (string_regex(&san()), edit).prop_map(|(san, (at, removed, added))| {
                let mut chars: Vec<char> = san.chars().collect();
                let at = at.index(chars.len() + 1).min(chars.len());
                let end = (at + removed).min(chars.len());
                let added: Vec<char> = added.chars().take(1).collect();
                chars.splice(at..end, added);
                chars.into_iter().collect()
            })
        }

        fn string_regex(regex: &str) -> impl Strategy<Value = String> + use<> {
            proptest::string::string_regex(regex).unwrap()
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
                ..ProptestConfig::default()
            })]

            #[test]
            fn san_parses_as_written(text in string_regex(&san())) {
                let ply: Ply = text.parse().map_err(|err: anyhow::Error| TestCaseError::fail(err.to_string()))?;
                prop_assert_eq!(ply.as_str(), &text);
                prop_assert_eq!(ply.to_string(), text);
            }

            #[test]
            fn text_parses_exactly_when_it_is_san(
                text in prop_oneof![near_miss(), string_regex(SOUP), any::<String>()]
            ) {
                let oracle = Regex::new(&format!(r"\A(?:{})\z", san())).unwrap();
                prop_assert_eq!(text.parse::<Ply>().is_ok(), oracle.is_match(&text), "{:?}", text);
            }
        }
    }
}
