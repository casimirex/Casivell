//! §§ 33 and 33b EStG: außergewöhnliche Belastungen.
//!
//! # Why most claims deduct nothing
//!
//! § 33 allows unavoidable extraordinary costs — medical bills, a funeral, damage from a
//! flood — but only the part exceeding a *zumutbare Belastung* calculated from income. For a
//! childless single on 60 000 € that threshold is 3 535,30 €, so a year of 3 000 € in dental
//! work deducts nothing whatever. Telling a household that plainly is more useful than
//! showing them a deduction they will not get.
//!
//! # The threshold is a staircase, and that was decided by a court
//!
//! § 33 Abs. 3 gives three income bands and a percentage for each, and does not say how they
//! combine. The administration long applied the band's percentage to the *whole* income. The
//! BFH rejected that in VI R 75/14 of 19 January 2017: each percentage applies only to the
//! part of income falling in its own band, exactly as § 32a's tariff works. The BMF adopted
//! it by letter of 1 June 2017.
//!
//! For that 60 000 € single, the difference is 4 200 € against 3 535,30 € — six hundred and
//! sixty-five euro of deduction turning on how a table is read. `the_staircase_beats_the_old_flat_reading`
//! pins both figures so the distinction cannot quietly regress.
//!
//! # § 33b is a different route, not an addition
//!
//! The Behinderten-Pauschbetrag is granted *instead of* deducting actual disability costs
//! under § 33, and — crucially — it is **not** reduced by the zumutbare Belastung. Someone
//! with a recognised Grad der Behinderung therefore receives a deduction in a year when the
//! § 33 route would give them none at all. Adding the two together would double-count; running
//! the Pauschbetrag through the threshold would wipe out an entitlement the statute grants
//! unconditionally. [`ExtraordinaryBurden`] keeps them apart and says which produced what.
//!
//! # Not modelled
//!
//! § 33a (Unterhaltsleistungen and the Ausbildungsfreibetrag), which turns on the recipient's
//! own income and assets, and the choice between the § 33b Pauschbetrag and actual costs
//! where the actual costs are larger — that election needs receipts this crate is not given.

use casivell_core::{Money, MoneyError, Rounding};
use casivell_lawdata::{BurdenRow, ExtraordinaryBurdenParameters};
use casivell_tax::FilingStatus;

/// What the household is claiming.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BurdenClaim {
    /// Costs of the general kind under § 33: medical, funeral, disaster.
    ///
    /// Reduced by the zumutbare Belastung, which is why most of these deduct nothing.
    pub general_costs: Money,
    /// Grad der Behinderung, for the § 33b Abs. 3 Pauschbetrag. Zero for none.
    pub disability_degree: u8,
    /// Whether the person is hilflos, blind or taubblind, which replaces the table figure
    /// with the much larger § 33b Abs. 3 Satz 3 amount.
    pub helpless: bool,
    /// Pflegegrad of a person cared for without payment, for the § 33b Abs. 6 Pauschbetrag.
    pub care_grade: u8,
}

/// The deduction, with the two routes kept apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtraordinaryBurden {
    /// The § 33 Abs. 3 threshold, computed as a staircase.
    pub reasonable_burden: Money,
    /// General costs claimed under § 33.
    pub general_costs: Money,
    /// What survived the threshold — often nothing.
    pub general_deductible: Money,

    /// The § 33b Abs. 3 Behinderten-Pauschbetrag, undiminished by the threshold.
    pub disability_lump_sum: Money,
    /// The § 33b Abs. 6 Pflege-Pauschbetrag, likewise.
    pub care_lump_sum: Money,

    /// Everything deductible, which is what § 2 Abs. 4 subtracts.
    pub total: Money,
}

impl ExtraordinaryBurden {
    /// Whether the § 33 threshold swallowed the whole of the general costs.
    ///
    /// Worth surfacing rather than leaving a household to infer it from a zero: "your costs
    /// were below the threshold" and "you had no costs" are different answers.
    #[must_use]
    pub const fn general_costs_absorbed(&self) -> bool {
        !self.general_costs.is_zero() && self.general_deductible.is_zero()
    }
}

