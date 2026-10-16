pub mod adapters;
mod app;
mod chess;
mod domain;

pub use app::evaluate::{
    EvalReport, EvalStore, Evaluator, EvaluatorId, FailedGame, evaluate_games, evaluate_position,
};
pub use app::game_reader::GameReader;
pub use app::sync::{Batch, BatchKey, GameSource, SourceQuery, SyncReport, SyncStore, sync};
pub use chess::{Color, Fen, NormalizedFen, Pgn, Ply};
pub use domain::features;
pub use domain::query;
pub use domain::{Eval, Game, GameId, Score, SourceKind, YearMonth};
