use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

use crate::adapters::MemoryStore;
use crate::app::evaluate::{EvalStore, EvaluatorId};
use crate::app::game_reader::GameReader;
use crate::app::sync::{BatchKey, SyncStore};
use crate::chess::NormalizedFen;
use crate::domain::{Eval, Game};

/// A [`MemoryStore`] saved whole to one file after every write.
#[derive(Debug)]
pub struct FileStore {
    path: PathBuf,
    state: MemoryStore,
}

impl FileStore {
    /// Starts empty if there's no file at `path` yet.
    ///
    /// # Errors
    ///
    /// Fails if the file can't be read or isn't a store.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let state =
            read(&path).with_context(|| format!("opening the store at {}", path.display()))?;
        Ok(Self { path, state })
    }

    fn commit(&mut self, change: impl FnOnce(&mut MemoryStore) -> Result<()>) -> Result<()> {
        let mut next = self.state.clone();
        change(&mut next)?;
        write(&self.path, &next)
            .with_context(|| format!("writing the store at {}", self.path.display()))?;
        self.state = next;
        Ok(())
    }
}

impl SyncStore for FileStore {
    fn is_sealed(&self, key: &BatchKey) -> Result<bool> {
        self.state.is_sealed(key)
    }

    fn save_batch(&mut self, key: BatchKey, sealed: bool, games: Vec<Game>) -> Result<()> {
        self.commit(|state| state.save_batch(key, sealed, games))
    }

    fn batch_games(&self, keys: &[BatchKey]) -> Result<Vec<Game>> {
        self.state.batch_games(keys)
    }
}

impl GameReader for FileStore {
    fn games(&self) -> Result<Vec<Game>> {
        self.state.games()
    }
}

impl EvalStore for FileStore {
    fn eval(&self, evaluator: &EvaluatorId, position: &NormalizedFen) -> Result<Option<Eval>> {
        self.state.eval(evaluator, position)
    }

    fn save_evals(
        &mut self,
        evaluator: &EvaluatorId,
        evals: Vec<(NormalizedFen, Eval)>,
    ) -> Result<()> {
        self.commit(|state| state.save_evals(evaluator, evals))
    }
}

fn read(path: &Path) -> Result<MemoryStore> {
    match fs::read(path) {
        Ok(bytes) => {
            let (state, rest) = postcard::take_from_bytes(&bytes)?;
            ensure!(
                rest.is_empty(),
                "{} stray bytes after the store",
                rest.len()
            );
            Ok(state)
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(MemoryStore::default()),
        Err(err) => Err(err.into()),
    }
}

fn write(path: &Path, state: &MemoryStore) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let copy = temp_path(path);
    fs::write(&copy, postcard::to_stdvec(state)?)?;
    fs::rename(&copy, path)?;
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let mut copy = path.as_os_str().to_owned();
    copy.push(".tmp");
    copy.into()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use anyhow::Result;
    use tempfile::TempDir;

    use super::{FileStore, temp_path};
    use crate::adapters::store::contract::{START_KEY, cp, evaluator, game, key};
    use crate::app::evaluate::{EvalStore, EvaluatorId};
    use crate::app::game_reader::GameReader;
    use crate::app::sync::{BatchKey, SyncStore};
    use crate::chess::NormalizedFen;
    use crate::domain::{Eval, Game};

    #[derive(Debug)]
    struct Reopening {
        dir: TempDir,
        store: FileStore,
    }

    impl Reopening {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let store = FileStore::open(store_path(&dir)).unwrap();
            Self { dir, store }
        }

        fn reopen(&mut self) -> Result<()> {
            self.store = FileStore::open(store_path(&self.dir))?;
            Ok(())
        }
    }

    impl SyncStore for Reopening {
        fn is_sealed(&self, key: &BatchKey) -> Result<bool> {
            self.store.is_sealed(key)
        }

        fn save_batch(&mut self, key: BatchKey, sealed: bool, games: Vec<Game>) -> Result<()> {
            self.store.save_batch(key, sealed, games)?;
            self.reopen()
        }

        fn batch_games(&self, keys: &[BatchKey]) -> Result<Vec<Game>> {
            self.store.batch_games(keys)
        }
    }

    impl GameReader for Reopening {
        fn games(&self) -> Result<Vec<Game>> {
            self.store.games()
        }
    }

    impl EvalStore for Reopening {
        fn eval(&self, evaluator: &EvaluatorId, position: &NormalizedFen) -> Result<Option<Eval>> {
            self.store.eval(evaluator, position)
        }

        fn save_evals(
            &mut self,
            evaluator: &EvaluatorId,
            evals: Vec<(NormalizedFen, Eval)>,
        ) -> Result<()> {
            self.store.save_evals(evaluator, evals)?;
            self.reopen()
        }
    }

    crate::adapters::store::contract::store_contract_tests!(Reopening::new());

    fn evals(centipawns: i32) -> Vec<(NormalizedFen, Eval)> {
        vec![(START_KEY.parse().unwrap(), cp(centipawns))]
    }

    #[track_caller]
    fn assert_holds(store: &FileStore, games: &[&str], centipawns: i32) {
        let mut ids: Vec<_> = store
            .games()
            .unwrap()
            .into_iter()
            .map(|g| g.id.external_id)
            .collect();
        ids.sort();
        assert_eq!(ids, games);
        let eval = store.eval(&evaluator("sf"), &START_KEY.parse().unwrap());
        assert_eq!(eval.unwrap(), Some(cp(centipawns)));
    }

    fn store_path(dir: &TempDir) -> PathBuf {
        dir.path().join("store")
    }

    #[test]
    fn missing_directories_are_created() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a/b/store");
        let mut store = FileStore::open(&path).unwrap();
        store
            .save_batch(key("2024-03"), true, vec![game("a")])
            .unwrap();
        store.save_evals(&evaluator("sf"), evals(20)).unwrap();
        assert_holds(&FileStore::open(&path).unwrap(), &["a"], 20);
    }

    #[test]
    fn a_file_that_is_not_a_store_fails_to_open_and_is_left_alone() {
        let empty_store_then_junk = b"\0\0junk";
        for bytes in [&b"not a store"[..], empty_store_then_junk, &[0; 16]] {
            let dir = TempDir::new().unwrap();
            let path = store_path(&dir);
            fs::write(&path, bytes).unwrap();
            let err = FileStore::open(&path).expect_err(&format!("{bytes:?}"));
            assert!(
                format!("{err:#}").contains(&path.display().to_string()),
                "{err:#}"
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn a_directory_fails_to_open() {
        let dir = TempDir::new().unwrap();
        FileStore::open(dir.path()).expect_err("a directory isn't a store");
    }

    #[test]
    fn a_failed_write_changes_neither_memory_nor_disk() {
        let dir = TempDir::new().unwrap();
        let path = store_path(&dir);
        let mut store = FileStore::open(&path).unwrap();
        store
            .save_batch(key("2024-03"), false, vec![game("a")])
            .unwrap();
        store.save_evals(&evaluator("sf"), evals(20)).unwrap();

        fs::create_dir(temp_path(&path)).unwrap();
        store
            .save_batch(key("2024-03"), true, vec![game("b")])
            .expect_err("the temp path is taken");
        store
            .save_evals(&evaluator("sf"), evals(35))
            .expect_err("the temp path is taken");

        assert!(
            !store.is_sealed(&key("2024-03")).unwrap(),
            "the batch was saved open"
        );
        assert_holds(&store, &["a"], 20);
        assert_holds(&FileStore::open(&path).unwrap(), &["a"], 20);
    }
}
