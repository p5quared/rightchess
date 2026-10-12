//! Facts about one game, computed from its stored record on demand and never saved.

use anyhow::Result;

use crate::domain::Game;

/// Thirty [plies](crate::Ply) is fifteen moves each.
pub const EARLY_PLIES: usize = 30;

/// As the [`TimeControl` tag](crate::Pgn) writes it, e.g. `180+2` for three minutes each plus
/// two seconds a move, or chess.com's `1/259200` for three days a move.
#[must_use]
pub fn time_control(game: &Game) -> Option<String> {
    game.pgn.tag("TimeControl")
}

/// Whether the game has fewer than [`EARLY_PLIES`] [plies](crate::Ply), however it ended.
///
/// # Errors
///
/// Fails on a move that isn't [SAN](crate::Ply).
pub fn did_end_early(game: &Game) -> Result<bool> {
    Ok(game.pgn.plies()?.len() < EARLY_PLIES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{GameId, SourceKind};

    fn game(pgn: &str) -> Game {
        Game {
            id: GameId {
                source: SourceKind::ChessCom,
                external_id: "id".to_string(),
            },
            pgn: pgn.parse().unwrap_or_else(|err| panic!("{pgn:?}: {err}")),
        }
    }

    /// `plies` knight moves out and back, numbered.
    fn shuffle(plies: usize) -> String {
        ["Nf3", "Nf6", "Ng1", "Ng8"]
            .iter()
            .cycle()
            .take(plies)
            .enumerate()
            .map(|(i, san)| {
                if i % 2 == 0 {
                    format!("{}. {san}", i / 2 + 1)
                } else {
                    (*san).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    mod time_control {
        use super::*;

        #[test]
        fn reads_the_tag() {
            for value in ["180+2", "600", "1/259200", "-", "?"] {
                let pgn = format!("[TimeControl \"{value}\"]\n\n1. e4 *");
                assert_eq!(time_control(&game(&pgn)).as_deref(), Some(value));
            }
        }

        #[test]
        fn is_none_without_the_tag() {
            assert_eq!(time_control(&game("[Event \"x\"]\n\n1. e4 *")), None);
            assert_eq!(time_control(&game("")), None);
        }
    }

    mod ended_early {
        use super::*;

        #[track_caller]
        fn check(pgn: &str, expected: bool) {
            let early = did_end_early(&game(pgn)).unwrap_or_else(|err| panic!("{pgn:?}: {err}"));
            assert_eq!(early, expected, "{pgn:?}");
        }

        #[test]
        fn fewer_than_early_plies_is_early() {
            for plies in [0, 1, 2, EARLY_PLIES - 1] {
                check(&shuffle(plies), true);
            }
        }

        #[test]
        fn early_plies_or_more_is_not_early() {
            for plies in [EARLY_PLIES, EARLY_PLIES + 1, 3 * EARLY_PLIES] {
                check(&shuffle(plies), false);
            }
        }

        #[test]
        fn only_the_mainline_counts() {
            let long = shuffle(2 * EARLY_PLIES);
            check(&format!("1. e4 ({long}) 1... e5 {{ {long} }} 1-0"), true);
            check(
                &format!(
                    "[Event \"x\"]\n\n{} {{[%clk 0:02:59.9]}} 1-0",
                    shuffle(EARLY_PLIES)
                ),
                false,
            );
        }

        /// [Plies](crate::Ply) are counted, not played, so variants and illegal moves still count.
        #[test]
        fn moves_are_not_played() {
            check("[Variant \"Chess960\"]\n\n1. e4 e5", true);
            check("1. e4 e5 2. Ke3", true);
        }

        #[test]
        fn a_move_that_is_not_san_is_an_error() {
            let err = did_end_early(&game("1. e4 Nz9")).unwrap_err();
            assert!(err.to_string().contains("Nz9"), "{err}");
        }
    }
}
