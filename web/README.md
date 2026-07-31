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

## What it computes

The payslip form only — gross to net for one employee. Everything else the engine does is
reachable through the CLI. `docs/LIMITATIONS.md` is the authoritative account of what is and
is not modelled.
