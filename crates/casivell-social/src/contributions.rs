//! Splitting social insurance contributions between employee and employer.
//!
//! # Two different splitting mechanisms, and why that matters
//!
//! It is tempting to model every branch as "apply half the rate to the base". That
//! is wrong twice over.
//!
//! **For pension, unemployment and health**, § 249 SGB V and its counterparts say
//! employer and employee each bear *half the contribution* — not that each applies
//! half the rate. Those differ by a cent whenever the full contribution is an odd
//! number of cents. The engine computes the total, halves it, and gives the
//! residual cent to the employer, so the two shares always reconstruct the total
//! exactly. [`ContributionSplit::shared_equally`] is that operation, and
//! `Money::div_int` exists in `casivell-core` for it.
//!
//! **For long-term care**, the shares genuinely differ by rate, and there is no
//! total to halve:
//!
//! - The employer always bears half the *base* rate — 1.8 % of 3.6 % — regardless
//!   of the employee's circumstances.
//! - The childless surcharge (§ 55 Abs. 3 SGB XI) falls on the **employee alone**.
//! - The per-child reductions also reduce the **employee's share only**; the
//!   employer's 1.8 % does not move when a second child arrives.
//! - In Saxony (§ 58 Abs. 3 SGB XI) the employee bears 0.5 points more and the
//!   employer 0.5 fewer, because Saxony kept Buß- und Bettag as a public holiday.
//!
//! So care insurance uses [`ContributionSplit::from_rates`], with each side
//! computed from its own rate. Modelling it as a halved total understates a
//! childless employee's burden by 0.3 % of gross — 180 € a year at 60 000 € gross,
//! silently. That was a defect in the original project specification; see
//! `docs/ROADMAP_ERRATA.md` §C.
//!
//! # Elterneigenschaft is permanent; the reductions are not
//!
//! A subtlety worth stating because it is easy to conflate into one field:
//! *having been* a parent permanently exempts someone from the childless surcharge,
//! even after the children are grown. The per-child *reductions*, by contrast,
//! require children **under 25**. A parent of three thirty-year-olds pays neither
//! the surcharge nor gets any reduction. [`Insured`] therefore carries `is_parent`
//! and `children_under_25` separately, and its constructor rejects the combination
//! that cannot occur.

use casivell_core::{Money, MoneyError, Rate, Rounding};
use casivell_lawdata::{Bundesland, CareInsurance, SocialParameters};

/// How one branch's contribution divides between the two parties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContributionSplit {
    /// The employee's share, deducted from gross pay.
    pub employee: Money,
    /// The employer's share, paid on top of gross pay.
    pub employer: Money,
}

impl ContributionSplit {
    /// Nothing owed by either party.
    pub const ZERO: Self = Self {
        employee: Money::ZERO,
        employer: Money::ZERO,
    };

    /// Splits a total contribution equally, giving any residual cent to the
    /// employer.
    ///
    /// The employee's share is truncated and the employer's is the remainder, so
    /// `employee + employer == total` holds exactly. Rounding the two shares
    /// independently would let them fail to sum to the amount actually owed.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] if an intermediate leaves the representable domain.
    pub const fn shared_equally(total: Money) -> Result<Self, MoneyError> {
        let employee = match total.div_int(2, Rounding::Floor) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        // Subtraction rather than a second division: this is what guarantees the
        // shares reconstitute the total.
        let employer = match total.sub(employee) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        Ok(Self { employee, employer })
    }

    /// Computes each side from its own rate, for branches where the shares differ.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] if an intermediate leaves the representable domain.
    pub const fn from_rates(
        base: Money,
        employee_rate: Rate,
        employer_rate: Rate,
    ) -> Result<Self, MoneyError> {
        let employee = match base.mul_rate(employee_rate, Rounding::HalfUp) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let employer = match base.mul_rate(employer_rate, Rounding::HalfUp) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        Ok(Self { employee, employer })
    }

    /// The combined contribution.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the sum leaves the representable domain.
    pub const fn total(&self) -> Result<Money, MoneyError> {
        self.employee.add(self.employer)
    }
}

/// The circumstances of an insured employee that affect their contributions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Insured {
    age_years: u8,
    is_parent: bool,
    children_under_25: u8,
    land: Bundesland,
    health_supplementary_rate: Option<Rate>,
}

