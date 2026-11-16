<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { _ } from "svelte-i18n";
  import { AppWindow, Pin, RefreshCw, Save, Search, X, Info } from "lucide-svelte";
  import { dockActive, dockList, dockPinned, dockAvailable, dockSave, type DockApp, type DockInfo } from "../lib/api";
  import { withProgress } from "../lib/progress";

  // The dock is edited on the preview itself: applications are dragged from the
  // list into the strip, moved inside it, and dragged out of it to remove them.
  // The drag is made with pointer events: the drag and drop of HTML is not
  // reliable in WebKitGTK.

  let active: boolean | null = null;
  let pinned: DockApp[] = [];
  // Every dock of the panels; each has its own applications.
  let docks: DockInfo[] = [];
  let current: number | undefined;
  let available: DockApp[] = [];
  let query = "";
  let selectedAvailable: string | null = null;
  let selectedPinned: string | null = null;
  let status = "";

  let strip: HTMLElement;
  // The dock is always one row. Its icons are as big as the width allows, up to
  // a limit, and they only stop shrinking at a size that can still be told
  // apart; with more icons than that, the strip scrolls sideways.
  let room = 0;
  const MIN_ICON = 32;
  const MAX_ICON = 76;
  const GAP = 8;
  const STRIP_PADDING = 28; // the padding and the border of the strip, both sides

  type Drag = {
    source: "pinned" | "available";
    app: DockApp;
    x: number;
    y: number;
    startX: number;
    startY: number;
    moved: boolean;
    inside: boolean;
    insertAt: number;
  };
  let drag: Drag | null = null;

  const MOVE_THRESHOLD = 5; // pixels before a press becomes a drag
  const OUTSIDE_MARGIN = 36; // pixels around the strip where an icon still counts as inside

  async function load() {
    active = await dockActive().catch(() => false);
    if (!active) return;
    docks = await dockList().catch(() => []);
    if (current === undefined || !docks.some((d) => d.id === current)) current = docks[0]?.id;
    await withProgress(async () => {
      [pinned, available] = await Promise.all([dockPinned(current), dockAvailable()]);
    });
    selectedPinned = null;
    status = $_("dock.pinnedCount", { values: { n: pinned.length } });
  }

  async function chooseDock(event: Event) {
    current = Number((event.currentTarget as HTMLSelectElement).value);
    await withProgress(async () => {
      pinned = await dockPinned(current);
    });
    selectedPinned = null;
    status = $_("dock.pinnedCount", { values: { n: pinned.length } });
  }

  onMount(load);
  onDestroy(() => stopListening());

  $: filtered = available.filter((a) => a.name.toLowerCase().includes(query.trim().toLowerCase()));
  // While an icon of the strip is dragged it leaves its place; the strip shows
  // the others and a gap where the icon would land.
  $: shown = drag?.moved && drag.source === "pinned" ? pinned.filter((p) => p.desktopPath !== drag!.app.desktopPath) : pinned;

  // The icon being dragged over the strip takes a place of its own.
  $: slots = Math.max(1, shown.length + (drag?.moved && drag.inside ? 1 : 0));
  $: usable = Math.max(MIN_ICON, room - STRIP_PADDING);
  $: iconSize = Math.min(MAX_ICON, Math.max(MIN_ICON, Math.floor((usable - GAP * (slots - 1)) / slots)));

  // ── Dragging ───────────────────────────────────────────────────────────────

  function startDrag(event: PointerEvent, source: Drag["source"], app: DockApp) {
    if (event.button !== 0) return;
    drag = { source, app, x: event.clientX, y: event.clientY, startX: event.clientX, startY: event.clientY, moved: false, inside: false, insertAt: pinned.length };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("keydown", onKey);
  }

  function stopListening() {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("keydown", onKey);
    document.body.style.userSelect = "";
    document.body.style.cursor = "";
  }

  function onMove(event: PointerEvent) {
    if (!drag) return;
    drag.x = event.clientX;
    drag.y = event.clientY;
    if (!drag.moved && Math.hypot(drag.x - drag.startX, drag.y - drag.startY) > MOVE_THRESHOLD) {
      drag.moved = true;
      document.body.style.userSelect = "none";
      document.body.style.cursor = "grabbing";
    }
    if (drag.moved && strip) {
      const box = strip.getBoundingClientRect();
      drag.inside =
        drag.x > box.left - OUTSIDE_MARGIN && drag.x < box.right + OUTSIDE_MARGIN && drag.y > box.top - OUTSIDE_MARGIN && drag.y < box.bottom + OUTSIDE_MARGIN;
      drag.insertAt = landingIndex(drag.x);
    }
    drag = drag;
  }

  // The icon lands after every icon whose middle is to its left.
  function landingIndex(x: number): number {
    const middles = Array.from(strip.querySelectorAll<HTMLElement>("[data-dock-item]")).map((el) => {
      const r = el.getBoundingClientRect();
      return r.left + r.width / 2;
    });
    return middles.filter((m) => m < x).length;
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === "Escape") finishDrag(true);
  }

  function onUp() {
    finishDrag(false);
  }

  function finishDrag(cancel: boolean) {
    const current = drag;
    drag = null;
    stopListening();
    if (!current) return;

    // A press that did not move is a click.
    if (!current.moved) {
      if (current.source === "available") selectedAvailable = current.app.desktopPath;
      else selectedPinned = current.app.desktopPath;
      return;
    }
    if (cancel) return;

    if (current.inside) {
      if (current.source === "available" && pinned.some((p) => p.desktopPath === current.app.desktopPath || sameApp(p, current.app))) {
        status = $_("dock.alreadyPinned", { values: { name: current.app.name } });
        return;
      }
      const rest = pinned.filter((p) => p.desktopPath !== current.app.desktopPath);
      pinned = [...rest.slice(0, current.insertAt), current.app, ...rest.slice(current.insertAt)];
      selectedPinned = current.app.desktopPath;
      status = current.source === "available" ? $_("dock.added", { values: { name: current.app.name } }) : $_("dock.unsaved");
    } else if (current.source === "pinned") {
      remove(current.app);
    }
  }

  // The same program can be named by its file name or by its full path.
  function sameApp(a: DockApp, b: DockApp): boolean {
    const id = (p: string) => (p.split("/").pop() ?? p).replace(/\.desktop$/, "");
    return id(a.desktopPath) === id(b.desktopPath);
  }

  // ── Other ways to edit ─────────────────────────────────────────────────────

  function remove(app: DockApp) {
    pinned = pinned.filter((p) => p.desktopPath !== app.desktopPath);
    if (selectedPinned === app.desktopPath) selectedPinned = null;
    status = $_("dock.removed");
  }

  function pinSelected() {
    const app = available.find((a) => a.desktopPath === selectedAvailable);
    if (!app) return;
    if (pinned.some((p) => sameApp(p, app))) {
      status = $_("dock.alreadyPinned", { values: { name: app.name } });
      return;
    }
    pinned = [...pinned, app];
    selectedPinned = app.desktopPath;
    status = $_("dock.added", { values: { name: app.name } });
  }

  function move(app: DockApp, by: number) {
    const from = pinned.findIndex((p) => p.desktopPath === app.desktopPath);
    const to = from + by;
    if (from < 0 || to < 0 || to >= pinned.length) return;
    const copy = [...pinned];
    copy.splice(to, 0, copy.splice(from, 1)[0]);
    pinned = copy;
    status = $_("dock.unsaved");
  }

  async function keyOnPinned(event: KeyboardEvent, app: DockApp) {
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      remove(app);
    } else if (event.altKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
      event.preventDefault();
      move(app, event.key === "ArrowLeft" ? -1 : 1);
      // The icon is drawn again: keep the focus on it.
      await tick();
      strip.querySelector<HTMLElement>(`[data-path="${CSS.escape(app.desktopPath)}"]`)?.focus();
    }
  }

  async function save() {
    try {
      await withProgress(() => dockSave(current, pinned.map((p) => p.desktopPath)));
      status = $_("dock.saved");
    } catch {
      status = $_("dock.saveError");
    }
  }
