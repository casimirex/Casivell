//! Evaluation of the § 32a EStG income tax tariff in exact integer arithmetic.
//!
//! # The algebra, and why it is written this way
//!
//! The statute is stated in decimal euro: `(914,51 · y + 1 400) · y`, with
//! `y = (x − 12 348) / 10 000`. Evaluating that literally in binary floating point
//! would introduce representation error in the coefficients before any arithmetic
//! happened. Instead the whole expression is cleared of denominators once, on
//! paper, and evaluated as a single integer quotient.
//!
//! Write `A`, `B`, `C` for the quadratic, linear and constant coefficients as the
//! statute prints them, `a = 100·A`, `b = 100·B`, `c = 100·C` for the scaled
//! integers actually stored, `S = 10 000` for the statutory divisor, and
//! `d = x − reference` for the whole-euro excess. Then
//!
//! ```text
//!   tax_euro  =  (A·(d/S) + B)·(d/S) + C
//!
//!             =  ( (a/100)·(d/S) + b/100 )·(d/S) + c/100
//!
//!             =  ( a·d + b·S )·d / (100·S²)  +  c/100
//!
//!             =  [ (a·d + b·S)·d + c·S² ] / (100·S²)
//!
//!   tax_cents =  [ (a·d + b·S)·d + c·S² ] / S²
//! ```
//!
//! and `S² = 10⁸`. That final line is `progression_tax_cents`, transcribed
//! directly. The proportional zones are simpler: `tax_euro = r·x − S_euro` with
//! `r` in parts per million becomes `tax_cents = x·ppm/10⁴ − S_cents`.
//!
//! # Overflow
//!
//! JPL Power-of-10 R2 asks for a provable upper bound on every quantity. The
//! bound here rests on the domain limits declared in `casivell-core`, and is
//! stated in full on `progression_tax_cents` and `proportional_tax_cents` (both
//! private; read them with `cargo doc --document-private-items`).
//! Both functions nonetheless use checked arithmetic: the proof establishes that
//! the error arm is unreachable, and the check ensures that if the proof is ever
//! invalidated by a change to the domain bounds the result is a returned error
//! rather than a silently wrapped number. A proof and a runtime check are not
//! redundant — the proof tells a reader the code is right, the check limits the
//! damage when the reader is wrong.
//!
//! # Rounding
//!
//! § 32a Abs. 1 Satz 2 requires the resulting tax to be truncated to whole euro
//! (*"auf den nächsten vollen Euro-Betrag abzurunden"*), and the taxable income
//! going in likewise. Both truncations are toward negative infinity. Because
//! flooring composes — `⌊⌊a/b⌋/c⌋ = ⌊a/(b·c)⌋` for positive `b`, `c` — computing
//! exact cents and then flooring to euro gives the same answer as flooring once,
//! so the intermediate cent value is safe to expose for inspection.

use casivell_core::{Money, MoneyError, Rate, Rounding, div_floor};
use casivell_lawdata::{IncomeTaxTariff, ProgressionZone, ProportionalZone};

/// Which zone of § 32a Abs. 1 produced a result.
///
/// Carried out of the calculation so the UI can explain *why* a figure is what it
/// is. An unexplainable number in a financial tool is a number the user cannot
/// check, and a tool whose arithmetic cannot be checked by its user is asking for
/// trust it has not earned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TariffZone {
    /// Zone 1: within the Grundfreibetrag, no tax.
    BasicAllowance,
    /// Zone 2: the first progression zone.
    FirstProgression,
    /// Zone 3: the second progression zone.
    SecondProgression,
    /// Zone 4: the 42 % band.
    UpperProportional,
    /// Zone 5: the 45 % band.
    TopProportional,
}

/// How a household is assessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilingStatus {
    /// Grundtarif, § 32a Abs. 1: the tariff applied directly.
    Individual,
    /// Splittingtarif, § 32a Abs. 5: twice the tax on half the joint income.
    ///
    /// The benefit this confers is the single largest lever in German household
    /// tax planning, and arises purely from the tariff's convexity.
    JointSplitting,
}

