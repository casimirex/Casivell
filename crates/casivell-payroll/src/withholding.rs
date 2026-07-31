//! The BMF Programmablaufplan für die maschinelle Berechnung der Lohnsteuer 2026.
//!
//! # Correspondence with the official flowchart
//!
//! The PAP is published as a flowchart of named subroutines. Each is implemented
//! here as a function carrying the PAP's own name in its documentation, so a
//! reviewer can hold the PDF beside the source:
//!
//! | PAP routine | Here |
//! |---|---|
//! | `MRE4JL` | [`PayPeriod::annualise`] |
//! | `MRE4ABZ` | [`withhold`], the `zre4` / `zre4vp` derivation |
//! | `MZTABFB` | `table_allowances` (private) |
//! | `UPEVP`, `MVSPKVPV`, `MVSPHB` | `vorsorgepauschale` (private) |
//! | `MLSTJAHR`, `UPMLST` | `annual_tax` (private) |
//! | `UPTAB26` | `casivell_tax::income_tax` |
//! | `MST5-6`, `UP5-6` | `class_five_six_tax`, `bracketed_tax` (private) |
//! | `MSOLZ` | `solidarity_and_church_base` (private) |
//! | `UPANTEIL` | [`PayPeriod::apportion`] |
//! | `MBERECH` | [`withhold`] |
//!
//! # Rounding: the direction differs per box, and it matters
//!
//! The PAP annotates boxes with a unit and an arrow — `Euro` or `Cent`, up or down.
//! **The direction is not uniform across the document**, and the arrows do not
//! survive text extraction from the PDF, so each one was read off the rendered
//! flowchart:
//!
//! | Box | Routine | Direction |
//! |---|---|---|
//! | `VSP = VSPKVPV + VSPR` | `MVSPKVPV` | **up** |
//! | `VSPN = VSPR + VSPHB` | `MVSPHB` | **up** |
//! | `X = ZX · 1,25`, `X = ZX · 0,75`, `MIST = ZX · 0,14` | `UP5-6` | down |
//! | `X = ZVE / KZTAB` | `UPMLST` | down |
//! | `ST` | `UPTAB26` | down |
//! | `SOLZJ = JBMG · 5,5/100` | `MSOLZ` | down (cent) |
//!
//! The two Vorsorgepauschale boxes rounding *up* is the one genuine trap. Getting
//! them wrong shifts `ZVE` by up to a euro, which shifts the annual tax by one or
//! two euro across most of the income range — and it is invisible in the
//! *besondere* Prüftabelle, where the Vorsorgepauschale happens to land on a whole
//! euro anyway. Only the *allgemeine* table exposes it, which is a good argument for
//! checking against both.
//!
//! The legend compounds the trap by illustrating the up arrow with `Cent↑`, while
//! the only `Cent` annotation in these routines — in `MSOLZ` — points down.
//!
//! Field precisions come from PAP § 3.1–3.2 together with the § 3 convention that
//! *"überschüssige Dezimalstellen sind wegzulassen"* — excess decimals are dropped,
//! not rounded. So `VSPR`, `VSPKVPV`, `VSPALV`, `VSPHB`, `ZVE` and `SOLZJ` carry
//! two decimals (cents); `ANP`, `ST` and `LSTJAHR` are whole euro.
//!
//! One consequence worth recording: `MSOLZ` resolves an open question left in
//! `casivell_tax::solidarity` about the surcharge's rounding direction. In the
//! withholding path it is truncation to the cent.

use casivell_core::{Money, MoneyError, Rate, Rounding, TaxYear};
use casivell_lawdata::{
    Bundesland, ChurchTaxParameters, IncomeTaxTariff, PayrollParameters, SocialParameters,
    SolidarityParameters, TaxClass,
};
use casivell_social::Insured;
use casivell_tax::{FilingStatus, income_tax};

/// The pay period, `LZZ` in the PAP.
///
/// Weekly (`LZZ = 3`) and daily (`LZZ = 4`) are deliberately absent; see the crate
/// documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PayPeriod {
    /// `LZZ = 1`: the figure supplied is already an annual amount.
    Year,
    /// `LZZ = 2`: a calendar month.
    Month,
}

impl PayPeriod {
    /// How many of this period make up a year.
    #[must_use]
    pub const fn periods_per_year(self) -> i64 {
        match self {
            Self::Year => 1,
            Self::Month => 12,
        }
    }

    /// How many calendar months this period spans.
    ///
    /// The inverse of [`Self::periods_per_year`], and a distinct quantity: a year is
    /// *one* period per year but spans *twelve* months. Social insurance ceilings are
    /// monthly, so scaling a contribution to a period needs this and not the other.
    /// Both exist as named methods because using the wrong one is an easy mistake
    /// that produces plausible-looking figures — it did, once, in the CLI's
    /// per-branch percentages.
    #[must_use]
    pub const fn months(self) -> i64 {
        match self {
            Self::Year => 12,
            Self::Month => 1,
        }
    }

    /// `MRE4JL`: scales a period amount up to an annual one.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the annualised amount leaves the domain.
    pub const fn annualise(self, amount: Money) -> Result<Money, MoneyError> {
        amount.mul_int(self.periods_per_year())
    }

    /// `UPANTEIL`: takes this period's share of an annual amount, truncating.
    ///
    /// The PAP truncates (`*) Ergebnis abrunden`), so twelve monthly withholdings
    /// can total slightly less than the annual figure. That under-withholding is
    /// intended: it is settled in the annual assessment.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] on a domain violation.
    pub const fn apportion(self, annual: Money) -> Result<Money, MoneyError> {
        annual.div_int(self.periods_per_year(), Rounding::Floor)
    }
}

