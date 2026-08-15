import { useEffect, useState } from "react";
import { loadWasm, type CasivellWasm } from "./wasm";

// Loads the engine once and exposes it, with a loading and an error state. The
// module is cached inside `loadWasm`, so this hook is safe under StrictMode's
// double effect.
export function useWasm(): { wasm: CasivellWasm | null; error: string | null } {
  const [wasm, setWasm] = useState<CasivellWasm | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    loadWasm()
      .then((w) => {
        if (!cancelled) setWasm(w);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return { wasm, error };
}
