import "./styles/global.css";
import { initI18n } from "./i18n";
import { initTheme } from "./lib/theme";
import App from "./App.svelte";

// This is a desktop program, not a web page: the context menu of the web
// engine (Back, Forward, Reload, Inspect) has no place in it. It stays
// available while the program is run for development.
if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", (e) => e.preventDefault());
}

initTheme();

// The dictionary has to be loaded before the first render, or the interface
// would flash its raw keys.
initI18n().then(() => {
  new App({ target: document.getElementById("app")! });
});
