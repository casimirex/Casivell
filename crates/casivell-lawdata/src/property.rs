//! Grunderwerbsteuer, and the other costs of buying a home.
//!
//! # The one tax the states set themselves
//!
//! Since the Föderalismusreform of 1 September 2006, § 11 Abs. 1 GrEStG's 3,5 % is only a
//! default: each Land may set its own rate, and fourteen of the sixteen have. The spread is
//! large enough to dominate a purchase decision at the margin — on a 400 000 € house the
//! difference between Bayern and Nordrhein-Westfalen is **12 000 €**, which is more than most
//! households' annual savings.
//!
//! Each rate here comes from that Land's own Grunderwerbsteuergesetz, and the table was
//! cross-checked against two independent published tables that agreed on all sixteen. That
//! matters more than usual: the rates change one state at a time and stale figures circulate
//! for years afterwards. Sachsen has been 5,5 % since 2023 and is still widely quoted at
//! 3,5 %; Thüringen came *down* to 5,0 % in 2024, the only reduction any state has made.
//!
//! # Where the statutory part stops
//:
//! The Grunderwerbsteuer is a rate on a price and is exact. The other purchase costs are not:
//!
//! - **Notary and land registry** follow the `GNotKG`'s fee tables, which price each act from
//!   the transaction value and then apply act-specific multipliers. That schedule is not
//!   implemented here, so [`PropertyCostParameters::notary_and_registry_rate`] carries the
//!   customary approximation instead — and is documented as an approximation rather than
//!   passed off as law.
//! - **Maklerprovision** is contractual, not statutory. § 656c BGB caps a private buyer's
//!   share at the seller's since December 2020, but the rate itself is negotiated, so it is an
//!   input with no default.
//!
//! Keeping the exact and the approximate apart is the point: a household should be able to see
//! which part of its 40 000 € of Kaufnebenkosten is a legal certainty and which is an estimate.

use casivell_core::{MoneyError, Rate, TaxYear};

use crate::provenance::{DataStatus, Provenance};
use crate::surcharges::Bundesland;

/// Parameters for the costs of acquiring property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyCostParameters {
    /// The year these parameters apply to.
    pub year: TaxYear,

    /// § 11 GrEStG and the Länder's own Acts: the transfer tax rate in each state.
    ///
    /// Indexed by [`Bundesland::ALL`] order, so the array and the enum cannot drift apart
    /// without a test noticing.
    pub transfer_tax_rates: [Rate; 16],

    /// Notary and land-registry costs as a share of the price.
    ///
    /// **An approximation, not a statutory rate.** The `GNotKG` prices each act from the
    /// transaction value through its own fee tables — a 2,0 multiplier for the Beurkundung,
    /// 0,5 for the Vollzug, 1,0 each for the Eigentumsumschreibung and any Grundschuld — and
    /// this crate does not implement that schedule. Two percent is the figure the market
    /// quotes and it is close for ordinary house prices, but it is an estimate and every
    /// report that shows it says so.
    pub notary_and_registry_rate: Rate,

    /// Citation.
    pub provenance: Provenance,
}

impl PropertyCostParameters {
    /// Returns the parameters for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if no verified set exists.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        match year.get() {
            2025 | 2026 => Ok(PROPERTY_COSTS),
            other => Err(MoneyError::YearOutOfRange { year: other }),
        }
    }

    /// The Grunderwerbsteuer rate in `land`.
    ///
    /// The index is in range by construction: [`Bundesland::index`] is an exhaustive match
    /// returning `0..=15` and the array is sixteen long, so `get` cannot fail. Written with
    /// `get` anyway rather than a bare index, so the impossible case is handled in the code
    /// rather than argued about in a comment — and the fallback is the § 11 GrEStG default,
    /// which is the only defensible answer if a state were ever added without a rate.
    #[must_use]
    pub fn transfer_tax_rate(&self, land: Bundesland) -> Rate {
        self.transfer_tax_rates
            .get(land.index())
            .copied()
            .unwrap_or(STATUTORY_DEFAULT_RATE)
    }

    /// The lowest and highest rates in force, for a report that wants to show the spread.
    #[must_use]
    pub fn transfer_tax_range(&self) -> (Rate, Rate) {
        let lowest = self
            .transfer_tax_rates
            .iter()
            .min_by_key(|rate| rate.ppm())
            .copied()
            .unwrap_or(STATUTORY_DEFAULT_RATE);
        let highest = self
            .transfer_tax_rates
            .iter()
            .max_by_key(|rate| rate.ppm())
            .copied()
            .unwrap_or(STATUTORY_DEFAULT_RATE);
        (lowest, highest)
    }
}

