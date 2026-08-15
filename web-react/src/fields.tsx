// The two field primitives every form is built from. Each is a real `<label>`
// wrapping its control, so the label is associated without an `id` and the
// control is reachable by keyboard and named for a screen reader.

interface Option {
  value: number;
  label: string;
}

interface NumberFieldProps {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  adornment?: string;
}

export function NumberField({
  label,
  value,
  onChange,
  min,
  max,
  step,
  adornment,
}: NumberFieldProps) {
  const input = (
    <input
      type="number"
      value={value}
      min={min}
      max={max}
      step={step}
      onChange={(e) => {
        // An emptied field reports NaN; treat it as zero rather than letting
        // NaN reach the engine.
        const v = e.target.valueAsNumber;
        onChange(Number.isFinite(v) ? v : 0);
      }}
    />
  );

  return (
    <label className="field">
      <span>{label}</span>
      {adornment ? (
        <div className="adorned">
          <span className="symbol" aria-hidden="true">
            {adornment}
          </span>
          {input}
        </div>
      ) : (
        input
      )}
    </label>
  );
}

interface SelectFieldProps {
  label: string;
  value: number;
  onChange: (value: number) => void;
  options: Option[];
}

export function SelectField({ label, value, onChange, options }: SelectFieldProps) {
  return (
    <label className="field">
      <span>{label}</span>
      <select value={value} onChange={(e) => onChange(Number(e.target.value))}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}

// The six Lohnsteuerklassen, as Roman numerals — the way they appear on a payslip.
export const TAX_CLASS_OPTIONS: Option[] = ["I", "II", "III", "IV", "V", "VI"].map(
  (label, i) => ({ value: i + 1, label }),
);
