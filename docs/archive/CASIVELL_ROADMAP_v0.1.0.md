# 🏛️ HAUSGELD — The Open Financial Universe for German Households
## A Privacy-First, WebAssembly-Powered Financial Simulation Engine

> **Context:** This document is the complete specification to build a next-generation financial planning tool that makes the universe turn and look back. Built with ❤️ to demonstrate world-class engineering and product thinking.

---

## 📋 EXECUTIVE SUMMARY

**HAUSGELD** (German: "House Money") is a radical reimagining of household financial simulation. Unlike existing tools, it combines:
- **WebAssembly compute engine** for 40-year Monte Carlo simulations in <100ms
- **Local-first architecture** — zero server, zero tracking, zero accounts required
- **German tax/pension law engine** compiled from official BMF (Bundesministerium der Finanzen) specifications
- **Visual scenario branching** — like Git for your financial life
- **AI-powered optimization** running entirely on-device

**Core Philosophy:** *"Your financial data never leaves your device. Not even we can see it."*

---

## 🎯 PRODUCT POSITIONING

| Dimension | Traditional Tools | HAUSGELD |
|-----------|------------------|----------|
| **Data** | Cloud-synced, analyzed | Never leaves device |
| **Speed** | Server round-trips | WASM instant calculations |
| **Accuracy** | Approximations | Official German law code |
| **Vision** | Retirement only | Entire life simulation |
| **Privacy** | "We care about privacy" | Cryptographically provable |

---

## 🏗️ ARCHITECTURE OVERVIEW

```
┌─────────────────────────────────────────────────────────────────┐
│                      PRESENTATION LAYER                          │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌────────────┐ │
│  │  React 19   │ │  D3.js      │ │  WebGL      │ │  PWA       │ │
│  │  (UI)       │ │(Visualize)  │ │(Charts)     │ │  (Offline) │ │
│  └─────────────┘ └─────────────┘ └─────────────┘ └────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│                      SIMULATION ENGINE                         │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │           Rust/WASM Core (40yr sim in <100ms)           │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐   │    │
│  │  │Tax Engine│ │Pension   │ │Monte     │ │Scenario  │   │    │
│  │  │(German)  │ │Calculator│ │Carlo     │ │Branching │   │    │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘   │    │
│  └─────────────────────────────────────────────────────────┘    │
├─────────────────────────────────────────────────────────────────┤
│                      DATA LAYER                                  │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌────────────┐ │
│  │  IndexedDB  │ │  LocalStorage│ │  OPFS       │ │  Export    │ │
│  │  (Scenarios)│ │  (Settings) │ │  (Large)    │ │  (JSON/CSV)│ │
│  └─────────────┘ └─────────────┘ └─────────────┘ └────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│                      AI LAYER (ON-DEVICE)                        │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐              │
│  │  ONNX Runtime│ │  TinyLlama  │ │  Optimization│              │
│  │  (Inference)│ │  (3B params)│ │  Engine      │              │
│  └─────────────┘ └─────────────┘ └─────────────┘              │
└─────────────────────────────────────────────────────────────────┘
```

---

## 📊 COMPLETE FEATURE MATRIX

### CORE SIMULATION ENGINE

| Feature | Description | Priority | Technical Approach |
|---------|-------------|----------|-------------------|
| **Multi-Scenario Branching** | Create unlimited "what-if" branches with visual diff | P0 | Git-inspired DAG in IndexedDB |
| **40-Year Projection** | Month-by-month financial timeline | P0 | Rust vectorized operations |
| **Real vs Nominal Toggle** | View all numbers in today's € or future € | P0 | Inflation-indexed data structures |
| **Monte Carlo Risk Analysis** | 10,000 simulation runs for uncertainty | P0 | WASM parallel processing |
| **Household Multi-User** | Track both partners + children | P0 | Entity-relationship graph |

### GERMAN TAX & SOCIAL SYSTEM (100% ACCURATE)

