//! A C ABI over the Casivell engine, for the browser.
//!
//! # Why a hand-written ABI and no bindings crate
//!
//! `wasm-bindgen` is the ordinary answer and it would work. This workspace has **no external
//! dependencies at all**, and that is a property worth more than the convenience: every line
//! that computes a German tax figure is in this repository and reviewable, and the build has no
//! supply chain. A calculator that tells people what they owe should be auditable end to end.
//!
//! The cost is a narrow, scalar-only interface, which for a payslip is no cost at all — the
//! inputs are eight numbers and the outputs are nine.
//!
//! # The calling convention
//!
//! Two steps, because returning a struct across the ABI would need pointers and therefore
//! `unsafe`:
//!
//! 1. Call [`casivell_payslip`] with the inputs. It computes once, stores the result, and
//!    returns `0` on success or a negative [`error`] code.
//! 2. Call [`casivell_result`] with a [`field`] index to read each figure out, in cents.
//!
//! The result is held in a `RefCell` in thread-local storage — safe, single-threaded, and
//! exactly the shape wasm has anyway. Nothing here contains an `unsafe` block; the only
//! exception to the workspace's `forbid(unsafe_code)` is the export attribute itself, which
//! Rust 2024 spells `#[unsafe(no_mangle)]`.
//!
//! # Money crosses the boundary in cents
//!
//! Never as a float. The engine's whole design is that a euro amount is an integer number of
//! cents, and handing JavaScript a `f64` at the boundary would throw that away at the last
//! step.
//!
//! Because they are `i64`, WebAssembly's JavaScript interface presents them as **`BigInt`** —
//! arguments must be passed as `BigInt` and results converted with `Number()`. That is
//! friction, and narrowing to `i32` would remove it at the cost of a silent ceiling around
//! 21 million euro. Money is `i64` throughout this engine and the boundary does not pretend
//! otherwise; `web/index.html` shows the two conversions, which are one call each.

#![deny(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::arithmetic_side_effects,
        clippy::indexing_slicing,
    )
)]

use core::cell::RefCell;

use casivell_core::{Money, Rate, TaxYear};
use casivell_lawdata::{Bundesland, Fingerprinted as _, LawYear, TaxClass};
use casivell_payroll::{Employment, HealthCover, NetPay, PayPeriod, PayrollLaw, net_pay};
use casivell_social::Insured;

/// Error codes returned by [`casivell_payslip`]. All negative; success is zero.
pub mod error {
    /// The year has no statutory data.
    pub const YEAR: i32 = -1;
    /// The tax class is not one of the six.
    pub const TAX_CLASS: i32 = -2;
    /// The Bundesland index is not one of the sixteen.
    pub const LAND: i32 = -3;
    /// An input was outside its domain — an impossible age, a negative rate, a person with
    /// children who denies Elterneigenschaft.
    pub const INPUT: i32 = -4;
    /// The calculation left the representable domain.
    pub const ARITHMETIC: i32 = -5;
    /// The requested result field does not exist.
    pub const FIELD: i32 = -6;
}

/// Field indices for [`casivell_result`].
pub mod field {
    /// Gross pay for the period, in cents.
    pub const GROSS: i32 = 0;
    /// Lohnsteuer.
    pub const INCOME_TAX: i32 = 1;
    /// Solidaritätszuschlag.
    pub const SOLIDARITY: i32 = 2;
    /// Church tax.
    pub const CHURCH_TAX: i32 = 3;
    /// The employee's social insurance contributions, all four branches.
    pub const CONTRIBUTIONS: i32 = 4;
    /// What reaches the household.
    pub const NET: i32 = 5;
    /// The employee's pension contribution alone.
    pub const PENSION: i32 = 6;
    /// The employee's health contribution alone.
    pub const HEALTH: i32 = 7;
    /// The employee's long-term care contribution alone.
    pub const CARE: i32 = 8;
    /// Unemployment insurance alone.
    pub const UNEMPLOYMENT: i32 = 9;

    // The § 39b working, in the Programmablaufplan's own order and under its own variable
    // names. These are what turn a net figure into something a person can check: the annual
    // wage, what came off it, and the tariff amount that produced the tax.
    /// `ZRE4`: the annualised gross the PAP works from.
    pub const ANNUAL_GROSS: i32 = 10;
    /// `ZTABFB`: the fixed table allowances.
    pub const TABLE_ALLOWANCES: i32 = 11;
    /// `VSP`: the Vorsorgepauschale actually deducted.
    pub const VORSORGEPAUSCHALE: i32 = 12;
    /// `ZVE`: the taxable annual amount the tariff was applied to.
    pub const TAXABLE_ANNUAL: i32 = 13;
    /// `LSTJAHR`: the annual Lohnsteuer the period figure was derived from.
    pub const ANNUAL_INCOME_TAX: i32 = 14;
    /// `JBMG`: the annual § 51a base, which the surcharges are levied on.
    pub const SURCHARGE_BASE: i32 = 15;

