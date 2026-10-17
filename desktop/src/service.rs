//! The sync workflow: fetch chess.com games and return everything the UI needs, including the
//! store so the next sync of the same query can reuse it.

use std::cmp::Reverse;

use anyhow::Result;
use rightchess::adapters::{ChessCom, MemoryStore};
use rightchess::{Game, GameReader, SourceQuery, SyncReport, YearMonth, sync};

use crate::json;

/// chess.com asks API clients to identify themselves.
const USER_AGENT: &str = "rightchess-desktop/0.1 (github.com/rightchess)";

/// The result of one sync attempt.
pub struct Synced {
    /// `Ok` when every batch was fetched. `Err` holds the failure; the games below may still
    /// contain batches saved before it.
    pub result: Result<SyncReport>,
    /// Handed back so a later sync of the same query reuses the downloaded batches.
    pub store: MemoryStore,
    /// Every game in the store, newest first.
    pub games: Vec<Game>,
}

/// Runs one sync from scratch or on top of an existing store.
#[must_use]
pub fn run(store: Option<MemoryStore>, username: String, since: Option<YearMonth>) -> Synced {
    let mut store = store.unwrap_or_default();
    let source = ChessCom::new(USER_AGENT);
    let query = SourceQuery { username, since };
    let result = sync(&source, &query, &mut store);
    // Games that were saved before a failure still belong in the result.
    let mut games = store.games().unwrap_or_default();
    sort_games(&mut games);
    Synced {
        result,
        store,
        games,
    }
}

/// Newest first, by PGN date and then by the source id for games without a date.
pub fn sort_games(games: &mut [Game]) {
    games.sort_by_key(|game| Reverse(sort_key(game)));
}

fn sort_key(game: &Game) -> (Option<(i32, u32, u32)>, String) {
    (
        json::meta(game).date.as_deref().and_then(parse_date),
        game.id.external_id.clone(),
    )
}

/// Parses a PGN date tag like `2024.03.15`.
fn parse_date(text: &str) -> Option<(i32, u32, u32)> {
    let mut parts = text.split('.');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    Some((year, month, day))
}