| Feature | Description | Data Source | Status |
|---------|-------------|-------------|--------|
| **Progressive Income Tax** | All 6 tax classes (I-VI), splitting benefit | BMF 2026 | Required |
| **Solidarity Surcharge** | Correct thresholds and exemptions | BMF 2026 | Required |
| **Church Tax** | Optional 8%/9% calculation per state | Official rates | Required |
| **Health Insurance** | Public (GKV) vs Private (PKV) comparison | GKV-Spitzenverband | Required |
| **Nursing Care Insurance** | Childless surcharge calculation | SGB V | Required |
| **Unemployment Insurance** | ALG I duration and amount | SGB III | Required |
| **Statutory Pension** | Exact Rentenpunkte calculation | Deutsche Rentenversicherung | Required |
| **Capital Gains Tax** | Abgeltungsteuer + Sparer-Pauschbetrag | BMF | Required |
| **Parental Benefits** | Elterngeld, Elterngeld Plus variants | BEEG | Required |
| **Child Benefits** | Kindergeld vs Kinderfreibetrag optimization | Familienkasse | Required |
| **BAföG Simulation** | Education funding impact | BMBF | P1 |
| **Hartz IV / Bürgergeld** | Social safety net floor calculation | SGB II | Required |

### LIFE EVENT SIMULATORS

| Event | What It Models | Complexity |
|-------|---------------|------------|
| **👶 Having Children** | Elterngeld, reduced income, Kita costs, future impact on pension | High |
| **🏠 Buying Property** | Closing costs, Grundsteuer, mortgage into retirement, opportunity cost vs ETF | High |
| **⏱️ Part-Time Work** | Net loss calculator, pension gap, "Teilzeitfalle" warning | Medium |
| **💼 Job Change** | ALG I bridge, notice period, signing bonus tax implications | Medium |
| **🌍 Moving Abroad** | Pension export, tax residency changes, Krankenversicherung abroad | High |
| **💔 Divorce/Separation** | Zugewinnausgleich, child support, splitting benefit loss | High |
| **🏥 Critical Illness** | Berufsunfähigkeit, reduced earning capacity pension | Medium |
| **📚 Further Education** | Opportunity cost, BAföG, delayed pension entries | Medium |
| **🎓 University Costs** | Kinder studying, BaföG for them, support payments | Medium |
| **🌴 Sabbatical** | Unpaid leave, pension gaps, savings runway | Low |
| **🚀 Starting Business** | Gründungszuschuss, Künstlersozialkasse, volatile income | High |
| **👵 Early Retirement** | Pension deductions, withdrawal strategies, 4% rule adaptation | High |

### VISUALIZATION & UX

| Feature | Description |
|---------|-------------|
| **🌌 Galaxy Timeline View** | Your life as a scrollable universe, income as stars, expenses as black holes |
| **🔀 Scenario Comparison** | Side-by-side or overlay comparison of any two branches |
| **📱 Mobile-First** | Touch-optimized, works offline as PWA |
| **🎨 Dark/Light Modes** | Automatic system preference respect |
| **♿ WCAG AAA** | Full accessibility compliance |
| **🌍 i18n Ready** | German first, English expansion ready |

### ADVANCED FEATURES

| Feature | Description | Tech |
|---------|-------------|------|
| **AI Financial Advisor** | On-device LLM suggesting optimizations | TinyLlama + ONNX |
| **ETF Portfolio Simulator** | Monte Carlo on S&P 500, MSCI World historical returns | Historical data |
| **Immigration Path** | Blue Card, Permanent Residency financial requirements | Visa rules |
| **Steuererklärung Helper** | Estimate tax refunds throughout the year | BMF forms |
| **Vermögensaufbau Optimizer** | Optimal order: Debt → EF → ETF → Property? | Mathematical optimization |
| **Fire Calculator** | Financial Independence, Retire Early with German specifics | 4% rule adapted |
| **Erb-Simulator** | Inheritance tax, Schenkung, Pflichtteil calculations | ErbStG |

---

## 🔢 CALCULATION ENGINE SPECIFICATIONS

### Tax Calculation (German 2026)

