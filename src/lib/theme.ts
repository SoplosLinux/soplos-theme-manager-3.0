import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

export type ThemeName = "dark" | "light";

// What the user chose: null follows the theme of the desktop.
const STORAGE_KEY = "soplos-theme-manager-theme-choice";
const POLL_MS = 3000;

let choice: ThemeName | null = null;
let desktopTheme: ThemeName = "dark";

/** The theme the window is showing right now. */
export const theme = writable<ThemeName>("dark");

function apply() {
  const effective = choice ?? desktopTheme;
  document.documentElement.classList.toggle("light", effective === "light");
  theme.set(effective);
}

async function readDesktopTheme() {
  try {
    const found = await invoke<string>("get_system_theme");
    if (found === "dark" || found === "light") {
      desktopTheme = found;
      apply();
    }
  } catch {
    // Without an answer the last known theme stays.
  }
}

/** Reads the theme of the desktop again, for when the program has just changed it. */
export const refreshTheme = readDesktopTheme;

export function initTheme() {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "dark" || saved === "light") choice = saved;
  } catch {
    choice = null;
  }
  apply();
  void readDesktopTheme();
  // The desktop theme can change while the window is open.
  setInterval(readDesktopTheme, POLL_MS);
}

/**
 * Switches between dark and light. Switching to the theme the desktop already
 * has goes back to following the desktop instead of saving a fixed choice.
 */
export function toggleTheme() {
  const next: ThemeName = get(theme) === "dark" ? "light" : "dark";
  choice = next === desktopTheme ? null : next;
  try {
    if (choice) localStorage.setItem(STORAGE_KEY, choice);
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // The choice just is not remembered.
  }
  apply();
}
