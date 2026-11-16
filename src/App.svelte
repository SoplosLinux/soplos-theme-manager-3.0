<script lang="ts">
  import { setContext, onMount, type ComponentType } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getVersion } from "@tauri-apps/api/app";
  import { _ } from "svelte-i18n";
  import { Palette, Image, Paintbrush, Shapes, MousePointer2, PanelTop, Dock as DockIcon, LogIn } from "lucide-svelte";
  import { busy } from "./lib/progress";
  import { listen } from "@tauri-apps/api/event";
  import { pendingBundle } from "./lib/openBundle";
  import { takePendingBundle } from "./lib/api";
  import Sidebar from "./components/Sidebar.svelte";
  import WindowControls from "./components/WindowControls.svelte";
  import AboutDialog from "./components/AboutDialog.svelte";
  import Themes from "./views/Themes.svelte";
  import Wallpapers from "./views/Wallpapers.svelte";
  import GtkThemes from "./views/GtkThemes.svelte";
  import Icons from "./views/Icons.svelte";
  import Cursors from "./views/Cursors.svelte";
  import Panel from "./views/Panel.svelte";
  import Dock from "./views/Dock.svelte";
  import LoginScreen from "./views/LoginScreen.svelte";

  // The tabs of the sidebar, in the order of the 2.x program.
  $: tabs = [
    { id: "themes", icon: Palette, label: $_("app.tabs.themes") },
    { id: "wallpapers", icon: Image, label: $_("app.tabs.wallpapers") },
    { id: "gtk", icon: Paintbrush, label: $_("app.tabs.gtk") },
    { id: "icons", icon: Shapes, label: $_("app.tabs.icons") },
    { id: "cursors", icon: MousePointer2, label: $_("app.tabs.cursors") },
    { id: "panel", icon: PanelTop, label: $_("app.tabs.panel") },
    { id: "dock", icon: DockIcon, label: $_("app.tabs.dock") },
    { id: "login", icon: LogIn, label: $_("app.tabs.login") },
  ];

  const views: Record<string, ComponentType> = {
    themes: Themes,
    wallpapers: Wallpapers,
    gtk: GtkThemes,
    icons: Icons,
    cursors: Cursors,
    panel: Panel,
    dock: Dock,
    login: LoginScreen,
  };

  let active = "themes";
  let appVersion = "";
  let aboutOpen = false;

  // With `decorations` off in tauri.conf.json (and `transparent` on) the system
  // draws no frame: the window is one card with rounded corners and its
  // buttons float at the top right. With `decorations` on none of this shows,
  // so that setting is the only switch between the two.
  let frameless = false;
  let maximized = false;

  // A .sth file opened from the file manager goes to the Themes page.
  function openBundle(path: string) {
    active = "themes";
    pendingBundle.set(path);
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let unlistenOpen: (() => void) | undefined;
    (async () => {
      try {
        const first = await takePendingBundle();
        if (first) openBundle(first);
        unlistenOpen = await listen<string>("open-bundle", (event) => openBundle(event.payload));
      } catch {
        // Not running inside the program: nothing to open.
      }
      try {
        appVersion = await getVersion();
      } catch {
        appVersion = "";
      }
      try {
        const win = getCurrentWindow();
        frameless = !(await win.isDecorated());
        if (frameless) {
          maximized = await win.isMaximized();
          unlisten = await win.onResized(async () => {
            maximized = await win.isMaximized();
          });
        }
      } catch {
        frameless = false;
      }
    })();
    return () => {
      unlisten?.();
      unlistenOpen?.();
    };
  });

  function select(id: string) {
    active = id;
    document.getElementById("main-scroll")?.scrollTo({ top: 0 });
  }

  setContext("openAbout", () => (aboutOpen = true));

  $: ActiveView = views[active];

  // F1 opens About, Ctrl+Q quits, Ctrl+Tab and Ctrl+Shift+Tab move between tabs.
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "F1") {
      e.preventDefault();
      aboutOpen = true;
    } else if (e.ctrlKey && (e.key === "q" || e.key === "Q")) {
      e.preventDefault();
      getCurrentWindow().close();
    } else if (e.ctrlKey && e.key === "Tab") {
      e.preventDefault();
      const i = tabs.findIndex((t) => t.id === active);
      const next = (i + (e.shiftKey ? -1 : 1) + tabs.length) % tabs.length;
      select(tabs[next].id);
    }
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="relative flex h-screen overflow-hidden bg-dark {frameless && !maximized ? 'window-card' : ''}">
  <Sidebar {tabs} {active} onSelect={select} {appVersion} />

  {#if frameless}
    <div class="absolute right-2 top-[5px] z-40"><WindowControls {maximized} /></div>
  {/if}

  <div class="flex-1 flex flex-col overflow-hidden min-w-0">
    <!-- No top bar: with no frame of the system this strip holds the window
         buttons and moves the window. -->
    {#if frameless}
      <div class="h-8 shrink-0" data-tauri-drag-region></div>
    {/if}
    <div id="main-scroll" class="content-scroll flex-1 overflow-y-auto p-7 {frameless ? 'pt-0' : ''}">
      <svelte:component this={ActiveView} />
    </div>

    <!-- The bar of progress: it is there while the program works on something. -->
    <div class="h-1.5 bg-surface-2 overflow-hidden shrink-0" class:invisible={$busy === 0}>
      <div class="h-full w-1/3 bg-primary progress-slide"></div>
    </div>
  </div>

  {#if aboutOpen}
    <AboutDialog onClose={() => (aboutOpen = false)} />
  {/if}
</div>
