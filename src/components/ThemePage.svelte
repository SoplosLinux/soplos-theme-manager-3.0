<script lang="ts">
  import { onMount } from "svelte";
  import { _ } from "svelte-i18n";
  import { Check, Plus, Trash2, Paintbrush, Image, MousePointer2 } from "lucide-svelte";
  import {
    listThemes,
    activeTheme,
    setActiveTheme,
    themeSamples,
    pickPackage,
    inspectPackage,
    discardPackage,
    installCandidates,
    themePackage,
    deleteTheme,
    type ThemeKind,
    type ThemeEntry,
    type Inspected,
    type Scope,
  } from "../lib/api";
  import { withProgress } from "../lib/progress";
  import { refreshTheme } from "../lib/theme";
  import Modal from "./Modal.svelte";

  /** The kind of theme the page lists: GTK, icons or cursors. */
  export let kind: ThemeKind;

  const FALLBACK = { gtk: Paintbrush, icons: Image, cursors: MousePointer2 };

  let themes: ThemeEntry[] = [];
  let active: string | null = null;
  let selected: string | null = null;
  let loaded = false;
  let applying = false;
  // The pictures that show what an icon theme or a cursor theme looks like,
  // by theme: asked for one theme at a time, so the cards show at once.
  let samples: Record<string, string[]> = {};

  // The steps of installing a package: the themes it holds, who they are for,
  // and the message that closes it.
  let package_: Inspected | null = null;
  let step: "candidates" | "scope" | null = null;
  let chosen: Record<string, boolean> = {};
  let info: string | null = null;

  async function load() {
    await withProgress(async () => {
      [themes, active] = await Promise.all([listThemes(kind), activeTheme(kind)]);
    });
    selected = active;
    loaded = true;
    if (kind === "icons" || kind === "cursors") void loadSamples(kind);
  }

  onMount(load);

  async function loadSamples(forKind: ThemeKind) {
    for (const theme of themes) {
      if (forKind !== kind) return;
      try {
        const found = await themeSamples(forKind, theme.name);
        samples = { ...samples, [theme.name]: found };
      } catch {
        // The card keeps its icon.
      }
    }
  }

  // A click on a card applies the theme: there is no Activate button.
  async function activate(name: string) {
    selected = name;
    if (applying || name === active) return;
    applying = true;
    try {
      await withProgress(() => setActiveTheme(kind, name));
    } catch {
      info = $_("themes.errors.activate");
      applying = false;
      return;
    }
    active = (await activeTheme(kind)) ?? name;
    // The window follows the GTK theme of the desktop, so it notices at once.
    if (kind === "gtk") await refreshTheme();
    applying = false;
  }

  // Deleting asks first. A system theme that came with a package is not
  // removed by hand: the message says which package to remove instead.
  let toDelete: ThemeEntry | null = null;

  async function askDelete(theme: ThemeEntry) {
    if (theme.name === active) return;
    if (theme.system) {
      const owner = await themePackage(kind, theme.name).catch(() => null);
      if (owner) {
        info = $_("themes.owned", { values: { package: owner } });
        return;
      }
    }
    toDelete = theme;
  }

  async function confirmDelete() {
    const theme = toDelete;
    toDelete = null;
    if (!theme) return;
    try {
      await withProgress(() => deleteTheme(kind, theme.name));
    } catch (e) {
      const text = String(e);
      info = text.startsWith("package:") ? $_("themes.owned", { values: { package: text.slice(8) } }) : $_("themes.errors.delete");
    }
    await load();
  }

  async function install() {
    const path = await pickPackage($_(`themes.${kind}.filter`));
    if (!path) return;
    let found: Inspected;
    try {
      found = await withProgress(() => inspectPackage(path, kind));
    } catch {
      info = $_("themes.errors.read");
      return;
    }
    if (found.candidates.length === 0) {
      // A package for another page says what it is and where it belongs; one
      // that holds nothing usable gets the general message.
      info = found.otherKinds.length > 0 ? found.otherKinds.map((k) => $_(`themes.wrong.${k}`)).join("\n\n") : $_(`themes.${kind}.noneFound`);
      return;
    }
    package_ = found;
    chosen = Object.fromEntries(found.candidates.map((c) => [c.rel, true]));
    step = "candidates";
  }

  // Leaves the installation, throwing away what was unpacked.
  async function cancelInstall() {
    const token = package_?.token;
    package_ = null;
    step = null;
    if (token) await discardPackage(token).catch(() => {});
  }

  function toScope() {
    if (package_?.candidates.some((c) => chosen[c.rel])) step = "scope";
  }

  async function finish(scope: Scope) {
    if (!package_) return;
    const { token, candidates } = package_;
    const rels = candidates.filter((c) => chosen[c.rel]).map((c) => c.rel);
    package_ = null;
    step = null;
    let skipped: string[];
    try {
      skipped = await withProgress(() => installCandidates(token, rels, kind, scope));
    } catch {
      info = $_("themes.errors.install");
      return;
    }
    if (skipped.length > 0) info = $_("themes.skipped", { values: { names: skipped.join(", ") } });
    await load();
  }

  $: status = $_("themes.status", { values: { n: themes.length, active: active ?? $_("themes.none") } });
  $: anyChosen = package_ ? package_.candidates.some((c) => chosen[c.rel]) : false;
