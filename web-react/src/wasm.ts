// The Casivell C ABI, typed.
//
// There is no bindings crate: the module is instantiated directly and its exports
// are plain numbers and BigInts. Money crosses the boundary in integer cents as
// `i64`, which WebAssembly's JavaScript interface presents as `BigInt`; rates
// cross in parts per million. The two conversions are one call each, and they
// live here so no component has to remember them.

export interface CasivellWasm {
  casivell_payslip(
    gross_cents: bigint,
    period: number,
    year: number,
    tax_class: number,
    land: number,
    age_years: number,
    children: number,
    is_parent: number,
    church: number,
    supplementary_rate_ppm: bigint,
  ): number;
  casivell_result(field: number): bigint;
  casivell_fingerprint(year: number): bigint;
  casivell_enacted_years(): bigint;
  casivell_compare_classes(
    first_cents: bigint,
    second_cents: bigint,
    year: number,
    land: number,
    age_years: number,
    children: number,
    is_parent: number,
    church: number,
    supplementary_rate_ppm: bigint,
  ): number;
  casivell_class_result(which: number, what: number): bigint;
  casivell_class_liability(): bigint;
  casivell_class_factor(): bigint;
  casivell_project_reset(): void;
  casivell_project_set(which: number, value: bigint): number;
  casivell_project_run(): number;
  casivell_project_years(): number;
  casivell_project_value(index: number, field: number): bigint;
}

// Field indices for `casivell_result`, mirroring `crates/casivell-wasm/src/lib.rs`.
export const FIELD = {
  GROSS: 0,
  INCOME_TAX: 1,
  SOLIDARITY: 2,
  CHURCH_TAX: 3,
  CONTRIBUTIONS: 4,
  NET: 5,
  PENSION: 6,
  HEALTH: 7,
  CARE: 8,
  UNEMPLOYMENT: 9,
  ANNUAL_GROSS: 10,
  TABLE_ALLOWANCES: 11,
  VORSORGEPAUSCHALE: 12,
  TAXABLE_ANNUAL: 13,
  ANNUAL_INCOME_TAX: 14,
  SURCHARGE_BASE: 15,
} as const;

// The three tax-class arrangements, for `casivell_class_result`.
export const ARR = { FOUR_FOUR: 0, THREE_FIVE: 1, FOUR_FACTOR: 2 } as const;

// Figures available per arrangement.
export const CF = {
  HIGHER: 0,
  LOWER: 1,
  WITHHOLDING: 2,
  NET: 3,
  SETTLEMENT: 4,
} as const;

// Projection parameters, for `casivell_project_set`.
export const PARAM = {
  GROSS: 0,
  YEAR: 1,
  TAX_CLASS: 2,
  LAND: 3,
  AGE: 4,
  CHILDREN: 5,
  IS_PARENT: 6,
  CHURCH: 7,
  SUPPLEMENTARY_RATE: 8,
  EXPENSES: 9,
  YEARS: 10,
  INVESTMENT_RETURN: 11,
  PAY_GROWTH: 12,
  INFLATION: 13,
  WAGE_GROWTH: 14,
} as const;

// Figures available per projection year, for `casivell_project_value`.
export const ROW = {
  YEAR: 0,
  GROSS: 1,
  NET: 2,
  SAVED: 3,
  WEALTH: 4,
  NET_WORTH: 5,
  PENSION_POINTS: 6,
  ACCRUED_PENSION: 7,
  IS_ENACTED: 8,
} as const;

// The sixteen states, in `Bundesland::ALL` order — the index the ABI expects.
export const LAENDER = [
  "Baden-Württemberg",
  "Bayern",
  "Berlin",
  "Brandenburg",
  "Bremen",
  "Hamburg",
  "Hessen",
  "Mecklenburg-Vorpommern",
  "Niedersachsen",
  "Nordrhein-Westfalen",
  "Rheinland-Pfalz",
  "Saarland",
  "Sachsen",
  "Sachsen-Anhalt",
  "Schleswig-Holstein",
  "Thüringen",
] as const;

let instance: CasivellWasm | null = null;

// `instantiateStreaming` needs the `.wasm` served as `application/wasm`; a plain
// `instantiate` over the bytes does not. The fallback keeps the app working on a
// host that mislabels the file, without changing the happy path.
async function instantiate(url: string): Promise<CasivellWasm> {
  try {
    const { instance: inst } = await WebAssembly.instantiateStreaming(fetch(url), {});
    return inst.exports as unknown as CasivellWasm;
  } catch {
    const bytes = await (await fetch(url)).arrayBuffer();
    const { instance: inst } = await WebAssembly.instantiate(bytes, {});
    return inst.exports as unknown as CasivellWasm;
  }
}

// Loads the module once and caches it. Idempotent, so React StrictMode's double
// effect in development does not instantiate it twice.
export async function loadWasm(): Promise<CasivellWasm> {
  if (instance) return instance;
  instance = await instantiate(import.meta.env.BASE_URL + "casivell_wasm.wasm");
  return instance;
}
