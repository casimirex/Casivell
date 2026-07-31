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

## What is not tested automatically

The ABI is covered by the Rust tests in `casivell-wasm` — 14 of them, including that the
§ 39b chain reconciles and that all three tax-class arrangements settle to one liability. The
page's **rendering** is not. Checking it properly needs a headless browser or `jsdom`, and
this repository has no external dependencies; a shim thin enough to add without one would
verify the shim rather than the page.

So the JavaScript is syntax-checked and the ABI it calls is verified against the CLI figure by
figure, and the markup between them is reviewed by reading. That is stated rather than papered
over — it is the weakest link in this directory.

## What it computes

Two forms. **Brutto-Netto**: gross to net for one employee. **Steuerklassen**: the three arrangements
open to a married couple, and the fact that the annual tax is identical under all of them.
Everything else the engine does is reachable through the CLI. `docs/LIMITATIONS.md` is the authoritative account of what is and
is not modelled.
