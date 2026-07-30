//! Retirement age and the Zugangsfaktor, SGB VI.
//!
//! # The Regelaltersgrenze is not 67
//!
//! It is *becoming* 67. § 235 Abs. 2 SGB VI raises it in two stages, and the
//! transition is still running: someone born in 1961 reaches it at 66 years and
//! 6 months, not 67. A simulator that assumes a flat 67 misdates the retirement of
//! everyone born before 1964 by up to two years, which moves the single most
//! consequential date in the whole projection.
//!
//! The two stages differ in slope, which is the part that is easy to get wrong:
//!
//! | Birth year | Regelaltersgrenze | Increment |
//! |---|---|---|
//! | 1946 and earlier | 65 y 0 m | — |
//! | 1947–1958 | 65 y 1 m … 66 y 0 m | **+1 month** per year |
//! | 1959–1963 | 66 y 2 m … 66 y 10 m | **+2 months** per year |
//! | 1964 and later | 67 y 0 m | — |
//!
//! Note that 1958 lands exactly on 66 y 0 m and 1959 resumes at 66 y 2 m, so the
//! two stages meet without a gap but with a visible kink. `retirement_age_months`
//! is written as a piecewise function over total months for exactly this reason:
//! expressed in years-and-months the arithmetic invites a carry bug at the join.
//!
//! # The Zugangsfaktor is asymmetric
//!
//! § 77 SGB VI reduces the pension by **0.3 % per month** claimed early and
//! increases it by **0.5 % per month** deferred past the Regelaltersgrenze. Those
//! are different numbers, and modelling them with one symmetric rate understates
//! the cost of retiring early or the reward for waiting. Both are stored.

use casivell_core::{MoneyError, Rate, TaxYear};

use crate::provenance::{DataStatus, Provenance};

/// Months in a year. Named so the constant never appears as a bare `12`.
pub const MONTHS_PER_YEAR: u32 = 12;

/// Parameters governing when a pension may be drawn and how early or late
/// drawing adjusts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetirementParameters {
    /// The year these parameters apply to.
    pub year: TaxYear,
    /// Reduction per month of early claim, § 77 Abs. 2 Satz 1 Nr. 2 SGB VI: 0.3 %.
    pub early_claim_reduction_per_month: Rate,
    /// Increase per month of deferral, § 77 Abs. 2 Satz 1 Nr. 2 SGB VI: 0.5 %.
    pub deferred_claim_increase_per_month: Rate,
    /// Rentenartfaktor for a Regelaltersrente, § 67 Nr. 1 SGB VI: 1.0.
    ///
    /// Present as a field rather than assumed, because other pension types carry
    /// different factors — 0.55 for a large widow's pension, for instance — and a
    /// hard-coded 1.0 would have to be found and removed when those arrive.
    pub old_age_pension_type_factor: Rate,
    /// The greatest number of months by which an Altersrente für langjährig
    /// Versicherte may be drawn early, § 236 SGB VI: 48.
    ///
    /// Used to reject implausible inputs rather than to compute anything. The
    /// binding limit in any real case is the individual's insurance record.
    pub max_early_claim_months: u32,
    /// Citation.
    pub provenance: Provenance,
}

impl RetirementParameters {
    /// Returns the parameters for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if no verified set exists.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        match year.get() {
            2025 | 2026 => Ok(RETIREMENT),
            other => Err(MoneyError::YearOutOfRange { year: other }),
        }
    }
}

