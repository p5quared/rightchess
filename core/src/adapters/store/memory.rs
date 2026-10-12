use std::collections::HashMap;

use anyhow::Result;

use crate::app::evaluate::{EvalStore, EvaluatorId};
use crate::app::game_reader::GameReader;
use crate::app::sync::{BatchKey, SyncStore};
use crate::chess::NormalizedFen;
use crate::domain::{Eval, Game};

#[derive(Debug, Default)]
pub struct MemoryStore {
    batches: HashMap<BatchKey, StoredBatch>,
    evals: HashMap<(EvaluatorId, NormalizedFen), Eval>,
}

#[derive(Debug)]
struct StoredBatch {
    sealed: bool,
    games: Vec<Game>,
}

impl MemoryStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SyncStore for MemoryStore {
    fn is_sealed(&self, key: &BatchKey) -> Result<bool> {
        Ok(self.batches.get(key).is_some_and(|b| b.sealed))
    }

    fn save_batch(&mut self, key: BatchKey, sealed: bool, games: Vec<Game>) -> Result<()> {
        self.batches.insert(key, StoredBatch { sealed, games });
        Ok(())
    }
}

impl GameReader for MemoryStore {
    fn games(&self) -> Result<Vec<Game>> {
        let unique: HashMap<_, _> = self
            .batches
            .values()
            .flat_map(|b| &b.games)
            .map(|g| (&g.id, g))
            .collect();
        Ok(unique.into_values().cloned().collect())
    }
}

impl EvalStore for MemoryStore {
    fn eval(&self, evaluator: &EvaluatorId, position: &NormalizedFen) -> Result<Option<Eval>> {
        Ok(self
            .evals
            .get(&(evaluator.clone(), position.clone()))
            .copied())
    }

    fn save_evals(
        &mut self,
        evaluator: &EvaluatorId,
        evals: Vec<(NormalizedFen, Eval)>,
    ) -> Result<()> {
        let keyed = evals
            .into_iter()
            .map(|(position, eval)| ((evaluator.clone(), position), eval));
        self.evals.extend(keyed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    crate::adapters::store::contract::store_contract_tests!(super::MemoryStore::new());
}
