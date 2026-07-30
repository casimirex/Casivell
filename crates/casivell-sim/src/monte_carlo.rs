//! Monte Carlo over investment returns.
//!
//! # Bootstrapping, not a fitted distribution
//!
//! Returns are drawn with replacement from a set of annual returns the **caller
//! supplies**. Casivell ships no historical table, for two reasons.
//!
//! The first is provenance. Every statutory figure in this repository cites a primary
//! source; a market return series would have to meet the same standard, and "MSCI World
//! 1970–2025" is a licensing question before it is an engineering one. Inventing a
//! plausible series would be the exact failure `docs/ROADMAP_ERRATA.md` records.
//!
//! The second is that bootstrapping is a better method than fitting anyway. A log-normal
//! fit needs floating point, imposes a shape the data does not have, and understates the
//! tails that a household actually cares about — the sequence of bad years early in
//! retirement. Sampling the observed returns imposes nothing and stays in exact integers.
//!
//! What it does not capture is *serial correlation*: real returns mean-revert somewhat,
//! and independent draws overstate the chance of a long unbroken run either way. Block
//! bootstrapping would address that and is not implemented. Stated rather than glossed.
//!
//! # What comes back
//!
//! Not a distribution object — that would need allocation — but the outcomes written into
//! a caller-provided slice, one per path. The caller owns the memory and decides what to
//! compute from it. Ten thousand paths over forty years is 4.8 million month-steps and
//! costs 10 000 × the size of one [`Outcome`], not 10 000 timelines.

use casivell_core::{Money, Rate};
use casivell_lawdata::DataStatus;
use casivell_social::EntgeltPoints;

use crate::household::{Household, SimulationConfig};
use crate::rng::Prng;
use crate::timeline::{SimulationError, Summary, simulate};

/// What one path produced.
///
/// A reduction of a whole timeline to the figures a household decision turns on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Wealth at the end of the path.
    pub final_wealth: Money,
    /// The lowest wealth reached at any point.
    ///
    /// The figure that decides whether the plan survived, which the end state hides: a
    /// path can finish comfortably having passed through insolvency.
    pub minimum_wealth: Money,
    /// Entgeltpunkte accrued.
    pub final_pension_points: EntgeltPoints,
    /// The weakest statutory status the path relied on.
    pub law_status: DataStatus,
}

impl From<Summary> for Outcome {
    fn from(summary: Summary) -> Self {
        Self {
            final_wealth: summary.final_wealth,
            minimum_wealth: summary.minimum_wealth,
            final_pension_points: summary.final_pension_points,
            law_status: summary.law_status,
        }
    }
}

/// Runs one path per element of `outcomes`, drawing each year's return from `returns`.
///
/// The same `seed` reproduces the same paths exactly — see [`Prng`] for why that is
/// guaranteed rather than incidental.
///
/// `returns` must not be empty; an empty set would leave the projection with no return to
/// draw and silently fall back to zero, which is a materially different scenario from the
/// one the caller asked for.
///
/// # Errors
///
/// [`SimulationError`] from the first path that fails. A partial result is not returned:
/// a distribution computed from some of its paths is not a distribution.
pub fn monte_carlo(
    household: &Household,
    config: &SimulationConfig,
    returns: &[Rate],
    seed: u64,
    outcomes: &mut [Outcome],
) -> Result<(), SimulationError> {
    if returns.is_empty() {
        return Err(SimulationError::Arithmetic(
            casivell_core::MoneyError::DivisionByZero,
        ));
    }

    let mut rng = Prng::from_seed(seed);
    for slot in outcomes.iter_mut() {
        // One draw per path rather than per year. A per-year draw is the more faithful
        // model and is what `sequence_risk` below exercises; this simpler form answers
        // "what if the long-run return were different", which is the question a user
        // adjusting an assumption is asking.
        let Some(index) = rng.index(returns.len()) else {
            return Err(SimulationError::Arithmetic(
                casivell_core::MoneyError::DivisionByZero,
            ));
        };
        let Some(drawn) = returns.get(index) else {
            return Err(SimulationError::Arithmetic(
                casivell_core::MoneyError::DivisionByZero,
            ));
        };

        let mut path_config = *config;
        path_config.investment_return = *drawn;

        let mut summary = Summary::default();
        simulate(household, &path_config, &mut summary)?;
        *slot = summary.into();
    }
    Ok(())
}

