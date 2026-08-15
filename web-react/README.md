# Casivell — React web

A React + Vite + TypeScript front end over the same C ABI the static `web/` site
uses. It renders the three calculators — Brutto-Netto, Steuerklassen, Projektion —
in German and English, with the same explainability, accessibility and
local-first guarantees as the rest of the repository.

## Why a second front end

The engine is a `#![no_std]` Rust library compiled to WebAssembly and exposed as a
hand-written C ABI (`crates/casivell-wasm`). That ABI is framework-agnostic: it is
just exported functions returning `i32`/`i64`, so any JavaScript — including React —
can call it. This app is the same calculations in a component model, not a second
implementation of them.

## Layout

```
src/
├── wasm.ts             The C ABI, typed: the export signatures, the field/param
│                       constants, and the loader (with a MIME-type fallback).
├── i18n.ts             German and English strings; statutory terms stay German.
├── compute.ts          The three calculations as pure functions of (engine, inputs).
│                       Every conversion to cents / ppm happens here, nowhere else.
├── types.ts            Inputs and result types.
├── format.ts           The euro formatter (always de-DE, matching the payslip).
├── useWasm.ts          Loads the engine once and exposes loading/error state.
├── sw.ts               Registers the service worker (production builds only).
├── fields.tsx          The two field primitives (NumberField, SelectField).
├── Announce.tsx        The visually-hidden live region for screen readers.
├── App.tsx             The shell: language, theme, form switching, hero card
│                       layout, skeleton loading, and the trust bar.
├── hooks/              Tiny custom hooks.
│   ├── useAnimatedNumber.ts
│   └── useTheme.ts
└── forms/              Payslip, Classes, Projection, and the hand-drawn Chart.
```

## Build

The `.wasm` is produced by Cargo, not by npm. Copy the current artifact into
`public/` (already done), or rebuild it first:

```sh
npm run copy-wasm   # copy the existing target/…/release/casivell_wasm.wasm
npm run build:wasm  # cargo build the workspace, then copy it
```

Then:

```sh
npm install
npm run dev      # local development server
npm run build    # type-check (tsc) + production bundle into dist/
npm run preview  # serve the built dist/
```

`vite.config.ts` sets `base: "./"`, so `dist/` is relocatable — it works at a
domain root or under a sub-path without a rebuild.

## UI/UX

- **Hero cards** surface the headline figure — net pay, annual tax, or final
  wealth — immediately, with copy-to-clipboard on the payslip.
- **Animated numbers** interpolate when inputs change, making the connection
  between a changed value and its result visible.
- **Theme selector** lets the user override the system light/dark preference;
  the manual choice is persisted in `localStorage`.
- **Input adornments** (`€`, `%`) remove ambiguity about units.
- **Interactive chart** shows a tooltip on hover and an area fill under the
  enacted segment.
- **Skeleton screen** replaces the plain loading message while WASM boots.
- **Sticky header and tab bar** keep navigation reachable on long projection
  pages and small viewports.
- **Reduced-motion** media query disables animations for users who prefer it.

All of this is done with CSS custom properties and a few tiny hooks — no UI
library, no animation framework, no chart dependency.

## Offline

`public/sw.js` is a hand-written service worker with the same stale-while-revalidate
strategy as the static `web/` site: it serves the cached shell at once, revalidates
in the background, and posts an "update available" notice when the fresh copy
differs — rather than swapping figures underneath a reader mid-calculation. It is
registered only in a production build, so it never fights the dev server's hot
reload. The Datenstand footer names the statutory data a cached build rests on,
which is the guard that matters for a tax calculator.
