//! The contract of [`Pgn::tag`]: the value of a game's first [tag](Pgn) with a given name.

use crate::chess::Pgn;

#[track_caller]
fn check(pgn: &str, name: &str, expected: Option<&str>) {
    let pgn: Pgn = pgn.parse().unwrap_or_else(|err| panic!("{pgn:?}: {err}"));
    assert_eq!(pgn.tag(name).as_deref(), expected, "{name} in {pgn:?}");
}

mod spec {
    use super::check;

    const CHESS_COM_TAGS: &str = r#"[Event "Live Chess"]
[Site "Chess.com"]
[White "someone"]
[Black "opponent"]
[Result "0-1"]
[WhiteElo "1459"]
[BlackElo "1469"]
[TimeControl "180+2"]
[Termination "opponent won by checkmate"]

1. e4 {[%clk 0:03:01.1]} 1... e5 {[%clk 0:03:00.8]} 0-1
"#;

    #[test]
    fn values_by_name() {
        for (name, expected) in [
            ("Event", "Live Chess"),
            ("White", "someone"),
            ("Black", "opponent"),
            ("BlackElo", "1469"),
            ("TimeControl", "180+2"),
            ("Termination", "opponent won by checkmate"),
        ] {
            check(CHESS_COM_TAGS, name, Some(expected));
        }
    }

    #[test]
    fn a_missing_tag_is_none() {
        check(CHESS_COM_TAGS, "ECO", None);
        check("1. e4 e5", "Event", None);
        check("", "Event", None);
    }

    #[test]
    fn names_are_case_sensitive() {
        check(CHESS_COM_TAGS, "timecontrol", None);
        check("[event \"x\"]", "Event", None);
        check("[event \"x\"]", "event", Some("x"));
    }

    #[test]
    fn names_match_whole() {
        check(CHESS_COM_TAGS, "White", Some("someone"));
        check(CHESS_COM_TAGS, "Elo", None);
        check(CHESS_COM_TAGS, "", None);
    }

    #[test]
    fn values_are_unescaped() {
        check(r#"[Event "say \"hi\""]"#, "Event", Some(r#"say "hi""#));
        check(
            r#"[Event "ends in a backslash \\"]"#,
            "Event",
            Some("ends in a backslash \\"),
        );
        check("[Event \"Blitz ] 1. d4\"]", "Event", Some("Blitz ] 1. d4"));
        check("[Event \"two\nlines\"]", "Event", Some("two\nlines"));
    }

    #[test]
    fn an_empty_value_is_some() {
        check("[Event \"\"]", "Event", Some(""));
    }

    #[test]
    fn whitespace_around_the_pair_is_dropped() {
        check("[ Event \"x\" ]", "Event", Some("x"));
        check("[Event\"x\"]", "Event", Some("x"));
        check("[Event\n\"x\"]", "Event", Some("x"));
        check("[Event \" x \"]", "Event", Some(" x "));
    }

    #[test]
    fn the_first_of_repeated_tags_wins() {
        check("[Event \"a\"]\n[Event \"b\"]", "Event", Some("a"));
    }

    #[test]
    fn tags_in_comments_and_variations_do_not_count() {
        check("{ [Event \"x\"] } 1. e4", "Event", None);
        check("; [Event \"x\"]\n1. e4", "Event", None);
        check("1. e4 ([Event \"x\"] 1. d4) e5", "Event", None);
        check(
            "[Event \"a\"] { [Event \"x\"] } [Site \"b\"]",
            "Event",
            Some("a"),
        );
    }
}

mod properties {
    use proptest::prelude::*;
    use proptest::test_runner::FileFailurePersistence;

    use crate::chess::Pgn;
    use crate::chess::pgn::tests::games::game;

    proptest! {
        #![proptest_config(ProptestConfig {
            failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
            ..ProptestConfig::default()
        })]

        #[test]
        fn each_tag_reads_back_its_first_value(game in game()) {
            let text = game.text();
            let pgn: Pgn = text.parse().map_err(|err: anyhow::Error| TestCaseError::fail(err.to_string()))?;
            for (name, _) in &game.tag_values {
                let first = game.tag_values.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
                prop_assert_eq!(pgn.tag(name), first, "{:?}", text);
            }
            // Generated names start with a capital letter.
            prop_assert_eq!(pgn.tag("absent"), None, "{:?}", text);
        }
    }
}
