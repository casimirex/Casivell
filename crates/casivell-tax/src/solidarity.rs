//! Solidaritätszuschlag, SolzG 1995.
//!
//! # The formula
//!
//! Let `T` be the assessed income tax and `F` the Freigrenze. Then
//!
//! ```text
//!   soli = 0                                    if T ≤ F
//!   soli = min( 5.5 % · T ,  11.9 % · (T − F) ) if T > F
//! ```
//!
//! The `min` is § 4 Satz 2 SolzG. Its effect is a phase-in band — the
//! Milderungszone — whose upper end is wherever the two branches cross:
//! `0.055·T = 0.119·(T − F)` gives `T = F · 0.119 / 0.064 ≈ 1.859·F`. For 2026
//! that is roughly 37 800 € of income tax, or something over 110 000 € of taxable
//! income for a single person. Between the Freigrenze and that point the
//! *effective* marginal rate on the surcharge is 11.9 %, more than double the
//! headline figure — which is precisely the sort of thing a household planner
//! exists to reveal, and precisely what a flat 5.5 % model would hide.

use casivell_core::{Money, MoneyError, Rounding};
use casivell_lawdata::SolidarityParameters;

use crate::tariff::FilingStatus;

/// The outcome of a surcharge calculation, with the reasoning attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurchargeResult {
    /// The surcharge payable.
    pub amount: Money,
    /// Whether the taxpayer sits inside the Milderungszone, where the marginal
    /// rate is the taper rate rather than the headline rate.
    ///
    /// Surfaced because it changes the answer to "what happens if I earn a little
    /// more", which is the question users actually ask.
    pub in_taper_zone: bool,
    /// The Freigrenze that applied, which depends on the filing status.
    pub exemption_applied: Money,
}

/// Computes the Solidaritätszuschlag on an assessed income tax amount.
///
/// `income_tax` must be the assessed *Einkommensteuer*, not taxable income. The
/// Freigrenze is a threshold on tax, not on income, and passing income here would
/// exempt almost everybody.
///
/// # Rounding
///
/// The surcharge is computed to the cent, truncating.
///
/// SolzG does not itself prescribe a direction, so this was previously recorded as
/// an open item. It is now settled for the withholding path: the `MSOLZ` routine of
/// the BMF Programmablaufplan 2026 annotates `SOLZJ = JBMG · 5,5/100` with `Cent↓`,
/// i.e. truncation, and `casivell-payroll` follows it. Truncation here keeps the
/// annual assessment consistent with withholding.
///
/// One residual caveat: the PAP governs *withholding*, and its rounding is
/// authoritative for that. Whether the annual assessment truncates identically has
/// not been confirmed against a Steuerbescheid. The exposure is one cent.
///
/// # Errors
///
/// [`MoneyError::Overflow`] if an intermediate leaves the representable domain.
pub fn solidarity_surcharge(
    income_tax: Money,
    params: &SolidarityParameters,
    filing: FilingStatus,
) -> Result<SurchargeResult, MoneyError> {
    let exemption = match filing {
        FilingStatus::Individual => params.exemption_individual,
        FilingStatus::JointSplitting => params.exemption_joint,
    };

    // § 3 Abs. 3 SolzG: no surcharge at or below the Freigrenze. Note `<=`: the
    // statute exempts tax that does not *exceed* the threshold.
    if income_tax <= exemption {
        return Ok(SurchargeResult {
            amount: Money::ZERO,
            in_taper_zone: false,
            exemption_applied: exemption,
        });
    }

    let headline = income_tax.mul_rate(params.rate, Rounding::Floor)?;
    let excess = income_tax.sub(exemption)?;
    let tapered = excess.mul_rate(params.taper_rate, Rounding::Floor)?;

    // § 4 Satz 2: the surcharge may not exceed the tapered cap.
    let amount = headline.min(tapered);
    Ok(SurchargeResult {
        amount,
        in_taper_zone: tapered < headline,
        exemption_applied: exemption,
    })
}

#[cfg(test)]
mod tests {
    use super::{SurchargeResult, solidarity_surcharge};
    use crate::tariff::FilingStatus;
    use casivell_core::{Money, TaxYear};
    use casivell_lawdata::SolidarityParameters;

    fn params(year: u16) -> SolidarityParameters {
        SolidarityParameters::for_year(TaxYear::new(year).unwrap()).unwrap()
    }

    fn soli(tax_euro: i64, year: u16, filing: FilingStatus) -> SurchargeResult {
        let tax = Money::from_euro(tax_euro).unwrap();
        solidarity_surcharge(tax, &params(year), filing).unwrap()
    }

    #[test]
    fn no_surcharge_at_or_below_the_freigrenze() {
        // 2026: 20 350 € for an individual.
        assert_eq!(soli(0, 2026, FilingStatus::Individual).amount, Money::ZERO);
        assert_eq!(
            soli(20_349, 2026, FilingStatus::Individual).amount,
            Money::ZERO
        );
        // Exactly at the threshold: exempt, because the statute says "übersteigt".
        assert_eq!(
            soli(20_350, 2026, FilingStatus::Individual).amount,
            Money::ZERO
        );
    }

