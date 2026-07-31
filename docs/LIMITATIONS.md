# What Casivell does not do

Every claim Casivell makes about its accuracy is worth exactly as much as this page. The
verification story is in `README.md`; this is its counterweight, gathered in one place because
until now these caveats lived in a dozen module headers where only someone reading the Rust
would find them.

It is organised by what a household would ask, not by crate.

---

## The short version

Casivell computes **one employee's** German income tax, social insurance, payroll withholding
and pension entitlement, and projects a household forward from there. It is an estimate, not a
liability, and `Assessment::is_exact` is `false` on every assessment it produces.

The three things most likely to matter to you:

1. **A second earner is not modelled.** If both spouses work, the annual assessment is refused
   outright rather than computed on half a household.
2. **Only employment and capital income.** No self-employment, no rental income, no pensions in
   payment, no transfers except Elterngeld.
3. **Nothing here has been reconciled against a real Steuerbescheid.** The § 10 chain in
   particular is checked by construction and by cross-comparison, not against an actual
   assessment notice.

---

## Income the engine cannot see

§ 2 Abs. 1 EStG has seven categories of income. Casivell models **two**:

| Category | Status |
|---|---|
| Nichtselbständige Arbeit (§ 19) — employment | Modelled |
| Kapitalvermögen (§ 20) — capital income | Modelled, including the § 32d Abs. 6 election |
| Gewerbebetrieb — business | Not modelled |
| Selbständige Arbeit — self-employment | Not modelled |
| Vermietung und Verpachtung — rental | Not modelled |
| Land- und Forstwirtschaft — agriculture | Not modelled |
| Sonstige Einkünfte (§ 22) — incl. pensions in payment | Not modelled |

A household with rental income or a side business will see figures that are wrong by more than
a rounding error, and Casivell has no way to detect that it is being asked the wrong question.

**Elterngeld and other wage-replacement benefits** are handled, including the § 32b
Progressionsvorbehalt. `Event::OtherIncome` accepts a known net amount for anything else, and
applies **no tax to it at all** — which is right for almost nothing in general and is documented
as the caller's problem.

---

## Households the engine refuses

The simulation kernel models **one employment**. Where that is not enough it declines to answer
rather than answering badly — `NoAssessment` names the reason and every report prints it.

- **Tax classes IV and V.** Both spouses are assessed together on their combined income.
  Assessing one salary alone would apply the Splittingtarif to half a household and produce a
  large fictitious refund every year.
- **Tax class VI.** The class exists precisely because another job holds the allowances.
- **Private health cover.** § 10 Abs. 1 Nr. 3 deducts the *Basisabsicherung* portion of a
  private premium, a figure the insurer certifies and Casivell is not given.

Class III *is* assessed: it describes a married couple whose other spouse has no employment
income, which is a household the kernel models completely.

---

## Deductions and reliefs

**Implemented:** Werbungskosten with the § 9a Pauschbetrag, Vorsorgeaufwendungen in full
(§ 10 Abs. 1 Nr. 2 and 3/3a with both caps and the Satz 4 override), the § 10c Pauschbetrag,
the § 31 Günstigerprüfung between Kindergeld and Kinderfreibetrag, außergewöhnliche Belastungen
under §§ 33 and 33b with the staggered zumutbare Belastung, and the § 20 Abs. 9
Sparer-Pauschbetrag.

**Not implemented:**

- **§ 33a** — Unterhaltsleistungen and the Ausbildungsfreibetrag, which turn on the recipient's
  own income and assets.
- **The election between the § 33b Pauschbetrag and larger actual costs**, which needs receipts.
- **Riester and Rürup** beyond the statutory pension's own contributions.
- **Loss carry-forward (§ 10d)**, the Härteausgleich and the Altersentlastungsbetrag.
- **Kirchensteuer-Kappung.** Most Landeskirchen cap church tax at a share of taxable income.
  Casivell applies the plain rate, so a high earner's church tax is overstated.

---

## Payroll

Implemented against the BMF Programmablaufplan and checked against its **516 published
values**: tax classes I–VI, monthly and annual pay periods, the full Vorsorgepauschale,
statutory and private health cover, the Saxon care split, the childless surcharge, and the
§ 39f Faktorverfahren.

**Not implemented:**