/// § 33 Abs. 3: the zumutbare Belastung, applied as a staircase.
///
/// Each band's percentage applies only to the part of `total_income` within that band, per
/// BFH VI R 75/14. `children` selects the table row and is counted in whole children.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn reasonable_burden(
    total_income: Money,
    filing: FilingStatus,
    children: u8,
    burden: &ExtraordinaryBurdenParameters,
) -> Result<Money, MoneyError> {
    let income = total_income.floor_at_zero();
    let row = BurdenRow::for_household(matches!(filing, FilingStatus::JointSplitting), children);
    let rates = burden.rates(row);

    // The three slices of income, each priced at its own band's rate. Written as slices
    // rather than as a running remainder so that each line matches one row of the statute's
    // table and can be checked against it by eye.
    let first = income.min(burden.first_threshold);
    let second = income
        .min(burden.second_threshold)
        .sub(burden.first_threshold)?
        .floor_at_zero();
    let third = income.sub(burden.second_threshold)?.floor_at_zero();

    first
        .mul_rate(rates.lower, Rounding::HalfUp)?
        .add(second.mul_rate(rates.middle, Rounding::HalfUp)?)?
        .add(third.mul_rate(rates.upper, Rounding::HalfUp)?)
}

/// Computes the deduction under §§ 33 and 33b.
///
/// `total_income` is the Gesamtbetrag der Einkünfte, which is what § 33 Abs. 3 measures the
/// threshold against — not the gross salary and not the Einkommen.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn extraordinary_burden(
    claim: &BurdenClaim,
    total_income: Money,
    filing: FilingStatus,
    children: u8,
    burden: &ExtraordinaryBurdenParameters,
) -> Result<ExtraordinaryBurden, MoneyError> {
    let threshold = reasonable_burden(total_income, filing, children, burden)?;
    let general_costs = claim.general_costs.floor_at_zero();
    let general_deductible = general_costs.sub(threshold)?.floor_at_zero();

    // § 33b Abs. 3 Satz 3: the helpless amount *replaces* the table figure rather than adding
    // to it, so the two are a choice and not a sum.
    let disability_lump_sum = if claim.helpless {
        burden.helpless_lump_sum
    } else {
        burden.disability_lump_sum(claim.disability_degree)
    };
    let care_lump_sum = burden.care_lump_sum(claim.care_grade);

    Ok(ExtraordinaryBurden {
        reasonable_burden: threshold,
        general_costs,
        general_deductible,
        disability_lump_sum,
        care_lump_sum,
        // The Pauschbeträge join the deductible remainder untouched by the threshold, which
        // is the whole point of their being Pauschbeträge.
        total: general_deductible
            .add(disability_lump_sum)?
            .add(care_lump_sum)?,
    })
}

