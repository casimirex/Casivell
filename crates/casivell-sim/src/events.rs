//! Life events: changes to a household at points along the timeline.
//!
//! # Why a schedule rather than twelve special cases
//!
//! Every life event has the same shape — *from month N, something about the household is
//! different*. Part-time work scales pay. A career break removes it. A house purchase swaps
//! rent for a mortgage payment. Twelve fields bolted onto [`crate::Household`] would each
//! need their own interaction with the other eleven; one ordered schedule of modifiers needs
//! none.
//!
//! So the kernel resolves the household's state for each month by starting from its baseline
//! and applying whichever events are active. That keeps the events independent of each other
//! and independent of the kernel, and it is what makes adding the thirteenth cheap.
//!
//! # Bounded, as everything here is
//!
//! A [`Schedule`] holds a fixed array. The crate is `#![no_std]` and cannot allocate, so the
//! event count has a hard limit — which also gives the per-month resolution a provable upper
//! bound (JPL R2). Thirty-two events is more than any household plan states explicitly.
//!
//! # What an event may and may not change
//!
//! Events change *inputs*: pay, expenses, one-off amounts, and non-employment income. They
//! never change tax or contributions directly. Those follow from the inputs through the same
//! verified code that produces a payslip, so an event cannot accidentally invent a tax rule.
//!
//! # Permanent changes rebase; transient ones modify
//!
//! Two kinds, and conflating them is a bug the test suite caught rather than a distinction for
//! its own sake.
//!
//! A **permanent** change — a promotion, a move to a cheaper flat — replaces the baseline, and
//! the household's own growth then compounds from the *new* figure. [`Schedule::rebase_at`]
//! reports these, and the kernel adopts them.
//!
//! A **transient** modifier — reduced hours, a career break, a windfall — leaves the baseline
//! alone and adjusts the month. [`Schedule::resolve`] applies these.
//!
//! Treating a promotion as a transient override was the original design, and it was wrong in a
//! way that only showed up decades out: the override held a fixed amount while the baseline
//! kept growing behind it, so after about twenty years at 2.8 % the "promotion" had quietly
//! become a pay cut. A forty-year test caught it; no unit test would have.

use casivell_benefits::Variant;
use casivell_core::{Money, MoneyError, Rate, Rounding};

