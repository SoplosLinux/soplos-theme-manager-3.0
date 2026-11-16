<script context="module" lang="ts">
  import type { Wallpaper } from "../lib/api";

  // What was found and drawn is kept while the program runs, so coming back to
  // the page shows the cards at once; the folder is read again behind them.
  const scanned = new Map<string, Wallpaper[]>();
  const smallCopies = new Map<string, string | null>();
  let lastFolder: string | null = null;
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { _ } from "svelte-i18n";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { FolderOpen, Image, Plus } from "lucide-svelte";
  import {
    defaultWallpaperFolder,
    scanWallpapers,
    setWallpaper,
    installWallpapers,
    makeThumbnail,
    pickFolder,
    pickImages,
  } from "../lib/api";
  import { withProgress } from "../lib/progress";
  import Modal from "../components/Modal.svelte";

  let folder: string | null = null;
  let items: Wallpaper[] = [];
  let selected: string | null = null;
  let status = "";
  let info: string | null = null;

  // The small copies, by the path of the picture. A picture whose copy could
  // not be made shows itself.
  let thumbs: Record<string, string | null> = Object.fromEntries(smallCopies);
  let generation = 0;

  async function loadThumbs(list: Wallpaper[], mine: number) {
    const queue = list.filter((w) => !(w.path in thumbs));
    // A few at a time: the first visit has to make them, later ones find them.
    const worker = async () => {
      for (let w = queue.shift(); w; w = queue.shift()) {
        if (mine !== generation) return;
        let copy: string | null = null;
        try {
          copy = await makeThumbnail(w.path);
        } catch {
          copy = null;
        }
        smallCopies.set(w.path, copy);
        thumbs = { ...thumbs, [w.path]: copy };
      }
    };
    await Promise.all([worker(), worker(), worker(), worker()]);
  }

  // `copies` is a parameter, not read from the component: the template has to
  // name `thumbs` itself, or Svelte does not draw the cards again when a copy
  // is made and they would stay empty until the page is opened again.
  function shown(copies: Record<string, string | null>, path: string): string | null {
    const copy = copies[path];
    if (copy) return convertFileSrc(copy);
    // Not made (yet): only a picture that could not be made shows itself.
    return path in copies ? convertFileSrc(path) : null;
  }

  async function scan(quiet = false) {
    if (!folder) {
      items = [];
      return;
    }
    const where = folder;
    const mine = ++generation;
    let found: Wallpaper[];
    try {
      found = quiet ? await scanWallpapers(where) : await withProgress(() => scanWallpapers(where));
    } catch {
      found = [];
    }
    if (where !== folder) return;
    scanned.set(where, found);
    const same = found.length === items.length && found.every((w, i) => w.path === items[i].path);
    if (!same) {
      items = found;
      if (!found.some((w) => w.path === selected)) selected = null;
    }
    status = $_("wallpapers.found", { values: { n: items.length } });
    void loadThumbs(found, mine);
  }

  onMount(async () => {
    folder = lastFolder ?? (await defaultWallpaperFolder());
    const known = folder ? scanned.get(folder) : undefined;
    if (known) {
      items = known;
      status = $_("wallpapers.found", { values: { n: items.length } });
      void loadThumbs(known, ++generation);
    }
    await scan(!!known);
  });

  onDestroy(() => {
    generation++;
  });

  async function chooseFolder() {
    const chosen = await pickFolder($_("wallpapers.pickFolder"), folder ?? undefined);
    if (!chosen) return;
    openFolder(chosen);
  }

  async function openFolder(path: string) {
    folder = path;
    lastFolder = path;
    const known = scanned.get(path);
    items = known ?? [];
    selected = null;
    await scan(!!known);
  }

  async function apply() {
    if (!selected) {
      info = $_("wallpapers.selectFirst");
      return;
    }
    try {
      await withProgress(() => setWallpaper(selected!));
      status = $_("wallpapers.applied");
    } catch (e) {
      status = $_("wallpapers.applyError");
      info = `${$_("wallpapers.applyError")}: ${String(e)}`;
    }
  }

  // Like the Xfce settings: choosing a picture puts it on the desktop at once.
  async function select(path: string) {
    selected = path;
    await apply();
  }

  // ── Installing ─────────────────────────────────────────────────────────────

  let toInstall: string[] = [];
  let everyone = false;
  let sub = "";

  async function install() {
    const paths = await pickImages($_("wallpapers.pickImages"), $_("wallpapers.images"));
    if (paths.length === 0) return;
    toInstall = paths;
  }

  async function confirmInstall() {
    const paths = toInstall;
    toInstall = [];
    let where: string;
    try {
      where = await withProgress(() => installWallpapers(paths, everyone, sub.trim()));
    } catch (e) {
      status = $_("wallpapers.installError", { values: { e: String(e) } });
      info = status;
      return;
    }
    // The page shows the folder they went to.
    scanned.delete(where);
    await openFolder(where);
    status = $_("wallpapers.installed");
  }
