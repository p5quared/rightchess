use crate::{Error, Game};

pub trait GameReader {
    /// Every stored game, each `GameId` once.
    fn games(&self) -> Result<Vec<Game>, Error>;
}
