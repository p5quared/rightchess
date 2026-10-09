use std::collections::HashMap;

use crate::app::game_reader::GameReader;
use crate::app::sync::{BatchKey, SyncStore};
use crate::domain::Game;
use crate::error::Error;

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
}

impl SyncStore for MemoryStore {
    fn is_sealed(&self, key: &BatchKey) -> Result<bool, Error> {
        Ok(self.batches.get(key).is_some_and(|b| b.sealed))
    }

    fn save_batch(&mut self, key: BatchKey, sealed: bool, games: Vec<Game>) -> Result<(), Error> {
        self.batches.insert(key, StoredBatch { sealed, games });
        Ok(())
    }
}

impl GameReader for MemoryStore {
    fn games(&self) -> Result<Vec<Game>, Error> {
        let unique: HashMap<_, _> = self
            .batches
            .values()
            .flat_map(|b| &b.games)
            .map(|g| (&g.id, g))
            .collect();
        Ok(unique.into_values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    crate::adapters::store::contract::store_contract_tests!(super::MemoryStore::new());
}
