export type Lang = "de" | "en";
export type FormKey = "payslip" | "classes" | "project";

// Every input the three forms share, plus the ones only one form uses. The
// values are what the user typed: euros, years, percentages — not cents or ppm.
// Conversion to the ABI's units happens at the compute boundary.
export interface Inputs {
  gross: number;
  partner: number;
  taxClass: number;
  land: number;
  year: number;
  age: number;
  children: number;
  kvz: number;
  church: number;
  expenses: number;
  years: number;
  ret: number;
  paygrowth: number;
}

export const DEFAULT_INPUTS: Inputs = {
  gross: 5500,
  partner: 1800,
  taxClass: 1,
  land: 9, // Nordrhein-Westfalen, the most populous
  year: 2026,
  age: 30,
  children: 0,
  kvz: 2.9,
  church: 0,
  expenses: 2000,
  years: 40,
  ret: 4,
  paygrowth: 2,
};

// A payslip's figures, in cents.
export interface Payslip {
  gross: number;
  incomeTax: number;
  solidarity: number;
  churchTax: number;
  contributions: number;
  net: number;
  pension: number;
  health: number;
  care: number;
  unemployment: number;
  annualGross: number;
  tableAllowances: number;
  vorsorgepauschale: number;
  taxableAnnual: number;
  annualIncomeTax: number;
  surchargeBase: number;
}

export type PayslipResult = { ok: true; pay: Payslip } | { ok: false; code: number };

export interface ClassRow {
  higher: number;
  lower: number;
  withholding: number;
  net: number;
  settlement: number;
}

export interface Classes {
  liability: number;
  factor: number;
  fourFour: ClassRow;
  threeFive: ClassRow;
  fourFactor: ClassRow;
}

export type ClassesResult = { ok: true; classes: Classes } | { ok: false; code: number };

export interface ProjectionRow {
  year: number;
  gross: number;
  net: number;
  saved: number;
  wealth: number;
  netWorth: number;
  pensionPoints: number;
  accruedPension: number;
  enacted: boolean;
}

export type ProjectionResult =
  | { ok: true; rows: ProjectionRow[] }
  | { ok: false; code: number };
