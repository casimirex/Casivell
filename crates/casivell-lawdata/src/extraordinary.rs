//! Außergewöhnliche Belastungen: §§ 33 and 33b EStG.
//!
//! # Two routes that must not be mixed
//!
//! **§ 33** deducts unavoidable extraordinary costs — medical bills, a funeral, a
//! flood — but only the part exceeding a *zumutbare Belastung* computed from income.
//! For a middling household that threshold runs to several thousand euro, which is
//! why most claims under this provision deduct nothing at all.
//!
//! **§ 33b** grants a flat Pauschbetrag for a disability, and that one is **not**
//! reduced by the zumutbare Belastung. Someone with a recognised Grad der Behinderung
//! therefore gets a deduction where the § 33 route would give them none. The two are
//! alternatives for the same costs, not a sum, and treating them as one would either
//! double-count or wipe out a real entitlement.
//!
//! # The threshold is a staircase, and only since 2017
//!
//! § 33 Abs. 3 states three income bands and a percentage for each, and says nothing
//! about how they combine. The administration long read the table as: find the band
//! the income falls in, apply that percentage to *all* of it. The BFH rejected that in
//! [VI R 75/14] of 19 January 2017, holding that each percentage applies only to the
//! part of income within its own band — a staircase, exactly like § 32a's tariff. The
//! BMF adopted it by letter of 1 June 2017 for all open assessments.
//!
//! The difference is not small. At 60 000 € for a childless single the old reading gave
//! 7 % of everything — 4 200 € — and the staggered one gives 3 535,30 €. Six hundred and
//! sixty-five euro of deduction, from a reading of a table.
//!
//! [VI R 75/14]: https://www.bundesfinanzhof.de/en/entscheidungen/entscheidungen-online/decision-detail/STRE201710072/

use casivell_core::{Money, MoneyError, Rate, TaxYear};

use crate::provenance::{DataStatus, Provenance};

/// Which row of the § 33 Abs. 3 table applies.
///
/// The statute keys the threshold to family circumstances as well as income, and the spread
/// is wide: a childless single pays 7 % at the top band where a household with three children
/// pays 2 %.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurdenRow {
    /// Nr. 1a: no children, assessed under § 32a Abs. 1.
    NoChildrenIndividual,
    /// Nr. 1b: no children, assessed under the Splittingverfahren.
    NoChildrenJoint,
    /// Nr. 2a: one or two children.
    OneOrTwoChildren,
    /// Nr. 2b: three or more children.
    ThreeOrMoreChildren,
}

impl BurdenRow {
    /// The row for a household, from its filing status and child count.
    ///
    /// Children take precedence over the filing status, exactly as the table does: Nr. 2
    /// makes no distinction between individual and joint assessment.
    #[must_use]
    pub const fn for_household(joint: bool, children: u8) -> Self {
        match children {
            0 => {
                if joint {
                    Self::NoChildrenJoint
                } else {
                    Self::NoChildrenIndividual
                }
            }
            1 | 2 => Self::OneOrTwoChildren,
            _ => Self::ThreeOrMoreChildren,
        }
    }
}

/// One row of the § 33 Abs. 3 table: three percentages, one per income band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BurdenRates {
    /// Up to the first threshold.
    pub lower: Rate,
    /// Between the two thresholds.
    pub middle: Rate,
    /// Above the second threshold.
    pub upper: Rate,
}

