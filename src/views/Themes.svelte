<script lang="ts">
  import { onMount } from "svelte";
  import { _ } from "svelte-i18n";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { Palette, Paintbrush, Shapes, MousePointer2, Image, PanelTop, Check, Plus, Trash2, FileOutput, FileInput, EllipsisVertical, RotateCcw } from "lucide-svelte";
  import {
    listSavedThemes,
    applySavedTheme,
    saveCurrentTheme,
    removeSavedTheme,
    renameSavedTheme,
    exportSavedTheme,
    bundleInfo,
    installBundle,
    restoreBaseThemes,
    seedFirstRun,
    screenshotAvailable,
    type SavedTheme,
    type ThemePart,
    type Scope,
  } from "../lib/api";
  import { withProgress } from "../lib/progress";
  import { pendingBundle } from "../lib/openBundle";
  import Modal from "../components/Modal.svelte";
  import Switch from "../components/Switch.svelte";

  const partIcons = { gtk: Paintbrush, icons: Shapes, cursor: MousePointer2, wallpaper: Image, panel: PanelTop };

  let items: SavedTheme[] = [];
  let selected: string | null = null;
  let status = "";
  let info: string | null = null;

  let creating = false;
  let newName = "";
  let withScreenshot = true;
  let canScreenshot = false;

  let toRemove: string | null = null;
  let menuOpen = false;
  let renaming: string | null = null;
  let renameTo = "";
  let installPath: string | null = null;

  async function refresh() {
    try {
      items = await listSavedThemes();
    } catch {
      items = [];
    }
    if (selected && !items.some((t) => t.name === selected)) selected = null;
    status = $_("bundles.status", { values: { n: items.length } });
  }

  onMount(() => {
    const stop = pendingBundle.subscribe((path) => {
      if (path) {
        pendingBundle.set(null);
        startInstall(path);
      }
    });
    (async () => {
      await withProgress(seedFirstRun);
      await refresh();
      canScreenshot = await screenshotAvailable().catch(() => false);
    })();
    return stop;
  });

  function partsText(parts: ThemePart[]): string {
    return $_("bundles.includes", { values: { list: parts.map((p) => $_(`bundles.parts.${p}`)).join(", ") } });
  }

  async function apply() {
    if (!selected) {
      info = $_("bundles.selectFirst");
      return;
    }
    try {
      const failed = await withProgress(() => applySavedTheme(selected!));
      status = failed.length === 0 ? $_("bundles.applied") : $_("bundles.appliedWithErrors");
    } catch {
      status = $_("bundles.applyError");
    }
  }

  function openNew() {
    newName = "";
    creating = true;
  }

  async function saveNew() {
    const name = newName.trim();
    if (!name) return;
    const shoot = withScreenshot && canScreenshot;
    creating = false;
    const win = getCurrentWindow();
    try {
      if (shoot) await win.minimize();
      await withProgress(() => saveCurrentTheme(name, shoot));
      status = $_("bundles.saved");
    } catch (e) {
      const code = String(e);
      info = code === "exists" ? $_("bundles.new.exists") : code === "invalid" ? $_("bundles.new.invalid") : $_("bundles.new.error");
    } finally {
      if (shoot) {
        await win.unminimize().catch(() => {});
        await win.setFocus().catch(() => {});
      }
    }
    await refresh();
    selected = name;
  }

  // F2 renames the selected theme.
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") menuOpen = false;
    if (e.key !== "F2" || !selected || creating || toRemove || installPath || info || renaming) return;
    e.preventDefault();
    renameTo = selected;
    renaming = selected;
  }

  async function confirmRename() {
    const from = renaming;
    const to = renameTo.trim();
    renaming = null;
    if (!from || !to || to === from) return;
    try {
      await withProgress(() => renameSavedTheme(from, to));
      status = $_("bundles.rename.done");
      await refresh();
      selected = to;
    } catch (e) {
      const code = String(e);
      info = code === "exists" ? $_("bundles.new.exists") : code === "invalid" ? $_("bundles.new.invalid") : $_("bundles.rename.error");
    }
  }

  async function confirmRemove() {
    const name = toRemove;
    toRemove = null;
    if (!name) return;
    try {
      await withProgress(() => removeSavedTheme(name));
      status = $_("bundles.removed");
    } catch {
      info = $_("bundles.removeError");
    }
    await refresh();
  }

  async function exportTheme() {
    if (!selected) {
      info = $_("bundles.selectFirst");
      return;
    }
    const dest = await save({
      title: $_("bundles.export.pick"),
      defaultPath: `${selected}.sth`,
      filters: [{ name: $_("bundles.filter"), extensions: ["sth"] }],
    });
    if (!dest) return;
    try {
      await withProgress(() => exportSavedTheme(selected!, dest.endsWith(".sth") ? dest : `${dest}.sth`));
      status = $_("bundles.export.done");
    } catch {
      info = $_("bundles.export.error");
    }
  }

  async function chooseBundle() {
    const chosen = await open({
      multiple: false,
      directory: false,
      title: $_("bundles.install.pick"),
      filters: [{ name: $_("bundles.filter"), extensions: ["sth"] }],
    });
    if (typeof chosen === "string") await startInstall(chosen);
  }

  // Checks the file and asks for whom to install it.
  async function startInstall(path: string) {
    try {
      await bundleInfo(path);
    } catch {
      info = $_("bundles.install.invalid");
      return;
    }
    installPath = path;
  }

  async function finishInstall(scope: Scope) {
    const path = installPath;
    installPath = null;
    if (!path) return;
    try {
      const name = await withProgress(() => installBundle(path, scope));
      await refresh();
      selected = name;
      status = $_("bundles.install.done", { values: { name } });
    } catch {
      info = $_("bundles.install.error");
    }
  }

  async function restore() {
    try {
      const added = await withProgress(restoreBaseThemes);
      await refresh();
      info = added === 0 ? $_("bundles.restore.all") : $_("bundles.restore.added", { values: { n: added } });
    } catch {
      info = $_("bundles.restore.none");
    }
  }

  const button = "flex items-center gap-1.5 px-2.5 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem] shrink-0 disabled:opacity-40 disabled:pointer-events-none";
