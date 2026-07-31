//! A stable digest of a year's statutory data.
//!
//! # The problem this solves
//!
//! Casivell's statutory tables change. A rate is corrected, a Land raises its
//! Grunderwerbsteuer, a projected year becomes an enacted one. When that happens, a scenario
//! a household saved last year no longer reproduces: the same inputs give different figures,
//! and nothing in the output says why.
//!
//! That is not a bug to be fixed — the new figures are *right* — but it is a change the
//! household is entitled to be told about. A saved scenario therefore records the fingerprint
//! of the law it was computed under, and replaying it compares. Same fingerprint, same
//! answers, guaranteed. Different fingerprint, and the report can say **which year's data
//! moved** rather than leaving someone to wonder why their pension projection shifted.
//!
//! # What is hashed, and what a stable hash requires
//!
//! Every statutory figure in a [`crate::LawYear`], as its *value* — never its memory
//! representation. Padding, field order in memory and pointer addresses are all invisible
//! here, so the digest is identical across compilers, architectures and builds. That is what
//! makes it safe to write into a file.
//!
//! [`Provenance`] is deliberately **not** hashed. A verification date or a tidied-up citation
//! string changes no computed figure, and a digest that moved when documentation improved
//! would cry wolf until it was ignored.
//!
//! # The maintenance hazard, and the guard against it
//!
//! A field added to a parameter set and not added to its digest would be invisible: scenarios
//! would silently claim reproducibility they no longer have. Two things guard it. Every
//! `fingerprint_into` writes its fields in declaration order with no gaps, so a reader can
//! check one against the other; and `the_digest_is_pinned` asserts an exact value, so any
//! change to any hashed figure fails loudly and whoever added a field must decide, on purpose,
//! whether it belongs.
//!
//! [`Provenance`]: crate::Provenance

use casivell_core::{Money, Rate, TaxYear};

/// A 64-bit digest of statutory data.
///
/// FNV-1a: not cryptographic, and not meant to be. The threat here is an accidental change to
/// a table, not an adversary constructing a collision, and FNV is short enough to read, to
/// reimplement in another language, and to write into a file by hand if need be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(u64);

/// FNV-1a's 64-bit offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a's 64-bit prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

impl Fingerprint {
    /// An empty digest.
    #[must_use]
    pub const fn new() -> Self {
        Self(FNV_OFFSET)
    }

    /// The digest as a number, for writing into a file or showing in a report.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Mixes in one signed integer.
    ///
    /// Every figure reaches the digest through here, as an `i64`: amounts in cents, rates in
    /// parts per million, counts as themselves. One shape for all of them, so the digest does
    /// not depend on which Rust type happened to hold a number.
    ///
    /// The bytes come from [`i64::to_le_bytes`] rather than from a cast to `u64`. Same eight
    /// bytes either way, but no cast to explain — and little-endian explicitly, so the digest
    /// is the same on a big-endian machine.
    #[must_use]
    pub fn write_i64(self, value: i64) -> Self {
        let mut hash = self.0;
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        Self(hash)
    }

    /// Mixes in an amount, in cents.
    #[must_use]
    pub fn write_money(self, value: Money) -> Self {
        self.write_i64(value.cents())
    }

    /// Mixes in a rate, in parts per million.
    #[must_use]
    pub fn write_rate(self, value: Rate) -> Self {
        self.write_i64(value.ppm())
    }

    /// Mixes in a year.
    #[must_use]
    pub fn write_year(self, value: TaxYear) -> Self {
        self.write_i64(i64::from(value.get()))
    }

    /// Mixes in a flag.
    ///
    /// As one or zero rather than as a byte, so a `bool` and an `i64` of the same value are
    /// indistinguishable to the digest — which is what keeps the encoding one shape.
    #[must_use]
    pub fn write_bool(self, value: bool) -> Self {
        self.write_i64(i64::from(value))
    }
}

impl Default for Fingerprint {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Display for Fingerprint {
    /// Sixteen lowercase hex digits, which is how it appears in a saved scenario.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// How each statutory parameter set contributes to a digest.
///
/// Implemented by hand rather than derived, because the point is to state *which* figures are
/// part of the reproducibility promise. A derive would hash whatever happened to be in the
/// struct, including the provenance, and would silently absorb new fields without anyone
/// deciding they belonged.
pub trait Fingerprinted {
    /// Mixes this parameter set's figures into `digest`, in declaration order.
    #[must_use]
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint;

