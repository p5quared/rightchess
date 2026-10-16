use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::app::evaluate::{Evaluator, EvaluatorId};
use crate::chess::{Color, Fen};
use crate::domain::{Eval, Score};

const HASH_MB: u32 = 16;

/// A Stockfish process spoken to over UCI (Universal Chess Interface), the text protocol chess
/// engines read on stdin and answer on stdout.
///
/// Evals are deterministic for a given binary: one search thread, a fixed hash table cleared
/// before every position, and a search stopped after a fixed number of nodes (positions
/// visited) rather than after a time limit.
pub struct Stockfish {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    id: EvaluatorId,
    nodes: u64,
}

impl Stockfish {
    /// Starts the engine at `program`, a path or a name on `PATH`.
    ///
    /// # Errors
    ///
    /// Fails if the program can't be started or doesn't complete the UCI handshake.
    pub fn spawn(program: impl AsRef<OsStr>, nodes: u64) -> Result<Stockfish> {
        let program = program.as_ref();
        let mut process = Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("starting {}", program.display()))?;
        let stdin = process.stdin.take().context("the engine has no stdin")?;
        let stdout = process.stdout.take().context("the engine has no stdout")?;
        let mut engine = Stockfish {
            process,
            stdin,
            stdout: BufReader::new(stdout),
            id: EvaluatorId(String::new()),
            nodes,
        };
        engine.send("uci")?;
        let mut name = None;
        engine.read_until("uciok", |line| {
            if let Some(rest) = line.strip_prefix("id name ") {
                name = Some(rest.to_string());
            }
        })?;
        let name = name.context("the engine didn't send its name")?;
        engine.send("setoption name Threads value 1")?;
        engine.send(&format!("setoption name Hash value {HASH_MB}"))?;
        engine.ready()?;
        engine.id = EvaluatorId(format!("{name} nodes={nodes} threads=1 hash={HASH_MB}"));
        Ok(engine)
    }

    fn evaluate_one(&mut self, fen: &Fen) -> Result<Eval> {
        self.send("ucinewgame")?;
        self.send(&format!("position fen {fen}"))?;
        self.ready()?;
        self.send(&format!("go nodes {}", self.nodes))?;
        let mut last = None;
        self.read_until("bestmove", |line| {
            if let Some(score) = score(line) {
                last = Some(score);
            }
        })?;
        let score = last.context("the engine sent no score")?;
        Ok(match fen.side_to_move() {
            Color::White => Eval::from_white(score),
            Color::Black => Eval::from_black(score),
        })
    }

    fn send(&mut self, command: &str) -> Result<()> {
        writeln!(self.stdin, "{command}")
            .and_then(|()| self.stdin.flush())
            .with_context(|| format!("sending {command:?}"))
    }

    /// Waits until the engine has applied every command sent so far.
    fn ready(&mut self) -> Result<()> {
        self.send("isready")?;
        self.read_until("readyok", |_| {})
    }

    /// Reads lines through the first whose first word is `last`, passing each earlier line to
    /// `each`.
    fn read_until(&mut self, last: &str, mut each: impl FnMut(&str)) -> Result<()> {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self
                .stdout
                .read_line(&mut line)
                .with_context(|| format!("reading the engine's output before {last:?}"))?;
            if read == 0 {
                bail!("the engine exited before {last:?}");
            }
            let line = line.trim_end();
            if line.split_whitespace().next() == Some(last) {
                return Ok(());
            }
            each(line);
        }
    }
}

impl Evaluator for Stockfish {
    fn id(&self) -> EvaluatorId {
        self.id.clone()
    }

    fn evaluate(&mut self, positions: &[Fen]) -> Result<Vec<Eval>> {
        positions
            .iter()
            .map(|fen| {
                self.evaluate_one(fen)
                    .with_context(|| format!("evaluating {fen}"))
            })
            .collect()
    }
}

impl Drop for Stockfish {
    fn drop(&mut self) {
        let _quit = self.send("quit");
        let _exit = self.process.wait();
    }
}

/// The score on a UCI `info` line, from the side to move. Bound lines, which only limit the
/// score from one side, and lines without a score are `None`.
fn score(line: &str) -> Option<Score> {
    let mut words = line.split_whitespace();
    if words.next() != Some("info") || line.starts_with("info string") {
        return None;
    }
    let mut words = words.skip_while(|word| *word != "score").skip(1);
    let (kind, value) = (words.next()?, words.next()?);
    if matches!(words.next(), Some("lowerbound" | "upperbound")) {
        return None;
    }
    match (kind, value.strip_prefix('-')) {
        ("cp", _) => value.parse().ok().map(Score::Cp),
        ("mate", Some(moves)) => moves.parse().ok().map(Score::Mated),
        ("mate", None) => match value.parse().ok()? {
            0 => Some(Score::Mated(0)),
            moves => Some(Score::Mates(moves)),
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::FileFailurePersistence;

    use super::*;

    #[track_caller]
    fn check(line: &str, expected: Option<Score>) {
        assert_eq!(score(line), expected, "{line:?}");
    }

    #[test]
    fn reads_centipawns() {
        check(
            "info depth 20 seldepth 31 multipv 1 score cp 35 nodes 200000 nps 1500000 pv e2e4 e7e5",
            Some(Score::Cp(35)),
        );
        check(
            "info depth 3 score cp -120 nodes 90 pv d8h4",
            Some(Score::Cp(-120)),
        );
    }

    #[test]
    fn reads_mates_for_and_against_the_side_to_move() {
        check("info depth 9 score mate 3 pv a1a8", Some(Score::Mates(3)));
        check("info depth 9 score mate -2 pv h7h8", Some(Score::Mated(2)));
    }

    #[test]
    fn a_finished_game_is_scored_at_depth_zero() {
        check("info depth 0 score mate 0", Some(Score::Mated(0)));
        check("info depth 0 score cp 0", Some(Score::Cp(0)));
    }

    #[test]
    fn bounds_are_not_scores() {
        check("info depth 18 score cp 50 lowerbound nodes 1000", None);
        check("info depth 18 score cp 40 upperbound nodes 1000", None);
    }

    #[test]
    fn other_lines_have_no_score() {
        for line in [
            "info string NNUE evaluation using nn-1111cefa1111.nnue enabled",
            "info depth 5 currmove e2e4 currmovenumber 1",
            "bestmove e2e4 ponder e7e5",
            "readyok",
            "",
        ] {
            check(line, None);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
            ..ProptestConfig::default()
        })]

        #[test]
        fn any_centipawn_score_reads_back(cp: i32) {
            check(&format!("info depth 12 score cp {cp} nodes 5000 pv e2e4"), Some(Score::Cp(cp)));
        }

        #[test]
        fn any_mate_count_reads_back(moves: u8) {
            let expected = if moves == 0 { Score::Mated(0) } else { Score::Mates(moves) };
            check(&format!("info depth 30 score mate {moves} pv a1a8"), Some(expected));
            check(&format!("info depth 30 score mate -{moves} pv a1a8"), Some(Score::Mated(moves)));
        }
    }
}
