//! The kernel: one month at a time, streamed to a sink.

use casivell_core::{Money, MoneyError, Rate, Rounding, TaxYear};
use casivell_lawdata::{DataStatus, PayrollParameters};
use casivell_payroll::{NetPay, PayPeriod, PayrollLaw, net_pay};
use casivell_projection::{Assumptions, ProjectionError, project_payroll, resolve};
use casivell_social::EntgeltPoints;

use casivell_income::AssessmentLaw;

use crate::assessment::{
    AnnualSettlement, YearTally, assessment_law, filing_status_for, settle_year,
};
use crate::events::MonthInputs;
use crate::household::{Household, SimulationConfig};

/// Months in a year. Named so the constant never appears bare.
pub const MONTHS_PER_YEAR: u32 = 12;

/// Whether figures are nominal or in the starting year's purchasing power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Basis {
    /// Euro of the day. Honest but hard to read across decades.
    Nominal,
    /// Deflated to the starting year, so amounts are comparable with today's.
    ///
    /// Deflated by the *price inflation* assumption, not by wage growth: the question a
    /// real basis answers is "what could this buy", and that is a price question.
    Real,
}

/// How long a projection runs.
///
/// A newtype with a hard bound, so the kernel's loop has a provable upper limit (JPL R2)
/// and cannot be handed a horizon that would run for minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Horizon {
    months: u32,
}

impl Horizon {
    /// The longest projection permitted: seventy years.
    ///
    /// Longer than any household plan and short enough to stay inside the tariff's
    /// coherence limit, which the projection crate puts at about seventy years under the
    /// default inflation assumption.
    pub const MAX_MONTHS: u32 = 70 * MONTHS_PER_YEAR;

    /// Builds a horizon from whole years.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if it exceeds [`Self::MAX_MONTHS`].
    pub const fn years(years: u32) -> Result<Self, MoneyError> {
        match years.checked_mul(MONTHS_PER_YEAR) {
            Some(months) => Self::months(months),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Builds a horizon from whole months.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if it exceeds [`Self::MAX_MONTHS`].
    pub const fn months(months: u32) -> Result<Self, MoneyError> {
        if months > Self::MAX_MONTHS {
            return Err(MoneyError::OutOfDomain {
                cents: months as i64,
            });
        }
        Ok(Self { months })
    }

    /// The horizon in months.
    #[must_use]
    pub const fn as_months(self) -> u32 {
        self.months
    }
}

/// Anything that can stop a projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SimulationError {
    /// The projection ran past the last year whose statutory parameters can be resolved.
    ///
    /// Carries the year, so a caller can tell the user how far the projection did reach
    /// rather than only that it failed.
    LawUnavailable {
        /// The year that could not be resolved.
        year: u16,
        /// Why.
        cause: ProjectionError,
    },
    /// Arithmetic left the representable domain.
    Arithmetic(MoneyError),
}

impl From<MoneyError> for SimulationError {
    fn from(error: MoneyError) -> Self {
        Self::Arithmetic(error)
    }
}

impl core::fmt::Display for SimulationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::LawUnavailable { year, cause } => {
                write!(f, "no statutory parameters for {year}: {cause}")
            }
            Self::Arithmetic(e) => write!(f, "simulation arithmetic failed: {e}"),
        }
    }
}

impl core::error::Error for SimulationError {}

/// One month of a projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthSnapshot {
    /// Months elapsed since the start, counting from zero.
    pub month_index: u32,
    /// Calendar year.
    pub year: u16,
    /// Calendar month, January being 1.
    pub month: u8,

    /// Gross pay for the month.
    pub gross: Money,
    /// Lohnsteuer withheld.
    pub income_tax: Money,
    /// Solidaritätszuschlag withheld.
    pub solidarity_surcharge: Money,
    /// Church tax withheld.
    pub church_tax: Money,
    /// The employee's social insurance contributions.
    pub employee_contributions: Money,
    /// What reaches the household.
    pub net: Money,

    /// Non-employment income received this month, from a scheduled event.
    ///
    /// Attracts no contributions and earns no Entgeltpunkte, and is reported separately so it
    /// cannot be mistaken for salary.
    pub other_income: Money,
    /// A one-off cost or windfall applied to wealth this month. Negative is a cost.
    pub one_off: Money,
    /// An annual assessment settling this month: positive is a refund, negative a demand.
    ///
    /// Zero in eleven months of twelve, and zero throughout for a household the kernel
    /// declines to assess — see [`crate::assessment::NoAssessment`]. Reported separately from
    /// [`Self::one_off`] because it is not a household decision but a consequence of the
    /// year that preceded it.
    pub tax_settlement: Money,
    /// Expenses for the month.
    pub expenses: Money,
    /// Net less expenses. Negative when the household is drawing down.
    pub savings: Money,
    /// Investment return credited this month.
    pub investment_return: Money,
    /// Wealth at the end of the month.
    pub wealth: Money,

    /// Entgeltpunkte accrued in total by the end of the month.
    pub pension_points: EntgeltPoints,
    /// The monthly statutory pension this entitlement would pay if drawn now, at the
    /// Rentenwert in force.
    ///
    /// Not a forecast of retirement income — no Zugangsfaktor is applied and no
    /// retirement date is assumed — but a running measure of what the record is worth.
    pub accrued_pension: Money,

    /// Whether this month's statutory parameters are enacted law or a projection.
    ///
    /// Carried on every snapshot rather than reported once, because a forty-year run
    /// crosses from one to the other and a chart should be able to show where.
    pub law_status: DataStatus,
    /// Which basis the amounts above are expressed in.
    pub basis: Basis,

    /// Whether employment income was interrupted this month by a scheduled event.
    ///
    /// Reported so a chart can shade the period rather than leaving a reader to infer it from
    /// a zero, which a zero-income month at the start of a projection would also produce.
    pub employment_interrupted: bool,
    /// Whether working hours were reduced this month.
    pub working_time_reduced: bool,
}

