pub mod adapters;
mod app;
mod domain;

pub use app::game_reader::GameReader;
pub use app::sync::{Batch, BatchKey, GameSource, SourceQuery, SyncReport, SyncStore, sync};
pub use domain::{Game, GameId, Pgn, Ply, SourceKind, YearMonth};
