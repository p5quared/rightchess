//! Queries end to end through the crate's public API: a position drawn as a board, one move
//! played from it by the PGN rules, both positions evaluated by a real Stockfish (found as in
//! `tests/stockfish.rs`), and the query matched against that decision.
//!
//! Each case is a board, a move in SAN, a query and the expected [`MatchStatus`]. Positions are
//! picked so that any strong engine agrees on the outcome, not on exact scores.

use std::env;
use std::num::NonZeroUsize;

use rightchess::adapters::{MemoryStore, Stockfish};
use rightchess::query::{
    BlunderQuery, DecisionFacts, EvaluationQuery, MatchStatus, PlyQuery, Query, matches,
};
use rightchess::{Color, Fen, Pgn, evaluate_position};

const NODES: u64 = 200_000;

/// A board as eight ranks from Black's side, one square per word, `.` for empty and FEN letters
/// for pieces, then FEN's last five fields. The counters place the position in a game from the
/// standard start, which gives the move's [ply](PlyQuery).
fn fen(board: &str) -> Fen {
    let lines: Vec<&str> = board
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let [ranks @ .., footer] = lines.as_slice() else {
        unreachable!("a slice pattern with a rest always matches a nonempty slice")
    };
    assert_eq!(
        ranks.len(),
        8,
        "expected eight ranks then the footer:\n{board}"
    );
    let placement: Vec<String> = ranks
        .iter()
        .map(|rank| {
            let squares: Vec<&str> = rank.split_whitespace().collect();
            assert_eq!(squares.len(), 8, "expected eight squares in {rank:?}");
            let mut text = String::new();
            let mut empty = 0;
            for square in squares {
                if square == "." {
                    empty += 1;
                    continue;
                }
                if empty > 0 {
                    text += &empty.to_string();
                    empty = 0;
                }
                text += square;
            }
            if empty > 0 {
                text += &empty.to_string();
            }
            text
        })
        .collect();
    format!("{} {footer}", placement.join("/"))
        .parse()
        .unwrap_or_else(|err| panic!("{err}\n{board}"))
}

/// The ply of the next move from `fen`, counting from the standard start.
fn next_ply(fen: &Fen) -> NonZeroUsize {
    let fullmove: usize = fen
        .as_str()
        .rsplit(' ')
        .next()
        .and_then(|field| field.parse().ok())
        .unwrap_or_else(|| panic!("{fen} has no fullmove number"));
    let ply = match fen.side_to_move() {
        Color::White => 2 * fullmove - 1,
        Color::Black => 2 * fullmove,
    };
    NonZeroUsize::new(ply).unwrap_or_else(|| panic!("{fen} has fullmove number 0"))
}

#[track_caller]
fn check(board: &str, san: &str, query: impl Into<Query>, expected: MatchStatus) {
    let query = query.into();
    let start = fen(board);
    let pgn: Pgn = format!("[FEN \"{start}\"]\n\n{san} *")
        .parse()
        .unwrap_or_else(|err| panic!("{err}"));
    let fens: Vec<Fen> = pgn
        .fens()
        .collect::<anyhow::Result<_>>()
        .unwrap_or_else(|err| panic!("playing {san}: {err:#}\n{board}"));
    let [before, after] = fens.as_slice() else {
        panic!("expected two positions from {pgn}, got {fens:?}")
    };

    let program = env::var_os("STOCKFISH").unwrap_or_else(|| "stockfish".into());
    let mut engine = Stockfish::spawn(&program, NODES)
        .unwrap_or_else(|err| panic!("starting {}: {err:#}", program.display()));
    let mut store = MemoryStore::new();
    let [before_eval, after_eval] = [before, after].map(|fen| {
        evaluate_position(fen, &mut engine, &mut store)
            .unwrap_or_else(|err| panic!("evaluating {fen}: {err:#}"))
    });

    let facts = DecisionFacts::from_evals(
        Some(next_ply(before)),
        before.side_to_move(),
        Some(before_eval),
        Some(after_eval),
    );
    let actual = matches(&query, &facts);
    assert_eq!(
        actual, expected,
        "\n{board}\nmove {san}\nbefore {before_eval:?}\nafter {after_eval:?}\nquery {query:?}"
    );
}

use MatchStatus::{Match, NoMatch, Unknown};

/// 1. e4 d5 2. exd5 Qxd5 3. Nc3, attacking Black's queen.
const SCANDINAVIAN: &str = "
    r n b . k b n r
    p p p . p p p p
    . . . . . . . .
    . . . q . . . .
    . . . . . . . .
    . . N . . . . .
    P P P P . P P P
    R . B Q K B N R
    b KQkq - 1 3
