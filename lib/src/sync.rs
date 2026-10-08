use crate::{Batch, BatchKey, Error, Game, GameQuery, GameSource};

pub trait SyncStore {
    /// True if the batch is stored and was sealed when it was saved.
    fn is_sealed(&self, key: &BatchKey) -> bool;
    /// Replaces any games previously saved for this batch.
    fn save_batch(&mut self, key: BatchKey, sealed: bool, games: Vec<Game>);
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub fetched: usize,
    pub skipped: usize,
    pub games: usize,
}

/// Fetches every batch that could have changed since the last sync and saves it to `store`.
///
/// Batches are fetched one at a time. On error, batches saved before the failure stay saved.
pub fn sync<S: GameSource>(
    source: &S,
    query: &GameQuery,
    store: &mut impl SyncStore,
) -> Result<SyncReport, Error> {
    let mut report = SyncReport::default();
    for batch in source.batches(query)? {
        let key = batch.key();
        if batch.is_sealed() && store.is_sealed(&key) {
            report.skipped += 1;
            continue;
        }
        let games = source.fetch(&batch)?;
        report.fetched += 1;
        report.games += games.len();
        store.save_batch(key, batch.is_sealed(), games);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::{GameId, MemoryStore, Pgn, SourceKind};

    #[derive(Clone)]
    struct FakeBatch {
        id: &'static str,
        sealed: bool,
        /// `None` makes fetching this batch fail.
        games: Option<&'static [&'static str]>,
    }

    impl FakeBatch {
        fn sealed(id: &'static str, games: &'static [&'static str]) -> Self {
            Self {
                id,
                sealed: true,
                games: Some(games),
            }
        }

        fn open(id: &'static str, games: &'static [&'static str]) -> Self {
            Self {
                id,
                sealed: false,
                games: Some(games),
            }
        }

        fn broken(id: &'static str) -> Self {
            Self {
                id,
                sealed: false,
                games: None,
            }
        }
    }

    impl Batch for FakeBatch {
        fn key(&self) -> BatchKey {
            BatchKey {
                source: SourceKind::ChessCom,
                id: self.id.to_string(),
            }
        }

        fn is_sealed(&self) -> bool {
            self.sealed
        }
    }

    struct FakeSource {
        batches: Vec<FakeBatch>,
        fetched: RefCell<Vec<&'static str>>,
    }

    impl GameSource for FakeSource {
        type Batch = FakeBatch;

        fn batches(&self, _query: &GameQuery) -> Result<Vec<FakeBatch>, Error> {
            Ok(self.batches.clone())
        }

        fn fetch(&self, batch: &FakeBatch) -> Result<Vec<Game>, Error> {
            self.fetched.borrow_mut().push(batch.id);
            let games = batch
                .games
                .ok_or(Error::Http(ureq::Error::ConnectionFailed))?;
            Ok(games.iter().map(|id| game(id)).collect())
        }
    }

    fn game(id: &str) -> Game {
        Game {
            id: GameId {
                source: SourceKind::ChessCom,
                external_id: id.to_string(),
            },
            pgn: Pgn(String::new()),
        }
    }

    fn run(
        store: &mut MemoryStore,
        remote: &[FakeBatch],
    ) -> (Result<SyncReport, Error>, FakeSource) {
        let source = FakeSource {
            batches: remote.to_vec(),
            fetched: RefCell::new(Vec::new()),
        };
        let query = GameQuery {
            username: "someone".to_string(),
            since: None,
        };
        (sync(&source, &query, store), source)
    }

    #[track_caller]
    fn assert_stored(store: &MemoryStore, expected: &[&str]) {
        let mut ids: Vec<_> = store.games().map(|g| g.id.external_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, expected);
    }

    /// Syncs `store` against `remote`, expecting exactly the `fetched` batches to be
    /// downloaded and the store to then hold the `stored` games.
    #[track_caller]
    fn check(store: &mut MemoryStore, remote: &[FakeBatch], fetched: &[&str], stored: &[&str]) {
        let (report, source) = run(store, remote);
        assert_eq!(*source.fetched.borrow(), fetched);
        let games = remote
            .iter()
            .filter(|b| fetched.contains(&b.id))
            .map(|b| b.games.unwrap().len())
            .sum();
        assert_eq!(
            report.unwrap(),
            SyncReport {
                fetched: fetched.len(),
                skipped: remote.len() - fetched.len(),
                games,
            }
        );
        assert_stored(store, stored);
    }

    /// Syncs `store` against `remote`, expecting the sync to fail and the store to
    /// then hold the `stored` games.
    #[track_caller]
    fn check_failure(store: &mut MemoryStore, remote: &[FakeBatch], stored: &[&str]) {
        let (report, _) = run(store, remote);
        assert!(report.is_err(), "{report:?}");
        assert_stored(store, stored);
    }

    #[test]
    fn second_sync_fetches_only_unsealed_batches() {
        let remote = [
            FakeBatch::sealed("2024-01", &["a", "b"]),
            FakeBatch::sealed("2024-02", &["c"]),
            FakeBatch::open("2024-03", &["d"]),
        ];
        let mut store = MemoryStore::new();
        check(
            &mut store,
            &remote,
            &["2024-01", "2024-02", "2024-03"],
            &["a", "b", "c", "d"],
        );
        check(&mut store, &remote, &["2024-03"], &["a", "b", "c", "d"]);
    }

    #[test]
    fn batch_is_fetched_once_more_after_it_becomes_sealed() {
        let mut store = MemoryStore::new();
        check(
            &mut store,
            &[FakeBatch::open("2024-03", &["a"])],
            &["2024-03"],
            &["a"],
        );

        let remote = [
            FakeBatch::sealed("2024-03", &["a", "b"]),
            FakeBatch::open("2024-04", &["c"]),
        ];
        check(
            &mut store,
            &remote,
            &["2024-03", "2024-04"],
            &["a", "b", "c"],
        );
        check(&mut store, &remote, &["2024-04"], &["a", "b", "c"]);
    }

    #[test]
    fn refetching_a_batch_replaces_its_games() {
        let mut store = MemoryStore::new();
        check(
            &mut store,
            &[FakeBatch::open("2024-03", &["a"])],
            &["2024-03"],
            &["a"],
        );
        check(
            &mut store,
            &[FakeBatch::open("2024-03", &["b"])],
            &["2024-03"],
            &["b"],
        );
    }

    #[test]
    fn failed_fetch_keeps_earlier_batches() {
        let mut store = MemoryStore::new();
        let remote = [
            FakeBatch::sealed("2024-01", &["a"]),
            FakeBatch::broken("2024-02"),
            FakeBatch::open("2024-03", &["c"]),
        ];
        check_failure(&mut store, &remote, &["a"]);
    }
}
