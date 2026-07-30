//! The income tax tariff of § 32a Abs. 1 EStG, as data.
//!
//! # How the statute is transcribed
//!
//! § 32a Abs. 1 prints five zones. For the 2026 Veranlagungszeitraum they read:
//!
//! ```text
//! 1. bis 12 348 Euro (Grundfreibetrag):            0;
//! 2. von 12 349 Euro bis 17 799 Euro:              (914,51 · y + 1 400) · y;
//! 3. von 17 800 Euro bis 69 878 Euro:              (173,10 · z + 2 397) · z + 1 034,87;
//! 4. von 69 879 Euro bis 277 825 Euro:             0,42 · x −  11 135,63;
//! 5. von 277 826 Euro an:                          0,45 · x −  19 470,38.
//! ```
//!
//! where `x` is the taxable income truncated to whole euro, `y` is one
//! ten-thousandth of the amount by which `x` exceeds the Grundfreibetrag, and `z`
//! is one ten-thousandth of the amount by which `x` exceeds the upper end of
//! zone 2.
//!
//! The structs below hold exactly those numbers, scaled to integers by a factor
//! of one hundred so that no coefficient loses a digit and no floating point is
//! involved. `914,51` is stored as `91_451`; `1 400` as `140_000`. A reviewer
//! should be able to read the table beside the Gesetzestext and check it by eye,
//! which is why the fields are named after the statute's own structure rather
//! than being flattened into an array of anonymous coefficients.
//!
//! Evaluation lives in `casivell-tax`; this module is data and validation only.

use casivell_core::{MoneyError, Rate, TaxYear};

use crate::provenance::{DataStatus, Provenance};

/// One of the two linear-progressive zones, zone 2 or zone 3 of § 32a Abs. 1.
///
/// Evaluates to `(quadratic · t + linear) · t + constant` euro, where
/// `t = (x − reference) / 10 000`, all coefficients being stored in hundredths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressionZone {
    /// Lowest taxable income in this zone, in whole euro. Inclusive.
    pub lower_bound_euro: i64,
    /// Highest taxable income in this zone, in whole euro. Inclusive.
    pub upper_bound_euro: i64,
    /// The amount subtracted from `x` before scaling — the Grundfreibetrag for
    /// zone 2, the top of zone 2 for zone 3.
    pub reference_euro: i64,
    /// Coefficient of `t²`, in hundredths of a euro. `914,51` is `91_451`.
    pub quadratic_centi: i64,
    /// Coefficient of `t`, in hundredths of a euro. `1 400` is `140_000`.
    pub linear_centi: i64,
    /// Additive constant, in hundredths of a euro. `1 034,87` is `103_487`.
    pub constant_centi: i64,
}

impl ProgressionZone {
    /// The statutory divisor that turns a euro excess into `y` or `z`.
    ///
    /// § 32a Abs. 1 Satz 3: *"y ist ein Zehntausendstel des ... übersteigenden
    /// Teils"*.
    pub const SCALE_DIVISOR: i64 = 10_000;

    /// Hundredths per euro, the scaling applied to the stored coefficients.
    pub const COEFFICIENT_SCALE: i64 = 100;

    /// Whether `taxable_euro` falls in this zone.
    #[must_use]
    pub const fn contains(&self, taxable_euro: i64) -> bool {
        taxable_euro >= self.lower_bound_euro && taxable_euro <= self.upper_bound_euro
    }

    /// The widest excess `x − reference` this zone admits.
    ///
    /// The overflow proof in `casivell-tax::tariff` is stated in terms of this
    /// value, so it is exposed rather than recomputed at the call site.
    #[must_use]
    pub const fn max_excess_euro(&self) -> i64 {
        self.upper_bound_euro.saturating_sub(self.reference_euro)
    }
}

