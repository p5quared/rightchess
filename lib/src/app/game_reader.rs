use crate::domain::Game;
use anyhow::Result;

pub trait GameReader {
    /// Every stored game, each `GameId` once.
    fn games(&self) -> Result<Vec<Game>>;
}
