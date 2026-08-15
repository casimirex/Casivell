import { useMemo } from "react";
import { LAENDER, type CasivellWasm } from "../wasm";
import type { Messages } from "../i18n";
import { euro } from "../format";
import { computeProjection } from "../compute";
import type { Inputs, ProjectionRow } from "../types";
import { NumberField, SelectField, TAX_CLASS_OPTIONS } from "../fields";
import { Announce } from "../Announce";
import { Chart } from "./Chart";
import { useAnimatedNumber } from "../hooks/useAnimatedNumber";

interface Props {
  wasm: CasivellWasm;
  inputs: Inputs;
  patch: (p: Partial<Inputs>) => void;
  years: number[];
  t: Messages;
}

export function Projection({ wasm, inputs, patch, years, t }: Props) {
  const result = useMemo(() => computeProjection(wasm, inputs), [wasm, inputs]);
  const rows = result.ok ? result.rows : null;
  const last = rows ? rows[rows.length - 1] : null;
  const finalWealth = useAnimatedNumber(last?.wealth ?? 0);
  const finalPension = useAnimatedNumber(last?.accruedPension ?? 0);

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
        <NumberField
          label={t.fieldExpenses}
          value={inputs.expenses}
          min={0}
          step={50}
          adornment="€"
          onChange={(v) => patch({ expenses: v })}
        />
        <NumberField
          label={t.fieldYears}
          value={inputs.years}
          min={1}
          max={70}
          onChange={(v) => patch({ years: v })}
        />
        <NumberField
          label={t.fieldReturn}
          value={inputs.ret}
          min={0}
          max={20}
          step={0.5}
          adornment="%"
          onChange={(v) => patch({ ret: v })}
        />
        <NumberField
          label={t.fieldPayGrowth}
          value={inputs.paygrowth}
          min={0}
          max={20}
          step={0.5}
          adornment="%"
          onChange={(v) => patch({ paygrowth: v })}
        />
      </form>

      {result.ok ? (
        <>
          <div className="hero">
            <p className="label">{t.heroFinalWealth}</p>
            <p className="value">{euro(finalWealth)}</p>
            <div className="hero-grid">
              <div className="kpi">
                <span className="kpi-label">{t.heroYears}</span>
                <span className="kpi-value">{inputs.years}</span>
              </div>
              <div className="kpi">
                <span className="kpi-label">{t.heroPension}</span>
                <span className="kpi-value">{euro(finalPension)}</span>
              </div>
            </div>
          </div>
          <ProjectionView rows={result.rows} t={t} />
        </>
      ) : (
        <p className="error">{t.errors[String(result.code)] ?? t.errors.unknown}</p>
      )}

      <Announce
        text={
          rows
            ? t.announceProjection(
                rows.length,
                euro(rows[rows.length - 1].wealth),
                euro(rows[rows.length - 1].accruedPension),
              )
            : ""
        }
      />
    </>
  );
}

function ProjectionView({ rows, t }: { rows: ProjectionRow[]; t: Messages }) {
  const last = rows[rows.length - 1];
  const peak = rows.reduce((best, r) => (r.wealth > best.wealth ? r : best), rows[0]);
  const turns = peak.year !== last.year;
  const lastEnacted = rows.filter((r) => r.enacted).at(-1)?.year ?? "?";
  const maxWealth = Math.max(...rows.map((r) => r.wealth), 1);

  return (
    <>
      <Chart
        series={rows}
        label={t.chartLabel(rows[0].year, last.year, euro(maxWealth))}
        caption={t.chartCaption}
      />
      <div className="scroller">
        <table className="table">
          <caption className="vh">{t.captionProject}</caption>
          <thead>
            <tr>
              <th scope="col">{t.colYear}</th>
              <th scope="col">{t.colNetMonthly}</th>
              <th scope="col">{t.colSaved}</th>
              <th scope="col">{t.colWealth}</th>
              <th scope="col">{t.colPension}</th>
            </tr>
          </thead>
          <tbody>
            {rows
              .filter((_, i) => i % 5 === 0 || i === rows.length - 1)
              .map((r) => (
                <tr key={r.year}>
                  <th scope="row">
                    {r.year}
                    {r.enacted ? "" : " *"}
                  </th>
                  <td className="num">{euro(r.net)}</td>
                  <td className="num">{euro(r.saved)}</td>
                  <td className="num">{euro(r.wealth)}</td>
                  <td className="num">{euro(r.accruedPension)}</td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
      <dl className="explain open">
        <div>
          <dt>{t.whatsHereTitle}</dt>
          <dd dangerouslySetInnerHTML={{ __html: t.whatsHere(lastEnacted) }} />
          {turns && (
            <>
              <dt>{t.turnsTitle}</dt>
              <dd>{t.turns(euro(peak.wealth), peak.year)}</dd>
            </>
          )}
          <dt>{t.missingTitle}</dt>
          <dd>{t.missing}</dd>
        </div>
      </dl>
    </>
  );
}
