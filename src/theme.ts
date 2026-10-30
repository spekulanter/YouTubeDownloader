import { useEffect, useState } from "react";

type Theme = "light" | "dark";
const THEME_KEY = "streamforge.theme.v1";

function loadPreference(): Theme | null {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    return saved === "light" || saved === "dark" ? saved : null;
  } catch {
    return null;
  }
}

function systemTheme(): Theme {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function initializeTheme() {
  document.documentElement.dataset.theme = loadPreference() ?? systemTheme();
}

export function useTheme() {
  const [preference, setPreference] = useState(loadPreference);
  const [system, setSystem] = useState(systemTheme);
  const theme = preference ?? system;

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setSystem(media.matches ? "dark" : "light");
    media.addEventListener("change", onChange);
    onChange();
    return () => media.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  function toggleTheme() {
    const next = theme === "dark" ? "light" : "dark";
    setPreference(next);
    try {
      localStorage.setItem(THEME_KEY, next);
    } catch {
      // The switch still works if persistent storage is unavailable.
    }
  }

  return { theme, toggleTheme };
}
