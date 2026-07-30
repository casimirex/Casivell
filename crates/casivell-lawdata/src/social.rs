//! Social insurance parameters: pension, health and long-term care.
//!
//! # Three corrections to the original specification, encoded structurally
//!
//! 1. **There is no longer a West/East split.** The pension contribution ceiling
//!    was unified across the former inner-German border on 1 January 2025, and
//!    the Rentenwert on 1 July 2023. Carrying two regional values would not merely
//!    be redundant, it would produce wrong answers, so the types offer no place
//!    to put one.
//!
//! 2. **The Rentenwert changes on 1 July, not 1 January.** The annual pension
//!    adjustment takes effect mid-year under § 65 SGB VI, so a calendar year has
//!    two pension values. A model with one value per year is wrong for six months
//!    of every year. [`PensionInsurance`] therefore holds both, and
//!    [`PensionInsurance::pension_value_for_month`] selects between them.
//!
//! 3. **The childless surcharge is not shared with the employer.** Under
//!    § 55 Abs. 3 SGB XI the 0.6 % Beitragszuschlag für Kinderlose is borne by the
//!    employee alone. Halving a combined rate — as the original specification's
//!    sketch did — understates the employee's burden by 0.3 % of gross pay for
//!    every childless person over 23, which is most young professionals. The
//!    surcharge is kept in its own field so it cannot be swept into the halving.

use casivell_core::{Money, MoneyError, Rate, TaxYear};

use crate::provenance::{DataStatus, Provenance};

/// Statutory pension insurance, SGB VI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionInsurance {
    /// Combined employer and employee contribution rate, § 158 SGB VI.
    pub contribution_rate: Rate,
    /// Monthly Beitragsbemessungsgrenze. Income above this is not contributory
    /// and earns no further Entgeltpunkte.
    pub ceiling_monthly: Money,
    /// Provisional Durchschnittsentgelt for the year, Anlage 1 SGB VI.
    ///
    /// One Entgeltpunkt is earned by contributing on exactly this much income, so
    /// it is the denominator of every pension entitlement calculation.
    pub average_earnings_annual: Money,
    /// Aktueller Rentenwert for January through June.
    pub pension_value_jan_to_jun: Money,
    /// Aktueller Rentenwert for July through December, after the § 65 SGB VI
    /// adjustment takes effect.
    pub pension_value_jul_to_dec: Money,
    /// Citation.
    pub provenance: Provenance,
}

impl PensionInsurance {
    /// The month in which the annual pension adjustment takes effect.
    pub const ADJUSTMENT_MONTH: u8 = 7;

    /// The Rentenwert applicable in `month`, where January is 1.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if `month` is not in `1..=12`. Refused rather
    /// than clamped: a month index out of range is a caller defect, and silently
    /// treating month 13 as December would hide it.
    pub const fn pension_value_for_month(&self, month: u8) -> Result<Money, MoneyError> {
        if month < 1 || month > 12 {
            return Err(MoneyError::OutOfDomain {
                cents: month as i64,
            });
        }
        if month < Self::ADJUSTMENT_MONTH {
            Ok(self.pension_value_jan_to_jun)
        } else {
            Ok(self.pension_value_jul_to_dec)
        }
    }
}

/// Statutory health insurance, SGB V.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthInsurance {
    /// Allgemeiner Beitragssatz, fixed at 14.6 % by § 241 SGB V.
    pub general_rate: Rate,
    /// The average Zusatzbeitrag announced by the BMG for the year.
    ///
    /// This is an *average*, published so that funds without their own published
    /// rate have a default. An individual fund's actual rate may differ by well
    /// over a percentage point, so a user must be able to override it — the
    /// engine must never present this figure as that user's cost.
    pub average_supplementary_rate: Rate,
    /// Monthly Beitragsbemessungsgrenze.
    pub ceiling_monthly: Money,
    /// Annual Jahresarbeitsentgeltgrenze, above which an employee may leave the
    /// statutory system for private cover.
    pub compulsory_insurance_threshold_annual: Money,
    /// Citation.
    pub provenance: Provenance,
}

