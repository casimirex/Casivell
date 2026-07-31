# Casivell

**A local-first financial simulator for German households.**

Casivell simulates decades of tax, social insurance and pension consequences under
real German law, so a household can answer the questions that actually matter:
*should we buy or keep renting? what does part-time really cost over a lifetime?
can we afford a year off?*

Your financial data stays on your device. There is no network code path below the
UI layer.

> **Status: early.** The engine is built and tested, statutory parameters can be projected
> past the last enacted year, decades-long household projections run with life events, and
> there is a working CLI. There is no graphical interface and no persistence yet — see
> [ROADMAP.md](ROADMAP.md) for what exists and what comes next.

---

## Why this exists

Existing tools either send your financial life to a server, or approximate German
law badly, or both. Casivell is built on three commitments:

**Correct, and demonstrably so.** Every statutory figure cites its provision, a
primary source, and the date a human verified it. Every calculation is exact integer
arithmetic — there is no floating point anywhere in the engine, because a cent of
drift compounded over 480 months is a wrong answer.

**Local-first.** Not a privacy policy. An architecture: the engine is `#![no_std]`
and has no way to open a socket.

**Explainable.** Every figure traces back to the rule that produced it. A number you
cannot check is a number asking for trust it has not earned.

---

## Engine layout

```
crates/
├── casivell-core/      Money (integer cents), Rate (integer ppm), named rounding
├── casivell-lawdata/   Year-keyed statutory tables, every figure cited
├── casivell-tax/       § 32a EStG tariff, Solidaritätszuschlag, church tax
├── casivell-income/    § 2 EStG: gross → taxable income, the assessment, § 32d capital
├── casivell-social/    Social insurance contributions, pension entitlement
├── casivell-benefits/  Elterngeld (BEEG) — above payroll, because § 2e uses the PAP
├── casivell-payroll/   Lohnsteuer (BMF Programmablaufplan), gross-to-net
├── casivell-projection/ Statutory parameters past the last enacted year
├── casivell-sim/       Month-by-month household projection, streaming
└── casivell-cli/       The `casivell` command — the only crate that uses std
```

The dependency direction is the design: `core` knows nothing of German law;
`lawdata` holds law but performs no calculation; calculation crates hold no
statutory constants. Each layer can be reviewed on its own.

Every engine crate is `#![no_std]` and `#![forbid(unsafe_code)]`. `casivell-cli` is
the only crate that uses `std`, and that boundary is what keeps the guarantee that
the calculation layer cannot allocate or open a socket. Zero third-party
dependencies, throughout.

---

## Try it

```sh
cargo run -p casivell-cli -- --gross 4500 --class 1
```

```
Casivell — Lohnabrechnung
  2026 · Steuerklasse I · NW · monthly

  Bruttoentgelt                                     4.500,00 €

  Steuern
    Lohnsteuer                                       -650,16 €
    Solidaritätszuschlag                                0,00 €

  Sozialversicherung (Arbeitnehmeranteil)
    Rentenversicherung          9,30 %               -418,50 €
    Arbeitslosenversicherung    1,30 %                -58,50 €
    Krankenversicherung         8,75 %               -393,75 €
    Pflegeversicherung          2,40 %               -108,00 €
    Summe                                            -978,75 €

  ────────────────────────────────────────────────────────────
  Nettoentgelt                                      2.871,09 €
  (63,80 % of gross)

  Wie die Lohnsteuer ermittelt wurde (§ 39b EStG, BMF-PAP 2026)
    Jahresarbeitslohn (ZRE4)                        54.000,00 €
    − Tabellenfreibeträge (ZTABFB)                  -1.266,00 €
    − Vorsorgepauschale (VSP)                      -10.881,00 €
    = zu versteuernder Betrag (ZVE)                 41.853,00 €
    Jahreslohnsteuer (LSTJAHR)                       7.802,00 €
    ÷ 12 = Lohnsteuer im Monat                        650,16 €
```

Every figure is traceable to the rule that produced it, and the report states its
own assumptions and limits. `--help` lists the options.

