use std::cmp::Reverse;
use std::sync::{Mutex, PoisonError};

use anyhow::{Context, Result};
use rightchess::adapters::{ChessCom, FileStore};
use rightchess::{Game, SourceQuery, SyncReport, YearMonth, sync};

/// chess.com asks API clients to identify themselves.
const USER_AGENT: &str = "rightchess-desktop/0.1 (github.com/rightchess)";

pub fn open_store() -> Result<FileStore> {
    let dir = dirs::data_dir().context("this system has no app data directory")?;
    FileStore::open(dir.join("rightchess/store.postcard"))
}

/// The report's games are sorted newest first.
pub fn run(
    store: &Mutex<FileStore>,
    username: String,
    since: Option<YearMonth>,
) -> Result<SyncReport> {
    // A sync that panicked poisons the lock, but each batch it saved was written whole, so the
    // store is still sound.
    let mut store = store.lock().unwrap_or_else(PoisonError::into_inner);
    let source = ChessCom::new(USER_AGENT);
    let query = SourceQuery { username, since };
    let mut report = sync(&source, &query, &mut *store)?;
    sort_games(&mut report.games);
    Ok(report)
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
