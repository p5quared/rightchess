//! The contract of [`Pgn::plies`]: which parts of a [PGN](Pgn) become
//! [plies](Ply). Only a move that isn't [SAN](Ply) is an error; text that isn't well-formed PGN
//! never becomes a [`Pgn`].

use crate::chess::{Pgn, Ply};

fn texts(plies: &[Ply]) -> Vec<&str> {
    plies.iter().map(Ply::as_str).collect()
}

/// `expected` lists the [plies](Ply) separated by spaces, e.g. `"e4 e5"`.
#[track_caller]
fn check(pgn: &str, expected: &str) {
    let plies = parse(pgn).plies().expect(pgn);
    let expected: Vec<&str> = expected.split_whitespace().collect();
    assert_eq!(texts(&plies), expected, "{pgn:?}");
}

/// `expected` is text the error message must contain.
#[track_caller]
fn check_err(pgn: &str, expected: &str) {
    let err = parse(pgn).plies().expect_err(pgn);
    assert!(err.to_string().contains(expected), "{pgn:?}: {err}");
}

#[track_caller]
fn parse(pgn: &str) -> Pgn {
    pgn.parse().unwrap_or_else(|err| panic!("{pgn:?}: {err}"))
}

mod spec {
    use super::{check, check_err};

    /// A daily game from chess.com's API, with the players renamed and the moves cut after move 8.
    const CHESS_COM_GAME: &str = r#"[Event "Let's Play!"]
[Site "Chess.com"]
[Date "2026.09.26"]
[Round "-"]
[White "someone"]
[Black "opponent"]
[Result "1-0"]
[CurrentPosition "3q3k/p3Q1bp/1p1p2p1/5r2/2rP1P2/2P5/P2B2PP/1R2R1K1 b - - 0 23"]
[Timezone "UTC"]
[ECO "B06"]
[ECOUrl "https://www.chess.com/openings/Modern-Defense-Three-Pawns-Attack-3...d6-4.Nf3-Nf6-5.Nc3"]
[UTCDate "2026.09.26"]
[UTCTime "02:13:52"]
[WhiteElo "1459"]
[BlackElo "1469"]
[TimeControl "1/259200"]
[Termination "someone won by resignation"]
[StartTime "02:13:52"]
[EndDate "2026.10.07"]
[EndTime "20:00:58"]
[Link "https://www.chess.com/game/daily/1"]

1. e4 {[%clk 0:22:44.2]} 1... d6 {[%clk 0:39:56.2]} 2. d4 {[%clk 0:13:49.7]} 2... Nf6 {[%clk 0:12:04.5]} 3. Nc3 {[%clk 2:17:59.9]} 3... g6 {[%clk 0:09:42.5]} 4. f4 {[%clk 0:03:00.8]} 4... Bg7 {[%clk 0:00:10]} 5. Nf3 {[%clk 2:10:11]} 5... Na6 {[%clk 0:06:31.6]} 6. Bb5+ {[%clk 0:54:57.4]} 6... c6 {[%clk 0:02:53.8]} 7. Bc4 {[%clk 0:19:10.5]} 7... Nc7 {[%clk 0:39:25.3]} 8. O-O {[%clk 0:22:43.9]} 8... O-O {[%clk 0:13:52]} 1-0
"#;

    #[test]
    fn moves_and_move_numbers() {
        check("", "");
        check(" \r\n\t ", "");
        check("e4 e5", "e4 e5");
        check("1. e4 e5 2. Nf3 Nc6", "e4 e5 Nf3 Nc6");
        check("1. e4 1... e5", "e4 e5");
        check("1.e4 e5 2.Nf3 2...Nc6", "e4 e5 Nf3 Nc6");
        check("1 e4 2 Nf3", "e4 Nf3");
        check("1. ... e5", "e5");
        check("119. Kh1 119... Kh8", "Kh1 Kh8");
        check("\r\n 1.\te4\r\n\r\n1...  e5 \r\n", "e4 e5");
    }

    #[test]
    fn moves_are_kept_as_written() {
        check(
            "1. exd5 Nbd7 2. R1e2 Qh4xe1+ 3. e8=Q# O-O-O",
            "exd5 Nbd7 R1e2 Qh4xe1+ e8=Q# O-O-O",
        );
        // Castling with zeros is neither a result nor a move number.
        check("1. 0-0 0-0-0", "0-0 0-0-0");
        check("1. N@f3 P@e6", "N@f3 P@e6");
        check("1. e4 -- 2. d4", "e4 -- d4");
    }

    #[test]
    fn annotations_are_dropped() {
        check(
            "1. e4! e5? 2. Nf3!! Nc6?? 3. Bb5!? a6?!",
            "e4 e5 Nf3 Nc6 Bb5 a6",
        );
        check("1. e4 !? e5 ?", "e4 e5");
        check("1. e4 $1 e5 $14 $146", "e4 e5");
        check("1. e4$1 e5", "e4 e5");
        check("1. f3 e5 2. g4 Qh4#!", "f3 e5 g4 Qh4#");
    }

