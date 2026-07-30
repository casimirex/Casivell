# Casivell Coding Standard

Casivell's calculation engine follows the *JPL Institutional Coding Standard for
the C Programming Language* — Gerard Holzmann's "Power of Ten" — adapted to Rust.

The rules exist because JPL writes software that cannot be patched after launch.
Casivell's constraint is different but rhymes: a household planning a house
purchase or a career break around our output will not audit our arithmetic, and a
wrong answer that looks confident is worse than no answer. The rules are also
mostly about *reviewability*: a reader with an hour should be able to convince
themselves a calculation is right.

**Each rule below states its enforcement mechanism.** A rule that is not
mechanically enforced is a preference, and preferences decay. Where a rule cannot
be enforced automatically, it says so and names what a reviewer must check.

Run everything CI runs with:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --target wasm32-unknown-unknown --release
```

---

## R1 — Simple control flow. No recursion.

Recursion makes stack depth a function of input, which makes it unbounded.

**Enforced:** `clippy::cognitive_complexity` (threshold 15) and
`clippy::too_many_lines` (60). Recursion is checked in review.

**In practice:** `income_tax` handles the Splittingverfahren by calling the shared
`tariff_at` helper twice rather than calling itself with a halved income. The
iterative version is also simply clearer about what § 32a Abs. 5 does.

---

## R2 — All loops have a fixed upper bound. All quantities have a provable range.

A reader must be able to prove a loop terminates and an arithmetic expression fits.

**Enforced:** `clippy::arithmetic_side_effects` is **denied**, so every `+`, `-`,
`*`, `/` and `%` on primitives must be written as `checked_*` or `saturating_*`.
Domain bounds are constants: `Money::MAX_ABS_CENTS`, `Rate::MAX_ABS_PPM`,
`TaxYear::MIN`/`MAX`.

**In practice:** `progression_tax_cents` carries its overflow proof in its doc
comment, deriving a bound of 2.1 × 10¹⁵ against `i64::MAX` of 9.22 × 10¹⁸ — three
orders of margin. It *also* uses checked arithmetic. That is not redundant:

> The proof tells a reader the code is right. The check limits the damage when the
> reader is wrong.

The bounded domain is what makes the proof possible at all. `Money` rejects amounts
beyond ±10 billion euro on construction, so downstream code may assume it.

---

## R3 — No dynamic memory allocation after initialisation.

Allocation introduces failure modes and latency that are hard to bound.

**Enforced structurally:** every engine crate is `#![no_std]`. There is no
allocator, so the engine *cannot* heap-allocate. This is the strongest form of
enforcement available — not a lint that can be silenced but an absence of
capability.

**Consequence:** error types are `Copy` and carry no strings. `MoneyError` is a
small enum with numeric payloads, returnable from a hot loop for free.

---

## R4 — No function longer than one printed page (~60 lines).

**Enforced:** `clippy::too_many_lines` at 60 in `clippy.toml`.

---

## R5 — Minimum two assertions per function.

Holzmann's original rule. Rust reframes it: a `Result` at a type boundary is
strictly better than a runtime assertion, because it is checked at compile time and
cannot be disabled in release.

**Enforced:** `debug-assertions` and `overflow-checks` are on in the `test` profile.
Preconditions are encoded in constructors — `Money::from_cents` range-checks,
`Rate::from_ppm` rejects implausible rates, `TaxYear::new` refuses years we have no
data for.

**In practice:** `Rate`'s plausibility band exists for exactly this. Its job is not
to reject rates nobody would use; it is to catch a **unit mix-up** — a percent
figure handed to the ppm constructor. `from_percent(14_600)` is rejected, because
14 600 % is not a rate anyone meant.

The same idea appears in the data tables. `no_amount_or_rate_in_a_table_is_accidentally_zero`
exists because the `const` helpers fall back to zero when a literal is out of
domain (a `const fn` cannot `unwrap`). That fallback is only safe *because* a test
proves it was never taken.

---

## R6 — Declare data at the smallest possible scope.

**Enforced:** `unreachable_pub` warns. `#![no_std]` and the absence of any global
mutable state make this largely automatic — every calculation function is pure,
taking its parameters explicitly.

**In practice:** this is why calculation functions take a `&IncomeTaxTariff`
argument rather than reading a global. It makes the year an explicit input, which
in turn makes it impossible to compute 2026 tax with 2025 parameters by accident.

---

## R7 — Check the return value of every non-void function. Check every parameter.

**Enforced, and this is the strictest rule here:**

```toml
unwrap_used       = "deny"
expect_used       = "deny"
panic             = "deny"
indexing_slicing  = "deny"
todo              = "deny"
unimplemented     = "deny"
```

`Money` and `Rate` deliberately **do not implement `Add`, `Sub` or `Mul`**. There
are only named methods returning `Result`. The call sites are more verbose, and
that is the point: the easiest way to ensure every return value is checked is to
leave no unchecked alternative.

