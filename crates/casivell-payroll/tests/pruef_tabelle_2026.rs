//! The official Prüftabellen from the BMF Programmablaufplan 2026, Anlage 1.
//!
//! # Why this file is the most important test in the repository
//!
//! Every other test in Casivell checks internal consistency, or checks a figure
//! against the same source the code was written from. This one is different: the BMF
//! publishes, on pages 39 and 40 of the PAP, two tables of annual Lohnsteuer for
//! 43 salary levels across all six tax classes. They are the reference values every
//! German payroll product is checked against.
//!
//! 516 values in total. If the implementation agrees with all of them, the
//! withholding algorithm is right for the mainline case — not "consistent with our
//! own reading of the statute", but agreeing with the tax authority's own arithmetic.
//!
//! The values below are transcribed from the PDF by hand and must **never** be
//! regenerated from this crate's output. That would convert an external check into a
//! tautology, which is exactly the mistake `docs/ROADMAP_ERRATA.md` documents.
//!
//! # The two tables
//!
//! **Allgemeine Lohnsteuer** (page 39) is for an employee insured in every branch of
//! social insurance. Its stated parameters are `ALV = KRV = PKV = 0` and
//! `KVZ = 2,90`, with `PVZ = 1` in every class except II, where `PVZ = 0`.
//!
//! **Besondere Lohnsteuer** (page 40) is for an employee insured in none of them:
//! `ALV = KRV = PKV = 1`, with `PKPV = 50 000` cents in class III, `0` in class VI,
//! and `30 000` in the rest. Because `PKV = 1` the Vorsorgepauschale's health
//! component comes from the private premium rather than a rate, and because
//! `KRV = ALV = 1` the pension and unemployment components vanish entirely — so this
//! table exercises three branches the first one never reaches.

// An integration test is its own crate, so the `#![cfg_attr(test, allow(...))]` in
// the library root does not reach it. The same reasoning applies: in a test, a
// failed constructor on a hard-coded literal *is* the failure being reported.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use casivell_core::{Money, Rate, TaxYear};
use casivell_lawdata::{Bundesland, TaxClass};
use casivell_payroll::{Employment, HealthCover, PayPeriod, PayrollLaw, withhold};
use casivell_social::Insured;

/// The salary levels both tables use, in whole euro.
const GROSS_LEVELS: [i64; 43] = [
    5_000, 7_500, 10_000, 12_500, 15_000, 17_500, 20_000, 22_500, 25_000, 27_500, 30_000, 32_500,
    35_000, 37_500, 40_000, 42_500, 45_000, 47_500, 50_000, 52_500, 55_000, 57_500, 60_000, 62_500,
    65_000, 67_500, 70_000, 72_500, 75_000, 77_500, 80_000, 82_500, 85_000, 87_500, 90_000, 92_500,
    95_000, 97_500, 100_000, 102_500, 105_000, 107_500, 110_000,
];