A forty-year projection has to name years no legislature has legislated for, so the
statutory parameters can be projected — from explicit assumptions, and labelled:

```sh
cargo run -p casivell-cli -- law --year 2060
```

```
Casivell — Rechengrößen 2060
  ⚠  PROJECTED — NOT ENACTED LAW. No statute exists for 2060.
     Extrapolated from 2026 at 2,00 % price inflation and 2,80 % wage growth.
     Rates are held constant; the 45 % threshold is not indexed.

  Einkommensteuertarif (§ 32a EStG)
    Grundfreibetrag                          24.209,00 €
    Beginn 42 % (Spitzensteuersatz)         137.013,00 €
    Beginn 45 % (Reichensteuer)             277.826,00 €
```

That last pair is the model earning its keep: the 45 % threshold has not been indexed
since 2007, so a projection shows it being overtaken. Push far enough and the tariff
stops being well formed — which Casivell refuses rather than papering over.

And a household can be projected forward, month by month, for decades:

```sh
cargo run -p casivell-cli -- project --gross 4500 --class 1 --expenses 2500 \
    --pay-growth 2,8 --return 5,0 --real
```

```
  Year   Gross/mo      Net/mo   Saved/mo        Wealth    Points   Pension/mo
  ──────────────────────────────────────────────────────────────────────────
  2026   4.500,00    2.871,09     371,09      4.556,58      1,04        44,20
  ┈┈┈┈┈┈┈┈  enacted law ends here; rows below are projected
  2027   4.535,29    2.889,65     389,65      9.480,25      2,08        89,10
  ...
  2065   6.102,90    3.677,94   1.177,95    637.114,68     41,58     2.397,65
```

Every month runs the same verified payroll code that produces a payslip, against the
statutory parameters for that year. The row where enacted law ends is marked rather than
footnoted.

Life events change the course of it — `--part-time 3:8:60` for five years at three days a
week, `--break 5:6` for a year off, `--raise 15:8000` for a promotion:

```sh
cargo run -p casivell-cli -- project --gross 4500 --class 1 --expenses 2500 \
    --part-time 3:8:60 --pay-growth 2,8
```

Five years at 60 % pushes this household into deficit for six years, and its pension record
accrues 0,62 Entgeltpunkte a year instead of 1,04 — a shortfall that does *not* recover when
the hours do, because Entgeltpunkte are a ratio to the national average wage. That is the
Teilzeitfalle, and showing it is the point of the exercise.

---

## Build

Requires a Rust toolchain; the channel and targets are pinned in
`rust-toolchain.toml`.

```sh
cargo test --workspace                                        # 567 tests
cargo clippy --workspace --all-targets -- -D warnings         # clean at `pedantic`
cargo build --workspace --target wasm32-unknown-unknown --release
python3 scripts/check_no_statutory_literals.py                # rule D2
python3 docs/reference/generate_tariff_reference.py           # cross-check values
```

Every one of these is a CI step, run with the identical command. A CI that cannot
be reproduced on a laptop teaches people to push and pray.

---

## Engineering standard

The engine follows the JPL Institutional Coding Standard — Holzmann's "Power of
Ten" — adapted to Rust. In summary:

| Constraint | Mechanism |
|---|---|
| No heap allocation in the engine | `#![no_std]` — there is no allocator |
| No unsafe code | `#![forbid(unsafe_code)]` |
| No panics in library code | `unwrap`, `expect`, `panic`, indexing all **denied** |
| No unchecked arithmetic | `clippy::arithmetic_side_effects` **denied** |
| No floating point | `clippy::float_arithmetic` **denied** |
| Bounded functions | 60-line limit, complexity limit |
| Zero warnings | `clippy::pedantic` with `-D warnings` in CI |

`Money` and `Rate` deliberately do **not** implement `Add`/`Sub`/`Mul`. Every
operation is a named method returning `Result`. Call sites are more verbose, and
that is the point: the surest way to have every return value checked is to leave no
unchecked alternative.

Full rules, each with its enforcement mechanism, in
**[docs/CODING_STANDARD.md](docs/CODING_STANDARD.md)**.