/// Receives each month as the kernel produces it.
///
/// The kernel keeps no history, so this is the only way to observe a projection. A sink
/// may store everything, sample it, or reduce it to a single number — see
/// [`Summary`] for the last of those.
pub trait Sink {
    /// Called once per month, in order.
    ///
    /// Returning `false` stops the projection early, which lets a caller run "until
    /// wealth is exhausted" without knowing the horizon in advance.
    fn accept(&mut self, snapshot: &MonthSnapshot) -> bool;
}

impl<F> Sink for F
where
    F: FnMut(&MonthSnapshot) -> bool,
{
    fn accept(&mut self, snapshot: &MonthSnapshot) -> bool {
        self(snapshot)
    }
}

/// A sink that keeps only running aggregates.
///
/// The whole point of the streaming design: a forty-year projection can be reduced to
/// this in constant memory, and ten thousand of them cost no more than one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    /// Months actually simulated.
    pub months: u32,
    /// Wealth at the end.
    pub final_wealth: Money,
    /// The lowest wealth reached at any point.
    ///
    /// The figure that answers "does this plan survive", which an end-state alone hides.
    pub minimum_wealth: Money,
    /// Total net income received.
    pub total_net: Money,
    /// Total tax and surcharges withheld.
    pub total_tax: Money,
    /// Total social insurance contributions paid by the employee.
    pub total_contributions: Money,
    /// Net of every annual settlement received: positive is refunds on balance.
    ///
    /// A projection's *actual* tax burden is [`Self::total_tax`] less this. Kept apart rather
    /// than netted so a household can see how much of what was withheld came back.
    pub total_settlements: Money,
    /// How many annual settlements landed inside the horizon.
    pub settlements_applied: u32,
    /// Entgeltpunkte accrued by the end.
    pub final_pension_points: EntgeltPoints,
    /// The weakest status encountered, so a summary drawn from projected years says so.
    pub law_status: DataStatus,
}

impl Sink for Summary {
    fn accept(&mut self, snapshot: &MonthSnapshot) -> bool {
        if self.months == 0 {
            self.minimum_wealth = snapshot.wealth;
            self.law_status = snapshot.law_status;
        } else {
            self.minimum_wealth = self.minimum_wealth.min(snapshot.wealth);
            self.law_status = self.law_status.weakest(snapshot.law_status);
        }
        self.months = self.months.saturating_add(1);
        self.final_wealth = snapshot.wealth;
        self.final_pension_points = snapshot.pension_points;

        // Saturating rather than checked: a summary is a report, and losing precision at
        // the domain edge is better than aborting a projection that was otherwise fine.
        // The per-month figures are exact; only these totals saturate.
        self.total_net = add_saturating(self.total_net, snapshot.net);
        let tax = add_saturating(
            add_saturating(snapshot.income_tax, snapshot.solidarity_surcharge),
            snapshot.church_tax,
        );
        self.total_tax = add_saturating(self.total_tax, tax);
        self.total_contributions =
            add_saturating(self.total_contributions, snapshot.employee_contributions);
        if !snapshot.tax_settlement.is_zero() {
            self.total_settlements =
                add_saturating(self.total_settlements, snapshot.tax_settlement);
            self.settlements_applied = self.settlements_applied.saturating_add(1);
        }
        true
    }
}

fn add_saturating(a: Money, b: Money) -> Money {
    a.add(b).unwrap_or(a)
}

/// The state carried from one month to the next.
struct State {
    wealth: Money,
    pension_points: EntgeltPoints,
    monthly_gross: Money,
    monthly_expenses: Money,
    /// Contributory income so far this calendar year, for the annual Entgeltpunkte
    /// calculation.
    contributory_this_year: Money,
    /// This calendar year's income and withholding, for the annual assessment.
    tally: YearTally,
    /// The one settlement that can be outstanding at a time.
    ///
    /// One slot suffices because [`crate::assessment::SETTLEMENT_LAG_MONTHS`] is under a
    /// year, which is asserted at compile time where the constant is defined.
    pending: Option<AnnualSettlement>,
}

