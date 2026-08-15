import { useEffect, useState } from "react";

export type Theme = "system" | "light" | "dark";

function readStored(): Theme {
  try {
    const raw = localStorage.getItem("casivell-theme");
    if (raw === "light" || raw === "dark" || raw === "system") return raw;
  } catch {
    // private mode
  }
  return "system";
}

export function useTheme(): [Theme, (t: Theme) => void] {
  const [theme, setTheme] = useState<Theme>(readStored);

  useEffect(() => {
    const root = document.documentElement;
    root.removeAttribute("data-theme");
    if (theme === "light") root.setAttribute("data-theme", "light");
    else if (theme === "dark") root.setAttribute("data-theme", "dark");
    try {
      localStorage.setItem("casivell-theme", theme);
    } catch {
      // private mode
    }
  }, [theme]);

  return [theme, setTheme];
}
