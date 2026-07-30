//! Vorsorgeaufwendungen: § 10 Abs. 1 Nr. 2, 3 and 3a EStG, with the Abs. 3 and 4 caps.
//!
//! # Two separate baskets, two separate caps
//!
//! The statute splits provision expenses in a way that is easy to flatten and wrong to:
//!
//! **Altersvorsorge** (Nr. 2) — statutory pension and Rürup contributions. Fully deductible
//! since 2023, capped by § 10 Abs. 3 at the maximum contribution to the *miners'* pension
//! scheme, which is 30 826 € for 2026. Both the employee's and the employer's contributions
//! count toward the cap, after which the tax-free employer share is subtracted again,
//! because it was never taxed in the first place:
//!
//! ```text
//!   deductible = min(employee + employer, cap) − employer
//! ```
//!
//! For an ordinary employee that reduces to their own contribution, since the cap is nearly
//! twice the maximum statutory contribution. It binds only on large voluntary contributions
//! — which is exactly the case someone consulting a planner about a Rürup policy is in.
//!
//! **Other provision expenses** (Nr. 3 and 3a) — health, long-term care, unemployment,
//! liability. Capped by § 10 Abs. 4 at **1 900 €** for an employee, and here the flattening
//! error is severe, because Nr. 3 has an override: basic health and care contributions are
//! **always fully deductible**, even far above the cap (§ 10 Abs. 4 Satz 4). So:
//!
//! ```text
//!   deductible = max( min(health + care + other, cap), health + care )
//! ```
//!
//! An employee's health and care contributions alone are several thousand euro, so the cap
//! is exceeded by the override in essentially every real case and the *other* expenses —
//! unemployment insurance, liability cover — end up deducting nothing at all. That is a
//! genuine and counter-intuitive feature of the statute: paying for liability insurance
//! reduces an ordinary employee's tax by exactly zero.
//!
//! # The 4 % reduction, and where it does not apply
//!
//! § 10 Abs. 1 Nr. 3 Buchst. a Satz 4 reduces a health contribution by 4 % where it can give
//! rise to a Krankengeld entitlement, on the reasoning that sick pay is not basic cover.
//!
//! It applies to the portion arising from the **general rate only**. The Zusatzbeitrag is
//! not reduced. Applying the 4 % to the whole contribution — the obvious simplification —
//! understates the deduction, and the error grows with the fund's supplementary rate.
//!
//! Long-term care carries no reduction: there is no Pflegegeld equivalent of Krankengeld.

use casivell_core::{Money, MoneyError, Rate, Rounding};
use casivell_lawdata::{DeductionParameters, SocialParameters};
use casivell_social::SocialContributions;

/// The deduction for provision expenses, with its parts exposed.
///
/// Exposed because the two baskets are capped separately and a user reconciling against a
/// Steuerbescheid needs to see which cap bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vorsorgeaufwendungen {
    /// § 10 Abs. 1 Nr. 2: the deductible retirement provision.
    pub retirement: Money,
    /// § 10 Abs. 1 Nr. 3 and 3a: the deductible other provision expenses.
    pub other: Money,
    /// The total deduction.
    pub total: Money,

    /// Whether the § 10 Abs. 3 retirement cap bound.
    ///
    /// Surfaced because it changes the answer to "should I contribute more", which is the
    /// question someone reading this figure is usually asking.
    pub retirement_cap_applied: bool,
    /// Whether the § 10 Abs. 4 Satz 4 override carried the other basket past its cap.
    ///
    /// True for essentially every employee, and worth showing precisely because it explains
    /// why additional insurance premiums deduct nothing.
    pub other_cap_overridden: bool,
}