/// Runs a projection, streaming each month to `sink`.
///
/// # Errors
///
/// [`SimulationError::LawUnavailable`] if a year's statutory parameters cannot be
/// resolved — which for a long enough horizon means the tariff stopped being coherent,
/// and the error names the year. [`SimulationError::Arithmetic`] on a domain violation.
pub fn simulate<S: Sink>(
    household: &Household,
    config: &SimulationConfig,
    sink: &mut S,
) -> Result<(), SimulationError> {
    let mut state = State {
        wealth: household.initial_wealth,
        pension_points: household.initial_pension_points,
        monthly_gross: household.monthly_gross,
        monthly_expenses: household.monthly_expenses,
        contributory_this_year: Money::ZERO,
        tally: YearTally::new(household.start_year.get()),
        pending: None,
    };

    // `Err` is not a failure: it names the reason this household's assessment would be
    // meaningless, and the projection then reports withholding as paid — which is what such
    // a household actually pays month to month.
    let filing = filing_status_for(&household.employment).ok();

    let mut cached: Option<(u16, PayrollLaw, AssessmentLaw)> = None;

    for month_index in 0..config.horizon.as_months() {
        let (year, month) = calendar(household, month_index)?;

        // A calendar year just ended. Assess it *before* the law rolls over, because a year
        // is assessed under its own year's tariff and allowances.
        if year != state.tally.year {
            close_year(&mut state, month_index, household, filing, cached.as_ref())?;
        }

        // Statutory parameters change once a year, so they are resolved once a year. The
        // hot loop must not be re-projecting a tariff twelve times.
        if cached
            .as_ref()
            .is_none_or(|(cached_year, _, _)| *cached_year != year)
        {
            let (payroll, assessment) = resolve_year_law(year, &config.assumptions)?;
            cached = Some((year, payroll, assessment));
        }
        let Some((_, law, _)) = cached.as_ref() else {
            // Unreachable: the branch above always populates it.
            return Err(SimulationError::Arithmetic(MoneyError::Overflow));
        };

        let (pay, inputs) = advance_month(&mut state, month_index, household, law)?;

        // A settlement assessed some months ago reaches the household now.
        let settlement = match state.pending {
            Some(pending) if pending.month_index == month_index => {
                state.pending = None;
                pending.amount
            }
            _ => Money::ZERO,
        };

        accrue_pension(&mut state, &pay, law)?;
        let (investment_return, savings) =
            advance_wealth(&mut state, &pay, &inputs, settlement, config)?;

        let snapshot = build_snapshot(
            month_index,
            year,
            month,
            &pay,
            &state,
            &inputs,
            Flows {
                investment_return,
                savings,
                settlement,
            },
            law,
            config,
        )?;

        if !sink.accept(&snapshot) {
            return Ok(());
        }
    }
    Ok(())
}

/// Applies growth, events and payroll for one month.
///
/// Split out of [`simulate`] so the loop reads as its five steps — close the year, resolve
/// the law, run the month, settle, emit — rather than as one long body.
fn advance_month(
    state: &mut State,
    month_index: u32,
    household: &Household,
    law: &PayrollLaw,
) -> Result<(NetPay, MonthInputs), SimulationError> {
    // Pay and expenses step up once a year, on the anniversary rather than in January,
    // because a raise is a property of the employment and not of the calendar.
    if month_index > 0 && month_index % MONTHS_PER_YEAR == 0 {
        state.monthly_gross = grow(state.monthly_gross, household.annual_pay_growth)?;
        state.monthly_expenses = grow(state.monthly_expenses, household.annual_expense_growth)?;
    }

    // A permanent change rebases the baseline, so the household's own growth compounds
    // from the new figure rather than being overtaken by it. Adopted after this year's
    // growth, so a promotion landing on an anniversary is not immediately inflated.
    let rebase = household.schedule.rebase_at(month_index);
    if let Some(gross) = rebase.gross {
        state.monthly_gross = gross;
    }
    if let Some(expenses) = rebase.expenses {
        state.monthly_expenses = expenses;
    }

    // Transient modifiers then adjust the month. Events change inputs only: tax and
    // contributions still follow from the resulting gross through the same verified code
    // that produces a payslip, so an event cannot invent a tax rule.
    let inputs = household
        .schedule
        .resolve(month_index, state.monthly_gross, state.monthly_expenses)
        .map_err(SimulationError::Arithmetic)?;

    let pay = net_pay(inputs.gross, PayPeriod::Month, &household.employment, law)
        .map_err(SimulationError::Arithmetic)?;
    state
        .tally
        .add_month(&pay)
        .map_err(SimulationError::Arithmetic)?;

    Ok((pay, inputs))
}

/// Assesses the calendar year that just ended and schedules its settlement.
///
/// Called on the first month of a new calendar year, so `month_index` is one past the
/// assessed year's last month. Does nothing when the household is one the kernel declines to
/// assess, or on the very first month, when no year has yet been accumulated.
///
/// The projection's *final* calendar year is deliberately never assessed: there is no
/// following month to trigger it, and its settlement would in any case fall past the horizon
/// by [`crate::assessment::SETTLEMENT_LAG_MONTHS`]. A refund that arrives after the
/// projection ends is one the household does not have yet.
fn close_year(
    state: &mut State,
    month_index: u32,
    household: &Household,
    filing: Option<casivell_tax::FilingStatus>,
    cached: Option<&(u16, PayrollLaw, AssessmentLaw)>,
) -> Result<(), SimulationError> {
    if let (Some(filing), Some((_, payroll, assessment))) = (filing, cached)
        && state.tally.months > 0
    {
        state.pending = Some(
            settle_year(
                &state.tally,
                month_index.saturating_sub(1),
                &household.employment,
                filing,
                &payroll.social,
                assessment,
            )
            .map_err(SimulationError::Arithmetic)?,
        );
    }
    // Entgeltpunkte are a calendar-year entitlement, so the contributory total resets here
    // rather than on the employment anniversary. For a projection starting in January the
    // two coincide; for one starting in July they do not, and resetting on the anniversary
    // would accrue points over a July-to-June window the statute knows nothing about.
    state.contributory_this_year = Money::ZERO;
    state.tally = YearTally::new(calendar_year_of(state.tally.year).saturating_add(1));
    Ok(())
}

/// The calendar year a tally belongs to. A named step, so the successor above reads as one.
const fn calendar_year_of(year: u16) -> u16 {
    year
}