/// A change to the household from some month onward.
///
/// Months are indices into the projection, counting from zero at its start — not calendar
/// months. A projection beginning in July 2026 has month 0 = July 2026.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Gross pay becomes a new amount, permanently, and grows from there.
    ///
    /// A promotion or a job change. This *rebases* the baseline, so the household's own pay
    /// growth compounds from the new figure — see the module documentation on why that is not
    /// the same as overriding it each month.
    PayChange {
        /// First month the new pay applies to.
        from_month: u32,
        /// The new monthly gross.
        monthly_gross: Money,
    },

    /// Working time is scaled for a period, scaling gross pay with it.
    ///
    /// The *Teilzeitfalle*: pay falls immediately, and pension entitlement falls with it and
    /// does not recover when the hours do. `fraction` is the share of full time retained, so
    /// `60 %` is a three-day week.
    WorkingTime {
        /// First month at the reduced hours.
        from_month: u32,
        /// First month back at full time, or `None` for permanent.
        until_month: Option<u32>,
        /// The share of full-time pay retained.
        fraction: Rate,
    },

    /// Employment income stops entirely for a period, with no replacement.
    ///
    /// A sabbatical or unpaid leave. Distinct from [`Event::WorkingTime`] at zero, because
    /// the intent differs and the report should say which the user asked for.
    ///
    /// # What this does not model
    ///
    /// Health and long-term care cover does not stop when pay does. Someone on unpaid leave
    /// stays insured and pays the contributions themselves — several hundred euro a month —
    /// which this event does not add. Model it with a concurrent
    /// [`Event::ExpenseChange`] until that is handled properly.
    UnpaidLeave {
        /// First month without pay.
        from_month: u32,
        /// First month back at work, or `None` for permanent.
        until_month: Option<u32>,
    },

    /// Monthly expenses become a new amount, permanently, and grow from there.
    ///
    /// Rebases the baseline, as [`Event::PayChange`] does.
    ExpenseChange {
        /// First month the new figure applies to.
        from_month: u32,
        /// The new monthly expenses.
        monthly_expenses: Money,
    },

    /// A single cost or windfall in one month.
    ///
    /// Negative for a cost, positive for a windfall. Applied to wealth directly, without
    /// touching income or tax — so it is the right shape for a deposit on a house or an
    /// inheritance already taxed elsewhere, and the wrong shape for a bonus, which is
    /// employment income under § 39b Abs. 3.
    OneOff {
        /// The month it falls in.
        month: u32,
        /// The amount. Negative is a cost.
        amount: Money,
    },

    /// A child is born.
    ///
    /// Distinct from [`Event::ParentalLeave`], and deliberately so: § 56 SGB VI credits
    /// Kindererziehungszeiten to whoever *raises* the child, whether or not they stop working.
    /// A parent who returns to work the following month still receives all thirty-six months
    /// of credit, so deriving the credit from a leave would deny it to exactly the households
    /// that took none.
    ///
    /// # What this changes and what it does not
    ///
    /// It credits pension entitlement, and nothing else. Kindergeld, the Kinderfreibetrag and
    /// the care-insurance child reductions are still static properties of the employment
    /// rather than things a birth switches on — so a projection wanting those must set them
    /// there as well. Stated because a `ChildBorn` event that silently did only a third of
    /// what its name suggests would be worse than one that says so.
    ChildBorn {
        /// The month of the birth. The Kindererziehungszeit begins the month *after*.
        month: u32,
    },

    /// Parental leave with Elterngeld, for a number of Lebensmonate.
    ///
    /// The one event whose payment the kernel computes rather than being told: the amount
    /// follows from the pre-birth salary through the BEEG, so a household states *when* and
    /// *how* it takes leave, not how much it will receive.
    ///
    /// Unlike [`Event::OtherIncome`], the money is carried into the annual assessment as a
    /// § 32b benefit, so the rate increase on the household's other income is modelled too.
    /// That is the half of parental leave nobody budgets for: the benefit arrives untaxed all
    /// year and the demand lands with the Steuerbescheid the following summer.
    ///
    /// # What this does not model
    ///
    /// **Kindererziehungszeiten.** § 56 SGB VI credits a parent with pension entitlement for
    /// the first thirty-six months after a birth, worth about one Entgeltpunkt a year. Without
    /// it the projection **overstates** the pension cost of taking leave. The direction is
    /// stated because it matters: this is the one place where the model is currently
    /// pessimistic rather than neutral.
    ///
    /// Health and care cover during the leave is not modelled either, for the same reason it
    /// is not under [`Event::UnpaidLeave`].
    ParentalLeave {
        /// First month of the leave.
        from_month: u32,
        /// How many Lebensmonate are drawn.
        months: u32,
        /// The share of full-time pay still earned during the leave.
        ///
        /// Zero for a full break. Anything higher reduces the benefit through the § 2 Abs. 3
        /// difference rule, and is what makes [`Variant::Plus`] worth choosing.
        working_fraction: Rate,
        /// Basiselterngeld or `ElterngeldPlus`.
        variant: Variant,
        /// Whether the § 2a Abs. 1 Geschwisterbonus applies.
        sibling_bonus: bool,
        /// Further children of a multiple birth, for the § 2a Abs. 4 supplement.
        additional_children: u8,
    },

    /// Income that is not employment income, for a period.
    ///
    /// Attracts no social insurance contributions and earns no Entgeltpunkte. Reported
    /// separately so it cannot be mistaken for salary.
    ///
    /// # Tax treatment is the caller's problem, deliberately
    ///
    /// Different non-employment income is taxed entirely differently — Elterngeld is tax-free
    /// but raises the rate on everything else under § 32b EStG; rental income is ordinary
    /// income; capital gains have their own flat rate. This event therefore adds the money
    /// and applies **no** tax to it, which is right for none of them in general.
    ///
    /// It exists so a household can model a known net amount. Elterngeld specifically is not
    /// yet modelled, because doing it correctly needs the Progressionsvorbehalt — see the
    /// crate documentation.
    OtherIncome {
        /// First month it is received.
        from_month: u32,
        /// First month it stops, or `None` for permanent.
        until_month: Option<u32>,
        /// The monthly amount, net of any tax the caller has already accounted for.
        monthly_amount: Money,
    },
}

