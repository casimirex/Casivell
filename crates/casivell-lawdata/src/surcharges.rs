//! Parameters for the two surcharges levied on assessed income tax.

use casivell_core::{Money, MoneyError, Rate, TaxYear};

use crate::provenance::{DataStatus, Provenance};

/// Solidaritätszuschlag parameters, SolzG 1995.
///
/// # The mechanism the original specification got wrong
///
/// The surcharge is not a flat 5.5 % of income tax. Since the 2021 reform it has
/// three regimes, and the middle one is where most higher earners actually sit:
///
/// 1. **Below the Freigrenze** (§ 3 Abs. 3 SolzG): no surcharge at all. The
///    threshold is on the *assessed income tax*, not on income — a distinction
///    worth stressing, because 20 350 € of tax corresponds to roughly 75 000 € of
///    taxable income for a single person.
/// 2. **The Milderungszone** (§ 4 Satz 2): the surcharge is capped at 11.9 % of
///    the amount by which the income tax exceeds the Freigrenze, which phases it
///    in gradually rather than imposing a cliff.
/// 3. **Above the zone**: the full 5.5 % applies.
///
/// The zone's upper end is not stated as a number in the statute; it is wherever
/// the two formulas cross, which the engine computes rather than tabulates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolidarityParameters {
    /// The year these parameters apply to.
    pub year: TaxYear,
    /// The surcharge rate, 5.5 %, § 4 Satz 1 SolzG.
    pub rate: Rate,
    /// Freigrenze on assessed income tax for an individual assessment,
    /// § 3 Abs. 3 SolzG.
    pub exemption_individual: Money,
    /// Freigrenze for a joint assessment. Exactly twice the individual figure.
    pub exemption_joint: Money,
    /// The Milderungszone cap: 11.9 % of the excess over the Freigrenze,
    /// § 4 Satz 2 SolzG.
    pub taper_rate: Rate,
    /// Citation.
    pub provenance: Provenance,
}

impl SolidarityParameters {
    /// Returns the parameters for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if no verified set exists.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        match year.get() {
            2025 => Ok(SOLI_2025),
            2026 => Ok(SOLI_2026),
            other => Err(MoneyError::YearOutOfRange { year: other }),
        }
    }
}

/// A German federal state.
///
/// Enumerated in full rather than reduced to a boolean "is it Bavaria or
/// Baden-Württemberg", because the state is already needed for other purposes —
/// the Grunderwerbsteuer rate varies from 3.5 % to 6.5 % across these sixteen, and
/// public holidays differ — and a type that answers only one question would have
/// to be replaced later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bundesland {
    /// Baden-Württemberg. Church tax 8 %.
    BadenWuerttemberg,
    /// Freistaat Bayern. Church tax 8 %.
    Bayern,
    /// Berlin.
    Berlin,
    /// Brandenburg.
    Brandenburg,
    /// Freie Hansestadt Bremen.
    Bremen,
    /// Freie und Hansestadt Hamburg.
    Hamburg,
    /// Hessen.
    Hessen,
    /// Mecklenburg-Vorpommern.
    MecklenburgVorpommern,
    /// Niedersachsen.
    Niedersachsen,
    /// Nordrhein-Westfalen.
    NordrheinWestfalen,
    /// Rheinland-Pfalz.
    RheinlandPfalz,
    /// Saarland.
    Saarland,
    /// Freistaat Sachsen. Carries the higher employee share of care insurance.
    Sachsen,
    /// Sachsen-Anhalt.
    SachsenAnhalt,
    /// Schleswig-Holstein.
    SchleswigHolstein,
    /// Freistaat Thüringen.
    Thueringen,
}

impl Bundesland {
    /// Every state, for exhaustive iteration in tests and UI pickers.
    pub const ALL: [Self; 16] = [
        Self::BadenWuerttemberg,
        Self::Bayern,
        Self::Berlin,
        Self::Brandenburg,
        Self::Bremen,
        Self::Hamburg,
        Self::Hessen,
        Self::MecklenburgVorpommern,
        Self::Niedersachsen,
        Self::NordrheinWestfalen,
        Self::RheinlandPfalz,
        Self::Saarland,
        Self::Sachsen,
        Self::SachsenAnhalt,
        Self::SchleswigHolstein,
        Self::Thueringen,
    ];

    /// Whether this state levies the reduced 8 % church tax rate.
    ///
    /// Only Baden-Württemberg and Bayern do; the other fourteen levy 9 %.
    #[must_use]
    pub const fn has_reduced_church_tax_rate(self) -> bool {
        matches!(self, Self::BadenWuerttemberg | Self::Bayern)
    }

    /// Whether employees in this state bear the higher share of care insurance.
    ///
    /// Only Sachsen, which retained Buß- und Bettag as a public holiday
    /// (§ 58 Abs. 3 SGB XI).
    #[must_use]
    pub const fn has_higher_employee_care_share(self) -> bool {
        matches!(self, Self::Sachsen)
    }
}

