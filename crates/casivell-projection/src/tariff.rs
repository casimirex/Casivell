//! Projecting the § 32a EStG tariff.
//!
//! # The coefficients are not free parameters
//!
//! § 32a prints nine numbers: a Grundfreibetrag, three further Eckwerte, and five
//! coefficients. Only the Eckwerte are policy. The coefficients follow, because the
//! statute fixes the *marginal rate* at each zone join:
//!
//! - `14 %` — the Eingangssteuersatz, at the Grundfreibetrag
//! - `23.97 %` — where zone 2 meets zone 3
//! - `42 %` — the Spitzensteuersatz, where zone 3 meets zone 4
//! - `45 %` — the Reichensteuersatz
//!
//! Write `G` for the Grundfreibetrag and `E₁`, `E₂`, `E₃` for the upper bounds of
//! zones 2, 3 and 4. Zone 2 is `(a·y + b)·y` with `y = (x − G)/10⁴`, so its marginal
//! rate is `(2a·y + b)/10⁴`. Evaluating at `x = G` gives `b/10⁴ = 0.14`, hence
//! `b = 1400` — which is why 1 400 appears in every published version of the statute.
//! Evaluating at `x = E₁` and setting it to 23.97 %:
//!
//! ```text
//!   (2a·(E₁−G)/10⁴ + 1400) / 10⁴ = 0.2397
//!                              a = 4 985 000 / (E₁ − G)
//! ```
//!
//! The same argument on zone 3, whose linear coefficient `d = 2397` for the same
//! reason, gives
//!
//! ```text
//!                              c = 9 015 000 / (E₂ − E₁)
//! ```
//!
//! and the remaining constants follow from requiring the zones to *meet*:
//!
//! ```text
//!   e  = zone2(E₁)                      the constant carried into zone 3
//!   S₄ = 0.42·E₂ − zone3(E₂)            so zone 4 joins zone 3
//!   S₅ = S₄ + 0.03·E₃                   so zone 5 joins zone 4
//! ```
//!
//! # Why this is trustworthy
//!
//! Applied to the *enacted* Eckwerte for 2025 and 2026, this derivation reproduces all
//! eight published coefficients exactly at the statute's two decimal places:
//!
//! | | derived | published |
//! |---|---|---|
//! | 2025 `a` | 932.2985 | **932.30** |
//! | 2025 `c` | 176.6366 | **176.64** |
//! | 2025 `e` | 1015.1280 | **1015.13** |
//! | 2025 `S₄` | 10911.9176 | **10911.92** |
//! | 2026 `a` | 914.5111 | **914.51** |
//! | 2026 `c` | 173.1024 | **173.10** |
//! | 2026 `e` | 1034.8724 | **1034.87** |
//! | 2026 `S₄` | 11135.6295 | **11135.63** |
//!
//! `the_derivation_reproduces_every_enacted_tariff` asserts this, so the method is
//! checked against real statutory output rather than against its own reasoning. A
//! derivation that reproduces two enacted years is a defensible basis for a third.
//!
//! # What is indexed and what is not
//!
//! `G`, `E₁` and `E₂` are indexed to price inflation. `E₃` — the 45 % threshold — is
//! **held constant**, because it has been 277 826 € since 2007 and no amending act has
//! touched it. That asymmetry is a real feature of German tax policy and modelling it
//! away would flatter the projection.
//!
//! It also has a consequence worth surfacing rather than hiding: indexed long enough,
//! `E₂` overtakes `E₃` and the tariff stops being well formed. That happens around
//! 2094 at 2 % inflation, and [`project_tariff`] refuses rather than returning
//! nonsense. The refusal is itself informative.

use casivell_core::{Money, MoneyError, Rate, TaxYear, div_round_half_up};
use casivell_lawdata::{IncomeTaxTariff, ProgressionZone, ProportionalZone, Provenance};

use crate::assumptions::Assumptions;
use crate::growth::compound_to_euro;
use crate::{ProjectionError, parameters};

/// `4 985 000`: the numerator fixing zone 2's quadratic coefficient.
///
/// `(0.2397 − 0.14) · 10⁴ · 10⁴ / 2`. Spelled as a named constant so the derivation in
/// the module documentation can be checked against the code without re-deriving it.
const ZONE_TWO_QUADRATIC_NUMERATOR: i64 = 4_985_000;