/// The outcome of applying the tariff, with enough detail to audit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assessment {
    /// The taxable income actually used, after truncation to whole euro.
    ///
    /// Reported because it may differ from the input by up to 99 cents, and a
    /// user reconciling against their Steuerbescheid needs to see the figure the
    /// tariff was fed.
    pub taxable_income: Money,
    /// The assessed income tax, a whole euro amount.
    pub income_tax: Money,
    /// For a joint assessment, the half-income the tariff was applied to.
    /// `None` for an individual assessment.
    pub split_base: Option<Money>,
    /// The zone that produced the figure. For a joint assessment this is the zone
    /// the *halved* income fell into.
    pub zone: TariffZone,
    /// How the household was assessed.
    pub filing: FilingStatus,
}

/// Computes income tax on `taxable_income` under `tariff`.
///
/// `taxable_income` is the *zu versteuerndes Einkommen*, already net of every
/// allowance and deduction. Determining it is not this function's job; see the
/// crate documentation.
///
/// A negative taxable income yields zero tax. The tariff is not defined below
/// zero, and a loss does not generate a refund through the tariff itself — it is
/// carried backward or forward under § 10d EStG, which is a separate mechanism.
///
/// # Errors
///
/// [`MoneyError::Overflow`] if the amount is so large that an intermediate leaves
/// the representable domain; [`MoneyError::OutOfDomain`] if `tariff` has
/// inconsistent zone boundaries, which the shipped tables are tested against.
pub fn income_tax(
    taxable_income: Money,
    tariff: &IncomeTaxTariff,
    filing: FilingStatus,
) -> Result<Assessment, MoneyError> {
    match filing {
        FilingStatus::Individual => {
            let x = taxable_income.floor_to_euro()?;
            let (tax, zone) = tariff_at(x, tariff)?;
            Ok(Assessment {
                taxable_income: x,
                income_tax: tax,
                split_base: None,
                zone,
                filing,
            })
        }
        FilingStatus::JointSplitting => {
            // § 32a Abs. 5: twice the tax on half the joint income. The halving
            // precedes the whole-euro truncation that Abs. 1 demands of its input,
            // so an odd euro of joint income is truncated away rather than
            // producing a half-euro base.
            let joint = taxable_income.floor_to_euro()?;
            let half = joint.div_int(2, Rounding::Floor)?.floor_to_euro()?;
            let (half_tax, zone) = tariff_at(half, tariff)?;
            // Doubling a whole-euro amount stays a whole-euro amount, so no
            // further truncation is required or permitted here.
            let tax = half_tax.mul_int(2)?;
            Ok(Assessment {
                taxable_income: joint,
                income_tax: tax,
                split_base: Some(half),
                zone,
                filing,
            })
        }
    }
}

/// Applies § 32a Abs. 1 to a whole-euro taxable income, returning the tax and the
/// zone that produced it.
///
/// Split out from [`income_tax`] so that the Splittingverfahren can reuse it
/// without recursion — JPL Power-of-10 R1 forbids recursion, and here the
/// iterative structure is also simply clearer.
fn tariff_at(x: Money, tariff: &IncomeTaxTariff) -> Result<(Money, TariffZone), MoneyError> {
    let euro = x.whole_euro_floor()?;

    // Zone 1. Also catches negative income: a loss attracts no tariff.
    if euro <= tariff.basic_allowance_euro {
        return Ok((Money::ZERO, TariffZone::BasicAllowance));
    }

    // Zones are tested from the top down so that the unbounded zone 5 is the
    // fallback and no income can escape the ladder. `validate` proves the zones
    // tile without gap, so exactly one arm can match.
    if euro >= tariff.top_proportional.lower_bound_euro {
        let cents = proportional_tax_cents(euro, &tariff.top_proportional)?;
        return Ok((to_whole_euro(cents)?, TariffZone::TopProportional));
    }
    if euro >= tariff.upper_proportional.lower_bound_euro {
        let cents = proportional_tax_cents(euro, &tariff.upper_proportional)?;
        return Ok((to_whole_euro(cents)?, TariffZone::UpperProportional));
    }
    if tariff.second_progression.contains(euro) {
        let cents = progression_tax_cents(euro, &tariff.second_progression)?;
        return Ok((to_whole_euro(cents)?, TariffZone::SecondProgression));
    }
    if tariff.first_progression.contains(euro) {
        let cents = progression_tax_cents(euro, &tariff.first_progression)?;
        return Ok((to_whole_euro(cents)?, TariffZone::FirstProgression));
    }

    // Unreachable for any tariff that passes `IncomeTaxTariff::validate`, which
    // every shipped table is tested against. Reported rather than asserted so
    // that a hand-built tariff cannot turn a data error into a panic.
    Err(MoneyError::OutOfDomain { cents: euro })
}