impl Event {
    /// The first month this event affects.
    ///
    /// Named `start_month` rather than `from_month` because a `from_*` method reads as a
    /// constructor, and the field it returns is already called `from_month`.
    #[must_use]
    pub const fn start_month(&self) -> u32 {
        match *self {
            Self::PayChange { from_month, .. }
            | Self::WorkingTime { from_month, .. }
            | Self::UnpaidLeave { from_month, .. }
            | Self::ExpenseChange { from_month, .. }
            | Self::ParentalLeave { from_month, .. }
            | Self::OtherIncome { from_month, .. } => from_month,
            Self::OneOff { month, .. } | Self::ChildBorn { month, .. } => month,
        }
    }

    /// Whether this event is active in `month_index`.
    #[must_use]
    pub const fn active_in(&self, month_index: u32) -> bool {
        let started = month_index >= self.start_month();
        if !started {
            return false;
        }
        match *self {
            // Both are instants. A birth's Kindererziehungszeit is a separate question that
            // `Schedule::child_raising_active` answers, because § 56 Abs. 5 lets one child's
            // period push another's later and no single event can know that alone.
            Self::OneOff { month, .. } | Self::ChildBorn { month, .. } => month_index == month,
            Self::WorkingTime { until_month, .. }
            | Self::UnpaidLeave { until_month, .. }
            | Self::OtherIncome { until_month, .. } => match until_month {
                Some(end) => month_index < end,
                None => true,
            },
            Self::ParentalLeave {
                from_month, months, ..
            } => month_index < from_month.saturating_add(months),
            // Permanent changes stay in force once they start.
            Self::PayChange { .. } | Self::ExpenseChange { .. } => true,
        }
    }
}

/// A permanent change to the baseline, taking effect in some month.
///
/// Applied by the kernel before its own growth, so growth then compounds from the new figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rebase {
    /// The new monthly gross, if a [`Event::PayChange`] lands this month.
    pub gross: Option<Money>,
    /// The new monthly expenses, if an [`Event::ExpenseChange`] does.
    pub expenses: Option<Money>,
}

impl Rebase {
    /// Whether anything changes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.gross.is_none() && self.expenses.is_none()
    }
}

/// The household's inputs for one month, after events are applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthInputs {
    /// Employment gross pay, which drives tax and contributions.
    pub gross: Money,
    /// Non-employment income, which does not.
    pub other_income: Money,
    /// Monthly expenses.
    pub expenses: Money,
    /// A one-off amount applied to wealth this month. Negative is a cost.
    pub one_off: Money,
    /// Whether employment income was interrupted this month.
    ///
    /// Reported so a chart can shade the period and a summary can count it, rather than
    /// leaving the reader to infer it from a zero.
    pub employment_interrupted: bool,
    /// Whether hours were reduced this month.
    pub working_time_reduced: bool,
    /// Parental leave active this month, if any.
    ///
    /// Carries what the *benefit* needs rather than the benefit itself: the amount depends on
    /// the payroll law and the pre-birth salary, which the schedule does not hold. The kernel
    /// computes it — see [`crate::assessment`] for why the schedule never computes money that
    /// depends on statute.
    pub parental_leave: Option<ParentalLeaveMonth>,
}