/// `9 015 000`: the same, for zone 3. `(0.42 − 0.2397) · 10⁴ · 10⁴ / 2`.
const ZONE_THREE_QUADRATIC_NUMERATOR: i64 = 9_015_000;

/// Projects the income tax tariff for `year`, `steps` years past the base.
///
/// # Errors
///
/// [`ProjectionError::TariffNoLongerCoherent`] if the indexed Eckwerte no longer
/// satisfy the tariff's structural invariants — see the module documentation.
/// [`ProjectionError::Arithmetic`] on a domain violation.
pub fn project_tariff(
    base: &IncomeTaxTariff,
    year: TaxYear,
    steps: u32,
    assumptions: &Assumptions,
) -> Result<IncomeTaxTariff, ProjectionError> {
    let inflation = assumptions.price_inflation();

    // Index the three policy Eckwerte; hold the 45 % threshold.
    let allowance = index_euro(base.basic_allowance_euro, inflation, steps)?;
    let zone_two_top = index_euro(base.first_progression.upper_bound_euro, inflation, steps)?;
    let zone_three_top = index_euro(base.second_progression.upper_bound_euro, inflation, steps)?;
    let top_threshold = base.top_proportional.lower_bound_euro;

    // The zones must remain strictly ordered, and the 42 % band must not be squeezed
    // out of existence by the unindexed 45 % threshold.
    let ordered = allowance < zone_two_top && zone_two_top < zone_three_top;
    if !ordered || zone_three_top >= top_threshold {
        return Err(ProjectionError::TariffNoLongerCoherent { year: year.get() });
    }

    let tariff = derive_tariff(
        year,
        allowance,
        zone_two_top,
        zone_three_top,
        base,
        parameters::projected_provenance(
            "§ 32a EStG, projected",
            base.provenance.source_url,
            assumptions,
        ),
    )?;

    // Defence in depth: the derivation is only sound if the result is a well-formed
    // tariff, and `validate` is the same check the enacted tables are held to.
    tariff.validate().map_err(ProjectionError::Arithmetic)?;
    Ok(tariff)
}

/// Builds a complete tariff from three Eckwerte, deriving every coefficient.
///
/// # Precision, and why it decides whether this works at all
///
/// The stored coefficients are rounded to two decimals, as the statute prints them. The
/// *dependent* constants — `e` and the two subtrahends — must nonetheless be derived from
/// the **unrounded** coefficients, because they are far more sensitive: rounding `c` from
/// `173.1024` to `173.10` first shifts `S₄` by seven cents, and the derivation then fails
/// to reproduce the enacted figure.
///
/// So the quadratic coefficients are carried internally at millionths of a euro, `e` and
/// the subtrahends are computed from those, and only the stored fields are rounded to
/// hundredths. That the result then matches all eight published constants exactly is what
/// makes the method credible rather than merely plausible.
///
/// The evaluation runs in `i128`. With a far-future Eckwert span the intermediate reaches
/// roughly `7 × 10¹⁸`, which fits `i64` only barely; widening removes a correctness cliff
/// at no cost in a routine that runs once per projected year rather than once per
/// simulated month.
///
/// # Errors
///
/// [`ProjectionError::Arithmetic`] on a domain violation.
pub(crate) fn derive_tariff(
    year: TaxYear,
    allowance_euro: i64,
    zone_two_top_euro: i64,
    zone_three_top_euro: i64,
    base: &IncomeTaxTariff,
    provenance: Provenance,
) -> Result<IncomeTaxTariff, ProjectionError> {
    let (first_progression, second_progression) = derive_progression_zones(
        allowance_euro,
        zone_two_top_euro,
        zone_three_top_euro,
        base.first_progression.linear_centi,
        base.second_progression.linear_centi,
    )?;

    // The tax at the very top of zone 3, which zone 4's line must meet.
    let zone_three_at_top = progression_cents(
        micro_from_centi_span(
            ZONE_THREE_QUADRATIC_NUMERATOR,
            zone_three_top_euro,
            zone_two_top_euro,
        )?,
        second_progression.linear_centi,
        second_progression.constant_centi,
        zone_three_top_euro
            .checked_sub(zone_two_top_euro)
            .ok_or(MoneyError::Overflow)?,
    )?;

    let (upper_subtrahend, top_subtrahend) = derive_subtrahends(
        zone_three_top_euro,
        zone_three_at_top,
        base.upper_proportional.marginal_rate,
        base.top_proportional.marginal_rate,
        base.top_proportional.lower_bound_euro,
    )?;

    Ok(IncomeTaxTariff {
        year,
        basic_allowance_euro: allowance_euro,
        first_progression,
        second_progression,
        upper_proportional: ProportionalZone {
            lower_bound_euro: zone_three_top_euro
                .checked_add(1)
                .ok_or(MoneyError::Overflow)?,
            marginal_rate: base.upper_proportional.marginal_rate,
            subtrahend_cents: upper_subtrahend,
        },
        top_proportional: ProportionalZone {
            lower_bound_euro: base.top_proportional.lower_bound_euro,
            marginal_rate: base.top_proportional.marginal_rate,
            subtrahend_cents: top_subtrahend,
        },
        provenance,
    })
}

