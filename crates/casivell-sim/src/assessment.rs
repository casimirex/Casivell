//! The annual assessment, run inside the projection.
//!
//! # What this closes
//!
//! Until now every projected month took Lohnsteuer withholding as the tax actually paid.
//! Withholding is not the tax: § 39b computes it from each month in isolation, as though the
//! year continued unchanged, and the Steuerbescheid then settles the difference. For a
//! household whose year is flat the gap is small. For one that took unpaid leave, went part
//! time, started work in July or has children it is not small at all — and those are exactly
//! the cases `casivell-sim` exists to model. A projection that ignored the assessment would
//! show a career break costing more than it does, because the refund that follows it would
//! never arrive.
//!
//! # The lag is part of the answer
//!
//! A refund is not received in the year it is earned. The return is filed and the Bescheid
//! follows, so the money lands well into the following year — which matters to a cash-flow
//! projection in a way it does not to a tax calculation. [`SETTLEMENT_LAG_MONTHS`] carries
//! that, and a household modelling a gap year sees the refund arrive when it actually would.
//!
//! # Where an assessment is refused rather than approximated
//!
//! The kernel models one employment. Three circumstances make an assessment for that
//! employment alone meaningless rather than merely imprecise, and [`NoAssessment`] names
//! each. In every one the projection falls back to withholding — which is what the household
//! actually pays month to month — and reports why, rather than producing a confident figure
//! from an input it does not have.

use casivell_core::{Money, MoneyError, Rate};
use casivell_income::{Assessment, AssessmentLaw, Contributions, Employee, assess, taxable_income};
use casivell_lawdata::{SocialParameters, TaxClass};
use casivell_payroll::{Employment, HealthCover};
use casivell_social::{ContributionSplit, SocialContributions};
use casivell_tax::FilingStatus;

use crate::timeline::MONTHS_PER_YEAR;

/// Months after the end of a tax year before its settlement reaches the household.
///
/// Seven, putting the money at the end of the following July. § 149 Abs. 2 AO sets the
/// filing deadline for a self-prepared return at 31 July, and the Bescheid follows within
/// weeks; a household that files early gets it sooner and one that uses a Steuerberater
/// later. The point of modelling the lag at all is that a refund is *not* current-year cash,
/// so the exact month matters far less than the fact that it is not month zero.
pub const SETTLEMENT_LAG_MONTHS: u32 = 7;

/// The kernel keeps one settlement slot, which is only sound while the lag is under a year.
///
/// With a longer lag two assessments could be outstanding at once and the second would
/// overwrite the first, silently losing a refund. A compile-time check rather than a runtime
/// one, because it is a property of the constant and not of any run.
const _: () = assert!(
    SETTLEMENT_LAG_MONTHS < MONTHS_PER_YEAR,
    "a lag of a year or more would allow two settlements to be outstanding at once"
);

/// Why no annual assessment was run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoAssessment {
    /// Tax class IV or V: married, with a spouse who also earns.
    ///
    /// Both spouses are assessed together on their combined income, and the kernel models
    /// one employment. Assessing this salary alone would apply the Splittingtarif to half a
    /// household's income and produce a large fictitious refund.
    ///
    /// Class III is *not* here: it describes a married couple whose other spouse has no
    /// employment income, which is a household the kernel models correctly and completely.
    SpouseIncomeUnknown,
    /// Tax class VI: a second or further employment.
    ///
    /// The class exists precisely because another job holds the allowances, so this salary
    /// is by definition not the whole picture.
    SecondEmployment,
    /// Private health and care cover.
    ///
    /// § 10 Abs. 1 Nr. 3 deducts the *Basisabsicherung* portion of a private premium, which
    /// is a figure the insurer certifies and the kernel is not given. Deducting the whole
    /// premium would overstate the deduction and deducting none would understate it, so
    /// neither is done.
    PrivateHealthCover,
}

impl core::fmt::Display for NoAssessment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let text = match *self {
            Self::SpouseIncomeUnknown => "tax class IV or V: the spouse's income is not modelled",
            Self::SecondEmployment => "tax class VI: this is not the only employment",
            Self::PrivateHealthCover => {
                "private cover: the deductible Basisabsicherung portion is not known"
            }
        };
        f.write_str(text)
    }
}

/// Whether an annual assessment is meaningful for this employment, and under which
/// filing status.
///
/// Exposed so a caller can say *why* a projection fell back to withholding, instead of the
/// user noticing that no refund ever arrived and drawing their own conclusion.
///
/// # Errors
///
/// [`NoAssessment`] when the employment alone does not determine the household's assessment.
pub const fn filing_status_for(employment: &Employment) -> Result<FilingStatus, NoAssessment> {
    if let HealthCover::Private { .. } = employment.health_cover {
        return Err(NoAssessment::PrivateHealthCover);
    }
    match employment.tax_class {
        TaxClass::Class1 | TaxClass::Class2 => Ok(FilingStatus::Individual),
        // A single-earner married couple: the Splittingtarif applies to exactly the income
        // the kernel has, so the assessment is complete rather than partial.
        TaxClass::Class3 => Ok(FilingStatus::JointSplitting),
        TaxClass::Class4 | TaxClass::Class5 => Err(NoAssessment::SpouseIncomeUnknown),
        TaxClass::Class6 => Err(NoAssessment::SecondEmployment),
    }
}

