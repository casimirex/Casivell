//! §§ 2, 2a, 2c–2f and 4a BEEG: how much Elterngeld.

use casivell_core::{Money, MoneyError, Rate, Rounding};
use casivell_lawdata::{DeductionParameters, ElterngeldParameters};
use casivell_payroll::{Employment, PayPeriod, PayrollLaw, withhold};

/// Which of the two forms is being drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// Basiselterngeld: the full monthly amount, for up to fourteen months between two
    /// parents.
    Basis,
    /// `ElterngeldPlus`: § 4a caps it at **half of what would be due with no income during the
    /// period**, and pays it for twice as many months.
    ///
    /// The cap is a maximum, not a halving. That distinction is the whole design of the
    /// provision: someone working part time already has their Basiselterngeld cut by the
    /// § 2 Abs. 3 difference rule, so the half-cap often does not bind at all and they draw
    /// nearly the same monthly amount for twice as long. See
    /// `elternggeld_plus_rewards_part_time_work`.
    Plus,
}

/// What is being asked.
#[derive(Debug, Clone, Copy)]
pub struct ElterngeldRequest {
    /// Average monthly gross employment income over the twelve months before the birth.
    ///
    /// § 2b's shifts of that window — for an earlier child's parental leave, for a
    /// pregnancy-related illness — are the caller's to apply.
    pub monthly_gross_before: Money,
    /// Average monthly gross during the months being drawn, zero for a full interruption.
    ///
    /// Non-zero reduces the benefit through § 2 Abs. 3, which pays the rate on the
    /// *difference* rather than on the whole.
    pub monthly_gross_during: Money,

    /// The household's zu versteuerndes Einkommen, for the § 1 Abs. 8 limit.
    pub household_taxable_income: Money,

    /// Whether the § 2a Abs. 1 Geschwisterbonus applies.
    ///
    /// An input rather than a derivation: the condition is about the ages of other children
    /// in the household, which is not modelled here.
    pub sibling_bonus: bool,
    /// Children of a multiple birth *beyond the first*, for the § 2a Abs. 4 supplement.
    pub additional_children: u8,

    /// Basiselterngeld or `ElterngeldPlus`.
    pub variant: Variant,
}

impl ElterngeldRequest {
    /// A full interruption of work by someone with no other complications.
    ///
    /// The common case, and the one worth having a short constructor for.
    #[must_use]
    pub const fn full_interruption(
        monthly_gross_before: Money,
        household_taxable_income: Money,
    ) -> Self {
        Self {
            monthly_gross_before,
            monthly_gross_during: Money::ZERO,
            household_taxable_income,
            sibling_bonus: false,
            additional_children: 0,
            variant: Variant::Basis,
        }
    }
}

/// The monthly Elterngeld, with every stage of the calculation exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elterngeld {
    /// Whether there is any entitlement at all.
    ///
    /// False only for the § 1 Abs. 8 income limit, which is the one eligibility rule that is
    /// pure arithmetic. Every other condition is the caller's to assert.
    pub entitled: bool,

    /// The stylised pre-birth net of §§ 2c–2f — not the payslip net.
    pub net_before: Money,
    /// The same computation applied to income during the period, zero for a full break.
    pub net_during: Money,
    /// The § 2 Abs. 1–2 replacement rate, between 65 % and 100 %.
    pub replacement_rate: Rate,
    /// The income the rate was applied to: the difference, after the § 2 Abs. 3 cap.
    pub replaced_income: Money,

    /// The amount after the rate and the 300 … 1 800 € clamp, before § 2a.
    pub before_bonuses: Money,
    /// The § 2a Abs. 1 Geschwisterbonus.
    pub sibling_bonus: Money,
    /// The § 2a Abs. 4 Mehrlingszuschlag.
    pub multiple_birth_supplement: Money,

    /// What is actually paid each month.
    pub monthly_amount: Money,
    /// Which form this is.
    pub variant: Variant,
}

