//! Elterngeld parameters: the BEEG.
//!
//! # A benefit defined by a formula rather than a table
//!
//! Most German benefits are stated as amounts. Elterngeld is stated as a *calculation*: § 2
//! BEEG replaces a share of the pre-birth net income, and the share itself slides with that
//! income. There is no table of amounts to transcribe — which means there is also no table to
//! check against, and the verification has to come from reproducing the formula's own
//! published examples and its stated boundary values.
//!
//! # The pre-birth net is a stylised figure, not the real one
//!
//! §§ 2c to 2f define an *Elterngeld-Netto* that deliberately does not equal what the payslip
//! showed. Two simplifications matter:
//!
//! **Social contributions are flat percentages with no ceilings.** § 2f sets 9 % for health
//! and care, 10 % for pensions and 2 % for unemployment, and § 2f Abs. 3 says in terms that
//! "andere Maßgaben zur Bestimmung der sozialversicherungsrechtlichen
//! Beitragsbemessungsgrundlagen werden nicht berücksichtigt" — the Beitragsbemessungsgrenzen
//! are disregarded. A high earner's stylised deduction is therefore far larger than their real
//! one, which is a real feature of the statute and not an approximation made here.
//!
//! **Tax comes from the Programmablaufplan.** § 2e computes the tax deduction with the PAP in
//! force on 1 January of the year before the birth. That is a stroke of luck for this
//! repository: the PAP is the best-verified thing in it, checked against 516 official values,
//! so the largest deduction in the Elterngeld calculation runs through code that is already
//! known to be right.
//!
//! # What is not here
//!
//! The eligibility rules. Whether someone is entitled at all — residence, the child living in
//! the household, working hours during the reference period — is a question about facts a
//! household simulator does not hold. The parameters below describe *how much*, given that
//! someone qualifies, and the caller asserts the qualification.

use casivell_core::{Money, MoneyError, Rate, TaxYear};

use crate::provenance::{DataStatus, Provenance};

/// Parameters for computing Elterngeld under the BEEG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElterngeldParameters {
    /// The year these parameters apply to.
    pub year: TaxYear,

    /// § 2 Abs. 1 Satz 1: the base replacement rate, 67 %.
    pub base_rate: Rate,
    /// § 2 Abs. 2 Satz 2: the floor the rate slides down to for higher incomes, 65 %.
    pub floor_rate: Rate,
    /// § 2 Abs. 2 Satz 1: the ceiling it slides up to for lower incomes, 100 %.
    pub ceiling_rate: Rate,

    /// § 2 Abs. 2 Satz 1: below this monthly net the rate rises.
    pub lower_income_threshold: Money,
    /// § 2 Abs. 2 Satz 2: above this monthly net the rate falls.
    ///
    /// Note the gap: between the two thresholds the rate is flat at 67 %. A single threshold
    /// would be simpler and wrong.
    pub upper_income_threshold: Money,
    /// The rate moves by this much …
    pub rate_step: Rate,
    /// … for every this much of income past the threshold. Two euro.
    pub rate_step_income: Money,

    /// § 2 Abs. 3 Satz 2: the pre-birth income is capped at this for the difference
    /// calculation when the beneficiary earns during the Bezugszeitraum.
    ///
    /// 2 770 € × 65 % = 1 800,50, which is why the cap and the maximum agree to fifty cents
    /// rather than exactly — the maximum then binds.
    pub difference_income_cap: Money,

    /// § 2 Abs. 4: the minimum monthly Basiselterngeld, paid even on no prior income.
    pub minimum_monthly: Money,
    /// § 2 Abs. 1 Satz 2: the maximum monthly Basiselterngeld.
    pub maximum_monthly: Money,

    /// § 2a Abs. 1: the Geschwisterbonus, 10 % …
    pub sibling_bonus_rate: Rate,
    /// … but at least 75 €.
    pub sibling_bonus_minimum: Money,
    /// § 2a Abs. 4: the Mehrlingszuschlag, per additional child of a multiple birth.
    pub multiple_birth_supplement: Money,

    /// § 2f Abs. 1 Nr. 1: the flat health and care deduction.
    pub social_health_care_rate: Rate,
    /// § 2f Abs. 1 Nr. 2: the flat pension deduction.
    pub social_pension_rate: Rate,
    /// § 2f Abs. 1 Nr. 3: the flat unemployment deduction.
    pub social_unemployment_rate: Rate,

    /// § 1 Abs. 8: the zu versteuerndes Einkommen above which entitlement lapses entirely.
    ///
    /// A cliff, not a taper: one euro over and the whole benefit is gone. Unified at 175 000 €
    /// for couples and single parents alike for births from 1 April 2025.
    pub income_limit_annual: Money,

    /// § 4 Abs. 3: months of Basiselterngeld the parents share.
    pub base_months: u8,
    /// § 4 Abs. 3: the two further months available when both parents take leave.
    pub partner_months: u8,
    /// § 4 Abs. 3: months of `ElterngeldPlus` per month of Basiselterngeld given up.
    pub plus_months_per_base_month: u8,

    /// Citation.
    pub provenance: Provenance,
}

