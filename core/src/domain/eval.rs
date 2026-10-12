/// From one side's point of view. Mate counts are in moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Score {
    Cp(i32),
    Mates(u8),
    /// `Mated(0)`: checkmated now.
    Mated(u8),
}

/// Draws and stalemates are `Cp(0)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eval {
    white: Score,
}

impl Score {
    fn flipped(self) -> Score {
        match self {
            Score::Cp(centipawns) => Score::Cp(centipawns.saturating_neg()),
            Score::Mates(moves) => Score::Mated(moves),
            Score::Mated(moves) => Score::Mates(moves),
        }
    }
}

impl Eval {
    pub fn from_white(score: Score) -> Eval {
        Eval { white: score }
    }

    pub fn from_black(score: Score) -> Eval {
        Eval {
            white: score.flipped(),
        }
    }

    pub fn white(&self) -> Score {
        self.white
    }

    pub fn black(&self) -> Score {
        self.white.flipped()
    }
}

/// Centipawn negation saturates: `Cp(i32::MIN)` flips to `Cp(i32::MAX)`.
#[cfg(test)]
mod tests {
    use super::{Eval, Score};

    fn sides() -> Vec<(Score, Score)> {
        vec![
            (Score::Cp(0), Score::Cp(0)),
            (Score::Cp(35), Score::Cp(-35)),
            (Score::Cp(-120), Score::Cp(120)),
            (Score::Cp(i32::MAX), Score::Cp(-i32::MAX)),
            (Score::Mates(1), Score::Mated(1)),
            (Score::Mates(255), Score::Mated(255)),
            (Score::Mated(3), Score::Mates(3)),
            (Score::Mated(0), Score::Mates(0)),
        ]
    }

    #[test]
    fn white_score_reads_back_from_both_sides() {
        for (white, black) in sides() {
            let eval = Eval::from_white(white);
            assert_eq!((eval.white(), eval.black()), (white, black), "{white:?}");
        }
    }

    #[test]
    fn black_score_reads_back_from_both_sides() {
        for (white, black) in sides() {
            let eval = Eval::from_black(black);
            assert_eq!((eval.white(), eval.black()), (white, black), "{black:?}");
        }
    }

    #[test]
    fn most_negative_centipawns_saturate() {
        let eval = Eval::from_white(Score::Cp(i32::MIN));
        assert_eq!(eval.white(), Score::Cp(i32::MIN));
        assert_eq!(eval.black(), Score::Cp(i32::MAX));
        assert_eq!(
            Eval::from_black(Score::Cp(i32::MIN)).white(),
            Score::Cp(i32::MAX)
        );
    }

    mod properties {
        use proptest::prelude::*;
        use proptest::test_runner::FileFailurePersistence;

        use super::super::{Eval, Score};

        /// Excludes `Cp(i32::MIN)`, which has no negation.
        fn score() -> impl Strategy<Value = Score> {
            prop_oneof![
                (i32::MIN + 1..=i32::MAX).prop_map(Score::Cp),
                any::<u8>().prop_map(Score::Mates),
                any::<u8>().prop_map(Score::Mated),
            ]
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
                ..ProptestConfig::default()
            })]

            #[test]
            fn flipping_sides_twice_gives_the_score_back(score in score()) {
                prop_assert_eq!(Eval::from_black(Eval::from_white(score).black()).white(), score);
                prop_assert_eq!(Eval::from_white(Eval::from_black(score).white()).black(), score);
            }
        }
    }
}