/// Derives the two progressive zones from the Eckwerte.
///
/// The quadratic coefficients come from the fixed marginal rates at the zone joins; the
/// linear ones are fixed by the entry rates and carry over unchanged; and zone 3's
/// constant is the tax at the top of zone 2, evaluated from the *unrounded* coefficient.
fn derive_progression_zones(
    allowance_euro: i64,
    zone_two_top_euro: i64,
    zone_three_top_euro: i64,
    linear_two_centi: i64,
    linear_three_centi: i64,
) -> Result<(ProgressionZone, ProgressionZone), ProjectionError> {
    let zone_two_span = zone_two_top_euro
        .checked_sub(allowance_euro)
        .ok_or(MoneyError::Overflow)?;
    let quadratic_two_micro = scaled_quotient(ZONE_TWO_QUADRATIC_NUMERATOR, zone_two_span, MICRO)?;

    let zone_three_span = zone_three_top_euro
        .checked_sub(zone_two_top_euro)
        .ok_or(MoneyError::Overflow)?;
    let quadratic_three_micro =
        scaled_quotient(ZONE_THREE_QUADRATIC_NUMERATOR, zone_three_span, MICRO)?;

    let first = ProgressionZone {
        lower_bound_euro: allowance_euro.checked_add(1).ok_or(MoneyError::Overflow)?,
        upper_bound_euro: zone_two_top_euro,
        reference_euro: allowance_euro,
        quadratic_centi: micro_to_centi(quadratic_two_micro)?,
        linear_centi: linear_two_centi,
        constant_centi: 0,
    };

    // e = zone2(E1), from the unrounded a. See the function documentation on precision.
    let constant_three =
        progression_cents(quadratic_two_micro, linear_two_centi, 0, zone_two_span)?;

    let second = ProgressionZone {
        lower_bound_euro: zone_two_top_euro
            .checked_add(1)
            .ok_or(MoneyError::Overflow)?,
        upper_bound_euro: zone_three_top_euro,
        reference_euro: zone_two_top_euro,
        quadratic_centi: micro_to_centi(quadratic_three_micro)?,
        linear_centi: linear_three_centi,
        constant_centi: constant_three,
    };

    Ok((first, second))
}

/// The unrounded quadratic coefficient for a zone spanning `top − bottom`.
///
/// Recomputed rather than threaded through, so that every evaluation uses the unrounded
/// value and none accidentally picks up the rounded field.
fn micro_from_centi_span(numerator: i64, top: i64, bottom: i64) -> Result<i64, ProjectionError> {
    let span = top.checked_sub(bottom).ok_or(MoneyError::Overflow)?;
    scaled_quotient(numerator, span, MICRO)
}