";

/// 1. e4 e5 2. Qh5 Nc6 3. Bc4, threatening Qxf7#.
const SCHOLARS_MATE_THREAT: &str = "
    r . b q k b n r
    p p p p . p p p
    . . n . . . . .
    . . . . p . . Q
    . . B . P . . .
    . . . . . . . .
    P P P P . P P P
    R N B . K . N R
    b KQkq - 3 3
";

/// [`SCHOLARS_MATE_THREAT`] after 3... Nf6??, so Qxf7 is mate.
const SCHOLARS_MATE_IN_ONE: &str = "
    r . b q k b . r
    p p p p . p p p
    . . n . . n . .
    . . . . p . . Q
    . . B . P . . .
    . . . . . . . .
    P P P P . P P P
    R N B . K . N R
    w KQkq - 4 4
";

/// 1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5.
const ITALIAN: &str = "
    r . b q k . n r
    p p p p . p p p
    . . n . . . . .
    . . b . p . . .
    . . B . P . . .
    . . . . . N . .
    P P P P . P P P
    R N B Q K . . R
    w KQkq - 4 4
";

const BLACK_WITHOUT_QUEEN: &str = "
    r n b . k b n r
    p p p p p p p p
    . . . . . . . .
    . . . . . . . .
    . . . . . . . .
    . . . . . . . .
    P P P P P P P P
    R N B Q K B N R
    w KQkq - 0 1
";

const WHITE_WITHOUT_QUEEN: &str = "
    r n b q k b n r
    p p p p p p p p
    . . . . . . . .
    . . . . . . . .
    . . . . . . . .
    . . . . . . . .
    P P P P P P P P
    R N B . K B N R
    b KQkq - 0 1
";

fn blunder(min_centipawn_loss: u32) -> BlunderQuery {
    BlunderQuery::loss_at_least(min_centipawn_loss)
}

fn ply(first: usize, last: usize) -> PlyQuery {
    PlyQuery::between(first, last).unwrap()
}

#[test]
fn boards_read_as_fens() {
    let start = "
        r n b q k b n r
        p p p p p p p p
        . . . . . . . .
        . . . . . . . .
        . . . . . . . .
        . . . . . . . .
        P P P P P P P P
        R N B Q K B N R
        w KQkq - 0 1
    ";
    assert_eq!(fen(start), Fen::starting());
    assert_eq!(
        fen(SCANDINAVIAN).as_str(),
        "rnb1kbnr/ppp1pppp/8/3q4/8/2N5/PPPP1PPP/R1BQKBNR b KQkq - 1 3"
    );
    assert_eq!(next_ply(&fen(start)).get(), 1);
    assert_eq!(next_ply(&fen(SCANDINAVIAN)).get(), 6);
}

mod blunders {
    use super::*;

    #[test]
    fn saving_an_attacked_queen_is_not_a_blunder() {
        check(SCANDINAVIAN, "Qa5", blunder(200), NoMatch);
        check(SCHOLARS_MATE_THREAT, "g6", blunder(200), NoMatch);
    }

    #[test]
    fn leaving_a_queen_to_be_taken_is_a_blunder() {
        check(SCANDINAVIAN, "Bg4", blunder(200), Match);
        check(SCANDINAVIAN, "Qxa2", blunder(200), Match);
    }

    #[test]
    fn castling_is_not_a_blunder() {
        check(ITALIAN, "O-O", blunder(100), NoMatch);
    }

    /// Centipawn loss can't be measured once either side has a forced mate.
    #[test]
    fn allowing_mate_is_unknown() {
        check(SCHOLARS_MATE_THREAT, "Nf6", blunder(200), Unknown);
    }

    #[test]
    fn delivering_mate_is_unknown() {
        check(SCHOLARS_MATE_IN_ONE, "Qxf7#", blunder(200), Unknown);
    }
}

mod evaluations {
    use super::*;

    #[test]
    fn are_from_the_movers_side() {
        check(
            BLACK_WITHOUT_QUEEN,
            "e4",
            EvaluationQuery::before_move().at_least(500),
            Match,
        );
        check(
            WHITE_WITHOUT_QUEEN,
            "e5",
            EvaluationQuery::before_move().at_least(500),
            Match,
        );
    }

