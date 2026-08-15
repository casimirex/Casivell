// Always German formatting, in both languages. Every document these figures are
// compared against uses 1.234,56 € — matching the payslip matters more than
// matching the reader's numeric convention.
export const euro = (cents: number): string =>
  (cents / 100).toLocaleString("de-DE", { style: "currency", currency: "EUR" });