/// The provision expenses a taxpayer actually paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contributions {
    /// The employee's own statutory pension contribution for the year.
    pub pension_employee: Money,
    /// The employer's statutory pension contribution, which counts toward the § 10 Abs. 3
    /// cap and is then subtracted again.
    pub pension_employer: Money,
    /// Voluntary Rürup or comparable basic retirement provision.
    pub retirement_voluntary: Money,

    /// The employee's health contribution arising from the *general* rate.
    ///
    /// Separate from the supplementary portion because only this one is reduced by 4 %.
    pub health_general: Money,
    /// The employee's health contribution arising from the fund's Zusatzbeitrag.
    pub health_supplementary: Money,
    /// The employee's long-term care contribution. No reduction applies.
    pub care: Money,

    /// Unemployment insurance and any other Nr. 3a expenses: liability, disability,
    /// term life.
    ///
    /// Almost always deducts nothing for an employee, because the Nr. 3 override has already
    /// carried the basket past its cap. Accepted anyway so the report can show that it
    /// deducted nothing rather than silently omitting it.
    pub other_provision: Money,
}

impl Contributions {
    /// Builds the contributions from a year's social insurance split.
    ///
    /// `months` scales a monthly split to the period — twelve for a full year.
    ///
    /// # Why the health contribution is re-derived rather than read
    ///
    /// [`SocialContributions`] carries one health figure, but § 10 needs the general-rate and
    /// supplementary portions separately, because only the former is reduced by 4 %. So the
    /// two are recomputed from the rates and the contribution base.
    ///
    /// That leaves a rounding difference of a few cents from the split's own aggregate: the
    /// split rounds the combined contribution once a month, while the portions round
    /// separately. `the_split_helper_reproduces_the_aggregate_health_contribution` bounds it.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] on a domain violation.
    pub fn from_social(
        contributions: &SocialContributions,
        social: &SocialParameters,
        supplementary_rate: Rate,
        months: i64,
    ) -> Result<Self, MoneyError> {
        let annual = |amount: Money| amount.mul_int(months);
        let annual_base = contributions.health_base.mul_int(months)?;

        // Half of each rate, since the employee bears half of both.
        let general_half = social.health.general_rate.half()?;
        let supplementary_half = supplementary_rate.half()?;

        Ok(Self {
            pension_employee: annual(contributions.pension.employee)?,
            pension_employer: annual(contributions.pension.employer)?,
            retirement_voluntary: Money::ZERO,
            health_general: annual_base.mul_rate(general_half, Rounding::HalfUp)?,
            health_supplementary: annual_base.mul_rate(supplementary_half, Rounding::HalfUp)?,
            care: annual(contributions.care.employee)?,
            other_provision: annual(contributions.unemployment.employee)?,
        })
    }
}

/// Computes the deduction for provision expenses.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn vorsorgeaufwendungen(
    contributions: &Contributions,
    deductions: &DeductionParameters,
) -> Result<Vorsorgeaufwendungen, MoneyError> {
    let retirement = retirement_provision(contributions, deductions)?;
    let other = other_provision(contributions, deductions)?;

    Ok(Vorsorgeaufwendungen {
        retirement: retirement.0,
        other: other.0,
        total: retirement.0.add(other.0)?,
        retirement_cap_applied: retirement.1,
        other_cap_overridden: other.1,
    })
}

/// § 10 Abs. 1 Nr. 2 with the Abs. 3 cap.
///
/// Returns the deductible amount and whether the cap bound.
fn retirement_provision(
    contributions: &Contributions,
    deductions: &DeductionParameters,
) -> Result<(Money, bool), MoneyError> {
    let cap = deductions.retirement_provision_cap()?;
    let gross = contributions
        .pension_employee
        .add(contributions.pension_employer)?
        .add(contributions.retirement_voluntary)?;

    let allowed = gross.min(cap);
    // The tax-free employer share is subtracted after the cap, because it was never taxed.
    // Flooring at zero matters for a taxpayer whose employer share alone exceeds the cap.
    let deductible = allowed.sub(contributions.pension_employer)?.floor_at_zero();
    Ok((deductible, gross > cap))
}