`no_arithmetic_panics_at_any_magnitude` sweeps the extremes of `i64` through every
operation and asserts none panics. That is what lets the release profile set
`panic = "abort"` and still be trusted not to abort on user input.

**Test code is exempt**, via one `#![cfg_attr(test, allow(...))]` per crate. In a
test, a failed constructor on a hard-coded literal *is* the failure being reported,
and threading `Result` through assertions buries the property under plumbing. The
exemption is `cfg(test)`, so nothing shipped is covered by it.

Integration tests under `tests/` are separate crates, so the library root's
`cfg_attr` does not reach them. Each needs its own `#![allow(...)]` header with the
same reasoning stated.

---

## R8 — Limited use of the preprocessor.

Rust has no preprocessor. The analogue is macros.

**Enforced by convention:** the engine defines no `macro_rules!`. `cfg` is used
only for `cfg(test)`.

---

## R9 — Restrict pointer use. No function pointers.

**Enforced:** `unsafe_code = "forbid"` at workspace level and repeated as
`#![forbid(unsafe_code)]` in each crate root. `forbid` cannot be overridden by an
inner `allow`. There are no raw pointers, and safe references cannot dangle.

---

## R10 — Compile with all warnings enabled. Zero warnings. Static analysis clean.

**Enforced:** CI runs `clippy --all-targets -- -D warnings` with
`clippy::pedantic` on. The build must be warning-free.

### Deliberate relaxations

Two `pedantic` lints are wrong for this codebase and are relaxed **in
`Cargo.toml` with a stated reason**, not silenced at each site:

- `many_single_char_names` — in the rounding helpers, `n`, `d`, `q`, `r` are
  numerator, denominator, quotient, remainder. Those are the names the mathematics
  uses; `numerator`/`denominator` would make a three-line division proof harder to
  check against the algebra it implements.
- `similar_names` — the domain is full of genuine near-homographs that must not be
  renamed for a linter: `employee`/`employer`, `Freibetrag`/`Freigrenze`,
  `Beitragssatz`/`Beitragssatzpunkt`. Renaming either half would make the code
  disagree with the statute it transcribes.

### Prefer `expect` over `allow`

For a *local* exception, use `#[expect(lint, reason = "…")]`. Unlike `allow`, an
`expect` whose lint stops firing becomes a build error, so stale suppressions
delete themselves.

This is not theoretical. While writing `Money::from_euro_cents` I added an
`expect(clippy::cast_lossless)` for a `u8 as i64` widening, reasoning that
`i64::from` is not const-callable. The build then failed with *"this lint
expectation is unfulfilled"* — clippy does not fire that lint in a `const` context.
The suppression was unnecessary and got deleted. An `allow` would have sat there
misleading readers indefinitely.

---

## Domain rules specific to Casivell

These carry no JPL analogue but are as binding.

### D1 — No floating point anywhere in the calculation path.

**Enforced:** `clippy::float_arithmetic = "deny"`.

Money is integer cents; rates are integer parts-per-million. Rationale in
`casivell-core`'s crate docs and `docs/ROADMAP_ERRATA.md` §F1. Every statutory rate
Germany actually uses is exactly representable in ppm.

### D2 — No statutory constant outside `casivell-lawdata`.

If `12_348` appears in a calculation crate, that is a defect. Calculation functions
take parameter structs. A reviewer checking 2026 tax reads one table against one
Gesetzestext, instead of grepping a simulation kernel for magic numbers.

**Enforced by review** plus the tests in `casivell-lawdata` that assert every
figure carries a citation to a primary source.

### D3 — Every statutory figure carries a `Provenance`.

Provision, primary-source URL, verification date, and `Enacted`/`Draft`/`Projected`
status. There is no constructor that omits it.

`DataStatus::weakest` propagates the least certain input's status to a composite
result, so a projection cannot present an extrapolation with the same authority as
enacted law.

**Transcription and derivation live in different crates.** `casivell-lawdata` holds
figures read off a primary source; `casivell-projection` derives figures for years that
have none. The boundary is load-bearing, not tidy: it is what lets a reviewer check the
whole of `casivell-lawdata` against Gesetzestexte, with nothing computed mixed in.

**Representable is not the same as verified.** `TaxYear` spans a century so a projection
can name the years it needs; `TaxYear::has_verified_data` reports whether a statute
exists. The guard sits on the *data lookup* — `LawYear::for_year` refuses any year it
cannot cite — never on year construction. Conflating the two is what made projection
impossible in the first place.

**A projection must be impossible to obtain by accident.** Nothing past the last enacted
year is reachable without passing `Assumptions`, and everything so obtained is marked
`Projected` and says "NOT enacted law" in its `legal_basis` — because a reader may see
that string without the `DataStatus` beside it.

### D4 — Name the rounding direction.