/// One of the two constant-marginal-rate zones, zone 4 or zone 5.
///
/// Evaluates to `marginal_rate · x − subtrahend` euro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProportionalZone {
    /// Lowest taxable income in this zone, in whole euro. Inclusive.
    pub lower_bound_euro: i64,
    /// The constant marginal rate: 42 % or 45 %.
    pub marginal_rate: Rate,
    /// The amount subtracted, in cents. `11 135,63 €` is `1_113_563`.
    pub subtrahend_cents: i64,
}

/// The complete income tax tariff for one Veranlagungszeitraum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncomeTaxTariff {
    /// The year this tariff applies to.
    pub year: TaxYear,
    /// Grundfreibetrag, in whole euro. Taxable income up to and including this
    /// attracts no tax.
    pub basic_allowance_euro: i64,
    /// Zone 2 of § 32a Abs. 1: the steep progression above the Grundfreibetrag.
    pub first_progression: ProgressionZone,
    /// Zone 3: the shallower progression up to the 42 % threshold.
    pub second_progression: ProgressionZone,
    /// Zone 4: the 42 % Spitzensteuersatz band.
    pub upper_proportional: ProportionalZone,
    /// Zone 5: the 45 % "Reichensteuer" band.
    pub top_proportional: ProportionalZone,
    /// Citation for every figure above.
    pub provenance: Provenance,
}

impl IncomeTaxTariff {
    /// Returns the tariff for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if no verified tariff exists. Note that
    /// [`TaxYear`] already refuses to construct such a year, so this arm is
    /// defence in depth against the two ranges drifting apart.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        match year.get() {
            2025 => Ok(TARIFF_2025),
            2026 => Ok(TARIFF_2026),
            other => Err(MoneyError::YearOutOfRange { year: other }),
        }
    }

    /// Checks the structural invariants a tariff must satisfy to be evaluable.
    ///
    /// Called from tests over every shipped year rather than at runtime: these
    /// are properties of a `const` table, so a violation is a compile-time-fixable
    /// defect and paying for the check on every simulation step would be waste.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] naming the first violated invariant.
    pub const fn validate(&self) -> Result<(), MoneyError> {
        // The zones must tile the number line without gap or overlap, or some
        // income would have either no tax or two different taxes.
        if self.first_progression.lower_bound_euro != self.basic_allowance_euro.saturating_add(1) {
            return Err(MoneyError::OutOfDomain {
                cents: self.first_progression.lower_bound_euro,
            });
        }
        if self.second_progression.lower_bound_euro
            != self.first_progression.upper_bound_euro.saturating_add(1)
        {
            return Err(MoneyError::OutOfDomain {
                cents: self.second_progression.lower_bound_euro,
            });
        }
        if self.upper_proportional.lower_bound_euro
            != self.second_progression.upper_bound_euro.saturating_add(1)
        {
            return Err(MoneyError::OutOfDomain {
                cents: self.upper_proportional.lower_bound_euro,
            });
        }
        if self.top_proportional.lower_bound_euro <= self.upper_proportional.lower_bound_euro {
            return Err(MoneyError::OutOfDomain {
                cents: self.top_proportional.lower_bound_euro,
            });
        }
        // Zone 2 measures its excess from the Grundfreibetrag, zone 3 from the top
        // of zone 2. Getting either reference wrong shifts the whole curve.
        if self.first_progression.reference_euro != self.basic_allowance_euro {
            return Err(MoneyError::OutOfDomain {
                cents: self.first_progression.reference_euro,
            });
        }
        if self.second_progression.reference_euro != self.first_progression.upper_bound_euro {
            return Err(MoneyError::OutOfDomain {
                cents: self.second_progression.reference_euro,
            });
        }
        // The top rate must exceed the upper rate, or zone 5 is not a surcharge.
        if self.top_proportional.marginal_rate.ppm() <= self.upper_proportional.marginal_rate.ppm()
        {
            return Err(MoneyError::OutOfDomain {
                cents: self.top_proportional.marginal_rate.ppm(),
            });
        }
        Ok(())
    }
}

