import { useMemo } from "react";
import { LAENDER, type CasivellWasm } from "../wasm";
import type { Messages } from "../i18n";
import { euro } from "../format";
import { computeClasses } from "../compute";
import type { Classes, ClassRow, Inputs } from "../types";
import { NumberField, SelectField } from "../fields";
import { Announce } from "../Announce";
import { useAnimatedNumber } from "../hooks/useAnimatedNumber";

interface Props {
  wasm: CasivellWasm;
  inputs: Inputs;
  patch: (p: Partial<Inputs>) => void;
  years: number[];
  t: Messages;
}

export function Classes({ wasm, inputs, patch, years, t }: Props) {
  const result = useMemo(() => computeClasses(wasm, inputs), [wasm, inputs]);
  const classes = result.ok ? result.classes : null;
  const liability = useAnimatedNumber(classes?.liability ?? 0);

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
        <NumberField
          label={t.fieldPartner}
          value={inputs.partner}
          min={0}
          step={1}
          adornment="€"
          onChange={(v) => patch({ partner: v })}
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
            <p className="label">{t.heroAnnualTax}</p>
            <p className="value">{euro(liability)}</p>
            <p className="subline">{t.heroAllThree}</p>
          </div>
          <ClassesTable classes={result.classes} t={t} />
        </>
      ) : (
        <p className="error">{t.errors[String(result.code)] ?? t.errors.unknown}</p>
      )}

      <Announce
        text={classes ? t.announceLiability(euro(classes.liability)) : ""}
      />
    </>
  );
}

function ClassesTable({ classes, t }: { classes: Classes; t: Messages }) {
  const settlement = (cents: number): string =>
    cents < 0 ? t.owes(euro(-cents)) : t.back(euro(cents));

  const row = (label: string, r: ClassRow) => (
    <tr>
      <th scope="row">{label}</th>
      <td className="num">{euro(r.higher)}</td>
      <td className="num">{euro(r.lower)}</td>
      <td className="num">{euro(r.net)}</td>
      <td className="num">{settlement(r.settlement)}</td>
    </tr>
  );

  const factorLabel = classes.factor
    ? `IV + Faktor 0,${String(classes.factor).padStart(3, "0")}`
    : "IV + Faktor";

  return (
    <>
      {/* The lede and the "why" prose are authored i18n strings, not user input. */}
      <p
        className="lede"
        dangerouslySetInnerHTML={{ __html: t.classesLede(euro(classes.liability)) }}
      />
      <div className="scroller">
        <table className="table">
          <caption className="vh">{t.captionClasses}</caption>
          <thead>
            <tr>
              <th scope="col">{t.colVariant}</th>
              <th scope="col">{t.colHigher}</th>
              <th scope="col">{t.colLower}</th>
              <th scope="col">{t.colNetTotal}</th>
              <th scope="col">{t.colAssessment}</th>
            </tr>
          </thead>
          <tbody>
            {row("IV / IV", classes.fourFour)}
            {row("III / V", classes.threeFive)}
            {row(factorLabel, classes.fourFactor)}
          </tbody>
        </table>
      </div>
      {!classes.factor && <p className="hint">{t.noFactor}</p>}
      <dl className="explain open">
        <div>
          <dt>{t.whySameTitle}</dt>
          <dd>{t.whySame}</dd>
          <dt>{t.whyMattersTitle}</dt>
          <dd dangerouslySetInnerHTML={{ __html: t.whyMatters }} />
        </div>
      </dl>
    </>
  );
}