</script>

{#if active === false}
  <div class="h-full min-h-[18rem] flex flex-col items-center justify-center gap-4 text-center">
    <div class="w-16 h-16 rounded-2xl bg-surface-2 grid place-items-center text-primary"><Info size={30} /></div>
    <p class="text-[0.95rem] text-text-bright">{$_("dock.missing.title")}</p>
    <p class="text-[0.85rem] text-text-muted max-w-[28rem]">{$_("dock.missing.hint")}</p>
    <button class="flex items-center gap-2 px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.85rem]" on:click={load}>
      <RefreshCw size={16} />{$_("dock.refresh")}
    </button>
  </div>
{:else if active}
  <div class="h-full flex flex-col gap-4">
    <div class="flex items-baseline justify-between gap-3">
      <div class="flex items-baseline gap-3 min-w-0">
        <h2 class="text-[1.25rem] font-semibold text-text-bright">{$_("dock.title")}</h2>
        {#if docks.length > 1}
          <select
            class="bg-surface border border-border rounded px-2 py-1 text-[0.82rem] text-text-bright outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
            value={current}
            on:change={chooseDock}
            aria-label={$_("dock.which")}
          >
            {#each docks as d}
              <option value={d.id}>{$_("dock.ofPanel", { values: { n: d.panel ?? "?" } })}</option>
            {/each}
          </select>
        {/if}
      </div>
      <span class="text-[0.8rem] text-text-muted truncate">{status}</span>
    </div>

    <!-- The dock: the preview is the editor. -->
    <div class="bg-surface border border-border rounded p-4 flex flex-col items-center gap-2.5">
      <div class="self-start text-[0.85rem]">
        <span class="font-semibold text-text-bright">{$_("dock.preview")}</span>
        <span class="text-text-muted ml-1.5">{$_("dock.previewHint")}</span>
      </div>
      <div class="w-full flex justify-center" bind:clientWidth={room}>
      <div
        bind:this={strip}
        class="content-scroll max-w-full overflow-x-auto grid grid-flow-col auto-cols-max items-center justify-center gap-2 px-3 py-3 rounded-2xl border transition-colors {drag?.moved && drag.inside ? 'border-primary bg-primary/10' : 'border-border bg-panel'}"
        style="min-height: {iconSize + 24}px"
      >
        {#each shown as app, i (app.desktopPath)}
          {#if drag?.moved && drag.inside && drag.insertAt === i}
            <div class="rounded-lg border-2 border-dashed border-primary" style="width: {iconSize}px; height: {iconSize}px"></div>
          {/if}
          <div class="relative group" style="width: {iconSize}px" data-dock-item>
            <button
              data-path={app.desktopPath}
              class="w-full aspect-square grid place-items-center rounded-lg transition-colors touch-none cursor-grab {selectedPinned === app.desktopPath ? 'bg-primary/20 ring-2 ring-primary/60' : 'hover:bg-surface-2'}"
              title={app.name}
              on:pointerdown={(e) => startDrag(e, "pinned", app)}
              on:keydown={(e) => keyOnPinned(e, app)}
            >
              {#if app.icon}
                <img src={app.icon} alt={app.name} draggable="false" class="w-[80%] h-[80%] object-contain pointer-events-none" />
              {:else}
                <AppWindow size={Math.round(iconSize * 0.55)} class="text-text-muted" />
              {/if}
            </button>
            <button
              class="absolute -top-1.5 -right-1.5 w-5 h-5 rounded-full bg-danger text-white grid place-items-center opacity-0 group-hover:opacity-100 focus:opacity-100 transition-opacity"
              title={$_("dock.remove")}
              aria-label={$_("dock.remove")}
              on:pointerdown|stopPropagation
              on:click={() => remove(app)}
            >
              <X size={12} />
            </button>
          </div>
        {/each}
        {#if drag?.moved && drag.inside && drag.insertAt >= shown.length}
          <div class="rounded-lg border-2 border-dashed border-primary" style="width: {iconSize}px; height: {iconSize}px"></div>
        {/if}
        {#if shown.length === 0 && !(drag?.moved && drag.inside)}
          <span class="px-3 text-[0.85rem] text-text-muted">{$_("dock.empty")}</span>
        {/if}
      </div>
      </div>
      <span class="text-[0.75rem] text-text-muted">{$_("dock.dragOut")}</span>
    </div>

    <!-- The applications that can be pinned. -->
    <div class="flex flex-col flex-1 min-h-0 gap-2.5">
      <div class="flex items-center gap-3">
        <h3 class="text-[0.95rem] font-semibold text-text-bright shrink-0">{$_("dock.available")}</h3>
        <div class="relative flex-1 min-w-0">
          <Search size={15} class="absolute left-3 top-1/2 -translate-y-1/2 text-text-muted" />
          <input
            type="search"
            bind:value={query}
            placeholder={$_("dock.search")}
            class="w-full bg-surface border border-border rounded pl-9 pr-3 py-2 text-[0.85rem] text-text-bright outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
          />
        </div>
        <button
          class="flex items-center gap-2 px-4 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold shrink-0 disabled:opacity-50 disabled:hover:bg-primary"
          disabled={!selectedAvailable}
          on:click={pinSelected}
        >
          <Pin size={15} />{$_("dock.pin")}
        </button>
      </div>

      <div class="content-scroll flex-1 min-h-0 overflow-y-auto bg-surface border border-border rounded p-2">
        <div class="grid gap-1 [grid-template-columns:repeat(auto-fill,minmax(210px,1fr))]">
          {#each filtered as app (app.desktopPath)}
            <button
              class="flex items-center gap-3 px-2.5 py-2 rounded text-left transition-colors touch-none cursor-grab {selectedAvailable === app.desktopPath ? 'bg-primary/15' : 'hover:bg-surface-2'}"
              title={app.name}
              on:pointerdown={(e) => startDrag(e, "available", app)}
              on:dblclick={() => {
                selectedAvailable = app.desktopPath;
                pinSelected();
              }}
            >
              {#if app.icon}
                <img src={app.icon} alt="" draggable="false" class="w-8 h-8 object-contain shrink-0 pointer-events-none" />
              {:else}
                <AppWindow size={26} class="w-8 shrink-0 text-text-muted" />
              {/if}
              <span class="truncate text-[0.82rem] text-text">{app.name}</span>
            </button>
          {/each}
        </div>
      </div>
    </div>

    <div class="flex justify-center">
      <button class="flex items-center gap-2 px-6 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.85rem] font-semibold" on:click={save}>
        <Save size={16} />{$_("dock.save")}
      </button>
    </div>
  </div>

  <!-- The icon that follows the pointer while it is dragged. -->
  {#if drag?.moved}
    <div class="fixed z-50 pointer-events-none -translate-x-1/2 -translate-y-1/2 opacity-90" style="left: {drag.x}px; top: {drag.y}px">
      {#if drag.app.icon}
        <img src={drag.app.icon} alt="" class="w-11 h-11 object-contain drop-shadow-lg" />
      {:else}
        <AppWindow size={36} class="text-text-bright drop-shadow-lg" />
      {/if}
    </div>
  {/if}
{/if}
