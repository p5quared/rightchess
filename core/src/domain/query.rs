//! Declarative conditions on one move decision. Every child of a composition
//! refers to that same decision, rather than to unrelated moves in a game.
//!
//! Centipawn conditions use the mover's perspective, including after the move.
//!
//! ```
//! use rightchess::query::{BlunderQuery, EvaluationQuery, PlyQuery, Query};
//!
//! let query = Query::all([
//!     BlunderQuery::loss_at_least(200).into(),
//!     EvaluationQuery::before_move().at_least(100).into(),
//!     PlyQuery::between(1, 20)?.into(),
//! ]);
//! # let _ = query;
//! # Ok::<(), anyhow::Error>(())
//! ```

use anyhow::{Result, ensure};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Query {
    Blunder(BlunderQuery),
    Evaluation(EvaluationQuery),
    Ply(PlyQuery),
    All(Vec<Query>),
    Any(Vec<Query>),
}

impl Query {
    #[must_use]
    pub fn all(queries: impl IntoIterator<Item = Query>) -> Self {
        Self::All(queries.into_iter().collect())
    }

    #[must_use]
    pub fn any(queries: impl IntoIterator<Item = Query>) -> Self {
        Self::Any(queries.into_iter().collect())
    }
}

/// Loss is the before-move evaluation minus the after-move evaluation, both
/// measured in centipawns from the mover's perspective with compatible analysis
/// settings. The threshold is inclusive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlunderQuery {
    min_centipawn_loss: u32,
}

impl BlunderQuery {
    /// A zero threshold also matches moves with no evaluation change.
    #[must_use]
    pub fn loss_at_least(min_centipawn_loss: u32) -> Self {
        Self { min_centipawn_loss }
    }

    #[must_use]
    pub fn min_centipawn_loss(&self) -> u32 {
        self.min_centipawn_loss
    }
}

impl From<BlunderQuery> for Query {
    fn from(query: BlunderQuery) -> Self {
        Self::Blunder(query)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvaluationMoment {
    BeforeMove,
    AfterMove,
}

/// Bounds are inclusive. An absent bound is unbounded.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvaluationQuery {
    moment: EvaluationMoment,
    min_centipawns: Option<i32>,
    max_centipawns: Option<i32>,
}

impl EvaluationQuery {
    #[must_use]
    pub fn before_move() -> EvaluationQueryBuilder {
        EvaluationQueryBuilder {
            moment: EvaluationMoment::BeforeMove,
        }
    }

    #[must_use]
    pub fn after_move() -> EvaluationQueryBuilder {
        EvaluationQueryBuilder {
            moment: EvaluationMoment::AfterMove,
        }
    }

    #[must_use]
    pub fn moment(&self) -> EvaluationMoment {
        self.moment
    }

    #[must_use]
    pub fn min_centipawns(&self) -> Option<i32> {
        self.min_centipawns
    }

    #[must_use]
    pub fn max_centipawns(&self) -> Option<i32> {
        self.max_centipawns
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EvaluationQueryBuilder {
    moment: EvaluationMoment,
}

impl EvaluationQueryBuilder {
    #[must_use]
    pub fn at_least(self, min_centipawns: i32) -> EvaluationQuery {
        EvaluationQuery {
            moment: self.moment,
            min_centipawns: Some(min_centipawns),
            max_centipawns: None,
        }
    }

    #[must_use]
    pub fn at_most(self, max_centipawns: i32) -> EvaluationQuery {
        EvaluationQuery {
            moment: self.moment,
            min_centipawns: None,
            max_centipawns: Some(max_centipawns),
        }
    }

    /// # Errors
    ///
    /// Fails if the minimum exceeds the maximum.
    pub fn between(self, min_centipawns: i32, max_centipawns: i32) -> Result<EvaluationQuery> {
        ensure!(
            min_centipawns <= max_centipawns,
            "minimum centipawns must not exceed maximum centipawns"
        );
        Ok(EvaluationQuery {
            moment: self.moment,
            min_centipawns: Some(min_centipawns),
            max_centipawns: Some(max_centipawns),
        })
    }
}

impl From<EvaluationQuery> for Query {
    fn from(query: EvaluationQuery) -> Self {
        Self::Evaluation(query)
    }
}

/// An inclusive range of one-based move ordinals within the stored game.
///
/// Each move by either side counts as one ply. The first played move is ply 1,
/// even when the game's starting FEN has Black to move or a later fullmove number.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlyQuery {
    first_ply: usize,
    last_ply: usize,
}

impl PlyQuery {
    /// # Errors
    ///
    /// Fails if the first ply is zero or exceeds the last ply.
    pub fn between(first_ply: usize, last_ply: usize) -> Result<Self> {
        ensure!(first_ply > 0, "ply ordinals start at 1");
        ensure!(first_ply <= last_ply, "first ply must not exceed last ply");
        Ok(Self {
            first_ply,
            last_ply,
        })
    }