---

## On correctness

This project was rebuilt because its original specification's statutory constants
were substantially wrong — the 42 % tax threshold was specified as 28 397 € against
an actual 69 879 €, which understated tax on a 30 000 € income by 48 %. Most values
were correct figures for the wrong year, blended across years with nothing to reveal
it.

**[docs/ROADMAP_ERRATA.md](docs/ROADMAP_ERRATA.md)** is the full record. It is kept
in the repository deliberately: a project claiming legal accuracy should show how it
checks itself and what it got wrong.

The mechanisms that came out of it:

- Statutory figures live only in `casivell-lawdata`, each with a `Provenance`
  (provision, primary-source URL, verification date, and an
  `Enacted`/`Draft`/`Projected` status). CI fails if one appears elsewhere.
- `the_zones_join_continuously` validates the tariff data using the statute's *own*
  internal consistency — § 32a's coefficients are chosen so the zone formulas meet,
  so a discontinuity means a mistranscription. It checks the data without reference
  to any figure copied from the same place the data came from.
- An independent decimal implementation in `docs/reference/` cross-checks the
  engine's integer algebra. They share only the statutory coefficients.
- Lohnsteuer is checked against the **516 official values** of the BMF's own
  Prüftabellen — the reference tables German payroll products are validated against.
- Projected tariffs are derived rather than invented: § 32a's coefficients follow from
  its Eckwerte, and the derivation reproduces **all eight published coefficients** for
  both enacted years exactly. Nothing past the last statute can be obtained without
  passing explicit assumptions, and everything so obtained reports itself as projected.
- Where no official reference exists — `casivell-income`, which determines taxable income
  under § 10 — that is **stated in the crate docs**, and the verification is built from what
  is available: constants cross-checked against the Programmablaufplan's own tables, the
  Altersvorsorge cap *derived* and matched to its published value, an external crossover
  figure for the Günstigerprüfung, and a bounded comparison against the Vorsorgepauschale.
  `Assessment::is_exact` is `false` and says why.
- The strongest of those checks only became possible once the assessment ran inside the
  kernel. Withholding is *designed* to be right for a flat year — that is the premise of
  § 39b — so on a flat year the annual assessment must return almost nothing. It does: the
  two paths land within **96 to 332 cents** of each other on withholding of 2 248 € to
  59 917 €. They share only the tariff; one runs the BMF Programmablaufplan with its
  simplified Vorsorgepauschale, the other § 2 EStG with the real § 10 deduction. Nothing
  makes them agree except both being right.
- The simulation kernel is `#![no_std]` and **streams**: it holds one month at a time and
  hands each to a sink. A forty-year run is `O(1)` in memory, and ten thousand Monte Carlo
  paths cost no more than one. `Vec` appearing outside a test module would mean the design
  had been abandoned.
- Where a result is knowably incomplete, the type says so:
  `ChurchTaxResult::base_is_exact` is `false` for households with children, because
  § 51a Abs. 2 EStG is not yet implemented.

It also caught a bug of exactly the kind it was built for. The § 9a Pauschbetrag is projected
by two modules — the Programmablaufplan's table and the deduction table — and they disagreed
about whether to index it. Withholding and the annual assessment then used different figures
in the same simulated month, and a household's Steuerbescheid drifted about ten euro a year
into a **169 € demand after twenty years that no statute produced**. The two are now held
equal at every horizon out to 44 years, because the error grew with distance and a spot check
near 2026 would have passed.

This machinery has already caught a live error. A test asserting the care-insurance
rate floor at 2.4 % — a figure current secondary sources still publish — failed
against the table's 2.6 %. The table was right: 2.4 % belongs to the 3.4 % base-rate
era that ended in 2024.

---

## Scope

Casivell is an information and simulation tool. It is **not** tax advice
(Steuerberatungsgesetz §§ 1–4), **not** investment advice, and **not** a filing
tool. Those boundaries shape what gets built — see ROADMAP.md §7.

---

## Licence

Engine: Apache-2.0. The arithmetic is free and auditable; that is the part you have
to trust.
