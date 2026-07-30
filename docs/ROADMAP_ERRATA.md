# Roadmap Errata

**Reviewed:** 2026-07-30
**Subject:** roadmap v0.1.0 ("HAUSGELD"), archived unmodified at
[`archive/CASIVELL_ROADMAP_v0.1.0.md`](archive/CASIVELL_ROADMAP_v0.1.0.md)

Every statutory figure in the original specification was checked against primary
sources. This document records what was wrong, what the correct value is, and —
where it matters — what the error would have cost.

The short version: **the specification's central technical claim was "100 % match
with BMF reference cases", and not one of its statutory constants was correct for
the year it claimed.** Most were correct values for an earlier year, blended
across years. That failure mode is the reason `casivell-lawdata` now attaches a
`Provenance` — a citation, a source URL, a verification date and an
enacted/draft/projected status — to every figure, and why no statutory literal is
permitted anywhere else in the engine.

---

## A. Income tax tariff, § 32a Abs. 1 EStG

All nine parameters of the tariff were wrong. The figures below are the version in
force from VZ 2026 (Art. 2 SteFeG v. 23.12.2024), with 2025 shown for context
because several of the specification's values are near-misses of neither year.

| Parameter | As specified | **Correct 2026** | Correct 2025 |
|---|---|---|---|
| Grundfreibetrag | 12 096 | **12 348** | 12 096 |
| Zone 2 upper bound | 17 643 | **17 799** | 17 443 |
| Zone 2 quadratic coefficient | 922,98 | **914,51** | 932,30 |
| Zone 3 quadratic coefficient | 181,19 | **173,10** | 176,64 |
| Zone 3 constant | 1 035 | **1 034,87** | 1 015,13 |
| 42 % zone starts at | 28 397 | **69 879** | 68 481 |
| 42 % subtrahend | 10 397 | **11 135,63** | 10 911,92 |
| 45 % zone starts at | 392 782 | **277 826** | 277 826 |
| 45 % subtrahend | 22 228 | **19 470,38** | 19 246,67 |

