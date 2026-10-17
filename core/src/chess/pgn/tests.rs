//! The contract of [`Pgn`]: which text is one game in [PGN](Pgn). Its structure is checked:
//! comments, [variations](Pgn) and [tags](Pgn) close, tags are `[Name "value"]` pairs before the
//! [movetext](Pgn), and only comments follow the [result](Pgn). Its moves aren't; that's
//! [`Pgn::plies`].

use crate::chess::Pgn;

mod dates;
mod fens;
mod games;
mod plies;
mod tags;

/// `text` parses, and the pgn keeps it as written.
#[track_caller]
fn check(text: &str) {
    let pgn: Pgn = text.parse().unwrap_or_else(|err| panic!("{text:?}: {err}"));
    assert_eq!(pgn.as_str(), text);
    assert_eq!(pgn.to_string(), text);
}

/// `text` doesn't parse, and the error contains `expected`.
#[track_caller]
fn check_err(text: &str, expected: &str) {
    let err = text.parse::<Pgn>().expect_err(text);
    assert!(err.to_string().contains(expected), "{text:?}: {err}");
}

mod spec {
    use super::{Pgn, check, check_err};

    #[test]
    fn deserializing_checks_the_text() {
        let pgn: Pgn = serde_json::from_str(r#""1. e4 e5 *""#).unwrap();
        assert_eq!(pgn.as_str(), "1. e4 e5 *");
        serde_json::from_str::<Pgn>(r#""1. e4 {""#).expect_err("the comment isn't closed");
    }

    #[test]
    fn blank_text_is_a_game_without_moves() {
        check("");
        check(" \r\n\t ");
    }

    #[test]
    fn movetext() {
        for text in [
            "e4 e5",
            "1. e4 e5 2. Nf3 Nc6",
            "1.e4 e5 2.Nf3 2...Nc6",
            "1. ... e5",
            "1. e4! e5?? 2. Nf3 $1 Nc6 $14",
            "1. 0-0 0-0-0",
            "1. e4 -- 2. N@f3",
        ] {
            check(text);
        }
    }

    #[test]
    fn moves_are_not_checked() {
        for text in ["1. e4 Nz9", "hello world", "1. e4 e5!x", "1. e8", "1-0e5"] {
            check(text);
        }
    }

    #[test]
    fn comments_and_variations() {
        for text in [
            "1. e4 {[%clk 0:02:59.9]} 1... e5",
            "1. e4{a ( b ] ; c $1}e5",
            "1. e4 ; } ) ] to the end of the line\n1... e5",
            "1. e4 e5 ; no newline at the end",
            "1. e4 (1. d4 d5 (1... Nf6 2. c4) 2. c4) 1... e5",
            "1. e4(1. d4)e5",
            "1. e4 (1. d4 { a ) in a comment } d5) e5",
            "1. e4 (1. Nz9 hello) e5",
            "1. e4 ([Event \"x\"] 1. d4) e5",
        ] {
            check(text);
        }
    }

    #[test]
    fn tags() {
        for text in [
            "[Event \"x\"]",
            "[Event \"x\"]\n[Site \"y\"]\n\n1. e4 *",
            "[White \"a\"][Black \"b\"]1. e4",
            "[Event \"\"]",
            "[ Event \"x\" ]",
            "[Event\"x\"]",
            "[Event\n\"x\"]",
            "[event \"x\"]",
            "[WhiteElo \"1500\"] [Opening_Name2 \"x\"] [1Event \"x\"]",
            "[Event \"Blitz ] 1. d4\"]",
            "[Event \"say \\\"hi\\\" ] 1. d4\"]",
            "[Event \"ends in a backslash \\\\\"]",
            "[Event \"two\nlines\"]",
        ] {
            check(text);
        }
    }

    #[test]
    fn comments_may_come_anywhere() {
        for text in [
            "{ before the tags } [Event \"x\"] 1. e4",
            "; before the tags\n[Event \"x\"] 1. e4",
            "[Event \"x\"] { between the tags } [Site \"y\"] 1. e4",
            "1. e4 1-0 { after the result }",
            "1. e4 1-0 ; after the result",
            "1. e4 1-0 {a} {b}\n; c\n",
        ] {
            check(text);
        }
    }

    #[test]
    fn results() {
        for text in [
            "1-0",
            "1. e4 1-0",
            "1. e4 0-1",
            "1. e4 1/2-1/2",
            "1. e4 *",
            "1. e4 1-0\n\n\n",
            "[Event \"x\"]\n[Result \"1-0\"]\n\n1-0",
            // A result in a variation doesn't end the game.
            "1. e4 (1. d4 1-0) e5 *",
        ] {
            check(text);
        }
    }

    #[test]
    fn whitespace_is_kept() {
        check("\n\n[Event \"x\"]\r\n\r\n1.  e4\te5 * \n\n\n");
    }

    #[test]
    fn unterminated_and_unmatched_delimiters() {
        check_err("1. e4 { never closed", "unterminated comment");
        check_err("1. e4 (1. d4", "unterminated variation");
        check_err("1. e4 (1. d4 (1. c4) d5", "unterminated variation");
        check_err("1. e4 (1. d4 1-0", "unterminated variation");
        check_err("[Event \"x\"", "unterminated tag");
        check_err("[Event \"x] 1. e4", "unterminated tag");
        check_err("1. e4 ) e5", "unmatched )");
        check_err("1. e4 } e5", "unmatched }");
        check_err("1. e4 ] e5", "unmatched ]");
        check_err("[Event \"x\"]] 1. e4", "unmatched ]");
    }

    #[test]
    fn malformed_tags() {
        for tag in [
            "[]",
            "[ ]",
            "[Event]",
            "[Event ]",
            "[\"x\"]",
            "[Event x]",
            "[Event 'x']",
            "[Event \"a\" \"b\"]",
            "[Event \"x\" y]",
            "[_Event \"x\"]",
            "[Ev-ent \"x\"]",
            "[Évent \"x\"]",
        ] {
            check_err(
                &format!("{tag}\n\n1. e4 *"),
                &format!("malformed tag {tag}"),
            );
        }
    }

    #[test]
    fn tags_come_before_the_movetext() {
        for text in [
            "1. e4 [Site \"y\"]",
            "[Event \"x\"] e4 [Site \"y\"] e5",
            "1. [Site \"y\"]",
            "(1. d4) [Site \"y\"]",
            "$1 [Site \"y\"]",
            "e4 { comment } [Site \"y\"]",
        ] {
            check_err(text, "[Site \"y\"] after the movetext");
        }
    }

    #[test]
    fn only_comments_follow_the_result() {
        for (text, expected) in [
            ("1. e4 1-0 e5", "e5 after the result"),
            ("1. e4 1-0 0-1", "0-1 after the result"),
            ("1-0 1-0", "1-0 after the result"),
            ("1. e4 * 2.", "2. after the result"),
            ("1. e4 1-0 { comment } e5", "e5 after the result"),
            ("1. e4 1-0 (1. d4)", "( after the result"),
            ("1. e4 1-0 $1", "$1 after the result"),
            ("1. e4 1-0 !", "! after the result"),
            // Two games.
            (
                "[Event \"a\"] 1. e4 1-0\n\n[Event \"b\"] 1. d4 0-1",
                "[Event \"b\"] after the result",
            ),
        ] {
            check_err(text, expected);
        }
    }
}

mod properties {
    use proptest::prelude::*;
    use proptest::sample::{Index, select};
    use proptest::test_runner::FileFailurePersistence;

    use super::games::{Piece, SOUP, game, group, render, word};
    use super::*;

    const MALFORMED_TAGS: &[&str] = &[
        "[]",
        "[Event]",
        "[\"x\"]",
        "[Event x]",
        "[Event \"a\" \"b\"]",
        "[_Event \"x\"]",
        "[Ev-ent \"x\"]",
    ];

    /// What may wrongly follow a [result](Pgn), and the lexeme the error must name.
    const AFTER_RESULT: &[(&str, &str)] = &[
        ("e4", "e4"),
        ("2.", "2."),
        ("0-1", "0-1"),
        ("*", "*"),
        ("$1", "$1"),
        ("!", "!"),
        ("(1. d4)", "("),
        ("[Event \"x\"]", "[Event \"x\"]"),
    ];

    /// A generated game with one thing broken, and text the error must contain.
    fn broken_game() -> impl Strategy<Value = (String, String)> {
        prop_oneof![
            game().prop_map(|game| {
                let mut pieces = game.pieces();
                pieces.push(group("{ never closed"));
                (render(&pieces), "unterminated comment".to_string())
            }),
            (game(), any::<Index>()).prop_map(|(game, at)| {
                let mut pieces = game.pieces();
                let at = game.tags.len() + at.index(game.body.len() + 1);
                pieces.insert(at, group("("));
                (render(&pieces), "unterminated variation".to_string())
            }),
            // Anywhere before the result.
            (game(), any::<Index>(), select(vec![')', '}', ']'])).prop_map(|(game, at, close)| {
                let mut pieces = game.pieces();
                let at = at.index(game.tags.len() + game.body.len() + 1);
                pieces.insert(at, group(close.to_string()));
                (render(&pieces), format!("unmatched {close}"))
            }),
            (game(), any::<Index>(), select(MALFORMED_TAGS)).prop_map(|(mut game, at, tag)| {
                match game.tags.len() {
                    0 => game.tags.push(group(tag)),
                    len => game.tags[at.index(len)] = group(tag),
                }
                (game.text(), format!("malformed tag {tag}"))
            }),
            // Anywhere after the first word of the movetext, before the result.
            (game(), any::<Index>()).prop_map(|(mut game, at)| {
                let first_word = game.body.iter().position(Piece::is_word);
                let first_word = first_word.unwrap_or_else(|| {
                    game.body.insert(0, word("e4"));
                    0
                });
                let at = first_word + 1 + at.index(game.body.len() - first_word);
                game.body.insert(at, group("[Site \"y\"]"));
                (game.text(), "[Site \"y\"] after the movetext".to_string())
            }),
            // Anywhere among the comments after the result.
            (game(), any::<Index>(), select(AFTER_RESULT)).prop_map(
                |(mut game, at, (extra, lexeme))| {
                    game.result.get_or_insert_with(|| word("1-0"));
                    let piece = if extra.starts_with(['(', '[']) {
                        group(extra)
                    } else {
                        word(extra)
                    };
                    game.tail.insert(at.index(game.tail.len() + 1), piece);
                    (game.text(), format!("{lexeme} after the result"))
                }
            ),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
            ..ProptestConfig::default()
        })]

        #[test]
        fn generated_games_parse_as_written(game in game()) {
            let text = game.text();
            let pgn: Pgn = text.parse().map_err(|err: anyhow::Error| TestCaseError::fail(err.to_string()))?;
            prop_assert_eq!(pgn.as_str(), &text);
            prop_assert_eq!(pgn.to_string(), text);
        }

        #[test]
        fn broken_games_are_errors_that_name_the_problem((text, expected) in broken_game()) {
            let err = text.parse::<Pgn>().err();
            let err = err.ok_or_else(|| TestCaseError::fail(format!("{text:?} parsed")))?;
            prop_assert!(err.to_string().contains(&expected), "{:?}: {}", text, err);
        }

        /// A [`Pgn`] is as written, and its [plies](crate::Ply) can only fail on a move that isn't
        /// [SAN](crate::Ply).
        #[test]
        fn any_text_parses_as_written_or_is_an_error(text in prop_oneof![SOUP, any::<String>()]) {
            if let Ok(pgn) = text.parse::<Pgn>() {
                prop_assert_eq!(pgn.as_str(), &text);
                if let Err(err) = pgn.plies() {
                    prop_assert!(err.to_string().contains("is not a SAN move"), "{:?}: {}", text, err);
                }
            }
        }
    }
}