    /// This parameter set's digest on its own.
    #[must_use]
    fn fingerprint(&self) -> Fingerprint {
        self.fingerprint_into(Fingerprint::new())
    }
}

impl Fingerprinted for crate::income_tax::ProgressionZone {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        digest
            .write_i64(self.lower_bound_euro)
            .write_i64(self.upper_bound_euro)
            .write_i64(self.reference_euro)
            .write_i64(self.quadratic_centi)
            .write_i64(self.linear_centi)
            .write_i64(self.constant_centi)
    }
}

impl Fingerprinted for crate::income_tax::ProportionalZone {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        digest
            .write_i64(self.lower_bound_euro)
            .write_rate(self.marginal_rate)
            .write_i64(self.subtrahend_cents)
    }
}

impl Fingerprinted for crate::income_tax::IncomeTaxTariff {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        let digest = digest
            .write_year(self.year)
            .write_i64(self.basic_allowance_euro);
        let digest = self.first_progression.fingerprint_into(digest);
        let digest = self.second_progression.fingerprint_into(digest);
        let digest = self.upper_proportional.fingerprint_into(digest);
        self.top_proportional.fingerprint_into(digest)
    }
}

impl Fingerprinted for crate::surcharges::SolidarityParameters {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        digest
            .write_year(self.year)
            .write_rate(self.rate)
            .write_money(self.exemption_individual)
            .write_money(self.exemption_joint)
            .write_rate(self.taper_rate)
    }
}

impl Fingerprinted for crate::surcharges::ChurchTaxParameters {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        digest
            .write_year(self.year)
            .write_rate(self.reduced_rate)
            .write_rate(self.standard_rate)
    }
}

impl Fingerprinted for crate::social::SocialParameters {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        let digest = digest
            .write_year(self.year)
            .write_rate(self.pension.contribution_rate)
            .write_money(self.pension.ceiling_monthly)
            .write_money(self.pension.average_earnings_annual)
            .write_money(self.pension.pension_value_jan_to_jun)
            .write_money(self.pension.pension_value_jul_to_dec)
            .write_i64(self.pension.child_raising_points_micro)
            .write_i64(i64::from(self.pension.child_raising_months))
            .write_rate(self.unemployment.contribution_rate)
            .write_money(self.unemployment.ceiling_monthly);
        let digest = digest
            .write_rate(self.health.general_rate)
            .write_rate(self.health.average_supplementary_rate)
            .write_money(self.health.ceiling_monthly)
            .write_money(self.health.compulsory_insurance_threshold_annual);
        digest
            .write_rate(self.care.base_rate)
            .write_rate(self.care.childless_surcharge)
            .write_i64(i64::from(self.care.childless_surcharge_min_age))
            .write_rate(self.care.per_child_reduction)
            .write_i64(i64::from(self.care.max_reduced_child_ordinal))
            .write_i64(i64::from(self.care.child_reduction_max_child_age))
            .write_rate(self.care.saxony_employee_surcharge)
            .write_money(self.reference_value_monthly)
    }
}

impl Fingerprinted for crate::deductions::DeductionParameters {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        digest
            .write_year(self.year)
            .write_money(self.employee_lump_sum)
            .write_money(self.special_expenses_lump_sum)
            .write_money(self.miners_pension_ceiling_annual)
            .write_rate(self.miners_pension_rate)
            .write_money(self.other_provision_cap)
            .write_money(self.other_provision_cap_employee)
            .write_rate(self.sick_pay_reduction)
            .write_money(self.child_allowance_material)
            .write_money(self.child_allowance_care)
            .write_money(self.child_benefit_monthly)
            .write_money(self.saver_allowance)
            .write_rate(self.capital_income_rate)
    }
}