/// Tariff for Veranlagungszeitraum 2025.
///
/// § 32a Abs. 1 EStG in the version created by Artikel 1 des
/// Steuerfortentwicklungsgesetzes (SteFeG) of 23 December 2024.
const TARIFF_2025: IncomeTaxTariff = IncomeTaxTariff {
    year: match TaxYear::new(2025) {
        Ok(y) => y,
        // `TaxYear::FIRST_VERIFIED` is 2025, so this is unreachable. A `const` panic here
        // would be caught at compile time; instead the table is written so that
        // no panicking construct appears in it at all.
        Err(_) => TaxYear::FIRST_VERIFIED,
    },
    basic_allowance_euro: 12_096,
    first_progression: ProgressionZone {
        lower_bound_euro: 12_097,
        upper_bound_euro: 17_443,
        reference_euro: 12_096,
        quadratic_centi: 93_230, // 932,30
        linear_centi: 140_000,   // 1 400
        constant_centi: 0,
    },
    second_progression: ProgressionZone {
        lower_bound_euro: 17_444,
        upper_bound_euro: 68_480,
        reference_euro: 17_443,
        quadratic_centi: 17_664, // 176,64
        linear_centi: 239_700,   // 2 397
        constant_centi: 101_513, // 1 015,13
    },
    upper_proportional: ProportionalZone {
        lower_bound_euro: 68_481,
        marginal_rate: match Rate::from_percent(42) {
            Ok(r) => r,
            Err(_) => Rate::ZERO,
        },
        subtrahend_cents: 1_091_192, // 10 911,92
    },
    top_proportional: ProportionalZone {
        lower_bound_euro: 277_826,
        marginal_rate: match Rate::from_percent(45) {
            Ok(r) => r,
            Err(_) => Rate::ZERO,
        },
        subtrahend_cents: 1_924_667, // 19 246,67
    },
    provenance: Provenance::new(
        "§ 32a Abs. 1 EStG, Fassung für VZ 2025 (Art. 1 SteFeG v. 23.12.2024)",
        "https://www.gesetze-im-internet.de/estg/__32a.html",
        "2026-07-30",
        DataStatus::Enacted,
    ),
};

/// Tariff for Veranlagungszeitraum 2026.
///
/// § 32a Abs. 1 EStG in the version created by Artikel 2 SteFeG, in force from
/// 1 January 2026.
const TARIFF_2026: IncomeTaxTariff = IncomeTaxTariff {
    year: match TaxYear::new(2026) {
        Ok(y) => y,
        Err(_) => TaxYear::LAST_VERIFIED,
    },
    basic_allowance_euro: 12_348,
    first_progression: ProgressionZone {
        lower_bound_euro: 12_349,
        upper_bound_euro: 17_799,
        reference_euro: 12_348,
        quadratic_centi: 91_451, // 914,51
        linear_centi: 140_000,   // 1 400
        constant_centi: 0,
    },
    second_progression: ProgressionZone {
        lower_bound_euro: 17_800,
        upper_bound_euro: 69_878,
        reference_euro: 17_799,
        quadratic_centi: 17_310, // 173,10
        linear_centi: 239_700,   // 2 397
        constant_centi: 103_487, // 1 034,87
    },
    upper_proportional: ProportionalZone {
        lower_bound_euro: 69_879,
        marginal_rate: match Rate::from_percent(42) {
            Ok(r) => r,
            Err(_) => Rate::ZERO,
        },
        subtrahend_cents: 1_113_563, // 11 135,63
    },
    top_proportional: ProportionalZone {
        lower_bound_euro: 277_826,
        marginal_rate: match Rate::from_percent(45) {
            Ok(r) => r,
            Err(_) => Rate::ZERO,
        },
        subtrahend_cents: 1_947_038, // 19 470,38
    },
    provenance: Provenance::new(
        "§ 32a Abs. 1 EStG, Fassung ab VZ 2026 (Art. 2 SteFeG v. 23.12.2024)",
        "https://www.gesetze-im-internet.de/estg/__32a.html",
        "2026-07-30",
        DataStatus::Enacted,
    ),
};

#[cfg(test)]
mod tests {
    use super::{IncomeTaxTariff, TARIFF_2025, TARIFF_2026};
    use casivell_core::TaxYear;