/// Long-term care insurance, SGB XI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CareInsurance {
    /// Base rate for a member with one child, § 55 Abs. 1 SGB XI. Shared equally
    /// with the employer, except in Saxony.
    pub base_rate: Rate,
    /// Beitragszuschlag for childless members, § 55 Abs. 3 SGB XI.
    ///
    /// Borne by the employee alone. See the module documentation.
    pub childless_surcharge: Rate,
    /// Age from which the childless surcharge applies.
    pub childless_surcharge_min_age: u8,
    /// Reduction per child, from the second through the fifth, § 55 Abs. 3 SGB XI.
    pub per_child_reduction: Rate,
    /// The highest child ordinal that still attracts a reduction. Children beyond
    /// this do not reduce the rate further.
    pub max_reduced_child_ordinal: u8,
    /// Age at which a child stops counting toward the reduction.
    pub child_reduction_max_child_age: u8,
    /// The extra share a Saxon employee bears relative to an equal split.
    ///
    /// Saxony did not abolish Buß- und Bettag as a public holiday, so under
    /// § 58 Abs. 3 SGB XI its employees carry 0.5 percentage points more and
    /// employers 0.5 fewer than elsewhere.
    pub saxony_employee_surcharge: Rate,
    /// Citation.
    pub provenance: Provenance,
}

/// Unemployment insurance, SGB III.
///
/// Shares the pension insurance contribution ceiling — both are set to the same
/// figure by the annual SVBezGrV. The value is nonetheless stored here rather than
/// read from [`PensionInsurance`], because the two being equal is a recurring
/// legislative choice, not a structural identity: they could diverge, and a
/// reviewer should be able to check this figure against the ordinance without
/// first working out which other field it borrows from. The
/// `unemployment_and_pension_share_a_ceiling` test asserts the equality holds for
/// every shipped year, so a genuine divergence surfaces as a test failure rather
/// than as silently wrong arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnemploymentInsurance {
    /// Combined employer and employee contribution rate, § 341 Abs. 2 SGB III.
    pub contribution_rate: Rate,
    /// Monthly Beitragsbemessungsgrenze.
    pub ceiling_monthly: Money,
    /// Citation.
    pub provenance: Provenance,
}

/// All social insurance parameters for one year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocialParameters {
    /// The year these parameters apply to.
    pub year: TaxYear,
    /// Pension insurance.
    pub pension: PensionInsurance,
    /// Unemployment insurance.
    pub unemployment: UnemploymentInsurance,
    /// Health insurance.
    pub health: HealthInsurance,
    /// Long-term care insurance.
    pub care: CareInsurance,
    /// Monthly Bezugsgröße, § 18 SGB IV — the reference value a number of
    /// downstream thresholds are expressed as fractions of.
    pub reference_value_monthly: Money,
}

impl SocialParameters {
    /// Returns the parameters for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if no verified set exists.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        match year.get() {
            2025 => Ok(SOCIAL_2025),
            2026 => Ok(SOCIAL_2026),
            other => Err(MoneyError::YearOutOfRange { year: other }),
        }
    }

    /// The weakest [`DataStatus`] across the four branches.
    #[must_use]
    pub const fn status(&self) -> DataStatus {
        self.pension
            .provenance
            .status
            .weakest(self.unemployment.provenance.status)
            .weakest(self.health.provenance.status)
            .weakest(self.care.provenance.status)
    }
}

/// Builds a [`Money`] from euro and cents inside a `const` table.
///
/// `Money::from_euro_cents` returns a `Result`, which cannot be unwrapped in a
/// `const` initialiser without a panicking construct. Every call site below uses
/// a literal well inside the domain, so the error arm is unreachable; it resolves
/// to zero rather than panicking, and the `no_zero_amounts_in_tables` test proves
/// no such fallback was actually taken.
const fn euro(whole: i64, cents: u8) -> Money {
    match Money::from_euro_cents(whole, cents) {
        Ok(m) => m,
        Err(_) => Money::ZERO,
    }
}

/// Builds a [`Rate`] from thousandths of a percent inside a `const` table.
///
/// Same reasoning as [`euro`]; guarded by `no_zero_rates_in_tables`.
const fn pct_milli(percent_millis: i64) -> Rate {
    match Rate::from_percent_millis(percent_millis) {
        Ok(r) => r,
        Err(_) => Rate::ZERO,
    }
}

/// Constructs a [`TaxYear`] inside a `const` table.
const fn year(value: u16) -> TaxYear {
    match TaxYear::new(value) {
        Ok(y) => y,
        Err(_) => TaxYear::MIN,
    }
}