</script>

<svelte:window on:keydown={handleKeydown} on:click={() => (menuOpen = false)} />

<div class="h-full flex flex-col gap-4">
  <div class="flex items-baseline justify-between gap-3">
    <h2 class="text-[1.25rem] font-semibold text-text-bright">{$_("bundles.title")}</h2>
    <span class="text-[0.8rem] text-text-muted truncate">{status}</span>
  </div>

  <!-- One row, two groups: what acts on the selected theme, then what does not
       depend on the selection. -->
  <div class="flex items-center gap-1.5">
    <button
      class="flex items-center gap-1.5 px-3.5 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold shrink-0 disabled:opacity-40 disabled:pointer-events-none"
      disabled={!selected}
      on:click={apply}
    >
      <Check size={16} />{$_("bundles.apply")}
    </button>
    <button class={button} disabled={!selected} on:click={() => (toRemove = selected)}><Trash2 size={16} />{$_("bundles.remove")}</button>
    <button class={button} disabled={!selected} title={$_("bundles.hints.export")} on:click={exportTheme}><FileOutput size={16} />{$_("bundles.export.button")}</button>

    <div class="w-px h-6 bg-border mx-1 shrink-0"></div>

    <button class={button} on:click={openNew}><Plus size={16} />{$_("bundles.new.button")}</button>
    <button class={button} title={$_("bundles.hints.install")} on:click={chooseBundle}><FileInput size={16} />{$_("bundles.install.button")}</button>

    <div class="relative ml-auto shrink-0">
      <button
        class="flex items-center px-2 py-2 rounded bg-surface-2 hover:bg-border text-text-bright"
        title={$_("bundles.hints.more")}
        aria-label={$_("bundles.hints.more")}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        on:click|stopPropagation={() => (menuOpen = !menuOpen)}
      >
        <EllipsisVertical size={16} />
      </button>
      {#if menuOpen}
        <div class="absolute right-0 top-full mt-1 z-30 min-w-[210px] bg-surface border border-border rounded-lg shadow-2xl p-1" role="menu">
          <button
            class="w-full flex items-center gap-2 px-3 py-2 rounded text-left text-[0.82rem] text-text-bright hover:bg-surface-2"
            role="menuitem"
            on:click={() => {
              menuOpen = false;
              restore();
            }}
          >
            <RotateCcw size={16} />{$_("bundles.restore.button")}
          </button>
        </div>
      {/if}
    </div>
  </div>

  <div class="content-scroll flex-1 min-h-0 overflow-y-auto bg-surface border border-border rounded p-4">
    <div class="grid gap-4 [grid-template-columns:repeat(auto-fill,minmax(200px,1fr))]">
      {#each items as item (item.name)}
        <button
          class="flex flex-col gap-1.5 p-1.5 rounded border text-left transition-colors {selected === item.name ? 'border-primary bg-primary/10' : 'border-transparent hover:bg-surface-2'}"
          on:click={() => (selected = item.name)}
          on:dblclick={apply}
          aria-pressed={selected === item.name}
          title={item.author ? `${item.name} - ${item.author}` : item.name}
        >
          <div class="relative w-full aspect-[16/9] rounded bg-surface-2 overflow-hidden flex items-center justify-center">
            {#if item.preview}
              <img src={item.preview} alt="" decoding="async" class="w-full h-full object-cover" />
            {:else}
              <Palette size={32} class="text-text-muted" />
            {/if}
            <!-- What the theme carries, in a corner. -->
            <div class="absolute right-1 top-1 flex gap-1 px-1.5 py-1 rounded-full bg-black/60 text-white" title={partsText(item.parts)}>
              {#each item.parts as part}
                <svelte:component this={partIcons[part]} size={12} />
              {/each}
            </div>
          </div>
          <span class="truncate text-[0.82rem] text-text">{item.name}</span>
        </button>
      {/each}
    </div>
  </div>
</div>

{#if creating}
  <Modal title={$_("bundles.new.title")} onClose={() => (creating = false)}>
    <label class="flex flex-col gap-1.5">
      <span class="text-text-muted">{$_("bundles.new.name")}</span>
      <input
        data-autofocus
        bind:value={newName}
        maxlength="100"
        class="bg-surface-2 border border-border rounded px-3 py-2 text-text-bright"
        on:keydown={(e) => e.key === "Enter" && saveNew()}
      />
    </label>
    {#if canScreenshot}
      <div class="flex items-center justify-between gap-3">
        <span>{$_("bundles.new.screenshot")}</span>
        <Switch bind:checked={withScreenshot} label={$_("bundles.new.screenshot")} />
      </div>
    {/if}
    <svelte:fragment slot="actions">
      <button class="px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={() => (creating = false)}>{$_("common.cancel")}</button>
      <button class="px-4 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={saveNew}>{$_("bundles.new.save")}</button>
    </svelte:fragment>
  </Modal>
{/if}

{#if renaming}
  <Modal title={$_("bundles.rename.title")} onClose={() => (renaming = null)}>
    <label class="flex flex-col gap-1.5">
      <span class="text-text-muted">{$_("bundles.new.name")}</span>
      <input
        data-autofocus
        bind:value={renameTo}
        maxlength="100"
        class="bg-surface-2 border border-border rounded px-3 py-2 text-text-bright"
        on:focus={(e) => e.currentTarget.select()}
        on:keydown={(e) => e.key === "Enter" && confirmRename()}
      />
    </label>
    <svelte:fragment slot="actions">
      <button class="px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={() => (renaming = null)}>{$_("common.cancel")}</button>
      <button class="px-4 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={confirmRename}>{$_("bundles.rename.save")}</button>
    </svelte:fragment>
  </Modal>
{/if}

{#if toRemove}
  <Modal title={$_("bundles.removeTitle")} onClose={() => (toRemove = null)}>
    <p>{$_("bundles.removeQuestion", { values: { name: toRemove } })}</p>
    <svelte:fragment slot="actions">
      <button data-autofocus class="px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={() => (toRemove = null)}>{$_("common.cancel")}</button>
      <button class="px-4 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={confirmRemove}>{$_("bundles.remove")}</button>
    </svelte:fragment>
  </Modal>
{/if}

{#if installPath}
  <Modal title={$_("themes.scope.title")} onClose={() => (installPath = null)}>
    <p>{$_("bundles.install.question")}</p>
    <svelte:fragment slot="actions">
      <button class="px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={() => (installPath = null)}>{$_("common.cancel")}</button>
      <button class="px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={() => finishInstall("user")}>{$_("themes.scope.user")}</button>
      <button data-autofocus class="px-4 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={() => finishInstall("global")}>{$_("themes.scope.global")}</button>
    </svelte:fragment>
  </Modal>
{/if}

{#if info}
  <Modal title={$_("common.notice")} onClose={() => (info = null)}>
    <p>{info}</p>
    <svelte:fragment slot="actions">
      <button data-autofocus class="px-6 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={() => (info = null)}>{$_("common.ok")}</button>
    </svelte:fragment>
  </Modal>
{/if}