/// The stylised monthly net of §§ 2c, 2e and 2f.
///
/// `law` must be the payroll parameters § 2e names: those in force on 1 January of the
/// calendar year before the birth. Passing the wrong year is a caller error the type system
/// cannot catch, so it is stated here.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn elterngeld_netto(
    monthly_gross: Money,
    employment: &Employment,
    law: &PayrollLaw,
    deductions: &DeductionParameters,
    beeg: &ElterngeldParameters,
) -> Result<Money, MoneyError> {
    let gross = monthly_gross.floor_at_zero();
    if gross.is_zero() {
        return Ok(Money::ZERO);
    }

    // § 2e: tax from the Programmablaufplan, on the Einnahmen. The PAP applies the
    // Arbeitnehmer-Pauschbetrag and the Vorsorgepauschale internally, which is exactly what
    // § 2e Satz 2 and 3 prescribe — so the whole deduction is one call to verified code.
    let tax = withhold(gross, PayPeriod::Month, employment, law)?;
    let taxes = tax
        .income_tax
        .add(tax.solidarity_surcharge)?
        .add(tax.church_tax)?;

    // § 2f Abs. 2: the flat 21 % applies to the *raw* Einnahmen, before the Pauschbetrag —
    // a different base from the one the Pauschbetrag itself comes off, and easy to conflate.
    let social = gross.mul_rate(beeg.social_deduction_rate()?, Rounding::HalfUp)?;

    // § 2c Abs. 1: one twelfth of the Arbeitnehmer-Pauschbetrag, then both deductions.
    let lump_sum_share = deductions.employee_lump_sum.div_int(
        i64::from(casivell_lawdata::MONTHS_PER_YEAR),
        Rounding::HalfUp,
    )?;

    Ok(gross
        .sub(lump_sum_share)?
        .sub(taxes)?
        .sub(social)?
        .floor_at_zero())
}

/// The § 2 Abs. 1–2 replacement rate for a given stylised net.
///
/// Flat at 67 % between the two thresholds, sliding up to 100 % below the lower one and down
/// to 65 % above the upper one, by 0,1 points per 2 €.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn replacement_rate(net: Money, beeg: &ElterngeldParameters) -> Result<Rate, MoneyError> {
    if net > beeg.upper_income_threshold {
        let excess = net.sub(beeg.upper_income_threshold)?;
        let reduction = slide(excess, beeg)?;
        // Saturating at the floor rather than erroring: a large excess simply lands on 65 %.
        return Ok(match beeg.base_rate.sub(reduction) {
            Ok(rate) if rate.ppm() > beeg.floor_rate.ppm() => rate,
            _ => beeg.floor_rate,
        });
    }
    if net < beeg.lower_income_threshold {
        let shortfall = beeg.lower_income_threshold.sub(net)?;
        let increase = slide(shortfall, beeg)?;
        let raised = beeg.base_rate.add(increase)?;
        return Ok(if raised.ppm() > beeg.ceiling_rate.ppm() {
            beeg.ceiling_rate
        } else {
            raised
        });
    }
    Ok(beeg.base_rate)
}

/// How far the rate moves for a given distance past a threshold.
///
/// Whole steps only: the statute moves the rate "für je 2 Euro", so a euro of the way to the
/// next step moves it not at all. Truncation is the statute's, not a rounding convenience.
fn slide(distance: Money, beeg: &ElterngeldParameters) -> Result<Rate, MoneyError> {
    let steps = casivell_core::div_trunc(distance.cents(), beeg.rate_step_income.cents())?;
    Rate::from_ppm(
        beeg.rate_step
            .ppm()
            .checked_mul(steps)
            .ok_or(MoneyError::Overflow)?,
    )
}