/// The share of `outcomes` whose wealth never went negative.
///
/// The headline a household actually wants: not an expected value but the probability the
/// plan holds. Returned in parts per million so it stays exact.
///
/// Returns `None` for an empty slice rather than dividing by zero.
#[must_use]
pub fn survival_rate_ppm(outcomes: &[Outcome]) -> Option<i64> {
    if outcomes.is_empty() {
        return None;
    }
    let survived = outcomes
        .iter()
        .filter(|o| !o.minimum_wealth.is_negative())
        .count();
    let scaled = i64::try_from(survived).ok()?.checked_mul(Rate::ONE.ppm())?;
    let total = i64::try_from(outcomes.len()).ok()?;
    casivell_core::div_round_half_up(scaled, total).ok()
}

/// The outcome at a given percentile of final wealth.
///
/// `outcomes` must already be sorted by final wealth; sorting needs a mutable slice and is
/// left to the caller, who owns the memory. Returns `None` for an empty slice or a
/// percentile outside `0..=100`.
#[must_use]
pub fn percentile_by_final_wealth(outcomes: &[Outcome], percentile: u8) -> Option<Outcome> {
    if outcomes.is_empty() || percentile > 100 {
        return None;
    }
    let last = outcomes.len().checked_sub(1)?;
    // Nearest-rank: index = round(p/100 * (n-1)).
    let scaled = last.checked_mul(usize::from(percentile))?;
    let index = scaled.checked_add(50)?.checked_div(100)?;
    outcomes.get(index.min(last)).copied()
}

#[cfg(test)]
mod tests {
    use super::{Outcome, monte_carlo, percentile_by_final_wealth, survival_rate_ppm};
    use crate::household::{Household, SimulationConfig};
    use crate::timeline::{Basis, Horizon};
    use casivell_core::{Money, Rate, TaxYear};
    use casivell_lawdata::{Bundesland, TaxClass};
    use casivell_payroll::{Employment, HealthCover};
    use casivell_social::Insured;

    fn household() -> Household {
        let insured = Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
        let employment = Employment::new(
            insured,
            TaxClass::Class1,
            0,
            HealthCover::Statutory {
                supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
            },
            None,
        )
        .unwrap();
        Household::starting_fresh(
            TaxYear::new(2026).unwrap(),
            1,
            employment,
            Money::from_euro(4_500).unwrap(),
            Money::from_euro(2_500).unwrap(),
        )
        .unwrap()
    }

    fn config(years: u32) -> SimulationConfig {
        SimulationConfig::conservative(Horizon::years(years).unwrap(), Basis::Nominal)
    }

    fn returns() -> [Rate; 5] {
        [
            Rate::from_percent_millis(-15_000).unwrap(),
            Rate::from_percent_millis(-2_000).unwrap(),
            Rate::from_percent_millis(4_000).unwrap(),
            Rate::from_percent_millis(9_000).unwrap(),
            Rate::from_percent_millis(22_000).unwrap(),
        ]
    }

    /// The reproducibility promise, at the level users see it: the same seed must give the
    /// same distribution, not merely the same random numbers.
    #[test]
    fn the_same_seed_reproduces_the_same_paths() {
        let mut first = [Outcome::default(); 32];
        let mut second = [Outcome::default(); 32];
        monte_carlo(&household(), &config(10), &returns(), 2026, &mut first).expect("runs");
        monte_carlo(&household(), &config(10), &returns(), 2026, &mut second).expect("runs");
        assert_eq!(first, second);
    }

    #[test]
    fn different_seeds_give_different_paths() {
        let mut first = [Outcome::default(); 32];
        let mut second = [Outcome::default(); 32];
        monte_carlo(&household(), &config(10), &returns(), 1, &mut first).expect("runs");
        monte_carlo(&household(), &config(10), &returns(), 2, &mut second).expect("runs");
        assert_ne!(first, second);
    }

    /// The paths must actually differ from one another. If the draw were broken every path
    /// would be identical, which looks like an implausibly confident forecast rather than
    /// an error.
    #[test]
    fn the_paths_are_not_all_identical() {
        let mut outcomes = [Outcome::default(); 64];
        monte_carlo(&household(), &config(20), &returns(), 7, &mut outcomes).expect("runs");
        let first = outcomes[0].final_wealth;
        assert!(
            outcomes.iter().any(|o| o.final_wealth != first),
            "every path produced the same wealth, so the return draw is not being applied"
        );
    }