/// A month of parental leave, as the schedule describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParentalLeaveMonth {
    /// Basiselterngeld or `ElterngeldPlus`.
    pub variant: Variant,
    /// Whether the Geschwisterbonus applies.
    pub sibling_bonus: bool,
    /// Further children of a multiple birth.
    pub additional_children: u8,
    /// Which month of the leave this is, counting from zero.
    ///
    /// Reported so the kernel can recognise the first month and fix the Bemessungsentgelt
    /// there, as the BEEG does: the amount is set by the year before the birth and does not
    /// move as the household's baseline salary grows underneath it.
    pub month_of_leave: u32,
}

/// An ordered set of life events.
///
/// Built with [`Schedule::with`], which keeps the events sorted by start month so resolution
/// applies them in the order they occur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    events: [Option<Event>; Self::MAX_EVENTS],
    count: usize,
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new()
    }
}

impl Schedule {
    /// The most events a schedule holds.
    ///
    /// A hard bound because the crate cannot allocate, and because it gives the per-month
    /// resolution a provable upper limit. More than any household states explicitly.
    pub const MAX_EVENTS: usize = 32;

    /// An empty schedule: the household proceeds unchanged.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            events: [None; Self::MAX_EVENTS],
            count: 0,
        }
    }

    /// Adds an event, keeping the schedule ordered by start month.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] when the schedule is full, or when an event's window ends
    /// before it starts — which is a caller mistake that would otherwise silently produce an
    /// event that never fires.
    pub fn with(mut self, event: Event) -> Result<Self, MoneyError> {
        if self.count >= Self::MAX_EVENTS {
            return Err(MoneyError::OutOfDomain {
                cents: i64::try_from(Self::MAX_EVENTS).unwrap_or(i64::MAX),
            });
        }
        if let Some(end) = window_end(&event) {
            if end <= event.start_month() {
                return Err(MoneyError::OutOfDomain {
                    cents: i64::from(end),
                });
            }
        }

        // Insertion sort by start month. The schedule is small and built once, so the
        // simplest correct approach is the right one.
        let mut position = self.count;
        while position > 0 {
            let previous = position.saturating_sub(1);
            let Some(Some(earlier)) = self.events.get(previous) else {
                break;
            };
            if earlier.start_month() <= event.start_month() {
                break;
            }
            let moved = *earlier;
            if let Some(slot) = self.events.get_mut(position) {
                *slot = Some(moved);
            }
            position = previous;
        }
        if let Some(slot) = self.events.get_mut(position) {
            *slot = Some(event);
        }
        self.count = self.count.saturating_add(1);
        Ok(self)
    }

    /// The events, in the order they occur.
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.events.iter().take(self.count).flatten()
    }

    /// How many events the schedule holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.count
    }

    /// Whether the schedule is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Whether `month_index` falls inside a Kindererziehungszeit.
    ///
    /// # § 56 Abs. 5 extends rather than overlapping
    ///
    /// A child's period runs for thirty-six calendar months from the month after its birth.
    /// Where a second child arrives while the first's period is still running, Satz 2
    /// *extends* the period "um die Anzahl an Kalendermonaten der gleichzeitigen Erziehung"
    /// — it does not run two periods in parallel. A parent of three children under three
    /// therefore accrues one child's credit at a time for nine years, not three at once for
    /// three.
    ///
    /// Modelled by queueing: each birth's window starts at the later of the month after the
    /// birth and the end of the previous window, so the windows abut without ever overlapping.
    /// That reproduces the extension exactly, and for well-spaced children reduces to the
    /// plain thirty-six months apiece.
    ///
    /// `months_per_child` comes from the statute via `casivell-lawdata`, so the thirty-six is
    /// not written here.
    #[must_use]
    pub fn child_raising_active(&self, month_index: u32, months_per_child: u32) -> bool {
        let mut window_end = 0_u32;
        for event in self.events() {
            let Event::ChildBorn { month } = *event else {
                continue;
            };
            // Events are kept in start-month order, so births arrive oldest first and the
            // cursor only ever moves forward.
            let start = month.saturating_add(1).max(window_end);
            window_end = start.saturating_add(months_per_child);
            if month_index >= start && month_index < window_end {
                return true;
            }
        }
        false
    }

    /// The permanent changes taking effect exactly in `month_index`.
    ///
    /// The kernel adopts these as the new baseline before applying its own growth, so a
    /// promotion compounds. Where several land in the same month the last in schedule order
    /// wins, which is the later-added one at the same start month.
    #[must_use]
    pub fn rebase_at(&self, month_index: u32) -> Rebase {
        let mut rebase = Rebase::default();
        for event in self.events() {
            match *event {
                Event::PayChange {
                    from_month,
                    monthly_gross,
                } if from_month == month_index => rebase.gross = Some(monthly_gross),
                Event::ExpenseChange {
                    from_month,
                    monthly_expenses,
                } if from_month == month_index => rebase.expenses = Some(monthly_expenses),
                _ => {}
            }
        }
        rebase
    }

    /// Resolves the household's inputs for one month.
    ///
    /// `baseline_gross` and `baseline_expenses` are the figures after the household's own
    /// growth has been applied, before any event. Events then modify them.
    ///
    /// Permanent changes are **not** applied here — the kernel has already folded them into
    /// the baseline via [`Schedule::rebase_at`]. This applies the transient modifiers only.
    ///
    /// # Precedence
    ///
    /// [`Event::UnpaidLeave`] overrides pay entirely, and [`Event::WorkingTime`] scales
    /// whatever survives. Leave beats reduced hours because stopping work is not a special
    /// case of working less.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] on a domain violation.
    pub fn resolve(
        &self,
        month_index: u32,
        baseline_gross: Money,
        baseline_expenses: Money,
    ) -> Result<MonthInputs, MoneyError> {
        let mut gross = baseline_gross;
        let expenses = baseline_expenses;
        let mut other_income = Money::ZERO;
        let mut one_off = Money::ZERO;
        let mut interrupted = false;
        let mut reduced = false;
        let mut fraction: Option<Rate> = None;
        let mut parental_leave = None;

        for event in self.events() {
            if !event.active_in(month_index) {
                continue;
            }
            match *event {
                // Permanent changes are already in the baseline (see `rebase_at`), and a
                // birth changes no monthly input at all — it credits pension entitlement,
                // which `Schedule::child_raising_active` reports separately.
                Event::PayChange { .. } | Event::ExpenseChange { .. } | Event::ChildBorn { .. } => {
                }
                Event::WorkingTime {
                    fraction: share, ..
                } => fraction = Some(share),
                Event::UnpaidLeave { .. } => interrupted = true,
                Event::OtherIncome { monthly_amount, .. } => {
                    other_income = other_income.add(monthly_amount)?;
                }
                Event::OneOff { amount, .. } => one_off = one_off.add(amount)?,
                Event::ParentalLeave {
                    from_month,
                    working_fraction,
                    variant,
                    sibling_bonus,
                    additional_children,
                    ..
                } => {
                    parental_leave = Some(ParentalLeaveMonth {
                        variant,
                        sibling_bonus,
                        additional_children,
                        month_of_leave: month_index.saturating_sub(from_month),
                    });
                    // Parental leave sets the hours directly. A full break is
                    // `working_fraction` of zero, which the branch below turns into an
                    // interruption rather than a reduction — the two are reported
                    // differently and a household asked for a leave, not for part-time.
                    fraction = Some(working_fraction);
                }
            }
        }

        // Leave overrides reduced hours: stopping work is not working less.
        if interrupted || matches!(fraction, Some(share) if share.is_zero()) {
            gross = Money::ZERO;
            interrupted = true;
        } else if let Some(share) = fraction {
            gross = gross.mul_rate(share, Rounding::HalfUp)?;
            reduced = true;
        }

        Ok(MonthInputs {
            gross,
            other_income,
            expenses,
            one_off,
            employment_interrupted: interrupted,
            working_time_reduced: reduced,
            parental_leave,
        })
    }
}

