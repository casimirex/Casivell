# Casivell — Roadmap

**A local-first financial simulator for German households.**
Version 0.2.0 · Revised 2026-07-30
Supersedes v0.1.0, archived at
[`docs/archive/CASIVELL_ROADMAP_v0.1.0.md`](docs/archive/CASIVELL_ROADMAP_v0.1.0.md)

> This revision corrects the previous version, whose statutory constants were
> substantially wrong. See **[docs/ROADMAP_ERRATA.md](docs/ROADMAP_ERRATA.md)** for
> the full verification record — what was wrong, the correct values, and the errors
> that would have shipped. The product thesis survived review; the arithmetic did
> not, and has been rebuilt.

---

## 1. What this is

A tool that answers questions German households actually face — *should we buy or
keep renting? what does part-time really cost? can I afford a year off?* — by
simulating decades of tax, social insurance and pension consequences under real
German law.

Three commitments distinguish it:

1. **Correct, and demonstrably so.** Every statutory figure cites its provision and
   a primary source, with a verification date. Every calculation is exact integer
   arithmetic. Where we are knowably incomplete, the output says so.
2. **Local-first.** Personal financial data never leaves the device. Not as a
   policy — as an architecture with no network code path.
3. **Explainable.** Every figure can be traced to the rule that produced it. A
   number a user cannot check is a number asking for trust it has not earned.

### What it is not

- **Not tax advice.** Steuerberatungsgesetz §§ 1–4 restricts who may give it. This
  is an information and simulation tool.
- **Not investment advice.** Suggesting portfolio allocations touches WpIG/KWG.
- **Not a Steuererklärung.** It estimates; it does not file.

These boundaries are product constraints, not disclaimers to bury in a footer. They
shape what gets built — see §7.

### Naming

The previous version was called *HAUSGELD*. That is a specific German legal term:
the monthly service charge a condominium owner pays their
Wohnungseigentümergemeinschaft. It states the wrong category to precisely the
audience we want. The project is **Casivell**.

---

## 2. Current state

Implemented, tested, and clippy-clean at `pedantic` with `-D warnings`:

| Crate | Contents |
|---|---|
| `casivell-core` | `Money` (integer cents), `Rate` (integer ppm), named rounding, `TaxYear`. `#![no_std]`, `#![forbid(unsafe_code)]`, no panicking operators. |
| `casivell-lawdata` | Year-keyed statutory tables for 2025 and 2026, each with a `Provenance`. Income tax tariff, pension, unemployment, health, care, Soli, church tax, retirement ages, all 16 Bundesländer. |
| `casivell-tax` | § 32a EStG tariff incl. Splittingverfahren, Solidaritätszuschlag incl. Milderungszone, church tax. |
| `casivell-income` | § 2 EStG: gross pay → taxable income, incl. Vorsorgeaufwendungen with both § 10 caps; the annual assessment, the Günstigerprüfung and the refund. |
| `casivell-social` | All four branches of social insurance with employee/employer incidence; Entgeltpunkte accrual; Zugangsfaktor; monthly pension. |
| `casivell-payroll` | Lohnsteuer per the BMF Programmablaufplan 2026, incl. the full Vorsorgepauschale and the class V/VI formula; Soli; church tax on the § 51a base; net pay for a month or a year. |
| `casivell-projection` | Statutory parameters past the last enacted year, derived from explicit assumptions and marked `Projected`. |
| `casivell-sim` | Month-by-month household projection over decades, with life events. `#![no_std]` and streaming: one month held at a time. |
| `casivell-cli` | The `casivell` command: a payslip, the parameters for any year, and a household projection. The only crate using `std`. |

**426 tests pass**, including all **516 values of the official BMF Prüftabellen**. The engine builds for `wasm32-unknown-unknown` and has zero
third-party dependencies.