/// § 11 Abs. 1 GrEStG's own rate, which applies where a Land has set none.
///
/// Only Bayern still charges it, but it remains the statutory default and so is the right
/// answer for a state whose rate is somehow absent.
const STATUTORY_DEFAULT_RATE: Rate = pct_milli(3_500);

const fn pct_milli(percent_millis: i64) -> Rate {
    match Rate::from_percent_millis(percent_millis) {
        Ok(r) => r,
        Err(_) => Rate::ZERO,
    }
}

/// Grunderwerbsteuer and purchase costs.
///
/// The rates are in [`Bundesland::ALL`] order. Each is from that Land's own Act; the dates in
/// the comments are when the current rate took effect, which is what makes a stale figure easy
/// to spot.
const PROPERTY_COSTS: PropertyCostParameters = PropertyCostParameters {
    year: match TaxYear::new(2026) {
        Ok(y) => y,
        Err(_) => TaxYear::LAST_VERIFIED,
    },

    transfer_tax_rates: [
        pct_milli(5_000), // Baden-Württemberg, since 05.11.2011
        pct_milli(3_500), // Bayern, since 01.01.1997 — the § 11 GrEStG default, never raised
        pct_milli(6_000), // Berlin, since 01.01.2014
        pct_milli(6_500), // Brandenburg, since 01.07.2015
        pct_milli(5_500), // Bremen, since 01.07.2025
        pct_milli(5_500), // Hamburg, since 01.01.2023
        pct_milli(6_000), // Hessen, since 01.08.2014
        pct_milli(6_000), // Mecklenburg-Vorpommern, since 01.07.2019
        pct_milli(5_000), // Niedersachsen, since 01.01.2014
        pct_milli(6_500), // Nordrhein-Westfalen, since 01.01.2015
        pct_milli(5_000), // Rheinland-Pfalz, since 01.03.2012
        pct_milli(6_500), // Saarland, since 01.01.2015
        pct_milli(5_500), // Sachsen, since 01.01.2023 — widely still quoted at 3,5 %
        pct_milli(5_000), // Sachsen-Anhalt, since 01.03.2012
        pct_milli(6_500), // Schleswig-Holstein, since 01.01.2014
        pct_milli(5_000), // Thüringen, since 01.01.2024 — the only reduction so far
    ],

    notary_and_registry_rate: pct_milli(2_000),

    provenance: Provenance::new(
        "§ 11 GrEStG and the Grunderwerbsteuergesetze der Länder; GNotKG (approximated)",
        "https://www.gesetze-im-internet.de/grestg_1983/__11.html",
        "2026-07-31",
        DataStatus::Enacted,
    ),
};

#[cfg(test)]
mod tests {
    use super::{PROPERTY_COSTS, PropertyCostParameters};
    use crate::surcharges::Bundesland;
    use casivell_core::{Money, Rate, Rounding, TaxYear};

    fn pct(value: i64) -> Rate {
        Rate::from_percent_millis(value).expect("valid")
    }

    /// Every state's rate, against the two published tables they were checked against.
    ///
    /// Sixteen figures that change one at a time and are widely misquoted when they do. Listed
    /// individually rather than spot-checked, because a wrong rate here is a four-figure error
    /// in a household's largest single transaction.
    #[test]
    fn every_states_rate_matches_the_published_tables() {
        let p = PROPERTY_COSTS;
        let expected = [
            (Bundesland::BadenWuerttemberg, 5_000),
            (Bundesland::Bayern, 3_500),
            (Bundesland::Berlin, 6_000),
            (Bundesland::Brandenburg, 6_500),
            (Bundesland::Bremen, 5_500),
            (Bundesland::Hamburg, 5_500),
            (Bundesland::Hessen, 6_000),
            (Bundesland::MecklenburgVorpommern, 6_000),
            (Bundesland::Niedersachsen, 5_000),
            (Bundesland::NordrheinWestfalen, 6_500),
            (Bundesland::RheinlandPfalz, 5_000),
            (Bundesland::Saarland, 6_500),
            (Bundesland::Sachsen, 5_500),
            (Bundesland::SachsenAnhalt, 5_000),
            (Bundesland::SchleswigHolstein, 6_500),
            (Bundesland::Thueringen, 5_000),
        ];
        for (land, millis) in expected {
            assert_eq!(
                p.transfer_tax_rate(land),
                pct(millis),
                "{land:?} has the wrong Grunderwerbsteuer rate"
            );
        }
    }

