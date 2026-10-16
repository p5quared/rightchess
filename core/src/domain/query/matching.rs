use std::num::NonZeroUsize;

use crate::chess::Color;
use crate::domain::{Eval, Score};

use super::{EvaluationMoment, Query};

/// Facts about one move decision, with both scores in the original mover's perspective.
/// Missing facts are permitted; only predicates that need them become unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionFacts {
    game_ply: Option<NonZeroUsize>,
    before: Option<Score>,
    after: Option<Score>,
}

impl DecisionFacts {
    /// Converts both evaluations into `mover`'s perspective, including after the move.
    /// Use the side to move in the before position as `mover`.
    ///
    /// `game_ply` is the one-based ordinal within the original game record, independent
    /// of FEN move counters.
    ///
    /// The caller must supply evaluations of this decision's before and after positions
    /// with compatible analysis settings. An [`Eval`] does not carry that provenance,
    /// so this constructor cannot check it.
    ///
    /// Perspective conversion follows [`Eval`]: negating `i32::MIN` saturates to `i32::MAX`.
    #[must_use]
    pub fn from_evals(
        game_ply: Option<NonZeroUsize>,
        mover: Color,
        before: Option<Eval>,
        after: Option<Eval>,
    ) -> Self {
        let score = |eval: Eval| match mover {
            Color::White => eval.white(),
            Color::Black => eval.black(),
        };
        Self {
            game_ply,
            before: before.map(score),
            after: after.map(score),
        }
    }
}

/// Whether the available facts establish that a query matches one decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchStatus {
    Match,
    NoMatch,
    /// Required facts are missing, or a centipawn predicate encounters a mate score.
    /// More analysis does not necessarily resolve an unknown result.
    Unknown,
}

/// Evaluates every condition against the same decision. Unused facts do not affect the result.
///
/// For [`Query::All`], any non-match wins; otherwise any unknown makes the result unknown.
/// For [`Query::Any`], any match wins; otherwise any unknown makes the result unknown.
/// Empty `All` matches, and empty `Any` does not.
///
/// ```
/// use std::num::NonZeroUsize;
/// use rightchess::{Color, Eval, Score};
/// use rightchess::query::{BlunderQuery, DecisionFacts, MatchStatus, matches};
///
/// // White's advantage rises on Black's move: Black loses 250 centipawns.
/// let facts = DecisionFacts::from_evals(
///     NonZeroUsize::new(2),
///     Color::Black,
///     Some(Eval::from_white(Score::Cp(-100))),
///     Some(Eval::from_white(Score::Cp(150))),
/// );
/// let query = BlunderQuery::loss_at_least(250).into();
/// assert_eq!(matches(&query, &facts), MatchStatus::Match);
/// ```
#[must_use]
pub fn matches(query: &Query, facts: &DecisionFacts) -> MatchStatus {
    match query {
        Query::Blunder(query) => {
            let (Some(Score::Cp(before)), Some(Score::Cp(after))) = (facts.before, facts.after)
            else {
                return MatchStatus::Unknown;
            };
            let loss = i64::from(before) - i64::from(after);
            verdict(loss >= i64::from(query.min_centipawn_loss()))
        }
        Query::Evaluation(query) => {
            let score = match query.moment() {
                EvaluationMoment::BeforeMove => facts.before,
                EvaluationMoment::AfterMove => facts.after,
            };
            let Some(Score::Cp(cp)) = score else {
                return MatchStatus::Unknown;
            };
            verdict(
                query.min_centipawns().is_none_or(|min| cp >= min)
                    && query.max_centipawns().is_none_or(|max| cp <= max),
            )
        }
        Query::Ply(query) => {
            let Some(ply) = facts.game_ply else {
                return MatchStatus::Unknown;
            };
            verdict((query.first_ply()..=query.last_ply()).contains(&ply.get()))
        }
        Query::All(queries) => {
            let mut status = MatchStatus::Match;
            for query in queries {
                match matches(query, facts) {
                    MatchStatus::NoMatch => return MatchStatus::NoMatch,
                    MatchStatus::Unknown => status = MatchStatus::Unknown,
                    MatchStatus::Match => {}
                }
            }
            status
        }
        Query::Any(queries) => {
            let mut status = MatchStatus::NoMatch;
            for query in queries {
                match matches(query, facts) {
                    MatchStatus::Match => return MatchStatus::Match,
                    MatchStatus::Unknown => status = MatchStatus::Unknown,
                    MatchStatus::NoMatch => {}
                }
            }
            status
        }
    }
}