/// The calendar year and month `month_index` months after the start.
fn calendar(household: &Household, month_index: u32) -> Result<(u16, u8), SimulationError> {
    let zero_based = u32::from(household.start_month)
        .checked_sub(1)
        .ok_or(MoneyError::Overflow)?
        .checked_add(month_index)
        .ok_or(MoneyError::Overflow)?;
    let years = zero_based
        .checked_div(MONTHS_PER_YEAR)
        .ok_or(MoneyError::Overflow)?;
    let month = zero_based
        .checked_rem(MONTHS_PER_YEAR)
        .ok_or(MoneyError::Overflow)?
        .checked_add(1)
        .ok_or(MoneyError::Overflow)?;
    let year = u32::from(household.start_year.get())
        .checked_add(years)
        .ok_or(MoneyError::Overflow)?;
    Ok((
        u16::try_from(year).map_err(|_| MoneyError::Overflow)?,
        u8::try_from(month).map_err(|_| MoneyError::Overflow)?,
    ))
}

/// Resolves a year's parameters, projecting where no statute exists.
///
/// Returns both views of the same year: the payroll parameters the monthly loop needs, and
/// the assessment parameters December needs. Resolved together so the two can never disagree
/// about which year's law they are on.
fn resolve_year_law(
    year: u16,
    assumptions: &Assumptions,
) -> Result<(PayrollLaw, AssessmentLaw), SimulationError> {
    let fail = |cause: ProjectionError| SimulationError::LawUnavailable { year, cause };
    let tax_year = TaxYear::new(year).map_err(|e| fail(ProjectionError::Arithmetic(e)))?;
    let law = resolve(tax_year, assumptions).map_err(fail)?;

    let steps = tax_year.years_from(TaxYear::LAST_VERIFIED);
    let payroll = if tax_year.has_verified_data() {
        PayrollParameters::for_year(tax_year).map_err(|e| fail(ProjectionError::Arithmetic(e)))?
    } else {
        let base = PayrollParameters::for_year(TaxYear::LAST_VERIFIED)
            .map_err(|e| fail(ProjectionError::Arithmetic(e)))?;
        project_payroll(
            &base,
            &law.social,
            &law.income_tax,
            tax_year,
            steps,
            assumptions,
        )
        .map_err(fail)?
    };

    Ok((
        PayrollLaw {
            year: tax_year,
            payroll,
            tariff: law.income_tax,
            solidarity: law.solidarity,
            church: law.church_tax,
            social: law.social,
        },
        assessment_law(&law),
    ))
}

/// Accrues Entgeltpunkte for the month.
///
/// Entitlement is defined annually — one point for a year at the average wage — so the
/// running total is recomputed from the year's contributory income to date rather than
/// accumulated monthly. That keeps the figure exactly equal to the annual calculation at
/// each December, instead of drifting by twelve roundings.
fn accrue_pension(
    state: &mut State,
    pay: &NetPay,
    law: &PayrollLaw,
) -> Result<(), SimulationError> {
    let contributory = pay.monthly_contributions.pension_base;
    let before = EntgeltPoints::accrued_in_year(state.contributory_this_year, &law.social.pension)
        .map_err(SimulationError::Arithmetic)?;
    state.contributory_this_year = state
        .contributory_this_year
        .add(contributory)
        .map_err(SimulationError::Arithmetic)?;
    let after = EntgeltPoints::accrued_in_year(state.contributory_this_year, &law.social.pension)
        .map_err(SimulationError::Arithmetic)?;

    let gained = EntgeltPoints::from_micro(after.micro().saturating_sub(before.micro()))
        .map_err(SimulationError::Arithmetic)?;
    state.pension_points = state
        .pension_points
        .add(gained)
        .map_err(SimulationError::Arithmetic)?;
    Ok(())
}

/// Credits investment return, then applies the month's savings.
///
/// Return is credited on the opening balance, before the month's savings arrive — the
/// conservative order, and the one that avoids crediting a full month's return to money
/// deposited on the last day.
fn advance_wealth(
    state: &mut State,
    pay: &NetPay,
    inputs: &MonthInputs,
    settlement: Money,
    config: &SimulationConfig,
) -> Result<(Money, Money), SimulationError> {
    let monthly_return = monthly_rate(config.investment_return)?;
    let investment_return = state
        .wealth
        .mul_rate(monthly_return, Rounding::HalfUp)
        .map_err(SimulationError::Arithmetic)?;
    // Non-employment income adds to what is available to save; a one-off is applied to
    // wealth directly, so it is deliberately outside `savings`.
    let savings = pay
        .net
        .add(inputs.other_income)
        .and_then(|available| available.sub(inputs.expenses))
        .map_err(SimulationError::Arithmetic)?;
    state.wealth = state
        .wealth
        .add(investment_return)
        .and_then(|w| w.add(savings))
        .and_then(|w| w.add(inputs.one_off))
        .and_then(|w| w.add(settlement))
        .map_err(SimulationError::Arithmetic)?;
    Ok((investment_return, savings))
}

/// The month's money movements that are not payroll and not events.
#[derive(Debug, Clone, Copy)]
struct Flows {
    investment_return: Money,
    savings: Money,
    settlement: Money,
}

