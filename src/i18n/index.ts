import { register, init, getLocaleFromNavigator, locale, waitLocale } from "svelte-i18n";

export const SUPPORTED_LOCALES = ["en", "es", "de", "fr", "it", "pt", "ro", "ru"] as const;
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

// Each language is shown with its own name, so it can always be recognized.
export const LOCALE_NAMES: Record<SupportedLocale, string> = {
  en: "English",
  es: "Español",
  de: "Deutsch",
  fr: "Français",
  it: "Italiano",
  pt: "Português",
  ro: "Română",
  ru: "Русский",
};

register("en", () => import("./locales/en.json"));
register("es", () => import("./locales/es.json"));
register("de", () => import("./locales/de.json"));
register("fr", () => import("./locales/fr.json"));
register("it", () => import("./locales/it.json"));
register("pt", () => import("./locales/pt.json"));
register("ro", () => import("./locales/ro.json"));
register("ru", () => import("./locales/ru.json"));

const STORAGE_KEY = "soplos-theme-manager-locale";

function isSupported(code: string | null | undefined): code is SupportedLocale {
  return !!code && (SUPPORTED_LOCALES as readonly string[]).includes(code);
}

// The language the user chose, else the one of the system, else English.
function initialLocale(): SupportedLocale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (isSupported(saved)) return saved;
  } catch {
    // Storage can be unavailable; the system language is used then.
  }
  const system = (getLocaleFromNavigator() ?? "en").split("-")[0].toLowerCase();
  return isSupported(system) ? system : "en";
}

export async function initI18n() {
  init({ fallbackLocale: "en", initialLocale: initialLocale() });
  await waitLocale();
}

export function setLocale(code: SupportedLocale) {
  locale.set(code);
  try {
    localStorage.setItem(STORAGE_KEY, code);
  } catch {
    // Not saving the choice is not worth an error.
  }
}