/// Social insurance parameters for 2025.
const SOCIAL_2025: SocialParameters = SocialParameters {
    year: year(2025),
    pension: PensionInsurance {
        contribution_rate: pct_milli(18_600),
        ceiling_monthly: euro(8_050, 0),
        average_earnings_annual: euro(50_493, 0),
        pension_value_jan_to_jun: euro(39, 32),
        pension_value_jul_to_dec: euro(40, 79),
        provenance: Provenance::new(
            "§ 158 SGB VI, Anlage 1 SGB VI, SVBezGrV 2025, RWBestV 2025",
            "https://www.gesetze-im-internet.de/svbezgrv_2025/BJNR16D0A0024.html",
            "2026-07-30",
            DataStatus::Enacted,
        ),
    },
    unemployment: UnemploymentInsurance {
        contribution_rate: pct_milli(2_600),
        ceiling_monthly: euro(8_050, 0),
        provenance: Provenance::new(
            "§ 341 Abs. 2 SGB III, SVBezGrV 2025",
            "https://www.gesetze-im-internet.de/sgb_3/__341.html",
            "2026-07-30",
            DataStatus::Enacted,
        ),
    },
    health: HealthInsurance {
        general_rate: pct_milli(14_600),
        average_supplementary_rate: pct_milli(2_500),
        ceiling_monthly: euro(5_512, 50),
        compulsory_insurance_threshold_annual: euro(73_800, 0),
        provenance: Provenance::new(
            "§ 241 SGB V, § 6 Abs. 7 SGB V, SVBezGrV 2025; Zusatzbeitrag: BMG-Bekanntmachung nach § 242a SGB V",
            "https://www.gesetze-im-internet.de/svbezgrv_2025/BJNR16D0A0024.html",
            "2026-07-30",
            DataStatus::Enacted,
        ),
    },
    care: CARE_COMMON_2025_2026,
    reference_value_monthly: euro(3_745, 0),
};

/// Social insurance parameters for 2026.
const SOCIAL_2026: SocialParameters = SocialParameters {
    year: year(2026),
    pension: PensionInsurance {
        contribution_rate: pct_milli(18_600),
        ceiling_monthly: euro(8_450, 0),
        average_earnings_annual: euro(51_944, 0),
        pension_value_jan_to_jun: euro(40, 79),
        pension_value_jul_to_dec: euro(42, 52),
        provenance: Provenance::new(
            "§ 158 SGB VI, Anlage 1 SGB VI, SVBezGrV 2026, RWBestV 2026",
            "https://www.gesetze-im-internet.de/svbezgrv_2026/BJNR1160A0025.html",
            "2026-07-30",
            DataStatus::Enacted,
        ),
    },
    unemployment: UnemploymentInsurance {
        contribution_rate: pct_milli(2_600),
        ceiling_monthly: euro(8_450, 0),
        provenance: Provenance::new(
            "§ 341 Abs. 2 SGB III, SVBezGrV 2026",
            "https://www.gesetze-im-internet.de/sgb_3/__341.html",
            "2026-07-30",
            DataStatus::Enacted,
        ),
    },
    health: HealthInsurance {
        general_rate: pct_milli(14_600),
        average_supplementary_rate: pct_milli(2_900),
        ceiling_monthly: euro(5_812, 50),
        compulsory_insurance_threshold_annual: euro(77_400, 0),
        provenance: Provenance::new(
            "§ 241 SGB V, § 6 Abs. 7 SGB V, SVBezGrV 2026; Zusatzbeitrag: BMG-Bekanntmachung nach § 242a SGB V",
            "https://www.gesetze-im-internet.de/svbezgrv_2026/BJNR1160A0025.html",
            "2026-07-30",
            DataStatus::Enacted,
        ),
    },
    care: CARE_COMMON_2025_2026,
    reference_value_monthly: euro(3_955, 0),
};

/// Care insurance rates, unchanged between 2025 and 2026.
///
/// Shared rather than duplicated so the two years cannot drift apart by a typo.
/// The moment they genuinely diverge, this splits into two constants — that is a
/// deliberate edit, which is the point.
const CARE_COMMON_2025_2026: CareInsurance = CareInsurance {
    base_rate: pct_milli(3_600),
    childless_surcharge: pct_milli(600),
    childless_surcharge_min_age: 23,
    per_child_reduction: pct_milli(250),
    max_reduced_child_ordinal: 5,
    child_reduction_max_child_age: 25,
    saxony_employee_surcharge: pct_milli(500),
    provenance: Provenance::new(
        "§ 55 Abs. 1 und 3 SGB XI, § 58 Abs. 3 SGB XI",
        "https://www.gesetze-im-internet.de/sgb_11/__55.html",
        "2026-07-30",
        DataStatus::Enacted,
    ),
};