/// The two subtrahends, derived by requiring each proportional zone to meet the one
/// below it.
///
/// ```text
///   S₄ = 0.42·E₂ − zone3(E₂)
///   S₅ = S₄ + (0.45 − 0.42)·(E₃ − 1)
/// ```
///
/// # The crossover is one euro below where zone 5 starts
///
/// `S₅` is pinned at the *top of zone 4* — the last income the 42 % line actually
/// applies to — not at `E₃` itself. Using `E₃` puts `S₅` three cents out, which is
/// precisely the discrepancy that revealed the boundary: 277 825 yields the published
/// 19 470,38 and 19 246,67, while 277 826 yields 19 470,41 and 19 246,70.
fn derive_subtrahends(
    zone_three_top_euro: i64,
    zone_three_at_top_cents: i64,
    upper_rate: Rate,
    top_rate: Rate,
    top_threshold_euro: i64,
) -> Result<(i64, i64), ProjectionError> {
    let upper_at_top = proportional_gross_cents(zone_three_top_euro, upper_rate)?;
    let upper_subtrahend = upper_at_top
        .checked_sub(zone_three_at_top_cents)
        .ok_or(MoneyError::Overflow)?;

    let upper_zone_top = top_threshold_euro
        .checked_sub(1)
        .ok_or(MoneyError::Overflow)?;
    let rate_step = top_rate
        .sub(upper_rate)
        .map_err(ProjectionError::Arithmetic)?;
    let step_at_threshold = proportional_gross_cents(upper_zone_top, rate_step)?;
    let top_subtrahend = upper_subtrahend
        .checked_add(step_at_threshold)
        .ok_or(MoneyError::Overflow)?;

    Ok((upper_subtrahend, top_subtrahend))
}

/// Millionths of a euro: the internal precision for the quadratic coefficients.
const MICRO: i64 = 1_000_000;

/// Narrows a millionths-of-a-euro coefficient to the hundredths the statute prints.
fn micro_to_centi(micro: i64) -> Result<i64, ProjectionError> {
    let per_centi = MICRO
        .checked_div(ProgressionZone::COEFFICIENT_SCALE)
        .ok_or(MoneyError::Overflow)
        .map_err(ProjectionError::Arithmetic)?;
    div_round_half_up(micro, per_centi).map_err(ProjectionError::Arithmetic)
}

/// Evaluates `(a·t + b)·t + c` in cents, with `a` in millionths of a euro, `b` and `c` in
/// hundredths, at `t = excess / 10⁴`.
///
/// Runs in `i128`; see [`derive_tariff`] for why.
fn progression_cents(
    quadratic_micro: i64,
    linear_centi: i64,
    constant_cents: i64,
    excess_euro: i64,
) -> Result<i64, ProjectionError> {
    let scale = i128::from(ProgressionZone::SCALE_DIVISOR);
    let centi = i128::from(ProgressionZone::COEFFICIENT_SCALE);
    let micro = i128::from(MICRO);
    let excess = i128::from(excess_euro);

    // Clearing denominators once, as in the module documentation:
    //   cents = [ (a_micro·e·100 + b_centi·10⁴·10⁶) · e ] / (10⁶ · 10⁸)
    let quadratic = i128::from(quadratic_micro)
        .saturating_mul(excess)
        .saturating_mul(centi);
    let linear = i128::from(linear_centi)
        .saturating_mul(scale)
        .saturating_mul(micro);
    let product = quadratic.saturating_add(linear).saturating_mul(excess);
    let denominator = micro.saturating_mul(scale).saturating_mul(scale);

    let quotient = div_round_half_up_i128(product, denominator)?;
    quotient
        .checked_add(constant_cents)
        .ok_or(MoneyError::Overflow)
        .map_err(ProjectionError::Arithmetic)
}

/// `n / d` rounded half away from zero, narrowed to `i64`.
fn div_round_half_up_i128(n: i128, d: i128) -> Result<i64, ProjectionError> {
    if d == 0 {
        return Err(ProjectionError::Arithmetic(MoneyError::DivisionByZero));
    }
    let (Some(q), Some(r)) = (n.checked_div(d), n.checked_rem(d)) else {
        return Err(ProjectionError::Arithmetic(MoneyError::Overflow));
    };
    // A zero remainder and a remainder below half both leave the quotient alone.
    let adjusted = if r == 0 || r.saturating_mul(2).unsigned_abs() < d.unsigned_abs() {
        q
    } else {
        let step = if (n < 0) == (d < 0) { 1 } else { -1 };
        q.checked_add(step)
            .ok_or(MoneyError::Overflow)
            .map_err(ProjectionError::Arithmetic)?
    };
    i64::try_from(adjusted).map_err(|_| ProjectionError::Arithmetic(MoneyError::Overflow))
}