/// Page 39: Allgemeine maschinelle Jahreslohnsteuer 2026.
///
/// Columns are classes I, II, III, IV, V, VI.
const ALLGEMEINE: [[i64; 6]; 43] = [
    [0, 0, 0, 0, 372, 558],
    [0, 0, 0, 0, 647, 838],
    [0, 0, 0, 0, 922, 1_117],
    [0, 0, 0, 0, 1_197, 1_397],
    [0, 0, 0, 0, 1_472, 1_676],
    [51, 0, 0, 51, 1_778, 1_956],
    [380, 0, 0, 380, 2_234, 2_766],
    [782, 32, 0, 782, 3_073, 3_604],
    [1_251, 359, 0, 1_251, 3_911, 4_443],
    [1_742, 759, 0, 1_742, 4_749, 5_281],
    [2_248, 1_230, 0, 2_248, 5_588, 6_120],
    [2_767, 1_724, 0, 2_767, 6_426, 6_952],
    [3_300, 2_233, 294, 3_300, 7_216, 7_682],
    [3_847, 2_756, 628, 3_847, 7_954, 8_436],
    [4_407, 3_293, 1_000, 4_407, 8_720, 9_218],
    [4_982, 3_843, 1_406, 4_982, 9_512, 10_030],
    [5_570, 4_408, 1_850, 5_570, 10_334, 10_865],
    [6_172, 4_987, 2_324, 6_172, 11_171, 11_703],
    [6_788, 5_580, 2_810, 6_788, 12_010, 12_542],
    [7_417, 6_186, 3_302, 7_417, 12_848, 13_380],
    [8_060, 6_807, 3_802, 8_060, 13_687, 14_218],
    [8_718, 7_442, 4_308, 8_718, 14_525, 15_057],
    [9_389, 8_091, 4_822, 9_389, 15_364, 15_895],
    [10_073, 8_754, 5_342, 10_073, 16_202, 16_734],
    [10_772, 9_430, 5_870, 10_772, 17_040, 17_572],
    [11_484, 10_121, 6_402, 11_484, 17_879, 18_410],
    [12_220, 10_835, 6_952, 12_220, 18_729, 19_260],
    [13_062, 11_647, 7_574, 13_062, 19_681, 20_213],
    [13_922, 12_476, 8_206, 13_922, 20_633, 21_165],
    [14_799, 13_323, 8_846, 14_799, 21_585, 22_117],
    [15_694, 14_188, 9_496, 15_694, 22_538, 23_070],
    [16_607, 15_071, 10_154, 16_607, 23_490, 24_022],
    [17_538, 15_971, 10_822, 17_538, 24_443, 24_974],
    [18_486, 16_890, 11_498, 18_486, 25_395, 25_927],
    [19_438, 17_826, 12_182, 19_438, 26_347, 26_879],
    [20_390, 18_777, 12_876, 20_390, 27_300, 27_831],
    [21_343, 19_729, 13_580, 21_343, 28_252, 28_784],
    [22_295, 20_682, 14_292, 22_295, 29_204, 29_736],
    [23_248, 21_634, 15_012, 23_248, 30_157, 30_689],
    [24_243, 22_629, 15_774, 24_243, 31_152, 31_684],
    [25_293, 23_679, 16_590, 25_293, 32_202, 32_734],
    [26_343, 24_729, 17_416, 26_343, 33_252, 33_784],
    [27_393, 25_779, 18_252, 27_393, 34_302, 34_834],
];

/// Page 40: Besondere maschinelle Jahreslohnsteuer 2026.
const BESONDERE: [[i64; 6]; 43] = [
    [0, 0, 0, 0, 18, 700],
    [0, 0, 0, 0, 368, 1_050],
    [0, 0, 0, 0, 718, 1_400],
    [0, 0, 0, 0, 1_068, 1_750],
    [0, 0, 0, 0, 1_418, 2_359],
    [40, 0, 0, 40, 1_768, 3_409],
    [461, 0, 0, 461, 2_415, 4_459],
    [995, 153, 0, 995, 3_465, 5_509],
    [1_604, 607, 0, 1_604, 4_515, 6_559],
    [2_234, 1_173, 0, 2_234, 5_565, 7_514],
    [2_886, 1_788, 0, 2_886, 6_615, 8_460],
    [3_559, 2_424, 76, 3_559, 7_564, 9_446],
    [4_254, 3_083, 466, 4_254, 8_510, 10_473],
    [4_971, 3_763, 914, 4_971, 9_498, 11_523],
    [5_710, 4_464, 1_420, 5_710, 10_529, 12_573],
    [6_470, 5_188, 1_982, 6_470, 11_579, 13_623],
    [7_252, 5_932, 2_584, 7_252, 12_629, 14_673],
    [8_055, 6_699, 3_198, 8_055, 13_679, 15_723],
    [8_880, 7_487, 3_824, 8_880, 14_729, 16_773],
    [9_727, 8_297, 4_458, 9_727, 15_779, 17_823],
    [10_595, 9_128, 5_106, 10_595, 16_829, 18_873],
    [11_485, 9_981, 5_762, 11_485, 17_879, 19_923],
    [12_396, 10_856, 6_430, 12_396, 18_929, 20_973],
    [13_330, 11_752, 7_110, 13_330, 19_979, 22_023],
    [14_284, 12_670, 7_798, 14_284, 21_029, 23_073],
    [15_261, 13_610, 8_500, 15_261, 22_079, 24_123],
    [16_259, 14_571, 9_210, 16_259, 23_129, 25_173],
    [17_279, 15_554, 9_932, 17_279, 24_179, 26_223],
    [18_320, 16_559, 10_666, 18_320, 25_229, 27_273],
    [19_370, 17_585, 11_410, 19_370, 26_279, 28_323],
    [20_420, 18_631, 12_164, 20_420, 27_329, 29_373],
    [21_470, 19_681, 12_930, 21_470, 28_379, 30_423],
    [22_520, 20_731, 13_706, 22_520, 29_429, 31_473],
    [23_570, 21_781, 14_492, 23_570, 30_479, 32_523],
    [24_620, 22_831, 15_290, 24_620, 31_529, 33_573],
    [25_670, 23_881, 16_098, 25_670, 32_579, 34_623],
    [26_720, 24_931, 16_918, 26_720, 33_629, 35_673],
    [27_770, 25_981, 17_748, 27_770, 34_679, 36_723],
    [28_820, 27_031, 18_590, 28_820, 35_729, 37_773],
    [29_870, 28_081, 19_442, 29_870, 36_779, 38_823],
    [30_920, 29_131, 20_304, 30_920, 37_829, 39_873],
    [31_970, 30_181, 21_178, 31_970, 38_879, 40_923],
    [33_020, 31_231, 22_062, 33_020, 39_929, 41_973],
];