impl Insured {
    /// The largest number of children the type will accept.
    ///
    /// A bound rather than an open `u8` so that every downstream loop over children
    /// has a provable upper limit (JPL R2). Well beyond the statutory reduction
    /// cap of five, so it never truncates a real entitlement.
    pub const MAX_CHILDREN: u8 = 20;

    /// The greatest age the type will accept, as a sanity bound.
    pub const MAX_AGE_YEARS: u8 = 120;

    /// Describes an insured employee.
    ///
    /// `is_parent` is *Elterneigenschaft*: whether this person has ever had a
    /// child. It is permanent and exempts them from the childless surcharge for
    /// life. `children_under_25` counts only children young enough to still attract
    /// a per-child reduction, so it may be zero while `is_parent` is true.
    ///
    /// `health_supplementary_rate` overrides the published average Zusatzbeitrag
    /// with the actual rate of the employee's own fund. Passing `None` uses the
    /// average, which is a *default*, not this person's cost — real fund rates
    /// differ by well over a percentage point.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if `age_years` or `children_under_25` exceeds
    /// its bound, or if `children_under_25 > 0` while `is_parent` is `false`. That
    /// last combination is not merely implausible but self-contradictory: it would
    /// levy the childless surcharge and the per-child reductions simultaneously.
    /// Rejected rather than silently reconciled, because either reconciliation
    /// would be a guess about what the caller meant.
    pub const fn new(
        age_years: u8,
        is_parent: bool,
        children_under_25: u8,
        land: Bundesland,
        health_supplementary_rate: Option<Rate>,
    ) -> Result<Self, MoneyError> {
        if age_years > Self::MAX_AGE_YEARS {
            return Err(MoneyError::OutOfDomain {
                cents: age_years as i64,
            });
        }
        if children_under_25 > Self::MAX_CHILDREN {
            return Err(MoneyError::OutOfDomain {
                cents: children_under_25 as i64,
            });
        }
        if children_under_25 > 0 && !is_parent {
            return Err(MoneyError::OutOfDomain {
                cents: children_under_25 as i64,
            });
        }
        Ok(Self {
            age_years,
            is_parent,
            children_under_25,
            land,
            health_supplementary_rate,
        })
    }

    /// The employee's age in whole years.
    #[must_use]
    pub const fn age_years(&self) -> u8 {
        self.age_years
    }

    /// Whether the employee has Elterneigenschaft.
    #[must_use]
    pub const fn is_parent(&self) -> bool {
        self.is_parent
    }

    /// The number of children under 25.
    #[must_use]
    pub const fn children_under_25(&self) -> u8 {
        self.children_under_25
    }

    /// The employee's federal state.
    #[must_use]
    pub const fn land(&self) -> Bundesland {
        self.land
    }

    /// The fund-specific Zusatzbeitrag, if one was supplied.
    #[must_use]
    pub const fn health_supplementary_rate(&self) -> Option<Rate> {
        self.health_supplementary_rate
    }
}

/// All four branches of social insurance for one month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocialContributions {
    /// Statutory pension insurance, SGB VI.
    pub pension: ContributionSplit,
    /// Unemployment insurance, SGB III.
    pub unemployment: ContributionSplit,
    /// Statutory health insurance, SGB V.
    pub health: ContributionSplit,
    /// Long-term care insurance, SGB XI.
    pub care: ContributionSplit,
    /// The income the pension and unemployment contributions were levied on, after
    /// the ceiling was applied.
    ///
    /// Reported because it is what determines the employee's Entgeltpunkte, and
    /// because a user earning above the ceiling should be able to see that their
    /// marginal euro attracted no contribution.
    pub pension_base: Money,
    /// The income the health and care contributions were levied on.
    pub health_base: Money,
}

impl SocialContributions {
    /// The total deducted from the employee's gross pay.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the sum leaves the representable domain.
    pub const fn employee_total(&self) -> Result<Money, MoneyError> {
        let a = match self.pension.employee.add(self.unemployment.employee) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let b = match a.add(self.health.employee) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        b.add(self.care.employee)
    }

    /// The total the employer pays on top of gross pay.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the sum leaves the representable domain.
    pub const fn employer_total(&self) -> Result<Money, MoneyError> {
        let a = match self.pension.employer.add(self.unemployment.employer) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let b = match a.add(self.health.employer) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        b.add(self.care.employer)
    }
}