Source: [§ 32a EStG](https://www.gesetze-im-internet.de/estg/__32a.html).

### Why this one mattered most

The 42 % threshold was specified as **28 397 €** against an actual **69 879 €** — a
factor of 2.5. That single error puts the entire German middle class into the top
tax bracket. Combined with the wrong coefficients, the resulting error is large and
**changes sign across the income range**, which is worse than a consistent bias
because no single correction factor can compensate for it:

| zvE | As specified | Correct | Error |
|---|---|---|---|
| 15 000 € | 484 € | 435 € | **+49 € (+11.3 %)** |
| 25 000 € | 2 896 € | 2 850 € | +46 € (+1.6 %) |
| **30 000 €** | **2 203 €** | **4 217 €** | **−2 014 € (−47.8 %)** |
| 40 000 € | 6 403 € | 7 209 € | −806 € (−11.2 %) |
| 70 000 € | 19 003 € | 18 264 € | +739 € (+4.0 %) |
| 400 000 € | 157 772 € | 160 529 € | −2 757 € (−1.7 %) |

At a taxable income of 30 000 € — squarely the target market — the specification
understates income tax by **48 %**. A user planning around that figure would
believe they had roughly 2 000 € a year more than they do.

Reproduce with `python3 docs/reference/generate_tariff_reference.py`.

### Solidaritätszuschlag

The specification's Soli code was not a version of the statute at all:

```rust
let solidarity = if tax <= 18_720.0 / 12.0 { 0.0 }
                 else { (tax * 0.055).min(tax * 0.055 - (11_784.0 - tax * 0.75) * 0.055) };
```

`11_784` is the **2024 Grundfreibetrag**, which has no role in the Soli
calculation. `18_720 / 12` divides an annual threshold by twelve while comparing it
against an annual tax figure. And the `min` is degenerate: `min(a, a − k)` is
always `a − k` for positive `k`, so the first branch is unreachable.

The actual mechanism (SolzG 1995) has three regimes:

| Parameter | As specified | **Correct 2026** | Correct 2025 |
|---|---|---|---|
| Rate | 5,5 % ✓ | 5,5 % | 5,5 % |
| Freigrenze, individual | — (wrong construct) | **20 350 €** | 19 950 € |
| Freigrenze, joint | — (absent) | **40 700 €** | 39 900 € |
| Milderungszone cap | — (absent) | **11,9 % of the excess** | 11,9 % |

The Freigrenze is a threshold on **assessed tax**, not on income — 20 350 € of tax
is roughly 75 000 € of taxable income. Between the Freigrenze and about 37 800 € of
tax, the *marginal* rate on the surcharge is 11.9 %, not 5.5 %. A flat-5.5 % model
hides precisely the cliff a planning tool exists to reveal.

Implemented in `casivell-tax::solidarity`.

---

## B. Pension insurance, SGB VI

| Parameter | As specified | **Correct 2026** | Note |
|---|---|---|---|
| Contribution ceiling | 96 600 West / 93 600 East | **101 400 (8 450/mo), nationwide** | The West/East split was **abolished on 1 Jan 2025**. 96 600 € was the 2025 nationwide figure. |
| Durchschnittsentgelt | 43 142 € | **51 944 €** | Off by 20 %. 43 142 € is roughly the 2019 value. |
| Aktueller Rentenwert | 39,32 West / 38,44 East | **42,52 from 1 Jul 2026** (40,79 before) | East/West parity was reached **1 Jul 2023**. There has been no separate East value for three years. |
| Contribution rate | absent | **18,6 %** (9,3 % each side) | |

Sources: [SVBezGrV 2026](https://www.gesetze-im-internet.de/svbezgrv_2026/BJNR1160A0025.html),
[DRV announcement of 5 Mar 2026](https://www.deutsche-rentenversicherung.de/DRV/DE/Ueber-uns-und-Presse/Presse/Meldungen/2026/260305-rentenanpassung-2026).

### A structural error, not just stale numbers

**The Rentenwert changes on 1 July, not 1 January** (§ 65 SGB VI). The
specification's `calculate_monthly_pension(total_points, retirement_year)` takes a
year and returns one value, so it is wrong for six months of every year — and in
2026, when the value rises 4.24 %, wrong by 4.24 % for half the year.

`PensionInsurance` therefore stores `pension_value_jan_to_jun` **and**
`pension_value_jul_to_dec`, with a test asserting that each year's second-half
value equals the next year's first-half value — the continuity that makes the
model coherent.

---

## C. Health and long-term care insurance, SGB V / SGB XI

| Parameter | As specified | **Correct 2026** | Note |
|---|---|---|---|
| GKV ceiling, monthly | 5 512,50 € | **5 812,50 €** | 5 512,50 € was 2025. |
| General GKV rate | 14,6 % ✓ | 14,6 % | Correct. |
| Average Zusatzbeitrag | 1,7 % | **2,9 %** | 1,7 % was 2024. Understates by 1.2 points of gross. |
| Care insurance base rate | 3,4 % | **3,6 %** | 3,4 % was 2024. |
| Childless surcharge | 0,6 % ✓ | 0,6 %, **employee only** | Rate right, incidence wrong — see below. |
| Versicherungspflichtgrenze | absent | **77 400 €/yr** | Needed for any GKV/PKV comparison. |
| Per-child reduction | absent | **0,25 pt for children 2–5** | Floor 2,6 %. |

Sources: [SVBezGrV 2026](https://www.gesetze-im-internet.de/svbezgrv_2026/BJNR1160A0025.html),
[§ 55 SGB XI](https://www.gesetze-im-internet.de/sgb_11/__55.html).

### The incidence error

The specification computed:

```rust
let pv_rate = 0.034 + if age >= 23 && children == 0 { 0.006 } else { 0.0 };
let total_rate = (kv_rate + pv_rate) / 2.0; // Employee pays half
```

Under § 55 Abs. 3 SGB XI the childless surcharge is **borne by the employee
alone**. Halving it understates the employee's burden by 0.3 % of gross pay for
every childless person over 23 — which is most of the young professionals the
product targets. At 60 000 € gross that is 180 € a year, silently.

The specification also missed that **Saxony splits care contributions differently**
(§ 58 Abs. 3 SGB XI: employees there bear 0.5 points more, because Saxony retained
Buß- und Bettag). `CareInsurance` keeps the childless surcharge and the Saxon
surcharge in their own fields, so neither can be swept into a halving.

### A caught trap, worth recording

While writing the test for the five-child rate floor, I asserted **2.4 %** — a
figure that appears in current secondary sources. The test failed against the
table's 2.6 %. The table was right: 2.4 % is the **2023–24** figure, when the base
rate was 3.4 %. Four reductions of 0.25 points off 3.4 % gives 2.4 %; off today's
3.6 % it gives 2.6 %. Secondary sources are still pairing the old floor with the
new base rate.

This is the same error class as the original specification's, caught by the same
mechanism. It is why `the_care_rate_ladder_matches_the_published_table` derives
every rung from the base rate in the table rather than from remembered constants.

---

## D. Elterngeld, BEEG

The specification's sketch returns `net_pre_birth * 0.65` clamped to
[300, 1 800]. Three problems:

1. **The replacement rate is not a constant.** It slides from 100 % to 65 % as
   pre-birth net income rises (§ 2 Abs. 2 BEEG); 65 % applies only above
   about 1 240 €/month.
2. **The income cap is missing.** Since 1 April 2024 there is no entitlement at
   all above 175 000 € of joint taxable income (§ 1 Abs. 8 BEEG).
3. **The Partnerschaftsbonus is described wrongly.** The specification says
   "+10 % each if both take 2–4 months concurrent". It is not a percentage uplift;
   it is 2–4 additional months of ElterngeldPlus. ElterngeldPlus has its own
   bounds (150 €–900 €), which the sketch does not model.

Not yet implemented. Deferred rather than approximated, because a wrong Elterngeld
figure is worse than an absent one: it is the input to a decision that is hard to
reverse.

---

## E. Other statutory items

| Item | Issue |
|---|---|
| **"Hartz IV / Bürgergeld"** | Hartz IV ceased to exist in January 2023. As of 2026 Bürgergeld is itself being reformed toward a "Neue Grundsicherung". A tool whose credibility rests on legal accuracy cannot use a name retired three years ago. |
| **Kindergeld** | Not specified at all despite being a headline feature. **259 €/month** per child in 2026. |
| **Church tax** | Rates (8 %/9 %) correct, but the specification missed that § 51a Abs. 2 EStG recomputes the base with the full Kinderfreibetrag, so church tax is **overstated for families**. Also missed Kappung. Both are now documented on the type and reported via `ChurchTaxResult::base_is_exact` rather than silently wrong. |
| **Tax classes I–VI** | Listed as a P0 feature with no mention that III/V are slated for replacement by the Faktorverfahren. A 40-year projection needs to model that as a legislative risk. |

---

## F. Architectural corrections

These are not data errors; they are decisions that would have made the data errors
unfixable.

### F1. `f64` for money → integer cents

The specification used `f64` throughout while promising *"Gleiche Eingaben,
gleiches Ergebnis, immer"*. Binary floating point cannot represent `0,01`, does not
associate, and may differ across targets where fused multiply-add is available. A
cent of drift compounded over 480 months is a wrong answer; an answer that differs
between a user's phone and their laptop is an unfixable support ticket.

`Money` is now an integer count of cents with no panicking operators, and
`clippy::float_arithmetic` is **denied workspace-wide**. `Rate` is integer
parts-per-million. The test `repeated_addition_never_drifts` sums ten cents a
thousand times and asserts exactly 10 000.

This also matters for correctness, not just reproducibility: § 32a Abs. 1 Satz 2
requires truncation to whole euro at two specific points. Truncating a float that
is `4216.9999999997` instead of `4217` costs a euro.

### F2. Statutory constants hard-coded in calculation functions → cited data tables

The specification embedded `12_096` and `43_142` in function bodies. That is how
figures for four different years ended up in one file with nothing to reveal it.

Statutory figures now live only in `casivell-lawdata`, keyed by year, each with a
`Provenance`. Calculation crates take parameters and contain **no statutory
literals**. Tests assert that every citation names its provision and points at
`gesetze-im-internet.de` — a primary source, never a tax blog.

`DataStatus` marks each set `Enacted`, `Draft`, or `Projected`. A 40-year
projection necessarily runs past the last enacted statute; the specification had no
way to distinguish law from guess and would have rendered an extrapolation about
2059 in the same typeface as § 32a.

### F3. Contradictions to resolve

| Claim | Conflicts with |
|---|---|
| "Bundle size < 500 KB WASM + JS gzipped" | D3 + visx + Framer Motion + Dexie + **ONNX Runtime Web + TinyLlama**. A Q4-quantised 1.1 B model is ~600 MB — **1 200× the budget**. |
| "TinyLlama 1.1B" (tech stack) | "TinyLlama (3B params)" (architecture diagram). The document contradicts itself. |
| "Zero server, zero accounts required" | "React 19 Canary with **Server Actions**"; and a €5/mo tier, which cannot be gated without identity. React 19 is also **stable**, not canary. |
| "Cryptographically provable" privacy | Not provable. You cannot prove a web app does not exfiltrate. |
| 16 weeks solo for all of the above | Realistically 3–4× that. |

On the privacy claim specifically: the honest and *stronger* position is to ship
verifiable technical measures — a CSP with `connect-src 'none'`, subresource
integrity, reproducible builds, and no network code path in the engine at all —
and invite verification. "Audit the CSP header yourself" beats "trust our proof",
because the former is checkable in a browser's devtools in ten seconds.

### F4. Missing: legal and compliance

The specification had no compliance section. In Germany this is not optional:

- **Steuerberatungsgesetz §§ 1–4** restricts giving tax advice. An "AI Financial
  Advisor" and a "Steuererklärung Helper" sit close to that line.
- **Rechtsdienstleistungsgesetz** restricts legal advice.
- **§ 5 DDG** mandates an Impressum; a Datenschutzerklärung is required even for a
  local-first app the moment it is hosted.
- Investment suggestions can touch **WpIG/KWG** licensing.

Local-first architecture is a genuine and large GDPR advantage — it should be
argued explicitly rather than left implicit.

### F5. Missing: engineering fundamentals

Absent from the specification and now scheduled: versioned scenario schema with
migrations; law-version pinning so a scenario saved in 2026 still reproduces its
original numbers in 2031; an explainability trace (`Assessment::zone` and
`SurchargeResult::in_taper_zone` are the first instalment); an explicit rounding
specification; a threat model; and a test strategy against the BMF
**Programmablaufplan** — which is the authoritative payroll algorithm, and is what
"BMF XML Schnittstellen" should have said.

### F6. Two smaller notes

- **`wee_alloc` is unmaintained** and has a known unbounded-memory-growth issue.
  Do not ship it. The default `dlmalloc` is fine; `talc` if measurement justifies it.
- **Rust 1.82** was already old. Pinned via `rust-toolchain.toml` so builds are
  reproducible, which the specification asked for without providing.

---

## G. Two product-level observations

**"HAUSGELD" is a poor name for this product.** *Hausgeld* is a specific German
legal term: the monthly service charge a condominium owner pays their
Wohnungseigentümergemeinschaft. German users searching it want WEG cost
calculators. The name states the wrong category to exactly the audience the
product is for. The work now proceeds as **Casivell**, matching the repository —
an invented name with no statutory collision.

**The "Why this impresses [a named founder at a named competitor]" section should
not live in the repository.** As personal motivation it is entirely reasonable. As
a document in a public repo it reads as building a pitch rather than a product,
names a real person in a commercial context, and would let architecture be driven
by what demos well rather than by what is correct. Keep it — in a private note.

---

## What was already right

Worth stating, since the above is unrelenting:

- **The core product thesis is sound.** A local-first, offline German household
  simulator with law-as-code is genuinely underserved, and privacy-by-architecture
  is a real differentiator rather than a slogan.
- **Rust/WASM is the right choice** for a deterministic calculation kernel.
- **The performance target is achievable.** 10 000 Monte Carlo paths × 480 months
  is 4.8 M month-steps; a lean integer kernel does that in tens of milliseconds
  single-threaded. It needs stating, though, that "WASM parallel processing"
  requires `SharedArrayBuffer`, hence COOP/COEP headers — which are deliverable on
  Cloudflare Pages via `_headers` but break some embedding contexts.
- **Scenario branching as a DAG** is a good idea and a real differentiator.
- **The general GKV rate of 14,6 %, the Soli rate of 5,5 %, the church tax rates,
  and the childless surcharge of 0,6 %** were all correct.

The thesis survives. The arithmetic needed rebuilding.