/// Evaluates a progression zone, returning tax in cents.
///
/// Implements `[(a·d + b·S)·d + c·S²] / S²` from the module documentation.
///
/// # Overflow bound
///
/// With `a ≤ 10⁵` (the largest coefficient the statute has ever printed is
/// `932,30`, stored as `93_230`), `b ≤ 10⁶`, `c ≤ 10⁶`, `S = 10⁴` and
/// `d ≤ 10⁵` (no progression zone has ever spanned more than about 52 000 €):
///
/// ```text
///   a·d       ≤ 10⁵ · 10⁵            = 10¹⁰
///   b·S       ≤ 10⁶ · 10⁴            = 10¹⁰
///   (a·d+b·S) ≤ 2·10¹⁰
///   ·d        ≤ 2·10¹⁰ · 10⁵         = 2·10¹⁵
///   c·S²      ≤ 10⁶ · 10⁸            = 10¹⁴
///   total     ≤ 2.1·10¹⁵  <  9.22·10¹⁸ = i64::MAX
/// ```
///
/// Three orders of magnitude of margin, and the bounds used are themselves
/// generous by a factor of ten against the actual tables.
fn progression_tax_cents(euro: i64, zone: &ProgressionZone) -> Result<i64, MoneyError> {
    let scale = ProgressionZone::SCALE_DIVISOR;
    let excess = euro
        .checked_sub(zone.reference_euro)
        .ok_or(MoneyError::Overflow)?;
    // The zone's own bounds guarantee a non-negative excess; a negative one means
    // the caller dispatched to the wrong zone.
    if excess < 0 {
        return Err(MoneyError::OutOfDomain { cents: excess });
    }

    let quadratic_term = zone
        .quadratic_centi
        .checked_mul(excess)
        .ok_or(MoneyError::Overflow)?;
    let linear_term = zone
        .linear_centi
        .checked_mul(scale)
        .ok_or(MoneyError::Overflow)?;
    let inner = quadratic_term
        .checked_add(linear_term)
        .ok_or(MoneyError::Overflow)?;
    let product = inner.checked_mul(excess).ok_or(MoneyError::Overflow)?;

    let scale_squared = scale.checked_mul(scale).ok_or(MoneyError::Overflow)?;
    let constant_term = zone
        .constant_centi
        .checked_mul(scale_squared)
        .ok_or(MoneyError::Overflow)?;
    let numerator = product
        .checked_add(constant_term)
        .ok_or(MoneyError::Overflow)?;

    div_floor(numerator, scale_squared)
}

/// Evaluates a proportional zone, returning tax in cents.
///
/// Implements `tax_cents = x·ppm/10⁴ − subtrahend_cents`.
///
/// # Overflow bound
///
/// `x` is bounded by `Money::MAX_ABS_CENTS / 100 = 10¹⁰` euro and `ppm` by
/// `Rate::MAX_ABS_PPM = 10⁷`, so the product is at most `10¹⁷`, comfortably
/// inside `i64`. The statutory rates are 4.2·10⁵ ppm and 4.5·10⁵ ppm, two orders
/// smaller again.
fn proportional_tax_cents(euro: i64, zone: &ProportionalZone) -> Result<i64, MoneyError> {
    // ppm per cent-of-a-euro: 10⁶ ppm per unit ÷ 100 cents per euro = 10⁴.
    let divisor = Rate::ONE
        .ppm()
        .checked_div(Money::CENTS_PER_EURO)
        .ok_or(MoneyError::Overflow)?;
    let scaled = euro
        .checked_mul(zone.marginal_rate.ppm())
        .ok_or(MoneyError::Overflow)?;
    let gross_cents = div_floor(scaled, divisor)?;
    gross_cents
        .checked_sub(zone.subtrahend_cents)
        .ok_or(MoneyError::Overflow)
}