/// Computes the four social insurance contributions on one month's gross salary.
///
/// `monthly_gross` is the Bruttoarbeitsentgelt. A negative amount is treated as
/// zero: there is no such thing as a negative contribution.
///
/// Applies to ordinary employment **above** the Übergangsbereich. Minijobs and
/// Midijobs follow different rules and are not implemented — see the crate
/// documentation.
///
/// # Errors
///
/// [`MoneyError`] if an intermediate leaves the representable domain.
pub fn contributions(
    monthly_gross: Money,
    params: &SocialParameters,
    insured: &Insured,
) -> Result<SocialContributions, MoneyError> {
    let gross = monthly_gross.floor_at_zero();

    // Each branch is capped at its own Beitragsbemessungsgrenze. Pension and
    // unemployment share one ceiling, health and care share a lower one, so income
    // between the two ceilings attracts pension but not health contributions.
    let pension_base = gross.min(params.pension.ceiling_monthly);
    let unemployment_base = gross.min(params.unemployment.ceiling_monthly);
    let health_base = gross.min(params.health.ceiling_monthly);

    // § 168 SGB VI, § 346 SGB III, § 249 SGB V: the contribution is halved, not
    // the rate.
    let pension_total =
        pension_base.mul_rate(params.pension.contribution_rate, Rounding::HalfUp)?;
    let pension = ContributionSplit::shared_equally(pension_total)?;

    let unemployment_total =
        unemployment_base.mul_rate(params.unemployment.contribution_rate, Rounding::HalfUp)?;
    let unemployment = ContributionSplit::shared_equally(unemployment_total)?;

    // The general rate plus this fund's Zusatzbeitrag, both shared equally since
    // the 2019 restoration of parity funding.
    let supplementary = insured
        .health_supplementary_rate()
        .unwrap_or(params.health.average_supplementary_rate);
    let health_rate = params.health.general_rate.add(supplementary)?;
    let health_total = health_base.mul_rate(health_rate, Rounding::HalfUp)?;
    let health = ContributionSplit::shared_equally(health_total)?;

    // Care insurance splits by rate, not by halving a total. See the module docs.
    let care = care_contribution(health_base, &params.care, insured)?;

    Ok(SocialContributions {
        pension,
        unemployment,
        health,
        care,
        pension_base,
        health_base,
    })
}

/// Splits the long-term care contribution, whose two sides carry different rates.
fn care_contribution(
    base: Money,
    care: &CareInsurance,
    insured: &Insured,
) -> Result<ContributionSplit, MoneyError> {
    let half_base = care.base_rate.half()?;

    // § 58 Abs. 3 SGB XI: Saxony shifts 0.5 points from employer to employee.
    let (mut employee_rate, employer_rate) = if insured.land().has_higher_employee_care_share() {
        (
            half_base.add(care.saxony_employee_surcharge)?,
            half_base.sub(care.saxony_employee_surcharge)?,
        )
    } else {
        (half_base, half_base)
    };

    // § 55 Abs. 3 SGB XI: the childless surcharge, borne by the employee alone.
    // Elterneigenschaft is permanent, so it turns on `is_parent` and not on whether
    // any child is currently under 25.
    if !insured.is_parent() && insured.age_years() >= care.childless_surcharge_min_age {
        employee_rate = employee_rate.add(care.childless_surcharge)?;
    }

    // The reductions run from the second child to the fifth: at most
    // `max_reduced_child_ordinal - 1` of them. They too reduce only the employee's
    // share.
    let reduction = child_reduction(care, insured.children_under_25())?;
    employee_rate = employee_rate.sub(reduction)?;

    ContributionSplit::from_rates(base, employee_rate, employer_rate)
}

/// The total per-child reduction for `children` children under 25.
///
/// Zero for none or one child; then 0.25 points per additional child up to the
/// statutory cap.
fn child_reduction(care: &CareInsurance, children: u8) -> Result<Rate, MoneyError> {
    let capped = children.min(care.max_reduced_child_ordinal);
    // The first child attracts no reduction, hence the saturating decrement. It
    // also makes the zero-children case fall out correctly rather than underflowing.
    let steps = i64::from(capped.saturating_sub(1));
    let ppm = care
        .per_child_reduction
        .ppm()
        .checked_mul(steps)
        .ok_or(MoneyError::Overflow)?;
    Rate::from_ppm(ppm)
}