/// Computes the monthly Elterngeld.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn elterngeld(
    request: &ElterngeldRequest,
    employment: &Employment,
    law: &PayrollLaw,
    deductions: &DeductionParameters,
    beeg: &ElterngeldParameters,
) -> Result<Elterngeld, MoneyError> {
    let net_before = elterngeld_netto(
        request.monthly_gross_before,
        employment,
        law,
        deductions,
        beeg,
    )?;
    let net_during = elterngeld_netto(
        request.monthly_gross_during,
        employment,
        law,
        deductions,
        beeg,
    )?;
    let rate = replacement_rate(net_before, beeg)?;

    // § 1 Abs. 8: a cliff, not a taper. One euro over the limit and the entire entitlement
    // goes, which is worth reporting as a distinct state rather than as a zero amount.
    if request.household_taxable_income > beeg.income_limit_annual {
        return Ok(Elterngeld {
            entitled: false,
            net_before,
            net_during,
            replacement_rate: rate,
            replaced_income: Money::ZERO,
            before_bonuses: Money::ZERO,
            sibling_bonus: Money::ZERO,
            multiple_birth_supplement: Money::ZERO,
            monthly_amount: Money::ZERO,
            variant: request.variant,
        });
    }

    let base = basis_amount(net_before, net_during, rate, beeg)?;

    // § 4a Abs. 1: `ElterngeldPlus` is capped at half of what would be due with *no* income
    // during the period — not half of what is due. Where part-time work has already reduced
    // the Basiselterngeld below that cap, the cap does not bind.
    let amount = match request.variant {
        Variant::Basis => base,
        Variant::Plus => {
            let without_income = basis_amount(net_before, Money::ZERO, rate, beeg)?;
            let cap = without_income.div_int(2, Rounding::Floor)?;
            base.min(cap)
        }
    };

    // § 2a: the bonuses apply to the amount as clamped, and are not themselves capped by the
    // § 2 Abs. 1 maximum — a household at the ceiling with two small children receives more
    // than 1 800 €.
    let sibling_bonus = if request.sibling_bonus {
        amount
            .mul_rate(beeg.sibling_bonus_rate, Rounding::HalfUp)?
            .max(beeg.sibling_bonus_minimum)
    } else {
        Money::ZERO
    };
    let multiple_birth_supplement = beeg
        .multiple_birth_supplement
        .mul_int(i64::from(request.additional_children))?;

    Ok(Elterngeld {
        entitled: true,
        net_before,
        net_during,
        replacement_rate: rate,
        replaced_income: replaced_income(net_before, net_during, beeg)?,
        before_bonuses: amount,
        sibling_bonus,
        multiple_birth_supplement,
        monthly_amount: amount.add(sibling_bonus)?.add(multiple_birth_supplement)?,
        variant: request.variant,
    })
}

/// The income the rate is applied to, per § 2 Abs. 3.
///
/// The difference between the pre- and post-birth nets, with the pre-birth figure capped at
/// 2 770 €. With no income during the period this is simply the capped pre-birth net.
fn replaced_income(
    net_before: Money,
    net_during: Money,
    beeg: &ElterngeldParameters,
) -> Result<Money, MoneyError> {
    let capped = net_before.min(beeg.difference_income_cap);
    Ok(capped.sub(net_during)?.floor_at_zero())
}

/// The Basiselterngeld: rate on the replaced income, clamped to the statutory bounds.
fn basis_amount(
    net_before: Money,
    net_during: Money,
    rate: Rate,
    beeg: &ElterngeldParameters,
) -> Result<Money, MoneyError> {
    let replaced = replaced_income(net_before, net_during, beeg)?;
    let raw = replaced.mul_rate(rate, Rounding::HalfUp)?;
    // § 2 Abs. 1 Satz 2 caps, § 2 Abs. 4 floors — and the floor applies even to someone who
    // had no income before the birth at all.
    Ok(raw.min(beeg.maximum_monthly).max(beeg.minimum_monthly))
}

#[cfg(test)]
mod tests {
    use super::{
        Elterngeld, ElterngeldRequest, Variant, elterngeld, elterngeld_netto, replacement_rate,
    };
    use casivell_core::{Money, Rate, TaxYear};
    use casivell_lawdata::{Bundesland, DeductionParameters, ElterngeldParameters, TaxClass};
    use casivell_payroll::{Employment, HealthCover, PayrollLaw};
    use casivell_social::Insured;

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    fn year() -> TaxYear {
        TaxYear::new(2026).unwrap()
    }

