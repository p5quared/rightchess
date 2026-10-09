pub mod adapters;
mod app;
mod domain;
mod error;

pub use app::game_reader::GameReader;
pub use app::sync::{Batch, BatchKey, GameSource, SourceQuery, SyncReport, SyncStore, sync};
pub use domain::{Game, GameId, Pgn, SourceKind, YearMonth};
pub use error::Error;