/// Indexes a whole-euro Eckwert forward.
fn index_euro(euro: i64, rate: Rate, steps: u32) -> Result<i64, ProjectionError> {
    let base = Money::from_euro(euro).map_err(ProjectionError::Arithmetic)?;
    let grown = compound_to_euro(base, rate, steps).map_err(ProjectionError::Arithmetic)?;
    grown
        .whole_euro_floor()
        .map_err(ProjectionError::Arithmetic)
}

/// `numerator / denominator`, scaled by `scale`, rounded half up.
///
/// Used for the quadratic coefficients, whose stored form is hundredths of a euro.
fn scaled_quotient(numerator: i64, denominator: i64, scale: i64) -> Result<i64, ProjectionError> {
    if denominator <= 0 {
        return Err(ProjectionError::Arithmetic(MoneyError::DivisionByZero));
    }
    let scaled = numerator
        .checked_mul(scale)
        .ok_or(MoneyError::Overflow)
        .map_err(ProjectionError::Arithmetic)?;
    div_round_half_up(scaled, denominator).map_err(ProjectionError::Arithmetic)
}

/// `rate · euro`, in cents.
fn proportional_gross_cents(euro: i64, rate: Rate) -> Result<i64, ProjectionError> {
    // ppm per cent-of-a-euro: 10⁶ ppm per unit / 100 cents per euro = 10⁴.
    let divisor = Rate::ONE
        .ppm()
        .checked_div(Money::CENTS_PER_EURO)
        .ok_or(MoneyError::Overflow)
        .map_err(ProjectionError::Arithmetic)?;
    let scaled = euro
        .checked_mul(rate.ppm())
        .ok_or(MoneyError::Overflow)
        .map_err(ProjectionError::Arithmetic)?;
    div_round_half_up(scaled, divisor).map_err(ProjectionError::Arithmetic)
}

#[cfg(test)]
mod tests {
    use super::{derive_tariff, project_tariff};
    use crate::assumptions::Assumptions;
    use crate::{ProjectionError, parameters};
    use casivell_core::TaxYear;
    use casivell_lawdata::IncomeTaxTariff;

    fn year(value: u16) -> TaxYear {
        TaxYear::new(value).expect("representable")
    }

    fn enacted(value: u16) -> IncomeTaxTariff {
        IncomeTaxTariff::for_year(year(value)).expect("enacted")
    }

    /// The validation that makes projection credible: the derivation, given only the
    /// enacted Eckwerte, must reproduce every published coefficient exactly.
    ///
    /// If this fails, the method is not a model of how § 32a is constructed and no
    /// projected tariff produced by it should be trusted.
    #[test]
    fn the_derivation_reproduces_every_enacted_tariff() {
        for value in [2025_u16, 2026] {
            let published = enacted(value);
            let assumptions = Assumptions::frozen();
            let derived = derive_tariff(
                year(value),
                published.basic_allowance_euro,
                published.first_progression.upper_bound_euro,
                published.second_progression.upper_bound_euro,
                &published,
                parameters::projected_provenance("test", "test", &assumptions),
            )
            .expect("derives");

            assert_eq!(
                derived.first_progression.quadratic_centi,
                published.first_progression.quadratic_centi,
                "{value}: zone 2 quadratic coefficient"
            );
            assert_eq!(
                derived.second_progression.quadratic_centi,
                published.second_progression.quadratic_centi,
                "{value}: zone 3 quadratic coefficient"
            );
            assert_eq!(
                derived.second_progression.constant_centi,
                published.second_progression.constant_centi,
                "{value}: zone 3 constant"
            );
            assert_eq!(
                derived.upper_proportional.subtrahend_cents,
                published.upper_proportional.subtrahend_cents,
                "{value}: 42 % subtrahend"
            );
            assert_eq!(
                derived.top_proportional.subtrahend_cents,
                published.top_proportional.subtrahend_cents,
                "{value}: 45 % subtrahend"
            );
            // And the zone boundaries must come back identical too.
            assert_eq!(
                derived.first_progression.lower_bound_euro,
                published.first_progression.lower_bound_euro
            );
            assert_eq!(
                derived.upper_proportional.lower_bound_euro,
                published.upper_proportional.lower_bound_euro
            );
        }
    }

