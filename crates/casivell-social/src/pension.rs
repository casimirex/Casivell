//! Statutory pension entitlement: Entgeltpunkte and the monthly pension.
//!
//! # The formula
//!
//! § 64 SGB VI gives the monthly pension as a product of four factors:
//!
//! ```text
//!   monthly pension = Entgeltpunkte × Zugangsfaktor × Rentenartfaktor × aktueller Rentenwert
//! ```
//!
//! Each factor has a distinct job, and conflating any two of them is a common
//! modelling error:
//!
//! - **Entgeltpunkte** (§ 63 Abs. 2, § 70) measure a career. One point is earned by
//!   contributing for a year on exactly the national average wage. Contributions on
//!   income above the Beitragsbemessungsgrenze earn nothing further.
//! - **Zugangsfaktor** (§ 77) adjusts for claiming early or late. It is
//!   **asymmetric**: −0.3 % per month early, +0.5 % per month deferred.
//! - **Rentenartfaktor** (§ 67) is 1.0 for an old-age pension and lower for
//!   survivors' pensions.
//! - **Aktueller Rentenwert** (§ 68) is the euro value of one point. It changes on
//!   **1 July**, not 1 January, so a calendar year has two values — see
//!   [`casivell_lawdata::PensionInsurance`].
//!
//! # Precision
//!
//! Entgeltpunkte are not whole numbers: a year at 50 000 € against a 51 944 €
//! average earns 0.962 575 points. [`EntgeltPoints`] stores millionths of a point,
//! which is two digits finer than the DRV's own four-decimal reporting, so
//! accumulating forty years of accruals cannot drift into the reported figure.

use casivell_core::{Money, MoneyError, Rate, div_floor, div_round_half_up};
use casivell_lawdata::{
    MONTHS_PER_YEAR, PensionInsurance, RetirementParameters, retirement_age_months,
};

/// A quantity of Entgeltpunkte, stored as millionths of a point.
///
/// Millionths rather than the DRV's four decimal places so that a forty-year sum of
/// monthly accruals stays exact to the precision anyone will ever compare against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct EntgeltPoints {
    micro: i64,
}

impl EntgeltPoints {
    /// Millionths of a point in one point.
    pub const MICRO_PER_POINT: i64 = 1_000_000;

    /// Zero points.
    pub const ZERO: Self = Self { micro: 0 };

    /// The most points this type will represent: 200.
    ///
    /// A full career at twice the contribution ceiling could not reach 100, so 200
    /// is generous while still bounding every downstream product (JPL R2). The
    /// overflow proof in [`monthly_pension`] depends on this bound.
    pub const MAX_MICRO: i64 = 200 * Self::MICRO_PER_POINT;

    /// Constructs a quantity from millionths of a point.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if outside `0..=200` points. Negative
    /// entitlement does not exist.
    pub const fn from_micro(micro: i64) -> Result<Self, MoneyError> {
        if micro < 0 || micro > Self::MAX_MICRO {
            return Err(MoneyError::OutOfDomain { cents: micro });
        }
        Ok(Self { micro })
    }