/// How the employee's health and long-term care cover is arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthCover {
    /// `PKV = 0`: statutory cover. Carries the employee's own fund's full
    /// Zusatzbeitrag (`KVZ`), which the PAP halves internally.
    ///
    /// The PAP is emphatic that the *fund-specific* rate belongs here and that
    /// "der durchschnittliche Zusatzbeitragssatz ist unmaßgeblich" — the published
    /// average is not a valid input for a real payslip.
    Statutory {
        /// The fund's full supplementary rate, e.g. 2.9 %.
        supplementary_rate: Rate,
    },
    /// `PKV = 1`: private basic health and compulsory care cover.
    ///
    /// Both amounts are **monthly**, whatever the pay period — the PAP specifies
    /// `PKPV` and `PKPVAGZ` as "unabhängig vom Lohnzahlungszeitraum immer als
    /// Monatsbetrag".
    Private {
        /// `PKPV`: the monthly basic premium.
        monthly_premium: Money,
        /// `PKPVAGZ`: the monthly tax-free employer subsidy.
        monthly_employer_subsidy: Money,
    },
}

/// The care-insurance flags the Vorsorgepauschale needs: `PVS`, `PVZ` and `PVA`.
///
/// Derived from an [`Insured`] rather than supplied directly, so that the payroll
/// calculation and the contribution calculation in `casivell-social` cannot
/// disagree about the same person. See [`CareStatus::derive`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CareStatus {
    /// `PVS`: the Saxon contribution split applies.
    pub saxony: bool,
    /// `PVZ`: the childless surcharge applies.
    pub childless: bool,
    /// `PVA`: how many per-child reductions apply, `0..=4`.
    pub child_reductions: u8,
}

impl CareStatus {
    /// Derives the flags from an insured person's circumstances.
    ///
    /// `PVZ` follows Elterneigenschaft and the statutory minimum age, exactly as
    /// the contribution calculation does. `PVA` counts children from the second to
    /// the fifth.
    #[must_use]
    pub fn derive(
        insured: &Insured,
        social: &SocialParameters,
        payroll: &PayrollParameters,
    ) -> Self {
        let care = social.care;
        let childless =
            !insured.is_parent() && insured.age_years() >= care.childless_surcharge_min_age;
        // Children two through five each attract one reduction step.
        let reductions = insured
            .children_under_25()
            .saturating_sub(1)
            .min(payroll.vorsorge_care_max_reductions);
        Self {
            saxony: insured.land().has_higher_employee_care_share(),
            childless,
            child_reductions: reductions,
        }
    }
}

/// Everything about an employment that affects withholding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Employment {
    /// The person's insurance circumstances, shared with `casivell-social`.
    pub insured: Insured,
    /// `STKL`: the Lohnsteuerklasse.
    pub tax_class: TaxClass,
    /// `ZKF` in tenths: the number of Kinderfreibeträge, which the statute allows
    /// to one decimal place. `10` is one full allowance, `5` is a half.
    ///
    /// Tenths rather than a fraction so the input is exact and the statute's single
    /// decimal place is representable without a rational type.
    pub child_allowance_tenths: u16,
    /// `PKV` and its associated amounts.
    pub health_cover: HealthCover,
    /// `KRV = 0`: compulsorily insured in the statutory pension scheme.
    pub statutory_pension: bool,
    /// `ALV = 0`: compulsorily insured against unemployment.
    pub statutory_unemployment: bool,
    /// `R > 0`: a church-tax-levying religious affiliation, and where.
    ///
    /// `None` means no affiliation, in which case the PAP outputs `BK = 0`.
    pub church: Option<Bundesland>,
    /// `JLFREIB`: an annual allowance from the employee's ELStAM record.
    pub annual_allowance: Money,
    /// `JLHINZU`: an annual add-back from the same record.
    pub annual_addition: Money,
    /// `F`: the § 39f Faktor, for a couple who elected the Faktorverfahren.
    ///
    /// Meaningful only in class IV, and only ever below 1 — § 39f Abs. 1 Satz 6 applies the
    /// procedure at all only when the factor comes out under one. Set with
    /// [`Employment::with_factor`], which enforces both.
    pub factor: Option<Rate>,
}

impl Employment {
    /// The largest number of Kinderfreibetrag tenths accepted: twenty children.
    ///
    /// A bound so every product involving it has a provable range (JPL R2).
    pub const MAX_CHILD_ALLOWANCE_TENTHS: u16 = 200;

    /// Describes an ordinary employment with no ELStAM adjustments.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if `child_allowance_tenths` exceeds
    /// [`Self::MAX_CHILD_ALLOWANCE_TENTHS`].
    pub const fn new(
        insured: Insured,
        tax_class: TaxClass,
        child_allowance_tenths: u16,
        health_cover: HealthCover,
        church: Option<Bundesland>,
    ) -> Result<Self, MoneyError> {
        if child_allowance_tenths > Self::MAX_CHILD_ALLOWANCE_TENTHS {
            return Err(MoneyError::OutOfDomain {
                cents: child_allowance_tenths as i64,
            });
        }
        Ok(Self {
            insured,
            tax_class,
            child_allowance_tenths,
            health_cover,
            statutory_pension: true,
            statutory_unemployment: true,
            church,
            annual_allowance: Money::ZERO,
            annual_addition: Money::ZERO,
            factor: None,
        })
    }

    /// Elects the § 39f Faktorverfahren.
    ///
    /// A builder method rather than a parameter on [`Self::new`], because the factor is a
    /// property of a *couple* and cannot be stated when one employment is first described —
    /// it has to be computed from both salaries by
    /// [`crate::factor::faktorverfahren`] first.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if the class is not IV, or if the factor is not strictly
    /// between zero and one. § 39f Abs. 1 Satz 6 makes the procedure available only where the
    /// factor comes out below one, so a factor of one or more is not a rounding edge but a
    /// case where the election does not apply at all.
    pub const fn with_factor(mut self, factor: Rate) -> Result<Self, MoneyError> {
        if !matches!(self.tax_class, TaxClass::Class4) {
            return Err(MoneyError::OutOfDomain { cents: 0 });
        }
        if factor.ppm() <= 0 || factor.ppm() >= Rate::ONE.ppm() {
            return Err(MoneyError::OutOfDomain {
                cents: factor.ppm(),
            });
        }
        self.factor = Some(factor);
        Ok(self)
    }
}