    #[test]
    fn the_surcharge_begins_one_euro_above_the_freigrenze() {
        let result = soli(20_351, 2026, FilingStatus::Individual);
        // 11.9 % of one euro of excess, which is 11 cents after truncation, and
        // far below 5.5 % of 20 351 €. The taper binds.
        assert_eq!(result.amount.cents(), 11);
        assert!(result.in_taper_zone);
    }

    /// The joint Freigrenze is double, so a couple with the same total tax pays no
    /// surcharge where a single person would.
    #[test]
    fn the_joint_freigrenze_is_double() {
        let individual = soli(30_000, 2026, FilingStatus::Individual);
        let joint = soli(30_000, 2026, FilingStatus::JointSplitting);
        assert!(individual.amount > Money::ZERO);
        assert_eq!(joint.amount, Money::ZERO);
        assert_eq!(joint.exemption_applied.cents(), 4_070_000);
    }

    /// Well above the zone the headline 5.5 % applies exactly.
    #[test]
    fn far_above_the_zone_the_headline_rate_applies() {
        let result = soli(100_000, 2026, FilingStatus::Individual);
        assert_eq!(result.amount.cents(), 550_000);
        assert!(!result.in_taper_zone);
    }

    /// The Milderungszone closes where `0.055·T = 0.119·(T − F)`, i.e. at
    /// `T = F · 119 / 64`. For 2026 that is `20 350 · 119 / 64 ≈ 37 838 €`. Below it
    /// the taper binds, above it the headline rate does.
    #[test]
    fn the_taper_zone_closes_at_the_computed_crossover() {
        let p = params(2026);
        let f = p.exemption_individual.whole_euro_floor().unwrap();
        let crossover = f.saturating_mul(119) / 64;
        assert!(
            soli(
                crossover.saturating_sub(500),
                2026,
                FilingStatus::Individual
            )
            .in_taper_zone,
            "the taper should still bind below the crossover"
        );
        assert!(
            !soli(
                crossover.saturating_add(500),
                2026,
                FilingStatus::Individual
            )
            .in_taper_zone,
            "the headline rate should apply above the crossover"
        );
    }

    /// Inside the zone the effective marginal rate on the surcharge is the taper
    /// rate, not the headline rate. This is the behaviour a flat-5.5 % model would
    /// get wrong, so it is asserted directly.
    #[test]
    fn inside_the_zone_the_marginal_rate_is_the_taper_rate() {
        let low = soli(25_000, 2026, FilingStatus::Individual);
        let high = soli(26_000, 2026, FilingStatus::Individual);
        assert!(low.in_taper_zone && high.in_taper_zone);
        let extra = high.amount.sub(low.amount).unwrap().cents();
        // 11.9 % of 1 000 € is 119,00 €.
        assert_eq!(extra, 11_900);
    }

    /// The surcharge must never exceed the headline rate, and never be negative,
    /// for any tax amount at all. A property test over the whole plausible range.
    #[test]
    fn the_surcharge_stays_within_its_bounds_everywhere() {
        for year in [2025_u16, 2026] {
            for filing in [FilingStatus::Individual, FilingStatus::JointSplitting] {
                let mut tax = 0_i64;
                while tax <= 400_000 {
                    let result = soli(tax, year, filing);
                    assert!(
                        !result.amount.is_negative(),
                        "{year}: negative surcharge at {tax} €"
                    );
                    // Never more than 5.5 % of the tax.
                    assert!(
                        result.amount.cents().saturating_mul(1_000) <= tax.saturating_mul(55_000),
                        "{year}: surcharge at {tax} € exceeded 5.5 %"
                    );
                    tax = tax.saturating_add(313);
                }
            }
        }
    }

    /// The surcharge must be monotonic in the tax it is levied on.
    #[test]
    fn the_surcharge_is_monotonic_in_the_tax() {
        let mut previous = Money::ZERO;
        let mut tax = 0_i64;
        while tax <= 200_000 {
            let current = soli(tax, 2026, FilingStatus::Individual).amount;
            assert!(
                current >= previous,
                "surcharge fell from {} to {} cents at {tax} €",
                previous.cents(),
                current.cents()
            );
            previous = current;
            tax = tax.saturating_add(97);
        }
    }

    /// 2026 raised the Freigrenze, so the same tax attracts no more surcharge than
    /// in 2025 and strictly less somewhere in the band between the two thresholds.
    #[test]
    fn the_2026_threshold_increase_never_raises_the_surcharge() {
        let mut tax = 0_i64;
        let mut found_strict_relief = false;
        while tax <= 60_000 {
            let old = soli(tax, 2025, FilingStatus::Individual).amount;
            let new = soli(tax, 2026, FilingStatus::Individual).amount;
            assert!(new <= old, "2026 charged more than 2025 at {tax} € of tax");
            if new < old {
                found_strict_relief = true;
            }
            tax = tax.saturating_add(53);
        }
        assert!(
            found_strict_relief,
            "the higher 2026 Freigrenze produced no relief anywhere, so it is not being applied"
        );
    }
}