    fn beeg() -> ElterngeldParameters {
        ElterngeldParameters::for_year(year()).unwrap()
    }

    fn deductions() -> DeductionParameters {
        DeductionParameters::for_year(year()).unwrap()
    }

    fn law() -> PayrollLaw {
        PayrollLaw::for_year(year()).unwrap()
    }

    fn employment() -> Employment {
        // A parent with one child: `is_parent` and `children_under_25` must agree, and the
        // type refuses the contradiction — which it duly did when this first said `false`.
        let insured = Insured::new(30, true, 1, Bundesland::NordrheinWestfalen, None).unwrap();
        Employment::new(
            insured,
            TaxClass::Class1,
            10,
            HealthCover::Statutory {
                supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
            },
            None,
        )
        .unwrap()
    }

    fn netto(gross: i64) -> Money {
        elterngeld_netto(euro(gross), &employment(), &law(), &deductions(), &beeg())
            .expect("computes")
    }

    fn compute(request: &ElterngeldRequest) -> Elterngeld {
        elterngeld(request, &employment(), &law(), &deductions(), &beeg()).expect("computes")
    }

    fn full_break(gross: i64) -> Elterngeld {
        compute(&ElterngeldRequest::full_interruption(
            euro(gross),
            euro(60_000),
        ))
    }

    // ---------------------------------------------------------------------
    // The replacement rate, which is pure arithmetic and fully checkable
    // ---------------------------------------------------------------------

    /// The statute's own boundary values. These are the figures § 2 Abs. 2 states outright,
    /// so they are the closest thing to an official reference table this benefit has.
    #[test]
    fn the_rate_matches_the_statutes_stated_boundaries() {
        let p = beeg();
        let rate = |net: i64| replacement_rate(euro(net), &p).unwrap();

        // Flat at 67 % across the band, inclusive at both ends.
        assert_eq!(rate(1_000), p.base_rate);
        assert_eq!(rate(1_100), p.base_rate);
        assert_eq!(rate(1_200), p.base_rate);

        // 0,1 points per 2 € above 1 200, bottoming out at 65 % at 1 240 €.
        assert_eq!(rate(1_202), Rate::from_percent_millis(66_900).unwrap());
        assert_eq!(rate(1_220), Rate::from_percent_millis(66_000).unwrap());
        assert_eq!(rate(1_240), p.floor_rate);
        assert_eq!(rate(5_000), p.floor_rate, "the floor holds however high");

        // And upward below 1 000, reaching 100 % at 340 €.
        assert_eq!(rate(998), Rate::from_percent_millis(67_100).unwrap());
        assert_eq!(rate(900), Rate::from_percent_millis(72_000).unwrap());
        assert_eq!(rate(340), p.ceiling_rate);
        assert_eq!(rate(0), p.ceiling_rate, "the ceiling holds however low");
    }

    /// The rate moves in whole steps: the statute says "für je 2 Euro", so one euro past a
    /// step moves nothing. Truncating rather than rounding is the statute's choice.
    #[test]
    fn the_rate_moves_in_whole_two_euro_steps() {
        let p = beeg();
        let rate = |net: i64| replacement_rate(euro(net), &p).unwrap();
        assert_eq!(rate(1_201), rate(1_200), "one euro is not a step");
        assert_eq!(rate(1_203), rate(1_202));
        assert!(rate(1_204) < rate(1_202));
    }

    /// The rate must be monotone in income across the whole range, or the benefit would
    /// reward earning less at some point.
    #[test]
    fn the_rate_falls_monotonically_as_income_rises() {
        let p = beeg();
        let mut previous = Rate::from_percent_millis(200_000).unwrap();
        for net in (0..3_000).step_by(7) {
            let rate = replacement_rate(euro(net), &p).unwrap();
            assert!(rate.ppm() <= previous.ppm(), "the rate rose at {net} EUR");
            assert!(rate.ppm() >= p.floor_rate.ppm());
            assert!(rate.ppm() <= p.ceiling_rate.ppm());
            previous = rate;
        }
    }

