use std::collections::HashSet;

use anyhow::{Context, Result, ensure};

use crate::app::game_reader::GameReader;
use crate::chess::{Fen, NormalizedFen};
use crate::domain::{Eval, Game, GameId};

/// Must name every setting that can change the evals, e.g. "stockfish 17.1 nodes=1000000".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvaluatorId(pub String);

pub trait Evaluator {
    fn id(&self) -> EvaluatorId;
    /// One eval per position, in order.
    fn evaluate(&mut self, positions: &[Fen]) -> Result<Vec<Eval>>;
}

pub trait EvalStore {
    fn eval(&self, evaluator: &EvaluatorId, position: &NormalizedFen) -> Result<Option<Eval>>;
    /// Saves every eval or none. Replaces evals already stored under the same key.
    fn save_evals(
        &mut self,
        evaluator: &EvaluatorId,
        evals: Vec<(NormalizedFen, Eval)>,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvalReport {
    pub num_games: usize,
    pub num_positions: usize,
    pub num_evaluated: usize,
    pub num_cached: usize,
    pub failed: Vec<FailedGame>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedGame {
    pub game: GameId,
    pub reason: String,
}

/// The evaluator gets the position's key with counters `0 1`, never `fen` itself.
pub fn evaluate_position(
    fen: &Fen,
    evaluator: &mut impl Evaluator,
    store: &mut impl EvalStore,
) -> Result<Eval> {
    if let Some(eval) = store
        .eval(&evaluator.id(), &fen.into())
        .with_context(|| format!("reading the eval of {fen}"))?
    {
        return Ok(eval);
    }
    let evals =
        evaluate_keys(evaluator, vec![fen.into()]).with_context(|| format!("evaluating {fen}"))?;
    let eval = evals[0].1;
    store
        .save_evals(&evaluator.id(), evals)
        .with_context(|| format!("saving the eval of {fen}"))?;
    Ok(eval)
}

/// Fails only if the games can't be read. A game that fails saves nothing and goes in `failed`.
pub fn evaluate_games(
    store: &mut (impl GameReader + EvalStore),
    evaluator: &mut impl Evaluator,
) -> Result<EvalReport> {
    let id = evaluator.id();
    let mut done = HashSet::new();
    let (succeeded, failed): (Vec<_>, Vec<_>) = store
        .games()
        .context("reading games")?
        .into_iter()
        .map(|game| {
            evaluate_game(&game, &id, &mut done, evaluator, store).map_err(|err| FailedGame {
                game: game.id,
                reason: format!("{err:#}"),
            })
        })
        .partition(Result::is_ok);
    let counts: Vec<GameCounts> = succeeded.into_iter().flatten().collect();
    let num_evaluated = counts.iter().map(|c| c.evaluated).sum();
    let num_cached = counts.iter().map(|c| c.cached).sum();
    Ok(EvalReport {
        num_games: counts.len(),
        num_positions: num_evaluated + num_cached,
        num_evaluated,
        num_cached,
        failed: failed.into_iter().filter_map(Result::err).collect(),
    })
}

struct GameCounts {
    evaluated: usize,
    cached: usize,
}

/// Adds the game's keys to `done` only if it succeeds, so later games retry a failed game's
/// positions.
fn evaluate_game(
    game: &Game,
    id: &EvaluatorId,
    done: &mut HashSet<NormalizedFen>,
    evaluator: &mut impl Evaluator,
    store: &mut impl EvalStore,
) -> Result<GameCounts> {
    let fens = game.pgn.fens().collect::<Result<Vec<Fen>>>()?;
    let mut seen = HashSet::new();
    let keys: Vec<NormalizedFen> = fens
        .iter()
        .map(NormalizedFen::from)
        .filter(|key| !done.contains(key) && seen.insert(key.clone()))
        .collect();
    let missing = keys
        .iter()
        .filter_map(|key| match store.eval(id, key) {
            Ok(Some(_)) => None,
            Ok(None) => Some(Ok(key.clone())),
            Err(err) => Some(Err(err.context(format!("reading the eval of {key}")))),
        })
        .collect::<Result<Vec<_>>>()?;
    let counts = GameCounts {
        evaluated: missing.len(),
        cached: keys.len() - missing.len(),
    };
    if !missing.is_empty() {
        let evals = evaluate_keys(evaluator, missing).context("evaluating")?;
        store.save_evals(id, evals).context("saving the evals")?;
    }
    done.extend(keys);
    Ok(counts)
}

fn evaluate_keys(
    evaluator: &mut impl Evaluator,
    keys: Vec<NormalizedFen>,
) -> Result<Vec<(NormalizedFen, Eval)>> {
    let fens: Vec<Fen> = keys.iter().map(Fen::from).collect();
    let evals = evaluator.evaluate(&fens)?;
    ensure!(
        evals.len() == keys.len(),
        "the evaluator gave {} evals for {} positions",
        evals.len(),
        keys.len()
    );
    Ok(keys.into_iter().zip(evals).collect())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::hash::{DefaultHasher, Hash, Hasher};

    use anyhow::{Result, bail};

    use super::*;
    use crate::adapters::MemoryStore;
    use crate::app::sync::{BatchKey, SyncStore};
    use crate::chess::Pgn;
    use crate::domain::{Game, Score, SourceKind};

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    /// No en passant square: no black pawn can take.
    const AFTER_E4: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";

    #[derive(Default)]
    struct FakeEvaluator {
        id: &'static str,
        calls: Vec<Vec<String>>,
        fail_on: Option<usize>,
        short: bool,
        long: bool,
    }

    impl FakeEvaluator {
        fn new(id: &'static str) -> Self {
            Self {
                id,
                ..Self::default()
            }
        }

        fn asked(&self) -> Vec<String> {
            self.calls.concat()
        }
    }

    impl Evaluator for FakeEvaluator {
        fn id(&self) -> EvaluatorId {
            EvaluatorId(self.id.to_string())
        }

        fn evaluate(&mut self, positions: &[Fen]) -> Result<Vec<Eval>> {
            self.calls
                .push(positions.iter().map(|fen| fen.to_string()).collect());
            if self.fail_on == Some(self.calls.len()) {
                bail!("engine crashed");
            }
            let mut evals: Vec<Eval> = positions
                .iter()
                .map(|fen| fake_eval(self.id, fen.as_str()))
                .collect();
            if self.short {
                evals.pop();
            }
            if self.long {
                evals.push(fake_eval(self.id, "extra"));
            }
            Ok(evals)
        }
    }

    fn fake_eval(id: &str, fen: &str) -> Eval {
        let mut hasher = DefaultHasher::new();
        (id, fen).hash(&mut hasher);
        Eval::from_white(Score::Cp(hasher.finish() as i32))
    }

    fn miscounting() -> [FakeEvaluator; 2] {
        [
            FakeEvaluator {
                short: true,
                ..FakeEvaluator::new("sf")
            },
            FakeEvaluator {
                long: true,
                ..FakeEvaluator::new("sf")
            },
        ]
    }

    #[track_caller]
    fn fen(text: &str) -> Fen {
        text.parse().unwrap_or_else(|err| panic!("{text:?}: {err}"))
    }

    fn as_asked(fen: &Fen) -> String {
        Fen::from(&NormalizedFen::from(fen)).to_string()
    }

    fn positions(movetexts: &[&str]) -> BTreeSet<String> {
        movetexts
            .iter()
            .flat_map(|text| {
                let pgn: Pgn = text.parse().unwrap();
                pgn.fens()
                    .map(|fen| as_asked(&fen.unwrap()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn game_id(id: &str) -> GameId {
        GameId {
            source: SourceKind::ChessCom,
            external_id: id.to_string(),
        }
    }

    fn store_with(games: &[(&str, &str)]) -> MemoryStore {
        let mut store = MemoryStore::new();
        let games = games
            .iter()
            .map(|(id, pgn)| Game {
                id: game_id(id),
                pgn: pgn.parse().unwrap(),
            })
            .collect();
        let key = BatchKey {
            source: SourceKind::ChessCom,
            id: "batch".to_string(),
        };
        store.save_batch(key, true, games).unwrap();
        store
    }

    fn stored(store: &MemoryStore, id: &str, asked: &str) -> Option<Eval> {
        let key = NormalizedFen::from(&fen(asked));
        store.eval(&EvaluatorId(id.to_string()), &key).unwrap()
    }

    mod position {
        use super::*;

        const ITALIAN: &str =
            "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/3P1N2/PPP2PPP/RNBQK2R w KQkq - 1 5";
        const ITALIAN_LATER: &str =
            "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/3P1N2/PPP2PPP/RNBQK2R w KQkq - 7 12";

        #[test]
        fn a_miss_is_evaluated_as_its_key_saved_and_returned() {
            let mut store = MemoryStore::new();
            let mut evaluator = FakeEvaluator::new("sf");
            let with_en_passant = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
            let eval =
                evaluate_position(&fen(with_en_passant), &mut evaluator, &mut store).unwrap();
            assert_eq!(evaluator.calls, [vec![AFTER_E4.to_string()]]);
            assert_eq!(eval, fake_eval("sf", AFTER_E4));
            assert_eq!(stored(&store, "sf", AFTER_E4), Some(eval));
        }

        #[test]
        fn a_hit_returns_the_stored_eval_without_evaluating() {
            let mut store = MemoryStore::new();
            let mut evaluator = FakeEvaluator::new("sf");
            let first = evaluate_position(&fen(START), &mut evaluator, &mut store).unwrap();
            let second = evaluate_position(&fen(START), &mut evaluator, &mut store).unwrap();
            assert_eq!(second, first);
            assert_eq!(evaluator.calls.len(), 1);
        }

        #[test]
        fn fens_differing_in_move_counters_share_an_eval() {
            let mut store = MemoryStore::new();
            let mut evaluator = FakeEvaluator::new("sf");
            let first = evaluate_position(&fen(ITALIAN), &mut evaluator, &mut store).unwrap();
            let later = evaluate_position(&fen(ITALIAN_LATER), &mut evaluator, &mut store).unwrap();
            assert_eq!(later, first);
            assert_eq!(evaluator.asked(), [as_asked(&fen(ITALIAN))]);
        }

        #[test]
        fn each_evaluator_gets_its_own_eval() {
            let mut store = MemoryStore::new();
            let mut a = FakeEvaluator::new("a");
            let mut b = FakeEvaluator::new("b");
            let from_a = evaluate_position(&fen(START), &mut a, &mut store).unwrap();
            let from_b = evaluate_position(&fen(START), &mut b, &mut store).unwrap();
            assert_eq!((a.calls.len(), b.calls.len()), (1, 1));
            assert_ne!(from_a, from_b);
            assert_eq!(
                evaluate_position(&fen(START), &mut a, &mut store).unwrap(),
                from_a
            );
            assert_eq!(a.calls.len(), 1);
        }

        #[test]
        fn evaluator_failure_saves_nothing() {
            let mut store = MemoryStore::new();
            let mut evaluator = FakeEvaluator {
                fail_on: Some(1),
                ..FakeEvaluator::new("sf")
            };
            let err = evaluate_position(&fen(START), &mut evaluator, &mut store).unwrap_err();
            let key = NormalizedFen::from(&fen(START)).to_string();
            let message = format!("{err:#}");
            assert!(message.contains("engine crashed"), "{message}");
            assert!(message.contains(&key), "{message}");
            assert_eq!(stored(&store, "sf", START), None);
        }

        #[test]
        fn a_wrong_eval_count_is_an_error() {
            for mut evaluator in miscounting() {
                let miscount = if evaluator.short { "short" } else { "long" };
                let mut store = MemoryStore::new();
                let err = evaluate_position(&fen(START), &mut evaluator, &mut store).unwrap_err();
                assert!(
                    format!("{err:#}").contains("evals for"),
                    "{miscount}: {err:#}"
                );
                assert_eq!(stored(&store, "sf", START), None, "{miscount}");
            }
        }
    }

    mod games {
        use super::*;

        /// Transposes into `QUEENSIDE_FIRST`; together they have 8 distinct positions.
        const KINGSIDE_FIRST: &str = "1. Nf3 Nf6 2. Nc3 Nc6";
        const QUEENSIDE_FIRST: &str = "1. Nc3 Nc6 2. Nf3 Nf6";
        const UNPLAYABLE: &str = "1. e4 e5 2. Ke3";
        const CHESS960: &str = "[Variant \"Chess960\"]\n\n1. e4";

        #[test]
        fn each_distinct_position_is_evaluated_once() {
            let games = [KINGSIDE_FIRST, QUEENSIDE_FIRST, "1. Nf3 d5"];
            let mut store = store_with(&[("a", games[0]), ("b", games[1]), ("c", games[2])]);
            let mut evaluator = FakeEvaluator::new("sf");
            evaluate_games(&mut store, &mut evaluator).unwrap();
            let asked = evaluator.asked();
            let distinct: BTreeSet<String> = asked.iter().cloned().collect();
            assert_eq!(asked.len(), distinct.len(), "asked twice: {asked:#?}");
            assert_eq!(distinct, positions(&games));
            assert_eq!(distinct.len(), 9);
            for fen in &distinct {
                assert_eq!(
                    stored(&store, "sf", fen),
                    Some(fake_eval("sf", fen)),
                    "{fen}"
                );
            }
        }

        #[test]
        fn a_position_repeated_in_one_game_is_evaluated_once() {
            let mut store = store_with(&[("a", "1. Nf3 Nf6 2. Ng1 Ng8 3. Nf3 Nf6")]);
            let mut evaluator = FakeEvaluator::new("sf");
            let report = evaluate_games(&mut store, &mut evaluator).unwrap();
            assert_eq!(evaluator.asked().len(), 4, "{:#?}", evaluator.calls);
            assert_eq!((report.num_positions, report.num_evaluated), (4, 4));
        }

        #[test]
        fn a_game_without_moves_has_one_position() {
            let mut store = store_with(&[("a", "")]);
            let mut evaluator = FakeEvaluator::new("sf");
            let report = evaluate_games(&mut store, &mut evaluator).unwrap();
            assert_eq!(evaluator.calls, [vec![START.to_string()]]);
            assert_eq!((report.num_games, report.num_positions), (1, 1));
        }

        #[test]
        fn an_empty_store_evaluates_nothing() {
            let mut store = MemoryStore::new();
            let mut evaluator = FakeEvaluator::new("sf");
            let report = evaluate_games(&mut store, &mut evaluator).unwrap();
            assert_eq!(report, EvalReport::default());
            assert!(evaluator.calls.is_empty());
        }

        #[test]
        fn a_second_run_evaluates_nothing() {
            let mut store = store_with(&[("a", KINGSIDE_FIRST), ("b", QUEENSIDE_FIRST)]);
            let first = evaluate_games(&mut store, &mut FakeEvaluator::new("sf")).unwrap();
            assert_eq!(
                first,
                EvalReport {
                    num_games: 2,
                    num_positions: 8,
                    num_evaluated: 8,
                    num_cached: 0,
                    failed: Vec::new(),
                }
            );
            let mut evaluator = FakeEvaluator::new("sf");
            let second = evaluate_games(&mut store, &mut evaluator).unwrap();
            assert!(evaluator.calls.is_empty(), "{:#?}", evaluator.calls);
            assert_eq!(
                second,
                EvalReport {
                    num_evaluated: 0,
                    num_cached: 8,
                    ..first
                }
            );
        }

        /// Whichever game comes first, the other has nothing left to evaluate.
        #[test]
        fn a_fully_cached_game_does_not_call_the_evaluator() {
            let mut store = store_with(&[("a", "1. e4 e5"), ("b", "1. e4")]);
            let mut evaluator = FakeEvaluator::new("sf");
            let report = evaluate_games(&mut store, &mut evaluator).unwrap();
            assert!(
                evaluator.calls.iter().all(|call| !call.is_empty()),
                "{:#?}",
                evaluator.calls
            );
            assert_eq!(evaluator.asked().len(), 3);
            assert_eq!((report.num_games, report.num_positions), (2, 3));
        }

        #[test]
        fn positions_evaluated_singly_count_as_cached() {
            let mut store = store_with(&[("a", "1. e4")]);
            let mut evaluator = FakeEvaluator::new("sf");
            evaluate_position(&fen(START), &mut evaluator, &mut store).unwrap();
            let report = evaluate_games(&mut store, &mut evaluator).unwrap();
            assert_eq!(
                evaluator.calls,
                [vec![START.to_string()], vec![AFTER_E4.to_string()]]
            );
            assert_eq!((report.num_evaluated, report.num_cached), (1, 1));
        }

        #[test]
        fn another_evaluator_evaluates_every_position_again() {
            let mut store = store_with(&[("a", KINGSIDE_FIRST), ("b", QUEENSIDE_FIRST)]);
            evaluate_games(&mut store, &mut FakeEvaluator::new("a")).unwrap();
            let mut b = FakeEvaluator::new("b");
            let report = evaluate_games(&mut store, &mut b).unwrap();
            assert_eq!(b.asked().len(), 8);
            assert_eq!((report.num_evaluated, report.num_cached), (8, 0));
        }

        #[test]
        fn unplayable_games_fail_and_the_rest_are_evaluated() {
            let mut store = store_with(&[
                ("good", KINGSIDE_FIRST),
                ("illegal", UNPLAYABLE),
                ("960", CHESS960),
            ]);
            let mut evaluator = FakeEvaluator::new("sf");
            let report = evaluate_games(&mut store, &mut evaluator).unwrap();

            let mut failed = report.failed.clone();
            failed.sort_by(|a, b| a.game.external_id.cmp(&b.game.external_id));
            let ids: Vec<&str> = failed.iter().map(|f| f.game.external_id.as_str()).collect();
            assert_eq!(ids, ["960", "illegal"]);
            assert!(
                failed[0].reason.contains("unsupported variant Chess960"),
                "{}",
                failed[0].reason
            );
            assert!(failed[1].reason.contains("Ke3"), "{}", failed[1].reason);

            let asked: BTreeSet<String> = evaluator.asked().into_iter().collect();
            assert_eq!(asked, positions(&[KINGSIDE_FIRST]));
            assert_eq!((report.num_games, report.num_positions), (1, 5));
        }

        /// Games run in the store's order, so which one failed is read from the calls.
        #[test]
        fn an_evaluator_failure_fails_only_that_game() {
            let mut store = store_with(&[("a", "1. e4"), ("b", "1. d4")]);
            let mut evaluator = FakeEvaluator {
                fail_on: Some(1),
                ..FakeEvaluator::new("sf")
            };
            let report = evaluate_games(&mut store, &mut evaluator).unwrap();
            let [first, second] = &evaluator.calls[..] else {
                panic!("{:#?}", evaluator.calls);
            };
            // Only game "a" reaches e4 (`4P3`).
            let failed = if first.iter().any(|fen| fen.contains("4P3")) {
                "a"
            } else {
                "b"
            };
            let [failure] = &report.failed[..] else {
                panic!("{:#?}", report.failed);
            };
            assert_eq!(failure.game, game_id(failed));
            assert!(
                failure.reason.contains("engine crashed"),
                "{}",
                failure.reason
            );
            assert_eq!(stored(&store, "sf", &first[1]), None);
            for fen in second {
                assert_eq!(
                    stored(&store, "sf", fen),
                    Some(fake_eval("sf", fen)),
                    "{fen}"
                );
            }
            assert_eq!(
                report,
                EvalReport {
                    num_games: 1,
                    num_positions: 2,
                    num_evaluated: 2,
                    num_cached: 0,
                    failed: report.failed.clone(),
                }
            );
        }

        #[test]
        fn a_failed_games_positions_are_retried_by_later_games() {
            let mut store = store_with(&[("a", "1. e4"), ("b", "1. d4")]);
            let mut evaluator = FakeEvaluator {
                fail_on: Some(1),
                ..FakeEvaluator::new("sf")
            };
            evaluate_games(&mut store, &mut evaluator).unwrap();
            assert!(
                evaluator
                    .calls
                    .iter()
                    .all(|call| call.iter().any(|fen| fen == START)),
                "{:#?}",
                evaluator.calls
            );
            assert_eq!(stored(&store, "sf", START), Some(fake_eval("sf", START)));
        }

        #[test]
        fn a_wrong_eval_count_fails_that_game_and_saves_nothing() {
            for mut evaluator in miscounting() {
                let miscount = if evaluator.short { "short" } else { "long" };
                let mut store = store_with(&[("a", "1. e4")]);
                let report = evaluate_games(&mut store, &mut evaluator).unwrap();
                let [failure] = &report.failed[..] else {
                    panic!("{miscount}: {:#?}", report.failed);
                };
                assert!(
                    failure.reason.contains("evals for"),
                    "{miscount}: {}",
                    failure.reason
                );
                assert_eq!(
                    (report.num_games, report.num_evaluated),
                    (0, 0),
                    "{miscount}"
                );
                for fen in &evaluator.calls[0] {
                    assert_eq!(stored(&store, "sf", fen), None, "{miscount}: {fen}");
                }
            }
        }
    }
}
