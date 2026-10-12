use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::app::sync::{Batch, BatchKey, GameSource, SourceQuery};
use crate::domain::{Game, GameId, SourceKind, YearMonth};

const API_BASE: &str = "https://api.chess.com/pub";
/// Monthly archives of very active players can exceed ureq's 10MB default.
const MAX_RESPONSE_BYTES: u64 = 100 * 1024 * 1024;

pub struct ChessCom {
    http: Http,
}

enum Http {
    Ureq(ureq::Agent),
    /// Response bodies keyed by url.
    #[cfg(test)]
    Fake(std::collections::HashMap<String, String>),
}

impl ChessCom {
    /// chess.com asks API clients to identify themselves, ideally with contact details.
    #[must_use]
    pub fn new(user_agent: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(user_agent)
            .build()
            .into();
        Self {
            http: Http::Ureq(agent),
        }
    }

    #[cfg(test)]
    fn fake(responses: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            http: Http::Fake(responses.into_iter().collect()),
        }
    }

    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        match &self.http {
            Http::Ureq(agent) => {
                let value = agent
                    .get(url)
                    .call()
                    .and_then(|mut response| {
                        response
                            .body_mut()
                            .with_config()
                            .limit(MAX_RESPONSE_BYTES)
                            .read_json()
                    })
                    .with_context(|| format!("requesting {url}"))?;
                Ok(value)
            }
            #[cfg(test)]
            Http::Fake(responses) => {
                let body = responses
                    .get(url)
                    .unwrap_or_else(|| panic!("unexpected request: {url}"));
                Ok(serde_json::from_str(body).unwrap())
            }
        }
    }
}

/// One chess.com monthly archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChessComBatch {
    username: String,
    month: YearMonth,
    url: String,
    sealed: bool,
}

impl Batch for ChessComBatch {
    fn key(&self) -> BatchKey {
        BatchKey {
            source: SourceKind::ChessCom,
            id: format!("{}:{}", self.username, self.month),
        }
    }

    fn is_sealed(&self) -> bool {
        self.sealed
    }
}

impl GameSource for ChessCom {
    type Batch = ChessComBatch;

    fn batches(&self, query: &SourceQuery) -> Result<Vec<ChessComBatch>> {
        let username = query.username.to_lowercase();
        let url = format!("{API_BASE}/player/{username}/games/archives");
        let response: ArchivesResponse = self.get_json(&url)?;
        batches_from_archives(&username, response.archives, query.since)
    }

    fn fetch(&self, batch: &ChessComBatch) -> Result<Vec<Game>> {
        let response: MonthResponse = self.get_json(&batch.url)?;
        games_from_month(response)
    }
}

#[derive(Deserialize)]
struct ArchivesResponse {
    archives: Vec<String>,
}

#[derive(Deserialize)]
struct MonthResponse {
    games: Vec<MonthGame>,
}

#[derive(Deserialize)]
struct MonthGame {
    uuid: String,
    pgn: Option<String>,
}

/// Archives are bucketed by the month a game ended, so only the newest one can still gain games.
fn batches_from_archives(
    username: &str,
    urls: Vec<String>,
    since: Option<YearMonth>,
) -> Result<Vec<ChessComBatch>> {
    let mut batches = Vec::with_capacity(urls.len());
    for url in urls {
        let month = parse_archive_month(&url)?;
        if since.is_some_and(|since| month < since) {
            continue;
        }
        batches.push(ChessComBatch {
            username: username.to_string(),
            month,
            url,
            sealed: true,
        });
    }
    batches.sort_by_key(|b| b.month);
    if let Some(latest) = batches.last_mut() {
        latest.sealed = false;
    }
    Ok(batches)
}

/// Parses the month from an archive url like `.../games/2024/03`.
fn parse_archive_month(url: &str) -> Result<YearMonth> {
    let bad_url = || anyhow!("unexpected archive url: {url}");
    let mut segments = url.trim_end_matches('/').rsplit('/');
    let month: u8 = segments
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(bad_url)?;
    let year: u16 = segments
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(bad_url)?;
    if !(1..=12).contains(&month) {
        return Err(bad_url());
    }
    Ok(YearMonth { year, month })
}