impl ElterngeldParameters {
    /// Returns the parameters for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if no verified set exists.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        match year.get() {
            2025 => Ok(ELTERNGELD_2025),
            2026 => Ok(ELTERNGELD_2026),
            other => Err(MoneyError::YearOutOfRange { year: other }),
        }
    }

    /// The combined flat social deduction of § 2f: 9 % + 10 % + 2 % = 21 %.
    ///
    /// Summed here rather than at the call site so the three components stay individually
    /// cited and individually testable, while the arithmetic happens once.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] on a domain violation.
    pub const fn social_deduction_rate(&self) -> Result<Rate, MoneyError> {
        let partial = match self.social_health_care_rate.add(self.social_pension_rate) {
            Ok(r) => r,
            Err(e) => return Err(e),
        };
        partial.add(self.social_unemployment_rate)
    }

    /// The most months of Basiselterngeld two parents can draw between them.
    #[must_use]
    pub const fn maximum_base_months(&self) -> u8 {
        self.base_months.saturating_add(self.partner_months)
    }

    /// The monthly net at which the rate reaches its floor.
    ///
    /// Derived rather than stored: it follows from the threshold, the step and the distance
    /// between the base and floor rates, and a stored figure could disagree with them.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] on a domain violation.
    pub const fn floor_rate_income(&self) -> Result<Money, MoneyError> {
        let span = match self.base_rate.sub(self.floor_rate) {
            Ok(r) => r,
            Err(e) => return Err(e),
        };
        let steps = match div_ppm(span.ppm(), self.rate_step.ppm()) {
            Ok(n) => n,
            Err(e) => return Err(e),
        };
        let distance = match self.rate_step_income.mul_int(steps) {
            Ok(m) => m,
            Err(e) => return Err(e),
        };
        self.upper_income_threshold.add(distance)
    }

    /// The monthly net at which the rate reaches its ceiling.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] on a domain violation.
    pub const fn ceiling_rate_income(&self) -> Result<Money, MoneyError> {
        let span = match self.ceiling_rate.sub(self.base_rate) {
            Ok(r) => r,
            Err(e) => return Err(e),
        };
        let steps = match div_ppm(span.ppm(), self.rate_step.ppm()) {
            Ok(n) => n,
            Err(e) => return Err(e),
        };
        let distance = match self.rate_step_income.mul_int(steps) {
            Ok(m) => m,
            Err(e) => return Err(e),
        };
        self.lower_income_threshold.sub(distance)
    }
}

/// Whole division of two ppm figures, refusing a zero divisor.
///
/// `div_trunc` rather than a bare `/`: the workspace denies raw arithmetic operators so that
/// every division states its rounding and its failure mode rather than inheriting Rust's.
const fn div_ppm(numerator: i64, denominator: i64) -> Result<i64, MoneyError> {
    casivell_core::div_trunc(numerator, denominator)
}

const fn euro(whole: i64) -> Money {
    match Money::from_euro(whole) {
        Ok(m) => m,
        Err(_) => Money::ZERO,
    }
}

const fn pct_milli(percent_millis: i64) -> Rate {
    match Rate::from_percent_millis(percent_millis) {
        Ok(r) => r,
        Err(_) => Rate::ZERO,
    }
}

const fn year(value: u16) -> TaxYear {
    match TaxYear::new(value) {
        Ok(y) => y,
        Err(_) => TaxYear::LAST_VERIFIED,
    }
}