#[cfg(test)]
mod tests {
    use super::{ContributionSplit, Insured, contributions};
    use casivell_core::{Money, MoneyError, Rate, TaxYear};
    use casivell_lawdata::{Bundesland, SocialParameters};

    fn params(year: u16) -> SocialParameters {
        SocialParameters::for_year(TaxYear::new(year).unwrap()).unwrap()
    }

    /// A childless 30-year-old in North Rhine-Westphalia on the average
    /// Zusatzbeitrag: the common case.
    fn childless() -> Insured {
        Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, None).unwrap()
    }

    fn parent_of(children: u8) -> Insured {
        Insured::new(40, true, children, Bundesland::NordrheinWestfalen, None).unwrap()
    }

    fn compute(gross_euro: i64, insured: &Insured) -> super::SocialContributions {
        let gross = Money::from_euro(gross_euro).unwrap();
        contributions(gross, &params(2026), insured).unwrap()
    }

    // ---------------------------------------------------------------------
    // The split mechanisms
    // ---------------------------------------------------------------------

    /// An equally-shared contribution must reconstitute its total exactly, odd cent
    /// included. Rounding both sides independently would lose or invent a cent.
    #[test]
    fn an_equal_split_always_reconstitutes_the_total() {
        for cents in 0_i64..200 {
            let total = Money::from_cents(cents).unwrap();
            let split = ContributionSplit::shared_equally(total).unwrap();
            assert_eq!(
                split.total().unwrap(),
                total,
                "the shares of {cents} cents do not sum back to the total"
            );
            // The residual cent goes to the employer, never the employee.
            assert!(split.employer >= split.employee);
            assert!(
                split.employer.sub(split.employee).unwrap().cents() <= 1,
                "the shares of {cents} cents differ by more than one cent"
            );
        }
    }

    // ---------------------------------------------------------------------
    // Pension and unemployment
    // ---------------------------------------------------------------------

    /// 18.6 % of 4 000 € is 744,00 €, so 372,00 € each side.
    #[test]
    fn the_pension_contribution_matches_the_statutory_rate() {
        let c = compute(4_000, &childless());
        assert_eq!(c.pension.total().unwrap().cents(), 74_400);
        assert_eq!(c.pension.employee.cents(), 37_200);
        assert_eq!(c.pension.employer.cents(), 37_200);
    }

    /// 2.6 % of 4 000 € is 104,00 €, so 52,00 € each side.
    #[test]
    fn the_unemployment_contribution_matches_the_statutory_rate() {
        let c = compute(4_000, &childless());
        assert_eq!(c.unemployment.total().unwrap().cents(), 10_400);
        assert_eq!(c.unemployment.employee.cents(), 5_200);
    }

    /// Above the ceiling the contribution stops growing. This is the single most
    /// visible feature of the German system to a high earner, and getting it wrong
    /// would overstate their deductions without limit.
    #[test]
    fn contributions_stop_at_the_ceiling() {
        let p = params(2026);
        let ceiling = p.pension.ceiling_monthly.whole_euro_floor().unwrap();

        let at = compute(ceiling, &childless());
        let above = compute(ceiling + 5_000, &childless());
        assert_eq!(at.pension.employee, above.pension.employee);
        assert_eq!(at.unemployment.employee, above.unemployment.employee);
        assert_eq!(above.pension_base, p.pension.ceiling_monthly);

        // 18.6 % of 8 450 € = 1 571,70 €, halved to 785,85 €.
        assert_eq!(above.pension.employee.cents(), 78_585);
    }

    /// The health ceiling is lower than the pension ceiling, so income between the
    /// two attracts pension but not health contributions. A single shared ceiling
    /// would be wrong for every salary in that band.
    #[test]
    fn the_two_ceilings_are_independent() {
        let p = params(2026);
        let health_ceiling = p.health.ceiling_monthly.whole_euro_floor().unwrap();
        let pension_ceiling = p.pension.ceiling_monthly.whole_euro_floor().unwrap();
        assert!(health_ceiling < pension_ceiling);

        // 7 000 € sits between the two.
        let between = compute(7_000, &childless());
        assert_eq!(between.health_base, p.health.ceiling_monthly);
        assert_eq!(between.pension_base, Money::from_euro(7_000).unwrap());

        // Raising pay within that band increases pension but not health.
        let higher = compute(8_000, &childless());
        assert!(higher.pension.employee > between.pension.employee);
        assert_eq!(higher.health.employee, between.health.employee);
    }

    // ---------------------------------------------------------------------
    // Health insurance
    // ---------------------------------------------------------------------

    /// 14.6 % + 2.9 % = 17.5 % of 3 000 € is 525,00 €, halved to 262,50 €.
    #[test]
    fn the_health_contribution_includes_the_supplementary_rate() {
        let c = compute(3_000, &childless());
        assert_eq!(c.health.total().unwrap().cents(), 52_500);
        assert_eq!(c.health.employee.cents(), 26_250);
    }

    /// A fund-specific rate must override the published average, since the average
    /// is a default and not anybody's actual cost.
    #[test]
    fn a_fund_specific_supplementary_rate_overrides_the_average() {
        let cheap_fund = Insured::new(
            30,
            false,
            0,
            Bundesland::NordrheinWestfalen,
            Some(Rate::from_percent_millis(1_000).unwrap()),
        )
        .unwrap();
        let average = compute(3_000, &childless());
        let cheaper = compute(3_000, &cheap_fund);
        assert!(cheaper.health.employee < average.health.employee);
        // 14.6 % + 1.0 % = 15.6 % of 3 000 € is 468,00 €, halved to 234,00 €.
        assert_eq!(cheaper.health.employee.cents(), 23_400);
    }

    // ---------------------------------------------------------------------
    // Care insurance: the corrections
    // ---------------------------------------------------------------------

    /// The employer's care share is half the base rate and does not move with the
    /// employee's circumstances. 1.8 % of 3 000 € is 54,00 €, for everyone outside
    /// Saxony.
    #[test]
    fn the_employer_care_share_is_unaffected_by_children_or_childlessness() {
        let expected = 5_400;
        assert_eq!(compute(3_000, &childless()).care.employer.cents(), expected);
        for children in 0_u8..=6 {
            assert_eq!(
                compute(3_000, &parent_of(children)).care.employer.cents(),
                expected,
                "the employer share moved for a parent of {children}"
            );
        }
    }

    /// The childless surcharge falls on the employee alone: 1.8 % + 0.6 % = 2.4 %
    /// of 3 000 € is 72,00 €, against a parent's 54,00 €. Halving a combined rate
    /// would have given both 63,00 € and understated the childless employee by
    /// 9,00 € a month.
    #[test]
    fn the_childless_surcharge_falls_on_the_employee_alone() {
        let childless_employee = compute(3_000, &childless()).care;
        let parent = compute(3_000, &parent_of(1)).care;

        assert_eq!(childless_employee.employee.cents(), 7_200);
        assert_eq!(parent.employee.cents(), 5_400);
        assert_eq!(childless_employee.employer, parent.employer);

        // The extra 0.6 % is 18,00 € on 3 000 €, borne entirely by the employee.
        let extra = childless_employee
            .employee
            .sub(parent.employee)
            .unwrap()
            .cents();
        assert_eq!(extra, 1_800);
    }

    /// Below the surcharge age there is no surcharge, even when childless.
    #[test]
    fn the_childless_surcharge_starts_at_the_statutory_age() {
        let p = params(2026);
        let min_age = p.care.childless_surcharge_min_age;

        let young = Insured::new(min_age - 1, false, 0, Bundesland::Berlin, None).unwrap();
        let at_age = Insured::new(min_age, false, 0, Bundesland::Berlin, None).unwrap();

        assert_eq!(compute(3_000, &young).care.employee.cents(), 5_400);
        assert_eq!(compute(3_000, &at_age).care.employee.cents(), 7_200);
    }

    /// The published rate ladder, reproduced through the contribution rather than
    /// the rate: employee shares of 1.8 %, 1.55 %, 1.30 %, 1.05 %, 0.80 % of 4 000 €.
    #[test]
    fn the_child_reductions_follow_the_published_ladder() {
        // (children under 25, employee care contribution in cents on 4 000 €)
        let ladder = [
            (1_u8, 7_200_i64), // 1.80 %
            (2, 6_200),        // 1.55 %
            (3, 5_200),        // 1.30 %
            (4, 4_200),        // 1.05 %
            (5, 3_200),        // 0.80 %
            (6, 3_200),        // capped at five
            (9, 3_200),
        ];
        for (children, expected) in ladder {
            assert_eq!(
                compute(4_000, &parent_of(children)).care.employee.cents(),
                expected,
                "the employee share is wrong for a parent of {children}"
            );
        }
    }

    /// Elterneigenschaft is permanent: a parent whose children are all over 25 pays
    /// neither the childless surcharge nor receives any reduction.
    #[test]
    fn a_parent_of_grown_children_pays_the_plain_base_share() {
        let empty_nest = Insured::new(60, true, 0, Bundesland::NordrheinWestfalen, None).unwrap();
        assert_eq!(compute(3_000, &empty_nest).care.employee.cents(), 5_400);
        // Strictly less than a childless person of the same age.
        let childless_sixty =
            Insured::new(60, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
        assert!(
            compute(3_000, &empty_nest).care.employee
                < compute(3_000, &childless_sixty).care.employee
        );
    }

    /// Saxony shifts 0.5 points from employer to employee, leaving the total
    /// unchanged. Both halves of that must hold, or the total would be wrong too.
    #[test]
    fn saxony_shifts_half_a_point_without_changing_the_total() {
        let saxon = Insured::new(40, true, 1, Bundesland::Sachsen, None).unwrap();
        let elsewhere = parent_of(1);

        let s = compute(3_000, &saxon).care;
        let e = compute(3_000, &elsewhere).care;

        // 2.3 % and 1.3 % of 3 000 € against 1.8 % and 1.8 %.
        assert_eq!(s.employee.cents(), 6_900);
        assert_eq!(s.employer.cents(), 3_900);
        assert_eq!(e.employee.cents(), 5_400);
        assert_eq!(e.employer.cents(), 5_400);

        // The combined contribution is identical; only the incidence differs.
        assert_eq!(s.total().unwrap(), e.total().unwrap());
    }

    /// A childless Saxon bears both adjustments: 1.8 + 0.5 + 0.6 = 2.9 %.
    #[test]
    fn a_childless_saxon_bears_both_adjustments() {
        let saxon = Insured::new(30, false, 0, Bundesland::Sachsen, None).unwrap();
        // 2.9 % of 3 000 € is 87,00 €.
        assert_eq!(compute(3_000, &saxon).care.employee.cents(), 8_700);
    }

    // ---------------------------------------------------------------------
    // Profile validation
    // ---------------------------------------------------------------------

    /// Children under 25 without Elterneigenschaft is self-contradictory and must
    /// be refused rather than reconciled: either reconciliation would be a guess.
    #[test]
    fn a_contradictory_profile_is_refused() {
        assert!(matches!(
            Insured::new(30, false, 2, Bundesland::Berlin, None),
            Err(MoneyError::OutOfDomain { .. })
        ));
        // The consistent versions are both fine.
        assert!(Insured::new(30, true, 2, Bundesland::Berlin, None).is_ok());
        assert!(Insured::new(30, true, 0, Bundesland::Berlin, None).is_ok());
    }

    #[test]
    fn implausible_ages_and_child_counts_are_refused() {
        assert!(matches!(
            Insured::new(200, false, 0, Bundesland::Berlin, None),
            Err(MoneyError::OutOfDomain { .. })
        ));
        assert!(matches!(
            Insured::new(30, true, 50, Bundesland::Berlin, None),
            Err(MoneyError::OutOfDomain { .. })
        ));
    }

    // ---------------------------------------------------------------------
    // Properties
    // ---------------------------------------------------------------------

    /// No contribution is ever negative, and none exceeds its own base. Swept
    /// across the whole salary range and every profile shape.
    #[test]
    fn contributions_are_non_negative_and_bounded_by_their_base() {
        let profiles = [
            childless(),
            parent_of(1),
            parent_of(5),
            Insured::new(30, false, 0, Bundesland::Sachsen, None).unwrap(),
            Insured::new(22, false, 0, Bundesland::Bayern, None).unwrap(),
        ];
        for insured in &profiles {
            let mut gross = 0_i64;
            while gross <= 15_000 {
                let c = compute(gross, insured);
                for split in [c.pension, c.unemployment, c.health, c.care] {
                    assert!(
                        !split.employee.is_negative(),
                        "negative employee share at {gross} €"
                    );
                    assert!(
                        !split.employer.is_negative(),
                        "negative employer share at {gross} €"
                    );
                }
                // A contribution never exceeds the income it is levied on. The
                // comparison must be `<=` rather than `<`: at zero pay both sides
                // are zero, which is correct and not a violation.
                assert!(c.pension.total().unwrap() <= c.pension_base);
                assert!(c.health.total().unwrap() <= c.health_base);
                gross = gross.saturating_add(137);
            }
        }
    }

    /// Contributions are monotonically non-decreasing in gross pay: earning more
    /// never reduces a contribution.
    #[test]
    fn contributions_are_monotonic_in_gross_pay() {
        let insured = childless();
        let mut previous = Money::ZERO;
        let mut gross = 0_i64;
        while gross <= 12_000 {
            let total = compute(gross, &insured).employee_total().unwrap();
            assert!(
                total >= previous,
                "the employee total fell at {gross} € of gross pay"
            );
            previous = total;
            gross = gross.saturating_add(89);
        }
    }

    /// Zero and negative gross pay produce no contributions rather than an error or
    /// a negative deduction.
    #[test]
    fn no_pay_means_no_contributions() {
        for gross in [Money::ZERO, Money::from_euro(-2_000).unwrap()] {
            let c = contributions(gross, &params(2026), &childless()).unwrap();
            assert_eq!(c.employee_total().unwrap(), Money::ZERO);
            assert_eq!(c.employer_total().unwrap(), Money::ZERO);
        }
    }

    /// The employee's total deduction on a typical salary, as a sanity anchor:
    /// 9.3 % + 1.3 % + 8.75 % + 2.4 % = 21.75 % of 3 000 € = 652,50 €.
    #[test]
    fn the_employee_total_matches_the_summed_statutory_rates() {
        let c = compute(3_000, &childless());
        assert_eq!(c.employee_total().unwrap().cents(), 65_250);
    }

    /// The employer bears slightly less than the employee for a childless person,
    /// because the childless surcharge is not shared. This asymmetry is the whole
    /// point of the correction and is worth asserting at the aggregate level too.
    #[test]
    fn the_employer_bears_less_than_a_childless_employee() {
        let c = compute(3_000, &childless());
        let employee = c.employee_total().unwrap();
        let employer = c.employer_total().unwrap();
        assert!(employee > employer);
        // Exactly the 0.6 % surcharge: 18,00 € on 3 000 €.
        assert_eq!(employee.sub(employer).unwrap().cents(), 1_800);
    }

    /// For a parent of one, the burden is symmetric outside Saxony.
    #[test]
    fn the_burden_is_symmetric_for_a_parent_of_one() {
        let c = compute(3_000, &parent_of(1));
        assert_eq!(c.employee_total().unwrap(), c.employer_total().unwrap());
    }

    /// The 2025 parameters must produce different figures from 2026, or the year is
    /// not actually being threaded through.
    ///
    /// The salary has to be chosen with care. At 7 000 € both years' pension
    /// ceilings (8 050 € and 8 450 €) sit above the salary and the rate is unchanged
    /// at 18.6 %, so the pension contribution is *identical* across the two years —
    /// picking that figure would have made this test vacuous in the one direction it
    /// most needs to check. 8 200 € falls between the two ceilings, so the ceiling
    /// increase actually bites.
    #[test]
    fn the_year_changes_the_result() {
        let gross = Money::from_euro(8_200).unwrap();
        let a = contributions(gross, &params(2025), &childless()).unwrap();
        let b = contributions(gross, &params(2026), &childless()).unwrap();

        // The pension base was capped at 8 050 € in 2025 and is the full 8 200 € now.
        assert_eq!(a.pension_base, params(2025).pension.ceiling_monthly);
        assert_eq!(b.pension_base, gross);
        assert!(b.pension.employee > a.pension.employee);

        // Health rose on two counts: a higher ceiling and a higher average
        // Zusatzbeitrag (2.5 % → 2.9 %).
        assert!(b.health.employee > a.health.employee);
    }

    /// Below both ceilings and with a fund-specific rate pinned, the two years must
    /// agree — the pension and unemployment rates did not change. The complement of
    /// the test above, and what makes its choice of salary meaningful.
    #[test]
    fn an_unchanged_rate_below_both_ceilings_gives_an_unchanged_contribution() {
        let gross = Money::from_euro(4_000).unwrap();
        let a = contributions(gross, &params(2025), &childless()).unwrap();
        let b = contributions(gross, &params(2026), &childless()).unwrap();
        assert_eq!(a.pension.employee, b.pension.employee);
        assert_eq!(a.unemployment.employee, b.unemployment.employee);
        // Care rates were also unchanged between the two years.
        assert_eq!(a.care.employee, b.care.employee);
    }
}