```rust
// Progressive tax formula (§ 32a EStG)
pub fn calculate_income_tax(zv_e: f64, tax_class: TaxClass) -> TaxResult {
    // Grundfreibetrag 2026: €12,096
    // Thresholds: €17,643 | €28,397 | €392,782
    // Formula segments with exact BMF coefficients
    
    let tax = match zv_e {
        0.0..=12_096.0 => 0.0,
        12_096.01..=17_643.0 => {
            let y = (zv_e - 12_096.0) / 10_000.0;
            (922.98 * y + 1_400.0) * y
        }
        17_643.01..=28_397.0 => {
            let z = (zv_e - 17_643.0) / 10_000.0;
            (181.19 * z + 2_397.0) * z + 1_035.0
        }
        28_397.01..=392_782.0 => {
            0.42 * zv_e - 10_397.0
        }
        _ => 0.45 * zv_e - 22_228.0,
    };
    
    // Solidarity surcharge (5.5% on tax, with Freigrenze)
    let solidarity = if tax <= 18_720.0 / 12.0 {
        0.0
    } else {
        (tax * 0.055).min(tax * 0.055 - (11_784.0 - tax * 0.75) * 0.055)
    };
    
    // Church tax (8% or 9% of income tax)
    let church = tax * church_tax_rate;
    
    TaxResult { income_tax: tax, solidarity, church, total: tax + solidarity + church }
}
```

### Pension Points (Rentenpunkte)

```rust
// Deutsche Rentenversicherung calculation
pub fn calculate_pension_points(annual_income: f64, year: u32) -> f64 {
    // Beitragsbemessungsgrenze 2026 West: €96,600/year
    // Beitragsbemessungsgrenze 2026 East: €93,600/year
    // Durchschnittsentgelt 2026: €43,142
    
    let contribution_ceiling = if is_west_germany { 96_600.0 } else { 93_600.0 };
    let average_earnings = 43_142.0;
    
    let relevant_income = annual_income.min(contribution_ceiling);
    relevant_income / average_earnings
}

// Monthly pension at retirement
pub fn calculate_monthly_pension(total_points: f64, retirement_year: u32) -> f64 {
    // Aktueller Rentenwert 2026 West: €39.32
    // Aktueller Rentenwert 2026 East: €38.44
    let rentenwert = get_rentenwert(retirement_year, region);
    total_points * rentenwert
}
```

### Health Insurance (GKV)

```rust
// Gesetzliche Krankenversicherung calculation
pub fn calculate_gkv_contribution(gross_salary: f64, age: u32, children: u32) -> f64 {
    // Beitragsbemessungsgrenze 2026: €5,512.50/month
    // Allgemeiner Beitragssatz: 14.6%
    // Zusatzbeitrag (average): 1.7%
    // Pflegeversicherung: 3.4% (3.4% + 0.6% for childless >23)
    
    let assessment_ceiling = 5_512.50;
    let relevant_income = gross_salary.min(assessment_ceiling);
    
    let kv_rate = 0.146 + zusatzbeitrag; // Employer pays half
    let pv_rate = 0.034 + if age >= 23 && children == 0 { 0.006 } else { 0.0 };
    
    let total_rate = (kv_rate + pv_rate) / 2.0; // Employee pays half
    relevant_income * total_rate
}
```

### Parental Benefits (Elterngeld)

```rust
pub fn calculate_elterngeld(
    net_pre_birth: f64,
    months_parent_leave: u32,
    variant: ElterngeldVariant
) -> Vec<f64> {
    // Basiselterngeld: 65-100% of net, min €300, max €1,800
    // Elterngeld Plus: 50% of basis, up to 24 months
    // Partnership Bonus: +10% each if both take 2-4 months concurrent
    
    let basis = net_pre_birth * 0.65; // Simplified
    let min_amount = 300.0;
    let max_amount = 1_800.0;
    
    let monthly = basis.clamp(min_amount, max_amount);
    
    vec![monthly; months_parent_leave as usize]
}
```

---

## 🚀 MVP ROADMAP

### Phase 1: Foundation (Weeks 1-3)
**Goal:** Core engine working in browser

- [ ] **Week 1:** Rust/WASM project setup, basic tax calculation
- [ ] **Week 2:** Pension system implementation, data structures
- [ ] **Week 3:** Health insurance, unemployment, integration tests

**Deliverable:** `npm run test:wasm` passes all German tax law test cases

### Phase 2: UI Foundation (Weeks 4-6)
**Goal:** User can input data and see basic projections

- [ ] **Week 4:** React 19 + Vite setup, Tailwind v4, form inputs
- [ ] **Week 5:** WASM integration, basic timeline visualization
- [ ] **Week 6:** PWA setup, offline persistence (Dexie.js)

**Deliverable:** Can simulate single income + expenses → 40 year projection

### Phase 3: Life Events (Weeks 7-10)
**Goal:** Handle real-world complexity