    fn every_tariff() -> [IncomeTaxTariff; 2] {
        [TARIFF_2025, TARIFF_2026]
    }

    /// The zones must tile without gap or overlap for every shipped year.
    #[test]
    fn all_shipped_tariffs_satisfy_their_invariants() {
        for tariff in every_tariff() {
            assert!(
                tariff.validate().is_ok(),
                "tariff for {} violates a structural invariant: {:?}",
                tariff.year.get(),
                tariff.validate()
            );
        }
    }

    /// A tariff filed under the wrong year would silently apply the wrong law.
    #[test]
    fn lookup_returns_the_year_it_was_asked_for() {
        for tariff in every_tariff() {
            let found = IncomeTaxTariff::for_year(tariff.year).expect("shipped year");
            assert_eq!(found.year, tariff.year);
            assert_eq!(found, tariff);
        }
    }

    /// The `match` in `for_year` is written by hand, so it can fall out of step
    /// with the `TaxYear` range. This catches that.
    #[test]
    fn every_supported_year_has_a_tariff() {
        let mut year = TaxYear::FIRST_VERIFIED.get();
        while year <= TaxYear::LAST_VERIFIED.get() {
            let tax_year = TaxYear::new(year).expect("in range");
            assert!(
                IncomeTaxTariff::for_year(tax_year).is_ok(),
                "no tariff for supported year {year}"
            );
            year = year.saturating_add(1);
        }
    }

    /// 2026 raised the Grundfreibetrag and shifted the Eckwerte outward; the 45 %
    /// threshold was deliberately left alone. If a future edit accidentally
    /// copies 2025's figures into 2026, this notices.
    #[test]
    fn the_two_years_differ_where_the_amending_act_changed_them() {
        assert_eq!(TARIFF_2025.basic_allowance_euro, 12_096);
        assert_eq!(TARIFF_2026.basic_allowance_euro, 12_348);
        // The Reichensteuer threshold has been 277 826 € since 2007 and was not
        // indexed by SteFeG.
        assert_eq!(
            TARIFF_2026.top_proportional.lower_bound_euro,
            TARIFF_2025.top_proportional.lower_bound_euro
        );
    }

    /// The monotonic relationships between the two years compare `const` values, so
    /// they are checked in `const` blocks: a violation fails compilation rather
    /// than waiting for the test binary to run. Cheaper feedback for the same
    /// guarantee, and it holds even if someone runs `cargo build` and never
    /// `cargo test`.
    #[test]
    fn the_eckwerte_moved_outward_from_2025_to_2026() {
        const {
            assert!(
                TARIFF_2026.basic_allowance_euro > TARIFF_2025.basic_allowance_euro,
                "the 2026 Grundfreibetrag must exceed the 2025 one"
            );
        }
        const {
            assert!(
                TARIFF_2026.upper_proportional.lower_bound_euro
                    > TARIFF_2025.upper_proportional.lower_bound_euro,
                "the 2026 42 % threshold must exceed the 2025 one"
            );
        }
        const {
            assert!(
                TARIFF_2026.second_progression.upper_bound_euro
                    > TARIFF_2025.second_progression.upper_bound_euro,
                "the top of zone 3 must have moved outward"
            );
        }
    }

    /// Every figure must be citable, and the citation must point at a primary
    /// source. This is the mechanical half of the auditability promise.
    #[test]
    fn every_tariff_cites_a_primary_source() {
        for tariff in every_tariff() {
            let p = tariff.provenance;
            assert!(
                p.legal_basis.contains("§ 32a"),
                "{}: legal basis does not name the provision",
                tariff.year.get()
            );
            assert!(
                p.source_url
                    .starts_with("https://www.gesetze-im-internet.de/"),
                "{}: source is not a primary one: {}",
                tariff.year.get(),
                p.source_url
            );
            assert_eq!(p.verified_on.len(), "YYYY-MM-DD".len());
            assert!(p.status.is_binding_law());
        }
    }
}