/// Games without a pgn are skipped. A pgn that doesn't parse fails the month.
fn games_from_month(response: MonthResponse) -> Result<Vec<Game>> {
    response
        .games
        .into_iter()
        .filter_map(|g| Some((g.uuid, g.pgn?)))
        .map(|(uuid, pgn)| {
            let pgn = pgn
                .parse()
                .with_context(|| format!("chess.com game {uuid}"))?;
            Ok(Game {
                id: GameId {
                    source: SourceKind::ChessCom,
                    external_id: uuid,
                },
                pgn,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::MemoryStore;
    use crate::app::game_reader::GameReader;
    use crate::app::sync::sync;

    fn archives_url(username: &str) -> String {
        format!("{API_BASE}/player/{username}/games/archives")
    }

    fn archives_json(archives: &[&str]) -> String {
        serde_json::json!({ "archives": archives }).to_string()
    }

    fn month_json(uuid: &str) -> String {
        serde_json::json!({ "games": [{ "uuid": uuid, "pgn": "1. e4" }] }).to_string()
    }

    fn query(username: &str, since: Option<YearMonth>) -> SourceQuery {
        SourceQuery {
            username: username.to_string(),
            since,
        }
    }

    fn ym(year: u16, month: u8) -> YearMonth {
        YearMonth { year, month }
    }

    /// `expected` has one entry per batch, e.g. `["2023-11", "2024-03 open"]`.
    #[track_caller]
    fn check_batches(
        username: &str,
        since: Option<YearMonth>,
        archives: &[&str],
        expected: &[&str],
    ) {
        let source = ChessCom::fake([(
            archives_url(&username.to_lowercase()),
            archives_json(archives),
        )]);
        let batches = source.batches(&query(username, since)).unwrap();
        let actual: Vec<String> = batches
            .iter()
            .map(|b| match b.is_sealed() {
                true => b.month.to_string(),
                false => format!("{} open", b.month),
            })
            .collect();
        assert_eq!(actual, expected);
    }

    #[track_caller]
    fn check_bad_archive(archive: &str) {
        let source = ChessCom::fake([(archives_url("someone"), archives_json(&[archive]))]);
        let result = source.batches(&query("someone", None));
        let err = result.expect_err(archive);
        assert!(
            err.to_string().contains("unexpected archive url"),
            "{archive:?}: {err}"
        );
    }

    /// Fetches the one month whose response is `month_json`.
    fn fetch_month(month_json: &str) -> Result<Vec<Game>> {
        let archive = format!("{API_BASE}/player/someone/games/2024/03");
        let source = ChessCom::fake([
            (archives_url("someone"), archives_json(&[&archive])),
            (archive, month_json.to_string()),
        ]);
        let batches = source.batches(&query("someone", None)).unwrap();
        source.fetch(&batches[0])
    }

    /// `month_json` is chess.com's month response; `expected` is `(uuid, pgn)` per game.
    #[track_caller]
    fn check_games(month_json: &str, expected: &[(&str, &str)]) {
        let games = fetch_month(month_json).unwrap();
        let actual: Vec<(&str, &str)> = games
            .iter()
            .map(|g| (g.id.external_id.as_str(), g.pgn.as_str()))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn archive_month_comes_from_url() {
        check_batches(
            "someone",
            None,
            &["https://x/games/2024/03"],
            &["2024-03 open"],
        );
        check_batches(
            "someone",
            None,
            &["https://x/games/2019/12/"],
            &["2019-12 open"],
        );
    }

    #[test]
    fn malformed_archive_url_is_an_error() {
        check_bad_archive("");
        check_bad_archive("https://x/games");
        check_bad_archive("https://x/games/2024/ab");
        check_bad_archive("https://x/games/2024/13");
    }

    #[test]
    fn only_latest_archive_is_unsealed() {
        check_batches(
            "someone",
            None,
            &[
                "https://x/games/2024/03",
                "https://x/games/2023/11",
                "https://x/games/2024/01",
            ],
            &["2023-11", "2024-01", "2024-03 open"],
        );
        check_batches("someone", None, &[], &[]);
    }

    #[test]
    fn since_drops_earlier_archives() {
        check_batches(
            "someone",
            Some(ym(2024, 1)),
            &[
                "https://x/games/2023/12",
                "https://x/games/2024/01",
                "https://x/games/2024/02",
            ],
            &["2024-01", "2024-02 open"],
        );
    }

    #[test]
    fn username_is_case_insensitive() {
        check_batches(
            "SomeOne",
            None,
            &["https://x/games/2024/03"],
            &["2024-03 open"],
        );
    }

    #[test]
    fn reads_games_and_skips_those_without_pgn() {
        check_games(
            r#"{"games": [
                {"uuid": "u1", "pgn": "1. e4 e5", "url": "https://www.chess.com/game/live/1", "rated": true},
                {"uuid": "u2"}
            ]}"#,
            &[("u1", "1. e4 e5")],
        );
    }

    #[test]
    fn a_pgn_that_does_not_parse_fails_the_fetch() {
        let err = fetch_month(
            r#"{"games": [
                {"uuid": "u1", "pgn": "1. e4 e5"},
                {"uuid": "u2", "pgn": "1. e4 { never closed"}
            ]}"#,
        )
        .expect_err("u2 doesn't parse");
        let err = format!("{err:#}");
        assert!(err.contains("chess.com game u2"), "{err}");
        assert!(err.contains("unterminated comment"), "{err}");
    }

    #[test]
    fn syncing_two_users_keeps_both() {
        let erik = format!("{API_BASE}/player/erik/games/2024/03");
        let hikaru = format!("{API_BASE}/player/hikaru/games/2024/03");
        let source = ChessCom::fake([
            (archives_url("erik"), archives_json(&[&erik])),
            (archives_url("hikaru"), archives_json(&[&hikaru])),
            (erik, month_json("e1")),
            (hikaru, month_json("h1")),
        ]);
        let mut store = MemoryStore::new();

        sync(&source, &query("erik", None), &mut store).unwrap();
        sync(&source, &query("hikaru", None), &mut store).unwrap();

        let mut ids: Vec<_> = store
            .games()
            .unwrap()
            .into_iter()
            .map(|g| g.id.external_id)
            .collect();
        ids.sort();
        assert_eq!(ids, ["e1", "h1"]);
    }

    #[test]
    #[ignore = "hits the live chess.com API"]
    fn live_lists_and_fetches_archives() {
        let source = ChessCom::new("rightchess-dev");
        let query = SourceQuery {
            username: "erik".to_string(),
            since: None,
        };

        let batches = source.batches(&query).unwrap();
        assert!(!batches.last().unwrap().is_sealed());

        let games = source.fetch(&batches[0]).unwrap();
        assert!(!games.is_empty());
        assert!(games.iter().all(|g| !g.pgn.as_str().is_empty()));
        for game in &games {
            if let Err(err) = game.pgn.plies() {
                panic!("game {}: {err}", game.id.external_id);
            }
        }
    }
}