/// Church tax parameters.
///
/// # Two simplifications, stated rather than hidden
///
/// 1. **The assessment base is not exactly the income tax.** § 51a Abs. 2 EStG
///    requires the base to be recomputed as if the full Kinderfreibetrag had been
///    deducted, even where Kindergeld was the more favourable option. For a
///    household with children the church tax base is therefore *lower* than the
///    assessed income tax. Casivell does not yet implement this, so church tax is
///    overstated for families; the discrepancy is nil for the childless.
///
/// 2. **Kappung is not modelled.** Most Landeskirchen cap church tax at between
///    2.75 % and 4 % of taxable income, which binds only at high incomes, and
///    several require the taxpayer to apply for it. The cap is set by each
///    Landeskirche rather than by statute and does not follow state borders.
///
/// Both are tracked as open items. Neither may be silently applied or silently
/// omitted in the UI: a figure that is knowably wrong for families must say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChurchTaxParameters {
    /// The year these parameters apply to.
    pub year: TaxYear,
    /// The reduced rate levied in Baden-Württemberg and Bayern.
    pub reduced_rate: Rate,
    /// The standard rate levied in the other fourteen states.
    pub standard_rate: Rate,
    /// Citation.
    pub provenance: Provenance,
}

impl ChurchTaxParameters {
    /// Returns the parameters for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if no verified set exists.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        match year.get() {
            2025 | 2026 => Ok(CHURCH_TAX),
            other => Err(MoneyError::YearOutOfRange { year: other }),
        }
    }

    /// The rate applicable in `land`.
    #[must_use]
    pub const fn rate_in(&self, land: Bundesland) -> Rate {
        if land.has_reduced_church_tax_rate() {
            self.reduced_rate
        } else {
            self.standard_rate
        }
    }
}

/// Builds a [`Money`] from whole euro inside a `const` table.
///
/// See the equivalent helper in [`crate::social`] for why the error arm resolves
/// to zero rather than panicking, and which test guards it.
const fn euro(whole: i64) -> Money {
    match Money::from_euro(whole) {
        Ok(m) => m,
        Err(_) => Money::ZERO,
    }
}

/// Builds a [`Rate`] from thousandths of a percent inside a `const` table.
const fn pct_milli(percent_millis: i64) -> Rate {
    match Rate::from_percent_millis(percent_millis) {
        Ok(r) => r,
        Err(_) => Rate::ZERO,
    }
}

/// Constructs a [`TaxYear`] inside a `const` table.
const fn year(value: u16) -> TaxYear {
    match TaxYear::new(value) {
        Ok(y) => y,
        Err(_) => TaxYear::MIN,
    }
}

const SOLI_2025: SolidarityParameters = SolidarityParameters {
    year: year(2025),
    rate: pct_milli(5_500),
    exemption_individual: euro(19_950),
    exemption_joint: euro(39_900),
    taper_rate: pct_milli(11_900),
    provenance: Provenance::new(
        "§ 3 Abs. 3 und § 4 SolzG 1995, Fassung für VZ 2025",
        "https://www.gesetze-im-internet.de/solzg_1995/__3.html",
        "2026-07-30",
        DataStatus::Enacted,
    ),
};

const SOLI_2026: SolidarityParameters = SolidarityParameters {
    year: year(2026),
    rate: pct_milli(5_500),
    exemption_individual: euro(20_350),
    exemption_joint: euro(40_700),
    taper_rate: pct_milli(11_900),
    provenance: Provenance::new(
        "§ 3 Abs. 3 und § 4 SolzG 1995, Fassung ab VZ 2026",
        "https://www.gesetze-im-internet.de/solzg_1995/__3.html",
        "2026-07-30",
        DataStatus::Enacted,
    ),
};

/// Church tax rates, which have not changed for decades and are the same in 2025
/// and 2026.
const CHURCH_TAX: ChurchTaxParameters = ChurchTaxParameters {
    // The year field is nominal here: the rates are Landeskirchensteuergesetze,
    // not annual ordinances. Retained so the struct is uniform with the others.
    year: year(2026),
    reduced_rate: pct_milli(8_000),
    standard_rate: pct_milli(9_000),
    provenance: Provenance::new(
        "§ 51a EStG i. V. m. den Kirchensteuergesetzen der Länder",
        "https://www.gesetze-im-internet.de/estg/__51a.html",
        "2026-07-30",
        DataStatus::Enacted,
    ),
};

#[cfg(test)]
mod tests {
    use super::{
        Bundesland, CHURCH_TAX, ChurchTaxParameters, SOLI_2025, SOLI_2026, SolidarityParameters,
    };
    use casivell_core::{Rate, TaxYear};

    fn every_soli() -> [SolidarityParameters; 2] {
        [SOLI_2025, SOLI_2026]
    }