- **Weekly and daily pay periods.** The PAP scales them by `360/7` and `1/360`, which do not
  terminate in decimal. Supporting them approximately would silently disagree with real
  payroll.
- **Sonstige Bezüge (§ 39b Abs. 3)** — a thirteenth month or a bonus follows a separate
  calculation.
- **Versorgungsbezüge** and the Altersentlastungsbetrag, which matter for pensions run through
  payroll.
- **Minijobs and the Übergangsbereich (Midijob) sliding scale.**
- Civil servants, the Künstlersozialkasse, and voluntary or self-employed contributions.

---

## Elterngeld

The amount is computed under §§ 2, 2a, 2c–2f and 4a BEEG. **Entitlement is asserted by the
caller**, not decided here: residence, the child living in the household and working hours
during the reference period are facts a simulator does not hold. The one eligibility rule that
is pure arithmetic — the § 1 Abs. 8 income limit — *is* applied.

Not modelled: self-employment income (§ 2d), the § 2b Bemessungszeitraum shifts for earlier
parental leave or illness, Mutterschaftsgeld offsetting, and the § 4b Partnerschaftsbonus.

The 2025 income limit is carried at 175 000 €, which is correct for births **from 1 April
2025**. Births earlier in 2025 fell under different limits that a year-keyed table cannot
express.

---

## Property

**Exact and statutory:** Grunderwerbsteuer, from each Land's own Act.

**Exact but not statutory:** the annuity mortgage, computed to the cent from stated terms.

**Neither:** everything else. Notary and land-registry costs approximate the GNotKG fee
schedule, which is not implemented — the report labels them *estimated* beside the
Grunderwerbsteuer's *statutory*. Maklerprovision is contractual and has no default.

Not modelled at all: the cost of selling, refinancing when the Zinsbindung expires,
Grundsteuer, and any tax treatment of a sale.

**A buy-versus-rent verdict is dominated by assumptions, not by arithmetic.** At 1 % annual
property growth renting wins the worked example; at 4 % buying does, with every computed figure
identical. Casivell therefore reports the assumption beside the answer and issues no verdict.

---

## Projections past the last enacted year

Casivell holds enacted data for **2025 and 2026**. Beyond that, figures are projected and every
one of them is labelled `Projected` — in the type, in the reports, and in the row where a table
crosses over.

- Rates are **never** projected. There is no indexation rule for a contribution rate or a tax
  rate; each is a political decision, and a formula would dress a guess as a method.
- Amounts with a statutory indexation rule are indexed. Amounts without one are carried
  forward, which is accurate rather than lazy — but it means the Elterngeld cap, unchanged
  since 2007, and the § 33 income thresholds, converted from Deutsche Mark, both erode in real
  terms exactly as the statute lets them.
- The tariff stops being well formed once the unindexed 45 % threshold is overtaken by the
  indexed 42 % one. At the default assumptions that is **2096**, and Casivell refuses rather
  than returning nonsense.

Two projections under different assumptions are **different data**, and the statutory
fingerprint says so.

---

## Verification, and where it is thin

Strongest first:

| Area | Evidence |
|---|---|
| Lohnsteuer | 516 official BMF Prüftabelle values |
| § 32a tariff | Independent decimal implementation; the zones' own continuity |
| Projected tariffs | Derivation reproduces all eight published coefficients, both years |
| Grunderwerbsteuer | Two independent published tables, agreeing on all sixteen states |
| Annual assessment | Agrees with withholding to **96–332 cents** on a flat year, across a sixfold salary range — two statutes, different code |
| § 10 Vorsorge | Bounded comparison against the Vorsorgepauschale; caps derived and matched to published figures |
| Elterngeld | The statute's own boundary values; a derived crossover three sources agree on |
| § 39f Faktorverfahren | The property the statute exists to produce. **No published table exists** |
| Kindererziehungszeiten | The statute's own figures. No reference table |

**Nothing has been reconciled against a real Steuerbescheid.** That is the single largest gap,
and no amount of internal consistency substitutes for it.

---

## This is not tax advice

Casivell is a calculator. It does not give advice within the meaning of §§ 1–4 StBerG, and
every report says so. Figures are estimates for planning, not liabilities. Where a real
decision or a real filing is at stake, a Steuerberater is the answer.