The engine never applies `/` to a monetary quantity. It calls `div_floor`,
`div_ceil`, `div_trunc` or `div_round_half_up` and cites the provision requiring it.
They disagree for negative operands, and a bare `/` does not say which was meant.

`div_ceil` exists because the BMF Programmablaufplan needs it: two Vorsorgepauschale
boxes are annotated `Euro↑` while every other rounding in the same document goes
down. Having all four directions available *by name* is what made that difference
expressible rather than something to approximate.

### D5 — Prefer property tests over point values; state the precondition.

Point values check that a formula was transcribed. Properties check that it is
*coherent* — and catch errors in cases nobody thought to tabulate.

The most valuable test in the engine is `the_zones_join_continuously`. It verifies
the tariff data using the statute's **own internal consistency**: § 32a's
coefficients are chosen so the four zone formulas meet, so a discontinuity means a
mistranscription. It validates the data without reference to any figure copied from
the same place the data came from.

Others in this class: `tax_is_monotonically_non_decreasing_in_income`,
`the_marginal_rate_never_leaves_the_statutory_band`,
`the_surcharge_is_monotonic_in_the_tax`, and
`the_pension_value_is_continuous_across_the_year_boundary`.

**State and assert your preconditions.** `the_splitting_benefit_saturates_at_the_subtrahend`
initially failed because I picked a joint income of 300 000 €, where the individual
assessment is in zone 5 while the half is in zone 4 — the affine identity needs
both in the *same* zone. The fixed test asserts the zone of each, so it fails
loudly rather than silently becoming vacuous if the Eckwerte move.

### D6 — Cross-check against an independent implementation, and against the authority.

Two distinct things, both required.

**The authority's own reference values, where they exist.** The BMF publishes
Prüftabellen with the Programmablaufplan: 516 values of annual Lohnsteuer across
43 salary levels and six tax classes. `casivell-payroll` is checked against all of
them. This is the strongest verification available — not "consistent with our reading
of the statute" but agreeing with the tax authority's arithmetic.

It also earns its keep. The PAP annotates each rounding step with an arrow, and the
directions are **not uniform**: two Vorsorgepauschale boxes round *up* while every
other `Euro` annotation rounds down. Implementing them all as truncation passed the
*besondere* table — where the Vorsorgepauschale lands on a whole euro anyway, hiding
the direction — and failed 56 of the 258 *allgemeine* values by one or two euro.
Only having both tables located it. Reference values must be transcribed by hand from
the primary document, and never regenerated from our own output.

**Derivation beats transcription where a rule exists.** § 32a's coefficients are not
free parameters — the marginal rate is pinned at each zone join, which determines all of
them from the Eckwerte. Deriving them makes a *projected* tariff credible, and the
derivation is validated by reproducing all eight published coefficients for both enacted
years exactly.

Precision decides whether that works. The dependent constants must be derived from the
**unrounded** coefficients: rounding `c` from `173.1024` to the statute's `173.10` first
shifts the 42 % subtrahend by seven cents and the reproduction fails. Finding the last
three cents also located a real boundary — the 42 %/45 % lines cross at the *top of
zone 4*, one euro below where zone 5 begins.

**An independent implementation, for algebra the authority does not tabulate.**

`docs/reference/generate_tariff_reference.py` evaluates § 32a with
arbitrary-precision decimals, transcribing each zone as printed. The engine clears
the denominators and evaluates one integer quotient. They share only the statutory
coefficients, so agreement is evidence the algebra was cleared correctly — which no
amount of internal consistency checking could establish.

That script must **never** be generated from the engine's output. It would make the
cross-check a tautology.

### D7 — Report known inexactness in the type system.

Where a calculation is knowably incomplete, the result says so.
`ChurchTaxResult::base_is_exact` is `false` when children are present, because
§ 51a Abs. 2 EStG is not yet implemented and the figure is therefore overstated.

A figure that is knowably wrong must not be returned as though it were right. This
is preferable both to silently returning it and to refusing to compute: the user
gets a number *and* the knowledge that it is provisional.

Where a rounding rule is genuinely uncertain — as with the Soli, where SolzG does
not prescribe a direction — the doc comment says so, bounds the error, and marks it
an open item. Uncertainty gets recorded, not resolved by assumption.

---

## Adding a new statutory year

1. Add the parameter tables to `casivell-lawdata` with full `Provenance`, checked
   against a **primary** source. Never a tax blog: `docs/ROADMAP_ERRATA.md` records
   a case where current secondary sources pair a 2023 rate floor with a 2026 base
   rate.
2. Widen `TaxYear::MAX`. The `every_supported_year_*` tests will fail until every
   table exists — the declared range and the data cannot drift apart.
3. Add the year to the `every_*()` helpers in each test module.
4. Regenerate the cross-check with `generate_tariff_reference.py`, transcribing the
   new statute **by hand** into it.
5. Confirm `the_zones_join_continuously` passes. If it does not, a coefficient is
   mistranscribed.