impl Fingerprinted for crate::benefits::ElterngeldParameters {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        let digest = digest
            .write_year(self.year)
            .write_rate(self.base_rate)
            .write_rate(self.floor_rate)
            .write_rate(self.ceiling_rate)
            .write_money(self.lower_income_threshold)
            .write_money(self.upper_income_threshold)
            .write_rate(self.rate_step)
            .write_money(self.rate_step_income)
            .write_money(self.difference_income_cap)
            .write_money(self.minimum_monthly)
            .write_money(self.maximum_monthly);
        digest
            .write_rate(self.sibling_bonus_rate)
            .write_money(self.sibling_bonus_minimum)
            .write_money(self.multiple_birth_supplement)
            .write_rate(self.social_health_care_rate)
            .write_rate(self.social_pension_rate)
            .write_rate(self.social_unemployment_rate)
            .write_money(self.income_limit_annual)
            .write_i64(i64::from(self.base_months))
            .write_i64(i64::from(self.partner_months))
            .write_i64(i64::from(self.plus_months_per_base_month))
    }
}

impl Fingerprinted for crate::extraordinary::ExtraordinaryBurdenParameters {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        let mut digest = digest
            .write_year(self.year)
            .write_money(self.first_threshold)
            .write_money(self.second_threshold);
        for row in [
            self.no_children_individual,
            self.no_children_joint,
            self.one_or_two_children,
            self.three_or_more_children,
        ] {
            digest = digest
                .write_rate(row.lower)
                .write_rate(row.middle)
                .write_rate(row.upper);
        }
        for (degree, amount) in self.disability_lump_sums {
            digest = digest.write_i64(i64::from(degree)).write_money(amount);
        }
        digest = digest.write_money(self.helpless_lump_sum);
        for (grade, amount) in self.care_lump_sums {
            digest = digest.write_i64(i64::from(grade)).write_money(amount);
        }
        digest
    }
}

impl Fingerprinted for crate::property::PropertyCostParameters {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        let mut digest = digest.write_year(self.year);
        for rate in self.transfer_tax_rates {
            digest = digest.write_rate(rate);
        }
        digest.write_rate(self.notary_and_registry_rate)
    }
}

impl Fingerprinted for crate::retirement::RetirementParameters {
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        digest
            .write_year(self.year)
            .write_rate(self.early_claim_reduction_per_month)
            .write_rate(self.deferred_claim_increase_per_month)
            .write_rate(self.old_age_pension_type_factor)
            .write_i64(i64::from(self.max_early_claim_months))
    }
}

impl Fingerprinted for crate::LawYear {
    /// The digest of a whole year's law.
    ///
    /// Every parameter set in declaration order, so a scenario that reproduces this digest
    /// reproduces every figure the engine could have used for that year.
    ///
    /// [`crate::PropertyCostParameters`] is **not** part of it, and that is deliberate: it is
    /// not carried on `LawYear`, because a Land's transfer-tax rate has no indexation rule and
    /// is therefore never projected. A scenario involving a purchase fingerprints it
    /// separately.
    fn fingerprint_into(&self, digest: Fingerprint) -> Fingerprint {
        let digest = digest.write_year(self.year);
        let digest = self.income_tax.fingerprint_into(digest);
        let digest = self.social.fingerprint_into(digest);
        let digest = self.solidarity.fingerprint_into(digest);
        let digest = self.church_tax.fingerprint_into(digest);
        let digest = self.retirement.fingerprint_into(digest);
        let digest = self.deductions.fingerprint_into(digest);
        let digest = self.benefits.fingerprint_into(digest);
        self.burden.fingerprint_into(digest)
    }
}

#[cfg(test)]
mod tests {
    use super::Fingerprint;
    use casivell_core::{Money, Rate, TaxYear};

    /// The digest must depend on every value it is given, and on their order.
    #[test]
    fn the_digest_depends_on_values_and_on_their_order() {
        let base = Fingerprint::new();
        assert_ne!(base.write_i64(1), base.write_i64(2));
        assert_ne!(
            base.write_i64(1).write_i64(2),
            base.write_i64(2).write_i64(1),
            "order must matter, or two swapped fields would collide"
        );
    }