/// The statutory parameters withholding needs, resolved together for one year.
#[derive(Debug, Clone, Copy)]
pub struct PayrollLaw {
    /// The year.
    pub year: TaxYear,
    /// PAP constants.
    pub payroll: PayrollParameters,
    /// The § 32a tariff, shared with the annual assessment.
    pub tariff: IncomeTaxTariff,
    /// Solidaritätszuschlag parameters.
    pub solidarity: SolidarityParameters,
    /// Church tax rates.
    pub church: ChurchTaxParameters,
    /// Social insurance parameters, for contribution rates and the care flags.
    pub social: SocialParameters,
}

impl PayrollLaw {
    /// Resolves every parameter set withholding needs.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if any of them is missing for `year`. In
    /// practice this restricts withholding to years whose PAP has been
    /// transcribed, which is stricter than the tariff's own range — and correctly
    /// so, since the PAP is what withholding follows.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        let payroll = match PayrollParameters::for_year(year) {
            Ok(p) => p,
            Err(e) => return Err(e),
        };
        let tariff = match IncomeTaxTariff::for_year(year) {
            Ok(t) => t,
            Err(e) => return Err(e),
        };
        let solidarity = match SolidarityParameters::for_year(year) {
            Ok(s) => s,
            Err(e) => return Err(e),
        };
        let church = match ChurchTaxParameters::for_year(year) {
            Ok(c) => c,
            Err(e) => return Err(e),
        };
        let social = match SocialParameters::for_year(year) {
            Ok(s) => s,
            Err(e) => return Err(e),
        };
        Ok(Self {
            year,
            payroll,
            tariff,
            solidarity,
            church,
            social,
        })
    }
}

/// The result of a withholding calculation, with its intermediates exposed.
///
/// The intermediates are not debugging aids. A payslip that a user cannot
/// reconcile is a payslip they cannot check, and the Vorsorgepauschale in
/// particular is the figure nobody can reproduce by hand — so it is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Withholding {
    /// `LSTLZZ`: Lohnsteuer for the pay period.
    pub income_tax: Money,
    /// `SOLZLZZ`: Solidaritätszuschlag for the pay period.
    pub solidarity_surcharge: Money,
    /// Church tax for the pay period, or zero with no affiliation.
    pub church_tax: Money,
    /// `BK`: the § 51a EStG assessment base apportioned to the pay period.
    ///
    /// This is the annual tax recomputed **with** Kinderfreibeträge, which is
    /// exactly the § 51a Abs. 2 base that `casivell_tax::church_tax` documents as
    /// not yet implemented for the annual assessment. In the withholding path it
    /// is implemented, and the church tax below is correct for families.
    pub church_tax_base: Money,
    /// `LSTJAHR`: the annual Lohnsteuer this period's figure was derived from.
    pub annual_income_tax: Money,
    /// `JBMG`: the annual § 51a base.
    pub annual_church_tax_base: Money,
    /// `ZVE`: the taxable annual amount the tariff was applied to.
    pub taxable_annual_amount: Money,
    /// `VSP`: the Vorsorgepauschale actually deducted.
    pub vorsorgepauschale: Money,
    /// `ZTABFB`: the fixed table allowances, excluding the Vorsorgepauschale.
    pub table_allowances: Money,
}

/// `MBERECH`: computes withholding for one pay period.
///
/// `gross` is `RE4`, the taxable pay for the period.
///
/// # Errors
///
/// [`MoneyError`] if an intermediate leaves the representable domain.
pub fn withhold(
    gross: Money,
    period: PayPeriod,
    employment: &Employment,
    law: &PayrollLaw,
) -> Result<Withholding, MoneyError> {
    let class = employment.tax_class;

    // MRE4JL, then MRE4ABZ.
    let annual_gross = period.annualise(gross.floor_at_zero())?;
    let zre4 = annual_gross
        .sub(employment.annual_allowance)?
        .add(employment.annual_addition)?
        .floor_at_zero();
    // The Vorsorgepauschale is computed on the unreduced annual pay: contributions
    // are levied on gross, not on gross less tax allowances.
    let zre4vp = annual_gross;

    // MZTABFB.
    let table_allowances = table_allowances(zre4, class, &law.payroll)?;
    let child_allowance = child_allowance(employment, &law.payroll)?;

    // UPEVP.
    let vsp = vorsorgepauschale(zre4vp, employment, law)?;

    // MLSTJAHR for the Lohnsteuer itself.
    let taxable = zre4.sub(table_allowances)?.sub(vsp)?;
    let annual_income_tax = apply_factor(annual_tax(taxable, class, law)?, employment)?;

    // MLSTJAHR again, with Kinderfreibeträge, for the § 51a base. The factor applies here
    // too: § 39f Abs. 3 puts the Solidaritätszuschlag and the church tax on the factored
    // amount, so a couple that elected the procedure is not surcharged on an unfactored base.
    let annual_church_tax_base = if child_allowance.is_zero() {
        annual_income_tax
    } else {
        let with_children = table_allowances.add(child_allowance)?;
        let taxable_51a = zre4.sub(with_children)?.sub(vsp)?;
        apply_factor(annual_tax(taxable_51a, class, law)?, employment)?
    };

    // MSOLZ.
    let (annual_solidarity, annual_base) =
        solidarity_and_church_base(annual_church_tax_base, class, employment, &law.solidarity)?;

    let church_tax_base = period.apportion(annual_base)?;
    let church_tax = match employment.church {
        Some(land) => church_tax_base.mul_rate(law.church.rate_in(land), Rounding::Floor)?,
        None => Money::ZERO,
    };

    Ok(Withholding {
        income_tax: period.apportion(annual_income_tax)?,
        solidarity_surcharge: period.apportion(annual_solidarity)?,
        church_tax,
        church_tax_base,
        annual_income_tax,
        annual_church_tax_base,
        taxable_annual_amount: taxable,
        vorsorgepauschale: vsp,
        table_allowances,
    })
}