    /// Every projected tariff must satisfy the same structural invariants the enacted
    /// tables are held to — zones tiling without gap or overlap.
    #[test]
    fn every_projected_tariff_is_structurally_valid() {
        let base = enacted(2026);
        let assumptions = Assumptions::default();
        for value in [2027_u16, 2030, 2040, 2060, 2080] {
            let steps = year(value).years_from(TaxYear::LAST_VERIFIED);
            let projected =
                project_tariff(&base, year(value), steps, &assumptions).expect("projects");
            assert!(
                projected.validate().is_ok(),
                "{value}: {:?}",
                projected.validate()
            );
        }
    }

    /// Frozen assumptions must leave the tariff identical to the enacted one, so that
    /// projection introduces nothing of its own when asked to change nothing.
    #[test]
    fn frozen_assumptions_reproduce_the_base_tariff() {
        let base = enacted(2026);
        let frozen = Assumptions::frozen();
        let projected = project_tariff(&base, year(2050), 24, &frozen).expect("projects");

        assert_eq!(projected.basic_allowance_euro, base.basic_allowance_euro);
        assert_eq!(projected.first_progression, base.first_progression);
        assert_eq!(projected.second_progression, base.second_progression);
        assert_eq!(
            projected.upper_proportional.subtrahend_cents,
            base.upper_proportional.subtrahend_cents
        );
        assert_eq!(
            projected.top_proportional.subtrahend_cents,
            base.top_proportional.subtrahend_cents
        );
    }

    /// Inflation raises the Grundfreibetrag and the 42 % threshold, and leaves the 45 %
    /// threshold alone. That asymmetry is a real feature of German policy.
    #[test]
    fn inflation_indexes_the_eckwerte_but_not_the_reichensteuer_threshold() {
        let base = enacted(2026);
        let assumptions = Assumptions::default();
        let projected = project_tariff(&base, year(2046), 20, &assumptions).expect("projects");

        assert!(projected.basic_allowance_euro > base.basic_allowance_euro);
        assert!(
            projected.upper_proportional.lower_bound_euro
                > base.upper_proportional.lower_bound_euro
        );
        assert_eq!(
            projected.top_proportional.lower_bound_euro, base.top_proportional.lower_bound_euro,
            "the 45 % threshold has not been indexed since 2007 and must not be here"
        );
    }

    /// Twenty years at 2 % is about 1.486x. The Grundfreibetrag should land near
    /// 18 350 EUR, which is a sanity check that the compounding is neither linear nor
    /// doubled.
    #[test]
    fn a_twenty_year_projection_lands_in_the_expected_range() {
        let base = enacted(2026);
        let projected =
            project_tariff(&base, year(2046), 20, &Assumptions::default()).expect("projects");
        let allowance = projected.basic_allowance_euro;
        assert!(
            (18_000..=18_700).contains(&allowance),
            "the projected Grundfreibetrag was {allowance}"
        );
    }

    /// Projected far enough, the indexed 42 % threshold overtakes the unindexed 45 %
    /// one and the tariff stops being well formed. That must be refused, not returned.
    ///
    /// The refusal is informative in itself: it is the model saying that leaving the
    /// Reichensteuer threshold unindexed cannot hold indefinitely.
    #[test]
    fn an_incoherent_far_future_tariff_is_refused() {
        let base = enacted(2026);
        let high_inflation = Assumptions::from_percent_millis(8_000, 8_000).expect("valid");
        // At 8 % a year, 69 878 EUR passes 277 826 EUR in about eighteen years.
        let result = project_tariff(&base, year(2060), 34, &high_inflation);
        assert!(
            matches!(result, Err(ProjectionError::TariffNoLongerCoherent { .. })),
            "expected a coherence refusal, got {result:?}"
        );
    }

    /// A projected tariff is never binding law, whatever the horizon.
    #[test]
    fn a_projected_tariff_never_claims_to_be_law() {
        let base = enacted(2026);
        let projected =
            project_tariff(&base, year(2030), 4, &Assumptions::default()).expect("projects");
        assert!(!projected.provenance.status.is_binding_law());
    }
}