    #[test]
    fn after_the_move_stay_on_the_movers_side() {
        check(
            BLACK_WITHOUT_QUEEN,
            "e4",
            EvaluationQuery::after_move().at_least(500),
            Match,
        );
        check(
            SCANDINAVIAN,
            "Bg4",
            EvaluationQuery::after_move().at_most(-500),
            Match,
        );
    }

    #[test]
    fn outside_the_range_do_not_match() {
        check(
            SCANDINAVIAN,
            "Qa5",
            EvaluationQuery::before_move().at_least(200),
            NoMatch,
        );
        check(
            BLACK_WITHOUT_QUEEN,
            "e4",
            EvaluationQuery::before_move().between(-100, 100).unwrap(),
            NoMatch,
        );
    }

    #[test]
    fn a_roughly_even_position_is_between_small_bounds() {
        check(
            ITALIAN,
            "O-O",
            EvaluationQuery::before_move().between(-150, 150).unwrap(),
            Match,
        );
    }

    #[test]
    fn a_mate_score_is_unknown() {
        check(
            SCHOLARS_MATE_THREAT,
            "Nf6",
            EvaluationQuery::after_move().at_most(0),
            Unknown,
        );
        check(
            SCHOLARS_MATE_IN_ONE,
            "Qxf7#",
            EvaluationQuery::before_move().at_least(0),
            Unknown,
        );
    }
}

mod plies {
    use super::*;

    #[test]
    fn come_from_the_move_counters() {
        check(SCANDINAVIAN, "Qa5", ply(6, 6), Match);
        check(SCHOLARS_MATE_IN_ONE, "Qxf7#", ply(7, 7), Match);
        check(BLACK_WITHOUT_QUEEN, "e4", ply(1, 1), Match);
        check(WHITE_WITHOUT_QUEEN, "e5", ply(2, 2), Match);
    }

    #[test]
    fn outside_the_range_do_not_match() {
        check(SCANDINAVIAN, "Qa5", ply(1, 5), NoMatch);
        check(SCANDINAVIAN, "Qa5", ply(7, 40), NoMatch);
    }
}

mod compositions {
    use super::*;

    #[test]
    fn all_needs_every_condition() {
        let early_blunder = Query::all([blunder(200).into(), ply(1, 10).into()]);
        check(SCANDINAVIAN, "Bg4", early_blunder.clone(), Match);
        check(SCANDINAVIAN, "Qa5", early_blunder, NoMatch);
        let late_blunder = Query::all([blunder(200).into(), ply(20, 30).into()]);
        check(SCANDINAVIAN, "Bg4", late_blunder, NoMatch);
    }

    #[test]
    fn any_needs_one_condition() {
        let blunder_or_sixth = Query::any([blunder(200).into(), ply(6, 6).into()]);
        check(SCANDINAVIAN, "Qa5", blunder_or_sixth, Match);
        let blunder_or_late = Query::any([blunder(200).into(), ply(20, 30).into()]);
        check(SCANDINAVIAN, "Qa5", blunder_or_late, NoMatch);
    }

    #[test]
    fn a_known_answer_outweighs_an_unknown_one() {
        let mate_blunder_late = Query::all([blunder(200).into(), ply(20, 30).into()]);
        check(SCHOLARS_MATE_THREAT, "Nf6", mate_blunder_late, NoMatch);
        let mate_blunder_or_sixth = Query::any([blunder(200).into(), ply(6, 6).into()]);
        check(SCHOLARS_MATE_THREAT, "Nf6", mate_blunder_or_sixth, Match);
    }

    #[test]
    fn otherwise_an_unknown_answer_wins() {
        let mate_blunder_early = Query::all([blunder(200).into(), ply(1, 10).into()]);
        check(SCHOLARS_MATE_THREAT, "Nf6", mate_blunder_early, Unknown);
        let mate_blunder_or_late = Query::any([blunder(200).into(), ply(20, 30).into()]);
        check(SCHOLARS_MATE_THREAT, "Nf6", mate_blunder_or_late, Unknown);
    }

    #[test]
    fn empty_all_matches_and_empty_any_does_not() {
        check(ITALIAN, "O-O", Query::all([]), Match);
        check(ITALIAN, "O-O", Query::any([]), NoMatch);
    }

    #[test]
    fn nest() {
        let nested = Query::any([
            Query::all([blunder(200).into(), ply(1, 4).into()]),
            Query::all([
                EvaluationQuery::after_move().at_most(-500).into(),
                ply(5, 8).into(),
            ]),
        ]);
        check(SCANDINAVIAN, "Bg4", nested.clone(), Match);
        check(SCANDINAVIAN, "Qa5", nested, NoMatch);
    }
}