/// Assembles a snapshot, applying the requested basis.
#[expect(
    clippy::too_many_arguments,
    reason = "a snapshot has many independent fields; bundling them into a struct purely \
              to satisfy an argument count would add a type nobody else uses"
)]
fn build_snapshot(
    month_index: u32,
    year: u16,
    month: u8,
    pay: &NetPay,
    state: &State,
    inputs: &MonthInputs,
    flows: Flows,
    law: &PayrollLaw,
    config: &SimulationConfig,
) -> Result<MonthSnapshot, SimulationError> {
    let pension_value = law
        .social
        .pension
        .pension_value_for_month(month)
        .map_err(SimulationError::Arithmetic)?;
    let accrued_pension =
        casivell_social::monthly_pension(state.pension_points, Rate::ONE, Rate::ONE, pension_value)
            .map_err(SimulationError::Arithmetic)?;

    let elapsed_years = month_index
        .checked_div(MONTHS_PER_YEAR)
        .ok_or(MoneyError::Overflow)?;
    let deflate = |amount: Money| -> Result<Money, SimulationError> {
        to_basis(amount, config, elapsed_years).map_err(SimulationError::Arithmetic)
    };

    Ok(MonthSnapshot {
        month_index,
        year,
        month,
        gross: deflate(pay.gross)?,
        income_tax: deflate(pay.income_tax)?,
        solidarity_surcharge: deflate(pay.solidarity_surcharge)?,
        church_tax: deflate(pay.church_tax)?,
        employee_contributions: deflate(pay.employee_contributions)?,
        net: deflate(pay.net)?,
        other_income: deflate(inputs.other_income)?,
        one_off: deflate(inputs.one_off)?,
        tax_settlement: deflate(flows.settlement)?,
        expenses: deflate(inputs.expenses)?,
        savings: deflate(flows.savings)?,
        investment_return: deflate(flows.investment_return)?,
        wealth: deflate(state.wealth)?,
        pension_points: state.pension_points,
        accrued_pension: deflate(accrued_pension)?,
        law_status: statutory_status(law),
        basis: config.basis,
        employment_interrupted: inputs.employment_interrupted,
        working_time_reduced: inputs.working_time_reduced,
    })
}

/// The weakest status among the parameter sets actually used.
fn statutory_status(law: &PayrollLaw) -> DataStatus {
    law.payroll
        .provenance
        .status
        .weakest(law.tariff.provenance.status)
        .weakest(law.social.status())
        .weakest(law.solidarity.provenance.status)
}

/// Converts an amount into the configured basis.
///
/// For [`Basis::Real`], divides by `(1 + π)^years` — undoing exactly the compounding the
/// projection applied, so a household whose pay grows at the inflation rate shows a flat
/// real income rather than a slowly drifting one.
fn to_basis(
    amount: Money,
    config: &SimulationConfig,
    elapsed_years: u32,
) -> Result<Money, MoneyError> {
    if config.basis == Basis::Nominal || elapsed_years == 0 {
        return Ok(amount);
    }
    let inflation = config.assumptions.price_inflation();
    if inflation.is_zero() {
        return Ok(amount);
    }
    let factor = Rate::ONE.add(inflation)?;
    let mut deflated = amount;
    for _ in 0..elapsed_years {
        // Dividing by the factor, expressed as a multiplication by its reciprocal scaled
        // to ppm — done stepwise so each year's rounding matches the projection's own.
        let scaled = deflated
            .cents()
            .checked_mul(Rate::ONE.ppm())
            .ok_or(MoneyError::Overflow)?;
        let cents = casivell_core::div_round_half_up(scaled, factor.ppm())?;
        deflated = Money::from_cents(cents)?;
    }
    Ok(deflated)
}

/// Turns an annual rate into the monthly rate that compounds to it.
///
/// Approximated as `annual / 12` rather than the twelfth root, which would need
/// non-integer arithmetic. The difference over a year is second order — 6 % annual gives
/// 6.17 % compounded monthly at a twelfth each — and the simplification is stated here
/// rather than hidden. A projection whose investment return is itself an assumption does
/// not gain from resolving this to the basis point.
fn monthly_rate(annual: Rate) -> Result<Rate, MoneyError> {
    Rate::from_ppm(casivell_core::div_round_half_up(
        annual.ppm(),
        i64::from(MONTHS_PER_YEAR),
    )?)
}

