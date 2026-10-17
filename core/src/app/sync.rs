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
    /// Every game in the stored batches among `keys`, each `GameId` once.
    ///
    /// # Errors
    ///
    /// Fails if the store can't be read.
    fn batch_games(&self, keys: &[BatchKey]) -> Result<Vec<Game>>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Every stored game in the listed batches, fetched this sync or not.
    pub games: Vec<Game>,
    pub num_fetched: usize,
    pub num_skipped: usize,
    /// Not `games.len()`: counts only games in the batches this sync fetched.
    pub num_games: usize,
    pub failed: Option<FailedBatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedBatch {
    pub batch: BatchKey,
    pub reason: String,
}

/// Fetches every batch that could have changed since the last sync and saves it to `store`.
///
/// Batches are fetched one at a time. The first that can't be fetched or saved stops the sync and
/// goes in `failed`; batches saved before it stay saved.
///
/// # Errors
///
/// Fails if the batches can't be listed or the store can't be read.
pub fn sync<S: GameSource>(
    source: &S,
    query: &SourceQuery,
    store: &mut impl SyncStore,
) -> Result<SyncReport> {
    let batches = source
        .batches(query)
        .with_context(|| format!("listing batches for {}", query.username))?;
    let keys: Vec<BatchKey> = batches.iter().map(Batch::key).collect();

    let to_fetch = batches
        .iter()
        .zip(&keys)
        .map(|(batch, key)| -> Result<_> {
            let unchanged = batch.is_sealed()
                && store
                    .is_sealed(key)
                    .with_context(|| format!("reading the stored batch {}", key.id))?;
            Ok((!unchanged).then_some((batch, key)))
        })
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>>>()?;

    let mut num_fetched = 0;
    let mut num_games = 0;
    let mut failed = None;
    for &(batch, key) in &to_fetch {
        match fetch_and_save(source, batch, key, store) {
            Ok(count) => {
                num_fetched += 1;
                num_games += count;
            }
            Err(err) => {
                failed = Some(FailedBatch {
                    batch: key.clone(),
                    reason: format!("{err:#}"),
                });
                break;
            }
        }
    }

    Ok(SyncReport {
        games: store
            .batch_games(&keys)
            .context("reading the synced games")?,
        num_fetched,
        num_skipped: batches.len() - to_fetch.len(),
        num_games,
        failed,
    })
}

