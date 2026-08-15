import { useMemo, useState } from "react";
import { FIELD, LAENDER, type CasivellWasm } from "../wasm";
import { TERMS, type Messages } from "../i18n";
import { euro } from "../format";
import { computePayslip } from "../compute";
import type { Inputs, Payslip } from "../types";
import { NumberField, SelectField, TAX_CLASS_OPTIONS } from "../fields";
import { Announce } from "../Announce";
import { useAnimatedNumber } from "../hooks/useAnimatedNumber";

interface Props {
  wasm: CasivellWasm;
  inputs: Inputs;
  patch: (p: Partial<Inputs>) => void;
  years: number[];
  t: Messages;
}

export function Payslip({ wasm, inputs, patch, years, t }: Props) {
  const [open, setOpen] = useState<number | null>(null);
  const [copied, setCopied] = useState(false);
  const result = useMemo(() => computePayslip(wasm, inputs), [wasm, inputs]);

  const pay = result.ok ? result.pay : null;
  const net = useAnimatedNumber(pay?.net ?? 0);
  const gross = useAnimatedNumber(pay?.gross ?? 0);
  const deductions = useAnimatedNumber(pay ? pay.gross - pay.net : 0);

  const copy = () => {
    if (!pay) return;
    const text = `${TERMS.net}: ${euro(pay.net)} · ${TERMS.gross}: ${euro(pay.gross)} · ${t.heroDeductions}: ${euro(pay.gross - pay.net)}`;
    navigator.clipboard.writeText(text).catch(() => {
      // Clipboard may be unavailable; ignore silently.
    });
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <>
      <form className="form" onSubmit={(e) => e.preventDefault()}>
        <NumberField
          label={t.fieldGross}
          value={inputs.gross}
          min={0}
          step={1}
          adornment="€"
          onChange={(v) => patch({ gross: v })}
        />
        <SelectField
          label={t.fieldClass}
          value={inputs.taxClass}
          onChange={(v) => patch({ taxClass: v })}
          options={TAX_CLASS_OPTIONS}
        />
        <SelectField
          label={t.fieldLand}
          value={inputs.land}
          onChange={(v) => patch({ land: v })}
          options={LAENDER.map((name, i) => ({ value: i, label: name }))}
        />
        <SelectField
          label={t.fieldYear}
          value={inputs.year}
          onChange={(v) => patch({ year: v })}
          options={years.map((y) => ({ value: y, label: String(y) }))}
        />
        <NumberField
          label={t.fieldAge}
          value={inputs.age}
          min={0}
          max={120}
          onChange={(v) => patch({ age: v })}
        />
        <NumberField
          label={t.fieldChildren}
          value={inputs.children}
          min={0}
          max={20}
          onChange={(v) => patch({ children: v })}
        />
        <NumberField
          label={t.fieldKvz}
          value={inputs.kvz}
          min={0}
          max={10}
          step={0.1}
          adornment="%"
          onChange={(v) => patch({ kvz: v })}
        />
        <SelectField
          label={t.fieldChurch}
          value={inputs.church}
          onChange={(v) => patch({ church: v })}
          options={[
            { value: 0, label: t.no },
            { value: 1, label: t.yes },
          ]}
        />
      </form>

      {result.ok ? (
        <>
          <div className="hero">
            <p className="label">{t.heroNet}</p>
            <p className="value">{euro(net)}</p>
            <div className="hero-grid">
              <div className="kpi">
                <span className="kpi-label">{t.heroGross}</span>
                <span className="kpi-value">{euro(gross)}</span>
              </div>
              <div className="kpi">
                <span className="kpi-label">{t.heroDeductions}</span>
                <span className="kpi-value">{euro(deductions)}</span>
              </div>
            </div>
            <div className="hero-actions">
              <button type="button" onClick={copy}>
                {copied ? t.copied : t.copyResult}
              </button>
            </div>
          </div>

          <PayslipTable pay={result.pay} t={t} open={open} setOpen={setOpen} />
        </>
      ) : (
        <p className="error">{t.errors[String(result.code)] ?? t.errors.unknown}</p>
      )}

      <Announce
        text={pay ? t.announceNet(euro(pay.net), euro(pay.gross)) : ""}
      />
    </>
  );
}