#[cfg(test)]
mod tests {
    use super::{CareInsurance, SOCIAL_2025, SOCIAL_2026, SocialParameters};
    use casivell_core::{Money, MoneyError, Rate, TaxYear};

    fn every_year() -> [SocialParameters; 2] {
        [SOCIAL_2025, SOCIAL_2026]
    }

    /// The `const` fallbacks in `euro`/`pct_milli` resolve to zero on error. A
    /// zero anywhere it does not belong means a literal fell outside its domain
    /// and the table is quietly wrong. This is the test that makes those
    /// non-panicking fallbacks safe to use.
    #[test]
    fn no_amount_or_rate_in_a_table_is_accidentally_zero() {
        for p in every_year() {
            let amounts = [
                ("pension ceiling", p.pension.ceiling_monthly),
                ("average earnings", p.pension.average_earnings_annual),
                ("Rentenwert H1", p.pension.pension_value_jan_to_jun),
                ("Rentenwert H2", p.pension.pension_value_jul_to_dec),
                ("health ceiling", p.health.ceiling_monthly),
                ("JAEG", p.health.compulsory_insurance_threshold_annual),
                ("Bezugsgröße", p.reference_value_monthly),
                ("unemployment ceiling", p.unemployment.ceiling_monthly),
            ];
            for (name, amount) in amounts {
                assert!(
                    !amount.is_zero() && !amount.is_negative(),
                    "{}: {name} is {} cents, which means the literal was rejected",
                    p.year.get(),
                    amount.cents()
                );
            }
            let rates = [
                ("pension rate", p.pension.contribution_rate),
                ("unemployment rate", p.unemployment.contribution_rate),
                ("GKV general rate", p.health.general_rate),
                (
                    "GKV supplementary rate",
                    p.health.average_supplementary_rate,
                ),
                ("care base rate", p.care.base_rate),
                ("childless surcharge", p.care.childless_surcharge),
                ("per-child reduction", p.care.per_child_reduction),
                ("Saxony surcharge", p.care.saxony_employee_surcharge),
            ];
            for (name, rate) in rates {
                assert!(
                    !rate.is_zero(),
                    "{}: {name} is zero, which means the literal was rejected",
                    p.year.get()
                );
            }
        }
    }

    #[test]
    fn lookup_returns_the_year_it_was_asked_for() {
        for p in every_year() {
            let found = SocialParameters::for_year(p.year).expect("shipped year");
            assert_eq!(found.year, p.year);
            assert_eq!(found, p);
        }
    }

    #[test]
    fn every_supported_year_has_social_parameters() {
        let mut y = TaxYear::MIN.get();
        while y <= TaxYear::MAX.get() {
            let tax_year = TaxYear::new(y).expect("in range");
            assert!(
                SocialParameters::for_year(tax_year).is_ok(),
                "no social parameters for supported year {y}"
            );
            y = y.saturating_add(1);
        }
    }

    /// The § 65 SGB VI adjustment lands on 1 July. Off-by-one here would apply the
    /// new Rentenwert to June or the old one to July.
    #[test]
    fn the_pension_value_switches_at_the_start_of_july() {
        let pension = SOCIAL_2026.pension;
        for month in 1_u8..=6 {
            assert_eq!(
                pension.pension_value_for_month(month),
                Ok(pension.pension_value_jan_to_jun),
                "month {month} should use the first-half Rentenwert"
            );
        }
        for month in 7_u8..=12 {
            assert_eq!(
                pension.pension_value_for_month(month),
                Ok(pension.pension_value_jul_to_dec),
                "month {month} should use the second-half Rentenwert"
            );
        }
    }

    #[test]
    fn an_out_of_range_month_is_refused_not_clamped() {
        let pension = SOCIAL_2026.pension;
        assert!(matches!(
            pension.pension_value_for_month(0),
            Err(MoneyError::OutOfDomain { .. })
        ));
        assert!(matches!(
            pension.pension_value_for_month(13),
            Err(MoneyError::OutOfDomain { .. })
        ));
    }

    /// The second half of one year must equal the first half of the next: the
    /// Rentenwert set on 1 July 2025 stays in force until 30 June 2026.
    #[test]
    fn the_pension_value_is_continuous_across_the_year_boundary() {
        assert_eq!(
            SOCIAL_2025.pension.pension_value_jul_to_dec,
            SOCIAL_2026.pension.pension_value_jan_to_jun,
            "the Rentenwert in force on 31 December must still be in force on 1 January"
        );
    }