/// The Regelaltersgrenze for someone born in `birth_year`, in whole months.
///
/// Returns months rather than a year-and-month pair so that callers comparing a
/// claim date against it do no carry arithmetic of their own. Divide by
/// [`MONTHS_PER_YEAR`] for the year part.
///
/// Total months, not an age offset: 780 is 65 years, 804 is 67 years.
///
/// # Panics
///
/// Never. The function is total over all of `u16`: years before the transition
/// return the pre-transition age and years after it return 67, which is what
/// § 235 provides.
#[must_use]
pub const fn retirement_age_months(birth_year: u16) -> u32 {
    // § 235 Abs. 2 Satz 1: 65 years for cohorts up to 1946.
    const BASE_65: u32 = 65 * MONTHS_PER_YEAR; // 780
    // The first stage ends at exactly 66 years for the 1958 cohort.
    const BASE_66: u32 = 66 * MONTHS_PER_YEAR; // 792
    // § 235 Abs. 2 Satz 2: 67 years for cohorts from 1964.
    const BASE_67: u32 = 67 * MONTHS_PER_YEAR; // 804

    if birth_year <= 1946 {
        return BASE_65;
    }
    if birth_year <= 1958 {
        // +1 month per cohort. 1947 → 781, 1958 → 792.
        let steps = (birth_year as u32).saturating_sub(1946);
        return BASE_65.saturating_add(steps);
    }
    if birth_year <= 1963 {
        // +2 months per cohort, resuming from 66 y 0 m. 1959 → 794, 1963 → 802.
        let steps = (birth_year as u32).saturating_sub(1958);
        return BASE_66.saturating_add(steps.saturating_mul(2));
    }
    BASE_67
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
        Err(_) => TaxYear::MIN,
    }
}

/// Retirement parameters. Unchanged between 2025 and 2026 — these are structural
/// provisions of SGB VI rather than annually-set figures.
const RETIREMENT: RetirementParameters = RetirementParameters {
    year: year(2026),
    early_claim_reduction_per_month: pct_milli(300),
    deferred_claim_increase_per_month: pct_milli(500),
    old_age_pension_type_factor: Rate::ONE,
    max_early_claim_months: 48,
    provenance: Provenance::new(
        "§ 77 SGB VI (Zugangsfaktor), § 67 SGB VI (Rentenartfaktor), § 235 SGB VI (Regelaltersgrenze)",
        "https://www.gesetze-im-internet.de/sgb_6/__77.html",
        "2026-07-30",
        DataStatus::Enacted,
    ),
};

#[cfg(test)]
mod tests {
    use super::{MONTHS_PER_YEAR, RETIREMENT, RetirementParameters, retirement_age_months};
    use casivell_core::{Rate, TaxYear};

    /// The published § 235 Abs. 2 table, cohort by cohort. Both stage slopes and
    /// both endpoints are pinned, because an error in either slope would still
    /// produce plausible-looking ages in the middle of its range.
    #[test]
    fn the_retirement_age_matches_the_published_table() {
        // (birth year, years, months)
        let table = [
            (1940_u16, 65_u32, 0_u32),
            (1946, 65, 0),
            (1947, 65, 1),
            (1950, 65, 4),
            (1958, 66, 0),
            (1959, 66, 2),
            (1960, 66, 4),
            (1961, 66, 6),
            (1962, 66, 8),
            (1963, 66, 10),
            (1964, 67, 0),
            (1980, 67, 0),
            (2010, 67, 0),
        ];
        for (birth_year, years, months) in table {
            let expected = years * MONTHS_PER_YEAR + months;
            assert_eq!(
                retirement_age_months(birth_year),
                expected,
                "cohort {birth_year} should retire at {years} y {months} m"
            );
        }
    }

    /// The two stages have different slopes but must still meet without a gap or a
    /// step: 1958 ends the first stage at exactly 66 y 0 m and 1959 begins the
    /// second at 66 y 2 m.
    #[test]
    fn the_two_transition_stages_join_without_a_gap() {
        assert_eq!(retirement_age_months(1958), 66 * MONTHS_PER_YEAR);
        assert_eq!(retirement_age_months(1959), 66 * MONTHS_PER_YEAR + 2);
        // The first stage advances one month at a time.
        for birth_year in 1947_u16..=1958 {
            let step = retirement_age_months(birth_year)
                .saturating_sub(retirement_age_months(birth_year.saturating_sub(1)));
            assert_eq!(step, 1, "cohort {birth_year} should advance by one month");
        }
        // The second stage advances two months at a time.
        for birth_year in 1959_u16..=1963 {
            let step = retirement_age_months(birth_year)
                .saturating_sub(retirement_age_months(birth_year.saturating_sub(1)));
            assert_eq!(step, 2, "cohort {birth_year} should advance by two months");
        }
    }