Verified against primary sources: [§ 32a EStG](https://www.gesetze-im-internet.de/estg/__32a.html),
[SVBezGrV 2026](https://www.gesetze-im-internet.de/svbezgrv_2026/BJNR1160A0025.html),
[SVBezGrV 2025](https://www.gesetze-im-internet.de/svbezgrv_2025/BJNR16D0A0024.html),
[§ 55 SGB XI](https://www.gesetze-im-internet.de/sgb_11/__55.html),
[§ 341 SGB III](https://www.gesetze-im-internet.de/sgb_3/__341.html),
[§ 77 SGB VI](https://www.gesetze-im-internet.de/sgb_6/__77.html),
[§ 235 SGB VI](https://www.gesetze-im-internet.de/sgb_6/__235.html),
[SolzG 1995](https://www.gesetze-im-internet.de/solzg_1995/__3.html), and the
[DRV pension adjustment announcement](https://www.deutsche-rentenversicherung.de/DRV/DE/Ueber-uns-und-Presse/Presse/Meldungen/2026/260305-rentenanpassung-2026).

### External verification

Three independent checks against figures Casivell did not derive:

- **The BMF Prüftabellen.** Pages 39–40 of the PAP publish annual Lohnsteuer for 43
  salary levels across all six tax classes, in two variants — 516 values. Every one
  matches exactly. These are the reference values German payroll products are
  checked against.
- **The DRV "Standardrentner".** 45 Entgeltpunkte at the 1 July 2026 Rentenwert give
  **1 913,40 €**, and the announced increase at that adjustment was **77,85 €**. Both
  match.
- **An independent decimal implementation** of § 32a in `docs/reference/`, agreeing
  with the engine's integer algebra across the whole curve.
- **The tariff derivation.** § 32a's coefficients follow from its Eckwerte, because the
  marginal rate is pinned at each zone join. Applying that derivation to the *enacted*
  Eckwerte reproduces all eight published coefficients for both 2025 and 2026 exactly —
  which is what makes a *projected* tariff credible rather than fabricated.

### Known gaps, recorded rather than hidden

| Gap | Effect | Where recorded |
|---|---|---|
| Weekly / daily pay periods | Refused, not approximated: `LZZ = 3` and `4` scale by 360/7 and 1/360, which do not terminate in decimal | `casivell-payroll` crate docs |
| Sonstige Bezüge | A thirteenth month or bonus follows § 39b Abs. 3, a separate calculation | `casivell-payroll` crate docs |
| Versorgungsbezüge, Altersentlastungsbetrag | Pensions run through payroll are not modelled | `casivell-payroll` crate docs |
| Faktorverfahren | The alternative to classes III/V for couples | `casivell-payroll` crate docs |
| § 51a Abs. 2 church tax base | Correct in both the withholding and the assessment path. Still unimplemented in the standalone `casivell_tax::church_tax` helper | `ChurchTaxResult::base_is_exact` |
| zvE exactness | § 10's interaction is not reconciled against a real Steuerbescheid; several income categories absent | `Assessment::is_exact` |
| Kirchensteuer-Kappung | Overstated at high incomes | `ChurchTaxParameters` docs |
| Minijob / Übergangsbereich | Contributions are wrong below the Midijob threshold | `casivell-social` crate docs |
| PKV comparison | Private cover is modelled for the Vorsorgepauschale, but there is no GKV/PKV cost comparison | this table |
| Current-year Durchschnittsentgelt | Provisional by statute, so current-year points shift when the final figure lands | `EntgeltPoints::accrued_in_year` |

The zvE gap is the largest. Deductions, Werbungskosten, Sonderausgaben and
außergewöhnliche Belastungen are where a household simulator earns or loses its
credibility, and that work is Phase 2.

---

## 3. Architecture

```
┌──────────────────────────────────────────────────────────┐
│  UI                     React 19 (stable) + Vite         │
│                         Charts, scenario comparison      │
├──────────────────────────────────────────────────────────┤
│  Scenario layer         Versioned schema + migrations    │
│                         Law-version pinning              │
│                         IndexedDB via Dexie              │
├──────────────────────────────────────────────────────────┤
│  Simulation kernel      Monte Carlo, projections         │
│                    ┌─────────────────────────────────┐   │
│  Rust / WASM       │ casivell-tax   casivell-pension │   │
│  #![no_std]        │ casivell-social  …              │   │
│  #![forbid(unsafe)]├─────────────────────────────────┤   │
│                    │ casivell-lawdata (cited tables) │   │
│                    ├─────────────────────────────────┤   │
│                    │ casivell-core (Money, Rate)     │   │
│                    └─────────────────────────────────┘   │
└──────────────────────────────────────────────────────────┘
        No network code path below the UI layer.
```

The dependency direction is the point: `core` knows nothing of German law,
`lawdata` holds law but performs no calculation, calculation crates hold no
statutory constants. Each layer is reviewable on its own.

### Determinism and reproducibility

- Integer arithmetic only; `clippy::float_arithmetic` denied.
- `rust-toolchain.toml` pins the toolchain.
- Scenarios record the **law version** they were computed under, so a projection
  saved in 2026 still reproduces its original figures in 2031. Without this, saved
  plans silently change under the user whenever a table is updated.

### Privacy: verifiable measures, not proofs

The previous version claimed privacy was "cryptographically provable". It is not —
you cannot prove a web app does not exfiltrate. The stronger position is
verifiable measures that a sceptic can check in a browser's devtools:

- CSP with `connect-src 'none'`. The page cannot make a network request.
- Subresource integrity on every asset.
- Reproducible builds, so the shipped WASM can be rebuilt from source and compared.
- No network code in the engine — enforced by `#![no_std]`, which has no sockets.
- Open source engine, invited audit.

"Check the CSP header yourself" beats "trust our proof".

---

## 4. Phases

Sized for one engineer. The previous version's 16 weeks for a superset of this was
optimistic by roughly 3–4×; these estimates assume things go wrong.

### Phase 0 — Foundation ✅ *complete*

Money/rate primitives, cited law tables, § 32a tariff, Soli, church
tax, coding standard, CI.

### Phase 1 — Social insurance and net income · *in progress*

- [x] **Pension entitlement.** Entgeltpunkte accrual against the annual
      Durchschnittsentgelt with the contribution ceiling applied; the asymmetric
      Zugangsfaktor (−0.3 %/month early, +0.5 %/month deferred); the § 235 SGB VI
      retirement-age transition, which is still running and is *not* a flat 67; and
      the mid-year Rentenwert change on 1 July.
- [x] **Statutory social insurance contributions.** All four branches with correct
      incidence: the two independent contribution ceilings, the employee-only
      childless care surcharge, the per-child care reductions that reduce only the
      employee's share, and the Saxon 0.5-point shift. Fund-specific Zusatzbeitrag
      overrides the published average.
- [x] **Lohnsteuer via the BMF Programmablaufplan 2026.** All six tax classes; the
      full Vorsorgepauschale of § 39b Abs. 2 Satz 5 Nr. 3 (pension, health, care,
      unemployment, and the 1 900 € cap taken as a maximum against the uncapped
      variant); the § 39b Abs. 2 Satz 7 formula for classes V and VI; statutory and
      private health cover; the Solidaritätszuschlag with its `KZTAB`-scaled
      Freigrenze; and the § 51a base with Kinderfreibeträge.
- [x] **`casivell-payroll`: gross → net**, composing withholding with contributions
      from one shared profile, so the two halves cannot describe different people.
- [x] **Verified against the PAP's own Prüftabellen** — 516 official values, all
      matching.
- [ ] PKV cost comparison against GKV.
- [ ] Minijob and the Übergangsbereich sliding scale.
- [ ] Sonstige Bezüge (§ 39b Abs. 3): a thirteenth month or bonus.
- [ ] Weekly and daily pay periods, which need a finer scale than cents.

**Done when:** ~~gross-to-net matches a published Brutto-Netto-Rechner~~ — replaced
by a stronger criterion, now met: agreement with the BMF's own published reference
tables rather than with a third-party calculator.

### Phase 2 — Taxable income determination ✅ *core complete*

- [x] **Werbungskosten** with the § 9a Pauschbetrag as a floor, not a cap.
- [x] **Vorsorgeaufwendungen** in full: § 10 Abs. 1 Nr. 2 with the Abs. 3 cap derived from the
      miners' pension scheme, and Nr. 3/3a with the Abs. 4 cap *and* its Satz 4 override.
- [x] **The 4 % Krankengeld reduction**, applied to the general-rate portion only — not to
      the Zusatzbeitrag.
- [x] **Sonderausgaben-Pauschbetrag** (§ 10c), also a floor, with church tax paid counting
      toward it.
- [x] **Kindergeld versus Kinderfreibetrag** (§ 31): the Günstigerprüfung on the tax *saving*,
      with the Kindergeld clawed back when the allowance wins.
- [x] **The annual assessment and the refund** — withheld less owed, which is the figure most
      people actually want from a tax tool.
- [x] **§ 51a Abs. 2 surcharge base**, closing the gap `casivell_tax::church_tax` recorded:
      church tax and Soli are now levied on the child-reduced base in the assessment path too.
- [x] **Kapitalerträge** (§ 32d): the flat 25 % Abgeltungsteuer, the § 20 Abs. 9
      Sparer-Pauschbetrag, and the **Abs. 6 Günstigerprüfung** — the election of the ordinary
      tariff, run as two whole computations and reported with what the election was worth.
      The crossover is lower than intuition suggests: at 20 000 € of other income the
      marginal rate is 24,7 %, and stacking the capital income carries the average over 25 %,
      so the flat rate already wins.
- [x] **Außergewöhnliche Belastungen** (§§ 33, 33b), with the zumutbare Belastung as the
      staircase BFH VI R 75/14 requires. § 33a (Unterhalt) remains, since it turns on the
      recipient's own income and assets.
- [x] **Tax class comparison III/V versus IV+Faktor**, with § 39f implemented in
      `casivell-payroll`. The comparison's whole point is that the annual tax is *identical*
      under all three: the class decides when it is paid and by which spouse.
- [ ] The other five income categories of § 2 Abs. 1.

**The verification problem, and what was done about it.**

This is the first substantial part of Casivell with **no official reference table**. Payroll
has 516 published values; § 32a has an independent implementation. Neither exists for § 10.
That is stated in the crate documentation rather than glossed, and the verification is built
from what is available:

1. Every constant cited, and three of them cross-checked against the Programmablaufplan's own
   tables — the Kinderfreibetrag and both Pauschbeträge appear in each and must agree.
2. The Altersvorsorge cap is **derived** from the miners' pension ceiling and rate, and
   asserted against the published 30 826 €. A derivation that reproduces the published figure
   is stronger than transcribing it.
3. An **external validation point** for the Günstigerprüfung: published commentary puts the
   crossover for a jointly assessed couple with one child near 86 000 €, and a test checks it.
4. A **bounded comparison against the Vorsorgepauschale** — the same contributions run through
   two independent computations, one of them verified against 516 values. The test also pins
   the *direction*: § 10 must be the more generous, because the Vorsorgepauschale halves the
   Zusatzbeitrag while § 10 deducts it in full.
5. Structural properties: taxable income never exceeds gross, deductions are monotonic, caps
   bind where they should, and the Günstigerprüfung never chooses the worse option.

**The check that was missing, found by wiring the assessment into the kernel (Phase 3+).**
Lohnsteuer withholding is *designed* to be right for an employee whose year is flat — that is
the premise of § 39b, which annualises each month and divides back. So for a flat year the
assessment must return almost nothing, and it does: the two paths land within **96 to 332
cents** of each other on withholding of 2 248 € to 59 917 €, across a sixfold salary range and
under both the Grund- and the Splittingtarif.

That is not a tautology. Withholding runs the BMF Programmablaufplan with its deliberately
simplified Vorsorgepauschale; the assessment runs § 2 EStG with the real § 10 deduction.
Different statutes, different code, sharing only the tariff. Nothing makes them agree except
both being right — which is the strongest evidence available for the § 10 chain short of a
real Steuerbescheid, and it did not exist before the kernel could run both.

**`Assessment::is_exact` is always `false`.** § 10's interaction has not been reconciled
against a real Steuerbescheid, and several income categories are absent. The flag is in the
type rather than a footnote, for the same reason `ChurchTaxResult::base_is_exact` is: a caveat
a caller can ignore by accident will be ignored.

**A finding worth recording.** Because the § 10 Abs. 4 Satz 4 override carries an employee's
basket past the 1 900 € cap on health and care cover alone, additional liability or
unemployment insurance deducts **exactly nothing**. A planner that showed a tax saving there
would be wrong, so it is asserted as a test rather than left implicit.

### Phase 3 — Simulation kernel ✅ *complete*

- [x] **Month-by-month projection over decades.** `casivell-sim` runs each month through
      the same verified payroll code that produces a payslip, against the statutory
      parameters for that year — enacted where they exist, projected beyond.
- [x] **Allocation-free and streaming.** The engine is `#![no_std]`, so a 480-month
      timeline cannot be returned as a `Vec`. That turned out to be the right design
      pressure rather than an obstacle: the kernel holds one month and hands each to a
      `Sink`. Memory is `O(1)` in the horizon, the caller decides what to keep, and Monte
      Carlo becomes cheap instead of expensive.
- [x] **Real versus nominal.** Deflation happens *in the kernel* and the basis is recorded
      on every snapshot, because a consumer that cannot tell whether a figure has already
      been deflated is a bug waiting to happen.
- [x] **Pension accrual alongside.** Entgeltpunkte accrue month by month but are
      recomputed from the year's contributory income to date, so a full year lands exactly
      on the annual figure rather than twelve roundings away from it.
- [x] **Monte Carlo** over investment returns, bootstrapped from a caller-supplied set,
      with a deterministic PRNG written here so the same seed reproduces the same paths
      forever.
- [x] A `project` CLI form, so the kernel is inspectable rather than invisible.

**Two findings the kernel surfaced.**

A household whose nominal pay never rises accrues about **25.5 Entgeltpunkte** over forty
years against about **41.6** for pay that tracks average wages — a pension roughly
two-fifths smaller, from a decision that never appears on a payslip. Entgeltpunkte are a
*ratio* to the national average, so standing still means falling behind. This is why the
household's own pay growth is a separate input from the statutory wage-growth assumption;
one rate for both could not show it.

And the 1 900 € cap on the Vorsorgepauschale is nominal and unindexed, so a projection
shows it binding on steadily more people as wages grow. Held constant deliberately, and
documented as a substantive assumption rather than a neutral one.

**Not implemented, and refused rather than approximated:** no historical return table
ships with Casivell. Market data has its own provenance problem — "MSCI World 1970–2025"
is a licensing question before it is an engineering one — and inventing a plausible series
would be the exact failure the errata records. `monte_carlo` therefore requires the caller
to supply the returns. Block bootstrapping, which would capture serial correlation that
independent draws miss, is also absent and stated.

### Phase 4 — Persistence and scenarios · ~3 weeks

- [ ] Versioned scenario schema with forward migrations
- [ ] Law-version pinning per scenario
- [ ] Scenario DAG: fork, compare, diff
- [ ] Export/import JSON; encryption at rest via `SubtleCrypto`

### Phase 4.5 — Projected law years ✅ *complete*

`TaxYear::MAX` was 2026, so `LawYear::for_year(2027)` refused **by design** and no
projection could run at all. The fix was not to widen the verified range but to
separate two properties that had been conflated:

- [x] **Representable versus verified.** `TaxYear` now spans a century, and
      `has_verified_data()` reports whether a statute has been transcribed. The safety
      property moved to where it belongs — `LawYear::for_year` still refuses any year
      it cannot cite.
- [x] **`casivell-projection`**, a separate crate so `casivell-lawdata` keeps the
      property its design rests on: everything there is transcribed law, everything in
      the new crate is forecast, and no figure can be both.
- [x] **Explicit assumptions.** Nothing projected is obtainable without passing
      `Assumptions` — price inflation for the tariff Eckwerte and the Soli Freigrenze,
      wage growth for the ceilings, the Durchschnittsentgelt and the Rentenwert. Every
      rate is held constant, because there is no indexation rule for a political
      decision and a formula would dress a guess as a method.
- [x] **`DataStatus::Projected` propagates.** `LawYear::status()` already took the
      weakest status of its inputs; that machinery is now exercised by a real
      projection, and the CLI's `law` view leads with the warning.
- [x] **Statutory rounding reproduced.** Contribution ceilings snap to their statutory
      grids — a multiple of 600 € annually for pensions (§ 159 SGB VI), 450 € for
      health (§ 6 Abs. 7 SGB V). Given the wage growth the SVBezGrV 2026 itself cites,
      the mechanism reproduces both enacted 2026 ceilings exactly.

**Two findings worth recording.**

The § 32a coefficients are not free parameters — the marginal rate is fixed at each
zone join, which determines all of them from the Eckwerte. So a projected tariff is
*derived*, and the derivation is validated by reproducing both enacted years exactly.

And because the 45 % threshold has not been indexed since 2007, a long enough
projection has the 42 % threshold overtake it and the tariff stops being well formed.
Casivell refuses rather than returning nonsense. At the default 2 % that happens in
**2096** — seventy years out, well past any household horizon, and the refusal is
itself the model saying a frozen Reichensteuer threshold cannot hold indefinitely.

**Reversed in Phase 3:** `PayrollParameters` *is* now projected. The original reasoning —
that a projection wants the annual assessment rather than payroll withholding — ignored
that the annual assessment needs a zvE, which is not implemented. So the real choice was
between projecting the PAP's parameters and inventing a simplified zvE of our own.
Withholding is the statute's own approximation of the annual liability and is verified
against 516 official values; an invented simplification would have been a plausible figure
with nothing behind it. The PAP's *structure* is still not projected — only its numbers.

**Extended when the assessment reached the kernel:** `DeductionParameters` are now projected
too, and `LawYear` carries them. The Kinderfreibetrag, the Kindergeld and both Pauschbeträge
are indexed to prices; the miners' pension ceiling to wages, on its own § 159 SGB VI grid,
because it alone determines the Altersvorsorge cap. Rates and the § 10 Abs. 4 caps carry
forward, as everywhere else.

**A bug this produced, and the guard it left behind.** § 9a sets a fixed nominal amount with
no indexation rule, so the crate's usual principle said carry it forward — and that is what
was written first. But the *same* Pauschbetrag is projected by `project_payroll` for the
Programmablaufplan, where it was already indexed. Withholding and the annual assessment then
used different figures in the same simulated month, and a household's Steuerbescheid drifted
about ten euro a year, compounding into a **169 € demand after twenty years that no statute
produced**. It surfaced as a settlement curve that kept falling past zero instead of settling
onto the flat-year residual.

The fix is consistency: between two defensible conventions, the one that keeps the whole
system coherent wins — and indexing is independently the better guess, since the
Arbeitnehmer-Pauschbetrag has in fact been raised repeatedly (1 000 → 1 200 → 1 230 €).
`the_two_paths_project_the_same_pauschbetraege` now holds the two tables equal at **every**
horizon from 0 to 44 steps, because the failure grew with distance and a single spot check
near 2026 would have passed.

### Phase 5 — UI · ~5 weeks

- [ ] Input flows for household, income, expenses (the CLI is the reference for what
      the inputs mean)
- [ ] Timeline visualisation; scenario comparison
- [ ] **Explainability view** — click any figure, see the rule and provision that produced it
- [ ] Inexactness and `Projected` status shown inline, never buried
- [ ] German first, i18n-ready; WCAG 2.2 AA (see §8)
- [ ] PWA, fully offline

### Phase 4 — Persistence · *the reproducibility half is done*

A saved scenario is only worth saving if it still means something when it is opened. That
turns on a question nothing in the engine could previously answer: **which statutory data was
this computed against?**

- [x] **A stable fingerprint of a year's law.** FNV-1a over every statutory *value* — never a
      memory representation, so the digest is identical across compilers, architectures and
      builds, and safe to write into a file. 2026's enacted law is `e177cfbaa6bb7121`, and the
      `law` report prints it as *Datenstand*.
- [x] **Provenance is deliberately excluded.** A tidied citation or a fresh verification date
      changes no computed figure, and a digest that moved when documentation improved would
      cry wolf until it was ignored.
- [x] **A projected year's digest follows its assumptions.** Two households projecting 2040 at
      different inflation rates are working from *different data*, and the digest says so. An
      enacted year's does not move, because no assumption enters it.
- [x] **A scenario file**, with a schema version and the digest inside it. `--save FILE` on
      any form; `casivell replay FILE` re-runs it and says whether the law has moved.
- [x] **Scenario comparison**: `casivell compare a b`, two saved projections side by side.
- [ ] ~~The scenario DAG~~ — **reduced deliberately.** What a household wants from "variants
      branching off a base" is to see two plans against each other, and that needs two files
      and a comparison, not a graph. A DAG is structure without a consumer until there is a UI
      driving it; building it now would be inventing a requirement.

**A scenario is an invocation plus a Datenstand, not a serialised struct.** The obvious design
is to serialise the household, the config and the schedule. That design drifts: every field
added to `Household` is one someone must remember to write, read, version and default, and the
failure is silent — an old file loads, a setting is quietly missing, the numbers are subtly
wrong. Storing the *arguments* instead makes replay exact by construction, because there is no
second representation to disagree with the first, and a new field on `Household` needs nothing
in the format at all.

Refusals are deliberate throughout: a file from a newer schema is refused rather than
half-read, an unknown key is refused rather than skipped, and a missing field is named rather
than defaulted. A defaulted scenario computes something nobody asked for. A *changed digest*,
by contrast, is a warning and not an error — the household still wants its numbers, it just
needs to know they are not the numbers it saved. The warning prints **before** the report,
because a caveat after several screens of figures is a caveat nobody reads.

**Comparison re-runs rather than diffing text.** Diffing two rendered reports would be at the
mercy of column widths, would flag every year that moved by a cent, and would say nothing about
*how much* two plans differ — which is the only question anyone asks. Both scenarios go back
through the kernel and their summaries are compared, so the output is the handful of figures a
decision turns on, each with its difference.

Two checks, and only one is a refusal. Both files must be **projections**, because a payslip
and a forty-year plan share no figures and there is nothing to render. They *should* rest on
the same statutory data, and that is a warning at the top: comparing across a change in the law
attributes to the household's choices a difference that is partly the tables moving underneath.
Re-saving both fixes it, so the figures still follow.

**A comparison worth having.** Renting against buying, on 6 000 € a month over twenty-five
years at a 5 % return and 2 % property growth: buying ends **128 918 € behind** on net worth,
and 636 714 € behind on liquid wealth, having paid **183 253 € of mortgage interest**. Tax,
contributions and pension are identical to the cent, which is the check that the difference is
the housing decision and nothing else. And the buyer's lowest point is 16 323 € *below zero* —
a plan that passed through insolvency, which an end state alone would have hidden.

**The maintenance hazard, and what guards it.** A field added to a parameter set and not added
to its digest would be invisible — scenarios would claim a reproducibility they no longer have.
`the_digest_is_pinned` asserts an exact value, so any change to any hashed figure fails loudly;
`every_parameter_set_reaches_the_digest` nudges one figure in each of the eight sets and checks
the year's digest moves, which catches a whole parameter set forgotten in `LawYear`. Updating
the pin is deliberately a moment where someone has to decide whether saved scenarios should be
told the law changed — a question that is easy to skip if nothing forces it.

### Phase 6+ — Buying a home ✅ *complete*

`casivell-property` prices the transaction and amortises the loan. It deliberately stops
before answering "should I buy", and the boundary is the point of the design:

- **Exact and statutory.** Grunderwerbsteuer, from each Land's own Act. All sixteen rates
  transcribed and cross-checked against two independent published tables that agreed — which
  mattered: the rates change one state at a time and stale figures circulate for years. Sachsen
  has been 5,5 % since 2023 and the first source consulted still said 3,5 %.
- **Exact but not statutory.** The annuity mortgage, computed to the cent from stated contract
  terms.
- **Neither.** House price growth, rent growth, maintenance, what it sells for later. A
  buy-versus-rent verdict is dominated by these, and no care taken over the first two makes the
  third reliable.

**Three numbers worth having.** On a 400 000 € house in Nordrhein-Westfalen with 80 000 € down:

- The **Nebenkosten are 48 280 €** — 12 % of the price with an agent — and they buy nothing a
  bank lends against. A nominal 20 % deposit finances 92 % of the price, which is a different
  mortgage at a different rate.
- **2 % Tilgung clears in 29 years, not the fifty** a naive `100 / 2` suggests, because the
  repayment portion grows as the interest portion shrinks. One point more takes it to 22 years
  and saves 61 585 € of interest.
- After a ten-year Zinsbindung the **Restschuld is 280 241 €** — a decade of payments retired
  under a quarter of the debt, and more than half of everything paid went to interest. That
  figure has to be refinanced at whatever rates then are, and it is the number households most
  need and least often see.

**And the comparison, through the kernel.** `Event::PropertyPurchase` completes a purchase,
rebases the household's expenses from rent to Hausgeld, and amortises the mortgage month by
month alongside the same payroll and annual assessment as every other projection. The report
gains a **Netto ges.** column — financial wealth plus the property, less the debt — because
without it a buyer looks bankrupt in the month they complete.

**The honest headline, and it is a negative one.** The verdict flips on the one number nobody
knows. Same household, same salary, same house, same twenty-five years: at 1 % annual property
growth renting wins, at 4 % buying does. Everything the engine computes exactly — the
Grunderwerbsteuer, the amortisation, the payroll, the assessment — is *identical* in both runs.
The renter's position does not move at all when the assumption changes, which is the control.

That is why there is no verdict in the output. A tool that picked one growth rate and
pronounced would be dressing a guess as a calculation, and this is the one place in Casivell
where that temptation is strongest.

**One thing the model shows that households routinely miss.** Immediately after completing,
the buyer's net worth is about 40 000 € *below* the renter's — almost exactly the incidental
costs, which bought no equity. Years of growth go into making that back before buying is even
level.

### Phase 2+ — Außergewöhnliche Belastungen ✅ *complete*

**The roadmap said this was blocked because "the data to test it is not available". That was
wrong, and inherited rather than checked.** § 33 Abs. 3's table is printed in the statute in
full — three income bands, four family rows, twelve percentages — and § 33b's Pauschbeträge
likewise. Nothing was missing. The lesson is the same one the roadmap was rewritten for at the
start: a claim about the data should be verified before it is carried forward.

**The method is not in the statute, though.** § 33 Abs. 3 gives the bands and says nothing
about how they combine, and the administration long applied one band's percentage to the whole
income. The BFH rejected that in VI R 75/14 of 19 January 2017 — each percentage applies only
to the part of income in its own band, a staircase exactly like § 32a's tariff — and the BMF
adopted it by letter of 1 June 2017. The provenance cites the case and the letter alongside
the statute, because the arithmetic comes from them and not from the text.

For a childless single on 60 000 €: 5 % of 15 340, plus 6 % of 35 790, plus 7 % of 8 870 =
**3 535,30 €**, against **4 200 €** under the old flat reading. Both are pinned in a test so a
regression to the cliff reading fails loudly rather than quietly costing people money.

**Two things the report now says that a bare number could not.** Most § 33 claims deduct
nothing — 3 000 € of dental work against a 3 449 € threshold is a real expense and a zero
deduction — so the assess form distinguishes "your costs were below the threshold" from "you
claimed nothing". And the § 33b Pauschbetrag is **not** reduced by that threshold, so someone
with a recognised Grad der Behinderung receives a deduction in a year when the § 33 route gives
them none. Running the Pauschbetrag through the threshold would wipe out an entitlement the
statute grants unconditionally; adding the two routes together would double-count. They are
kept apart and the report says which produced what.

**The thresholds are unindexed**, set in Deutsche Mark and merely converted, so
`carry_forward_burden` holds them fixed. The consequence is that the zumutbare Belastung
silently reaches a larger share of households every year as incomes rise past a frozen
51 130 €. That is the statute; the projection only continues it.

### Phase 2+ — The CLI reaches the whole engine ✅ *complete*

`casivell-income` had been reachable only through the simulation kernel, which meant the § 2
chain — the part of the engine a person would most want to check against their own
Steuerbescheid — could not be looked at directly. `casivell assess` prints it stage by stage:
Werbungskosten, both Vorsorge caps with a note where each binds, the § 31 Günstigerprüfung in
words, the settlement, and § 32d capital income with what the Abs. 6 election is worth.

Every intermediate is shown rather than only the answer, because reconciling a Bescheid means
finding *which* line diverged and a single figure makes that impossible.

`--benefits` and `--capital` also give § 32b and § 32d their first CLI surface; both were
implemented and unreachable.

### Phase 2+ — § 39f and the tax-class question ✅ *complete*

The most misunderstood thing in German payroll, and now the thing the tool says first.

- [x] **§ 39f EStG**, the Faktorverfahren: `Y : X` truncated to three decimals, available only
      where it comes out below one. `Employment::with_factor` refuses it outside class IV and
      outside `(0, 1)`, because § 39f Abs. 1 Satz 6 makes a factor of one not an edge case but
      a case where the election does not exist.
- [x] **A three-way comparison** and a `casivell classes` CLI form.

**The point the report leads with.** A married couple's income tax is fixed by § 32a Abs. 5
and no combination of classes moves it by a cent. At 5 000 € and 1 800 € the annual tax is
9 180 € under all three arrangements; what differs is that III/V takes 633 € a month and owes
**1 587 €** at assessment, IV/IV takes 835 € and gets **838 €** back, and IV+Faktor lands
within **4,08 €** of zero. That last figure is § 39f doing precisely what it was written to do.

**Where the choice does matter, and it is not the tax.** Wage-replacement benefits are computed
from *net pay*, so they follow the class: `casivell-benefits` already shows Elterngeld varying
by about 386 € a month between class III and class V at 3 000 € gross, for the same household
and the same total tax. Choosing a class before a birth is a real decision; choosing one to
"pay less tax" is not.

**A case that looks wrong until it does not.** For two equal earners III/V produces a *refund*,
not a demand — class V withholds punitively from a "lower" earner who is not actually lower.
And § 39f is unavailable to them, correctly: class IV already withholds them exactly right, so
there is nothing for a factor to correct.

**Verification.** There is no published Prüftabelle for the Faktorverfahren, so unlike the rest
of this crate it cannot be checked against official values. It is checked instead against the
property the statute exists to produce — that IV+Faktor withholding lands near the joint annual
liability — using the independently implemented § 32a. Weaker than a reference table, stronger
than nothing, and said plainly in the module documentation.

**A bug the test found.** The first comparison put withheld tax *including* the
Solidaritätszuschlag against a liability that was income tax alone, and blamed the 572 € Soli
on the class choice. `Arrangement` now keeps `annual_income_tax` apart from
`annual_withholding` so the settlement compares like with like.

### Phase 6+ — Elterngeld and the Progressionsvorbehalt ✅ *complete*

- [x] **§ 32b EStG** in `casivell-income`. Tax-free benefits are added to the taxable income
      solely to find a *rate*, which is then applied to the income that really is taxable.
      Covers Arbeitslosengeld, Kurzarbeitergeld and Krankengeld as well as Elterngeld.
- [x] **The BEEG** in a new `casivell-benefits` crate, which must sit above `casivell-payroll`
      because § 2e computes Elterngeld's tax deduction with the Programmablaufplan. The
      layering is the statute's, and it means the largest deduction in the formula runs
      through the code checked against 516 official values.
- [x] **`Event::ParentalLeave`** in the kernel, paying the benefit monthly and feeding it to
      the annual assessment so the rate effect lands where it really lands.

**The number this was built to produce.** A household on 4 000 € a month taking twelve months'
leave from July receives 19 700 € of Elterngeld — and pays **2 523 €** of it back in extra tax
the following summer. `the_progressionsvorbehalt_claws_back_part_of_the_benefit` isolates it by
running the identical interruption twice, once as `ParentalLeave` and once as `UnpaidLeave`:
withholding is identical to the cent in both, and the refund falls from 4 216 € to 1 693 €.

**And the case that surprises people.** Every straightforward parental-leave scenario ends in a
*refund*, because stopping work mid-year over-withholds. Part-time `ElterngeldPlus` does not:
the household draws a full year of reduced salary *and* the benefit, so withholding is roughly
right for the salary and § 32b then raises the rate on all of it. The result is a bill of about
1 200 € a year for a household whose income went **down** — arriving the summer after, with
nothing on any payslip in between to warn of it.

**A related finding, from the control case.** § 39b under-withholds whenever income *rises*
mid-year, so simply returning from part-time produces a demand of its own, with no § 32b
involved. The control in that test had to become quantitative rather than a sign test.

**Kindererziehungszeiten, and the pessimism they removed.** Casivell exists in part to show
what a career break costs a pension, and until now it showed a break with *no* credit at
all — overstating the harm in the one place the model most needed to be even-handed.

The correction is larger than "softens it". The credit is pegged to **average earnings**, not
to the parent's own salary, so for someone below average it is worth more than the entitlement
they gave up: a parent on 3 000 € a month taking two years out ends up with *more*
entitlement than one who never stopped. A model without it told exactly those households the
opposite.

§ 70 Abs. 2 caps the year's combined points at what a full-ceiling earner accrues, which cuts
the other way and equally deliberately: a parent already at the Beitragsbemessungsgrenze gets
**nothing** from the credit, because their salary alone is already at the cap. The provision
protects the people who gave up income and not the people who did not. It looks like a defect
until you see which way it points, so it has its own test saying so.

§ 56 Abs. 5 *extends* rather than overlaps — a second child born during the first's period
pushes it later instead of running two in parallel, so a parent never earns two children's
credit in one month. Modelled by queueing the windows, which reproduces the extension exactly
and reduces to plain thirty-six months apiece for well-spaced children.

Anlage 2b, which § 70 Abs. 2 points at for the cap, stops at 2002. From then on the ceiling is
simply what a full-Beitragsbemessungsgrenze earner accrues, so the cap is **derived** from the
contribution ceiling and the Durchschnittsentgelt rather than transcribed from a table that no
longer runs.

**Elterngeld is unindexed, and the projection says so.** The 1 800 € cap and the 300 € floor
have stood since 2007 and § 2 gives no adjustment rule of any kind, so `carry_forward_benefits`
holds them fixed — which is the *accurate* projection, not the lazy one. A long horizon shows
the benefit steadily losing real value, and that erosion is the statute's, not the model's.

### Phase 3+ — The annual assessment inside the kernel ✅ *complete*

The gap Phase 3 left open, and the precondition for several later items.

- [x] **Each calendar year is assessed as it closes**, under its own year's tariff and
      allowances, from income and withholding accumulated month by month rather than from an
      annual salary — because the whole point is that the twelve months need not be alike.
- [x] **The settlement arrives at a lag.** Seven months, putting it at the end of the
      following July: § 149 Abs. 2 AO's filing deadline, with the Bescheid following. A
      refund is not current-year cash, and a cash-flow projection that paid it in December
      would be wrong about the thing it exists to model.
- [x] **Refused rather than approximated** for tax classes IV, V and VI and for private
      health cover. `NoAssessment` names the reason and the CLI prints it. Assessing one
      salary of two under the Splittingtarif would invent a large refund every year, which is
      far worse than showing withholding and saying why.
- [x] **Entgeltpunkte now reset on the calendar year**, not the employment anniversary. A
      latent bug for any projection not starting in January, which had been accruing points
      over a July-to-June window the statute knows nothing about.

**What it changes.** A career break, a mid-year start or a part-time year all over-withhold
under § 39b, which taxes each month as though the year continued unchanged. Six unpaid months
at 5 000 € now refund over 1 000 € the following July. Before this the projection showed every
interruption costing strictly more than it does.

**A finding that fell out of it.** A household with one child and pay that never rises sees
the § 31 Günstigerprüfung *reverse* over twenty years: the Kinderfreibetrag is worth 372,96 €
at the start, declines every year as indexed allowances outgrow flat pay, and by about year
eleven the Kindergeld has become the better deal. Nobody encoded that crossover — it falls out
of two indexed statutory series meeting a household that stood still.

### Phase 6 — Life events · *in progress*

- [x] **The event architecture.** A bounded `Schedule` of events, resolved per month. Every
      life event has the same shape — *from month N, something is different* — so one
      mechanism serves all of them and the thirteenth is cheap.
- [x] **Part-time work** (the Teilzeitfalle), **unpaid career breaks**, **promotions**,
      **expense changes**, **one-off costs**, and **non-employment income**.
- [x] Exposed through the CLI: `--part-time 3:8:60`, `--break 5:6`, `--raise 15:8000`,
      `--one-off 5:-60000`.
- [x] **Elterngeld**, with the Progressionsvorbehalt. `casivell-benefits` computes the BEEG
      amount — the stylised net of §§ 2c–2f, the 65–100 % sliding rate, the 300 … 1 800 €
      clamp, § 2a's bonuses, `ElterngeldPlus`, and the § 1 Abs. 8 income cliff — and
      `Event::ParentalLeave` pays it monthly while carrying it into the annual assessment as
      a § 32b benefit. Both halves, because either alone misleads.
- [x] **Kindererziehungszeiten** (§§ 56, 70 Abs. 2 SGB VI): thirty-six months of pension
      credit from the month after a birth, at the statute's own 0,0833 points a month —
      2,9988 for a child, not a round three. `Event::ChildBorn` is separate from
      `Event::ParentalLeave` because § 56 credits whoever *raises* the child, so a parent
      back at work the next month keeps every month of it.
- [ ] Kita costs, which are municipal rather than statutory and have no national table.
- [ ] Buy versus rent: needs Grunderwerbsteuer by state (3.5 %–6.5 %) and mortgage
      amortisation. The deposit and the payment can be modelled today with `--one-off` and an
      expense change, which is not the same thing.
- [ ] Job change with ALG I, early retirement deductions, moving abroad.

**Two of the three questions on the front page are now answerable**, and the third is not.
*"What does part-time really cost?"* and *"Can we afford a year off?"* run end to end;
*"Should we buy or keep renting?"* needs the property model above.

**A design bug the tests caught.** `PayChange` was first implemented as a transient override
holding a fixed amount, while the household's own growth kept compounding behind it. After
about twenty years at 2.8 % the growing baseline overtook the "promotion" and it quietly
became a pay cut. Permanent changes now *rebase* the baseline so growth compounds from the
new figure; transient modifiers leave it alone. **No unit test would have found this** — it
took a forty-year integration test asserting that a promotion leaves the household better off.

**A finding, and its encouraging half.** Ten years at 60 % costs about four Entgeltpunkte
permanently, because a reduced year is permanently a reduced year. But a large enough later
promotion *can* more than repair the record, since points accrue faster on a higher salary.
Both are pinned as tests.

### Phase 7 — Launch readiness · ~3 weeks

- [ ] Impressum (§ 5 DDG), Datenschutzerklärung, AGB
- [ ] Legal review of positioning against StBerG and RDG
- [ ] Performance and bundle budget verification
- [ ] Accessibility audit
- [ ] Law-update pipeline: signed data bundles, a documented annual January/July cadence

**Total: roughly 27 weeks** to a defensible public release, excluding the AI work
below.

---

## 5. Deliberately not in the MVP

**On-device LLM (TinyLlama + ONNX Runtime Web).** Cut, for four reasons:

1. **Budget.** A Q4-quantised 1.1 B model is ~600 MB against a stated 500 KB
   budget — 1 200× over. The two goals were mutually exclusive as written. (The old
   document also said "1.1B" in one place and "3B" in another.)
2. **Quality.** A 1.1 B model is not competent at German tax reasoning. It will
   produce fluent, confident, wrong answers about consequential decisions.
3. **Legal exposure.** An "AI Financial Advisor" is the component closest to the
   StBerG line.
4. **Determinism.** It contradicts *"gleiche Eingaben, gleiches Ergebnis, immer"*.

**Replaced by** something that better serves the actual need: a **deterministic
optimiser**. Users asking "what should I do?" want a ranked, explainable comparison
of concrete options, and an exhaustive search over a bounded decision space
delivers that — reproducibly, in milliseconds, with a reason attached to every
ranking. That is strictly better than a small model's guess.

If narration is wanted later, it should be an *optional*, clearly-labelled
bring-your-own-API-key feature that reads results it cannot alter.

---

## 6. Business model

The previous version proposed €5/mo tiers while promising "no accounts required".
Those are incompatible: gating features requires knowing who is entitled.

**Resolution — offline licence keys.** A purchase issues an Ed25519-signed licence
verified locally. No account, no server call, no telemetry. The signature proves
entitlement without an identity.

| Tier | Price | Contents |
|---|---|---|
| Free | — | Full calculation engine, 3 scenarios, all visualisation |
| Pro | €5/mo or €45/yr | Unlimited scenarios, optimiser, PDF export |
| Advisor | €49/mo | Multi-client, white-label |

The engine stays Apache-2.0 and auditable. Charging for the *interface* while the
*arithmetic* is free and verifiable is consistent with the correctness claim: the
part users must trust is the part they can inspect.

---

## 7. Legal and compliance

Absent from the previous version, and not optional in Germany.

| Area | Requirement | Consequence for the product |
|---|---|---|
| **StBerG §§ 1–4** | Tax advice is restricted | Present computations and cite provisions; never prescribe a filing position. Avoid "you should". |
| **RDG** | Legal advice is restricted | Same posture for legal questions. |
| **WpIG/KWG** | Investment advice may need a licence | Model portfolios generically; no product recommendations. |
| **§ 5 DDG** | Impressum mandatory | Required at launch. |
| **GDPR** | Lawful basis, information duties | Local-first is a large genuine advantage — **argue it explicitly**. Personal data never reaches us, so most processing duties never arise. |
| **Payments** | PCI scope | Stripe or Paddle; card data never touches us. |

Phase 7 includes a lawyer's review of positioning. Doing this late is a mistake;
doing it before there is a product to describe is wasted money.

---

## 8. Success criteria

Revised to be measurable and achievable. Where the previous version's targets were
impossible, they are corrected rather than quietly dropped.

| Metric | Target | Note |
|---|---|---|
| Statutory accuracy | Matches BMF Programmablaufplan for a documented case matrix | The verifiable version of "100 % accurate" |
| Determinism | Bit-identical output across platforms for identical input | Integer arithmetic makes this achievable |
| Every figure cited | 100 % of statutory constants carry `Provenance` | Enforced by tests |
| 40-year projection | < 100 ms, 10 000 Monte Carlo paths | Measure; do not assume threading |
| Engine WASM size | < 300 KB gzipped | Achievable for `#![no_std]` integer code |
| Total initial payload | < 1.5 MB gzipped | The old 500 KB total was not reachable with a charting stack. Honest budget, still fast. |
| Offline | 100 % of functionality | No network code path |
| Accessibility | **WCAG 2.2 AA** | The old "AAA" is not realistically attainable for data-dense charts — AAA requires 7:1 contrast and forbids much of what makes a chart legible. AA fully met beats AAA claimed. |
| Lighthouse | ≥ 95 performance, 100 accessibility | |

---

## 9. Engineering practice

See **[docs/CODING_STANDARD.md](docs/CODING_STANDARD.md)**. The engine follows the
JPL "Power of Ten" adapted to Rust: `#![no_std]` (no allocation),
`#![forbid(unsafe_code)]`, denied `unwrap`/`expect`/`panic`/`indexing`, denied bare
arithmetic and all floating point, 60-line function limit, and `clippy::pedantic`
with `-D warnings` in CI.

Each rule names its enforcement mechanism, because a rule that is not mechanically
enforced is a preference, and preferences decay.

---

## 10. Open questions

Recorded rather than assumed, and each names what would resolve it.

1. **Soli rounding direction.** SolzG does not prescribe one. *Resolve:* compare
   against BMF reference cases.
2. **Which Zusatzbeitrag to default to.** The 2.9 % average is published for funds
   without their own rate; real rates vary by over a point. *Resolve:* require the
   user to pick their fund, defaulting to the average with a visible caveat.
3. ~~**How to project past the last enacted year.**~~ **Resolved in Phase 4.5.** The
   assumptions are a required, user-editable input (`--inflation`, `--wage-growth`),
   never a hidden constant, and everything derived from them is marked `Projected` and
   labelled as such wherever it surfaces.
4. **Steuerklassen III/V abolition.** Slated for replacement by the
   Faktorverfahren. *Resolve:* model as a scenario toggle once a date is enacted.
5. **Bürgergeld → Neue Grundsicherung.** Reform in progress; parameters not settled.
   *Resolve:* implement when enacted; mark `Draft` until then.