/// Parameters for außergewöhnliche Belastungen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtraordinaryBurdenParameters {
    /// The year these parameters apply to.
    pub year: TaxYear,

    /// § 33 Abs. 3: the top of the first income band.
    pub first_threshold: Money,
    /// § 33 Abs. 3: the top of the second income band.
    pub second_threshold: Money,

    /// Nr. 1a: no children, Grundtarif.
    pub no_children_individual: BurdenRates,
    /// Nr. 1b: no children, Splittingtarif.
    pub no_children_joint: BurdenRates,
    /// Nr. 2a: one or two children.
    pub one_or_two_children: BurdenRates,
    /// Nr. 2b: three or more children.
    pub three_or_more_children: BurdenRates,

    /// § 33b Abs. 3: the Behinderten-Pauschbeträge, as `(minimum GdB, amount)` pairs.
    ///
    /// Ordered by degree ascending. The applicable amount is the last entry whose degree the
    /// person reaches, so a `GdB` of 55 takes the `50` row — the statute says "mindestens".
    pub disability_lump_sums: [(u8, Money); 9],
    /// § 33b Abs. 3 Satz 3: the amount for a person who is hilflos, blind or taubblind.
    ///
    /// Replaces the table entirely rather than adding to it, and is more than twice the
    /// largest table figure.
    pub helpless_lump_sum: Money,

    /// § 33b Abs. 6: the Pflege-Pauschbeträge for Pflegegrad 2, 3 and 4-or-5.
    pub care_lump_sums: [(u8, Money); 3],

    /// Citation.
    pub provenance: Provenance,
}

impl ExtraordinaryBurdenParameters {
    /// Returns the parameters for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if no verified set exists.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        match year.get() {
            2025 | 2026 => Ok(BURDEN),
            other => Err(MoneyError::YearOutOfRange { year: other }),
        }
    }

    /// The rates for a row.
    #[must_use]
    pub const fn rates(&self, row: BurdenRow) -> BurdenRates {
        match row {
            BurdenRow::NoChildrenIndividual => self.no_children_individual,
            BurdenRow::NoChildrenJoint => self.no_children_joint,
            BurdenRow::OneOrTwoChildren => self.one_or_two_children,
            BurdenRow::ThreeOrMoreChildren => self.three_or_more_children,
        }
    }

    /// § 33b Abs. 3: the Pauschbetrag for a degree of disability.
    ///
    /// Zero below the statutory minimum of 20, which is not an error: a degree below it
    /// simply carries no entitlement.
    #[must_use]
    pub const fn disability_lump_sum(&self, degree: u8) -> Money {
        last_reached(&self.disability_lump_sums, degree)
    }

    /// § 33b Abs. 6: the Pflege-Pauschbetrag for a Pflegegrad.
    ///
    /// Zero for grade 0 or 1, which carry no entitlement.
    #[must_use]
    pub const fn care_lump_sum(&self, grade: u8) -> Money {
        last_reached(&self.care_lump_sums, grade)
    }
}

/// The amount of the last `(minimum, amount)` pair whose minimum `value` reaches.
///
/// Both statutory tables are "mindestens" ladders — a Grad der Behinderung of 55 takes the
/// 50 row, a Pflegegrad of 5 takes the 4 row — so one walk serves both. Zero when the value
/// reaches no entry at all, which is the statute's answer rather than an error.
///
/// Walks the slice by pattern rather than by index, so the absence of an out-of-range access
/// is a property of the code's shape rather than of a bound the reader has to check.
const fn last_reached(table: &[(u8, Money)], value: u8) -> Money {
    let mut applicable = Money::ZERO;
    let mut rest = table;
    while let [(minimum, amount), tail @ ..] = rest {
        if value >= *minimum {
            applicable = *amount;
        }
        rest = tail;
    }
    applicable
}

const fn euro(whole: i64) -> Money {
    match Money::from_euro(whole) {
        Ok(m) => m,
        Err(_) => Money::ZERO,
    }
}

const fn pct(percent: i64) -> Rate {
    match Rate::from_percent(percent) {
        Ok(r) => r,
        Err(_) => Rate::ZERO,
    }
}

const fn rates(lower: i64, middle: i64, upper: i64) -> BurdenRates {
    BurdenRates {
        lower: pct(lower),
        middle: pct(middle),
        upper: pct(upper),
    }
}

