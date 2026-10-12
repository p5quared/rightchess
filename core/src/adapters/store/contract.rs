//! Behavior every storage backend must share. A backend's test module runs it with
//! `store_contract_tests!(make_store)`.

use crate::app::evaluate::{EvalStore, EvaluatorId};
use crate::app::game_reader::GameReader;
use crate::app::sync::{BatchKey, SyncStore};
use crate::chess::NormalizedFen;
use crate::domain::{Eval, Game, GameId, Score, SourceKind};

macro_rules! store_contract_tests {
    ($make:expr) => {
        $crate::adapters::store::contract::store_contract_tests!(@cases $make;
            unknown_batch_is_not_sealed,
            batch_is_sealed_only_if_saved_sealed,
            resaving_a_batch_replaces_only_its_games,
            open_batch_can_become_sealed,
            empty_sealed_batch_is_sealed,
            game_in_two_batches_is_read_once,
            unusual_ids_round_trip,
            unknown_eval_is_none,
            saved_evals_read_back,
            evaluators_are_stored_apart,
            resaving_an_eval_replaces_it,
            every_score_round_trips,
            unusual_evaluator_ids_round_trip,
            saving_no_evals_is_fine,
        );
    };
    (@cases $make:expr; $($case:ident),* $(,)?) => {
        $(
            #[test]
            fn $case() {
                $crate::adapters::store::contract::$case($make);
            }
        )*
    };
}

pub(crate) use store_contract_tests;

fn key(id: &str) -> BatchKey {
    BatchKey {
        source: SourceKind::ChessCom,
        id: id.to_string(),
    }
}

