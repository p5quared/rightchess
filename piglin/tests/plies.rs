//! The contract of `Pgn::plies`: which parts of a PGN become plies, and which inputs are errors.

use piglin::{Pgn, Ply};

fn texts(plies: &[Ply]) -> Vec<&str> {
    plies.iter().map(Ply::as_str).collect()
}

/// `expected` lists the plies separated by spaces, e.g. `"e4 e5"`.
#[track_caller]
fn check(pgn: &str, expected: &str) {
    let plies = Pgn(pgn.to_string()).plies().expect(pgn);
    let expected: Vec<&str> = expected.split_whitespace().collect();
    assert_eq!(texts(&plies), expected, "{pgn:?}");
}

/// `expected` is text the error message must contain.
#[track_caller]
fn check_err(pgn: &str, expected: &str) {
    let err = Pgn(pgn.to_string()).plies().expect_err(pgn);
    assert!(err.to_string().contains(expected), "{pgn:?}: {err}");
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
    fn malformed_movetext_is_an_error() {
        check_err("1. e4 { never closed", "comment");
        check_err("1. e4 (1. d4", "variation");
        check_err("1. e4 (1. d4 (1. c4) d5", "variation");
        check_err("[Event \"x\"", "tag");
        check_err("[Event \"x] 1. e4", "tag");
        check_err("1. e4 ) e5", "unmatched )");
        check_err("1. e4 } e5", "unmatched }");
        check_err("1. e4 ] e5", "unmatched ]");
    }

    #[test]
    fn a_word_that_is_not_san_is_an_error() {
        check_err("1. e4 Nz9", "Nz9");
        check_err("1. e4 hello", "hello");
        check_err("1. e4 e5!x", "e5!x");
        check_err("1. e4 e5 2. e8", "e8");
    }
}

mod real_games {
    use serde::Deserialize;

    use super::*;

    /// One line of `fixtures/lichess_games.ndjson`: a game exported from lichess with clocks,
    /// evals and analysis, and lichess's own list of its moves.
    #[derive(Deserialize)]
    struct LichessGame {
        id: String,
        variant: String,
        moves: String,
        pgn: String,
    }

    #[test]
    fn lichess_plies_match_lichess_move_lists() {
        for line in include_str!("fixtures/lichess_games.ndjson").lines() {
            let game: LichessGame = serde_json::from_str(line).unwrap();
            let plies = Pgn(game.pgn)
                .plies()
                .unwrap_or_else(|err| panic!("{} game {}: {err}", game.variant, game.id));
            let expected: Vec<&str> = game.moves.split_whitespace().collect();
            assert_eq!(texts(&plies), expected, "{} game {}", game.variant, game.id);
        }
    }
}

mod properties {
    use proptest::prelude::*;
    use proptest::strategy::Union;
    use proptest::test_runner::FileFailurePersistence;

    use super::*;

    /// A move as SAN writes it, e.g. `Nbxd7+`, `e8=Q#` or `O-O-O`.
    const SAN: &str = "([KQRBN][a-h]?[1-8]?x?[a-h][1-8]\
        |([a-h]|axb|bxc|cxd|dxe|exf|fxg|gxh|hxg|gxf|fxe|exd|dxc|cxb|bxa)([2-7]|[18]=[QRBN])\
        |O-O(-O)?)[+#]?";
    const GLYPH: &str = r"!|\?|!!|\?\?|!\?|\?!";
    const MOVE_NUMBER: &str = r"[1-9][0-9]{0,2}\.{0,3}";
    /// A move number written against its move, as in `12.e4`.
    const NUMBER_PREFIX: &str = r"[1-9][0-9]{0,2}\.{1,3}";
    const NAG: &str = r"\$[0-9]{1,3}";
    const RESULT: &str = r"1-0|0-1|1/2-1/2|\*";
    /// Comment text may hold anything but `}`. These are the characters that could confuse a
    /// tokenizer.
    const COMMENT: &str = r#"\{[ a-h1-8KQRBNOx=+#.\-(){\[\];$!?"\\→\t\r\n]{0,20}\}"#;
    const LINE_COMMENT: &str = r#";[ a-h1-8KQRBNOx=+#.\-(){}\[\];$!?"\\→\t]{0,20}\n"#;
    const TAG_NAME: &str = "[A-Z][A-Za-z]{0,8}";
    const TAG_VALUE: &str = r#"[ a-h1-8KQRBNOx=+#.\-(){}\[\];$!?"\\→]{0,20}"#;
    /// Whitespace that may come before any piece of the PGN.
    const SPACE: &str = "[ \t\r\n]{0,2}";
    /// Characters that mean something in PGN, so that delimiters meet often.
    const SOUP: &str = r#"[\[\]{}()$;.!?*0-9a-hKQRBNOx=+#/@\-"\\→ \t\r\n]{0,60}"#;