    // ---------------------------------------------------------------------
    // The stylised net
    // ---------------------------------------------------------------------

    /// The stylised net must sit below the gross and above zero, and must be *lower* than the
    /// real payslip net for a high earner — because § 2f disregards the contribution ceilings
    /// and so over-deducts.
    #[test]
    fn the_stylised_net_undershoots_the_real_net_for_a_high_earner() {
        use casivell_payroll::monthly_net;

        for gross in [3_000_i64, 6_000, 12_000] {
            let stylised = netto(gross);
            let real = monthly_net(euro(gross), &employment(), &law()).unwrap().net;
            assert!(stylised > Money::ZERO && stylised < euro(gross));

            if gross >= 12_000 {
                assert!(
                    stylised < real,
                    "at {gross} EUR the ceiling-free 21 % should over-deduct: \
                     stylised {stylised:?} against real {real:?}"
                );
            }
        }
    }

    /// Zero gross must give zero net rather than a negative one, since the Pauschbetrag and
    /// the deductions would otherwise push it below zero.
    #[test]
    fn no_income_gives_a_zero_net() {
        assert_eq!(netto(0), Money::ZERO);
    }

    /// The net must rise with the gross throughout — a benefit that fell as pay rose would
    /// be a sign error somewhere in the deductions.
    #[test]
    fn the_stylised_net_rises_with_the_gross() {
        let mut previous = Money::ZERO;
        for gross in (500..10_000).step_by(250) {
            let net = netto(gross);
            assert!(net >= previous, "the net fell at {gross} EUR");
            previous = net;
        }
    }

    // ---------------------------------------------------------------------
    // The amount
    // ---------------------------------------------------------------------

    /// The statutory bounds must bind at both ends.
    #[test]
    fn the_amount_is_clamped_to_the_statutory_bounds() {
        let p = beeg();
        // Someone with no prior income still receives the minimum (§ 2 Abs. 4 Satz 2).
        assert_eq!(full_break(0).monthly_amount, p.minimum_monthly);
        // And a high earner is held to the maximum.
        assert_eq!(full_break(15_000).monthly_amount, p.maximum_monthly);
        assert_eq!(full_break(9_000).monthly_amount, p.maximum_monthly);
    }

    /// Between the bounds the amount is the rate times the net, and rises with income.
    #[test]
    fn the_amount_rises_with_income_between_the_bounds() {
        let p = beeg();
        let mut previous = Money::ZERO;
        for gross in (1_000..7_000).step_by(250) {
            let amount = full_break(gross).monthly_amount;
            assert!(amount >= previous, "the amount fell at {gross} EUR gross");
            assert!(amount >= p.minimum_monthly);
            assert!(amount <= p.maximum_monthly);
            previous = amount;
        }
        assert_eq!(previous, p.maximum_monthly);
    }

    /// The whole calculation must reconcile: the amount is the rate applied to the replaced
    /// income, subject to the clamp.
    #[test]
    fn the_amount_is_the_rate_applied_to_the_replaced_income() {
        let result = full_break(3_000);
        let expected = result
            .replaced_income
            .mul_rate(result.replacement_rate, casivell_core::Rounding::HalfUp)
            .unwrap();
        assert_eq!(result.before_bonuses, expected);
        assert!(expected > beeg().minimum_monthly && expected < beeg().maximum_monthly);
    }

