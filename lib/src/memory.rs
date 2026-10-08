use std::collections::HashMap;

use crate::{BatchKey, Game, SyncStore};

#[derive(Debug, Default)]
pub struct MemoryStore {
    batches: HashMap<BatchKey, StoredBatch>,
}

#[derive(Debug)]
struct StoredBatch {
    sealed: bool,
    games: Vec<Game>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn games(&self) -> impl Iterator<Item = &Game> {
        self.batches.values().flat_map(|b| &b.games)
    }
}

impl SyncStore for MemoryStore {
    fn is_sealed(&self, key: &BatchKey) -> bool {
        self.batches.get(key).is_some_and(|b| b.sealed)
    }

    fn save_batch(&mut self, key: BatchKey, sealed: bool, games: Vec<Game>) {
        self.batches.insert(key, StoredBatch { sealed, games });
    }
}
