import { useEffect, useMemo, useState } from "react";
import { useWasm } from "./useWasm";
import { MESSAGES, TERMS, initialLanguage, type Messages } from "./i18n";
import { DEFAULT_INPUTS, type FormKey, type Inputs, type Lang } from "./types";
import { registerServiceWorker } from "./sw";
import { Payslip } from "./forms/Payslip";
import { Classes } from "./forms/Classes";
import { Projection } from "./forms/Projection";
import { useTheme, type Theme } from "./hooks/useTheme";

const FORMS: { key: FormKey; label: (t: Messages) => string }[] = [
  { key: "payslip", label: (t) => t.formPayslip },
  { key: "classes", label: (t) => t.formClasses },
  { key: "project", label: (t) => t.formProject },
];

function readStored(): string | null {
  try {
    return localStorage.getItem("casivell-lang");
  } catch {
    return null; // private mode
  }
}

// A hash lets a link open a particular form — used by the documentation
// screenshots, and harmless otherwise.
function formFromHash(): FormKey | null {
  const wanted = location.hash.replace("#", "");
  return wanted === "payslip" || wanted === "classes" || wanted === "project"
    ? wanted
    : null;
}

export default function App() {
  const { wasm, error } = useWasm();
  const [lang, setLang] = useState<Lang>(() =>
    initialLanguage(
      readStored(),
      navigator.languages ?? [navigator.language],
      new URLSearchParams(location.search).get("lang"),
    ),
  );
  const [theme, setTheme] = useTheme();
  const [form, setForm] = useState<FormKey>(() => formFromHash() ?? "payslip");
  const [inputs, setInputs] = useState<Inputs>(DEFAULT_INPUTS);
  const [updateAvailable, setUpdateAvailable] = useState(false);

  // The years the engine can actually compute, probed rather than hard-coded, so
  // the picker cannot offer a year the calculation refuses.
  const years = useMemo(() => {
    if (!wasm) return [];
    const packed = Number(wasm.casivell_enacted_years());
    const first = Math.floor(packed / 10_000);
    const last = packed % 10_000;
    const ys: number[] = [];
    for (let y = last; y >= first; y--) ys.push(y);
    return ys;
  }, [wasm]);

  // The Datenstand: the digest of the statutory data these figures rest on.
  const fingerprint = useMemo(() => {
    if (!wasm || years.length === 0) return "";
    const last = years[0];
    return BigInt.asUintN(64, wasm.casivell_fingerprint(last))
      .toString(16)
      .padStart(16, "0");
  }, [wasm, years]);

  useEffect(() => {
    document.documentElement.lang = lang;
    try {
      localStorage.setItem("casivell-lang", lang);
    } catch {
      /* private mode */
    }
  }, [lang]);

  useEffect(() => {
    registerServiceWorker(() => setUpdateAvailable(true));
  }, []);

  const t = MESSAGES[lang];
  const patch = (p: Partial<Inputs>): void => setInputs((prev) => ({ ...prev, ...p }));

  const switchForm = (key: FormKey): void => {
    setForm(key);
    history.replaceState(null, "", `#${key}`);
  };

  if (error) {
    return <div className="screen">Failed to load the engine: {error}</div>;
  }
  if (!wasm) {
    return <Skeleton t={t} />;
  }

  // The default year is the latest enacted one; until the picker is populated the
  // stored value may not be among the offered years, so fall back to the latest.
  const year = years.includes(inputs.year) ? inputs.year : (years[0] ?? DEFAULT_INPUTS.year);
  const effectiveInputs = { ...inputs, year };

  const first = years[years.length - 1];
  const last = years[0];
  const basis = `${t.legalBasis} ${first === last ? first : `${first}–${last}`}, ` +
    `${t.inForce}. ${TERMS.datenstand} ${fingerprint}.`;

  return (
    <div className="app">
      <header className="header">
        <div className="brand">
          <h1 className="title">Casivell</h1>
          <p className="sub">{t.tagline}</p>
        </div>
        <div className="controls">
          <label className="lang">
            <span className="vh">Sprache / Language</span>
            <select value={lang} onChange={(e) => setLang(e.target.value as Lang)}>
              <option value="de">Deutsch</option>
              <option value="en">English</option>
            </select>
          </label>
          <label className="theme">
            <span className="vh">Theme</span>
            <select
              value={theme}
              onChange={(e) => setTheme(e.target.value as Theme)}
              aria-label={`Theme: ${t.themeSystem}`}
            >
              <option value="system">{t.themeSystem}</option>
              <option value="light">{t.themeLight}</option>
              <option value="dark">{t.themeDark}</option>
            </select>
          </label>
        </div>
      </header>

      <nav className="nav" aria-label="Rechner">
        {FORMS.map((f) => (
          <button
            key={f.key}
            type="button"
            aria-pressed={form === f.key}
            onClick={() => switchForm(f.key)}
          >
            {f.label(t)}
          </button>
        ))}
      </nav>

      <main>
        {form === "payslip" && (
          <Payslip wasm={wasm} inputs={effectiveInputs} patch={patch} years={years} t={t} />
        )}
        {form === "classes" && (
          <Classes wasm={wasm} inputs={effectiveInputs} patch={patch} years={years} t={t} />
        )}
        {form === "project" && (
          <Projection wasm={wasm} inputs={effectiveInputs} patch={patch} years={years} t={t} />
        )}
      </main>

      {updateAvailable && (
        <p className="update" role="status">
          {t.updateAvailable}{" "}
          <button type="button" onClick={() => location.reload()}>
            {t.reload}
          </button>
        </p>
      )}

      <TrustBar basis={basis} />

      <footer className="footer">
        <details>
          <summary>{t.noAdvice}</summary>
          <p>
            <strong>{t.noAdvice}</strong> {t.noAdviceRest}
          </p>
          <p>{basis}</p>
          <p>
            {t.limitations} <code>docs/LIMITATIONS.md</code>.
          </p>
          <p>{t.offline}</p>
        </details>
      </footer>
    </div>
  );
}

function TrustBar({ basis }: { basis: string }) {
  return (
    <div className="trust" role="contentinfo" aria-label="Statutory basis">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <path
          d="M12 2L4 6v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V6l-8-4z"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinejoin="round"
        />
      </svg>
      <span>{basis}</span>
    </div>
  );
}

function Skeleton({ t }: { t: Messages }) {
  return (
    <div className="app skeleton" aria-busy="true" aria-label={t.loading}>
      <header className="header">
        <div className="brand">
          <h1 className="title">Casivell</h1>
        </div>
      </header>
      <div className="skeleton-nav skeleton-shine" />
      <div className="skeleton-card skeleton-shine" />
      <div className="skeleton-form">
        {Array.from({ length: 6 }).map((_, i) => (
          <div key={i} className="skeleton-field skeleton-shine" />
        ))}
      </div>
    </div>
  );
}