- [ ] **Week 7:** Dual income household, tax class optimization
- [ ] **Week 8:** Children simulation (Elterngeld, Kita costs)
- [ ] **Week 9:** Property purchase vs rent comparison
- [ ] **Week 10:** Part-time work, job change scenarios

**Deliverable:** Can model 80% of real German household decisions

### Phase 4: Visualization Polish (Weeks 11-13)
**Goal:** Stunning, shareable visualizations

- [ ] **Week 11:** D3.js timeline, scenario branching UI
- [ ] **Week 12:** Comparison views, export to PDF/image
- [ ] **Week 13:** Mobile optimization, animations, micro-interactions

**Deliverable:** Product Hunt ready launch

### Phase 5: Intelligence (Weeks 14-16)
**Goal:** AI-powered insights (all on-device)

- [ ] **Week 14:** TinyLlama integration via ONNX
- [ ] **Week 15:** Natural language scenario creation
- [ ] **Week 16:** Optimization suggestions, "What should I do?" mode

**Deliverable:** Can ask "Should I buy or rent?" and get data-driven answer

---

## 🛠️ TECH STACK

```yaml
Core Engine:
  Language: Rust 1.82+
  Target: WebAssembly (wasm32-unknown-unknown)
  Math: nalgebra, rand for Monte Carlo
  Testing: wasm-bindgen-test, property-based testing

Frontend:
  Framework: React 19 (Canary with Server Actions)
  Build: Vite 6
  Styling: Tailwind CSS v4
  State: Zustand + Immer
  Forms: React Hook Form + Zod
  Charts: D3.js + visx
  Animation: Framer Motion

Persistence:
  Database: Dexie.js (IndexedDB wrapper)
  Large Files: Origin Private File System
  Encryption: SubtleCrypto API (AES-GCM)

AI (On-Device):
  Runtime: ONNX Runtime Web
  Model: TinyLlama 1.1B (Q4 quantized)
  Embeddings: all-MiniLM-L6-v2 (onnx)

Testing:
  Unit: Vitest
  E2E: Playwright
  Visual: Chromatic
  WASM: wasm-bindgen-test

CI/CD:
  Platform: GitHub Actions
  Deploy: Cloudflare Pages (edge, free)
  Analytics: Plausible (privacy-focused, self-hosted optional)

Data Sources:
  Tax Law: BMF XML Schnittstellen
  Pension: Deutsche Rentenversicherung Excel-Files
  Inflation: Bundesbank API
```

---

## 🌟 DIFFERENTIATORS (Why This Will Make The Universe Turn)

### 1. **Zero-Knowledge Architecture**
Not just "we don't store your data" but mathematically provable via code audit. All source open, WASM compiled deterministically.

### 2. **Simulation as Code**
Scenarios are stored as functional programs. Users can:
- Fork scenarios (like Git)
- Merge scenarios
- See visual diffs between decisions
- Share scenarios without sharing data

### 3. **German Law as Code**
Tax calculations verified against official BMF test cases. Monthly updates when laws change. Community-auditable.

### 4. **Edge AI**
First financial tool with on-device LLM. No API keys, no rate limits, complete privacy. Runs on M1 MacBook Air at 20 tokens/sec.

### 5. **Open Source Core**
Engine is Apache-2.0 licensed. Anyone can verify calculations. Premium for advanced visualizations, scenario marketplace.

---

## 📈 BUSINESS MODEL (Privacy-Compatible)

| Tier | Price | Features |
|------|-------|----------|
| **Essential** | Free | All calculations, 3 scenarios, basic viz |
| **Professional** | €5/mo | Unlimited scenarios, AI advisor, PDF reports |
| **Advisor** | €49/mo | Client management, white-label, scenario sharing |

**Revenue without tracking:**
- No user data sold (obviously)
- No analytics on behavior
- Stripe payment only (they handle PCI)
- Optional: Scenario marketplace (user-created, revenue share)

---

## 🎯 SUCCESS METRICS

| Metric | Target |
|--------|--------|
| **Calculation Accuracy** | 100% match with BMF reference cases |
| **Performance** | 40-year sim < 100ms on M1 Mac |
| **Bundle Size** | < 500KB WASM + JS gzipped |
| **Lighthouse** | 100 performance, 100 accessibility |
| **Offline Capability** | 100% functionality without network |

---

## 🗂️ PROJECT STRUCTURE