interface Row {
  field: number;
  label: string;
  value: number;
  why?: string;
  cls?: string;
}

interface TableProps {
  pay: Payslip;
  t: Messages;
  open: number | null;
  setOpen: (field: number | null) => void;
}

function PayslipTable({ pay, t, open, setOpen }: TableProps) {
  const rows: Row[] = [
    { field: FIELD.GROSS, label: TERMS.gross, value: pay.gross },
    { field: FIELD.INCOME_TAX, label: TERMS.incomeTax, value: pay.incomeTax, why: "incomeTax" },
    { field: FIELD.SOLIDARITY, label: TERMS.solidarity, value: pay.solidarity, why: "solidarity" },
    ...(pay.churchTax
      ? [{ field: FIELD.CHURCH_TAX, label: TERMS.churchTax, value: pay.churchTax, why: "churchTax" }]
      : []),
    {
      field: FIELD.CONTRIBUTIONS,
      label: TERMS.contributions,
      value: pay.contributions,
      why: "contributions",
    },
    { field: FIELD.PENSION, label: TERMS.pension, value: pay.pension, why: "pension", cls: "sub" },
    { field: FIELD.HEALTH, label: TERMS.health, value: pay.health, why: "health", cls: "sub" },
    { field: FIELD.CARE, label: TERMS.care, value: pay.care, why: "care", cls: "sub" },
    {
      field: FIELD.UNEMPLOYMENT,
      label: TERMS.unemployment,
      value: pay.unemployment,
      why: "unemployment",
      cls: "sub",
    },
    { field: FIELD.NET, label: TERMS.net, value: pay.net, why: "net", cls: "total" },
  ];

  const openRow = rows.find((r) => r.field === open);

  return (
    <>
      <table className="table">
        <caption className="vh">{t.captionPayslip}</caption>
        <tbody>
          {rows.map((r) => (
            <tr key={r.field} className={r.cls ?? ""}>
              <td>
                {r.why ? (
                  <button
                    type="button"
                    className="why"
                    aria-expanded={open === r.field}
                    onClick={() => setOpen(open === r.field ? null : r.field)}
                  >
                    <span>{r.label}</span>
                    <span className="pill" aria-hidden="true">?</span>
                    <span className="vh">{t.explainShow}</span>
                  </button>
                ) : (
                  r.label
                )}
              </td>
              <td>{euro(r.value)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="hint">{t.explainHint}</p>
      <dl className={`explain ${openRow?.why ? "open" : ""}`}>
        <div>
          {openRow?.why && (
            <>
              <dt>
                {openRow.label} — {t.why[openRow.why].law}
              </dt>
              <dd>{t.why[openRow.why].text}</dd>
              {openRow.field === FIELD.INCOME_TAX && (
                <dd>
                  <Chain pay={pay} t={t} />
                </dd>
              )}
            </>
          )}
        </div>
      </dl>
    </>
  );
}

// The § 39b chain, shown when the Lohnsteuer line is opened. Every number is the
// engine's; nothing here is recomputed in JavaScript.
function Chain({ pay, t }: { pay: Payslip; t: Messages }) {
  return (
    <div className="chain">
      <span>
        {TERMS.annualGross} <code>ZRE4</code>
      </span>
      <span>{euro(pay.annualGross)}</span>
      <span>
        − {TERMS.allowances} <code>ZTABFB</code>
      </span>
      <span>−{euro(pay.tableAllowances)}</span>
      <span>
        − {TERMS.vorsorge} <code>VSP</code>
      </span>
      <span>−{euro(pay.vorsorgepauschale)}</span>
      <span className="rule">
        = {TERMS.taxable} <code>ZVE</code>
      </span>
      <span>{euro(pay.taxableAnnual)}</span>
      <span>
        {TERMS.annualTax} <code>LSTJAHR</code>
      </span>
      <span>{euro(pay.annualIncomeTax)}</span>
      <span className="rule">{t.perMonth}</span>
      <span>{euro(pay.incomeTax)}</span>
    </div>
  );
}