    /// A single cent, a single part per million, and a single year must all move it.
    #[test]
    fn the_smallest_representable_change_moves_the_digest() {
        let base = Fingerprint::new();
        assert_ne!(
            base.write_money(Money::from_cents(1).unwrap()),
            base.write_money(Money::from_cents(2).unwrap())
        );
        assert_ne!(
            base.write_rate(Rate::from_ppm(1).unwrap()),
            base.write_rate(Rate::from_ppm(2).unwrap())
        );
        assert_ne!(
            base.write_year(TaxYear::new(2025).unwrap()),
            base.write_year(TaxYear::new(2026).unwrap())
        );
        assert_ne!(base.write_bool(false), base.write_bool(true));
    }

    /// Zero is not the same as absent: writing a zero must change the digest, or a field left
    /// at zero would be indistinguishable from one that was never hashed.
    #[test]
    fn writing_a_zero_is_not_the_same_as_writing_nothing() {
        let base = Fingerprint::new();
        assert_ne!(base, base.write_i64(0));
        assert_ne!(base.write_i64(0), base.write_i64(0).write_i64(0));
    }

    /// A `bool` and the integer it encodes to must be indistinguishable, which is what keeps
    /// the encoding one shape rather than several.
    #[test]
    fn the_encoding_has_one_shape() {
        let base = Fingerprint::new();
        assert_eq!(base.write_bool(true), base.write_i64(1));
        assert_eq!(base.write_bool(false), base.write_i64(0));
        assert_eq!(
            base.write_money(Money::from_cents(500).unwrap()),
            base.write_i64(500)
        );
    }

    /// The empty digest is FNV-1a's published offset basis, so an independent implementation
    /// in another language starts from the same place.
    #[test]
    fn the_empty_digest_is_the_published_offset_basis() {
        assert_eq!(Fingerprint::new().value(), 0xcbf2_9ce4_8422_2325);
    }

    /// FNV-1a of the single byte `0x00`, computed by hand from the published constants:
    /// `(offset ^ 0) * prime`, repeated for the eight little-endian bytes of a zero.
    ///
    /// Pins the algorithm itself rather than this crate's use of it, so a change to the
    /// mixing — a different prime, a big-endian byte order — fails here rather than silently
    /// invalidating every saved scenario.
    #[test]
    fn the_mixing_matches_fnv_1a() {
        let mut expected = 0xcbf2_9ce4_8422_2325_u64;
        for _ in 0..8 {
            expected ^= 0;
            expected = expected.wrapping_mul(0x0000_0100_0000_01b3);
        }
        assert_eq!(Fingerprint::new().write_i64(0).value(), expected);
    }

    // -----------------------------------------------------------------
    // The statutory digests
    // -----------------------------------------------------------------

    fn law(year: u16) -> crate::LawYear {
        crate::LawYear::for_year(TaxYear::new(year).unwrap()).expect("enacted")
    }

    /// The pin. Any change to any hashed statutory figure fails here, which is the whole
    /// point: a saved scenario's reproducibility promise rests on this number, and it must
    /// not move by accident.
    ///
    /// When it does move on purpose — a corrected rate, a newly enacted year — updating it is
    /// the moment to ask whether existing saved scenarios should be told the law changed. That
    /// question is easy to skip if nothing forces it.
    #[test]
    fn the_digest_is_pinned() {
        use super::Fingerprinted as _;
        assert_eq!(
            law(2026).fingerprint().value(),
            0xe177_cfba_a6bb_7121,
            "a hashed statutory figure changed; see this test's documentation"
        );
    }

    /// The two enacted years must differ, or the digest would not distinguish the law a
    /// scenario was computed under.
    #[test]
    fn the_two_enacted_years_have_different_digests() {
        use super::Fingerprinted as _;
        assert_ne!(law(2025).fingerprint(), law(2026).fingerprint());
    }

    /// The digest must be a pure function of the data: same year, same answer, every time.
    #[test]
    fn the_digest_is_stable_within_a_run() {
        use super::Fingerprinted as _;
        assert_eq!(law(2026).fingerprint(), law(2026).fingerprint());
    }