```
hausgeld/
├── engine/                    # Rust/WASM core
│   ├── src/
│   │   ├── tax/              # German tax law
│   │   ├── pension/          # Rentenversicherung
│   │   ├── insurance/        # GKV, PKV, Pflegeversicherung
│   │   ├── simulation/       # Monte Carlo, projections
│   │   └── lib.rs
│   ├── tests/                # Property-based tests
│   └── Cargo.toml
├── web/                       # React frontend
│   ├── src/
│   │   ├── components/       # UI components
│   │   ├── scenes/          # Main views
│   │   ├── stores/          # Zustand state
│   │   ├── wasm/            # WASM bindings
│   │   └── App.tsx
│   ├── public/
│   └── package.json
├── data/                      # Reference data
│   ├── tax/
│   ├── pension/
│   └── inflation/
├── docs/                      # Documentation
└── README.md
```

---

## 🎬 IMPLEMENTATION CHECKLIST

### Week 1-2: Bootstrap
- [ ] `cargo new --lib engine`
- [ ] `npm create vite@latest web -- --template react-ts`
- [ ] Setup wasm-pack build pipeline
- [ ] GitHub Actions for CI
- [ ] Write first tax test (Grundfreibetrag 2026)

### Week 3-4: Core Engine
- [ ] Progressive tax function (§ 32a EStG)
- [ ] Solidarity surcharge calculation
- [ ] Church tax (configurable by state)
- [ ] All 6 tax classes
- [ ] Pension points calculation
- [ ] Health insurance (GKV/PKV)

### Week 5-6: Web UI
- [ ] Income input form
- [ ] Expense categories
- [ ] Basic timeline chart
- [ ] Year-by-year breakdown
- [ ] Export to JSON

### Week 7-8: Advanced Scenarios
- [ ] Dual income
- [ ] Children (Elterngeld, Kindergeld)
- [ ] Property vs rent
- [ ] Part-time work

### Week 9-10: Polish
- [ ] Scenario branching UI
- [ ] Comparison views
- [ ] Mobile optimization
- [ ] PWA features

### Week 11-12: Launch Prep
- [ ] Performance optimization
- [ ] Accessibility audit
- [ ] Documentation
- [ ] Product Hunt listing

---

## 💡 WHY THIS IMPRESSES AGATA (Miravel Founder)

1. **Technical Depth:** Shows understanding of the hard problems (German tax law, WASM, privacy)
2. **Product Vision:** Goes beyond current Miravel with AI, branching, open source
3. **Privacy-First:** Aligns with her core values ("privacy from first line of code")
4. **Attention to Detail:** 100% accurate calculations, WCAG AAA, mathematical proofs
5. **Entrepreneurial Thinking:** Clear business model without compromising ethics
6. **Creativity:** "Galaxy view", "Git for finances", "Edge AI" — unexpected combinations
7. **German Market Understanding:** Deep knowledge of specific regulations she built around

---

## 🔗 REFERENCES

- [BMF Steuerliche Einzelanweisungen 2026](https://www.bundesfinanzministerium.de/)
- [Deutsche Rentenversicherung Rechengrößen](https://www.deutsche-rentenversicherung.de/)
- [GKV-Spitzenverband Beitragssätze](https://www.gkv-spitzenverband.de/)
- [Rust WASM Book](https://rustwasm.github.io/book/)
- [Local-First Software](https://www.inkandswitch.com/local-first/)

---

## 📝 NOTES FOR CLAUDE CODE

When implementing this:
1. **Start with the engine** — Rust/WASM is the hardest part, get it right first
2. **Use property-based testing** — Generate random inputs, verify invariants
3. **Keep data local** — IndexedDB only, no external APIs for personal data
4. **Optimize for bundle size** — WASM can bloat quickly, use wee_alloc, strip symbols
5. **Test against reference** — BMF provides test cases for tax calculations
6. **Document the math** — Every formula needs a comment linking to the law paragraph

---

**Built with precision. Engineered for privacy. Designed for German households.**

*"Gleiche Eingaben, gleiches Ergebnis, immer."* — Same inputs, same result, always.

---

**License:** Apache 2.0 (Engine) / MIT (Web UI)
**Author:** [Your Name] — Building this to work with the brightest minds in fintech
**Version:** 0.1.0 — The beginning of something extraordinary