/// Applies the § 39f Faktor to an annual tax figure.
///
/// `LSTJAHR` is a whole-euro quantity throughout the Programmablaufplan, so the product is
/// truncated back to whole euro rather than left in cents.
fn apply_factor(annual: Money, employment: &Employment) -> Result<Money, MoneyError> {
    match employment.factor {
        Some(factor) => annual.mul_rate(factor, Rounding::Floor)?.floor_to_euro(),
        None => Ok(annual),
    }
}

/// `MZTABFB`: the fixed table allowances, `ZTABFB`.
///
/// The Arbeitnehmer-Pauschbetrag is capped at the pay itself, so a very small
/// salary cannot generate a negative taxable amount through it. Class VI receives
/// neither lump sum.
fn table_allowances(
    zre4: Money,
    class: TaxClass,
    payroll: &PayrollParameters,
) -> Result<Money, MoneyError> {
    // ANP, truncated to whole euro and limited to the pay available.
    let employee_lump_sum = payroll
        .employee_allowance_for(class)
        .min(zre4)
        .floor_to_euro()?;
    let single_parent = if class.has_single_parent_relief() {
        payroll.single_parent_relief
    } else {
        Money::ZERO
    };
    single_parent
        .add(employee_lump_sum)?
        .add(payroll.special_expenses_allowance_for(class))
}

/// `KFB`: the total Kinderfreibetrag, which affects only the § 51a base.
fn child_allowance(
    employment: &Employment,
    payroll: &PayrollParameters,
) -> Result<Money, MoneyError> {
    let per_child = payroll.child_allowance_for(employment.tax_class);
    if per_child.is_zero() || employment.child_allowance_tenths == 0 {
        return Ok(Money::ZERO);
    }
    // ZKF carries one decimal, so scale by tenths and divide by ten.
    per_child
        .mul_int(i64::from(employment.child_allowance_tenths))?
        .div_int(10, Rounding::Floor)
}

/// `UPEVP`, `MVSPKVPV` and `MVSPHB`: the Vorsorgepauschale,
/// § 39b Abs. 2 Satz 5 Nr. 3 EStG.
///
/// The structure is a maximum of two candidates. The first is the pension component
/// plus the actual health and care component. The second replaces the health and
/// care component with the sum of unemployment, health and care capped at 1 900 €,
/// which is better for low earners and worse for high ones. The PAP takes whichever
/// is larger, so the employee always gets the more favourable allowance.
fn vorsorgepauschale(
    zre4vp: Money,
    employment: &Employment,
    law: &PayrollLaw,
) -> Result<Money, MoneyError> {
    let p = &law.payroll;

    // Teilbetrag Rentenversicherung.
    let pension_part = if employment.statutory_pension {
        zre4vp
            .min(p.ceiling_pension_unemployment_annual)
            .mul_rate(p.vorsorge_pension_rate, Rounding::Floor)?
    } else {
        Money::ZERO
    };

    let health_care_part = health_care_component(zre4vp, employment, law)?;
    // `VSP = VSPKVPV + VSPR` is annotated `Euro↑` in the PAP: rounded **up**. Almost
    // every other Euro annotation in the document points down, so this is easy to get
    // wrong — and getting it wrong shifts the annual tax by one or two euro across
    // most of the income range, because it shifts ZVE by up to a euro.
    let mut vsp = health_care_part.add(pension_part)?.ceil_to_euro()?;

    // MVSPHB: the capped alternative, unavailable in class VI.
    if employment.statutory_unemployment && employment.tax_class != TaxClass::Class6 {
        let unemployment_part = zre4vp
            .min(p.ceiling_pension_unemployment_annual)
            .mul_rate(p.vorsorge_unemployment_rate, Rounding::Floor)?;
        let capped = unemployment_part
            .add(health_care_part)?
            .min(p.vorsorge_unemployment_health_cap);
        // `VSPN = VSPR + VSPHB`, also annotated `Euro↑`.
        let alternative = pension_part.add(capped)?.ceil_to_euro()?;
        vsp = vsp.max(alternative);
    }

    Ok(vsp)
}

/// `MVSPKVPV`: the health and care component, `VSPKVPV`.
fn health_care_component(
    zre4vp: Money,
    employment: &Employment,
    law: &PayrollLaw,
) -> Result<Money, MoneyError> {
    let p = &law.payroll;
    match employment.health_cover {
        HealthCover::Private {
            monthly_premium,
            monthly_employer_subsidy,
        } => {
            // Class VI gets no relief for private premiums: the first employment
            // already accounted for them.
            if employment.tax_class == TaxClass::Class6 {
                return Ok(Money::ZERO);
            }
            let premium = monthly_premium.mul_int(12)?;
            let subsidy = monthly_employer_subsidy.mul_int(12)?;
            Ok(premium.sub(subsidy)?.floor_at_zero())
        }
        HealthCover::Statutory { supplementary_rate } => {
            // KVSATZAN = KVZ/2 + 7.0 %. The 7.0 % is half the *reduced* GKV rate,
            // not half the general one — see `casivell_lawdata::payroll`.
            let health_rate = p
                .vorsorge_health_half_rate
                .add(supplementary_rate.half()?)?;
            let care_rate = care_rate(employment, law)?;
            zre4vp
                .min(p.ceiling_health_care_annual)
                .mul_rate(health_rate.add(care_rate)?, Rounding::Floor)
        }
    }
}

/// `PVSATZAN`: the employee's care rate for Vorsorgepauschale purposes.
///
/// The childless surcharge and the per-child reductions are mutually exclusive in
/// the PAP — someone childless has no children to reduce for — so this is a branch,
/// not two independent adjustments.
fn care_rate(employment: &Employment, law: &PayrollLaw) -> Result<Rate, MoneyError> {
    let p = &law.payroll;
    let status = CareStatus::derive(&employment.insured, &law.social, p);

    let base = if status.saxony {
        p.vorsorge_care_rate_saxony
    } else {
        p.vorsorge_care_rate
    };

    if status.childless {
        return base.add(p.vorsorge_care_childless_surcharge);
    }
    let reduction = Rate::from_ppm(
        p.vorsorge_care_child_reduction
            .ppm()
            .checked_mul(i64::from(status.child_reductions))
            .ok_or(MoneyError::Overflow)?,
    )?;
    base.sub(reduction)
}