/// The pgn is derived from the id so reads also check that pgns round-trip.
fn game(id: &str) -> Game {
    let site = id.replace('\\', r"\\").replace('"', r#"\""#);
    Game {
        id: GameId {
            source: SourceKind::ChessCom,
            external_id: id.to_string(),
        },
        pgn: format!("[Site \"{site}\"]\n\n1. e4 e5 *").parse().unwrap(),
    }
}

#[track_caller]
fn save(store: &mut impl SyncStore, batch: &str, sealed: bool, games: &[&str]) {
    let games = games.iter().map(|id| game(id)).collect();
    store.save_batch(key(batch), sealed, games).unwrap();
}

#[track_caller]
fn assert_sealed(store: &impl SyncStore, batch: &str, expected: bool) {
    assert_eq!(
        store.is_sealed(&key(batch)).unwrap(),
        expected,
        "batch {batch:?}"
    );
}

#[track_caller]
fn assert_games(store: &impl GameReader, expected: &[&str]) {
    let mut actual = store.games().unwrap();
    actual.sort_by(|a, b| a.id.external_id.cmp(&b.id.external_id));
    let mut expected: Vec<_> = expected.iter().map(|id| game(id)).collect();
    expected.sort_by(|a, b| a.id.external_id.cmp(&b.id.external_id));
    assert_eq!(actual, expected);
}

pub fn unknown_batch_is_not_sealed(store: impl SyncStore + GameReader) {
    assert_sealed(&store, "2024-03", false);
    assert_games(&store, &[]);
}

pub fn batch_is_sealed_only_if_saved_sealed(mut store: impl SyncStore + GameReader) {
    save(&mut store, "2024-01", true, &["a"]);
    save(&mut store, "2024-02", false, &["b"]);
    assert_sealed(&store, "2024-01", true);
    assert_sealed(&store, "2024-02", false);
    assert_games(&store, &["a", "b"]);
}

pub fn resaving_a_batch_replaces_only_its_games(mut store: impl SyncStore + GameReader) {
    save(&mut store, "2024-01", true, &["a"]);
    save(&mut store, "2024-02", false, &["b", "c"]);
    save(&mut store, "2024-02", false, &["d"]);
    assert_games(&store, &["a", "d"]);
}

pub fn open_batch_can_become_sealed(mut store: impl SyncStore + GameReader) {
    save(&mut store, "2024-03", false, &["a"]);
    save(&mut store, "2024-03", true, &["a", "b"]);
    assert_sealed(&store, "2024-03", true);
    assert_games(&store, &["a", "b"]);
}

pub fn empty_sealed_batch_is_sealed(mut store: impl SyncStore + GameReader) {
    save(&mut store, "2024-03", true, &[]);
    assert_sealed(&store, "2024-03", true);
    assert_games(&store, &[]);
}

/// Two players who played each other both have the game in their own batch.
pub fn game_in_two_batches_is_read_once(mut store: impl SyncStore + GameReader) {
    save(&mut store, "erik:2024-03", true, &["a", "b"]);
    save(&mut store, "hikaru:2024-03", true, &["a", "c"]);
    assert_games(&store, &["a", "b", "c"]);
}

/// Ids must not collide or break once a backend turns them into paths or keys.
pub fn unusual_ids_round_trip(mut store: impl SyncStore + GameReader) {
    let ids = ["a:b", "a/b", "a b", "../a", "ä'\"\\", "A:B"];
    for id in ids {
        save(&mut store, id, id != "A:B", &[id]);
    }
    for id in ids {
        assert_sealed(&store, id, id != "A:B");
    }
    assert_games(&store, &ids);
}

const START_KEY: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -";
const AFTER_E4_KEY: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq -";

fn evaluator(id: &str) -> EvaluatorId {
    EvaluatorId(id.to_string())
}

fn position(key: &str) -> NormalizedFen {
    key.parse().unwrap()
}

fn cp(centipawns: i32) -> Eval {
    Eval::from_white(Score::Cp(centipawns))
}

#[track_caller]
fn save_evals(store: &mut impl EvalStore, id: &str, evals: &[(&str, Eval)]) {
    let evals = evals
        .iter()
        .map(|(key, eval)| (position(key), *eval))
        .collect();
    store.save_evals(&evaluator(id), evals).unwrap();
}

#[track_caller]
fn assert_eval(store: &impl EvalStore, id: &str, key: &str, expected: Option<Eval>) {
    let actual = store.eval(&evaluator(id), &position(key)).unwrap();
    assert_eq!(actual, expected, "evaluator {id:?}, position {key:?}");
}

pub fn unknown_eval_is_none(store: impl EvalStore) {
    assert_eval(&store, "sf", START_KEY, None);
}

pub fn saved_evals_read_back(mut store: impl EvalStore) {
    save_evals(
        &mut store,
        "sf",
        &[(START_KEY, cp(20)), (AFTER_E4_KEY, cp(-30))],
    );
    assert_eval(&store, "sf", START_KEY, Some(cp(20)));
    assert_eval(&store, "sf", AFTER_E4_KEY, Some(cp(-30)));
}

pub fn evaluators_are_stored_apart(mut store: impl EvalStore) {
    save_evals(&mut store, "a", &[(START_KEY, cp(20))]);
    save_evals(&mut store, "b", &[(START_KEY, cp(35))]);
    assert_eval(&store, "a", START_KEY, Some(cp(20)));
    assert_eval(&store, "b", START_KEY, Some(cp(35)));
    assert_eval(&store, "c", START_KEY, None);
}

pub fn resaving_an_eval_replaces_it(mut store: impl EvalStore) {
    save_evals(
        &mut store,
        "sf",
        &[(START_KEY, cp(20)), (AFTER_E4_KEY, cp(-30))],
    );
    save_evals(&mut store, "sf", &[(START_KEY, cp(25))]);
    assert_eval(&store, "sf", START_KEY, Some(cp(25)));
    assert_eval(&store, "sf", AFTER_E4_KEY, Some(cp(-30)));
}

pub fn every_score_round_trips(mut store: impl EvalStore) {
    let scores = [
        Score::Cp(0),
        Score::Cp(i32::MIN),
        Score::Cp(i32::MAX),
        Score::Mates(0),
        Score::Mates(255),
        Score::Mated(0),
        Score::Mated(255),
    ];
    for (i, score) in scores.iter().enumerate() {
        save_evals(
            &mut store,
            &i.to_string(),
            &[(START_KEY, Eval::from_white(*score))],
        );
    }
    for (i, score) in scores.iter().enumerate() {
        assert_eval(
            &store,
            &i.to_string(),
            START_KEY,
            Some(Eval::from_white(*score)),
        );
    }
}

pub fn unusual_evaluator_ids_round_trip(mut store: impl EvalStore) {
    let ids = ["", "a:b", "a/b", "a b", "../a", "ä'\"\\", "A:B", "a"];
    for (i, id) in ids.iter().enumerate() {
        save_evals(&mut store, id, &[(START_KEY, cp(i as i32))]);
    }
    for (i, id) in ids.iter().enumerate() {
        assert_eval(&store, id, START_KEY, Some(cp(i as i32)));
    }
}

pub fn saving_no_evals_is_fine(mut store: impl EvalStore) {
    store.save_evals(&evaluator("sf"), Vec::new()).unwrap();
    assert_eval(&store, "sf", START_KEY, None);
}