/// Truncates a cent amount to whole euro and floors it at zero.
///
/// The floor at zero matters in the proportional zones: `0,42·x − 11 135,63` is
/// negative for small `x`, and although the zone bounds prevent that from being
/// reached through [`tariff_at`], a caller evaluating a zone directly should not
/// receive a negative tax.
fn to_whole_euro(cents: i64) -> Result<Money, MoneyError> {
    Ok(Money::from_cents(cents)?.floor_to_euro()?.floor_at_zero())
}

#[cfg(test)]
mod tests {
    use super::{Assessment, FilingStatus, TariffZone, income_tax};
    use casivell_core::{Money, TaxYear};
    use casivell_lawdata::IncomeTaxTariff;

    fn tariff(year: u16) -> IncomeTaxTariff {
        IncomeTaxTariff::for_year(TaxYear::new(year).unwrap()).unwrap()
    }

    fn tax_euro(zve_euro: i64, year: u16) -> i64 {
        let income = Money::from_euro(zve_euro).unwrap();
        let assessment = income_tax(income, &tariff(year), FilingStatus::Individual).unwrap();
        assessment.income_tax.whole_euro_floor().unwrap()
    }

    fn assess(zve_euro: i64, year: u16, filing: FilingStatus) -> Assessment {
        let income = Money::from_euro(zve_euro).unwrap();
        income_tax(income, &tariff(year), filing).unwrap()
    }

    // ---------------------------------------------------------------------
    // Zone 1: the Grundfreibetrag
    // ---------------------------------------------------------------------

    #[test]
    fn income_up_to_the_grundfreibetrag_is_untaxed() {
        assert_eq!(tax_euro(0, 2026), 0);
        assert_eq!(tax_euro(12_347, 2026), 0);
        assert_eq!(tax_euro(12_348, 2026), 0);
        // 2025's lower allowance means 12 347 € was taxable that year. If both
        // years returned zero the tariff would not be year-dependent at all.
        assert_eq!(tax_euro(12_096, 2025), 0);
        assert!(tax_euro(12_347, 2025) > 0);
    }

    #[test]
    fn the_first_taxable_euro_attracts_almost_no_tax() {
        // At the very bottom of zone 2 the quadratic term vanishes and the
        // marginal rate is the 14 % Eingangssteuersatz, so one euro over the
        // allowance rounds down to zero tax.
        assert_eq!(tax_euro(12_349, 2026), 0);
    }

    #[test]
    fn a_loss_attracts_no_tax_rather_than_a_negative_one() {
        let loss = Money::from_euro(-25_000).unwrap();
        let assessment = income_tax(loss, &tariff(2026), FilingStatus::Individual).unwrap();
        assert_eq!(assessment.income_tax, Money::ZERO);
        assert_eq!(assessment.zone, TariffZone::BasicAllowance);
    }

    // ---------------------------------------------------------------------
    // Zone boundaries and continuity
    // ---------------------------------------------------------------------

    /// The statute's coefficients are chosen so the four zone formulas meet. A
    /// discontinuity of more than a euro at any join means a coefficient is
    /// mistranscribed — this is the single most valuable test in the crate,
    /// because it checks the *data* using the *statute's own* internal
    /// consistency rather than against a figure copied from the same place.
    #[test]
    fn the_zones_join_continuously() {
        for year in [2025_u16, 2026] {
            let t = tariff(year);
            let joins = [
                t.first_progression.lower_bound_euro,
                t.second_progression.lower_bound_euro,
                t.upper_proportional.lower_bound_euro,
                t.top_proportional.lower_bound_euro,
            ];
            for boundary in joins {
                let below = tax_euro(boundary.saturating_sub(1), year);
                let at = tax_euro(boundary, year);
                let step = at.saturating_sub(below);
                assert!(
                    (0..=1).contains(&step),
                    "{year}: tax jumps by {step} € at the {boundary} € boundary"
                );
            }
        }
    }

    /// Verified against the published Eckwerte: at the top of zone 3 the tariff
    /// must agree with zone 4's line to within the rounding of a single euro of
    /// income.
    #[test]
    fn the_forty_two_percent_threshold_matches_the_published_eckwert() {
        // 2026: zone 4 is 0,42·x − 11 135,63, first applying at 69 879 €.
        let at_threshold = tax_euro(69_879, 2026);
        let expected = (69_879_i64 * 42 - 1_113_563) / 100;
        assert_eq!(at_threshold, expected);
    }