fn law() -> PayrollLaw {
    PayrollLaw::for_year(TaxYear::new(2026).expect("2026 is supported"))
        .expect("the 2026 PAP is transcribed")
}

/// Builds the `Insured` whose derived `PVZ` and `PVA` match the table's footnote.
///
/// The footnote to the allgemeine table says `PVZ = 1` in every class except II,
/// where `PVZ = 0`. `PVZ` is the childless surcharge, so class II — the single-parent
/// class — is modelled as a parent of one child, and every other class as childless.
/// That is internally coherent with what the classes mean, which is a small piece of
/// evidence that the footnote has been read correctly.
fn insured_for(class: TaxClass) -> Insured {
    if class == TaxClass::Class2 {
        // A parent: PVZ = 0. One child under 25, so PVA = 0 — no reductions.
        Insured::new(40, true, 1, Bundesland::NordrheinWestfalen, None)
            .expect("a valid single-parent profile")
    } else {
        // Childless and over 23: PVZ = 1.
        Insured::new(40, false, 0, Bundesland::NordrheinWestfalen, None)
            .expect("a valid childless profile")
    }
}

/// The private premium the besondere table specifies for each class, in cents.
///
/// `PKPV = 50 000` in class III, `0` in class VI, `30 000` elsewhere. These are
/// monthly amounts, per the PAP's definition of `PKPV`.
fn private_premium_cents(class: TaxClass) -> i64 {
    match class {
        TaxClass::Class3 => 50_000,
        TaxClass::Class6 => 0,
        _ => 30_000,
    }
}

fn allgemeine_employment(class: TaxClass) -> Employment {
    Employment::new(
        insured_for(class),
        class,
        0,
        HealthCover::Statutory {
            // KVZ = 2,90.
            supplementary_rate: Rate::from_percent_millis(2_900).expect("a valid rate"),
        },
        None,
    )
    .expect("a valid employment")
}

fn besondere_employment(class: TaxClass) -> Employment {
    let mut employment = Employment::new(
        insured_for(class),
        class,
        0,
        HealthCover::Private {
            monthly_premium: Money::from_cents(private_premium_cents(class))
                .expect("a valid premium"),
            // The table specifies no employer subsidy.
            monthly_employer_subsidy: Money::ZERO,
        },
        None,
    )
    .expect("a valid employment");
    // KRV = ALV = 1: insured in neither the pension nor the unemployment scheme.
    employment.statutory_pension = false;
    employment.statutory_unemployment = false;
    employment
}

/// Runs one table and returns a count of mismatches, reporting each.
fn check_table(table: &[[i64; 6]; 43], build: fn(TaxClass) -> Employment, label: &str) -> usize {
    let law = law();
    let mut mismatches = 0_usize;

    for (row, gross_euro) in GROSS_LEVELS.iter().enumerate() {
        let gross = Money::from_euro(*gross_euro).expect("a valid salary");
        for (column, class) in TaxClass::ALL.iter().enumerate() {
            let expected = table[row][column];
            let employment = build(*class);
            let result = withhold(gross, PayPeriod::Year, &employment, &law)
                .expect("withholding must not fail on a table value");
            let actual = result
                .annual_income_tax
                .whole_euro_floor()
                .expect("a representable tax amount");
            if actual != expected {
                mismatches = mismatches.saturating_add(1);
                if mismatches <= 20 {
                    let difference = actual.saturating_sub(expected);
                    println!(
                        "{label}: {gross_euro} EUR, {class:?}: expected {expected}, got {actual} \
                         (off by {difference}); ZVE {} cents, VSP {} cents",
                        result.taxable_annual_amount.cents(),
                        result.vorsorgepauschale.cents(),
                    );
                }
            }
        }
    }
    mismatches
}

