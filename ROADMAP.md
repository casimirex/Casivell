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
| `casivell-social` | All four branches of social insurance with employee/employer incidence; Entgeltpunkte accrual; Zugangsfaktor; monthly pension. |

**159 tests pass.** The engine builds for `wasm32-unknown-unknown` and has zero
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

Two figures are checkable against externally published results and match exactly:
the DRV "Standardrentner" pension of **1 913,40 €** from 1 July 2026 (45 points ×
42,52 €), and the announced **77,85 €** monthly increase at that adjustment.

### Known gaps, recorded rather than hidden

| Gap | Effect | Where recorded |
|---|---|---|
| zvE determination | **Not implemented at all** — the tariff takes an already-determined taxable income | `casivell-tax` crate docs |
| Lohnsteuer (payroll withholding) | No gross→net yet; needs the BMF Programmablaufplan and its Vorsorgepauschale | `casivell-social` crate docs |
| § 51a Abs. 2 EStG church tax base | Church tax **overstated for families** | `ChurchTaxResult::base_is_exact` |
| Kirchensteuer-Kappung | Overstated at high incomes | `ChurchTaxParameters` docs |
| Soli rounding direction | Up to 1 cent | `solidarity_surcharge` docs |
| Minijob / Übergangsbereich | Contributions are wrong below the Midijob threshold | `casivell-social` crate docs |
| PKV | Only statutory health insurance is modelled; no GKV/PKV comparison | this table |
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

Money/rate primitives, cited law tables for 2025–26, § 32a tariff, Soli, church
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
- [ ] **`casivell-net`: gross → net.** Blocked on the item below — see the note.
- [ ] **Lohnsteuer via the BMF Programmablaufplan**, including the
      Vorsorgepauschale of § 39b Abs. 2 Satz 5 Nr. 3 EStG.
- [ ] PKV, and the GKV/PKV comparison.
- [ ] Minijob and the Übergangsbereich sliding scale.

**Why gross→net is not done yet.** It needs *Lohnsteuer*, and Lohnsteuer is not the
annual assessment already in `casivell-tax`: it is the BMF Programmablaufplan, a
separate algorithm with its own allowances, its own Vorsorgepauschale, and its own
rounding. Approximating it — say, by annualising and applying § 32a — would produce
a monthly net figure wrong by tens of euros while looking authoritative. That is
precisely the failure documented in `docs/ROADMAP_ERRATA.md`, so the PAP gets its
own increment rather than a shortcut.

**Done when:** gross-to-net matches a published Brutto-Netto-Rechner to the cent
for a documented matrix of cases.

### Phase 2 — Taxable income determination · ~4 weeks

The largest correctness risk in the product.

- [ ] Werbungskosten, Arbeitnehmer-Pauschbetrag
- [ ] Sonderausgaben incl. the Vorsorgeaufwendungen limits
- [ ] Kindergeld vs. Kinderfreibetrag (Günstigerprüfung), § 51a base for church tax
- [ ] Kapitalerträge: Abgeltungsteuer, Sparer-Pauschbetrag
- [ ] Tax class comparison III/V vs IV+Faktor — flagging the planned move to Faktorverfahren

### Phase 3 — Simulation kernel · ~3 weeks

- [ ] Month-by-month household projection over 40 years
- [ ] Real vs. nominal toggle, inflation indexing
- [ ] Monte Carlo over market returns and wage growth
- [ ] `DataStatus::Projected` for every year past the last enacted statute, surfaced in the UI

**Performance:** 10 000 paths × 480 months is 4.8 M month-steps — tens of
milliseconds single-threaded for a lean integer kernel. Threading needs
`SharedArrayBuffer` and therefore COOP/COEP headers; deliverable on Cloudflare
Pages via `_headers`, but it breaks some embedding contexts. Ship single-threaded
first and measure before paying that cost.

### Phase 4 — Persistence and scenarios · ~3 weeks

- [ ] Versioned scenario schema with forward migrations
- [ ] Law-version pinning per scenario
- [ ] Scenario DAG: fork, compare, diff
- [ ] Export/import JSON; encryption at rest via `SubtleCrypto`

### Phase 5 — UI · ~5 weeks

- [ ] Input flows for household, income, expenses
- [ ] Timeline visualisation; scenario comparison
- [ ] **Explainability view** — click any figure, see the rule and provision that produced it
- [ ] Inexactness and `Projected` status shown inline, never buried
- [ ] German first, i18n-ready; WCAG 2.2 AA (see §8)
- [ ] PWA, fully offline

### Phase 6 — Life events · ~6 weeks

Elterngeld (all three defects in the old sketch corrected), Kita costs, buy vs.
rent incl. Grunderwerbsteuer by state, part-time and the Teilzeitfalle, job change
and ALG I, early retirement deductions.

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
3. **How to project past the last enacted year.** Extrapolating the Grundfreibetrag
   at trend inflation is defensible but is a guess. *Resolve:* make the assumption a
   user-visible, user-editable input, never a hidden constant.
4. **Steuerklassen III/V abolition.** Slated for replacement by the
   Faktorverfahren. *Resolve:* model as a scenario toggle once a date is enacted.
5. **Bürgergeld → Neue Grundsicherung.** Reform in progress; parameters not settled.
   *Resolve:* implement when enacted; mark `Draft` until then.