    /// Monotonic and bounded across every representable cohort. This is the
    /// property that catches an off-by-one at either boundary of either stage.
    #[test]
    fn the_retirement_age_is_monotonic_and_bounded() {
        let mut previous = 0_u32;
        for birth_year in 1900_u16..=2100 {
            let age = retirement_age_months(birth_year);
            assert!(
                age >= previous,
                "the retirement age fell for cohort {birth_year}"
            );
            assert!(
                (65 * MONTHS_PER_YEAR..=67 * MONTHS_PER_YEAR).contains(&age),
                "cohort {birth_year} got an implausible retirement age of {age} months"
            );
            previous = age;
        }
        // The transition is complete: the last cohort sits at the statutory 67.
        assert_eq!(retirement_age_months(2100), 67 * MONTHS_PER_YEAR);
    }

    /// § 77 SGB VI is asymmetric: 0.3 % per month early against 0.5 % per month
    /// deferred. Collapsing them into one rate is the error this pins.
    #[test]
    fn the_zugangsfaktor_adjustments_are_asymmetric() {
        assert_eq!(
            RETIREMENT.early_claim_reduction_per_month,
            Rate::from_percent_millis(300).expect("valid rate")
        );
        assert_eq!(
            RETIREMENT.deferred_claim_increase_per_month,
            Rate::from_percent_millis(500).expect("valid rate")
        );
        assert!(
            RETIREMENT.deferred_claim_increase_per_month.ppm()
                > RETIREMENT.early_claim_reduction_per_month.ppm(),
            "deferral must be rewarded more per month than early claim is penalised"
        );
    }

    /// A year of early claim costs 3.6 %, a year of deferral gains 6.0 %. Stated as
    /// the annualised figures because that is how the statute is usually quoted,
    /// and agreement between the two framings is worth checking.
    #[test]
    fn the_annualised_adjustments_match_the_quoted_figures() {
        let year_early = Rate::from_ppm(
            RETIREMENT
                .early_claim_reduction_per_month
                .ppm()
                .saturating_mul(i64::from(
                    u8::try_from(MONTHS_PER_YEAR).expect("12 fits in u8"),
                )),
        )
        .expect("valid rate");
        assert_eq!(
            year_early,
            Rate::from_percent_millis(3_600).expect("valid rate")
        );

        let year_deferred = Rate::from_ppm(
            RETIREMENT
                .deferred_claim_increase_per_month
                .ppm()
                .saturating_mul(12),
        )
        .expect("valid rate");
        assert_eq!(
            year_deferred,
            Rate::from_percent_millis(6_000).expect("valid rate")
        );
    }

    #[test]
    fn the_old_age_pension_type_factor_is_unity() {
        assert_eq!(RETIREMENT.old_age_pension_type_factor, Rate::ONE);
    }

    #[test]
    fn every_supported_year_has_retirement_parameters() {
        let mut y = TaxYear::MIN.get();
        while y <= TaxYear::MAX.get() {
            let tax_year = TaxYear::new(y).expect("in range");
            assert!(
                RetirementParameters::for_year(tax_year).is_ok(),
                "no retirement parameters for supported year {y}"
            );
            y = y.saturating_add(1);
        }
    }

    #[test]
    fn the_parameters_cite_a_primary_source() {
        let p = RETIREMENT.provenance;
        assert!(p.legal_basis.contains("§ 77 SGB VI"));
        assert!(p.legal_basis.contains("§ 235 SGB VI"));
        assert!(
            p.source_url
                .starts_with("https://www.gesetze-im-internet.de/")
        );
        assert!(p.status.is_binding_law());
    }
}