/// § 10 Abs. 1 Nr. 3 and 3a with the Abs. 4 cap and its Satz 4 override.
///
/// Returns the deductible amount and whether the override carried it past the cap.
fn other_provision(
    contributions: &Contributions,
    deductions: &DeductionParameters,
) -> Result<(Money, bool), MoneyError> {
    // The 4 % Krankengeld reduction applies to the general-rate portion only.
    let retained = Rate::ONE.sub(deductions.sick_pay_reduction)?;
    let health_general = contributions
        .health_general
        .mul_rate(retained, Rounding::HalfUp)?;

    // § 10 Abs. 1 Nr. 3: basic health and care cover. Always fully deductible.
    let basic = health_general
        .add(contributions.health_supplementary)?
        .add(contributions.care)?;

    let with_other = basic.add(contributions.other_provision)?;
    let capped = with_other.min(deductions.other_provision_cap_employee);

    // § 10 Abs. 4 Satz 4: where the basic cover alone exceeds the cap, it is deductible in
    // full and the cap does not apply.
    let deductible = capped.max(basic);
    Ok((deductible, basic > deductions.other_provision_cap_employee))
}

#[cfg(test)]
mod tests {
    use super::{Contributions, vorsorgeaufwendungen};
    use casivell_core::{Money, Rate, TaxYear};
    use casivell_lawdata::{DeductionParameters, SocialParameters};