    /// Constructs a quantity from whole points.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if outside the representable range.
    pub const fn from_points(points: i64) -> Result<Self, MoneyError> {
        match points.checked_mul(Self::MICRO_PER_POINT) {
            Some(micro) => Self::from_micro(micro),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Returns the quantity in millionths of a point.
    #[must_use]
    pub const fn micro(self) -> i64 {
        self.micro
    }

    /// Adds two quantities.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if the sum exceeds the representable range,
    /// which would mean a career longer than any human one.
    pub const fn add(self, other: Self) -> Result<Self, MoneyError> {
        match self.micro.checked_add(other.micro) {
            Some(sum) => Self::from_micro(sum),
            None => Err(MoneyError::Overflow),
        }
    }

    /// The points earned by one year's contributory employment.
    ///
    /// `annual_income` is gross employment income for the year. Income above the
    /// Beitragsbemessungsgrenze is disregarded: it attracts no contribution and so
    /// earns no entitlement.
    ///
    /// # A caveat on the divisor
    ///
    /// The Durchschnittsentgelt for the current year is *provisional* (Anlage 1
    /// SGB VI gives it as "vorläufig") and is replaced by a final figure roughly
    /// two years later. Points computed for the current year will therefore shift
    /// slightly once the final value is published. This is inherent to the statute,
    /// not an approximation on our part, but a projection presenting current-year
    /// points as settled would be overstating its own certainty.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] if an intermediate leaves the representable domain, or
    /// [`MoneyError::DivisionByZero`] if the parameters carry a zero average wage.
    pub fn accrued_in_year(
        annual_income: Money,
        pension: &PensionInsurance,
    ) -> Result<Self, MoneyError> {
        let ceiling_annual = pension
            .ceiling_monthly
            .mul_int(i64::from(MONTHS_PER_YEAR))?;
        let contributory = annual_income.floor_at_zero().min(ceiling_annual);

        // points = contributory / average, carried in millionths.
        //
        // Overflow: contributory is bounded by the annual ceiling, about 1.014e7
        // cents, and MICRO_PER_POINT is 1e6, so the product is at most ~1.014e13 —
        // six orders inside i64::MAX.
        let scaled = contributory
            .cents()
            .checked_mul(Self::MICRO_PER_POINT)
            .ok_or(MoneyError::Overflow)?;
        let micro = div_round_half_up(scaled, pension.average_earnings_annual.cents())?;
        Self::from_micro(micro)
    }
}

/// The Zugangsfaktor for a pension claimed `months_offset` months from the
/// Regelaltersgrenze.
///
/// Negative `months_offset` means claiming early, positive means deferring. Zero
/// gives exactly 1.0.
///
/// # Errors
///
/// [`MoneyError::OutOfDomain`] if the early claim exceeds
/// [`RetirementParameters::max_early_claim_months`], or if the deferral is longer
/// than a decade — both indicate a caller error rather than a real scenario.
/// [`MoneyError`] on an arithmetic domain violation.
pub fn zugangsfaktor(
    months_offset: i32,
    params: &RetirementParameters,
) -> Result<Rate, MoneyError> {
    /// Deferring for more than ten years is not a scenario; it is a bug.
    const MAX_DEFERRAL_MONTHS: i32 = 120;

    if months_offset < 0 {
        let early = months_offset.unsigned_abs();
        if early > params.max_early_claim_months {
            return Err(MoneyError::OutOfDomain {
                cents: i64::from(early),
            });
        }
        let reduction = params
            .early_claim_reduction_per_month
            .ppm()
            .checked_mul(i64::from(early))
            .ok_or(MoneyError::Overflow)?;
        return Rate::from_ppm(
            Rate::ONE
                .ppm()
                .checked_sub(reduction)
                .ok_or(MoneyError::Overflow)?,
        );
    }

    if months_offset > MAX_DEFERRAL_MONTHS {
        return Err(MoneyError::OutOfDomain {
            cents: i64::from(months_offset),
        });
    }
    let increase = params
        .deferred_claim_increase_per_month
        .ppm()
        .checked_mul(i64::from(months_offset))
        .ok_or(MoneyError::Overflow)?;
    Rate::from_ppm(
        Rate::ONE
            .ppm()
            .checked_add(increase)
            .ok_or(MoneyError::Overflow)?,
    )
}

/// The Zugangsfaktor for someone born in `birth_year` claiming at
/// `claim_age_months`.
///
/// A convenience over [`zugangsfaktor`] that looks up the cohort's
/// Regelaltersgrenze, so callers do not compute the offset — and therefore cannot
/// get its sign backwards, which would turn a penalty into a bonus.
///
/// # Errors
///
/// As [`zugangsfaktor`].
pub fn zugangsfaktor_for_cohort(
    birth_year: u16,
    claim_age_months: u32,
    params: &RetirementParameters,
) -> Result<Rate, MoneyError> {
    let regular = retirement_age_months(birth_year);
    let offset = i64::from(claim_age_months)
        .checked_sub(i64::from(regular))
        .ok_or(MoneyError::Overflow)?;
    let offset = i32::try_from(offset).map_err(|_| MoneyError::Overflow)?;
    zugangsfaktor(offset, params)
}

/// The monthly pension for a given entitlement.
///
/// `pension_value` is the aktueller Rentenwert in force in the month being
/// computed — obtain it from [`PensionInsurance::pension_value_for_month`] rather
/// than assuming one value per year, because it changes on 1 July.
///
/// # Overflow
///
/// Applied in two steps rather than one four-way product, so that every
/// intermediate stays far inside `i64`:
///
/// ```text
///   points ≤ 2.0e8 micro, factors ≤ 1.0e7 ppm
///   step 1: 2.0e8 × 1.0e7 / 1e6 = 2.0e9          (adjusted points, twice)
///   step 2: 2.0e9 × ~5e3 cents / 1e6 = 1.0e7     (pension in cents)
///   worst intermediate: 2.0e9 × 5e3 = 1.0e13  ≪  9.22e18
/// ```
///
/// The single-product form would reach ~6e17 — still representable, but with three
/// orders less margin and no reason to prefer it.
///
/// # Errors
///
/// [`MoneyError`] if an intermediate leaves the representable domain.
pub fn monthly_pension(
    points: EntgeltPoints,
    access_factor: Rate,
    pension_type_factor: Rate,
    pension_value: Money,
) -> Result<Money, MoneyError> {
    let per_point = Rate::ONE.ppm();

    // Step 1: fold both dimensionless factors into the point total.
    let adjusted = apply_factor(points.micro(), access_factor, per_point)?;
    let adjusted = apply_factor(adjusted, pension_type_factor, per_point)?;

    // Step 2: convert points to money. Flooring to the cent: the DRV rounds the
    // monthly amount to two decimals, and truncation is the conservative direction
    // for a projection of one's own future income.
    let cents = adjusted
        .checked_mul(pension_value.cents())
        .ok_or(MoneyError::Overflow)?;
    Money::from_cents(div_floor(cents, EntgeltPoints::MICRO_PER_POINT)?)
}

/// Multiplies a scaled quantity by a dimensionless [`Rate`], keeping the scale.
fn apply_factor(scaled: i64, factor: Rate, per_unit: i64) -> Result<i64, MoneyError> {
    let product = scaled
        .checked_mul(factor.ppm())
        .ok_or(MoneyError::Overflow)?;
    div_round_half_up(product, per_unit)
}

#[cfg(test)]
mod tests {
    use super::{EntgeltPoints, monthly_pension, zugangsfaktor, zugangsfaktor_for_cohort};
    use casivell_core::{Money, MoneyError, Rate, TaxYear};
    use casivell_lawdata::{RetirementParameters, SocialParameters, retirement_age_months};

    fn pension_params(year: u16) -> casivell_lawdata::PensionInsurance {
        SocialParameters::for_year(TaxYear::new(year).unwrap())
            .unwrap()
            .pension
    }

    fn retirement() -> RetirementParameters {
        RetirementParameters::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn points_for(annual_euro: i64, year: u16) -> EntgeltPoints {
        EntgeltPoints::accrued_in_year(
            Money::from_euro(annual_euro).unwrap(),
            &pension_params(year),
        )
        .unwrap()
    }

    // ---------------------------------------------------------------------
    // Entgeltpunkte accrual
    // ---------------------------------------------------------------------

    /// Earning exactly the national average for a year earns exactly one point.
    /// This is the definition of the unit, and if it does not hold nothing else can.
    #[test]
    fn the_average_wage_earns_exactly_one_point() {
        for year in [2025_u16, 2026] {
            let average = pension_params(year).average_earnings_annual;
            let points = EntgeltPoints::accrued_in_year(average, &pension_params(year)).unwrap();
            assert_eq!(
                points.micro(),
                EntgeltPoints::MICRO_PER_POINT,
                "{year}: the average wage should earn exactly one point"
            );
        }
    }

    /// Half the average earns half a point; twice the average earns two — provided
    /// the ceiling does not bind first.
    #[test]
    fn points_scale_linearly_with_income() {
        let average = pension_params(2026).average_earnings_annual;
        let half = average.div_int(2, casivell_core::Rounding::Floor).unwrap();
        let points = EntgeltPoints::accrued_in_year(half, &pension_params(2026)).unwrap();
        // 25 972 € against 51 944 € is exactly 0.5.
        assert_eq!(points.micro(), 500_000);
    }

    /// 50 000 € against the 2026 average of 51 944 € is 0.962 575 points.
    #[test]
    fn a_concrete_accrual_matches_the_arithmetic() {
        let points = points_for(50_000, 2026);
        // 50 000 / 51 944 = 0.962 575 080 8... → 962 575 millionths, half-up.
        assert_eq!(points.micro(), 962_575);
    }

    /// Income above the Beitragsbemessungsgrenze earns no further points. A high
    /// earner's entitlement is capped, which is the counterpart of their capped
    /// contribution.
    #[test]
    fn income_above_the_ceiling_earns_no_further_points() {
        let p = pension_params(2026);
        let ceiling_annual = p.ceiling_monthly.mul_int(12).unwrap();
        let at_ceiling = EntgeltPoints::accrued_in_year(ceiling_annual, &p).unwrap();
        let far_above =
            EntgeltPoints::accrued_in_year(Money::from_euro(500_000).unwrap(), &p).unwrap();
        assert_eq!(at_ceiling, far_above);

        // 101 400 / 51 944 = 1.952 102 263... points, the annual maximum for 2026.
        assert_eq!(at_ceiling.micro(), 1_952_102);
    }

    #[test]
    fn no_income_earns_no_points() {
        let p = pension_params(2026);
        assert_eq!(
            EntgeltPoints::accrued_in_year(Money::ZERO, &p).unwrap(),
            EntgeltPoints::ZERO
        );
        // A loss cannot produce negative entitlement.
        assert_eq!(
            EntgeltPoints::accrued_in_year(Money::from_euro(-30_000).unwrap(), &p).unwrap(),
            EntgeltPoints::ZERO
        );
    }

    /// Forty years of accrual must not drift. Summing monthly-scale quantities is
    /// where a floating-point model would lose precision.
    #[test]
    fn accrual_accumulates_without_drift() {
        let p = pension_params(2026);
        let one_year = EntgeltPoints::accrued_in_year(p.average_earnings_annual, &p).unwrap();
        let mut total = EntgeltPoints::ZERO;
        for _ in 0..45 {
            total = total.add(one_year).unwrap();
        }
        // Exactly 45 points, the DRV's own "Standardrentner" benchmark.
        assert_eq!(total.micro(), 45 * EntgeltPoints::MICRO_PER_POINT);
    }

    #[test]
    fn negative_or_excessive_point_totals_are_refused() {
        assert!(matches!(
            EntgeltPoints::from_micro(-1),
            Err(MoneyError::OutOfDomain { .. })
        ));
        assert!(matches!(
            EntgeltPoints::from_micro(EntgeltPoints::MAX_MICRO + 1),
            Err(MoneyError::OutOfDomain { .. })
        ));
    }

    // ---------------------------------------------------------------------
    // Zugangsfaktor
    // ---------------------------------------------------------------------

    #[test]
    fn claiming_at_the_regular_age_gives_a_factor_of_one() {
        assert_eq!(zugangsfaktor(0, &retirement()).unwrap(), Rate::ONE);
    }

    /// −0.3 % per month, so twelve months early gives 0.964.
    #[test]
    fn early_claim_reduces_by_three_tenths_of_a_percent_per_month() {
        let one_month = zugangsfaktor(-1, &retirement()).unwrap();
        assert_eq!(one_month.ppm(), 997_000);

        let one_year = zugangsfaktor(-12, &retirement()).unwrap();
        assert_eq!(one_year.ppm(), 964_000);

        // The statutory maximum of 48 months: 1 − 0.144 = 0.856.
        let maximum = zugangsfaktor(-48, &retirement()).unwrap();
        assert_eq!(maximum.ppm(), 856_000);
    }

    /// +0.5 % per month, so twelve months deferred gives 1.06.
    #[test]
    fn deferral_increases_by_half_a_percent_per_month() {
        let one_month = zugangsfaktor(1, &retirement()).unwrap();
        assert_eq!(one_month.ppm(), 1_005_000);

        let one_year = zugangsfaktor(12, &retirement()).unwrap();
        assert_eq!(one_year.ppm(), 1_060_000);
    }

    /// The asymmetry is the point: a month deferred gains more than a month early
    /// costs. A symmetric model would misprice both directions.
    #[test]
    fn the_adjustment_is_asymmetric_in_the_two_directions() {
        let early = Rate::ONE.ppm() - zugangsfaktor(-12, &retirement()).unwrap().ppm();
        let late = zugangsfaktor(12, &retirement()).unwrap().ppm() - Rate::ONE.ppm();
        assert_eq!(early, 36_000); // 3.6 %
        assert_eq!(late, 60_000); // 6.0 %
        assert!(late > early);
    }

    #[test]
    fn an_impossible_early_claim_is_refused() {
        let params = retirement();
        let too_early = -(i32::try_from(params.max_early_claim_months).unwrap() + 1);
        assert!(matches!(
            zugangsfaktor(too_early, &params),
            Err(MoneyError::OutOfDomain { .. })
        ));
        // The statutory maximum itself is accepted.
        assert!(
            zugangsfaktor(
                -i32::try_from(params.max_early_claim_months).unwrap(),
                &params
            )
            .is_ok()
        );
    }

    #[test]
    fn an_absurd_deferral_is_refused() {
        assert!(matches!(
            zugangsfaktor(200, &retirement()),
            Err(MoneyError::OutOfDomain { .. })
        ));
    }

    /// The cohort helper must agree with the raw offset, and must get the sign
    /// right — the error that would turn a penalty into a bonus.
    #[test]
    fn the_cohort_helper_computes_the_offset_correctly() {
        let params = retirement();
        // A 1964 cohort reaches the Regelaltersgrenze at exactly 67 y 0 m.
        let regular = retirement_age_months(1964);
        assert_eq!(regular, 804);

        assert_eq!(
            zugangsfaktor_for_cohort(1964, regular, &params).unwrap(),
            Rate::ONE
        );
        // Claiming at 65 is 24 months early: 1 − 0.072 = 0.928.
        assert_eq!(
            zugangsfaktor_for_cohort(1964, 65 * 12, &params)
                .unwrap()
                .ppm(),
            928_000
        );
        // A 1961 cohort reaches it at 66 y 6 m, so claiming at 65 is only 18
        // months early: 1 − 0.054 = 0.946. Assuming a flat 67 would wrongly
        // charge 24 months.
        assert_eq!(retirement_age_months(1961), 798);
        assert_eq!(
            zugangsfaktor_for_cohort(1961, 65 * 12, &params)
                .unwrap()
                .ppm(),
            946_000
        );
    }

    /// The factor is monotonic in the claim date across the whole admissible range.
    #[test]
    fn the_factor_is_monotonic_in_the_claim_date() {
        let params = retirement();
        let mut previous = 0_i64;
        for offset in -48_i32..=120 {
            let factor = zugangsfaktor(offset, &params).unwrap();
            assert!(
                factor.ppm() > previous,
                "the factor did not increase at an offset of {offset} months"
            );
            previous = factor.ppm();
        }
    }

    // ---------------------------------------------------------------------
    // Monthly pension
    // ---------------------------------------------------------------------

    /// The DRV's own benchmark: the "Standardrentner" with 45 points, retiring at
    /// the regular age. From 1 July 2026 the Rentenwert is 42,52 €, so the standard
    /// pension is 45 × 42,52 € = 1 913,40 €.
    #[test]
    fn the_standard_pension_matches_the_published_benchmark() {
        let p = pension_params(2026);
        let points = EntgeltPoints::from_points(45).unwrap();
        let value = p.pension_value_for_month(7).unwrap();
        let pension = monthly_pension(points, Rate::ONE, Rate::ONE, value).unwrap();
        assert_eq!(pension.cents(), 191_340);
    }

    /// The published increase: the same 45 points yielded 1 835,55 € before 1 July
    /// 2026 and 1 913,40 € after, a rise of 77,85 €. The DRV announced exactly that
    /// figure, which makes this an external check on both Rentenwert values.
    #[test]
    fn the_mid_year_increase_matches_the_drv_announcement() {
        let p = pension_params(2026);
        let points = EntgeltPoints::from_points(45).unwrap();

        let june = monthly_pension(
            points,
            Rate::ONE,
            Rate::ONE,
            p.pension_value_for_month(6).unwrap(),
        )
        .unwrap();
        let july = monthly_pension(
            points,
            Rate::ONE,
            Rate::ONE,
            p.pension_value_for_month(7).unwrap(),
        )
        .unwrap();

        assert_eq!(june.cents(), 183_555);
        assert_eq!(july.cents(), 191_340);
        assert_eq!(july.sub(june).unwrap().cents(), 7_785);
    }

    /// Retiring three years early costs 10.8 % of the pension, permanently.
    #[test]
    fn early_retirement_reduces_the_pension_permanently() {
        let p = pension_params(2026);
        let points = EntgeltPoints::from_points(45).unwrap();
        let value = p.pension_value_for_month(7).unwrap();

        let full = monthly_pension(points, Rate::ONE, Rate::ONE, value).unwrap();
        let factor = zugangsfaktor(-36, &retirement()).unwrap();
        let reduced = monthly_pension(points, factor, Rate::ONE, value).unwrap();

        // 0.892 × 1 913,40 € = 1 706,75 €.
        assert_eq!(factor.ppm(), 892_000);
        assert_eq!(reduced.cents(), 170_675);
        assert!(reduced < full);
    }

    /// A zero entitlement or a zero Rentenwert yields no pension rather than an
    /// error.
    #[test]
    fn a_nil_entitlement_yields_no_pension() {
        let value = pension_params(2026).pension_value_for_month(1).unwrap();
        assert_eq!(
            monthly_pension(EntgeltPoints::ZERO, Rate::ONE, Rate::ONE, value).unwrap(),
            Money::ZERO
        );
    }

    /// The Rentenartfaktor scales the result: a large widow's pension is 55 %.
    #[test]
    fn the_pension_type_factor_scales_the_result() {
        let p = pension_params(2026);
        let points = EntgeltPoints::from_points(45).unwrap();
        let value = p.pension_value_for_month(7).unwrap();

        let old_age = monthly_pension(points, Rate::ONE, Rate::ONE, value).unwrap();
        let survivor = monthly_pension(
            points,
            Rate::ONE,
            Rate::from_percent_millis(55_000).unwrap(),
            value,
        )
        .unwrap();
        // 55 % of 1 913,40 € is 1 052,37 €.
        assert_eq!(survivor.cents(), 105_237);
        assert!(survivor < old_age);
    }

    /// Monotonic in points, and never negative, across the full domain.
    #[test]
    fn the_pension_is_monotonic_in_entitlement() {
        let value = pension_params(2026).pension_value_for_month(7).unwrap();
        let mut previous = Money::ZERO;
        let mut micro = 0_i64;
        while micro <= EntgeltPoints::MAX_MICRO {
            let points = EntgeltPoints::from_micro(micro).unwrap();
            let pension = monthly_pension(points, Rate::ONE, Rate::ONE, value).unwrap();
            assert!(!pension.is_negative());
            assert!(
                pension >= previous,
                "the pension fell at {micro} micropoints"
            );
            previous = pension;
            micro = micro.saturating_add(997_003);
        }
    }

    /// The extremes of the domain must not overflow, since the proof in
    /// `monthly_pension` claims they cannot.
    #[test]
    fn the_domain_corners_do_not_overflow() {
        let value = pension_params(2026).pension_value_for_month(7).unwrap();
        let max_points = EntgeltPoints::from_micro(EntgeltPoints::MAX_MICRO).unwrap();
        let max_factor = Rate::from_ppm(Rate::MAX_ABS_PPM).unwrap();
        assert!(monthly_pension(max_points, max_factor, max_factor, value).is_ok());
    }

    // ---------------------------------------------------------------------
    // End-to-end
    // ---------------------------------------------------------------------

    /// A full worked case: 40 years on 60 000 €, retiring two years early in the
    /// second half of 2026. Composed from the pieces to check they fit together.
    #[test]
    fn a_full_career_projection_composes() {
        let p = pension_params(2026);
        let yearly = points_for(60_000, 2026);

        let mut total = EntgeltPoints::ZERO;
        for _ in 0..40 {
            total = total.add(yearly).unwrap();
        }
        // 60 000 / 51 944 = 1.155 090 per year, so 46.203 600 over forty years.
        assert_eq!(yearly.micro(), 1_155_090);
        assert_eq!(total.micro(), 46_203_600);

        let factor = zugangsfaktor(-24, &retirement()).unwrap();
        assert_eq!(factor.ppm(), 928_000); // 1 − 0.072

        let pension = monthly_pension(
            total,
            factor,
            retirement().old_age_pension_type_factor,
            p.pension_value_for_month(7).unwrap(),
        )
        .unwrap();

        // 46.203 600 × 0.928 = 42.876 941 points, × 42,52 € = 1 823,127 5 €,
        // truncated to the cent.
        assert_eq!(pension.cents(), 182_312);
    }
}