    /// § 2 Abs. 3: income during the period reduces the benefit, because the rate applies to
    /// the *difference* rather than to the whole.
    #[test]
    fn income_during_the_period_reduces_the_benefit() {
        let mut request = ElterngeldRequest::full_interruption(euro(4_000), euro(60_000));
        let full = compute(&request).monthly_amount;

        request.monthly_gross_during = euro(2_000);
        let partial = compute(&request).monthly_amount;

        assert!(partial < full);
        assert!(partial >= beeg().minimum_monthly, "the floor still applies");

        // Working at the pre-birth level leaves nothing to replace, but the minimum stands.
        request.monthly_gross_during = euro(4_000);
        assert_eq!(compute(&request).monthly_amount, beeg().minimum_monthly);
    }

    /// § 2 Abs. 3 Satz 2 caps the pre-birth income at 2 770 € for the difference, so two high
    /// earners with different salaries but the same during-income get the same benefit.
    #[test]
    fn the_difference_cap_binds_for_high_earners() {
        let with_income = |before: i64| {
            compute(&ElterngeldRequest {
                monthly_gross_during: euro(1_000),
                ..ElterngeldRequest::full_interruption(euro(before), euro(60_000))
            })
            .monthly_amount
        };
        assert_eq!(with_income(8_000), with_income(12_000));
    }

    // ---------------------------------------------------------------------
    // `ElterngeldPlus`
    // ---------------------------------------------------------------------

    /// With a full break, `ElterngeldPlus` is exactly half — the § 4a cap binds, because there
    /// is no income to have already reduced the Basiselterngeld.
    #[test]
    fn with_no_income_elterngeld_plus_is_simply_half() {
        let basis = full_break(4_000).monthly_amount;
        let plus = compute(&ElterngeldRequest {
            variant: Variant::Plus,
            ..ElterngeldRequest::full_interruption(euro(4_000), euro(60_000))
        })
        .monthly_amount;
        assert_eq!(plus.cents(), basis.cents() / 2);
    }

    /// The point of `ElterngeldPlus`, and the reason § 4a caps rather than halves.
    ///
    /// Someone working part time has already had their Basiselterngeld cut by the § 2 Abs. 3
    /// difference rule. The half-cap is computed on what they *would* have received with no
    /// income at all, so where the difference rule has already cut below that, the cap does
    /// not bind and `ElterngeldPlus` pays nearly the Basis amount — for twice as many months.
    ///
    /// A model that halved instead of capping would understate `ElterngeldPlus` for exactly the
    /// households it was designed for, and make the option look worse than it is.
    #[test]
    fn elternggeld_plus_rewards_part_time_work() {
        let request = |variant| ElterngeldRequest {
            monthly_gross_during: euro(2_400),
            variant,
            ..ElterngeldRequest::full_interruption(euro(4_000), euro(60_000))
        };
        let basis = compute(&request(Variant::Basis)).monthly_amount;
        let plus = compute(&request(Variant::Plus)).monthly_amount;

        // The cap does not bind here, so Plus equals Basis — twice the months at no monthly
        // cost, which is the whole point of the provision.
        assert_eq!(plus, basis);

        // Whereas with a full break the cap would have bitten and halved it.
        let full_basis = full_break(4_000).monthly_amount;
        assert!(
            plus < full_basis,
            "part-time still reduces the monthly figure"
        );
        let plus_on_a_full_break = compute(&ElterngeldRequest {
            variant: Variant::Plus,
            ..ElterngeldRequest::full_interruption(euro(4_000), euro(60_000))
        })
        .monthly_amount;
        assert_eq!(plus_on_a_full_break.cents(), full_basis.cents() / 2);

        // The advantage is against taking *Basis* while working the same part-time hours:
        // the same monthly amount, drawn over twice as many months, so twice the total.
        assert_eq!(plus.mul_int(2).unwrap(), basis.mul_int(2).unwrap());
        assert!(
            plus.mul_int(2).unwrap() > basis,
            "two Plus months must beat the one Basis month they replace"
        );
    }