</script>

<div class="h-full flex flex-col gap-4">
  <div class="flex items-baseline justify-between gap-3">
    <h2 class="text-[1.25rem] font-semibold text-text-bright">{$_("wallpapers.title")}</h2>
    <span class="text-[0.8rem] text-text-muted truncate">{status}</span>
  </div>

  <div class="flex items-center gap-2">
    <span class="text-[0.85rem] text-text-muted shrink-0">{$_("wallpapers.folder")}</span>
    <div class="flex-1 min-w-0 truncate bg-surface border border-border rounded px-3 py-2 text-[0.82rem] text-text" title={folder ?? ""}>
      {folder ?? "—"}
    </div>
    <button class="flex items-center gap-2 px-3 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem] shrink-0" on:click={chooseFolder}>
      <FolderOpen size={16} />{$_("wallpapers.choose")}
    </button>
    <button class="flex items-center gap-2 px-4 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold shrink-0" on:click={apply}>
      <Image size={16} />{$_("wallpapers.set")}
    </button>
    <button class="flex items-center gap-2 px-3 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem] shrink-0" on:click={install}>
      <Plus size={16} />{$_("wallpapers.install")}
    </button>
  </div>

  <div class="content-scroll flex-1 min-h-0 overflow-y-auto bg-surface border border-border rounded p-4">
    <div class="grid gap-4 [grid-template-columns:repeat(auto-fill,minmax(180px,1fr))]">
      {#each items as item (item.path)}
        <button
          class="flex flex-col gap-1.5 p-1.5 rounded border text-left transition-colors {selected === item.path
            ? 'border-primary bg-primary/10'
            : 'border-transparent hover:bg-surface-2'}"
          on:click={() => select(item.path)}
          aria-pressed={selected === item.path}
          title={item.path}
        >
          {#if shown(thumbs, item.path)}
            <img src={shown(thumbs, item.path)} alt="" loading="lazy" decoding="async" class="w-full aspect-[5/3] object-cover bg-surface-2 rounded" />
          {:else}
            <div class="w-full aspect-[5/3] bg-surface-2 rounded animate-pulse"></div>
          {/if}
          <span class="truncate text-[0.78rem] text-text">{item.name}</span>
        </button>
      {/each}
    </div>
  </div>
</div>

{#if toInstall.length > 0}
  <Modal title={$_("wallpapers.installTitle", { values: { n: toInstall.length } })} onClose={() => (toInstall = [])}>
    <div class="flex flex-col gap-3 text-[0.85rem]">
      <label class="flex items-start gap-2.5 cursor-pointer">
        <input type="radio" class="mt-1" bind:group={everyone} value={false} />
        <span><span class="text-text-bright">{$_("wallpapers.forMe")}</span><br /><span class="text-text-muted">{$_("wallpapers.forMeHint")}</span></span>
      </label>
      <label class="flex items-start gap-2.5 cursor-pointer">
        <input type="radio" class="mt-1" bind:group={everyone} value={true} />
        <span><span class="text-text-bright">{$_("wallpapers.forAll")}</span><br /><span class="text-text-muted">{$_("wallpapers.forAllHint")}</span></span>
      </label>
      <label class="flex flex-col gap-1">
        <span class="text-text-muted">{$_("wallpapers.subfolder")}</span>
        <input
          type="text"
          bind:value={sub}
          placeholder={$_("wallpapers.subfolderHint")}
          class="bg-surface-2 border border-border rounded px-3 py-2 text-text-bright outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
        />
      </label>
    </div>
    <svelte:fragment slot="actions">
      <button class="px-6 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={() => (toInstall = [])}>{$_("common.cancel")}</button>
      <button data-autofocus class="px-6 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={confirmInstall}>{$_("wallpapers.install")}</button>
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