/// The §§ 33 and 33b figures.
///
/// Unchanged across both enacted years and for many before them: the two income thresholds
/// have stood since 1975 in Deutsche Mark terms and were merely converted, and the § 33b
/// Pauschbeträge were last reset in 2021, when they were doubled. Stored once for both years
/// because the data is identical and two copies could only drift apart.
const BURDEN: ExtraordinaryBurdenParameters = ExtraordinaryBurdenParameters {
    year: match TaxYear::new(2026) {
        Ok(y) => y,
        Err(_) => TaxYear::LAST_VERIFIED,
    },

    first_threshold: euro(15_340),
    second_threshold: euro(51_130),

    no_children_individual: rates(5, 6, 7),
    no_children_joint: rates(4, 5, 6),
    one_or_two_children: rates(2, 3, 4),
    three_or_more_children: rates(1, 1, 2),

    disability_lump_sums: [
        (20, euro(384)),
        (30, euro(620)),
        (40, euro(860)),
        (50, euro(1_140)),
        (60, euro(1_440)),
        (70, euro(1_780)),
        (80, euro(2_120)),
        (90, euro(2_460)),
        (100, euro(2_840)),
    ],
    helpless_lump_sum: euro(7_400),

    care_lump_sums: [(2, euro(600)), (3, euro(1_100)), (4, euro(1_800))],

    provenance: Provenance::new(
        "§ 33 Abs. 3, § 33b Abs. 3 und 6 EStG; BFH VI R 75/14; BMF-Schreiben vom 01.06.2017",
        "https://www.gesetze-im-internet.de/estg/__33.html",
        "2026-07-31",
        DataStatus::Enacted,
    ),
};