    #[test]
    fn the_forty_five_percent_threshold_matches_the_published_eckwert() {
        let at_threshold = tax_euro(277_826, 2026);
        let expected = (277_826_i64 * 45 - 1_947_038) / 100;
        assert_eq!(at_threshold, expected);
    }

    #[test]
    fn the_reported_zone_matches_the_income() {
        assert_eq!(
            assess(10_000, 2026, FilingStatus::Individual).zone,
            TariffZone::BasicAllowance
        );
        assert_eq!(
            assess(15_000, 2026, FilingStatus::Individual).zone,
            TariffZone::FirstProgression
        );
        assert_eq!(
            assess(40_000, 2026, FilingStatus::Individual).zone,
            TariffZone::SecondProgression
        );
        assert_eq!(
            assess(100_000, 2026, FilingStatus::Individual).zone,
            TariffZone::UpperProportional
        );
        assert_eq!(
            assess(500_000, 2026, FilingStatus::Individual).zone,
            TariffZone::TopProportional
        );
    }

    // ---------------------------------------------------------------------
    // Monotonicity and marginal rates: properties, not point values
    // ---------------------------------------------------------------------

    /// Tax must never fall as income rises. A single inversion would mean a
    /// taxpayer could reduce their liability by earning more, which is both
    /// legally impossible and a sure sign of a sign error.
    #[test]
    fn tax_is_monotonically_non_decreasing_in_income() {
        for year in [2025_u16, 2026] {
            let mut previous = 0_i64;
            let mut zve = 0_i64;
            // Step finely through the progression zones where the curvature is,
            // then coarsely through the linear region.
            while zve <= 300_000 {
                let tax = tax_euro(zve, year);
                assert!(
                    tax >= previous,
                    "{year}: tax fell from {previous} € to {tax} € at a zvE of {zve} €"
                );
                previous = tax;
                zve = zve.saturating_add(if zve < 80_000 { 17 } else { 971 });
            }
        }
    }

    /// The marginal rate must stay inside the statutory band: never below zero,
    /// never above the 45 % top rate. Checked euro by euro across both
    /// progression zones, where the rate is a moving target.
    #[test]
    fn the_marginal_rate_never_leaves_the_statutory_band() {
        for year in [2025_u16, 2026] {
            let t = tariff(year);
            let start = t.basic_allowance_euro;
            let end = t.second_progression.upper_bound_euro;
            let mut zve = start;
            while zve < end {
                let next = zve.saturating_add(100);
                let marginal = tax_euro(next, year).saturating_sub(tax_euro(zve, year));
                // 100 € of extra income can attract at most 45 € of extra tax.
                assert!(
                    (0..=45).contains(&marginal),
                    "{year}: 100 € above {zve} € attracted {marginal} € of tax"
                );
                zve = next;
            }
        }
    }

    /// The average rate can never reach the top marginal rate, because the lower
    /// zones are always traversed first. This catches a whole class of error where
    /// a zone formula is applied to the full income instead of the excess.
    #[test]
    fn the_average_rate_stays_below_the_top_marginal_rate() {
        for year in [2025_u16, 2026] {
            for zve in [
                13_000_i64, 20_000, 45_000, 70_000, 150_000, 400_000, 1_000_000,
            ] {
                let tax = tax_euro(zve, year);
                assert!(
                    tax * 100 < zve * 45,
                    "{year}: average rate at {zve} € reached the 45 % top rate"
                );
            }
        }
    }

    // ---------------------------------------------------------------------
    // Splittingverfahren, § 32a Abs. 5
    // ---------------------------------------------------------------------

    /// The splitting benefit is exactly zero when both partners would be taxed
    /// identically anyway, and strictly positive otherwise. This is the property
    /// the whole German household-tax-planning question turns on.
    #[test]
    fn splitting_never_costs_more_than_individual_assessment() {
        for joint in [0_i64, 20_000, 24_696, 50_000, 100_000, 300_000, 600_000] {
            let split = assess(joint, 2026, FilingStatus::JointSplitting);
            let single = assess(joint, 2026, FilingStatus::Individual);
            assert!(
                split.income_tax <= single.income_tax,
                "splitting a joint zvE of {joint} € produced more tax than a single assessment"
            );
        }
    }