    #[must_use]
    pub fn first_ply(&self) -> usize {
        self.first_ply
    }

    #[must_use]
    pub fn last_ply(&self) -> usize {
        self.last_ply
    }
}

impl From<PlyQuery> for Query {
    fn from(query: PlyQuery) -> Self {
        Self::Ply(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blunder_thresholds_include_zero_and_the_full_unsigned_range() {
        for threshold in [0, 1, 200, u32::MAX] {
            let query = BlunderQuery::loss_at_least(threshold);
            assert_eq!(query.min_centipawn_loss(), threshold);
            assert_eq!(Query::from(query.clone()), Query::Blunder(query));
        }
    }

    #[test]
    fn evaluation_bounds_preserve_the_selected_moment() -> Result<()> {
        for (builder, moment) in [
            (EvaluationQuery::before_move(), EvaluationMoment::BeforeMove),
            (EvaluationQuery::after_move(), EvaluationMoment::AfterMove),
        ] {
            let lower = builder.at_least(-200);
            assert_eq!(lower.moment(), moment);
            assert_eq!(lower.min_centipawns(), Some(-200));
            assert_eq!(lower.max_centipawns(), None);

            let upper = builder.at_most(100);
            assert_eq!(upper.moment(), moment);
            assert_eq!(upper.min_centipawns(), None);
            assert_eq!(upper.max_centipawns(), Some(100));

            let range = builder.between(-200, 100)?;
            assert_eq!(range.moment(), moment);
            assert_eq!(range.min_centipawns(), Some(-200));
            assert_eq!(range.max_centipawns(), Some(100));
            assert_eq!(Query::from(range.clone()), Query::Evaluation(range));
        }
        Ok(())
    }

    #[test]
    fn evaluation_ranges_accept_equal_and_extreme_endpoints() -> Result<()> {
        for (min, max) in [
            (i32::MIN, i32::MIN),
            (0, 0),
            (i32::MAX, i32::MAX),
            (i32::MIN, i32::MAX),
        ] {
            let query = EvaluationQuery::before_move().between(min, max)?;
            assert_eq!(query.min_centipawns(), Some(min));
            assert_eq!(query.max_centipawns(), Some(max));
        }
        Ok(())
    }

    #[test]
    fn evaluation_ranges_reject_reversed_endpoints() {
        for builder in [
            EvaluationQuery::before_move(),
            EvaluationQuery::after_move(),
        ] {
            for (min, max) in [(1, 0), (-100, -200), (i32::MAX, i32::MIN)] {
                let err = builder.between(min, max).unwrap_err();
                assert!(err.to_string().contains("minimum centipawns"), "{err}");
            }
        }
    }

    #[test]
    fn ply_ranges_accept_single_decisions_and_extreme_endpoints() -> Result<()> {
        for (first, last) in [(1, 1), (1, 20), (1, usize::MAX), (usize::MAX, usize::MAX)] {
            let query = PlyQuery::between(first, last)?;
            assert_eq!(query.first_ply(), first);
            assert_eq!(query.last_ply(), last);
            assert_eq!(Query::from(query.clone()), Query::Ply(query));
        }
        Ok(())
    }

    #[test]
    fn ply_ranges_reject_zero_ordinals() {
        for last in [0, 1, usize::MAX] {
            let err = PlyQuery::between(0, last).unwrap_err();
            assert!(err.to_string().contains("start at 1"), "{err}");
        }
    }

    #[test]
    fn ply_ranges_reject_reversed_endpoints() {
        for (first, last) in [(1, 0), (20, 1), (usize::MAX, 1)] {
            let err = PlyQuery::between(first, last).unwrap_err();
            assert!(err.to_string().contains("first ply"), "{err}");
        }
    }

    #[test]
    fn composition_preserves_grouping_and_order() -> Result<()> {
        let blunder = Query::from(BlunderQuery::loss_at_least(200));
        let winning = Query::from(EvaluationQuery::before_move().at_least(100));
        let early = Query::from(PlyQuery::between(1, 20)?);

        let query = Query::all([
            blunder.clone(),
            Query::any([winning.clone(), early.clone()]),
        ]);

        assert_eq!(
            query,
            Query::All(vec![blunder, Query::Any(vec![winning, early])])
        );
        Ok(())
    }

    #[test]
    fn composition_accepts_iterators() {
        let query = Query::any(
            [100, 200]
                .into_iter()
                .map(BlunderQuery::loss_at_least)
                .map(Query::from),
        );
        assert_eq!(
            query,
            Query::Any(vec![
                BlunderQuery::loss_at_least(100).into(),
                BlunderQuery::loss_at_least(200).into(),
            ])
        );
    }

    #[test]
    fn empty_compositions_remain_distinct() {
        assert_eq!(Query::all([]), Query::All(vec![]));
        assert_eq!(Query::any([]), Query::Any(vec![]));
        assert_ne!(Query::all([]), Query::any([]));
    }
}