fn verdict(matches: bool) -> MatchStatus {
    if matches {
        MatchStatus::Match
    } else {
        MatchStatus::NoMatch
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use crate::query::{
        BlunderQuery, DecisionFacts, EvaluationMoment, EvaluationQuery, MatchStatus, PlyQuery,
        Query, matches,
    };
    use crate::{Color, Eval, Score};

    use MatchStatus::{Match, NoMatch, Unknown};

    #[track_caller]
    fn check(query: impl Into<Query>, facts: &DecisionFacts, expected: MatchStatus) {
        let query = query.into();
        assert_eq!(matches(&query, facts), expected, "{query:?}, {facts:?}");
    }

    fn cp_facts(mover: Color, before: i32, after: i32) -> DecisionFacts {
        DecisionFacts::from_evals(
            None,
            mover,
            Some(Eval::from_white(Score::Cp(before))),
            Some(Eval::from_white(Score::Cp(after))),
        )
    }

    fn at(moment: EvaluationMoment, score: Option<Score>) -> DecisionFacts {
        let eval = score.map(Eval::from_white);
        match moment {
            EvaluationMoment::BeforeMove => {
                DecisionFacts::from_evals(None, Color::White, eval, None)
            }
            EvaluationMoment::AfterMove => {
                DecisionFacts::from_evals(None, Color::White, None, eval)
            }
        }
    }

    #[test]
    fn blunder_loss_is_inclusive_and_improvements_do_not_match_zero() {
        for (before, after, threshold, expected) in [
            (100, -100, 199, Match),
            (100, -100, 200, Match),
            (100, -100, 201, NoMatch),
            (-100, -100, 0, Match),
            (-100, -100, 1, NoMatch),
            (-100, -99, 0, NoMatch),
            (-100, -101, 0, Match),
            (i32::MAX, i32::MIN, u32::MAX, Match),
            (i32::MAX, i32::MIN + 1, u32::MAX, NoMatch),
            (i32::MIN, i32::MAX, 0, NoMatch),
        ] {
            check(
                BlunderQuery::loss_at_least(threshold),
                &cp_facts(Color::White, before, after),
                expected,
            );
        }
    }

    #[test]
    fn both_scores_stay_in_the_original_movers_perspective() {
        for (mover, before, after) in [(Color::White, 100, -150), (Color::Black, -100, 150)] {
            let facts = cp_facts(mover, before, after);
            check(BlunderQuery::loss_at_least(250), &facts, Match);
            check(BlunderQuery::loss_at_least(251), &facts, NoMatch);
            check(EvaluationQuery::before_move().at_least(100), &facts, Match);
            check(
                EvaluationQuery::before_move().at_least(101),
                &facts,
                NoMatch,
            );
            check(EvaluationQuery::after_move().at_most(-150), &facts, Match);
            check(EvaluationQuery::after_move().at_least(0), &facts, NoMatch);
        }
    }

    #[test]
    fn black_perspective_preserves_evals_saturating_negation() {
        let facts = cp_facts(Color::Black, i32::MIN, i32::MAX);
        check(
            EvaluationQuery::before_move().at_least(i32::MAX),
            &facts,
            Match,
        );
        check(BlunderQuery::loss_at_least(u32::MAX - 1), &facts, Match);
        check(BlunderQuery::loss_at_least(u32::MAX), &facts, NoMatch);
    }

    #[test]
    fn evaluation_bounds_are_inclusive_at_the_selected_moment() {
        for (builder, moment) in [
            (EvaluationQuery::before_move(), EvaluationMoment::BeforeMove),
            (EvaluationQuery::after_move(), EvaluationMoment::AfterMove),
        ] {
            for (cp, min_result, max_result) in [
                (99, NoMatch, Match),
                (100, Match, Match),
                (101, Match, NoMatch),
            ] {
                let facts = at(moment, Some(Score::Cp(cp)));
                check(builder.at_least(100), &facts, min_result);
                check(builder.at_most(100), &facts, max_result);
            }
            for (cp, expected) in [
                (-201, NoMatch),
                (-200, Match),
                (0, Match),
                (100, Match),
                (101, NoMatch),
            ] {
                check(
                    builder.between(-200, 100).expect("valid bounds"),
                    &at(moment, Some(Score::Cp(cp))),
                    expected,
                );
            }
            for cp in [i32::MIN, 0, i32::MAX] {
                let facts = at(moment, Some(Score::Cp(cp)));
                check(
                    builder.between(i32::MIN, i32::MAX).expect("valid bounds"),
                    &facts,
                    Match,
                );
                check(
                    builder.between(cp, cp).expect("valid bounds"),
                    &facts,
                    Match,
                );
            }
            check(
                builder.between(0, 0).expect("valid bounds"),
                &at(moment, Some(Score::Cp(-1))),
                NoMatch,
            );
        }
    }

    #[test]
    fn missing_and_mate_scores_make_required_centipawn_conditions_unknown() {
        for unavailable in [
            None,
            Some(Score::Mates(1)),
            Some(Score::Mated(3)),
            Some(Score::Mated(0)),
        ] {
            let unavailable = unavailable.map(Eval::from_white);
            let known = Some(Eval::from_white(Score::Cp(100)));
            for mover in [Color::White, Color::Black] {
                let before_missing = DecisionFacts::from_evals(None, mover, unavailable, known);
                let after_missing = DecisionFacts::from_evals(None, mover, known, unavailable);
                check(
                    EvaluationQuery::before_move().at_least(0),
                    &before_missing,
                    Unknown,
                );
                check(
                    EvaluationQuery::after_move().at_least(0),
                    &after_missing,
                    Unknown,
                );
                check(BlunderQuery::loss_at_least(0), &before_missing, Unknown);
                check(BlunderQuery::loss_at_least(0), &after_missing, Unknown);

                // An unavailable score at the other moment is irrelevant.
                check(
                    EvaluationQuery::after_move().at_least(-100),
                    &before_missing,
                    Match,
                );
                check(
                    EvaluationQuery::before_move().at_least(-100),
                    &after_missing,
                    Match,
                );
            }
        }
        let facts = DecisionFacts::from_evals(None, Color::White, None, None);
        check(BlunderQuery::loss_at_least(0), &facts, Unknown);
    }

    #[test]
    fn ply_ranges_use_the_supplied_ordinal_without_needing_evaluations() {
        let query = PlyQuery::between(5, 10).expect("valid range");
        for (ply, expected) in [
            (1, NoMatch),
            (4, NoMatch),
            (5, Match),
            (10, Match),
            (11, NoMatch),
        ] {
            let facts = DecisionFacts::from_evals(NonZeroUsize::new(ply), Color::Black, None, None);
            check(query.clone(), &facts, expected);
        }
        for ply in [1, usize::MAX] {
            let facts = DecisionFacts::from_evals(NonZeroUsize::new(ply), Color::White, None, None);
            check(
                PlyQuery::between(1, usize::MAX).expect("valid range"),
                &facts,
                Match,
            );
            check(
                PlyQuery::between(ply, ply).expect("valid range"),
                &facts,
                Match,
            );
        }
        check(query, &cp_facts(Color::White, 100, -100), Unknown);
    }

    #[test]
    fn compositions_follow_three_valued_logic_in_both_orders() {
        // With these facts, the queries below are respectively true, false, and unknown.
        let facts = DecisionFacts::from_evals(NonZeroUsize::new(1), Color::White, None, None);
        let queries: [Query; 3] = [
            PlyQuery::between(1, 1).expect("valid range").into(),
            PlyQuery::between(2, 2).expect("valid range").into(),
            EvaluationQuery::before_move().at_least(0).into(),
        ];
        let all = [
            [Match, NoMatch, Unknown],
            [NoMatch, NoMatch, NoMatch],
            [Unknown, NoMatch, Unknown],
        ];
        let any = [
            [Match, Match, Match],
            [Match, NoMatch, Unknown],
            [Match, Unknown, Unknown],
        ];
        for (i, left) in queries.iter().enumerate() {
            for (j, right) in queries.iter().enumerate() {
                check(Query::all([left.clone(), right.clone()]), &facts, all[i][j]);
                check(Query::any([left.clone(), right.clone()]), &facts, any[i][j]);
            }
        }
    }

    #[test]
    fn empty_and_nested_compositions_work_without_facts() {
        let facts = DecisionFacts::from_evals(None, Color::White, None, None);
        let unknown: Query = EvaluationQuery::before_move().at_least(0).into();
        check(Query::all([]), &facts, Match);
        check(Query::any([]), &facts, NoMatch);
        check(
            Query::all([unknown.clone(), Query::any([])]),
            &facts,
            NoMatch,
        );
        check(Query::any([unknown.clone(), Query::all([])]), &facts, Match);
        check(
            Query::all([
                Query::any([Query::any([]), unknown.clone()]),
                Query::all([]),
            ]),
            &facts,
            Unknown,
        );
        check(
            Query::any([Query::all([unknown, Query::any([])]), Query::all([])]),
            &facts,
            Match,
        );
    }

    #[test]
    fn all_conditions_must_hold_for_one_decision() {
        let blunder: Query = BlunderQuery::loss_at_least(200).into();
        let winning: Query = EvaluationQuery::before_move().at_least(100).into();
        let early: Query = PlyQuery::between(1, 20).expect("valid range").into();
        let query = Query::all([blunder.clone(), winning.clone(), early.clone()]);

        // Each decision satisfies one condition, but none satisfies their conjunction.
        for (condition, ply, before, after) in [
            (blunder, 30, -100, -400),
            (winning, 30, 200, 200),
            (early, 1, 0, 0),
        ] {
            let facts = DecisionFacts::from_evals(
                NonZeroUsize::new(ply),
                Color::White,
                Some(Eval::from_white(Score::Cp(before))),
                Some(Eval::from_white(Score::Cp(after))),
            );
            check(condition, &facts, Match);
            check(query.clone(), &facts, NoMatch);
        }
        let facts = DecisionFacts::from_evals(
            NonZeroUsize::new(1),
            Color::White,
            Some(Eval::from_white(Score::Cp(200))),
            Some(Eval::from_white(Score::Cp(-100))),
        );
        check(query, &facts, Match);
    }
}