    /// Verified against the DRV announcement of 5 March 2026: 40,79 € rising to
    /// 42,52 €, an increase of 4.24 %.
    #[test]
    fn the_2026_pension_adjustment_matches_the_published_figures() {
        let pension = SOCIAL_2026.pension;
        assert_eq!(pension.pension_value_jan_to_jun.cents(), 4_079);
        assert_eq!(pension.pension_value_jul_to_dec.cents(), 4_252);
    }

    /// Ceilings and the reference value rise with average wages. A year where any
    /// of them fell would mean a transcription error, not a policy change.
    #[test]
    fn ceilings_rise_from_2025_to_2026() {
        assert!(SOCIAL_2026.pension.ceiling_monthly > SOCIAL_2025.pension.ceiling_monthly);
        assert!(SOCIAL_2026.health.ceiling_monthly > SOCIAL_2025.health.ceiling_monthly);
        assert!(
            SOCIAL_2026.health.compulsory_insurance_threshold_annual
                > SOCIAL_2025.health.compulsory_insurance_threshold_annual
        );
        assert!(SOCIAL_2026.reference_value_monthly > SOCIAL_2025.reference_value_monthly);
        assert!(
            SOCIAL_2026.pension.average_earnings_annual
                > SOCIAL_2025.pension.average_earnings_annual
        );
    }

    /// The Jahresarbeitsentgeltgrenze must sit above the annualised health
    /// ceiling; the two are set together and this relationship is what makes the
    /// private-cover opt-out coherent.
    #[test]
    fn the_opt_out_threshold_exceeds_the_annualised_health_ceiling() {
        for p in every_year() {
            let annualised = p
                .health
                .ceiling_monthly
                .mul_int(12)
                .expect("twelve monthly ceilings are within the domain");
            assert!(
                p.health.compulsory_insurance_threshold_annual > annualised,
                "{}: JAEG {} is not above the annualised BBG {}",
                p.year.get(),
                p.health.compulsory_insurance_threshold_annual.cents(),
                annualised.cents()
            );
        }
    }

    /// § 55 Abs. 3 SGB XI reduces the rate by 0.25 points for each of the second
    /// through fifth child — four reductions, capped at 1.0 point in total — so the
    /// floor for a member with five or more children is 3.6 % − 1.0 % = **2.6 %**.
    ///
    /// A floor of 2.4 % is widely quoted in secondary sources and is wrong for any
    /// year from 2025 onward: it is the 2023–2024 figure, when the base rate was
    /// 3.4 %. The same four reductions applied to that older base gave 2.4 %. Both
    /// numbers are correct for their own year and blending them produces a rate
    /// 0.2 points too low, which is why this test pins the arithmetic to the base
    /// rate in the table rather than to a remembered constant.
    #[test]
    fn the_maximum_child_reduction_reaches_the_published_floor() {
        let care: CareInsurance = SOCIAL_2026.care;
        // Children 2..=5 attract a reduction: that is `max_ordinal - 1` of them.
        let reductions = i64::from(care.max_reduced_child_ordinal.saturating_sub(1));
        assert_eq!(
            reductions, 4,
            "the statute caps the reduction at four children"
        );

        let total_reduction =
            Rate::from_ppm(care.per_child_reduction.ppm().saturating_mul(reductions))
                .expect("four reductions stay within the rate domain");
        // § 55 Abs. 3: the total reduction may not exceed 1.0 percentage point.
        assert_eq!(
            total_reduction,
            Rate::from_percent_millis(1_000).expect("1.0 % is a valid rate")
        );

        let floor = care
            .base_rate
            .sub(total_reduction)
            .expect("the floor is a valid rate");
        assert_eq!(
            floor,
            Rate::from_percent_millis(2_600).expect("2.6 % is a valid rate"),
            "five or more children should reduce the 3.6 % base rate to 2.6 %"
        );
    }

    /// The full published rate ladder for 2026, from the childless surcharge down
    /// to the five-child floor. Pinning every rung catches an error in the
    /// reduction step that the endpoints alone would miss.
    #[test]
    fn the_care_rate_ladder_matches_the_published_table() {
        let care = SOCIAL_2026.care;
        // (children, expected rate in thousandths of a percent)
        let ladder = [
            (1_u8, 3_600_i64),
            (2, 3_350),
            (3, 3_100),
            (4, 2_850),
            (5, 2_600),
            (6, 2_600), // capped: a sixth child does not reduce it further
            (9, 2_600),
        ];
        for (children, expected_pct_milli) in ladder {
            let reductions = i64::from(children.min(care.max_reduced_child_ordinal))
                .saturating_sub(1)
                .max(0);
            let reduction =
                Rate::from_ppm(care.per_child_reduction.ppm().saturating_mul(reductions))
                    .expect("valid rate");
            let rate = care.base_rate.sub(reduction).expect("valid rate");
            assert_eq!(
                rate,
                Rate::from_percent_millis(expected_pct_milli).expect("valid rate"),
                "the rate for {children} children does not match the published table"
            );
        }
    }

