//! The sync workflow: fetch chess.com games and return everything the UI needs, including the
//! store so the next sync of the same query can reuse it.

use std::cmp::Reverse;

use anyhow::Result;
use rightchess::adapters::{ChessCom, MemoryStore};
use rightchess::{Game, GameReader, SourceQuery, SyncReport, YearMonth, sync};

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

/// Newest first, by the day the game started and then by the source id.
pub fn sort_games(games: &mut [Game]) {
    games.sort_by_cached_key(|game| Reverse((game.pgn.date(), game.id.external_id.clone())));
}

#[cfg(test)]
mod tests {
    use rightchess::{GameId, SourceKind};

    use super::*;

    fn game(id: &str, tags: &[(&str, &str)]) -> Game {
        let pgn = tags
            .iter()
            .map(|(name, value)| format!("[{name} \"{value}\"]"))
            .collect::<Vec<_>>()
            .join("\n");
        Game {
            id: GameId {
                source: SourceKind::ChessCom,
                external_id: id.to_string(),
            },
            pgn: pgn.parse().unwrap(),
        }
    }

    #[track_caller]
    fn check(mut games: Vec<Game>, expected: &[&str]) {
        sort_games(&mut games);
        let ids: Vec<_> = games.iter().map(|g| g.id.external_id.as_str()).collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn newest_first() {
        check(
            vec![
                game("a", &[("UTCDate", "2024.03.15")]),
                game("b", &[("UTCDate", "2025.01.02")]),
                game("c", &[("UTCDate", "2024.11.30")]),
            ],
            &["b", "c", "a"],
        );
    }

    #[test]
    fn utc_date_wins_over_date() {
        check(
            vec![
                game("a", &[("Date", "2024.03.16"), ("UTCDate", "2024.03.15")]),
                game("b", &[("Date", "2024.03.15"), ("UTCDate", "2024.03.16")]),
            ],
            &["b", "a"],
        );
    }

    #[test]
    fn partial_utc_date_falls_back_to_date() {
        check(
            vec![
                game("a", &[("UTCDate", "2024.03.15")]),
                game("b", &[("Date", "2024.06.01"), ("UTCDate", "2024.??.??")]),
            ],
            &["b", "a"],
        );
    }

    #[test]
    fn undated_games_go_last_by_descending_id() {
        check(
            vec![
                game("a", &[]),
                game("c", &[("Date", "2024.??.??")]),
                game("b", &[("UTCDate", "2024.03.15")]),
            ],
            &["b", "c", "a"],
        );
    }
}