    #[test]
    fn no_amount_or_rate_in_a_table_is_accidentally_zero() {
        for p in every_soli() {
            assert!(!p.exemption_individual.is_zero());
            assert!(!p.exemption_joint.is_zero());
            assert!(!p.rate.is_zero());
            assert!(!p.taper_rate.is_zero());
        }
        assert!(!CHURCH_TAX.reduced_rate.is_zero());
        assert!(!CHURCH_TAX.standard_rate.is_zero());
    }

    /// § 3 Abs. 3 SolzG sets the joint Freigrenze at exactly twice the individual
    /// one. Transcribing them independently makes a typo possible, so the
    /// relationship is asserted rather than assumed.
    #[test]
    fn the_joint_exemption_is_exactly_double_the_individual_one() {
        for p in every_soli() {
            let doubled = p
                .exemption_individual
                .mul_int(2)
                .expect("twice the exemption is within the domain");
            assert_eq!(
                p.exemption_joint,
                doubled,
                "{}: joint Freigrenze is not twice the individual one",
                p.year.get()
            );
        }
    }

    #[test]
    fn the_2026_exemption_rose_above_the_2025_one() {
        assert!(SOLI_2026.exemption_individual > SOLI_2025.exemption_individual);
        assert_eq!(SOLI_2025.exemption_individual.cents(), 1_995_000);
        assert_eq!(SOLI_2026.exemption_individual.cents(), 2_035_000);
    }

    /// The rate and the taper have both been unchanged since the 2021 reform.
    #[test]
    fn the_rate_and_taper_are_stable_across_years() {
        assert_eq!(SOLI_2025.rate, SOLI_2026.rate);
        assert_eq!(SOLI_2025.taper_rate, SOLI_2026.taper_rate);
        assert_eq!(
            SOLI_2026.rate,
            Rate::from_percent_millis(5_500).expect("valid")
        );
        assert_eq!(
            SOLI_2026.taper_rate,
            Rate::from_percent_millis(11_900).expect("valid")
        );
    }

    /// The taper must exceed the headline rate, or the Milderungszone would never
    /// close and the full 5.5 % would never be reached.
    #[test]
    fn the_taper_rate_exceeds_the_headline_rate() {
        for p in every_soli() {
            assert!(
                p.taper_rate.ppm() > p.rate.ppm(),
                "{}: the Milderungszone would never close",
                p.year.get()
            );
        }
    }

    #[test]
    fn every_supported_year_has_surcharge_parameters() {
        let mut y = TaxYear::MIN.get();
        while y <= TaxYear::MAX.get() {
            let tax_year = TaxYear::new(y).expect("in range");
            assert!(
                SolidarityParameters::for_year(tax_year).is_ok(),
                "no Soli for {y}"
            );
            assert!(
                ChurchTaxParameters::for_year(tax_year).is_ok(),
                "no church tax for {y}"
            );
            y = y.saturating_add(1);
        }
    }

    /// Exactly two of the sixteen states levy the reduced rate. Both an omission
    /// and an over-broad match would be caught by the count.
    #[test]
    fn exactly_two_states_levy_the_reduced_church_tax_rate() {
        let reduced = Bundesland::ALL
            .iter()
            .filter(|land| land.has_reduced_church_tax_rate())
            .count();
        assert_eq!(reduced, 2);
        assert!(Bundesland::BadenWuerttemberg.has_reduced_church_tax_rate());
        assert!(Bundesland::Bayern.has_reduced_church_tax_rate());
        assert!(!Bundesland::Berlin.has_reduced_church_tax_rate());
    }

    /// Exactly one state carries the higher employee care share.
    #[test]
    fn exactly_one_state_has_the_higher_employee_care_share() {
        let higher = Bundesland::ALL
            .iter()
            .filter(|land| land.has_higher_employee_care_share())
            .count();
        assert_eq!(higher, 1);
        assert!(Bundesland::Sachsen.has_higher_employee_care_share());
        assert!(!Bundesland::SachsenAnhalt.has_higher_employee_care_share());
    }

    /// `ALL` must list every variant exactly once. A variant added to the enum but
    /// forgotten here would silently disappear from every UI picker and every
    /// exhaustive test in the crate.
    #[test]
    fn all_lists_every_state_exactly_once() {
        assert_eq!(Bundesland::ALL.len(), 16);
        for (i, a) in Bundesland::ALL.iter().enumerate() {
            for b in Bundesland::ALL.iter().skip(i.saturating_add(1)) {
                assert_ne!(a, b, "Bundesland::ALL lists {a:?} twice");
            }
        }
    }

    #[test]
    fn the_rate_lookup_follows_the_state() {
        assert_eq!(
            CHURCH_TAX.rate_in(Bundesland::Bayern),
            CHURCH_TAX.reduced_rate
        );
        assert_eq!(
            CHURCH_TAX.rate_in(Bundesland::NordrheinWestfalen),
            CHURCH_TAX.standard_rate
        );
        for land in Bundesland::ALL {
            let rate = CHURCH_TAX.rate_in(land);
            assert!(rate == CHURCH_TAX.reduced_rate || rate == CHURCH_TAX.standard_rate);
        }
    }
}