    /// A piece of PGN text. Words (moves, move numbers, glyphs, NAGs and results) need
    /// whitespace between them. Comments, variations and tags may touch anything.
    #[derive(Debug, Clone)]
    struct Piece {
        space: String,
        text: String,
        word: bool,
    }

    fn word(text: String) -> Piece {
        Piece {
            space: String::new(),
            text,
            word: true,
        }
    }

    fn group(text: String) -> Piece {
        Piece {
            space: String::new(),
            text,
            word: false,
        }
    }

    fn spaced(piece: impl Strategy<Value = Piece>) -> impl Strategy<Value = Piece> {
        (SPACE, piece).prop_map(|(space, piece)| Piece { space, ..piece })
    }

    fn render(pieces: &[Piece]) -> String {
        let mut text = String::new();
        let mut after_word = false;
        for piece in pieces {
            let needs_space = after_word && piece.word && piece.space.is_empty();
            text += if needs_space {
                " "
            } else {
                piece.space.as_str()
            };
            text += &piece.text;
            after_word = piece.word;
        }
        text
    }

    /// The ply a move must give, and the move as written: maybe with its number in front
    /// (`12.e4`) and a glyph behind (`e4!?`).
    fn mv() -> impl Strategy<Value = (String, Piece)> {
        (
            SPACE,
            prop::option::of(NUMBER_PREFIX),
            SAN,
            prop::option::of(GLYPH),
        )
            .prop_map(|(space, number, san, glyph)| {
                let number = number.unwrap_or_default();
                let glyph = glyph.unwrap_or_default();
                let text = format!("{number}{san}{glyph}");
                let piece = Piece {
                    space,
                    text,
                    word: true,
                };
                (san, piece)
            })
    }

    /// Text that adds no plies. Variations nest up to `depth` deep.
    fn filler(depth: u32) -> BoxedStrategy<Piece> {
        let mut kinds = vec![
            spaced(MOVE_NUMBER.prop_map(word)).boxed(),
            spaced(GLYPH.prop_map(word)).boxed(),
            spaced(NAG.prop_map(word)).boxed(),
            spaced(COMMENT.prop_map(group)).boxed(),
            spaced(LINE_COMMENT.prop_map(group)).boxed(),
        ];
        if depth > 0 {
            let variation =
                movetext(depth - 1).prop_map(|(_, pieces)| group(format!("({})", render(&pieces))));
            kinds.push(spaced(variation).boxed());
        }
        Union::new(kinds).boxed()
    }

    /// Moves, and pieces that spell them out with filler around each move.
    fn movetext(depth: u32) -> BoxedStrategy<(Vec<String>, Vec<Piece>)> {
        let max_plies = 4 + 4 * depth as usize;
        let ply = (prop::collection::vec(filler(depth), 0..3), mv());
        (
            prop::collection::vec(ply, 0..max_plies),
            prop::collection::vec(filler(depth), 0..3),
        )
            .prop_map(|(plies, tail)| {
                let mut moves = Vec::new();
                let mut pieces = Vec::new();
                for (fillers, (san, piece)) in plies {
                    pieces.extend(fillers);
                    pieces.push(piece);
                    moves.push(san);
                }
                pieces.extend(tail);
                (moves, pieces)
            })
            .boxed()
    }

    /// A game's moves, and a PGN with tags, filler around the moves and maybe a result.
    fn game() -> impl Strategy<Value = (Vec<String>, String)> {
        let tag = (TAG_NAME, TAG_VALUE).prop_map(|(name, value)| {
            let value = value.replace('\\', r"\\").replace('"', r#"\""#);
            group(format!("[{name} \"{value}\"]"))
        });
        (
            prop::collection::vec(spaced(tag), 0..4),
            movetext(2),
            prop::option::of(spaced(RESULT.prop_map(word))),
        )
            .prop_map(|(tags, (moves, body), result)| {
                let pieces: Vec<Piece> = tags.into_iter().chain(body).chain(result).collect();
                (moves, render(&pieces))
            })
    }

    /// What the contract promises about every ply, whatever the input.
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
        fn generated_games_yield_their_moves((moves, pgn) in game()) {
            let plies = Pgn(pgn).plies().map_err(|err| TestCaseError::fail(err.to_string()))?;
            prop_assert_eq!(texts(&plies), moves);
        }

        #[test]
        fn any_text_yields_moves_or_an_error(text in prop_oneof![SOUP, any::<String>()]) {
            if let Ok(plies) = Pgn(text.clone()).plies() {
                for ply in texts(&plies) {
                    check_ply(&text, ply)?;
                }
            }
        }
    }
}
