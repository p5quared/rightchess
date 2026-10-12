use crate::domain::{Game, SourceKind, YearMonth};
use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceQuery {
    pub username: String,
    /// Only load [batches](Batch) from this month onward.
    pub since: Option<YearMonth>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BatchKey {
    pub source: SourceKind,
    pub id: String,
}

/// A source-defined unit of fetching, e.g. one chess.com monthly archive.
pub trait Batch {
    fn key(&self) -> BatchKey;
    /// True once the batch's games can no longer change.
    fn is_sealed(&self) -> bool;
}

pub trait GameSource {
    type Batch: Batch;

    /// # Errors
    ///
    /// Fails if the source can't list the user's batches.
    fn batches(&self, query: &SourceQuery) -> Result<Vec<Self::Batch>>;
    /// # Errors
    ///
    /// Fails if the batch can't be fetched or its games can't be read.
    fn fetch(&self, batch: &Self::Batch) -> Result<Vec<Game>>;
}

pub trait SyncStore {
    /// True if the [batch](Batch) is stored and was [sealed](Batch::is_sealed) when it was saved.
    ///
    /// # Errors
    ///
    /// Fails if the store can't be read.
    fn is_sealed(&self, key: &BatchKey) -> Result<bool>;
    /// Atomically replaces the batch's games and sealed flag.
    ///
    /// # Errors
    ///
    /// Fails if the store can't be written, leaving the batch as it was.
    fn save_batch(&mut self, key: BatchKey, sealed: bool, games: Vec<Game>) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub num_fetched: usize,
    pub num_skipped: usize,
    pub num_games: usize,
}

/// Fetches every batch that could have changed since the last sync and saves it to `store`.
///
/// Batches are fetched one at a time.
///
/// # Errors
///
/// Fails if listing, fetching or saving a batch fails. Batches saved before the failure stay
/// saved.
pub fn sync<S: GameSource>(
    source: &S,
    query: &SourceQuery,
    store: &mut impl SyncStore,
) -> Result<SyncReport> {
    let mut to_fetch = Vec::new();
    let mut num_skipped = 0;
    let batches = source
        .batches(query)
        .with_context(|| format!("listing batches for {}", query.username))?;
    for batch in batches {
        if batch.is_sealed() && store.is_sealed(&batch.key())? {
            num_skipped += 1;
        } else {
            to_fetch.push(batch);
        }
    }

    let num_games = to_fetch
        .iter()
        .try_fold(0, |total, batch| -> Result<usize> {
            let key = batch.key();
            let games = source
                .fetch(batch)
                .with_context(|| format!("fetching batch {}", key.id))?;
            let count = games.len();
            store
                .save_batch(key.clone(), batch.is_sealed(), games)
                .with_context(|| format!("saving batch {}", key.id))?;
            Ok(total + count)
        })?;

    Ok(SyncReport {
        num_fetched: to_fetch.len(),
        num_skipped,
        num_games,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::adapters::MemoryStore;
    use crate::app::game_reader::GameReader;
    use crate::domain::{GameId, SourceKind};

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

        fn batches(&self, _query: &SourceQuery) -> Result<Vec<FakeBatch>> {
            Ok(self.batches.clone())
        }

        fn fetch(&self, batch: &FakeBatch) -> Result<Vec<Game>> {
            self.fetched.borrow_mut().push(batch.id);
            let games = batch.games.ok_or_else(|| anyhow::anyhow!("fetch failed"))?;
            Ok(games.iter().map(|id| game(id)).collect())
        }
    }

    fn game(id: &str) -> Game {
        Game {
            id: GameId {
                source: SourceKind::ChessCom,
                external_id: id.to_string(),
            },
            pgn: "".parse().unwrap(),
        }
    }

    fn run(store: &mut MemoryStore, remote: &[FakeBatch]) -> (Result<SyncReport>, FakeSource) {
        let source = FakeSource {
            batches: remote.to_vec(),
            fetched: RefCell::new(Vec::new()),
        };
        let query = SourceQuery {
            username: "someone".to_string(),
            since: None,
        };
        (sync(&source, &query, store), source)
    }

    #[track_caller]
    fn assert_stored(store: &MemoryStore, expected: &[&str]) {
        let mut ids: Vec<_> = store
            .games()
            .unwrap()
            .into_iter()
            .map(|g| g.id.external_id)
            .collect();
        ids.sort();
        assert_eq!(ids, expected);
    }

    /// Syncs `store` against `remote`, expecting exactly the `fetched` batches to be
    /// downloaded and the store to then hold the `stored` games.
    #[track_caller]
    fn check(store: &mut MemoryStore, remote: &[FakeBatch], fetched: &[&str], stored: &[&str]) {
        let (report, source) = run(store, remote);
        assert_eq!(*source.fetched.borrow(), fetched);
        let num_games = remote
            .iter()
            .filter(|b| fetched.contains(&b.id))
            .map(|b| b.games.unwrap().len())
            .sum();
        assert_eq!(
            report.unwrap(),
            SyncReport {
                num_fetched: fetched.len(),
                num_skipped: remote.len() - fetched.len(),
                num_games,
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
