//! Rendering a property purchase.
//!
//! Arranged around the crate's own boundary: what is statutory, what is exact arithmetic on
//! stated terms, and what is neither. A household should be able to see which of its numbers
//! are facts.

use core::fmt::Write as _;

use casivell_core::MoneyError;
use casivell_lawdata::Bundesland;
use casivell_property::{Amortisation, PurchaseCosts};

use crate::format::{euro, percent};

/// Renders the purchase and its loan.
///
/// # Errors
///
/// [`MoneyError`] if a figure cannot be formatted.
pub(crate) fn render(
    costs: &PurchaseCosts,
    loan: &Amortisation,
    land: Bundesland,
    fixed_years: u32,
    year: u16,
) -> Result<String, MoneyError> {
    let mut out = String::with_capacity(3_072);
    let _ = writeln!(out, "\nCasivell — Immobilienkauf {year}");
    let _ = writeln!(out, "  {} € in {land:?}\n", euro(costs.price)?);

    write_costs(&mut out, costs)?;
    write_loan(&mut out, costs, loan, fixed_years)?;
    write_notes(&mut out);
    Ok(out)
}

/// The Kaufnebenkosten, with the statutory line marked as such.
fn write_costs(out: &mut String, costs: &PurchaseCosts) -> Result<(), MoneyError> {
    let _ = writeln!(out, "  Kaufnebenkosten");
    let _ = writeln!(
        out,
        "    {:<34} {:>12} €  statutory",
        format!("Grunderwerbsteuer ({})", percent(costs.transfer_tax_rate)?),
        euro(costs.transfer_tax)?
    );
    let _ = writeln!(
        out,
        "    {:<34} {:>12} €  estimated",
        "Notar und Grundbuch",
        euro(costs.notary_and_registry)?
    );
    if !costs.agent_commission.is_zero() {
        let _ = writeln!(
            out,
            "    {:<34} {:>12} €  contractual",
            "Maklerprovision",
            euro(costs.agent_commission)?
        );
    }
    let _ = writeln!(
        out,
        "    {:<34} {:>12} €",
        format!("= Nebenkosten ({})", percent(costs.incidental_rate()?)?),
        euro(costs.incidental_total)?
    );
    let _ = writeln!(
        out,
        "    {:<34} {:>12} €\n",
        "= Gesamtaufwand",
        euro(costs.total)?
    );
    Ok(())
}

/// The loan, and the residual the fixed period leaves behind.
fn write_loan(
    out: &mut String,
    costs: &PurchaseCosts,
    loan: &Amortisation,
    fixed_years: u32,
) -> Result<(), MoneyError> {
    let (years, months) = loan.term();
    let _ = writeln!(out, "  Finanzierung");
    let _ = writeln!(
        out,
        "    {:<34} {:>12} €  {} of the price",
        "Eigenkapital",
        euro(costs.deposit)?,
        percent(costs.deposit_against_price()?)?
    );
    let _ = writeln!(
        out,
        "    {:<34} {:>12} €",
        "Darlehen",
        euro(costs.loan_required)?
    );
    let _ = writeln!(
        out,
        "    {:<34} {:>12} €",
        "Monatliche Rate",
        euro(loan.monthly_payment)?
    );
    let _ = writeln!(out, "    {:<34} {years:>9} J {months:>2} M", "Laufzeit");
    let _ = writeln!(
        out,
        "    {:<34} {:>12} €",
        "Zinsen insgesamt",
        euro(loan.total_interest)?
    );
    let _ = writeln!(out, "\n    After the {fixed_years}-year Zinsbindung:");
    let _ = writeln!(
        out,
        "    {:<34} {:>12} €  to refinance",
        "  Restschuld",
        euro(loan.balance_at_fix_end)?
    );
    let _ = writeln!(
        out,
        "    {:<34} {:>12} €\n",
        "  Zinsen bis dahin",
        euro(loan.interest_during_fix)?
    );
    Ok(())
}

/// The caveats, which here are the point rather than a footnote.
fn write_notes(out: &mut String) {
    let _ = writeln!(out, "  Notes");
    let _ = writeln!(
        out,
        "  · Grunderwerbsteuer is the state's own rate and is exact. Notary and land"
    );
    let _ = writeln!(
        out,
        "    registry approximate the GNotKG fee schedule, which is not implemented."
    );
    let _ = writeln!(
        out,
        "  · The Nebenkosten buy nothing a bank lends against, so they come out of the"
    );
    let _ = writeln!(
        out,
        "    deposit: a nominal 20 % down is a good deal less once they are paid."
    );
    let _ = writeln!(
        out,
        "  · The Zinsbindung is not the term. The Restschuld above is refinanced at"
    );
    let _ = writeln!(
        out,
        "    whatever rates then are, which nobody can tell you."
    );
    let _ = writeln!(
        out,
        "  · Not modelled: maintenance, Hausgeld, the cost of selling, or what the"
    );
    let _ = writeln!(
        out,
        "    property is worth later. A buy-versus-rent answer turns on those, and"
    );
    let _ = writeln!(out, "    they are assumptions rather than arithmetic.");
    let _ = writeln!(out, "\n  Not tax or investment advice.\n");
}