#[cfg(test)]
mod tests {
    use super::{BurdenClaim, extraordinary_burden, reasonable_burden};
    use casivell_core::{Money, TaxYear};
    use casivell_lawdata::ExtraordinaryBurdenParameters;
    use casivell_tax::FilingStatus;

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    fn burden() -> ExtraordinaryBurdenParameters {
        ExtraordinaryBurdenParameters::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn threshold(income: i64, filing: FilingStatus, children: u8) -> Money {
        reasonable_burden(euro(income), filing, children, &burden()).expect("computes")
    }

    // ---------------------------------------------------------------------
    // The staircase
    // ---------------------------------------------------------------------

    /// The figure the whole module turns on, computed by hand from the statute.
    ///
    /// A childless single on 60 000 €:
    /// ```text
    ///   5 % of 15 340        =   767,00
    /// + 6 % of 35 790        = 2 147,40
    /// + 7 % of  8 870        =   620,90
    ///                        = 3 535,30
    /// ```
    ///
    /// The pre-2017 reading was 7 % of the whole 60 000 € — 4 200 € — so the BFH's staircase
    /// is worth 664,70 € of extra deduction to this household. Both figures are pinned, so a
    /// regression to the flat reading fails loudly rather than quietly costing people money.
    #[test]
    fn the_staircase_beats_the_old_flat_reading() {
        let staggered = threshold(60_000, FilingStatus::Individual, 0);
        assert_eq!(
            staggered,
            euro(3_535).add(Money::from_cents(30).unwrap()).unwrap()
        );

        let flat_reading = euro(60_000)
            .mul_rate(
                burden().no_children_individual.upper,
                casivell_core::Rounding::HalfUp,
            )
            .unwrap();
        assert_eq!(flat_reading, euro(4_200));
        assert!(
            staggered < flat_reading,
            "the staircase must be the gentler reading"
        );
        assert_eq!(
            flat_reading.sub(staggered).unwrap(),
            euro(664).add(Money::from_cents(70).unwrap()).unwrap()
        );
    }

    /// Each band boundary, checked exactly. Income sitting on a threshold must be priced
    /// entirely at the lower band's rate.
    #[test]
    fn the_band_boundaries_are_exact() {
        let p = burden();
        // Exactly at the first threshold: 5 % of all of it, nothing above.
        assert_eq!(threshold(15_340, FilingStatus::Individual, 0), euro(767));
        // One euro more attracts 6 % on that euro alone, which is six cents.
        assert_eq!(
            threshold(15_341, FilingStatus::Individual, 0),
            euro(767).add(Money::from_cents(6).unwrap()).unwrap()
        );
        // At the second threshold: 767,00 + 6 % of 35 790 = 767,00 + 2 147,40.
        assert_eq!(
            threshold(51_130, FilingStatus::Individual, 0),
            euro(2_914).add(Money::from_cents(40).unwrap()).unwrap()
        );
        let _ = p;
    }

    /// The threshold must be continuous and monotone across the whole range. A staircase read
    /// as a cliff would jump at a boundary, which is precisely the bug the BFH corrected.
    #[test]
    fn the_threshold_rises_smoothly_with_no_cliffs() {
        let mut previous = Money::ZERO;
        for income in (0..120_000).step_by(37) {
            let current = threshold(income, FilingStatus::Individual, 0);
            assert!(current >= previous, "the threshold fell at {income}");
            // A step of 37 € can raise the threshold by at most 7 % of it — under 3 €. A
            // cliff reading would jump by hundreds at a boundary.
            assert!(
                current.sub(previous).unwrap() < euro(3),
                "a jump of {:?} at {income} means the bands are not staggered",
                current.sub(previous).unwrap()
            );
            previous = current;
        }
        assert!(previous > euro(6_000));
    }

    /// Every row of the table, priced at one income, so the family-status spread is visible.
    #[test]
    fn family_circumstances_move_the_threshold_a_long_way() {
        let single = threshold(60_000, FilingStatus::Individual, 0);
        let joint = threshold(60_000, FilingStatus::JointSplitting, 0);
        let two_children = threshold(60_000, FilingStatus::JointSplitting, 2);
        let four_children = threshold(60_000, FilingStatus::JointSplitting, 4);

        assert!(joint < single);
        assert!(two_children < joint);
        assert!(four_children < two_children);
        // The spread across the table is more than fivefold at this income.
        assert!(single.cents() > four_children.cents() * 4);
    }

    /// Children override the filing status, as the table's own structure does.
    #[test]
    fn children_override_the_filing_status() {
        assert_eq!(
            threshold(60_000, FilingStatus::Individual, 2),
            threshold(60_000, FilingStatus::JointSplitting, 2)
        );
    }

    #[test]
    fn no_income_means_no_threshold() {
        assert_eq!(threshold(0, FilingStatus::Individual, 0), Money::ZERO);
    }

    // ---------------------------------------------------------------------
    // § 33: what survives the threshold
    // ---------------------------------------------------------------------

    fn claim_of(costs: i64, income: i64) -> super::ExtraordinaryBurden {
        extraordinary_burden(
            &BurdenClaim {
                general_costs: euro(costs),
                ..BurdenClaim::default()
            },
            euro(income),
            FilingStatus::Individual,
            0,
            &burden(),
        )
        .expect("computes")
    }

    /// The answer most households get, and the reason to say it out loud: a real expense that
    /// deducts nothing at all.
    #[test]
    fn costs_below_the_threshold_deduct_nothing() {
        let result = claim_of(3_000, 60_000);
        assert_eq!(result.general_deductible, Money::ZERO);
        assert_eq!(result.total, Money::ZERO);
        assert!(
            result.general_costs_absorbed(),
            "the report must be able to distinguish this from having no costs at all"
        );
    }

    /// And the distinction the flag exists for.
    #[test]
    fn no_costs_is_not_the_same_as_costs_absorbed() {
        let nothing = claim_of(0, 60_000);
        assert_eq!(nothing.total, Money::ZERO);
        assert!(!nothing.general_costs_absorbed());
    }

    /// Above the threshold only the excess is deductible.
    #[test]
    fn only_the_excess_is_deductible() {
        let result = claim_of(10_000, 60_000);
        assert_eq!(
            result.general_deductible,
            euro(10_000).sub(result.reasonable_burden).unwrap()
        );
        assert_eq!(result.total, result.general_deductible);
        assert!(!result.general_costs_absorbed());
    }

    // ---------------------------------------------------------------------
    // § 33b: the route that ignores the threshold
    // ---------------------------------------------------------------------

    /// The Behinderten-Pauschbetrag is not reduced by the zumutbare Belastung. This is the
    /// distinction most easily got wrong, and getting it wrong would wipe out an entitlement
    /// the statute grants unconditionally.
    #[test]
    fn the_disability_lump_sum_survives_a_threshold_that_absorbs_everything_else() {
        let result = extraordinary_burden(
            &BurdenClaim {
                general_costs: euro(3_000),
                disability_degree: 50,
                ..BurdenClaim::default()
            },
            euro(60_000),
            FilingStatus::Individual,
            0,
            &burden(),
        )
        .expect("computes");

        // The general costs are swallowed whole …
        assert_eq!(result.general_deductible, Money::ZERO);
        // … and the Pauschbetrag arrives untouched.
        assert_eq!(result.disability_lump_sum, euro(1_140));
        assert_eq!(result.total, euro(1_140));
    }

    /// Helplessness replaces the table figure rather than adding to it.
    #[test]
    fn the_helpless_amount_replaces_rather_than_adds() {
        let helpless = extraordinary_burden(
            &BurdenClaim {
                disability_degree: 100,
                helpless: true,
                ..BurdenClaim::default()
            },
            euro(60_000),
            FilingStatus::Individual,
            0,
            &burden(),
        )
        .expect("computes");

        assert_eq!(helpless.disability_lump_sum, euro(7_400));
        assert_eq!(
            helpless.total,
            euro(7_400),
            "the 2 840 EUR table figure must not be added on top"
        );
    }

    /// The Pflege-Pauschbetrag stacks with the disability one — they are different reliefs,
    /// for caring for someone and for being disabled oneself.
    #[test]
    fn the_care_lump_sum_stacks_with_the_disability_one() {
        let both = extraordinary_burden(
            &BurdenClaim {
                disability_degree: 30,
                care_grade: 4,
                ..BurdenClaim::default()
            },
            euro(60_000),
            FilingStatus::Individual,
            0,
            &burden(),
        )
        .expect("computes");
        assert_eq!(both.disability_lump_sum, euro(620));
        assert_eq!(both.care_lump_sum, euro(1_800));
        assert_eq!(both.total, euro(2_420));
    }

    /// Everything must reconcile with the reported total.
    #[test]
    fn the_parts_sum_to_the_total() {
        for (costs, degree, grade, income) in [
            (0_i64, 0_u8, 0_u8, 40_000_i64),
            (20_000, 0, 0, 40_000),
            (3_000, 70, 3, 90_000),
            (0, 100, 5, 20_000),
        ] {
            let result = extraordinary_burden(
                &BurdenClaim {
                    general_costs: euro(costs),
                    disability_degree: degree,
                    helpless: false,
                    care_grade: grade,
                },
                euro(income),
                FilingStatus::Individual,
                0,
                &burden(),
            )
            .expect("computes");
            assert_eq!(
                result.total,
                result
                    .general_deductible
                    .add(result.disability_lump_sum)
                    .unwrap()
                    .add(result.care_lump_sum)
                    .unwrap()
            );
        }
    }

    /// A claim of nothing must deduct nothing, so the provision is inert by default.
    #[test]
    fn an_empty_claim_deducts_nothing() {
        let empty = extraordinary_burden(
            &BurdenClaim::default(),
            euro(60_000),
            FilingStatus::Individual,
            0,
            &burden(),
        )
        .expect("computes");
        assert_eq!(empty.total, Money::ZERO);
        assert!(
            empty.reasonable_burden > Money::ZERO,
            "but the threshold still exists"
        );
    }
}