</script>

<div class="h-full flex flex-col gap-4">
  <div class="flex items-center justify-between gap-3">
    <h2 class="text-[1.25rem] font-semibold text-text-bright">{$_(`themes.${kind}.title`)}</h2>
    <div class="flex items-center gap-3">
      {#if loaded}<span class="text-[0.8rem] text-text-muted">{status}</span>{/if}
      <button
        class="flex items-center justify-center gap-2 px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.85rem]"
        on:click={install}
      >
        <Plus size={16} />{$_("themes.install")}
      </button>
    </div>
  </div>
  {#if kind === "gtk"}
    <p class="-mt-2 text-[0.78rem] text-text-muted">{$_("themes.gtk.hint")}</p>
  {/if}

  <div class="content-scroll flex-1 min-h-0 overflow-y-auto pr-1">
    <div class="grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))] gap-3">
      {#each themes as theme (theme.name)}
        <div class="relative group">
          <!-- A click on a card applies it: there is no Activate button. -->
          <button
            class="w-full text-left rounded border bg-surface overflow-hidden transition-colors
              {selected === theme.name ? 'border-primary' : 'border-border hover:border-primary/60'}"
            aria-pressed={selected === theme.name}
            on:click={() => activate(theme.name)}
          >
            <div class="relative overflow-hidden h-[110px] bg-surface-2">
              {#if kind === "gtk" && theme.swatches.length === 4}
                <!-- A small window painted with the colors of the theme. -->
                <div class="absolute inset-0 grid place-items-center p-3" style="background: {theme.swatches[0]}">
                  <div class="w-full h-full rounded-sm overflow-hidden" style="background: {theme.swatches[1]}; border: 1px solid {theme.swatches[3]}44">
                    <div class="h-3.5" style="background: {theme.swatches[0]}; border-bottom: 1px solid {theme.swatches[3]}33"></div>
                    <div class="p-2 flex flex-col gap-1.5">
                      <div class="h-1.5 w-3/4 rounded-full" style="background: {theme.swatches[3]}; opacity: 0.7"></div>
                      <div class="h-1.5 w-1/2 rounded-full" style="background: {theme.swatches[3]}; opacity: 0.4"></div>
                      <div class="h-4 w-14 rounded-sm mt-1" style="background: {theme.swatches[2]}"></div>
                    </div>
                  </div>
                </div>
              {:else if kind === "gtk" && theme.preview}
                <img src={theme.preview} alt="" class="absolute inset-0 w-full h-full object-contain p-2" />
              {:else if kind === "icons" && samples[theme.name]?.length}
                <div class="absolute inset-0 grid grid-cols-3 gap-1.5 p-2.5 place-items-center">
                  {#each samples[theme.name] as sample}
                    <img src={sample} alt="" class="w-9 h-9 object-contain" />
                  {/each}
                </div>
              {:else if kind === "cursors" && samples[theme.name]?.length}
                <div class="absolute inset-0 flex items-center justify-center gap-4">
                  {#each samples[theme.name] as sample}
                    <img src={sample} alt="" class="h-9 w-auto object-contain" />
                  {/each}
                </div>
              {:else}
                <div class="absolute inset-0 grid place-items-center text-text-muted">
                  <svelte:component this={FALLBACK[kind]} size={34} />
                </div>
              {/if}
              {#if active === theme.name}
                <span class="absolute top-1.5 right-1.5 w-6 h-6 rounded-full bg-primary text-text-bright grid place-items-center" title={$_("themes.active")}>
                  <Check size={14} />
                </span>
              {/if}
            </div>
            <div class="px-3 py-2">
              <div class="text-[0.86rem] truncate {active === theme.name ? 'font-semibold text-text-bright' : 'text-text'}">{theme.displayName}</div>
            </div>
          </button>
          {#if active !== theme.name}
            <button
              class="absolute top-2 left-2 w-8 h-8 rounded-full bg-surface text-text-bright border border-border shadow-[0_1px_4px_rgba(0,0,0,0.5)] grid place-items-center hover:bg-danger hover:text-white hover:border-danger"
              title={$_("themes.delete")}
              aria-label={$_("themes.delete")}
              on:click={() => askDelete(theme)}
            >
              <Trash2 size={16} />
            </button>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</div>

{#if step === "candidates" && package_}
  <Modal title={$_("themes.found.title")} onClose={cancelInstall}>
    <p>{$_("themes.found.hint", { values: { n: package_.candidates.length } })}</p>
    <div class="flex flex-col gap-1.5">
      {#each package_.candidates as c (c.rel)}
        <label class="flex items-center gap-2.5 cursor-pointer">
          <input type="checkbox" class="accent-[#ff7d2e]" bind:checked={chosen[c.rel]} />
          <span>{c.displayName}{c.hasXfwm4 ? $_("themes.found.withBorder") : ""}</span>
        </label>
      {/each}
    </div>
    <svelte:fragment slot="actions">
      <button class="px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={cancelInstall}>{$_("common.cancel")}</button>
      <button
        data-autofocus
        class="px-4 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold disabled:opacity-50 disabled:hover:bg-primary"
        disabled={!anyChosen}
        on:click={toScope}>{$_("themes.found.installSelected")}</button>
    </svelte:fragment>
  </Modal>
{:else if step === "scope"}
  <Modal title={$_("themes.scope.title")} onClose={cancelInstall}>
    <p>{$_("themes.scope.question")}</p>
    <svelte:fragment slot="actions">
      <button class="px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={cancelInstall}>{$_("common.cancel")}</button>
      <button class="px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={() => finish("user")}>{$_("themes.scope.user")}</button>
      <button data-autofocus class="px-4 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={() => finish("global")}>
        {$_("themes.scope.global")}
      </button>
    </svelte:fragment>
  </Modal>
{/if}

{#if toDelete}
  <Modal title={$_("themes.deleteDialog.title")} onClose={() => (toDelete = null)}>
    <p>{$_("themes.deleteDialog.question", { values: { name: toDelete.displayName } })}</p>
    <p class="text-text-muted">{toDelete.system ? $_("themes.deleteDialog.system") : $_("themes.deleteDialog.user")}</p>
    <svelte:fragment slot="actions">
      <button data-autofocus class="px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={() => (toDelete = null)}>{$_("common.cancel")}</button>
      <button class="px-4 py-2 rounded bg-danger hover:opacity-90 text-white text-[0.82rem] font-semibold" on:click={confirmDelete}>{$_("themes.delete")}</button>
    </svelte:fragment>
  </Modal>
{/if}

{#if info}
  <Modal title={$_("common.notice")} onClose={() => (info = null)}>
    <p class="whitespace-pre-line">{info}</p>
    <svelte:fragment slot="actions">
      <button data-autofocus class="px-6 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={() => (info = null)}>{$_("common.ok")}</button>
    </svelte:fragment>
  </Modal>
{/if}