    /// `ElterngeldPlus` can never exceed the Basiselterngeld it derives from.
    #[test]
    fn elterngeld_plus_never_exceeds_the_basis_amount() {
        for before in [1_000_i64, 3_000, 6_000] {
            for during in [0_i64, 500, 1_500, 3_000] {
                let request = |variant| ElterngeldRequest {
                    monthly_gross_during: euro(during),
                    variant,
                    ..ElterngeldRequest::full_interruption(euro(before), euro(60_000))
                };
                let basis = compute(&request(Variant::Basis)).monthly_amount;
                let plus = compute(&request(Variant::Plus)).monthly_amount;
                assert!(plus <= basis, "Plus exceeded Basis at {before}/{during}");
            }
        }
    }

    // ---------------------------------------------------------------------
    // The income limit and the bonuses
    // ---------------------------------------------------------------------

    /// § 1 Abs. 8 is a cliff. One euro over the limit removes the whole entitlement, which is
    /// reported as *not entitled* rather than as an amount of zero — the two mean different
    /// things to anyone reading the output.
    #[test]
    fn the_income_limit_is_a_cliff_not_a_taper() {
        let at = |income: i64| {
            compute(&ElterngeldRequest::full_interruption(
                euro(4_000),
                euro(income),
            ))
        };
        let limit = beeg().income_limit_annual.whole_euro_floor().unwrap();

        let under = at(limit);
        assert!(under.entitled);
        assert!(under.monthly_amount > Money::ZERO);

        let over = at(limit + 1);
        assert!(!over.entitled);
        assert_eq!(over.monthly_amount, Money::ZERO);
    }

    /// § 2a Abs. 1: ten percent, but never less than 75 €.
    #[test]
    fn the_sibling_bonus_has_a_floor_of_seventy_five_euro() {
        let with_bonus = |gross: i64| {
            compute(&ElterngeldRequest {
                sibling_bonus: true,
                ..ElterngeldRequest::full_interruption(euro(gross), euro(60_000))
            })
        };

        // At the minimum benefit, 10 % would be 30 € — so the 75 € floor applies.
        let low = with_bonus(0);
        assert_eq!(low.sibling_bonus, beeg().sibling_bonus_minimum);
        assert_eq!(
            low.monthly_amount,
            beeg().minimum_monthly.add(euro(75)).unwrap()
        );

        // At the maximum, 10 % of 1 800 € is 180 €, well above the floor.
        let high = with_bonus(15_000);
        assert_eq!(high.sibling_bonus, euro(180));
        // And the bonus is not itself capped by the 1 800 € maximum.
        assert!(high.monthly_amount > beeg().maximum_monthly);
    }

    /// § 2a Abs. 4: 300 € for each further child of a multiple birth.
    #[test]
    fn twins_add_three_hundred_euro() {
        let twins = compute(&ElterngeldRequest {
            additional_children: 1,
            ..ElterngeldRequest::full_interruption(euro(4_000), euro(60_000))
        });
        let single = full_break(4_000);
        assert_eq!(twins.multiple_birth_supplement, euro(300));
        assert_eq!(
            twins.monthly_amount,
            single.monthly_amount.add(euro(300)).unwrap()
        );

        let triplets = compute(&ElterngeldRequest {
            additional_children: 2,
            ..ElterngeldRequest::full_interruption(euro(4_000), euro(60_000))
        });
        assert_eq!(triplets.multiple_birth_supplement, euro(600));
    }

    /// Every reported figure must reconcile with the total, or the breakdown would be
    /// decorative rather than checkable.
    #[test]
    fn the_parts_sum_to_the_total() {
        for gross in [0_i64, 1_500, 4_000, 15_000] {
            for (sibling, extra) in [(false, 0_u8), (true, 0), (false, 2), (true, 1)] {
                let r = compute(&ElterngeldRequest {
                    sibling_bonus: sibling,
                    additional_children: extra,
                    ..ElterngeldRequest::full_interruption(euro(gross), euro(60_000))
                });
                assert_eq!(
                    r.monthly_amount,
                    r.before_bonuses
                        .add(r.sibling_bonus)
                        .unwrap()
                        .add(r.multiple_birth_supplement)
                        .unwrap()
                );
            }
        }
    }
}