#[cfg(test)]
mod tests {
    use super::{BURDEN, BurdenRow, ExtraordinaryBurdenParameters};
    use casivell_core::{Money, Rate, TaxYear};

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).expect("valid")
    }

    /// Every rate in the table, against the statute. Twelve figures, and a transposition
    /// anywhere in them would silently change a household's threshold.
    #[test]
    fn the_table_matches_the_statute() {
        let p = BURDEN;
        let pct = |v: i64| Rate::from_percent(v).unwrap();

        assert_eq!(p.no_children_individual.lower, pct(5));
        assert_eq!(p.no_children_individual.middle, pct(6));
        assert_eq!(p.no_children_individual.upper, pct(7));

        assert_eq!(p.no_children_joint.lower, pct(4));
        assert_eq!(p.no_children_joint.middle, pct(5));
        assert_eq!(p.no_children_joint.upper, pct(6));

        assert_eq!(p.one_or_two_children.lower, pct(2));
        assert_eq!(p.one_or_two_children.middle, pct(3));
        assert_eq!(p.one_or_two_children.upper, pct(4));

        assert_eq!(p.three_or_more_children.lower, pct(1));
        assert_eq!(p.three_or_more_children.middle, pct(1));
        assert_eq!(p.three_or_more_children.upper, pct(2));

        assert_eq!(p.first_threshold, euro(15_340));
        assert_eq!(p.second_threshold, euro(51_130));
    }

    /// Each rate must rise with income and fall with family size, in every band. The table is
    /// a relief that tapers *against* those without dependants, and an inverted figure would
    /// be hard to spot by eye.
    #[test]
    fn the_rates_rise_with_income_and_fall_with_children() {
        let p = BURDEN;
        for row in [
            p.no_children_individual,
            p.no_children_joint,
            p.one_or_two_children,
            p.three_or_more_children,
        ] {
            assert!(row.lower.ppm() <= row.middle.ppm());
            assert!(row.middle.ppm() <= row.upper.ppm());
        }
        for band in [
            (
                p.no_children_individual.upper,
                p.one_or_two_children.upper,
                p.three_or_more_children.upper,
            ),
            (
                p.no_children_individual.lower,
                p.one_or_two_children.lower,
                p.three_or_more_children.lower,
            ),
        ] {
            assert!(band.0.ppm() > band.1.ppm());
            assert!(band.1.ppm() > band.2.ppm());
        }
        // And a joint assessment is always gentler than an individual one.
        assert!(p.no_children_joint.upper.ppm() < p.no_children_individual.upper.ppm());
    }

    /// The row is chosen by children first, then filing status — as the table is laid out.
    #[test]
    fn the_row_is_chosen_by_children_before_filing_status() {
        assert_eq!(
            BurdenRow::for_household(false, 0),
            BurdenRow::NoChildrenIndividual
        );
        assert_eq!(
            BurdenRow::for_household(true, 0),
            BurdenRow::NoChildrenJoint
        );
        // With children the filing status no longer matters, which is the statute's own
        // structure rather than a simplification.
        for joint in [false, true] {
            assert_eq!(
                BurdenRow::for_household(joint, 1),
                BurdenRow::OneOrTwoChildren
            );
            assert_eq!(
                BurdenRow::for_household(joint, 2),
                BurdenRow::OneOrTwoChildren
            );
            assert_eq!(
                BurdenRow::for_household(joint, 3),
                BurdenRow::ThreeOrMoreChildren
            );
            assert_eq!(
                BurdenRow::for_household(joint, 9),
                BurdenRow::ThreeOrMoreChildren
            );
        }
    }

    /// § 33b Abs. 3: every step of the table, and the "mindestens" behaviour between steps.
    #[test]
    fn the_disability_table_matches_the_statute() {
        let p = BURDEN;
        for (degree, expected) in [
            (20_u8, 384_i64),
            (30, 620),
            (40, 860),
            (50, 1_140),
            (60, 1_440),
            (70, 1_780),
            (80, 2_120),
            (90, 2_460),
            (100, 2_840),
        ] {
            assert_eq!(
                p.disability_lump_sum(degree),
                euro(expected),
                "GdB {degree}"
            );
        }

        // "Mindestens": a degree between steps takes the lower one.
        assert_eq!(p.disability_lump_sum(55), euro(1_140));
        assert_eq!(p.disability_lump_sum(99), euro(2_460));
        // And below the statutory minimum there is no entitlement at all.
        assert_eq!(p.disability_lump_sum(19), Money::ZERO);
        assert_eq!(p.disability_lump_sum(0), Money::ZERO);
    }

    /// The helpless amount replaces the table rather than adding to it, and dwarfs it.
    #[test]
    fn the_helpless_amount_dwarfs_the_table() {
        assert_eq!(BURDEN.helpless_lump_sum, euro(7_400));
        assert!(BURDEN.helpless_lump_sum > BURDEN.disability_lump_sum(100).mul_int(2).unwrap());
    }

    /// § 33b Abs. 6: the Pflege-Pauschbeträge, including that grades 0 and 1 carry none.
    #[test]
    fn the_care_table_matches_the_statute() {
        let p = BURDEN;
        assert_eq!(p.care_lump_sum(0), Money::ZERO);
        assert_eq!(p.care_lump_sum(1), Money::ZERO);
        assert_eq!(p.care_lump_sum(2), euro(600));
        assert_eq!(p.care_lump_sum(3), euro(1_100));
        assert_eq!(p.care_lump_sum(4), euro(1_800));
        assert_eq!(
            p.care_lump_sum(5),
            euro(1_800),
            "grades 4 and 5 share a figure"
        );
    }

    #[test]
    fn both_verified_years_are_available_and_others_are_refused() {
        assert!(ExtraordinaryBurdenParameters::for_year(TaxYear::new(2025).unwrap()).is_ok());
        assert!(ExtraordinaryBurdenParameters::for_year(TaxYear::new(2026).unwrap()).is_ok());
        assert!(ExtraordinaryBurdenParameters::for_year(TaxYear::new(2027).unwrap()).is_err());
    }

    /// The provenance must cite the case law as well as the statute, because the staggered
    /// method is not in the statute at all.
    #[test]
    fn the_provenance_cites_the_case_law_that_settled_the_method() {
        let p = BURDEN.provenance;
        assert!(p.legal_basis.contains("§ 33"));
        assert!(p.legal_basis.contains("VI R 75/14"));
        assert!(p.legal_basis.contains("BMF"));
        assert!(p.status.is_binding_law());
    }
}