/// Applies one year of growth to an amount.
fn grow(amount: Money, rate: Rate) -> Result<Money, SimulationError> {
    if rate.is_zero() {
        return Ok(amount);
    }
    let factor = Rate::ONE.add(rate).map_err(SimulationError::Arithmetic)?;
    amount
        .mul_rate(factor, Rounding::HalfUp)
        .map_err(SimulationError::Arithmetic)
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{Basis, Horizon, MonthSnapshot, Sink, Summary, calendar, simulate};
    use crate::household::{Household, SimulationConfig};
    use casivell_core::{Money, MoneyError, Rate, TaxYear};
    use casivell_lawdata::{Bundesland, DataStatus, TaxClass};
    use casivell_payroll::{Employment, HealthCover};
    use casivell_social::Insured;

    fn employment(class: TaxClass) -> Employment {
        let insured = Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
        Employment::new(
            insured,
            class,
            0,
            HealthCover::Statutory {
                supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
            },
            None,
        )
        .unwrap()
    }

    fn household(gross: i64, expenses: i64) -> Household {
        Household::starting_fresh(
            TaxYear::new(2026).unwrap(),
            1,
            employment(TaxClass::Class1),
            Money::from_euro(gross).unwrap(),
            Money::from_euro(expenses).unwrap(),
        )
        .unwrap()
    }

    fn config(years: u32, basis: Basis) -> SimulationConfig {
        SimulationConfig::conservative(Horizon::years(years).unwrap(), basis)
    }

    /// Collects every month. Only a test needs the whole timeline; the kernel never does.
    struct Collect {
        months: Vec<MonthSnapshot>,
    }

    impl Sink for Collect {
        fn accept(&mut self, snapshot: &MonthSnapshot) -> bool {
            self.months.push(*snapshot);
            true
        }
    }

    fn collect(h: &Household, c: &SimulationConfig) -> Vec<MonthSnapshot> {
        let mut sink = Collect { months: Vec::new() };
        simulate(h, c, &mut sink).expect("simulates");
        sink.months
    }

    // ---------------------------------------------------------------------
    // The calendar
    // ---------------------------------------------------------------------

    /// Month indices must map onto real calendar dates, including across year ends. Getting
    /// this wrong would pick the wrong Rentenwert and the wrong statutory year.
    #[test]
    fn the_calendar_advances_correctly_across_year_boundaries() {
        let h = household(4_000, 2_000);
        assert_eq!(calendar(&h, 0).unwrap(), (2026, 1));
        assert_eq!(calendar(&h, 11).unwrap(), (2026, 12));
        assert_eq!(calendar(&h, 12).unwrap(), (2027, 1));
        assert_eq!(calendar(&h, 25).unwrap(), (2028, 2));

        // Starting mid-year must roll over at the right point, not in January.
        let mut july = h;
        july.start_month = 7;
        assert_eq!(calendar(&july, 0).unwrap(), (2026, 7));
        assert_eq!(calendar(&july, 5).unwrap(), (2026, 12));
        assert_eq!(calendar(&july, 6).unwrap(), (2027, 1));
    }

    // ---------------------------------------------------------------------
    // Shape and horizon
    // ---------------------------------------------------------------------

    #[test]
    fn a_projection_produces_one_snapshot_per_month() {
        let months = collect(&household(4_000, 2_000), &config(3, Basis::Nominal));
        assert_eq!(months.len(), 36);
        for (index, snapshot) in months.iter().enumerate() {
            assert_eq!(snapshot.month_index, u32::try_from(index).unwrap());
        }
    }

    #[test]
    fn an_excessive_horizon_is_refused() {
        assert!(matches!(
            Horizon::years(200),
            Err(MoneyError::OutOfDomain { .. })
        ));
        assert!(Horizon::years(70).is_ok());
    }

    /// A sink returning false must stop the run, so a caller can project "until the money
    /// runs out" without knowing the horizon in advance.
    #[test]
    fn a_sink_can_stop_the_projection_early() {
        let mut seen = 0_u32;
        let mut stop_after_five = |_: &MonthSnapshot| {
            seen = seen.saturating_add(1);
            seen < 5
        };
        simulate(
            &household(4_000, 2_000),
            &config(40, Basis::Nominal),
            &mut stop_after_five,
        )
        .expect("simulates");
        assert_eq!(seen, 5);
    }

    // ---------------------------------------------------------------------
    // Consistency with the payslip
    // ---------------------------------------------------------------------

    /// The first month must reproduce the payroll crate exactly. The kernel must not have
    /// its own idea of what a payslip is.
    #[test]
    fn the_first_month_matches_the_payroll_calculation() {
        use casivell_payroll::{PayrollLaw, monthly_net};

        let h = household(4_500, 2_000);
        let months = collect(&h, &config(1, Basis::Nominal));
        let first = months.first().expect("at least one month");

        let law = PayrollLaw::for_year(TaxYear::new(2026).unwrap()).unwrap();
        let pay = monthly_net(h.monthly_gross, &h.employment, &law).unwrap();

        assert_eq!(first.gross, pay.gross);
        assert_eq!(first.net, pay.net);
        assert_eq!(first.income_tax, pay.income_tax);
        assert_eq!(first.employee_contributions, pay.employee_contributions);
    }

    /// Every month must decompose exactly: net plus every deduction equals gross.
    #[test]
    fn every_month_decomposes_exactly() {
        for months in [
            collect(&household(2_500, 1_000), &config(5, Basis::Nominal)),
            collect(&household(9_000, 3_000), &config(5, Basis::Nominal)),
        ] {
            for m in &months {
                let deductions = m
                    .income_tax
                    .add(m.solidarity_surcharge)
                    .unwrap()
                    .add(m.church_tax)
                    .unwrap()
                    .add(m.employee_contributions)
                    .unwrap();
                assert_eq!(
                    m.net.add(deductions).unwrap(),
                    m.gross,
                    "month {} does not decompose",
                    m.month_index
                );
            }
        }
    }

    /// Wealth must move by exactly savings, return and settlement, every month. If this
    /// drifts, money is being created or destroyed somewhere in the loop.
    ///
    /// The settlement term is what the annual assessment added: without it this test failed
    /// at month 18 — July of the second year, which is precisely where the first
    /// Steuerbescheid lands.
    #[test]
    fn wealth_moves_by_exactly_savings_return_and_settlement() {
        let mut c = config(10, Basis::Nominal);
        c.investment_return = Rate::from_percent_millis(6_000).unwrap();
        let months = collect(&household(5_000, 2_500), &c);

        let mut previous = Money::ZERO;
        for m in &months {
            let expected = previous
                .add(m.investment_return)
                .unwrap()
                .add(m.savings)
                .unwrap()
                .add(m.tax_settlement)
                .unwrap();
            assert_eq!(
                m.wealth, expected,
                "wealth drifted at month {}",
                m.month_index
            );
            previous = m.wealth;
        }
    }

    #[test]
    fn savings_are_net_less_expenses() {
        let months = collect(&household(4_000, 1_500), &config(2, Basis::Nominal));
        for m in &months {
            assert_eq!(m.savings, m.net.sub(m.expenses).unwrap());
        }
    }

    // ---------------------------------------------------------------------
    // Crossing from enacted law into projection
    // ---------------------------------------------------------------------

    /// The run must start on enacted law and cross into projection, labelling each month.
    /// This is the property that lets a chart show where certainty ends.
    #[test]
    fn the_projection_labels_where_enacted_law_ends() {
        let months = collect(&household(4_000, 2_000), &config(5, Basis::Nominal));

        let enacted: Vec<_> = months
            .iter()
            .filter(|m| m.law_status == DataStatus::Enacted)
            .collect();
        let projected: Vec<_> = months
            .iter()
            .filter(|m| m.law_status == DataStatus::Projected)
            .collect();

        assert_eq!(enacted.len(), 12, "only 2026 has enacted data");
        assert!(!projected.is_empty(), "later years must be projected");
        for m in &enacted {
            assert_eq!(m.year, 2026);
        }
        for m in &projected {
            assert!(m.year > 2026);
        }
    }

    /// A forty-year projection — the horizon the product promises — must run end to end.
    #[test]
    fn a_forty_year_projection_runs_end_to_end() {
        let mut summary = Summary::default();
        simulate(
            &household(4_500, 2_500),
            &config(40, Basis::Nominal),
            &mut summary,
        )
        .expect("simulates");
        assert_eq!(summary.months, 480);
        assert!(summary.final_wealth > Money::ZERO);
        assert!(summary.final_pension_points.micro() > 20 * 1_000_000);
        // And the summary must report that it rested on projected law.
        assert_eq!(summary.law_status, DataStatus::Projected);
    }

    /// A finding the kernel surfaces, and the reason pay growth is a separate input from
    /// the statutory wage-growth assumption.
    ///
    /// Entgeltpunkte are a *ratio* to the national average wage. A household whose nominal
    /// pay never rises therefore accrues less entitlement every year as the
    /// Durchschnittsentgelt grows past it. Over forty years at 4 500 EUR a month that is
    /// about **25.5 points** against about **41.6** for pay that tracks wages — a pension
    /// roughly two-fifths smaller, from a decision that never appears on a payslip.
    ///
    /// A model with one growth rate for both could not show this, which is why there are
    /// two.
    #[test]
    fn a_salary_that_never_rises_erodes_the_pension_record() {
        let stagnant = household(4_500, 2_500);
        let mut keeping_up = stagnant;
        keeping_up.annual_pay_growth = config(40, Basis::Nominal).assumptions.wage_growth();

        let points = |h: &Household| {
            let mut summary = Summary::default();
            simulate(h, &config(40, Basis::Nominal), &mut summary).expect("simulates");
            summary.final_pension_points.micro()
        };

        let behind = points(&stagnant);
        let level = points(&keeping_up);

        // Both land where the arithmetic says they should, within a point.
        assert!(
            (24_500_000..=26_500_000).contains(&behind),
            "a stagnant salary accrued {behind} micropoints"
        );
        assert!(
            (40_500_000..=42_500_000).contains(&level),
            "pay tracking wages accrued {level} micropoints"
        );
        // And the gap is large enough to be the point of the exercise.
        assert!(
            level > behind + 14_000_000,
            "the erosion should cost well over fourteen points"
        );
    }

    // ---------------------------------------------------------------------
    // Real versus nominal
    // ---------------------------------------------------------------------

    /// Nominal and real must agree in the first year and diverge thereafter, with real
    /// below nominal under positive inflation.
    #[test]
    fn the_real_basis_deflates_later_years() {
        let h = household(4_000, 2_000);
        let nominal = collect(&h, &config(20, Basis::Nominal));
        let real = collect(&h, &config(20, Basis::Real));

        assert_eq!(nominal.len(), real.len());
        // Within the first year there is nothing to deflate yet.
        assert_eq!(nominal[0].net, real[0].net);
        assert_eq!(nominal[11].net, real[11].net);
        // Later, real must be strictly below nominal.
        let last = nominal.len() - 1;
        assert!(
            real[last].net < nominal[last].net,
            "real {} should be below nominal {}",
            real[last].net.cents(),
            nominal[last].net.cents()
        );
        // And the basis must be recorded, so a consumer cannot mistake one for the other.
        assert_eq!(real[last].basis, Basis::Real);
        assert_eq!(nominal[last].basis, Basis::Nominal);
    }

    /// A household whose pay grows exactly at the inflation rate should show a roughly flat
    /// *real* income. This is the test that the deflator undoes the same compounding the
    /// projection applied, rather than merely being some decreasing function.
    #[test]
    fn pay_growing_with_inflation_gives_a_flat_real_income() {
        let mut h = household(4_000, 2_000);
        let mut c = config(25, Basis::Real);
        // Same rate for the household's pay and for prices.
        h.annual_pay_growth = c.assumptions.price_inflation();

        let months = collect(&h, &c);
        let first = months[0].gross.cents();
        let last = months[months.len() - 1].gross.cents();
        // Within a percent over twenty-five years: the residual is annual rounding, not
        // drift. A deflator that did not match the growth would be out by tens of percent.
        let drift = (last - first).abs();
        assert!(
            drift * 100 < first,
            "real gross drifted from {first} to {last} cents"
        );
        let _ = &mut c;
    }

    /// With zero inflation the two bases must be identical, since there is nothing to
    /// deflate by.
    #[test]
    fn zero_inflation_makes_the_two_bases_identical() {
        use casivell_projection::Assumptions;

        let h = household(4_000, 2_000);
        let mut nominal = config(10, Basis::Nominal);
        nominal.assumptions = Assumptions::frozen();
        let mut real = nominal;
        real.basis = Basis::Real;

        let nominal_months = collect(&h, &nominal);
        let real_months = collect(&h, &real);
        assert_eq!(nominal_months.len(), real_months.len());
        for (n, r) in nominal_months.iter().zip(real_months.iter()) {
            // The amounts must be identical; only the recorded basis differs, which is the
            // snapshot correctly saying which basis it is in.
            assert_eq!(n.gross, r.gross);
            assert_eq!(n.net, r.net);
            assert_eq!(n.wealth, r.wealth);
            assert_eq!(n.accrued_pension, r.accrued_pension);
            assert_ne!(n.basis, r.basis);
        }
    }

    // ---------------------------------------------------------------------
    // Monotonicity and pension accrual
    // ---------------------------------------------------------------------

    /// Entgeltpunkte only ever accumulate. A month that reduced the record would be a sign
    /// error in the annual recomputation.
    #[test]
    fn pension_points_never_decrease() {
        let months = collect(&household(4_000, 2_000), &config(15, Basis::Nominal));
        let mut previous = 0_i64;
        for m in &months {
            assert!(
                m.pension_points.micro() >= previous,
                "points fell at month {}",
                m.month_index
            );
            previous = m.pension_points.micro();
        }
        assert!(previous > 0);
    }

    /// A full calendar year at a constant salary must accrue exactly the annual figure, not
    /// twelve monthly approximations of it. This is what the annual recomputation buys.
    #[test]
    fn a_full_year_accrues_exactly_the_annual_entitlement() {
        use casivell_lawdata::SocialParameters;
        use casivell_social::EntgeltPoints;

        let h = household(4_000, 0);
        let months = collect(&h, &config(1, Basis::Nominal));
        let after_twelve = months[11].pension_points;

        let social = SocialParameters::for_year(TaxYear::new(2026).unwrap()).unwrap();
        let annual =
            EntgeltPoints::accrued_in_year(Money::from_euro(4_000 * 12).unwrap(), &social.pension)
                .unwrap();
        assert_eq!(after_twelve, annual);
    }

    /// Income above the contribution ceiling must stop earning points, exactly as it stops
    /// attracting contributions.
    #[test]
    fn pension_accrual_respects_the_contribution_ceiling() {
        let ordinary = collect(&household(8_450, 0), &config(1, Basis::Nominal));
        let far_above = collect(&household(20_000, 0), &config(1, Basis::Nominal));
        assert_eq!(
            ordinary[11].pension_points, far_above[11].pension_points,
            "income above the ceiling must not earn further points"
        );
    }

    /// Pay growth must actually raise gross pay, on the anniversary rather than in January.
    #[test]
    fn pay_growth_applies_on_the_anniversary() {
        let mut h = household(4_000, 2_000);
        h.start_month = 7;
        h.annual_pay_growth = Rate::from_percent_millis(10_000).unwrap();

        let months = collect(&h, &config(3, Basis::Nominal));
        // Months 0..11 are the first year of employment, whatever the calendar says.
        assert_eq!(months[0].gross, months[11].gross);
        assert!(months[12].gross > months[11].gross);
        assert_eq!(months[12].month, 7, "the raise should land in July");
    }

    // ---------------------------------------------------------------------
    // The summary sink
    // ---------------------------------------------------------------------

    /// The summary must agree with the timeline it summarises.
    #[test]
    fn the_summary_agrees_with_the_collected_timeline() {
        let h = household(5_000, 2_000);
        let c = config(12, Basis::Nominal);
        let months = collect(&h, &c);

        let mut summary = Summary::default();
        simulate(&h, &c, &mut summary).expect("simulates");

        assert_eq!(summary.months, u32::try_from(months.len()).unwrap());
        assert_eq!(summary.final_wealth, months[months.len() - 1].wealth);
        assert_eq!(
            summary.final_pension_points,
            months[months.len() - 1].pension_points
        );
        let minimum = months.iter().map(|m| m.wealth).min().expect("non-empty");
        assert_eq!(summary.minimum_wealth, minimum);
    }

    /// The minimum is the figure that decides whether a plan survives, and an end state can
    /// hide a path that passed through insolvency.
    #[test]
    fn the_summary_records_a_dip_the_end_state_would_hide() {
        // Spends more than it earns for a while, then a large raise turns it around.
        let mut h = household(3_000, 3_400);
        h.annual_pay_growth = Rate::from_percent_millis(25_000).unwrap();

        let mut summary = Summary::default();
        simulate(&h, &config(12, Basis::Nominal), &mut summary).expect("simulates");

        assert!(
            summary.minimum_wealth.is_negative(),
            "the household should have gone into deficit"
        );
        assert!(
            summary.final_wealth > summary.minimum_wealth,
            "and recovered by the end"
        );
    }

    #[test]
    fn an_empty_summary_does_not_claim_to_be_law() {
        assert!(!Summary::default().law_status.is_binding_law());
    }

    /// A zero-length horizon must produce nothing rather than one month.
    #[test]
    fn a_zero_horizon_produces_nothing() {
        let months = collect(&household(4_000, 2_000), &config(0, Basis::Nominal));
        assert!(months.is_empty());
    }
}
