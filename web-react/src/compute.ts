// The three calculations, as pure functions of the engine and the inputs.
//
// Every conversion to the ABI's units happens here: euros to cents, percentages
// to parts per million, and the `i64` results back to `Number`. Nothing is
// recomputed in JavaScript — a second implementation would be a second thing to
// be wrong.

import { ARR, CF, FIELD, PARAM, ROW, type CasivellWasm } from "./wasm";
import type {
  Classes,
  ClassesResult,
  Inputs,
  Payslip,
  PayslipResult,
  ProjectionResult,
  ProjectionRow,
} from "./types";

const cents = (euros: number): bigint => BigInt(Math.round(euros * 100));
const ppm = (percent: number): bigint => BigInt(Math.round(percent * 10_000));

export function computePayslip(wasm: CasivellWasm, inputs: Inputs): PayslipResult {
  const children = inputs.children;
  const code = wasm.casivell_payslip(
    cents(inputs.gross),
    0,
    inputs.year,
    inputs.taxClass,
    inputs.land,
    inputs.age,
    children,
    children > 0 ? 1 : 0, // children imply Elterneigenschaft
    inputs.church,
    ppm(inputs.kvz),
  );
  if (code !== 0) return { ok: false, code };

  const get = (f: number): number => Number(wasm.casivell_result(f));
  const pay: Payslip = {
    gross: get(FIELD.GROSS),
    incomeTax: get(FIELD.INCOME_TAX),
    solidarity: get(FIELD.SOLIDARITY),
    churchTax: get(FIELD.CHURCH_TAX),
    contributions: get(FIELD.CONTRIBUTIONS),
    net: get(FIELD.NET),
    pension: get(FIELD.PENSION),
    health: get(FIELD.HEALTH),
    care: get(FIELD.CARE),
    unemployment: get(FIELD.UNEMPLOYMENT),
    annualGross: get(FIELD.ANNUAL_GROSS),
    tableAllowances: get(FIELD.TABLE_ALLOWANCES),
    vorsorgepauschale: get(FIELD.VORSORGEPAUSCHALE),
    taxableAnnual: get(FIELD.TAXABLE_ANNUAL),
    annualIncomeTax: get(FIELD.ANNUAL_INCOME_TAX),
    surchargeBase: get(FIELD.SURCHARGE_BASE),
  };
  return { ok: true, pay };
}

export function computeClasses(wasm: CasivellWasm, inputs: Inputs): ClassesResult {
  const children = inputs.children;
  const code = wasm.casivell_compare_classes(
    cents(inputs.gross),
    cents(inputs.partner),
    inputs.year,
    inputs.land,
    inputs.age,
    children,
    children > 0 ? 1 : 0,
    inputs.church,
    ppm(inputs.kvz),
  );
  if (code !== 0) return { ok: false, code };

  const g = (arr: number, f: number): number => Number(wasm.casivell_class_result(arr, f));
  const row = (arr: number) => ({
    higher: g(arr, CF.HIGHER),
    lower: g(arr, CF.LOWER),
    withholding: g(arr, CF.WITHHOLDING),
    net: g(arr, CF.NET),
    settlement: g(arr, CF.SETTLEMENT),
  });

  const classes: Classes = {
    liability: Number(wasm.casivell_class_liability()),
    factor: Number(wasm.casivell_class_factor()),
    fourFour: row(ARR.FOUR_FOUR),
    threeFive: row(ARR.THREE_FIVE),
    fourFactor: row(ARR.FOUR_FACTOR),
  };
  return { ok: true, classes };
}

export function computeProjection(wasm: CasivellWasm, inputs: Inputs): ProjectionResult {
  const children = inputs.children;
  wasm.casivell_project_reset();
  const set = (p: number, v: number): void => {
    wasm.casivell_project_set(p, BigInt(Math.round(v)));
  };
  set(PARAM.GROSS, inputs.gross * 100);
  set(PARAM.YEAR, inputs.year);
  set(PARAM.TAX_CLASS, inputs.taxClass);
  set(PARAM.LAND, inputs.land);
  set(PARAM.AGE, inputs.age);
  set(PARAM.CHILDREN, children);
  set(PARAM.IS_PARENT, children > 0 ? 1 : 0);
  set(PARAM.CHURCH, inputs.church);
  set(PARAM.SUPPLEMENTARY_RATE, inputs.kvz * 10_000);
  set(PARAM.EXPENSES, inputs.expenses * 100);
  set(PARAM.YEARS, inputs.years);
  set(PARAM.INVESTMENT_RETURN, inputs.ret * 10_000);
  set(PARAM.PAY_GROWTH, inputs.paygrowth * 10_000);

  const code = wasm.casivell_project_run();
  if (code !== 0) return { ok: false, code };

  const n = wasm.casivell_project_years();
  const at = (i: number, f: number): number => Number(wasm.casivell_project_value(i, f));
  const rows: ProjectionRow[] = Array.from({ length: n }, (_, i) => ({
    year: at(i, ROW.YEAR),
    gross: at(i, ROW.GROSS),
    net: at(i, ROW.NET),
    saved: at(i, ROW.SAVED),
    wealth: at(i, ROW.WEALTH),
    netWorth: at(i, ROW.NET_WORTH),
    pensionPoints: at(i, ROW.PENSION_POINTS),
    accruedPension: at(i, ROW.ACCRUED_PENSION),
    enacted: at(i, ROW.IS_ENACTED) === 1,
  }));
  return { ok: true, rows };
}