    #[test]
    fn comments_are_skipped() {
        check("{ 1. d4 } 1. e4", "e4");
        check(
            "1. e4 {[%clk 0:02:59.9]} 1... e5 {[%clk 0:02:59.1]}",
            "e4 e5",
        );
        check("1. e4 { [%eval 0.17] [%clk 0:03:00] } 1... e5", "e4 e5");
        check("1. e4 { (0.05 → 0.85) Inaccuracy. } e5", "e4 e5");
        check("1. e4{a ( b ] ; c $1}e5", "e4 e5");
        check("1. e4 {two\nlines} e5", "e4 e5");
        check("1. e4 ; 1... c5 to the end of the line\n1... e5", "e4 e5");
        check("1. e4 ; { opens nothing\ne5", "e4 e5");
        check("1. e4 e5 ; no newline at the end", "e4 e5");
    }

    #[test]
    fn variations_are_skipped() {
        check("1. e4 e5 (1... c5 2. Nf3) 2. Nf3", "e4 e5 Nf3");
        check("1. e4 (1. d4 d5 (1... Nf6 2. c4) 2. c4) 1... e5", "e4 e5");
        check("1. e4 (1. d4 { a ) in a comment } d5) e5", "e4 e5");
        check("1. e4 (1. d4 ; a ) in a line comment\nd5) e5", "e4 e5");
        check("1. e4(1. d4)e5", "e4 e5");
        // Moves in a variation aren't checked.
        check("1. e4 (1. Nz9 hello) e5", "e4 e5");
    }

    #[test]
    fn tags_are_skipped() {
        check(
            CHESS_COM_GAME,
            "e4 d6 d4 Nf6 Nc3 g6 f4 Bg7 Nf3 Na6 Bb5+ c6 Bc4 Nc7 O-O O-O",
        );
        check("[Event \"Blitz ] 1. d4\"]\n1. e4", "e4");
        check("[Event \"say \\\"hi\\\" ] 1. d4\"]\n1. e4", "e4");
        check("[Event \"ends in a backslash \\\\\"] 1. e4", "e4");
        check("[White \"a\"][Black \"b\"]1. e4", "e4");
        check("[Event \"x\"]\n[Result \"1-0\"]\n\n1-0", "");
    }

    #[test]
    fn results_are_skipped() {
        check("1. e4 1-0", "e4");
        check("1. e4 0-1", "e4");
        check("1. e4 1/2-1/2", "e4");
        check("1. e4 *", "e4");
    }

    #[test]
    fn a_word_that_is_not_san_is_an_error() {
        check_err("1. e4 Nz9", "Nz9");
        check_err("1. e4 hello", "hello");
        check_err("1. e4 e5!x", "e5!x");
        check_err("1. e4 e5 2. e8", "e8");
    }
}

mod properties {
    use proptest::prelude::*;
    use proptest::test_runner::FileFailurePersistence;

    use super::*;
    use crate::chess::pgn::tests::games::{SOUP, game};

    /// What the contract promises about every [ply](Ply), whatever the input.
    fn check_ply(input: &str, ply: &str) -> Result<(), TestCaseError> {
        prop_assert!(!ply.is_empty(), "empty ply");
        prop_assert!(input.contains(ply), "{ply:?} is not in the input");
        prop_assert!(
            !ply.contains(|c: char| c.is_whitespace() || "[]{}();$".contains(c)),
            "{ply:?} holds whitespace or a delimiter"
        );
        prop_assert!(!ply.ends_with(['!', '?']), "{ply:?} ends in a glyph");
        let after_number = ply.trim_start_matches(|c: char| c.is_ascii_digit());
        prop_assert!(
            !after_number.is_empty() && !after_number.starts_with('.'),
            "{ply:?} is or starts with a move number"
        );
        prop_assert!(
            !["1-0", "0-1", "1/2-1/2", "*"].contains(&ply),
            "{ply:?} is a result"
        );
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
            ..ProptestConfig::default()
        })]

        #[test]
        fn generated_games_yield_their_moves(game in game()) {
            let fail = |err: anyhow::Error| TestCaseError::fail(err.to_string());
            let plies = game.text().parse::<Pgn>().map_err(fail)?.plies().map_err(fail)?;
            prop_assert_eq!(texts(&plies), game.moves);
        }

        #[test]
        fn any_text_yields_moves_or_an_error(text in prop_oneof![SOUP, any::<String>()]) {
            if let Ok(plies) = text.parse::<Pgn>().and_then(|pgn| pgn.plies()) {
                for ply in texts(&plies) {
                    check_ply(&text, ply)?;
                }
            }
        }
    }
}
