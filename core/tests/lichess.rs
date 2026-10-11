//! Real games from lichess, through the crate's public API: each parses as written, gives lichess's
//! own move list, and, in standard chess, plays through position by position, each with a key.

use proptest::prelude::*;
use proptest::sample::{Index, select};
use proptest::test_runner::FileFailurePersistence;
use rightchess::{Fen, NormalizedFen, Pgn};
use serde::Deserialize;

/// One line of `fixtures/lichess_games.ndjson`: a game exported from lichess with clocks, evals
/// and analysis, and lichess's own list of its moves.
#[derive(Debug, Clone, Deserialize)]
struct LichessGame {
    id: String,
    variant: String,
    moves: String,
    pgn: String,
}

impl LichessGame {
    fn name(&self) -> String {
        format!("{} game {}", self.variant, self.id)
    }

    fn moves(&self) -> Vec<&str> {
        self.moves.split_whitespace().collect()
    }
}

fn games() -> Vec<LichessGame> {
    include_str!("fixtures/lichess_games.ndjson")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn parse(game: &LichessGame) -> Pgn {
    game.pgn
        .parse()
        .unwrap_or_else(|err| panic!("{}: {err}", game.name()))
}

/// The FENs of `pgn` as text, or the first error.
fn fens(pgn: &Pgn) -> Result<Vec<String>, String> {
    pgn.fens()
        .map(|item| {
            item.map(|fen| fen.to_string())
                .map_err(|err| err.to_string())
        })
        .collect()
}

/// What holds between consecutive positions of any game: the side to move alternates, the
/// fullmove number goes up after Black moves, and the halfmove clock resets or goes up by one.
fn check_sequence(fens: &[String]) -> Result<(), String> {
    for pair in fens.windows(2) {
        let [before, after] = [&pair[0], &pair[1]].map(|fen| {
            let fields: Vec<&str> = fen.split(' ').collect();
            let counter = |i: usize| fields[i].parse::<u32>().unwrap();
            (fields[1], counter(4), counter(5))
        });
        let (side, halfmove, fullmove) = before;
        let expected_side = if side == "w" { "b" } else { "w" };
        let expected_fullmove = if side == "w" { fullmove } else { fullmove + 1 };
        let ok = after.0 == expected_side
            && after.2 == expected_fullmove
            && (after.1 == 0 || after.1 == halfmove + 1);
        if !ok {
            return Err(format!("{:?} can't follow {:?}", pair[1], pair[0]));
        }
    }
    Ok(())
}

#[test]
fn games_parse_as_written() {
    for game in games() {
        assert_eq!(parse(&game).as_str(), game.pgn, "{}", game.name());
    }
}

#[test]
fn plies_match_lichess_move_lists() {
    for game in games() {
        let plies = parse(&game)
            .plies()
            .unwrap_or_else(|err| panic!("{}: {err}", game.name()));
        let plies: Vec<&str> = plies.iter().map(|ply| ply.as_str()).collect();
        assert_eq!(plies, game.moves(), "{}", game.name());
    }
}

#[test]
fn standard_games_play_through() {
    for game in games() {
        let name = game.name();
        let fens = fens(&parse(&game));
        if game.variant != "standard" {
            let err = fens.expect_err(&name);
            assert!(err.contains("unsupported variant"), "{name}: {err}");
            continue;
        }
        let fens = fens.unwrap_or_else(|err| panic!("{name}: {err}"));
        assert_eq!(fens.len(), game.moves().len() + 1, "{name}");
        assert_eq!(fens[0], Fen::starting().as_str(), "{name}");
        check_sequence(&fens).unwrap_or_else(|err| panic!("{name}: {err}"));
    }
}

/// Every position of a standard game has a key that keeps its placement, side to move and
/// castling rights, and reads back from its text.
#[test]
fn standard_positions_normalize() {
    for game in games()
        .into_iter()
        .filter(|game| game.variant == "standard")
    {
        let name = game.name();
        for fen in parse(&game).fens() {
            let fen = fen.unwrap_or_else(|err| panic!("{name}: {err}"));
            let key = NormalizedFen::from(&fen);
            let fields: Vec<&str> = fen.as_str().split(' ').collect();
            let kept = format!("{} ", fields[..3].join(" "));
            assert!(key.as_str().starts_with(&kept), "{name}: {fen} -> {key}");
            let read = key.as_str().parse::<NormalizedFen>();
            let read = read.unwrap_or_else(|err| panic!("{name}: {err}"));
            assert_eq!(read, key, "{name}");
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn a_prefix_of_a_game_yields_a_prefix_of_its_positions(
        game in select(games().into_iter().filter(|game| game.variant == "standard").collect::<Vec<_>>()),
        at in any::<Index>(),
    ) {
        let moves = game.moves();
        let fens_of = |moves: &[&str]| {
            let pgn: Pgn = moves.join(" ").parse().map_err(|err: anyhow::Error| TestCaseError::fail(err.to_string()))?;
            fens(&pgn).map_err(TestCaseError::fail)
        };
        let full = fens_of(&moves)?;
        let len = at.index(moves.len() + 1);
        prop_assert_eq!(fens_of(&moves[..len])?, &full[..=len]);
    }
}