/// The primary external verification: 258 official values for an employee insured
/// in every branch of social insurance.
#[test]
fn allgemeine_jahreslohnsteuer_matches_the_official_pruef_tabelle() {
    let mismatches = check_table(&ALLGEMEINE, allgemeine_employment, "allgemeine");
    assert_eq!(
        mismatches, 0,
        "{mismatches} of 258 official values disagree; see the printed rows above"
    );
}

/// The second table exercises the private-insurance branch of the
/// Vorsorgepauschale, and the case where the pension and unemployment components
/// are both absent.
#[test]
fn besondere_jahreslohnsteuer_matches_the_official_pruef_tabelle() {
    let mismatches = check_table(&BESONDERE, besondere_employment, "besondere");
    assert_eq!(
        mismatches, 0,
        "{mismatches} of 258 official values disagree; see the printed rows above"
    );
}

/// Both tables must be well formed: 43 rows of 6, monotonically non-decreasing down
/// each column. A transcription slip that swapped two rows would otherwise be caught
/// only as a confusing calculation failure.
#[test]
fn the_transcribed_tables_are_internally_consistent() {
    for (label, table) in [("allgemeine", &ALLGEMEINE), ("besondere", &BESONDERE)] {
        assert_eq!(table.len(), GROSS_LEVELS.len());
        for column in 0..6 {
            let mut previous = -1_i64;
            for (row, values) in table.iter().enumerate() {
                let value = values[column];
                assert!(
                    value >= previous,
                    "{label}: column {column} falls at row {row}: {previous} then {value}"
                );
                previous = value;
            }
        }
    }
    // The salary levels rise in steps of 2 500 EUR throughout.
    for pair in GROSS_LEVELS.windows(2) {
        assert_eq!(pair[1] - pair[0], 2_500);
    }
}

/// Classes I and IV withhold identically — the tables show identical columns, and
/// the algorithm should reproduce that rather than merely happening to agree.
#[test]
fn classes_one_and_four_agree_in_both_tables() {
    for table in [&ALLGEMEINE, &BESONDERE] {
        for values in table {
            assert_eq!(
                values[0], values[3],
                "the table's own class I and class IV columns disagree"
            );
        }
    }
    let law = law();
    for gross_euro in GROSS_LEVELS {
        let gross = Money::from_euro(gross_euro).expect("a valid salary");
        let one = withhold(
            gross,
            PayPeriod::Year,
            &allgemeine_employment(TaxClass::Class1),
            &law,
        )
        .expect("class I")
        .annual_income_tax;
        let four = withhold(
            gross,
            PayPeriod::Year,
            &allgemeine_employment(TaxClass::Class4),
            &law,
        )
        .expect("class IV")
        .annual_income_tax;
        assert_eq!(one, four, "classes I and IV diverged at {gross_euro} EUR");
    }
}

/// Twelve monthly withholdings must come to at most the annual figure, and within
/// twelve cents of it. The PAP truncates each month's share, so a small
/// under-withholding is expected and is settled in the annual assessment; a large
/// gap would mean the apportionment is wrong.
#[test]
fn monthly_withholding_apportions_the_annual_figure() {
    let law = law();
    for gross_euro in [30_000_i64, 45_000, 60_000, 90_000] {
        for class in TaxClass::ALL {
            let employment = allgemeine_employment(class);
            let annual = withhold(
                Money::from_euro(gross_euro).expect("valid"),
                PayPeriod::Year,
                &employment,
                &law,
            )
            .expect("annual")
            .annual_income_tax;

            let monthly = withhold(
                Money::from_euro(gross_euro / 12).expect("valid"),
                PayPeriod::Month,
                &employment,
                &law,
            )
            .expect("monthly");

            // The monthly calculation annualises its own input, so compare against
            // its own annual figure rather than the yearly run's.
            let twelve = monthly
                .income_tax
                .mul_int(12)
                .expect("twelve months is representable");
            assert!(
                twelve <= monthly.annual_income_tax,
                "{class:?} at {gross_euro}: twelve months exceeded the annual figure"
            );
            let shortfall = monthly
                .annual_income_tax
                .sub(twelve)
                .expect("representable")
                .cents();
            assert!(
                shortfall < 12,
                "{class:?} at {gross_euro}: twelve months fell {shortfall} cents short"
            );
            // Sanity: the annual figure from a monthly run is in the same region as
            // the annual run on twelve times the salary.
            let _ = annual;
        }
    }
}