/// A year's employment income and withholding, accumulated month by month.
///
/// Accumulated rather than derived from an annual salary, because the whole reason the
/// assessment is interesting is that the twelve months need not be alike.
#[derive(Debug, Clone, Copy)]
pub(crate) struct YearTally {
    /// The calendar year being accumulated.
    pub(crate) year: u16,
    /// Months of this calendar year actually simulated.
    ///
    /// Below twelve for the first and last years of a projection that does not start in
    /// January, which is a real partial year rather than an error.
    pub(crate) months: u32,

    gross: Money,
    /// § 32b wage-replacement benefits received this calendar year.
    benefits: Money,
    income_tax: Money,
    solidarity_surcharge: Money,
    church_tax: Money,

    /// The year's social insurance, summed in its raw split form.
    ///
    /// Summed here and converted to [`Contributions`] once at year end, so § 10's health
    /// split is rounded once on the annual base rather than twelve times on monthly ones.
    social: SocialContributions,
}

impl YearTally {
    /// An empty tally for `year`.
    pub(crate) const fn new(year: u16) -> Self {
        Self {
            year,
            months: 0,
            gross: Money::ZERO,
            benefits: Money::ZERO,
            income_tax: Money::ZERO,
            solidarity_surcharge: Money::ZERO,
            church_tax: Money::ZERO,
            social: SocialContributions {
                pension: ContributionSplit::ZERO,
                unemployment: ContributionSplit::ZERO,
                health: ContributionSplit::ZERO,
                care: ContributionSplit::ZERO,
                pension_base: Money::ZERO,
                health_base: Money::ZERO,
            },
        }
    }

    /// Adds one month's payslip and any tax-free benefit received alongside it.
    pub(crate) fn add_month(
        &mut self,
        pay: &casivell_payroll::NetPay,
        benefit: Money,
    ) -> Result<(), MoneyError> {
        self.months = self.months.saturating_add(1);
        self.gross = self.gross.add(pay.gross)?;
        self.benefits = self.benefits.add(benefit)?;
        self.income_tax = self.income_tax.add(pay.income_tax)?;
        self.solidarity_surcharge = self.solidarity_surcharge.add(pay.solidarity_surcharge)?;
        self.church_tax = self.church_tax.add(pay.church_tax)?;
        self.social = add_social(&self.social, &pay.monthly_contributions)?;
        Ok(())
    }

    /// Everything withheld during the year: income tax and both surcharges.
    ///
    /// All three are settled by the Bescheid together, so they are compared against the
    /// assessment as one figure.
    fn withheld(&self) -> Result<Money, MoneyError> {
        self.income_tax
            .add(self.solidarity_surcharge)?
            .add(self.church_tax)
    }
}

fn add_split(a: ContributionSplit, b: ContributionSplit) -> Result<ContributionSplit, MoneyError> {
    Ok(ContributionSplit {
        employee: a.employee.add(b.employee)?,
        employer: a.employer.add(b.employer)?,
    })
}

fn add_social(
    a: &SocialContributions,
    b: &SocialContributions,
) -> Result<SocialContributions, MoneyError> {
    Ok(SocialContributions {
        pension: add_split(a.pension, b.pension)?,
        unemployment: add_split(a.unemployment, b.unemployment)?,
        health: add_split(a.health, b.health)?,
        care: add_split(a.care, b.care)?,
        pension_base: a.pension_base.add(b.pension_base)?,
        health_base: a.health_base.add(b.health_base)?,
    })
}

/// The outcome of one year's assessment, and when it is paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnnualSettlement {
    /// The tax year assessed.
    pub year: u16,
    /// Months of that year actually simulated — below twelve for a partial first or last
    /// year, which is the case that produces the largest refunds.
    pub months_assessed: u32,
    /// The full assessment, with every stage of § 2 exposed.
    pub assessment: Assessment,
    /// Withheld less owed: positive is a refund, negative a further demand.
    pub amount: Money,
    /// The zu versteuerndes Einkommen assessed, which the BEEG income limit is measured on.
    pub taxable_income: Money,
    /// The month index at which it reaches the household.
    pub month_index: u32,
}

