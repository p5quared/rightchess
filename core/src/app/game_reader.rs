use crate::domain::Game;
use anyhow::Result;

pub trait GameReader {
    /// Every stored game, each `GameId` once.
    ///
    /// # Errors
    ///
    /// Fails if the store can't be read.
    fn games(&self) -> Result<Vec<Game>>;
}