    /// A childless member over 23 pays the base rate plus the full surcharge:
    /// 3.6 % + 0.6 % = 4.2 %.
    #[test]
    fn the_childless_rate_matches_the_published_total() {
        let care = SOCIAL_2026.care;
        let childless = care
            .base_rate
            .add(care.childless_surcharge)
            .expect("valid rate");
        assert_eq!(
            childless,
            Rate::from_percent_millis(4_200).expect("valid rate")
        );
    }

    /// The childless surcharge is employee-only, so a childless Saxon employee
    /// pays base/2 + 0.5 % + 0.6 %. Pinning the composite here documents the
    /// interaction the original specification got wrong.
    #[test]
    fn a_childless_saxon_employee_bears_the_documented_composite_rate() {
        let care = SOCIAL_2026.care;
        let half = care.base_rate.half().expect("half of 3.6 % is exact");
        let saxon = half
            .add(care.saxony_employee_surcharge)
            .expect("valid rate")
            .add(care.childless_surcharge)
            .expect("valid rate");
        // 1.8 % + 0.5 % + 0.6 % = 2.9 %
        assert_eq!(saxon, Rate::from_percent_millis(2_900).expect("valid rate"));
    }

    /// Pension and unemployment insurance are set to the same ceiling by each
    /// annual SVBezGrV. The two are stored separately (see
    /// [`UnemploymentInsurance`]), so this asserts the equality rather than
    /// assuming it — and would fail loudly on the day the legislature separates
    /// them, which is the outcome we want.
    #[test]
    fn unemployment_and_pension_share_a_ceiling() {
        for p in every_year() {
            assert_eq!(
                p.unemployment.ceiling_monthly,
                p.pension.ceiling_monthly,
                "{}: the unemployment and pension ceilings have diverged",
                p.year.get()
            );
        }
    }

    /// 2.6 % combined, so 1.3 % each side — and the halving must be exact, or the
    /// two shares would not reconstruct the whole.
    #[test]
    fn the_unemployment_rate_halves_exactly() {
        for p in every_year() {
            let half = p
                .unemployment
                .contribution_rate
                .half()
                .expect("half of 2.6 % is a valid rate");
            assert_eq!(
                half,
                Rate::from_percent_millis(1_300).expect("valid rate"),
                "{}: the employee share is not 1.3 %",
                p.year.get()
            );
            // Exactness: two halves must reconstitute the combined rate.
            assert_eq!(
                half.add(half).expect("valid rate"),
                p.unemployment.contribution_rate
            );
        }
    }

    #[test]
    fn every_branch_cites_a_primary_source_and_is_enacted() {
        for p in every_year() {
            for prov in [
                p.pension.provenance,
                p.unemployment.provenance,
                p.health.provenance,
                p.care.provenance,
            ] {
                assert!(
                    prov.source_url
                        .starts_with("https://www.gesetze-im-internet.de/"),
                    "{}: non-primary source {}",
                    p.year.get(),
                    prov.source_url
                );
                assert!(prov.legal_basis.contains("SGB"));
                assert_eq!(prov.verified_on.len(), "YYYY-MM-DD".len());
            }
            assert!(p.status().is_binding_law());
        }
    }

    /// A sanity bound on the ceiling: it should be a plausible monthly salary, not
    /// an annual figure accidentally stored in a monthly field. That mistake is
    /// easy to make and would inflate contributions twelvefold.
    #[test]
    fn monthly_fields_hold_monthly_magnitudes() {
        let plausible_max = Money::from_euro(20_000).expect("valid");
        for p in every_year() {
            assert!(p.pension.ceiling_monthly < plausible_max);
            assert!(p.health.ceiling_monthly < plausible_max);
            assert!(p.reference_value_monthly < plausible_max);
            // Conversely, the annual fields must be far larger than any monthly one.
            assert!(p.health.compulsory_insurance_threshold_annual > plausible_max);
            assert!(p.pension.average_earnings_annual > plausible_max);
        }
    }
}