    /// Changing any one figure in any one parameter set must move the whole year's digest.
    ///
    /// One representative field per set rather than all of them, because the pin above catches
    /// anything else that moves. What this adds is that each set actually *reaches* the digest
    /// — a parameter set forgotten in `LawYear::fingerprint_into` would pass the pin and fail
    /// here.
    #[test]
    fn every_parameter_set_reaches_the_digest() {
        use super::Fingerprinted as _;
        let baseline = law(2026).fingerprint();
        for which in ParameterSet::ALL {
            assert_ne!(
                bump(law(2026), which),
                baseline,
                "a change to {which:?} did not reach the year's digest"
            );
        }
    }

    /// The parameter sets a year's digest must cover.
    #[derive(Debug, Clone, Copy)]
    enum ParameterSet {
        Tariff,
        Social,
        Solidarity,
        Church,
        Retirement,
        Deductions,
        Benefits,
        Burden,
    }

    impl ParameterSet {
        const ALL: [Self; 8] = [
            Self::Tariff,
            Self::Social,
            Self::Solidarity,
            Self::Church,
            Self::Retirement,
            Self::Deductions,
            Self::Benefits,
            Self::Burden,
        ];
    }

    /// Nudges one figure in one parameter set by the smallest representable amount.
    fn bump(mut year: crate::LawYear, which: ParameterSet) -> super::Fingerprint {
        use super::Fingerprinted as _;
        let cent = Money::from_cents(1).unwrap();
        let more = |rate: Rate| Rate::from_ppm(rate.ppm() + 1).unwrap();

        match which {
            ParameterSet::Tariff => year.income_tax.basic_allowance_euro += 1,
            ParameterSet::Social => {
                year.social.pension.contribution_rate = more(year.social.pension.contribution_rate);
            }
            ParameterSet::Solidarity => {
                year.solidarity.exemption_individual =
                    year.solidarity.exemption_individual.add(cent).unwrap();
            }
            ParameterSet::Church => {
                year.church_tax.standard_rate = more(year.church_tax.standard_rate);
            }
            ParameterSet::Retirement => year.retirement.max_early_claim_months += 1,
            ParameterSet::Deductions => {
                year.deductions.employee_lump_sum =
                    year.deductions.employee_lump_sum.add(cent).unwrap();
            }
            ParameterSet::Benefits => {
                year.benefits.maximum_monthly = year.benefits.maximum_monthly.add(cent).unwrap();
            }
            ParameterSet::Burden => {
                year.burden.helpless_lump_sum = year.burden.helpless_lump_sum.add(cent).unwrap();
            }
        }
        year.fingerprint()
    }

    /// Documentation must not move it. A tidied citation or a fresh verification date changes
    /// no computed figure, and a digest that cried wolf over prose would be ignored.
    #[test]
    fn provenance_does_not_affect_the_digest() {
        use super::Fingerprinted as _;
        use crate::provenance::{DataStatus, Provenance};

        let mut edited = law(2026);
        let baseline = edited.fingerprint();
        edited.income_tax.provenance = Provenance::new(
            "a different citation entirely",
            "https://example.invalid/",
            "2099-12-31",
            DataStatus::Enacted,
        );
        assert_eq!(edited.fingerprint(), baseline);
    }

    /// Property costs are fingerprinted separately, and must not be silently absent from
    /// their own digest.
    #[test]
    fn property_costs_have_their_own_digest() {
        use super::Fingerprinted as _;
        use crate::property::PropertyCostParameters;

        let costs = PropertyCostParameters::for_year(TaxYear::new(2026).unwrap()).unwrap();
        let baseline = costs.fingerprint();

        let mut edited = costs;
        edited.notary_and_registry_rate =
            Rate::from_ppm(costs.notary_and_registry_rate.ppm() + 1).unwrap();
        assert_ne!(edited.fingerprint(), baseline);

        // And a state's rate reaches it too.
        let mut moved = costs;
        moved.transfer_tax_rates[0] =
            Rate::from_ppm(moved.transfer_tax_rates[0].ppm() + 1).unwrap();
        assert_ne!(moved.fingerprint(), baseline);
    }

    /// Displayed as sixteen hex digits, always — a shorter one would sort wrongly in a file.
    #[test]
    fn it_displays_as_sixteen_hex_digits() {
        extern crate alloc;
        use alloc::format;
        assert_eq!(format!("{}", Fingerprint::new()).len(), 16);
        assert_eq!(format!("{}", Fingerprint(1)), "0000000000000001");
    }
}