/// The parameters shared by both enacted years.
///
/// Every figure in the BEEG has been unchanged across 2025 and 2026 — the 175 000 € limit
/// arrived on 1 April 2025 and the amounts have not moved since 2007. Written once rather
/// than twice so the two years cannot silently drift apart, which for identical data is the
/// stronger arrangement.
const fn beeg_for(applicable: TaxYear, verified_on: &'static str) -> ElterngeldParameters {
    ElterngeldParameters {
        year: applicable,

        base_rate: pct_milli(67_000),
        floor_rate: pct_milli(65_000),
        ceiling_rate: pct_milli(100_000),

        lower_income_threshold: euro(1_000),
        upper_income_threshold: euro(1_200),
        rate_step: pct_milli(100),
        rate_step_income: euro(2),

        difference_income_cap: euro(2_770),
        minimum_monthly: euro(300),
        maximum_monthly: euro(1_800),

        sibling_bonus_rate: pct_milli(10_000),
        sibling_bonus_minimum: euro(75),
        multiple_birth_supplement: euro(300),

        social_health_care_rate: pct_milli(9_000),
        social_pension_rate: pct_milli(10_000),
        social_unemployment_rate: pct_milli(2_000),

        income_limit_annual: euro(175_000),

        base_months: 12,
        partner_months: 2,
        plus_months_per_base_month: 2,

        provenance: Provenance::new(
            "§§ 1 Abs. 8, 2, 2a, 2c-2f, 4 BEEG",
            "https://www.gesetze-im-internet.de/beeg/",
            verified_on,
            DataStatus::Enacted,
        ),
    }
}

/// Elterngeld parameters for 2025.
///
/// The 175 000 € limit applies to births **from 1 April 2025**. Births before that date fell
/// under the earlier 200 000 € (couples) / 150 000 € (single parents) limits, which this table
/// does not carry: the limit turns on the child's date of birth rather than the tax year, so a
/// year-keyed table cannot express both. Stated here rather than silently applying the wrong
/// one to an early-2025 birth.
const ELTERNGELD_2025: ElterngeldParameters = beeg_for(year(2025), "2026-07-31");

/// Elterngeld parameters for 2026.
const ELTERNGELD_2026: ElterngeldParameters = beeg_for(year(2026), "2026-07-31");

