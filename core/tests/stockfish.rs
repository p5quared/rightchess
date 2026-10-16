//! A real Stockfish, found through `STOCKFISH` or else as `stockfish` on `PATH`, through the
//! crate's public API.

use std::{env, slice};

use rightchess::adapters::Stockfish;
use rightchess::{Eval, Evaluator, EvaluatorId, Fen, Score};

const NODES: u64 = 200_000;

fn engine() -> Stockfish {
    let program = env::var_os("STOCKFISH").unwrap_or_else(|| "stockfish".into());
    Stockfish::spawn(&program, NODES)
        .unwrap_or_else(|err| panic!("starting {}: {err:#}", program.display()))
}

fn fen(text: &str) -> Fen {
    text.parse().unwrap_or_else(|err| panic!("{err}"))
}

fn eval(position: &str) -> Eval {
    let evals = engine()
        .evaluate(&[fen(position)])
        .unwrap_or_else(|err| panic!("{position}: {err:#}"));
    assert_eq!(evals.len(), 1, "{position}: {evals:?}");
    evals[0]
}

fn centipawns(eval: Eval) -> i32 {
    match eval.white() {
        Score::Cp(cp) => cp,
        score @ (Score::Mates(_) | Score::Mated(_)) => panic!("expected centipawns, got {score:?}"),
    }
}

const WHITE_WITHOUT_QUEEN_W: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB1KBNR w KQkq - 0 1";
const WHITE_WITHOUT_QUEEN_B: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB1KBNR b KQkq - 0 1";

#[test]
fn the_id_names_the_engine_and_its_settings() {
    let EvaluatorId(id) = engine().id();
    assert!(id.starts_with("Stockfish "), "{id}");
    assert!(id.contains(&format!("nodes={NODES}")), "{id}");
}

#[test]
fn the_starting_position_is_about_even() {
    let cp = centipawns(eval(&Fen::starting().to_string()));
    assert!((-100..=100).contains(&cp), "{cp}");
}

/// UCI scores are from the side to move; an [`Eval`] is from White whoever moves.
#[test]
fn scores_are_converted_from_the_side_to_move() {
    for position in [WHITE_WITHOUT_QUEEN_W, WHITE_WITHOUT_QUEEN_B] {
        let cp = centipawns(eval(position));
        assert!(cp <= -500, "{position}: {cp}");
    }
}

#[test]
fn one_eval_per_position_in_order() {
    let positions = [Fen::starting(), fen(WHITE_WITHOUT_QUEEN_W), Fen::starting()];
    let evals = engine().evaluate(&positions).unwrap();
    assert_eq!(evals.len(), 3, "{evals:?}");
    assert!(centipawns(evals[0]) > centipawns(evals[1]), "{evals:?}");
    assert_eq!(evals[0], evals[2], "{evals:?}");
    assert_eq!(engine().evaluate(&[]).unwrap(), Vec::<Eval>::new());
}

#[test]
fn evals_are_deterministic() {
    let middlegame = fen("r1bq1rk1/pp2bppp/2n1pn2/3p4/2PP4/2N1PN2/PP3PPP/R2QKB1R w KQ - 0 9");
    let mut first = engine();
    let once = first.evaluate(slice::from_ref(&middlegame)).unwrap();
    let again = first.evaluate(slice::from_ref(&middlegame)).unwrap();
    let fresh = engine().evaluate(&[middlegame]).unwrap();
    assert_eq!(once, again);
    assert_eq!(once, fresh);
}

#[test]
fn a_mate_for_the_side_to_move() {
    let back_rank = "6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1";
    assert_eq!(eval(back_rank).white(), Score::Mates(1));
    let mirrored = "r5k1/8/8/8/8/8/5PPP/6K1 b - - 0 1";
    assert_eq!(eval(mirrored).black(), Score::Mates(1));
}

/// After 1. f3 e5 2. g4 Qh4#.
#[test]
fn a_checkmated_side_to_move() {
    let fools_mate = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
    assert_eq!(eval(fools_mate).white(), Score::Mated(0));
}

#[test]
fn a_stalemate_is_even() {
    let stalemate = "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1";
    assert_eq!(eval(stalemate).white(), Score::Cp(0));
}