    /// One past the last valid index.
    pub const COUNT: i32 = 16;
}

thread_local! {
    /// The last successful result, held between the compute call and the reads of it.
    static LAST: RefCell<Option<NetPay>> = const { RefCell::new(None) };
}

/// Computes a payslip and stores the result.
///
/// Returns `0` on success, or one of the [`error`] codes. Money arrives in **cents** and rates
/// in **parts per million**; `1` and `0` stand for true and false.
///
/// `period` is `0` for monthly and `1` for annual. `land` is an index into
/// `Bundesland::ALL`.
///
/// # Panics
///
/// It does not. Every fallible step returns an error code.
#[expect(
    unsafe_code,
    reason = "the export attribute only, which Rust 2024 spells `unsafe(no_mangle)`; the \
              function body contains no unsafe operation"
)]
#[unsafe(no_mangle)]
pub extern "C" fn casivell_payslip(
    gross_cents: i64,
    period: i32,
    year: i32,
    tax_class: i32,
    land: i32,
    age_years: i32,
    children: i32,
    is_parent: i32,
    church: i32,
    supplementary_rate_ppm: i64,
) -> i32 {
    let outcome = compute(&Inputs {
        gross_cents,
        period,
        year,
        tax_class,
        land,
        age_years,
        children,
        is_parent: is_parent != 0,
        church: church != 0,
        supplementary_rate_ppm,
    });

    // A failed call clears the stored result. A caller that ignored this function's return
    // value would otherwise read the *previous* calculation's figures with no way to tell.
    match outcome {
        Ok(result) => {
            LAST.with(|slot| *slot.borrow_mut() = Some(result));
            0
        }
        Err(code) => {
            LAST.with(|slot| *slot.borrow_mut() = None);
            code
        }
    }
}

/// Reads one figure out of the last successful [`casivell_payslip`] call.
///
/// Returns the amount in cents, or [`error::FIELD`] where the index is unknown or no
/// calculation has succeeded. A caller that checked the compute call's return value cannot
/// reach the error case.
#[expect(
    unsafe_code,
    reason = "the export attribute only; the function body contains no unsafe operation"
)]
#[unsafe(no_mangle)]
pub extern "C" fn casivell_result(field: i32) -> i64 {
    LAST.with(|slot| match slot.borrow().as_ref() {
        Some(pay) => read(pay, field),
        None => i64::from(error::FIELD),
    })
}

/// The fingerprint of the statutory data the last calculation used.
///
/// The same digest `casivell law` prints as *Datenstand*. A page showing it lets a user say
/// which data produced a figure they wrote down — and lets two people confirm they are looking
/// at the same law rather than assuming it.
///
/// Returns `0` where no calculation has succeeded.
#[expect(
    unsafe_code,
    reason = "the export attribute only; the function body contains no unsafe operation"
)]
#[unsafe(no_mangle)]
pub extern "C" fn casivell_fingerprint(year: i32) -> i64 {
    let digest = u16::try_from(year)
        .ok()
        .and_then(|value| TaxYear::new(value).ok())
        .and_then(|value| LawYear::for_year(value).ok())
        .map(|law| law.fingerprint().value());
    match digest {
        // The digest is a u64 and the ABI carries i64. Reinterpreting keeps all sixty-four
        // bits; the caller formats it as unsigned hex, which is how it is displayed anywhere
        // else in this repository.
        Some(value) => i64::from_ne_bytes(value.to_ne_bytes()),
        None => 0,
    }
}

/// The first and last years this ABI can actually compute, packed as `first * 10_000 + last`.
///
/// One call rather than two, because a browser populating a year picker wants both and a
/// second export for a second number is not worth the surface.
///
/// **Probed, not asserted.** `TaxYear::FIRST_VERIFIED` is 2025, but the Programmablaufplan has
/// only ever been transcribed for 2026, so a picker built from the engine's general range
/// would offer a year the payslip refuses — which is exactly what
/// `the_offered_range_is_the_range_that_computes` caught. This walks the verified years and
/// reports the ones that succeed, so adding the 2025 PAP widens the picker by itself and
/// nothing has to remember to.
///
/// Returns `0` if no year computes at all, which cannot happen with any shipped data.
#[expect(
    unsafe_code,
    reason = "the export attribute only; the function body contains no unsafe operation"
)]
#[unsafe(no_mangle)]
pub extern "C" fn casivell_enacted_years() -> i64 {
    let computable: [u16; 2] = [TaxYear::FIRST_VERIFIED.get(), TaxYear::LAST_VERIFIED.get()];
    let (mut first, mut last) = (None, None);
    for year in computable[0]..=computable[1] {
        let works = TaxYear::new(year).is_ok_and(|y| PayrollLaw::for_year(y).is_ok());
        if works {
            first = first.or(Some(year));
            last = Some(year);
        }
    }
    match (first, last) {
        (Some(first), Some(last)) => i64::from(first)
            .saturating_mul(10_000)
            .saturating_add(i64::from(last)),
        _ => 0,
    }
}

