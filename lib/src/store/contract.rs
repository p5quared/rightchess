//! Behavior every storage backend must share. A backend's test module runs it with
//! `store_contract_tests!(make_store)`.

use crate::{BatchKey, Game, GameId, GameReader, Pgn, SourceKind, SyncStore};

macro_rules! store_contract_tests {
    ($make:expr) => {
        $crate::store::contract::store_contract_tests!(@cases $make;
            unknown_batch_is_not_sealed,
            batch_is_sealed_only_if_saved_sealed,
            resaving_a_batch_replaces_only_its_games,
            open_batch_can_become_sealed,
            empty_sealed_batch_is_sealed,
            game_in_two_batches_is_read_once,
            unusual_ids_round_trip,
        );
    };
    (@cases $make:expr; $($case:ident),* $(,)?) => {
        $(
            #[test]
            fn $case() {
                $crate::store::contract::$case($make);
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
    Game {
        id: GameId {
            source: SourceKind::ChessCom,
            external_id: id.to_string(),
        },
        pgn: Pgn(format!("[Site \"{id}\"]\n\n1. e4 e5 *")),
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