#[cfg(test)]
mod tests {
    use super::{ELTERNGELD_2025, ELTERNGELD_2026, ElterngeldParameters};
    use casivell_core::{Money, Rate, TaxYear};

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).expect("valid")
    }

    #[test]
    fn no_amount_or_rate_in_the_table_is_accidentally_zero() {
        let p = ELTERNGELD_2026;
        let amounts = [
            ("lower threshold", p.lower_income_threshold),
            ("upper threshold", p.upper_income_threshold),
            ("rate step income", p.rate_step_income),
            ("difference cap", p.difference_income_cap),
            ("minimum", p.minimum_monthly),
            ("maximum", p.maximum_monthly),
            ("sibling minimum", p.sibling_bonus_minimum),
            ("multiple birth", p.multiple_birth_supplement),
            ("income limit", p.income_limit_annual),
        ];
        for (name, amount) in amounts {
            assert!(
                !amount.is_zero() && !amount.is_negative(),
                "{name} is {} cents, so the literal was rejected",
                amount.cents()
            );
        }
        for rate in [
            p.base_rate,
            p.floor_rate,
            p.ceiling_rate,
            p.rate_step,
            p.sibling_bonus_rate,
            p.social_health_care_rate,
            p.social_pension_rate,
            p.social_unemployment_rate,
        ] {
            assert!(!rate.is_zero());
        }
        assert!(p.base_months > 0 && p.partner_months > 0);
    }

    /// The three rates must bracket correctly, or the sliding scale would run the wrong way.
    #[test]
    fn the_rates_are_ordered_as_the_statute_describes() {
        let p = ELTERNGELD_2026;
        assert!(p.floor_rate < p.base_rate);
        assert!(p.base_rate < p.ceiling_rate);
        assert_eq!(p.base_rate, Rate::from_percent_millis(67_000).unwrap());
        assert_eq!(p.floor_rate, Rate::from_percent_millis(65_000).unwrap());
        assert_eq!(p.ceiling_rate, Rate::ONE, "100 % is a full replacement");
    }

    /// The two thresholds must leave a flat band between them. Collapsing them into one would
    /// change the benefit for every income between 1 000 € and 1 200 €.
    #[test]
    fn there_is_a_flat_band_between_the_thresholds() {
        let p = ELTERNGELD_2026;
        assert!(p.lower_income_threshold < p.upper_income_threshold);
        assert_eq!(p.lower_income_threshold, euro(1_000));
        assert_eq!(p.upper_income_threshold, euro(1_200));
    }

    /// The derived boundary incomes must come out where the statute's arithmetic puts them:
    /// the 65 % floor at 1 240 € and the 100 % ceiling at 340 €.
    ///
    /// Derived from the step rather than stored, so this checks the parameters against each
    /// other. From 67 % to 65 % is 2 points, which at 0,1 points per 2 € is 40 € above 1 200 €;
    /// from 67 % to 100 % is 33 points, which is 660 € below 1 000 €.
    #[test]
    fn the_derived_boundary_incomes_match_the_statutes_arithmetic() {
        let p = ELTERNGELD_2026;
        assert_eq!(p.floor_rate_income().expect("in domain"), euro(1_240));
        assert_eq!(p.ceiling_rate_income().expect("in domain"), euro(340));
    }

    /// § 2f: the three flat rates must sum to 21 %, which is the figure the whole stylised
    /// net rests on.
    #[test]
    fn the_flat_social_deduction_is_twenty_one_percent() {
        assert_eq!(
            ELTERNGELD_2026.social_deduction_rate().expect("in domain"),
            Rate::from_percent_millis(21_000).unwrap()
        );
    }

    /// § 4 Abs. 3: twelve months plus two partner months.
    #[test]
    fn the_durations_match_the_statute() {
        let p = ELTERNGELD_2026;
        assert_eq!(p.base_months, 12);
        assert_eq!(p.partner_months, 2);
        assert_eq!(p.maximum_base_months(), 14);
        assert_eq!(p.plus_months_per_base_month, 2);
    }

    /// The maximum is reached at a stylised net of 1 800 / 65 % ≈ 2 769,24 €, which is the
    /// point beyond which more income buys no more benefit. Asserted as a relationship
    /// between the stored figures rather than as a transcribed number.
    #[test]
    fn the_maximum_binds_above_the_floor_rate_income() {
        let p = ELTERNGELD_2026;
        // At the income where the rate reaches its floor, the benefit is still well under
        // the cap — so the cap binds somewhere strictly above, in the flat-rate region.
        let at_floor = p
            .floor_rate_income()
            .expect("in domain")
            .mul_rate(p.floor_rate, casivell_core::Rounding::HalfUp)
            .expect("in domain");
        assert!(
            at_floor < p.maximum_monthly,
            "the cap should not already bind where the rate bottoms out"
        );
        assert_eq!(p.maximum_monthly, euro(1_800));
        assert_eq!(p.minimum_monthly, euro(300));
    }

    /// Both enacted years must be identical. Every BEEG figure has been unchanged across
    /// them, and the shared constructor is what guarantees it — this asserts the guarantee
    /// holds rather than assuming it.
    #[test]
    fn the_two_enacted_years_agree_on_every_figure() {
        let (a, b) = (ELTERNGELD_2025, ELTERNGELD_2026);
        assert_eq!(a.base_rate, b.base_rate);
        assert_eq!(a.minimum_monthly, b.minimum_monthly);
        assert_eq!(a.maximum_monthly, b.maximum_monthly);
        assert_eq!(a.income_limit_annual, b.income_limit_annual);
        assert_eq!(a.social_deduction_rate(), b.social_deduction_rate());
        assert_eq!(a.maximum_base_months(), b.maximum_base_months());
        // Only the year they are stamped with differs.
        assert_ne!(a.year, b.year);
    }

    #[test]
    fn both_verified_years_are_available_and_others_are_refused() {
        assert!(ElterngeldParameters::for_year(TaxYear::new(2025).unwrap()).is_ok());
        assert!(ElterngeldParameters::for_year(TaxYear::new(2026).unwrap()).is_ok());
        assert!(ElterngeldParameters::for_year(TaxYear::new(2027).unwrap()).is_err());
    }

    #[test]
    fn the_parameters_cite_a_primary_source() {
        let p = ELTERNGELD_2026.provenance;
        assert!(p.legal_basis.contains("BEEG"));
        // Names the amount provisions, the stylised-net provisions and the duration one.
        assert!(p.legal_basis.contains("2a"));
        assert!(p.legal_basis.contains("2c-2f"));
        assert!(
            p.source_url
                .starts_with("https://www.gesetze-im-internet.de/")
        );
        assert!(p.status.is_binding_law());
    }
}