    /// Twice the Grundfreibetrag is the exact point at which a couple's joint
    /// income first becomes taxable at all.
    #[test]
    fn a_couple_is_untaxed_up_to_twice_the_grundfreibetrag() {
        let t = tariff(2026);
        let doubled = t.basic_allowance_euro.saturating_mul(2);
        let at = assess(doubled, 2026, FilingStatus::JointSplitting);
        assert_eq!(at.income_tax, Money::ZERO);
        assert_eq!(
            at.split_base,
            Some(Money::from_euro(t.basic_allowance_euro).unwrap())
        );
    }

    /// Within a single linear zone the tariff is affine, `T(x) = r·x − S`, so
    ///
    /// ```text
    ///   2·T(x/2) = 2·(r·x/2 − S) = r·x − 2S = T(x) − S
    /// ```
    ///
    /// and the splitting benefit saturates at exactly the subtrahend `S`.
    ///
    /// The identity requires **both** the full income and its half to lie in the
    /// *same* zone. That is the real constraint: at a joint zvE of 300 000 € the
    /// individual assessment is already in zone 5 while the half is still in
    /// zone 4, the two zones have different `r` and `S`, and the benefit is a
    /// larger 11 801 € rather than the subtrahend. So the test picks an income
    /// where the precondition genuinely holds — and asserts that it holds, rather
    /// than trusting the choice to stay valid if the Eckwerte move.
    #[test]
    fn the_splitting_benefit_saturates_at_the_subtrahend() {
        let t = tariff(2026);
        // 200 000 € joint halves to 100 000 €; zone 4 spans 69 879 €–277 825 €, so
        // both figures fall inside it.
        let joint = 200_000_i64;
        let single_assessment = assess(joint, 2026, FilingStatus::Individual);
        let split_assessment = assess(joint, 2026, FilingStatus::JointSplitting);
        assert_eq!(
            single_assessment.zone,
            TariffZone::UpperProportional,
            "precondition: the full income must be in the 42 % zone"
        );
        assert_eq!(
            split_assessment.zone,
            TariffZone::UpperProportional,
            "precondition: the half income must also be in the 42 % zone"
        );

        let benefit = single_assessment
            .income_tax
            .sub(split_assessment.income_tax)
            .unwrap()
            .whole_euro_floor()
            .unwrap();
        let subtrahend_euro = t.upper_proportional.subtrahend_cents / Money::CENTS_PER_EURO;
        // Equal to the subtrahend, give or take the euro-truncation applied at each
        // of the two evaluation steps.
        assert!(
            (benefit - subtrahend_euro).abs() <= 2,
            "splitting benefit {benefit} € differs from the expected {subtrahend_euro} €"
        );
    }

    /// Where the full income has reached zone 5 but the half is still in zone 4,
    /// the benefit exceeds the zone-4 subtrahend, because the household also
    /// escapes the 45 % rate entirely. This is the largest splitting advantage the
    /// tariff offers, and it is worth pinning as a distinct case from the affine
    /// one above.
    #[test]
    fn the_splitting_benefit_is_larger_when_it_escapes_the_top_rate() {
        let t = tariff(2026);
        let joint = 300_000_i64;
        let single_assessment = assess(joint, 2026, FilingStatus::Individual);
        let split_assessment = assess(joint, 2026, FilingStatus::JointSplitting);
        assert_eq!(single_assessment.zone, TariffZone::TopProportional);
        assert_eq!(split_assessment.zone, TariffZone::UpperProportional);

        let benefit = single_assessment
            .income_tax
            .sub(split_assessment.income_tax)
            .unwrap()
            .whole_euro_floor()
            .unwrap();
        let subtrahend_euro = t.upper_proportional.subtrahend_cents / Money::CENTS_PER_EURO;
        assert!(
            benefit > subtrahend_euro,
            "escaping the top rate should beat the affine saturation: {benefit} € vs {subtrahend_euro} €"
        );
        // 0,45·300 000 − 19 470,38 = 115 529,62 → 115 529 € single.
        // 2·(0,42·150 000 − 11 135,63) = 2·51 864 = 103 728 € joint.
        assert_eq!(benefit, 11_801);
    }

