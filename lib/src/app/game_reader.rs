use crate::domain::Game;
use crate::error::Error;

pub trait GameReader {
    /// Every stored game, each `GameId` once.
    fn games(&self) -> Result<Vec<Game>, Error>;
}