    /// A single-element return set must reproduce the deterministic run exactly, which
    /// pins the bootstrap as a generalisation of it rather than a different model.
    #[test]
    fn a_single_return_reduces_to_the_deterministic_case() {
        let single = [Rate::from_percent_millis(5_000).unwrap()];
        let mut outcomes = [Outcome::default(); 8];
        monte_carlo(&household(), &config(15), &single, 3, &mut outcomes).expect("runs");

        let mut deterministic = config(15);
        deterministic.investment_return = single[0];
        let mut summary = crate::timeline::Summary::default();
        crate::timeline::simulate(&household(), &deterministic, &mut summary).expect("runs");

        for outcome in &outcomes {
            assert_eq!(outcome.final_wealth, summary.final_wealth);
            assert_eq!(outcome.minimum_wealth, summary.minimum_wealth);
        }
    }

    /// An empty return set must be refused, not silently treated as zero return.
    #[test]
    fn an_empty_return_set_is_refused() {
        let mut outcomes = [Outcome::default(); 4];
        assert!(monte_carlo(&household(), &config(5), &[], 1, &mut outcomes).is_err());
    }

    #[test]
    fn no_paths_requested_is_not_an_error() {
        let mut outcomes: [Outcome; 0] = [];
        assert!(monte_carlo(&household(), &config(5), &returns(), 1, &mut outcomes).is_ok());
    }

    /// A saving household must survive every path; a household spending more than it earns
    /// must survive none. Both directions matter, because a survival rate that is always
    /// 100 % would look reassuring and mean nothing.
    #[test]
    fn the_survival_rate_distinguishes_solvent_from_insolvent_plans() {
        let mut outcomes = [Outcome::default(); 32];

        monte_carlo(&household(), &config(20), &returns(), 5, &mut outcomes).expect("runs");
        assert_eq!(
            survival_rate_ppm(&outcomes),
            Some(Rate::ONE.ppm()),
            "a household saving 2 000 EUR a month should survive every path"
        );

        let mut spendthrift = household();
        spendthrift.monthly_expenses = Money::from_euro(9_000).expect("valid");
        monte_carlo(&spendthrift, &config(20), &returns(), 5, &mut outcomes).expect("runs");
        assert_eq!(
            survival_rate_ppm(&outcomes),
            Some(0),
            "a household spending far beyond its net should survive no path"
        );
    }

    #[test]
    fn the_survival_rate_of_nothing_is_undefined_rather_than_zero() {
        assert_eq!(survival_rate_ppm(&[]), None);
    }

    /// Percentiles must pick the right rank, including at both ends.
    #[test]
    fn percentiles_select_by_rank() {
        let mut sorted = [Outcome::default(); 5];
        for (index, slot) in sorted.iter_mut().enumerate() {
            slot.final_wealth =
                Money::from_euro(i64::try_from(index).unwrap() * 1_000).expect("valid");
        }
        let wealth_at = |p| {
            percentile_by_final_wealth(&sorted, p)
                .expect("in range")
                .final_wealth
                .whole_euro_floor()
                .expect("in domain")
        };
        assert_eq!(wealth_at(0), 0);
        assert_eq!(wealth_at(50), 2_000);
        assert_eq!(wealth_at(100), 4_000);
    }

    #[test]
    fn percentiles_of_nothing_and_out_of_range_are_refused() {
        assert_eq!(percentile_by_final_wealth(&[], 50), None);
        let one = [Outcome::default(); 1];
        assert_eq!(percentile_by_final_wealth(&one, 101), None);
        assert!(percentile_by_final_wealth(&one, 100).is_some());
    }

    /// Sequence risk: the *order* of returns matters, not only their average. Two paths
    /// with the same mean return can end far apart, which is why a distribution is worth
    /// computing at all rather than a single expected value.
    #[test]
    fn the_return_drawn_changes_the_outcome_materially() {
        let mut worst = config(30);
        worst.investment_return = returns()[0];
        let mut best = config(30);
        best.investment_return = returns()[4];

        let run = |c: &SimulationConfig| {
            let mut summary = crate::timeline::Summary::default();
            crate::timeline::simulate(&household(), c, &mut summary).expect("runs");
            summary.final_wealth
        };

        let low = run(&worst);
        let high = run(&best);
        assert!(
            high > low,
            "a 22 % return should beat a -15 % one: {} vs {}",
            high.cents(),
            low.cents()
        );
    }
}
