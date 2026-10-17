use std::fmt;

use serde::Serialize;

use crate::chess::Pgn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum SourceKind {
    ChessCom,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct GameId {
    pub source: SourceKind,
    pub external_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Game {
    pub id: GameId,
    pub pgn: Pgn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YearMonth {
    pub year: u16,
    pub month: u8,
}

impl fmt::Display for YearMonth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}