/// `MLSTJAHR` and `UPMLST`: the annual tax on a taxable amount, `ST`.
fn annual_tax(taxable: Money, class: TaxClass, law: &PayrollLaw) -> Result<Money, MoneyError> {
    if class.uses_class_five_six_formula() {
        // UPMLST divides by KZTAB, which is 1 in classes V and VI.
        let x = taxable.floor_at_zero().floor_to_euro()?;
        return class_five_six_tax(x, law);
    }
    // Classes I–IV go straight through § 32a, with class III doubling. The PAP's
    // `X = ZVE / KZTAB` followed by `ST = ST * KZTAB` is the Splittingverfahren, so
    // the shared tariff evaluator handles it.
    let filing = if class.tariff_divisor() == 2 {
        FilingStatus::JointSplitting
    } else {
        FilingStatus::Individual
    };
    Ok(income_tax(taxable, &law.tariff, filing)?.income_tax)
}

/// `MST5-6`: the tax in classes V and VI, § 39b Abs. 2 Satz 7 EStG.
///
/// Three regimes, split at `W2STKL5`:
///
/// - Above `W2`, the tax is the bracketed tax *at* `W2` plus a flat 42 % (and 45 %
///   above `W3`) on the excess.
/// - Between `W1` and `W2`, it is the lesser of the bracketed tax and the bracketed
///   tax at `W1` plus 42 % on the excess — a cap that stops the bracketed formula
///   from exceeding the marginal rate.
/// - Below `W1`, the bracketed tax alone.
fn class_five_six_tax(x: Money, law: &PayrollLaw) -> Result<Money, MoneyError> {
    let p = &law.payroll;
    let upper = p.class_five_six_upper_rate;

    if x > p.class_five_six_threshold_2 {
        let mut tax = bracketed_tax(p.class_five_six_threshold_2, law)?;
        if x > p.class_five_six_threshold_3 {
            let middle_band = p
                .class_five_six_threshold_3
                .sub(p.class_five_six_threshold_2)?;
            tax = tax.add(
                middle_band
                    .mul_rate(upper, Rounding::Floor)?
                    .floor_to_euro()?,
            )?;
            let top_band = x.sub(p.class_five_six_threshold_3)?;
            let top = top_band.mul_rate(p.class_five_six_top_rate, Rounding::Floor)?;
            return tax.add(top.floor_to_euro()?);
        }
        let excess = x.sub(p.class_five_six_threshold_2)?;
        return tax.add(excess.mul_rate(upper, Rounding::Floor)?.floor_to_euro()?);
    }

    let bracketed = bracketed_tax(x, law)?;
    if x <= p.class_five_six_threshold_1 {
        return Ok(bracketed);
    }
    let at_threshold = bracketed_tax(p.class_five_six_threshold_1, law)?;
    let excess = x.sub(p.class_five_six_threshold_1)?;
    let capped = at_threshold.add(excess.mul_rate(upper, Rounding::Floor)?.floor_to_euro()?)?;
    Ok(capped.min(bracketed))
}

/// `UP5-6`: twice the difference between the tariff at 1.25× and 0.75× the amount,
/// floored at 14 % of it.
///
/// The doubled difference approximates the marginal burden a second earner faces
/// when their partner is in class III. The 14 % floor is the Eingangssteuersatz,
/// which the difference formula can fall below at low incomes.
fn bracketed_tax(amount: Money, law: &PayrollLaw) -> Result<Money, MoneyError> {
    /// 1.25 as parts per million.
    const UPPER_FACTOR_PPM: i64 = 1_250_000;
    /// 0.75 as parts per million.
    const LOWER_FACTOR_PPM: i64 = 750_000;

    let upper_point = amount
        .mul_rate(Rate::from_ppm(UPPER_FACTOR_PPM)?, Rounding::Floor)?
        .floor_to_euro()?;
    let lower_point = amount
        .mul_rate(Rate::from_ppm(LOWER_FACTOR_PPM)?, Rounding::Floor)?
        .floor_to_euro()?;

    let upper_tax = income_tax(upper_point, &law.tariff, FilingStatus::Individual)?.income_tax;
    let lower_tax = income_tax(lower_point, &law.tariff, FilingStatus::Individual)?.income_tax;
    let doubled = upper_tax.sub(lower_tax)?.mul_int(2)?;

    let minimum = amount
        .mul_rate(law.payroll.class_five_six_min_rate, Rounding::Floor)?
        .floor_to_euro()?;
    Ok(doubled.max(minimum))
}

/// `MSOLZ`: the annual Solidaritätszuschlag and the § 51a base to apportion.
///
/// Returns `(SOLZJ, JBMG)`. The Freigrenze is scaled by `KZTAB`, which doubles it
/// in class III — the same doubling the annual assessment applies for a joint
/// return.
///
/// The surcharge is truncated to the cent before the Milderungszone comparison.
/// That is safe: the tapered figure is itself truncated on assignment, and
/// truncating both sides cannot change which branch is taken, because
/// `⌊b⌋ < a ⟺ b < a` when `a` is a whole number of cents.
fn solidarity_and_church_base(
    annual_base: Money,
    class: TaxClass,
    employment: &Employment,
    solidarity: &SolidarityParameters,
) -> Result<(Money, Money), MoneyError> {
    let exemption = solidarity
        .exemption_individual
        .mul_int(class.tariff_divisor())?;

    let surcharge = if annual_base > exemption {
        let headline = annual_base.mul_rate(solidarity.rate, Rounding::Floor)?;
        let tapered = annual_base
            .sub(exemption)?
            .mul_rate(solidarity.taper_rate, Rounding::Floor)?;
        headline.min(tapered)
    } else {
        Money::ZERO
    };

    // BK is only produced for a church member; the PAP sets it to zero otherwise.
    let base = if employment.church.is_some() {
        annual_base
    } else {
        Money::ZERO
    };
    Ok((surcharge, base))
}