/// Assesses a completed year and schedules its settlement.
///
/// `finished_at` is the month index of the year's last simulated month, from which the
/// settlement date follows.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub(crate) fn settle_year(
    tally: &YearTally,
    finished_at: u32,
    employment: &Employment,
    filing: FilingStatus,
    social: &SocialParameters,
    law: &AssessmentLaw,
) -> Result<AnnualSettlement, MoneyError> {
    let contributions = Contributions::from_social(
        &tally.social,
        social,
        supplementary_rate(employment),
        // The tally already holds annual sums, so no scaling is wanted here.
        1,
    )?;

    let employee = Employee {
        gross_annual: tally.gross,
        // The Pauschbetrag applies: the kernel has no receipts, and it is what the
        // overwhelming majority of employees get.
        work_expenses: Money::ZERO,
        contributions,
        // § 10 Abs. 1 Nr. 4 deducts church tax *paid* during the year, which for an employee
        // is what was withheld from their salary over those twelve months. The prior year's
        // settlement also lands in the year it is paid; that second-order term is omitted,
        // and it is small — a few euro of deduction on a difference of a few tens.
        church_tax_paid: tally.church_tax,
        other_special_expenses: Money::ZERO,
        // Elterngeld drawn during the year. Untaxed, but § 32b raises the rate on everything
        // above — which is the whole reason the kernel carries it rather than treating it as
        // ordinary non-employment income.
        wage_replacement_benefits: tally.benefits,
        children: 0,
    };

    let income = taxable_income(&employee, &law.deductions)?;
    let assessment = assess(
        &income,
        filing,
        employment.church,
        employment.child_allowance_tenths,
        tally.withheld()?,
        law,
    )?;

    Ok(AnnualSettlement {
        year: tally.year,
        months_assessed: tally.months,
        assessment,
        amount: assessment.refund,
        taxable_income: assessment.taxable_income,
        month_index: finished_at.saturating_add(SETTLEMENT_LAG_MONTHS),
    })
}

/// The fund's supplementary rate, which § 10 needs separately from the general one.
///
/// Only reachable for statutory cover: [`filing_status_for`] refuses private cover before an
/// assessment is ever attempted, so the fallback is unreachable rather than a default.
fn supplementary_rate(employment: &Employment) -> Rate {
    match employment.health_cover {
        HealthCover::Statutory { supplementary_rate } => supplementary_rate,
        HealthCover::Private { .. } => Rate::ZERO,
    }
}

/// Builds the assessment law for a year from a resolved [`casivell_lawdata::LawYear`].
pub(crate) fn assessment_law(law: &casivell_lawdata::LawYear) -> AssessmentLaw {
    AssessmentLaw {
        tariff: law.income_tax,
        solidarity: law.solidarity,
        church: law.church_tax,
        deductions: law.deductions,
    }
}

#[cfg(test)]
mod tests {
    use super::{NoAssessment, filing_status_for};
    use casivell_core::{Money, Rate};
    use casivell_lawdata::{Bundesland, TaxClass};
    use casivell_payroll::{Employment, HealthCover};
    use casivell_social::Insured;
    use casivell_tax::FilingStatus;

    fn employment(class: TaxClass, cover: HealthCover) -> Employment {
        let insured = Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
        Employment::new(insured, class, 0, cover, None).unwrap()
    }

    fn statutory() -> HealthCover {
        HealthCover::Statutory {
            supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
        }
    }

    /// The classes the kernel can assess, and the status each implies.
    #[test]
    fn the_assessable_classes_map_to_the_right_filing_status() {
        for class in [TaxClass::Class1, TaxClass::Class2] {
            assert_eq!(
                filing_status_for(&employment(class, statutory())),
                Ok(FilingStatus::Individual)
            );
        }
        // Class III is a single-earner married couple, which the kernel models completely.
        assert_eq!(
            filing_status_for(&employment(TaxClass::Class3, statutory())),
            Ok(FilingStatus::JointSplitting)
        );
    }

    /// The refusals must be refusals, not approximations. A household with a working spouse
    /// assessed on one salary alone would be shown a large fictitious refund every year.
    #[test]
    fn a_household_the_kernel_cannot_assess_is_refused_rather_than_guessed() {
        for class in [TaxClass::Class4, TaxClass::Class5] {
            assert_eq!(
                filing_status_for(&employment(class, statutory())),
                Err(NoAssessment::SpouseIncomeUnknown)
            );
        }
        assert_eq!(
            filing_status_for(&employment(TaxClass::Class6, statutory())),
            Err(NoAssessment::SecondEmployment)
        );
    }

    /// Private cover is refused whatever the class, because the obstacle is the missing
    /// Basisabsicherung figure rather than the household's shape.
    #[test]
    fn private_cover_is_refused_in_every_class() {
        let private = HealthCover::Private {
            monthly_premium: Money::from_euro(700).unwrap(),
            monthly_employer_subsidy: Money::from_euro(350).unwrap(),
        };
        for class in TaxClass::ALL {
            assert_eq!(
                filing_status_for(&employment(class, private)),
                Err(NoAssessment::PrivateHealthCover)
            );
        }
    }

    /// Every refusal must explain itself in words a report can print.
    #[test]
    fn every_refusal_states_a_reason() {
        use core::fmt::Write as _;
        for reason in [
            NoAssessment::SpouseIncomeUnknown,
            NoAssessment::SecondEmployment,
            NoAssessment::PrivateHealthCover,
        ] {
            let mut text = heapless_line();
            write!(text, "{reason}").unwrap();
            assert!(text.len() > 20, "a reason should be a sentence: {text}");
        }
    }

    /// A tiny owned string, so the test above needs no allocator beyond the test harness's.
    fn heapless_line() -> alloc::string::String {
        alloc::string::String::new()
    }
}