    /// The array must be in `Bundesland::ALL` order, or every lookup is silently off by one.
    ///
    /// Checked by looking each state up through the enum and comparing against the array
    /// position, which is the one thing a hand-ordered table gets wrong.
    #[test]
    fn the_table_is_in_enum_order() {
        for (index, land) in Bundesland::ALL.into_iter().enumerate() {
            assert_eq!(
                PROPERTY_COSTS.transfer_tax_rate(land),
                PROPERTY_COSTS.transfer_tax_rates[index],
                "{land:?} does not sit at index {index}"
            );
        }
    }

    /// Bayern alone still charges the § 11 GrEStG default; nobody exceeds 6,5 %.
    #[test]
    fn the_spread_runs_from_the_statutory_default_to_six_and_a_half() {
        let (lowest, highest) = PROPERTY_COSTS.transfer_tax_range();
        assert_eq!(lowest, pct(3_500), "the § 11 default, still Bayern's rate");
        assert_eq!(highest, pct(6_500));
        assert_eq!(
            PROPERTY_COSTS.transfer_tax_rate(Bundesland::Bayern),
            lowest,
            "Bayern is the only state never to have raised it"
        );
    }

    /// The figure that makes the table worth having: on a 400 000 € house the state costs
    /// 12 000 € more or less. That is larger than most households save in a year, and it is
    /// decided entirely by which side of a Land border the house is on.
    #[test]
    fn the_spread_is_material_on_a_real_house_price() {
        let price = Money::from_euro(400_000).expect("valid");
        let cheapest = price
            .mul_rate(
                PROPERTY_COSTS.transfer_tax_rate(Bundesland::Bayern),
                Rounding::HalfUp,
            )
            .expect("in domain");
        let dearest = price
            .mul_rate(
                PROPERTY_COSTS.transfer_tax_rate(Bundesland::NordrheinWestfalen),
                Rounding::HalfUp,
            )
            .expect("in domain");

        assert_eq!(cheapest, Money::from_euro(14_000).expect("valid"));
        assert_eq!(dearest, Money::from_euro(26_000).expect("valid"));
        assert_eq!(
            dearest.sub(cheapest).expect("in domain"),
            Money::from_euro(12_000).expect("valid")
        );
    }

    /// No rate may be zero or absurd — a dropped digit would otherwise pass unnoticed.
    #[test]
    fn no_rate_is_missing_or_out_of_range() {
        for land in Bundesland::ALL {
            let rate = PROPERTY_COSTS.transfer_tax_rate(land);
            assert!(
                rate.ppm() >= 35_000 && rate.ppm() <= 65_000,
                "{land:?} at {rate:?} is outside the 3,5–6,5 % range any state has ever set"
            );
        }
    }

    /// The notary approximation must be labelled as one in the provenance, since it is the
    /// only figure here that is not a rate from an Act.
    #[test]
    fn the_provenance_admits_the_notary_approximation() {
        let p = PROPERTY_COSTS.provenance;
        assert!(p.legal_basis.contains("GrEStG"));
        assert!(
            p.legal_basis.contains("approximated"),
            "the GNotKG figure is not implemented and the citation must say so"
        );
        assert_eq!(PROPERTY_COSTS.notary_and_registry_rate, pct(2_000));
    }

    #[test]
    fn both_verified_years_are_available_and_others_are_refused() {
        assert!(PropertyCostParameters::for_year(TaxYear::new(2025).unwrap()).is_ok());
        assert!(PropertyCostParameters::for_year(TaxYear::new(2026).unwrap()).is_ok());
        assert!(PropertyCostParameters::for_year(TaxYear::new(2027).unwrap()).is_err());
    }
}