fn fetch_and_save<S: GameSource>(
    source: &S,
    batch: &S::Batch,
    key: &BatchKey,
    store: &mut impl SyncStore,
) -> Result<usize> {
    let games = source.fetch(batch).context("fetching")?;
    let count = games.len();
    store
        .save_batch(key.clone(), batch.is_sealed(), games)
        .context("saving")?;
    Ok(count)
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
        batches: Option<Vec<FakeBatch>>,
        fetched: RefCell<Vec<&'static str>>,
    }

    impl GameSource for FakeSource {
        type Batch = FakeBatch;

        fn batches(&self, _query: &SourceQuery) -> Result<Vec<FakeBatch>> {
            self.batches
                .clone()
                .ok_or_else(|| anyhow::anyhow!("listing failed"))
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

    fn run_source(store: &mut MemoryStore, source: &FakeSource) -> Result<SyncReport> {
        let query = SourceQuery {
            username: "someone".to_string(),
            since: None,
        };
        sync(source, &query, store)
    }

    fn run(store: &mut MemoryStore, remote: &[FakeBatch]) -> (SyncReport, FakeSource) {
        let source = FakeSource {
            batches: Some(remote.to_vec()),
            fetched: RefCell::new(Vec::new()),
        };
        let report = run_source(store, &source).unwrap();
        (report, source)
    }

    fn sorted_ids(games: &[Game]) -> Vec<&str> {
        let mut ids: Vec<_> = games.iter().map(|g| g.id.external_id.as_str()).collect();
        ids.sort_unstable();
        ids
    }

    #[track_caller]
    fn assert_stored(store: &MemoryStore, expected: &[&str]) {
        assert_eq!(sorted_ids(&store.games().unwrap()), expected);
    }

    #[track_caller]
    fn check(store: &mut MemoryStore, remote: &[FakeBatch], fetched: &[&str], games: &[&str]) {
        let (report, source) = run(store, remote);
        assert_eq!(*source.fetched.borrow(), fetched);
        let num_games = remote
            .iter()
            .filter(|b| fetched.contains(&b.id))
            .map(|b| b.games.unwrap().len())
            .sum();
        assert_eq!(sorted_ids(&report.games), games);
        assert_eq!(
            report,
            SyncReport {
                games: report.games.clone(),
                num_fetched: fetched.len(),
                num_skipped: remote.len() - fetched.len(),
                num_games,
                failed: None,
            }
        );
    }

    #[track_caller]
    fn check_failure(
        store: &mut MemoryStore,
        remote: &[FakeBatch],
        fetched: &[&str],
        skipped: usize,
        games: &[&str],
    ) {
        let (report, source) = run(store, remote);
        assert_eq!(*source.fetched.borrow(), fetched);
        let (failed_id, saved) = fetched.split_last().expect("a batch is fetched");
        let failed = report.failed.as_ref().expect("a batch fails");
        assert_eq!(failed.batch.id, *failed_id);
        assert!(failed.reason.contains("fetch failed"), "{failed:?}");
        assert_eq!(
            (report.num_fetched, report.num_skipped),
            (saved.len(), skipped)
        );
        assert_eq!(sorted_ids(&report.games), games);
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
    fn report_leaves_out_stored_batches_the_query_does_not_list() {
        let mut store = MemoryStore::new();
        check(
            &mut store,
            &[
                FakeBatch::sealed("2024-01", &["a"]),
                FakeBatch::open("2024-02", &["b"]),
            ],
            &["2024-01", "2024-02"],
            &["a", "b"],
        );
        check(
            &mut store,
            &[FakeBatch::open("2024-02", &["b"])],
            &["2024-02"],
            &["b"],
        );
        assert_stored(&store, &["a", "b"]);
    }

    #[test]
    fn failed_fetch_stops_and_keeps_earlier_batches() {
        let mut store = MemoryStore::new();
        let remote = [
            FakeBatch::sealed("2024-01", &["a"]),
            FakeBatch::broken("2024-02"),
            FakeBatch::open("2024-03", &["c"]),
        ];
        check_failure(&mut store, &remote, &["2024-01", "2024-02"], 0, &["a"]);
        assert_stored(&store, &["a"]);
    }

    #[test]
    fn failed_fetch_still_reports_batches_from_earlier_syncs() {
        let mut store = MemoryStore::new();
        check(
            &mut store,
            &[
                FakeBatch::sealed("2024-01", &["a"]),
                FakeBatch::open("2024-03", &["c"]),
            ],
            &["2024-01", "2024-03"],
            &["a", "c"],
        );
        let remote = [
            FakeBatch::sealed("2024-01", &["a"]),
            FakeBatch::broken("2024-02"),
            FakeBatch::open("2024-03", &["c", "d"]),
        ];
        check_failure(&mut store, &remote, &["2024-02"], 1, &["a", "c"]);
    }

    #[test]
    fn failed_fetch_still_counts_later_skipped_batches() {
        let mut store = MemoryStore::new();
        check(
            &mut store,
            &[FakeBatch::sealed("2024-02", &["b"])],
            &["2024-02"],
            &["b"],
        );
        let remote = [
            FakeBatch::broken("2024-01"),
            FakeBatch::sealed("2024-02", &["b"]),
        ];
        check_failure(&mut store, &remote, &["2024-01"], 1, &["b"]);
    }

    #[test]
    fn failed_listing_is_an_error() {
        let source = FakeSource {
            batches: None,
            fetched: RefCell::new(Vec::new()),
        };
        let err = run_source(&mut MemoryStore::new(), &source).expect_err("listing fails");
        let err = format!("{err:#}");
        assert!(err.contains("listing batches for someone"), "{err}");
        assert!(err.contains("listing failed"), "{err}");
    }
}