#[cfg(test)]
mod tests {
    use super::{
        CareStatus, Employment, HealthCover, PayPeriod, PayrollLaw, Withholding, withhold,
    };
    use casivell_core::{Money, Rate, TaxYear};
    use casivell_lawdata::{Bundesland, TaxClass};
    use casivell_social::Insured;

    fn law() -> PayrollLaw {
        PayrollLaw::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn statutory() -> HealthCover {
        HealthCover::Statutory {
            supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
        }
    }

    fn employment(class: TaxClass, children: u8, is_parent: bool) -> Employment {
        let insured = Insured::new(
            40,
            is_parent,
            children,
            Bundesland::NordrheinWestfalen,
            None,
        )
        .unwrap();
        Employment::new(insured, class, 0, statutory(), None).unwrap()
    }

    fn annual(gross_euro: i64, employment: &Employment) -> Withholding {
        withhold(
            Money::from_euro(gross_euro).unwrap(),
            PayPeriod::Year,
            employment,
            &law(),
        )
        .unwrap()
    }

    // ---------------------------------------------------------------------
    // Table allowances
    // ---------------------------------------------------------------------

    /// Classes I–V get the 1 230 EUR Arbeitnehmer-Pauschbetrag plus the 36 EUR
    /// Sonderausgaben-Pauschbetrag; class II adds the 4 260 EUR single-parent relief;
    /// class VI gets nothing at all.
    #[test]
    fn the_table_allowances_follow_the_tax_class() {
        let expected_cents = |class| match class {
            TaxClass::Class2 => (4_260 + 1_230 + 36) * 100,
            TaxClass::Class6 => 0,
            _ => (1_230 + 36) * 100,
        };
        for class in TaxClass::ALL {
            let w = annual(40_000, &employment(class, 0, false));
            assert_eq!(
                w.table_allowances.cents(),
                expected_cents(class),
                "wrong ZTABFB for {class:?}"
            );
        }
    }

    /// The Arbeitnehmer-Pauschbetrag cannot exceed the pay it is deducted from, so a
    /// tiny salary produces a zero taxable amount rather than a negative one.
    #[test]
    fn the_employee_lump_sum_is_capped_at_the_pay() {
        let w = annual(500, &employment(TaxClass::Class1, 0, false));
        // 500 EUR of pay caps ANP at 500, leaving SAP on top.
        assert_eq!(w.table_allowances.cents(), (500 + 36) * 100);
        assert!(w.annual_income_tax.is_zero());
    }

    // ---------------------------------------------------------------------
    // Vorsorgepauschale
    // ---------------------------------------------------------------------

    /// Someone insured in no branch and privately covered with no premium gets no
    /// Vorsorgepauschale at all. This is the floor of the whole computation.
    #[test]
    fn no_insurance_and_no_premium_means_no_vorsorgepauschale() {
        let insured = Insured::new(40, false, 0, Bundesland::Berlin, None).unwrap();
        let mut e = Employment::new(
            insured,
            TaxClass::Class1,
            0,
            HealthCover::Private {
                monthly_premium: Money::ZERO,
                monthly_employer_subsidy: Money::ZERO,
            },
            None,
        )
        .unwrap();
        e.statutory_pension = false;
        e.statutory_unemployment = false;
        assert_eq!(annual(50_000, &e).vorsorgepauschale, Money::ZERO);
    }

    /// The employer's subsidy reduces the deductible private premium, and cannot
    /// push it below zero.
    #[test]
    fn the_employer_subsidy_reduces_the_private_premium_but_not_below_zero() {
        let insured = Insured::new(40, false, 0, Bundesland::Berlin, None).unwrap();
        let build = |premium: i64, subsidy: i64| {
            let mut e = Employment::new(
                insured,
                TaxClass::Class1,
                0,
                HealthCover::Private {
                    monthly_premium: Money::from_euro(premium).unwrap(),
                    monthly_employer_subsidy: Money::from_euro(subsidy).unwrap(),
                },
                None,
            )
            .unwrap();
            e.statutory_pension = false;
            e.statutory_unemployment = false;
            e
        };
        // 400 EUR monthly less a 150 EUR subsidy is 250 EUR, so 3 000 EUR a year.
        assert_eq!(
            annual(60_000, &build(400, 150)).vorsorgepauschale.cents(),
            3_000 * 100
        );
        // A subsidy larger than the premium floors the component at zero.
        assert_eq!(
            annual(60_000, &build(100, 500)).vorsorgepauschale,
            Money::ZERO
        );
    }

    /// The Vorsorgepauschale stops growing once pay passes both ceilings, because
    /// every component is computed on a capped base.
    #[test]
    fn the_vorsorgepauschale_saturates_above_the_ceilings() {
        let e = employment(TaxClass::Class1, 0, false);
        let at_ceiling = annual(101_400, &e).vorsorgepauschale;
        let far_above = annual(400_000, &e).vorsorgepauschale;
        assert_eq!(at_ceiling, far_above);
    }

    /// A childless employee gets a slightly *larger* Vorsorgepauschale than a
    /// parent, because the allowance tracks their higher care contribution. It is
    /// worth pinning the direction: it is the opposite of the intuition that having
    /// children is always tax-favourable.
    #[test]
    fn childlessness_increases_the_vorsorgepauschale() {
        let childless = annual(50_000, &employment(TaxClass::Class1, 0, false));
        let parent = annual(50_000, &employment(TaxClass::Class1, 1, true));
        assert!(childless.vorsorgepauschale > parent.vorsorgepauschale);
        // And so the childless employee's taxable amount is lower on this account,
        // even though their actual contributions are higher.
        assert!(childless.taxable_annual_amount < parent.taxable_annual_amount);
    }

    /// Each additional child from the second to the fifth lowers the care rate and
    /// therefore the Vorsorgepauschale; a sixth changes nothing.
    #[test]
    fn the_vorsorgepauschale_follows_the_child_reduction_ladder() {
        let mut previous = Money::ZERO;
        for children in 1_u8..=5 {
            let vsp =
                annual(60_000, &employment(TaxClass::Class1, children, true)).vorsorgepauschale;
            if children > 1 {
                assert!(
                    vsp < previous,
                    "child {children} did not reduce the Vorsorgepauschale"
                );
            }
            previous = vsp;
        }
        let five = annual(60_000, &employment(TaxClass::Class1, 5, true)).vorsorgepauschale;
        let six = annual(60_000, &employment(TaxClass::Class1, 6, true)).vorsorgepauschale;
        assert_eq!(five, six, "the reduction must cap at the fifth child");
    }

    /// A Saxon employee's higher care share raises their Vorsorgepauschale.
    #[test]
    fn saxony_raises_the_vorsorgepauschale() {
        let saxon = Insured::new(40, false, 0, Bundesland::Sachsen, None).unwrap();
        let saxon_employment =
            Employment::new(saxon, TaxClass::Class1, 0, statutory(), None).unwrap();
        let elsewhere = employment(TaxClass::Class1, 0, false);
        assert!(
            annual(50_000, &saxon_employment).vorsorgepauschale
                > annual(50_000, &elsewhere).vorsorgepauschale
        );
    }

    /// The care flags must be derived from the same profile the contributions use,
    /// so that one person cannot be childless for tax and a parent for insurance.
    #[test]
    fn the_care_flags_are_derived_from_the_insured_profile() {
        let law = law();
        let childless = Insured::new(30, false, 0, Bundesland::Sachsen, None).unwrap();
        let status = CareStatus::derive(&childless, &law.social, &law.payroll);
        assert!(status.childless);
        assert!(status.saxony);
        assert_eq!(status.child_reductions, 0);

        let parent = Insured::new(45, true, 4, Bundesland::Bayern, None).unwrap();
        let status = CareStatus::derive(&parent, &law.social, &law.payroll);
        assert!(!status.childless);
        assert!(!status.saxony);
        assert_eq!(status.child_reductions, 3);

        // Below the surcharge age, a childless person is still not surcharged.
        let young = Insured::new(20, false, 0, Bundesland::Berlin, None).unwrap();
        assert!(!CareStatus::derive(&young, &law.social, &law.payroll).childless);
    }

    // ---------------------------------------------------------------------
    // Kinderfreibetrag and the church tax base
    // ---------------------------------------------------------------------

    /// § 51a EStG recomputes the base with the full Kinderfreibetrag, so a family's
    /// church tax base is *lower* than their Lohnsteuer. This is exactly the
    /// correction `casivell_tax::church_tax` documents as missing from the annual
    /// assessment, and it is implemented here.
    #[test]
    fn children_lower_the_church_tax_base_below_the_income_tax() {
        let insured = Insured::new(40, true, 2, Bundesland::NordrheinWestfalen, None).unwrap();
        let with_children = Employment::new(
            insured,
            TaxClass::Class1,
            20, // two full Kinderfreibeträge
            statutory(),
            Some(Bundesland::NordrheinWestfalen),
        )
        .unwrap();
        let w = annual(70_000, &with_children);
        assert!(
            w.annual_church_tax_base < w.annual_income_tax,
            "the § 51a base must be below the Lohnsteuer when children are present"
        );
        // And the church tax is levied on the lower base, not the Lohnsteuer.
        let on_income_tax = w
            .annual_income_tax
            .mul_rate(
                law().church.rate_in(Bundesland::NordrheinWestfalen),
                casivell_core::Rounding::Floor,
            )
            .unwrap();
        assert!(w.church_tax < on_income_tax);
    }

    /// Without children the two bases coincide.
    #[test]
    fn without_children_the_church_tax_base_equals_the_income_tax() {
        let insured = Insured::new(40, false, 0, Bundesland::Bayern, None).unwrap();
        let e = Employment::new(
            insured,
            TaxClass::Class1,
            0,
            statutory(),
            Some(Bundesland::Bayern),
        )
        .unwrap();
        let w = annual(60_000, &e);
        assert_eq!(w.annual_church_tax_base, w.annual_income_tax);
    }

    /// Classes V and VI get no Kinderfreibetrag, so children cannot lower their
    /// church tax base.
    #[test]
    fn classes_five_and_six_get_no_child_allowance() {
        let insured = Insured::new(40, true, 2, Bundesland::Berlin, None).unwrap();
        for class in [TaxClass::Class5, TaxClass::Class6] {
            let e =
                Employment::new(insured, class, 20, statutory(), Some(Bundesland::Berlin)).unwrap();
            let w = annual(50_000, &e);
            assert_eq!(
                w.annual_church_tax_base, w.annual_income_tax,
                "{class:?} must get no Kinderfreibetrag"
            );
        }
    }

    /// No affiliation means no base and no church tax, per the PAP's `BK = 0`.
    #[test]
    fn no_church_affiliation_produces_no_base() {
        let w = annual(60_000, &employment(TaxClass::Class1, 0, false));
        assert_eq!(w.church_tax_base, Money::ZERO);
        assert_eq!(w.church_tax, Money::ZERO);
    }

    // ---------------------------------------------------------------------
    // Solidaritätszuschlag
    // ---------------------------------------------------------------------

    /// The Freigrenze is scaled by KZTAB, so class III is exempt to twice the tax of
    /// class I. A salary that attracts the surcharge in class I must not in class III.
    #[test]
    fn the_solidarity_freigrenze_doubles_in_class_three() {
        let class1 = annual(140_000, &employment(TaxClass::Class1, 0, false));
        let class3 = annual(140_000, &employment(TaxClass::Class3, 0, false));
        assert!(class1.solidarity_surcharge > Money::ZERO);
        assert_eq!(class3.solidarity_surcharge, Money::ZERO);
    }

    /// Below the Freigrenze there is no surcharge at all.
    #[test]
    fn modest_salaries_attract_no_solidarity_surcharge() {
        for gross in [30_000_i64, 50_000, 70_000] {
            let w = annual(gross, &employment(TaxClass::Class1, 0, false));
            assert_eq!(
                w.solidarity_surcharge,
                Money::ZERO,
                "{gross} EUR should be below the Soli Freigrenze"
            );
        }
    }

    /// The surcharge never exceeds 5.5 % of the base it is levied on.
    #[test]
    fn the_solidarity_surcharge_stays_within_its_headline_rate() {
        let mut gross = 0_i64;
        while gross <= 400_000 {
            let w = annual(gross, &employment(TaxClass::Class1, 0, false));
            let cap = w
                .annual_income_tax
                .mul_rate(
                    Rate::from_percent_millis(5_500).unwrap(),
                    casivell_core::Rounding::Ceiling,
                )
                .unwrap();
            assert!(
                w.solidarity_surcharge <= cap,
                "the surcharge exceeded 5.5 % at {gross} EUR"
            );
            gross = gross.saturating_add(4_567);
        }
    }

    // ---------------------------------------------------------------------
    // Properties
    // ---------------------------------------------------------------------

    /// Withholding is monotonically non-decreasing in pay, for every class. The
    /// class V/VI ladder has three regimes and two caps, which is ample room for a
    /// non-monotonicity to hide.
    #[test]
    fn withholding_is_monotonic_in_pay_for_every_class() {
        for class in TaxClass::ALL {
            let e = employment(class, 0, false);
            let mut previous = Money::ZERO;
            let mut gross = 0_i64;
            while gross <= 300_000 {
                let tax = annual(gross, &e).annual_income_tax;
                assert!(
                    tax >= previous,
                    "{class:?}: tax fell at {gross} EUR, from {} to {} cents",
                    previous.cents(),
                    tax.cents()
                );
                previous = tax;
                gross = gross.saturating_add(1_013);
            }
        }
    }

    /// Tax never exceeds pay, and is never negative.
    #[test]
    fn withholding_stays_between_zero_and_the_pay() {
        for class in TaxClass::ALL {
            let e = employment(class, 0, false);
            let mut gross = 0_i64;
            while gross <= 300_000 {
                let w = annual(gross, &e);
                assert!(!w.annual_income_tax.is_negative());
                assert!(
                    w.annual_income_tax <= Money::from_euro(gross).unwrap(),
                    "{class:?}: tax exceeded pay at {gross} EUR"
                );
                gross = gross.saturating_add(2_731);
            }
        }
    }

    /// The marginal rate never exceeds the 45 % top rate, in any class. In classes V
    /// and VI the bracketed formula can approach it, so the bound is worth checking
    /// rather than assuming.
    #[test]
    fn the_marginal_rate_never_exceeds_the_top_rate() {
        for class in TaxClass::ALL {
            let e = employment(class, 0, false);
            let mut gross = 10_000_i64;
            while gross <= 250_000 {
                let next = gross.saturating_add(1_000);
                let here = annual(gross, &e)
                    .annual_income_tax
                    .whole_euro_floor()
                    .unwrap();
                let there = annual(next, &e)
                    .annual_income_tax
                    .whole_euro_floor()
                    .unwrap();
                let marginal = there.saturating_sub(here);
                assert!(
                    (0..=450).contains(&marginal),
                    "{class:?}: 1 000 EUR above {gross} attracted {marginal} EUR"
                );
                gross = next;
            }
        }
    }

    /// Zero and negative pay produce no withholding rather than an error.
    #[test]
    fn no_pay_means_no_withholding() {
        for gross in [Money::ZERO, Money::from_euro(-5_000).unwrap()] {
            let w = withhold(
                gross,
                PayPeriod::Year,
                &employment(TaxClass::Class1, 0, false),
                &law(),
            )
            .unwrap();
            assert_eq!(w.annual_income_tax, Money::ZERO);
            assert_eq!(w.solidarity_surcharge, Money::ZERO);
        }
    }

    /// An ELStAM allowance lowers the taxable amount; an add-back raises it. Neither
    /// touches the Vorsorgepauschale, which is computed on unreduced pay.
    #[test]
    fn elstam_adjustments_move_the_taxable_amount_but_not_the_vorsorgepauschale() {
        let base = employment(TaxClass::Class1, 0, false);
        let mut with_allowance = base;
        with_allowance.annual_allowance = Money::from_euro(3_000).unwrap();
        let mut with_addition = base;
        with_addition.annual_addition = Money::from_euro(3_000).unwrap();

        let plain = annual(60_000, &base);
        let reduced = annual(60_000, &with_allowance);
        let raised = annual(60_000, &with_addition);

        assert!(reduced.annual_income_tax < plain.annual_income_tax);
        assert!(raised.annual_income_tax > plain.annual_income_tax);
        // The Vorsorgepauschale is levied on gross, so it is unaffected.
        assert_eq!(reduced.vorsorgepauschale, plain.vorsorgepauschale);
        assert_eq!(raised.vorsorgepauschale, plain.vorsorgepauschale);
    }

    /// A monthly run and an annual run on twelve times the salary must agree on the
    /// annual figures, since the monthly path simply annualises its input.
    #[test]
    fn monthly_and_annual_paths_agree_on_the_annual_figures() {
        let e = employment(TaxClass::Class1, 0, false);
        for monthly_euro in [1_500_i64, 3_000, 5_000, 9_000] {
            let monthly = withhold(
                Money::from_euro(monthly_euro).unwrap(),
                PayPeriod::Month,
                &e,
                &law(),
            )
            .unwrap();
            let yearly = annual(monthly_euro * 12, &e);
            assert_eq!(monthly.annual_income_tax, yearly.annual_income_tax);
            assert_eq!(monthly.vorsorgepauschale, yearly.vorsorgepauschale);
            assert_eq!(monthly.taxable_annual_amount, yearly.taxable_annual_amount);
        }
    }
}
