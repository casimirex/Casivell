# The browser build

A single page, no framework, no bundler, no bindings crate — `index.html` instantiates the
wasm module directly and calls it. The whole thing is two files.

## Building

```sh
cargo build -p casivell-wasm --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/casivell_wasm.wasm web/
```

Then serve the directory over HTTP — `WebAssembly.instantiateStreaming` will not load a
module from `file://`:

```sh
python3 -m http.server -d web 8000
```

The `.wasm` is a build artefact and is not committed.

## The calling convention

Two steps, because returning a struct across a C ABI needs pointers and therefore `unsafe`:

1. `casivell_payslip(...)` computes and stores the result, returning `0` or a negative error
   code.
2. `casivell_result(field)` reads each figure out, in **cents**.

`casivell_enacted_years()` reports the range that actually computes, packed as
`first * 10000 + last`. The page builds its year picker from it rather than from a hard-coded
list, so it cannot offer a year the engine will refuse.

Money parameters and results are `i64`, which JavaScript presents as `BigInt`: pass
`BigInt(cents)` in, and wrap results in `Number()`. See `crates/casivell-wasm/src/lib.rs` for
why that friction is preferred to a silent ceiling.

## Explainability

Every figure in the table is clickable, and names the provision that produced it. The
Lohnsteuer line opens the § 39b working in the Programmablaufplan's own variable names —
`ZRE4 − ZTABFB − VSP = ZVE`, then the annual tax and the division by twelve — which is the same
chain `casivell --gross ...` prints, from the same numbers.

**No figure in that panel is recomputed in JavaScript.** Every one comes from the engine
through `casivell_result`, because a second implementation would be a second thing to be wrong.
The citations are structural — a paragraph number, never an amount — so they do not go stale
when a rate changes.

The footer shows the **Datenstand**: the fingerprint of the statutory data the figures rest on,
the same digest `casivell law` prints. Two people quoting the same one are looking at the same
law rather than assuming they are.

## Offline

The page works without a network once loaded, which for a tax calculator is a hazard as much
as a feature: a cached build answers confidently with the law it was built against, and nothing
on the screen looks wrong. Two things guard it.

The **Datenstand** in the footer names the statutory data the figures rest on, so a stale
answer is identifiable rather than merely suspected. And the service worker is
*stale-while-revalidate* rather than cache-first: it serves the cached build at once, checks
for a newer one behind it, and **tells the page instead of swapping**. A reader mid-calculation
keeps the build they started with, so one table cannot show figures from two different
statutory datasets.

`node web/sw.test.mjs` drives all of that — cold fetch, warm hit, changed build, offline with
and without a cache, and the install shell — with a fake `Cache` and a switchable network.
Node has `Response` and `fetch` built in, so it needs no dependencies. CI runs it.

## Accessibility

Audited against WCAG 2.2 AA. What the audit found, and what changed:

- **The explainability panel was opened by a click handler on `<tr>`.** No keyboard user could
  reach it at all — WCAG 2.1.1 Level A, and by some distance the worst defect the page had. It
  is now a real `<button>` in the first cell, with `aria-expanded` and an accessible name.
- Results changed with nothing announced. There is now a visually-hidden `role="status"` region
  that says **one sentence** — the net figure, or the joint liability — rather than re-reading
  the whole table on every keystroke.
- The view switcher used `aria-current`, which is for navigation. It uses `aria-pressed`.
- Tables gained captions and `scope` on every header cell.
- Custom-styled controls gained a `:focus-visible` ring, and every control is at least 44 px
  tall — comfortably past WCAG 2.2's 24 × 24 minimum.

`scripts/check_accessibility.py` turns those findings into assertions and CI runs it. It was
checked against the *broken* markup first: a guard that passes on both the fixed and the
original version proves nothing.

It cannot check colour contrast, focus order, or whether the prose makes sense — those were
reviewed by hand, and a full audit still wants axe-core and a real browser.

## What is not tested automatically

The ABI has 14 Rust tests, including that the § 39b chain reconciles and that all three
tax-class arrangements settle to one liability. The service worker has six. The page's
**rendering** has none: checking it properly needs a headless browser or `jsdom`, and this
repository has no external dependencies — a shim thin enough to add without one verifies the
shim rather than the page. I wrote one, watched it do exactly that, and deleted it.

So the JavaScript is syntax-checked, the ABI it calls is verified against the CLI figure by
figure, and the markup between them is reviewed by reading. That is stated rather than papered
over — it is the weakest link in this directory.

## What it computes

Three forms. **Brutto-Netto**: gross to net for one employee. **Steuerklassen**: the three arrangements
open to a married couple, and the fact that the annual tax is identical under all of them.
**Projektion**: the household forward over decades, with a chart.

The chart is a hand-drawn SVG path — no library, for the same reason the ABI is hand-written.
Projected years are **dashed**, because a solid line through them would claim a certainty the
figures do not have, and with only 2026 enacted that is very nearly the whole line. It carries
`role="img"` and a label, and the table beneath it holds the same numbers: a picture nobody can
read is not an answer.

Everything else the engine does — life events, property, Elterngeld, the annual assessment —
is reachable through the CLI. `docs/LIMITATIONS.md` is the authoritative account of what is and
is not modelled.
