# Casivell

**A local-first financial simulator for German households.**

Casivell simulates decades of tax, social insurance and pension consequences under
real German law, so a household can answer the questions that actually matter:
*should we buy or keep renting? what does part-time really cost over a lifetime?
can we afford a year off?*

Your financial data stays on your device. There is no network code path below the
UI layer.

> **Status: early.** The calculation engine is built and tested, and there is a
> working CLI. There is no graphical interface yet, and no multi-year projection —
> see [ROADMAP.md](ROADMAP.md) for what exists and what comes next.

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
├── casivell-social/    Social insurance contributions, pension entitlement
├── casivell-payroll/   Lohnsteuer (BMF Programmablaufplan), gross-to-net
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

---

## Build

Requires a Rust toolchain; the channel and targets are pinned in
`rust-toolchain.toml`.

```sh
cargo test --workspace                                        # 238 tests
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
- Where a result is knowably incomplete, the type says so:
  `ChurchTaxResult::base_is_exact` is `false` for households with children, because
  § 51a Abs. 2 EStG is not yet implemented.

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