    fn deductions() -> DeductionParameters {
        DeductionParameters::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn social() -> SocialParameters {
        SocialParameters::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    /// A typical employee on 54 000 EUR a year, with the contributions that follow from the
    /// 2026 rates.
    fn typical() -> Contributions {
        Contributions {
            // 18.6 % of 54 000 = 10 044, halved.
            pension_employee: euro(5_022),
            pension_employer: euro(5_022),
            retirement_voluntary: Money::ZERO,
            // 14.6 %/2 = 7.3 % of 54 000.
            health_general: euro(3_942),
            // 2.9 %/2 = 1.45 % of 54 000.
            health_supplementary: euro(783),
            // Childless: (1.8 + 0.6) % of 54 000.
            care: euro(1_296),
            // 2.6 %/2 = 1.3 % of 54 000.
            other_provision: euro(702),
        }
    }

    // ---------------------------------------------------------------------
    // Retirement provision, § 10 Abs. 1 Nr. 2 and Abs. 3
    // ---------------------------------------------------------------------

    /// Below the cap, the deduction is exactly the employee's own contribution: both shares
    /// count toward the cap and the employer's is then subtracted again.
    #[test]
    fn below_the_cap_the_employee_deducts_their_own_contribution() {
        let result = vorsorgeaufwendungen(&typical(), &deductions()).expect("computes");
        assert_eq!(result.retirement, euro(5_022));
        assert!(!result.retirement_cap_applied);
    }

    /// The cap binds only on large voluntary contributions. At the statutory maximum it does
    /// not, because 30 826 EUR is nearly twice the largest statutory contribution.
    #[test]
    fn the_cap_does_not_bind_on_statutory_contributions_alone() {
        let mut at_ceiling = typical();
        // 18.6 % of the 101 400 EUR ceiling is 18 860.40, halved.
        at_ceiling.pension_employee = Money::from_euro_cents(9_430, 20).unwrap();
        at_ceiling.pension_employer = Money::from_euro_cents(9_430, 20).unwrap();

        let result = vorsorgeaufwendungen(&at_ceiling, &deductions()).expect("computes");
        assert!(!result.retirement_cap_applied);
        assert_eq!(
            result.retirement,
            Money::from_euro_cents(9_430, 20).unwrap()
        );
    }

    /// A large Rürup contribution does hit the cap, and the deduction stops growing. This is
    /// the case the cap exists for.
    #[test]
    fn a_large_voluntary_contribution_hits_the_cap() {
        let mut generous = typical();
        generous.retirement_voluntary = euro(30_000);

        let result = vorsorgeaufwendungen(&generous, &deductions()).expect("computes");
        assert!(result.retirement_cap_applied);
        // cap 30 826 − employer 5 022 = 25 804.
        assert_eq!(result.retirement, euro(25_804));

        // And contributing yet more deducts nothing further.
        let mut more = generous;
        more.retirement_voluntary = euro(50_000);
        let capped = vorsorgeaufwendungen(&more, &deductions()).expect("computes");
        assert_eq!(capped.retirement, result.retirement);
    }

    // ---------------------------------------------------------------------
    // Other provision, § 10 Abs. 1 Nr. 3/3a and Abs. 4
    // ---------------------------------------------------------------------

    /// The 4 % reduction applies to the general-rate portion only. Applying it to the whole
    /// health contribution would understate the deduction, and the error grows with the
    /// fund's supplementary rate.
    #[test]
    fn the_sick_pay_reduction_spares_the_supplementary_contribution() {
        let result = vorsorgeaufwendungen(&typical(), &deductions()).expect("computes");
        // 3 942 × 0.96 = 3 784.32, plus 783 supplementary, plus 1 296 care = 5 863.32.
        assert_eq!(result.other, Money::from_euro_cents(5_863, 32).unwrap());

        // Reducing the whole basket by 4 % would have given 5 780.16 — over 80 EUR less.
        let naive = Money::from_euro_cents(3_942 + 783 + 1_296, 0)
            .unwrap()
            .mul_rate(
                Rate::from_percent_millis(96_000).unwrap(),
                casivell_core::Rounding::HalfUp,
            )
            .unwrap();
        assert!(result.other > naive);
    }

    /// The § 10 Abs. 4 Satz 4 override: basic health and care cover is fully deductible even
    /// though it far exceeds the 1 900 EUR cap. For an employee this is the normal case, not
    /// an edge case.
    #[test]
    fn basic_health_cover_overrides_the_cap() {
        let result = vorsorgeaufwendungen(&typical(), &deductions()).expect("computes");
        assert!(result.other_cap_overridden);
        assert!(
            result.other > deductions().other_provision_cap_employee,
            "the override should carry the deduction past the 1 900 EUR cap"
        );
    }

    /// And the counter-intuitive consequence: because the override has already carried the
    /// basket past its cap, additional liability or unemployment cover deducts *nothing*.
    ///
    /// A household planner that showed a tax saving here would be wrong, so the property is
    /// asserted rather than left implicit.
    #[test]
    fn additional_insurance_deducts_nothing_once_the_cap_is_overridden() {
        let base = vorsorgeaufwendungen(&typical(), &deductions()).expect("computes");

        let mut insured = typical();
        insured.other_provision = euro(5_000);
        let more = vorsorgeaufwendungen(&insured, &deductions()).expect("computes");

        assert_eq!(
            more.other, base.other,
            "extra Nr. 3a cover must deduct nothing when the override applies"
        );
    }

    /// Where basic cover is *below* the cap — a low earner — the cap applies normally and
    /// other expenses do deduct, up to it.
    #[test]
    fn below_the_override_the_cap_applies_normally() {
        let modest = Contributions {
            pension_employee: euro(500),
            pension_employer: euro(500),
            retirement_voluntary: Money::ZERO,
            health_general: euro(400),
            health_supplementary: euro(80),
            care: euro(130),
            other_provision: euro(2_000),
        };
        let result = vorsorgeaufwendungen(&modest, &deductions()).expect("computes");
        assert!(!result.other_cap_overridden);
        // Basic cover is 400×0.96 + 80 + 130 = 594; plus 2 000 exceeds the cap, so the cap
        // applies.
        assert_eq!(result.other, deductions().other_provision_cap_employee);
    }

    // ---------------------------------------------------------------------
    // Properties
    // ---------------------------------------------------------------------

    /// The deduction never exceeds what was actually paid. A deduction larger than the
    /// outlay would be a straightforward error in the caps.
    #[test]
    fn the_deduction_never_exceeds_the_contributions_paid() {
        let mut scale = 1_i64;
        while scale <= 20 {
            let scaled = Contributions {
                pension_employee: euro(500 * scale),
                pension_employer: euro(500 * scale),
                retirement_voluntary: euro(1_000 * scale),
                health_general: euro(400 * scale),
                health_supplementary: euro(80 * scale),
                care: euro(130 * scale),
                other_provision: euro(70 * scale),
            };
            let result = vorsorgeaufwendungen(&scaled, &deductions()).expect("computes");
            let paid = euro(500 * scale)
                .add(euro(1_000 * scale))
                .unwrap()
                .add(euro(400 * scale))
                .unwrap()
                .add(euro(80 * scale))
                .unwrap()
                .add(euro(130 * scale))
                .unwrap()
                .add(euro(70 * scale))
                .unwrap();
            assert!(
                result.total <= paid,
                "deducted {} of {} paid at scale {scale}",
                result.total.cents(),
                paid.cents()
            );
            scale = scale.saturating_add(1);
        }
    }

    /// Monotonic in every contribution: paying more can never deduct less.
    #[test]
    fn the_deduction_is_monotonic_in_the_contributions() {
        let mut previous = Money::ZERO;
        let mut scale = 0_i64;
        while scale <= 40 {
            let scaled = Contributions {
                pension_employee: euro(200 * scale),
                pension_employer: euro(200 * scale),
                retirement_voluntary: Money::ZERO,
                health_general: euro(150 * scale),
                health_supplementary: euro(30 * scale),
                care: euro(50 * scale),
                other_provision: euro(25 * scale),
            };
            let total = vorsorgeaufwendungen(&scaled, &deductions())
                .expect("computes")
                .total;
            assert!(total >= previous, "the deduction fell at scale {scale}");
            previous = total;
            scale = scale.saturating_add(1);
        }
    }

    /// No contributions, no deduction — and no error.
    #[test]
    fn no_contributions_deduct_nothing() {
        let nothing = Contributions {
            pension_employee: Money::ZERO,
            pension_employer: Money::ZERO,
            retirement_voluntary: Money::ZERO,
            health_general: Money::ZERO,
            health_supplementary: Money::ZERO,
            care: Money::ZERO,
            other_provision: Money::ZERO,
        };
        let result = vorsorgeaufwendungen(&nothing, &deductions()).expect("computes");
        assert_eq!(result.total, Money::ZERO);
    }

    /// An employer share alone exceeding the cap must not produce a negative deduction.
    #[test]
    fn an_employer_share_beyond_the_cap_floors_at_zero() {
        let odd = Contributions {
            pension_employee: euro(1_000),
            pension_employer: euro(40_000),
            retirement_voluntary: Money::ZERO,
            health_general: Money::ZERO,
            health_supplementary: Money::ZERO,
            care: Money::ZERO,
            other_provision: Money::ZERO,
        };
        let result = vorsorgeaufwendungen(&odd, &deductions()).expect("computes");
        assert_eq!(result.retirement, Money::ZERO);
        assert!(!result.retirement.is_negative());
    }

    /// The helper that builds contributions from a social insurance split must agree with the
    /// split's own aggregate health figure, which is what makes it safe to re-derive the two
    /// portions from the rates.
    #[test]
    fn the_split_helper_reproduces_the_aggregate_health_contribution() {
        use casivell_lawdata::Bundesland;
        use casivell_social::{Insured, contributions};

        let insured = Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
        let social = social();
        let gross = euro(4_500);
        let split = contributions(gross, &social, &insured).unwrap();

        let built = Contributions::from_social(
            &split,
            &social,
            social.health.average_supplementary_rate,
            12,
        )
        .expect("builds");

        let combined = built
            .health_general
            .add(built.health_supplementary)
            .unwrap();
        let from_split = split.health.employee.mul_int(12).unwrap();
        // Within twelve cents: the split rounds the combined contribution once a month,
        // while the two portions round separately.
        let difference = combined.sub(from_split).unwrap().cents().abs();
        assert!(
            difference <= 12,
            "the re-derived health portions differ from the split by {difference} cents"
        );
    }
}