/// The exclusive end of an event's window, if it has one.
const fn window_end(event: &Event) -> Option<u32> {
    match *event {
        Event::WorkingTime { until_month, .. }
        | Event::UnpaidLeave { until_month, .. }
        | Event::OtherIncome { until_month, .. } => until_month,
        Event::ParentalLeave {
            from_month, months, ..
        } => Some(from_month.saturating_add(months)),
        Event::PayChange { .. }
        | Event::ExpenseChange { .. }
        | Event::OneOff { .. }
        | Event::ChildBorn { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Event, Schedule};
    use casivell_core::{Money, MoneyError, Rate};

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    fn resolve(schedule: &Schedule, month: u32) -> super::MonthInputs {
        schedule
            .resolve(month, euro(4_000), euro(2_000))
            .expect("resolves")
    }

    #[test]
    fn an_empty_schedule_leaves_the_baseline_alone() {
        let inputs = resolve(&Schedule::new(), 0);
        assert_eq!(inputs.gross, euro(4_000));
        assert_eq!(inputs.expenses, euro(2_000));
        assert_eq!(inputs.other_income, Money::ZERO);
        assert!(!inputs.employment_interrupted);
        assert!(!inputs.working_time_reduced);
    }

    // ---------------------------------------------------------------------
    // Windows
    // ---------------------------------------------------------------------

    /// A windowed event must apply on its first month and stop on its `until_month`, which is
    /// exclusive. An inclusive end would silently run every event one month long.
    #[test]
    fn a_window_is_half_open() {
        let schedule = Schedule::new()
            .with(Event::UnpaidLeave {
                from_month: 12,
                until_month: Some(24),
            })
            .expect("valid");

        assert!(!resolve(&schedule, 11).employment_interrupted);
        assert!(resolve(&schedule, 12).employment_interrupted);
        assert!(resolve(&schedule, 23).employment_interrupted);
        assert!(!resolve(&schedule, 24).employment_interrupted);
    }

    #[test]
    fn an_open_ended_window_never_stops() {
        let schedule = Schedule::new()
            .with(Event::UnpaidLeave {
                from_month: 6,
                until_month: None,
            })
            .expect("valid");
        assert!(!resolve(&schedule, 5).employment_interrupted);
        assert!(resolve(&schedule, 600).employment_interrupted);
    }

    /// A window ending before it starts would never fire, so it is refused rather than
    /// silently ignored.
    #[test]
    fn an_inverted_window_is_refused() {
        for event in [
            Event::UnpaidLeave {
                from_month: 24,
                until_month: Some(12),
            },
            Event::WorkingTime {
                from_month: 10,
                until_month: Some(10),
                fraction: Rate::ONE,
            },
        ] {
            assert!(matches!(
                Schedule::new().with(event),
                Err(MoneyError::OutOfDomain { .. })
            ));
        }
    }

    // ---------------------------------------------------------------------
    // Individual events
    // ---------------------------------------------------------------------

    /// A permanent change is reported as a *rebase*, in the month it lands and no other. The
    /// kernel adopts it as the new baseline, which is what makes later growth compound from it.
    #[test]
    fn a_pay_change_rebases_in_exactly_one_month() {
        let schedule = Schedule::new()
            .with(Event::PayChange {
                from_month: 24,
                monthly_gross: euro(5_500),
            })
            .expect("valid");

        assert!(schedule.rebase_at(23).is_empty());
        assert_eq!(schedule.rebase_at(24).gross, Some(euro(5_500)));
        assert!(schedule.rebase_at(25).is_empty());

        // And `resolve` leaves it alone, because the baseline already carries it.
        assert_eq!(resolve(&schedule, 24).gross, euro(4_000));
    }

    #[test]
    fn an_expense_change_rebases_the_same_way() {
        let schedule = Schedule::new()
            .with(Event::ExpenseChange {
                from_month: 12,
                monthly_expenses: euro(3_000),
            })
            .expect("valid");
        assert!(schedule.rebase_at(11).is_empty());
        assert_eq!(schedule.rebase_at(12).expenses, Some(euro(3_000)));
        assert!(schedule.rebase_at(13).is_empty());
    }

    /// Where two permanent changes land in the same month, the later-added one wins. A silent
    /// tie would make the result depend on insertion order in a way nothing documented.
    #[test]
    fn concurrent_rebases_resolve_to_the_last_added() {
        let schedule = Schedule::new()
            .with(Event::PayChange {
                from_month: 10,
                monthly_gross: euro(5_000),
            })
            .expect("valid")
            .with(Event::PayChange {
                from_month: 10,
                monthly_gross: euro(6_000),
            })
            .expect("valid");
        assert_eq!(schedule.rebase_at(10).gross, Some(euro(6_000)));
    }

    #[test]
    fn reduced_working_time_scales_pay() {
        let schedule = Schedule::new()
            .with(Event::WorkingTime {
                from_month: 0,
                until_month: None,
                fraction: Rate::from_percent_millis(60_000).unwrap(),
            })
            .expect("valid");
        let inputs = resolve(&schedule, 0);
        assert_eq!(inputs.gross, euro(2_400));
        assert!(inputs.working_time_reduced);
    }

    #[test]
    fn unpaid_leave_removes_pay_entirely() {
        let schedule = Schedule::new()
            .with(Event::UnpaidLeave {
                from_month: 0,
                until_month: Some(12),
            })
            .expect("valid");
        let inputs = resolve(&schedule, 0);
        assert_eq!(inputs.gross, Money::ZERO);
        assert!(inputs.employment_interrupted);
        // Expenses continue, which is the whole difficulty of a career break.
        assert_eq!(inputs.expenses, euro(2_000));
    }

    #[test]
    fn a_one_off_lands_in_exactly_one_month() {
        let schedule = Schedule::new()
            .with(Event::OneOff {
                month: 18,
                amount: euro(-40_000),
            })
            .expect("valid");
        assert_eq!(resolve(&schedule, 17).one_off, Money::ZERO);
        assert_eq!(resolve(&schedule, 18).one_off, euro(-40_000));
        assert_eq!(resolve(&schedule, 19).one_off, Money::ZERO);
    }

    #[test]
    fn other_income_is_kept_separate_from_pay() {
        let schedule = Schedule::new()
            .with(Event::OtherIncome {
                from_month: 0,
                until_month: Some(12),
                monthly_amount: euro(1_800),
            })
            .expect("valid");
        let inputs = resolve(&schedule, 0);
        assert_eq!(inputs.other_income, euro(1_800));
        // And it does not touch employment income, which is what drives contributions.
        assert_eq!(inputs.gross, euro(4_000));
    }

    // ---------------------------------------------------------------------
    // Interaction
    // ---------------------------------------------------------------------

    /// Leave must beat reduced hours: stopping work is not a special case of working less, and
    /// a household with both scheduled means the leave.
    #[test]
    fn leave_overrides_reduced_hours() {
        let schedule = Schedule::new()
            .with(Event::WorkingTime {
                from_month: 0,
                until_month: None,
                fraction: Rate::from_percent_millis(50_000).unwrap(),
            })
            .expect("valid")
            .with(Event::UnpaidLeave {
                from_month: 6,
                until_month: Some(12),
            })
            .expect("valid");

        assert_eq!(resolve(&schedule, 0).gross, euro(2_000));
        assert_eq!(resolve(&schedule, 6).gross, Money::ZERO);
        assert!(resolve(&schedule, 6).employment_interrupted);
        // And the reduced hours resume once the leave ends.
        assert_eq!(resolve(&schedule, 12).gross, euro(2_000));
    }

    /// Reduced hours scale whatever baseline the kernel supplies, which is how a promotion and
    /// part-time compose: the kernel has already rebased, so the scaling applies to the new pay.
    #[test]
    fn reduced_hours_scale_the_supplied_baseline() {
        let schedule = Schedule::new()
            .with(Event::WorkingTime {
                from_month: 0,
                until_month: None,
                fraction: Rate::from_percent_millis(50_000).unwrap(),
            })
            .expect("valid");

        assert_eq!(resolve(&schedule, 0).gross, euro(2_000));
        // A rebased baseline of 6 000 scales to 3 000, which is what the kernel passes after a
        // promotion.
        let after_promotion = schedule
            .resolve(12, euro(6_000), euro(2_000))
            .expect("resolves");
        assert_eq!(after_promotion.gross, euro(3_000));
    }

    /// Rebases are reported per month, so insertion order cannot change which month a change
    /// lands in. That is what keeping the schedule ordered is for.
    #[test]
    fn rebases_are_independent_of_insertion_order() {
        let forwards = Schedule::new()
            .with(Event::PayChange {
                from_month: 12,
                monthly_gross: euro(5_000),
            })
            .expect("valid")
            .with(Event::PayChange {
                from_month: 24,
                monthly_gross: euro(6_000),
            })
            .expect("valid");
        let backwards = Schedule::new()
            .with(Event::PayChange {
                from_month: 24,
                monthly_gross: euro(6_000),
            })
            .expect("valid")
            .with(Event::PayChange {
                from_month: 12,
                monthly_gross: euro(5_000),
            })
            .expect("valid");

        for month in [0_u32, 11, 12, 23, 24, 100] {
            assert_eq!(
                forwards.rebase_at(month).gross,
                backwards.rebase_at(month).gross,
                "insertion order changed the rebase at month {month}"
            );
        }
        assert_eq!(forwards.rebase_at(24).gross, Some(euro(6_000)));
    }

    #[test]
    fn the_schedule_stays_ordered_by_start_month() {
        let schedule = Schedule::new()
            .with(Event::OneOff {
                month: 30,
                amount: euro(100),
            })
            .expect("valid")
            .with(Event::OneOff {
                month: 5,
                amount: euro(200),
            })
            .expect("valid")
            .with(Event::OneOff {
                month: 18,
                amount: euro(300),
            })
            .expect("valid");

        let mut months = [0_u32; 3];
        for (slot, event) in months.iter_mut().zip(schedule.events()) {
            *slot = event.start_month();
        }
        assert_eq!(months, [5, 18, 30]);
    }

    /// Several one-offs in the same month accumulate rather than the last one winning.
    #[test]
    fn concurrent_one_offs_accumulate() {
        let schedule = Schedule::new()
            .with(Event::OneOff {
                month: 10,
                amount: euro(-5_000),
            })
            .expect("valid")
            .with(Event::OneOff {
                month: 10,
                amount: euro(2_000),
            })
            .expect("valid");
        assert_eq!(resolve(&schedule, 10).one_off, euro(-3_000));
    }

    #[test]
    fn a_full_schedule_is_refused_rather_than_dropping_events() {
        let mut schedule = Schedule::new();
        for index in 0..Schedule::MAX_EVENTS {
            schedule = schedule
                .with(Event::OneOff {
                    month: u32::try_from(index).unwrap(),
                    amount: euro(1),
                })
                .expect("within the bound");
        }
        assert_eq!(schedule.len(), Schedule::MAX_EVENTS);
        assert!(matches!(
            schedule.with(Event::OneOff {
                month: 99,
                amount: euro(1)
            }),
            Err(MoneyError::OutOfDomain { .. })
        ));
    }
}