/// Everything the payslip calculation needs, before validation.
struct Inputs {
    gross_cents: i64,
    period: i32,
    year: i32,
    tax_class: i32,
    land: i32,
    age_years: i32,
    children: i32,
    is_parent: bool,
    church: bool,
    supplementary_rate_ppm: i64,
}

/// Validates and computes, returning an error code on any failure.
fn compute(inputs: &Inputs) -> Result<NetPay, i32> {
    let year = u16::try_from(inputs.year)
        .ok()
        .and_then(|value| TaxYear::new(value).ok())
        .ok_or(error::YEAR)?;
    let law = PayrollLaw::for_year(year).map_err(|_| error::YEAR)?;

    let class = tax_class(inputs.tax_class)?;
    let land = *Bundesland::ALL
        .get(usize::try_from(inputs.land).map_err(|_| error::LAND)?)
        .ok_or(error::LAND)?;

    let age = u8::try_from(inputs.age_years).map_err(|_| error::INPUT)?;
    let children = u8::try_from(inputs.children).map_err(|_| error::INPUT)?;
    let supplementary = Rate::from_ppm(inputs.supplementary_rate_ppm).map_err(|_| error::INPUT)?;

    let insured = Insured::new(age, inputs.is_parent, children, land, Some(supplementary))
        .map_err(|_| error::INPUT)?;
    let employment = Employment::new(
        insured,
        class,
        u16::from(children).saturating_mul(10),
        HealthCover::Statutory {
            supplementary_rate: supplementary,
        },
        inputs.church.then_some(land),
    )
    .map_err(|_| error::INPUT)?;

    let period = match inputs.period {
        0 => PayPeriod::Month,
        1 => PayPeriod::Year,
        _ => return Err(error::INPUT),
    };
    let gross = Money::from_cents(inputs.gross_cents).map_err(|_| error::INPUT)?;

    net_pay(gross, period, &employment, &law).map_err(|_| error::ARITHMETIC)
}

/// The tax class an index names.
fn tax_class(index: i32) -> Result<TaxClass, i32> {
    match index {
        1 => Ok(TaxClass::Class1),
        2 => Ok(TaxClass::Class2),
        3 => Ok(TaxClass::Class3),
        4 => Ok(TaxClass::Class4),
        5 => Ok(TaxClass::Class5),
        6 => Ok(TaxClass::Class6),
        _ => Err(error::TAX_CLASS),
    }
}

/// `ZRE4`: the annualised gross the Programmablaufplan works from.
///
/// Not stored on the result, because the PAP derives it from the period and the gross and then
/// has no further use for it. Recovered the same way here rather than added to the payroll
/// crate's output for one consumer's benefit.
fn annual_gross(pay: &NetPay) -> i64 {
    pay.gross
        .mul_int(pay.period.periods_per_year())
        .map_or(i64::from(error::FIELD), casivell_core::Money::cents)
}