    #[test]
    fn splitting_reports_the_half_income_it_used() {
        let a = assess(100_001, 2026, FilingStatus::JointSplitting);
        // 100 001 € halves to 50 000,50 €, truncated to 50 000 €.
        assert_eq!(a.split_base, Some(Money::from_euro(50_000).unwrap()));
        assert_eq!(a.taxable_income, Money::from_euro(100_001).unwrap());
    }

    /// A joint assessment's tax is twice a whole-euro figure, hence always even.
    /// A stray extra rounding step would break this.
    #[test]
    fn joint_tax_is_always_an_even_number_of_euro() {
        for joint in [30_001_i64, 47_777, 88_889, 250_003] {
            let tax = assess(joint, 2026, FilingStatus::JointSplitting)
                .income_tax
                .whole_euro_floor()
                .unwrap();
            assert_eq!(tax % 2, 0, "joint tax on {joint} € was an odd {tax} €");
        }
    }

    // ---------------------------------------------------------------------
    // Rounding and reporting
    // ---------------------------------------------------------------------

    /// § 32a Abs. 1 truncates the taxable income to whole euro before applying the
    /// tariff, so every cent below the next euro yields the same tax.
    #[test]
    fn cents_of_taxable_income_are_truncated_away() {
        let base = tax_euro(40_000, 2026);
        for cents in [1_u8, 50, 99] {
            let with_cents = Money::from_euro_cents(40_000, cents).unwrap();
            let assessed = income_tax(with_cents, &tariff(2026), FilingStatus::Individual).unwrap();
            assert_eq!(assessed.income_tax.whole_euro_floor().unwrap(), base);
            // The reported taxable income shows what was actually used.
            assert_eq!(assessed.taxable_income, Money::from_euro(40_000).unwrap());
        }
    }

    /// The assessed tax is always a whole euro amount, never carrying cents.
    #[test]
    fn assessed_tax_carries_no_cents() {
        for zve in [13_000_i64, 17_800, 40_000, 69_879, 300_000] {
            for filing in [FilingStatus::Individual, FilingStatus::JointSplitting] {
                let tax = assess(zve, 2026, filing).income_tax;
                assert_eq!(
                    tax.cents() % Money::CENTS_PER_EURO,
                    0,
                    "tax on {zve} € carried cents: {}",
                    tax.cents()
                );
            }
        }
    }

    /// A very large income must produce an answer rather than an overflow, since
    /// the overflow proof claims the domain is safe.
    #[test]
    fn the_top_of_the_domain_does_not_overflow() {
        let large = Money::from_euro(1_000_000_000).unwrap();
        let assessed = income_tax(large, &tariff(2026), FilingStatus::Individual);
        assert!(
            assessed.is_ok(),
            "a billion euro of income overflowed: {assessed:?}"
        );
        let assessed = income_tax(large, &tariff(2026), FilingStatus::JointSplitting);
        assert!(
            assessed.is_ok(),
            "a billion euro of joint income overflowed: {assessed:?}"
        );
    }

    // ---------------------------------------------------------------------
    // Cross-check against an independent implementation
    // ---------------------------------------------------------------------

    /// Reference values produced by an independent implementation of § 32a — the
    /// statute evaluated with arbitrary-precision decimals rather than the cleared
    /// integer form this crate uses. The two derivations share only the statutory
    /// coefficients, so agreement across the whole curve is evidence that the
    /// algebra in the module documentation was cleared correctly, which no amount
    /// of internal consistency checking could establish on its own.
    ///
    /// The generating script is `docs/reference/generate_tariff_reference.py`.
    /// These figures must **never** be regenerated from this crate's own output;
    /// doing so would turn the cross-check into a tautology.
    #[test]
    fn the_integer_evaluation_agrees_with_an_independent_decimal_one() {
        // (zvE in euro, assessed income tax in euro), individual assessment, 2026.
        let reference = [
            (15_000_i64, 435_i64),
            (20_000, 1_570),
            (25_000, 2_850),
            (30_000, 4_217),
            (40_000, 7_209),
            (45_000, 8_835),
            (60_000, 14_233),
            (70_000, 18_264),
            (100_000, 30_864),
            (300_000, 115_529),
            (400_000, 160_529),
        ];
        for (zve, expected) in reference {
            assert_eq!(
                tax_euro(zve, 2026),
                expected,
                "the tariff disagrees with the decimal reference at a zvE of {zve} €"
            );
        }
    }
}