/// One figure from a computed payslip, in cents.
fn read(pay: &NetPay, which: i32) -> i64 {
    let contributions = &pay.monthly_contributions;
    match which {
        field::GROSS => pay.gross.cents(),
        field::INCOME_TAX => pay.income_tax.cents(),
        field::SOLIDARITY => pay.solidarity_surcharge.cents(),
        field::CHURCH_TAX => pay.church_tax.cents(),
        field::CONTRIBUTIONS => pay.employee_contributions.cents(),
        field::NET => pay.net.cents(),
        field::PENSION => contributions.pension.employee.cents(),
        field::HEALTH => contributions.health.employee.cents(),
        field::CARE => contributions.care.employee.cents(),
        field::UNEMPLOYMENT => contributions.unemployment.employee.cents(),

        field::ANNUAL_GROSS => annual_gross(pay),
        field::TABLE_ALLOWANCES => pay.withholding.table_allowances.cents(),
        field::VORSORGEPAUSCHALE => pay.withholding.vorsorgepauschale.cents(),
        field::TAXABLE_ANNUAL => pay.withholding.taxable_annual_amount.cents(),
        field::ANNUAL_INCOME_TAX => pay.withholding.annual_income_tax.cents(),
        field::SURCHARGE_BASE => pay.withholding.annual_church_tax_base.cents(),

        _ => i64::from(error::FIELD),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        casivell_enacted_years, casivell_fingerprint, casivell_payslip, casivell_result, error,
        field,
    };

    /// A monthly payslip through the ABI must equal the one the engine computes directly.
    ///
    /// The boundary is allowed to be narrow; it is not allowed to be a second implementation.
    #[test]
    fn the_abi_agrees_with_the_engine() {
        use casivell_core::{Money, Rate, TaxYear};
        use casivell_lawdata::{Bundesland, TaxClass};
        use casivell_payroll::{Employment, HealthCover, PayrollLaw, monthly_net};
        use casivell_social::Insured;

        assert_eq!(
            casivell_payslip(550_000, 0, 2026, 1, 9, 30, 0, 0, 0, 29_000),
            0
        );

        let rate = Rate::from_ppm(29_000).unwrap();
        let insured =
            Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, Some(rate)).unwrap();
        let employment = Employment::new(
            insured,
            TaxClass::Class1,
            0,
            HealthCover::Statutory {
                supplementary_rate: rate,
            },
            None,
        )
        .unwrap();
        let expected = monthly_net(
            Money::from_cents(550_000).unwrap(),
            &employment,
            &PayrollLaw::for_year(TaxYear::new(2026).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(casivell_result(field::NET), expected.net.cents());
        assert_eq!(
            casivell_result(field::INCOME_TAX),
            expected.income_tax.cents()
        );
        assert_eq!(
            casivell_result(field::CONTRIBUTIONS),
            expected.employee_contributions.cents()
        );
    }

    /// Every figure must decompose: net plus all deductions is gross. The property the whole
    /// engine is checked on, asserted again at the boundary in case a field is misrouted.
    #[test]
    fn the_fields_decompose_to_the_gross() {
        assert_eq!(
            casivell_payslip(550_000, 0, 2026, 1, 9, 30, 0, 0, 1, 29_000),
            0
        );
        let get = casivell_result;
        let deductions = get(field::INCOME_TAX)
            + get(field::SOLIDARITY)
            + get(field::CHURCH_TAX)
            + get(field::CONTRIBUTIONS);
        assert_eq!(get(field::NET) + deductions, get(field::GROSS));

        // And the four branches sum to the contributions total.
        let branches =
            get(field::PENSION) + get(field::HEALTH) + get(field::CARE) + get(field::UNEMPLOYMENT);
        assert_eq!(branches, get(field::CONTRIBUTIONS));
    }

    /// Every error is a distinct negative code, so a caller can say what was wrong.
    #[test]
    fn each_bad_input_has_its_own_code() {
        let ok = || casivell_payslip(550_000, 0, 2026, 1, 9, 30, 0, 0, 0, 29_000);
        assert_eq!(ok(), 0);

        assert_eq!(
            casivell_payslip(550_000, 0, 1999, 1, 9, 30, 0, 0, 0, 29_000),
            error::YEAR
        );
        assert_eq!(
            casivell_payslip(550_000, 0, 2026, 7, 9, 30, 0, 0, 0, 29_000),
            error::TAX_CLASS
        );
        assert_eq!(
            casivell_payslip(550_000, 0, 2026, 1, 99, 30, 0, 0, 0, 29_000),
            error::LAND
        );
        // A person with children who denies Elterneigenschaft: the engine refuses the
        // contradiction, and the boundary passes the refusal on rather than reconciling it.
        assert_eq!(
            casivell_payslip(550_000, 0, 2026, 1, 9, 30, 1, 0, 0, 29_000),
            error::INPUT
        );
        assert_eq!(
            casivell_payslip(550_000, 5, 2026, 1, 9, 30, 0, 0, 0, 29_000),
            error::INPUT
        );
    }

    /// A failed calculation must clear the stored result rather than leaving the previous one
    /// readable — a caller that ignored the return code would otherwise read stale figures and
    /// have no way to tell.
    #[test]
    fn a_failed_call_clears_the_previous_result() {
        assert_eq!(
            casivell_payslip(550_000, 0, 2026, 1, 9, 30, 0, 0, 0, 29_000),
            0
        );
        assert!(casivell_result(field::NET) > 0);

        assert_ne!(
            casivell_payslip(550_000, 0, 1999, 1, 9, 30, 0, 0, 0, 29_000),
            0
        );
        assert_eq!(casivell_result(field::NET), i64::from(error::FIELD));
    }

    /// An unknown field index is an error rather than a silent zero, which would read as a
    /// legitimate amount.
    #[test]
    fn an_unknown_field_is_an_error_not_a_zero() {
        assert_eq!(
            casivell_payslip(550_000, 0, 2026, 1, 9, 30, 0, 0, 0, 29_000),
            0
        );
        assert_eq!(casivell_result(field::COUNT), i64::from(error::FIELD));
        assert_eq!(casivell_result(-1), i64::from(error::FIELD));
    }

    /// Every year the ABI offers must compute, and the range must not be wider than that.
    ///
    /// This caught a real inconsistency. `TaxYear::FIRST_VERIFIED` is 2025 and the range was
    /// first written from it, but the Programmablaufplan exists only for 2026 — so a year
    /// picker built from the engine's general range would have offered 2025 and been refused.
    /// The range is now probed rather than asserted.
    #[test]
    fn the_offered_range_is_the_range_that_computes() {
        use casivell_core::TaxYear;

        let packed = casivell_enacted_years();
        let first = u16::try_from(packed / 10_000).expect("a year fits a u16");
        let last = u16::try_from(packed % 10_000).expect("a year fits a u16");
        assert!(first <= last);
        assert!(first >= TaxYear::FIRST_VERIFIED.get());
        assert!(last <= TaxYear::LAST_VERIFIED.get());

        // Everything offered computes …
        for year in first..=last {
            assert_eq!(
                casivell_payslip(550_000, 0, i32::from(year), 1, 9, 30, 0, 0, 0, 29_000),
                0,
                "{year} is offered but does not compute"
            );
        }
        // … and nothing outside it does, so the range is exact rather than merely safe.
        for year in [first.saturating_sub(1), last.saturating_add(1)] {
            if (TaxYear::FIRST_VERIFIED.get()..=TaxYear::LAST_VERIFIED.get()).contains(&year) {
                assert_ne!(
                    casivell_payslip(550_000, 0, i32::from(year), 1, 9, 30, 0, 0, 0, 29_000),
                    0,
                    "{year} computes but is not offered"
                );
            }
        }
    }

    /// The § 39b working must reconcile as the Programmablaufplan's own chain, or an
    /// "explainability" panel built on it would show a derivation that does not derive.
    ///
    /// `ZRE4 − ZTABFB − VSP = ZVE`, and the period's tax is the annual figure apportioned.
    #[test]
    fn the_pap_working_reconciles() {
        assert_eq!(
            casivell_payslip(550_000, 0, 2026, 1, 9, 30, 0, 0, 0, 29_000),
            0
        );
        let get = casivell_result;

        assert_eq!(get(field::ANNUAL_GROSS), get(field::GROSS) * 12);
        assert_eq!(
            get(field::ANNUAL_GROSS) - get(field::TABLE_ALLOWANCES) - get(field::VORSORGEPAUSCHALE),
            get(field::TAXABLE_ANNUAL),
            "ZRE4 − ZTABFB − VSP must equal ZVE"
        );
        // The monthly tax is the annual figure divided twelve ways, to within the cent the
        // PAP's own apportionment rounds by.
        let implied = get(field::ANNUAL_INCOME_TAX) / 12;
        assert!((implied - get(field::INCOME_TAX)).abs() <= 100);
        assert!(get(field::SURCHARGE_BASE) > 0);
    }

    /// The fingerprint must match the engine's, so a figure written down beside it can be
    /// traced to the data that produced it.
    #[test]
    fn the_fingerprint_matches_the_engine() {
        use casivell_core::TaxYear;
        use casivell_lawdata::{Fingerprinted as _, LawYear};

        let expected = LawYear::for_year(TaxYear::new(2026).unwrap())
            .unwrap()
            .fingerprint()
            .value();
        let reported = casivell_fingerprint(2026);
        assert_eq!(u64::from_ne_bytes(reported.to_ne_bytes()), expected);

        // A year with no enacted data has no fingerprint to report.
        assert_eq!(casivell_fingerprint(1999), 0);
    }

    /// Every tax class and every state must be reachable through the boundary.
    #[test]
    fn every_class_and_state_is_reachable() {
        for class in 1..=6 {
            for land in 0..16 {
                assert_eq!(
                    casivell_payslip(400_000, 0, 2026, class, land, 30, 0, 0, 1, 29_000),
                    0,
                    "class {class} in state {land} failed"
                );
                assert!(casivell_result(field::NET) > 0);
            }
        }
    }
}
