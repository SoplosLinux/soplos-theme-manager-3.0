<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { _ } from "svelte-i18n";
  import { AppWindow, ArrowDown, ArrowUp, Minus, Plus, Save, Search, Settings, Trash2 } from "lucide-svelte";
  import {
    panelList,
    panelRead,
    panelAvailable,
    panelApplyMany,
    panelSetOrder,
    panelRemovePlugin,
    panelAddPlugin,
    panelPreferences,
    panelNew,
    panelDelete,
    type PanelPlugin,
    type PanelSettings,
  } from "../lib/api";
  import { withProgress } from "../lib/progress";
  import Select from "../components/Select.svelte";
  import NumberField from "../components/NumberField.svelte";
  import Switch from "../components/Switch.svelte";
  import Modal from "../components/Modal.svelte";

  type Edge = PanelSettings["edge"];

  let loaded = false;
  let status = "";
  let ids: number[] = [];
  let current = 1;

  // The settings of the panel on show.
  let edge: Edge = "bottom";
  let align = 1;
  let mode = 0;
  let size = 42;
  let iconSize = 24;
  let length = 100;
  let lengthAdjust = false;
  let rows = 1;
  let autohide = 0;
  let dark = false;
  let locked = false;
  // What xfconf had, to tell what changed and to leave the position alone when
  // nothing about it did.
  let old = { position: "", edge: "bottom" as Edge, align: 1, length: 100 };

  let active: PanelPlugin[] = [];

  // What is on screen for each panel, kept when the tab changes so the changes
  // of every panel are applied together, and the text of what xfconf had for
  // each one, to tell which panels changed.
  interface Draft {
    settings: { edge: Edge; align: number; mode: number; size: number; iconSize: number; length: number; lengthAdjust: boolean; rows: number; autohide: number; dark: boolean; locked: boolean };
    old: typeof old;
    active: PanelPlugin[];
  }
  let drafts: Record<number, Draft> = {};
  let bases: Record<number, string> = {};

  const snapshot = (): Draft => ({
    settings: { edge, align, mode, size, iconSize, length, lengthAdjust, rows, autohide, dark, locked },
    old: { ...old },
    active: [...active],
  });
  const fingerprint = (d: Draft) => JSON.stringify([d.settings, d.active.map((p) => p.id)]);

  function show(d: Draft) {
    ({ edge, align, mode, size, iconSize, length, lengthAdjust, rows, autohide, dark, locked } = d.settings);
    old = { ...d.old };
    active = [...d.active];
    if (!active.some((p) => p.id === selectedActive)) selectedActive = null;
  }

  // The panels with something not applied yet.
  $: live = { edge, align, mode, size, iconSize, length, lengthAdjust, rows, autohide, dark, locked, active };
  $: changed = ids.map((id) => {
    if (bases[id] === undefined) return false;
    const d: Draft | undefined = id === current ? { settings: { edge: live.edge, align: live.align, mode: live.mode, size: live.size, iconSize: live.iconSize, length: live.length, lengthAdjust: live.lengthAdjust, rows: live.rows, autohide: live.autohide, dark: live.dark, locked: live.locked }, old, active: live.active } : drafts[id];
    return d ? fingerprint(d) !== bases[id] : false;
  });

  let available: PanelPlugin[] = [];
  let query = "";
  let selectedActive: number | null = null;
  let selectedAvailable: string | null = null;
  let confirmDelete = false;

  async function load(id: number) {
    const state = await withProgress(() => panelRead(id));
    current = id;
    const s = state.settings;
    edge = s.edge;
    align = s.align;
    mode = s.mode;
    size = s.size;
    iconSize = s.iconSize;
    length = s.length;
    lengthAdjust = s.lengthAdjust;
    rows = s.rows;
    autohide = s.autohide;
    dark = s.dark;
    locked = s.locked;
    old = { position: s.position, edge: s.edge, align: s.align, length: s.length };
    active = state.active;
    if (!active.some((p) => p.id === selectedActive)) selectedActive = null;
    drafts[id] = snapshot();
    bases[id] = fingerprint(drafts[id]);
  }

  // A tab: the panel keeps what was changed in the one that is left.
  async function select(id: number) {
    if (id === current) return;
    if (ids.includes(current)) drafts[current] = snapshot();
    if (drafts[id]) {
      current = id;
      show(drafts[id]);
    } else {
      await load(id);
    }
  }

  onMount(async () => {
    await withProgress(async () => {
      ids = await panelList();
      available = await panelAvailable();
    });
    await load(ids[0]);
    loaded = true;
  });

  onDestroy(() => stopDragging());

  // A panel on a side runs up and down, and one on the top or bottom runs across:
  // the mode follows the edge. A deskbar is left as the user chose it.
  function changeEdge(value: string) {
    const next = value as Edge;
    edge = next;
    if (mode !== 2) mode = next === "left" || next === "right" ? 1 : 0;
  }

  // ── Words ──────────────────────────────────────────────────────────────────

  $: vertical = edge === "left" || edge === "right";
  $: edgeOptions = [
    { value: "top", label: $_("panel.position.top") },
    { value: "bottom", label: $_("panel.position.bottom") },
    { value: "left", label: $_("panel.position.left") },
    { value: "right", label: $_("panel.position.right") },
  ];
  // Along a horizontal panel the places are left, center and right; along a
  // vertical one, top, center and bottom.
  $: alignOptions = [
    { value: "0", label: vertical ? $_("panel.position.alignTop") : $_("panel.position.alignLeft") },
    { value: "1", label: $_("panel.position.alignCenter") },
    { value: "2", label: vertical ? $_("panel.position.alignBottom") : $_("panel.position.alignRight") },
  ];
  $: modeOptions = [
    { value: "0", label: $_("panel.appearance.horizontal") },
    { value: "1", label: $_("panel.appearance.vertical") },
    { value: "2", label: $_("panel.appearance.deskbar") },
  ];
  $: autohideOptions = [
    { value: "0", label: $_("panel.behavior.never") },
    { value: "1", label: $_("panel.behavior.intelligent") },
    { value: "2", label: $_("panel.behavior.always") },
  ];
  $: preview =
    length >= 100
      ? $_("panel.preview.full", { values: { edge: $_(`panel.preview.${edge}`) } })
      : $_("panel.preview.part", { values: { length, align: $_(`panel.preview.align${align}`), edge: $_(`panel.preview.${edge}`) } });

  // The bar of the preview: where the panel sits on a small screen.
  const PREVIEW_W = 160;
  const PREVIEW_H = 90;
  const THICKNESS = 5;
  $: bar = (() => {
    const span = (whole: number) => (whole - 4) * (Math.max(1, Math.min(100, length)) / 100);
    const along = (whole: number, long: number) => (align === 0 ? 2 : align === 2 ? whole - 2 - long : 2 + (whole - 4 - long) / 2);
    if (edge === "top" || edge === "bottom") {
      const w = span(PREVIEW_W);
      return { left: along(PREVIEW_W, w), top: edge === "top" ? 2 : PREVIEW_H - 2 - THICKNESS, width: w, height: THICKNESS };
    }
    const h = span(PREVIEW_H);
    return { left: edge === "left" ? 2 : PREVIEW_W - 2 - THICKNESS, top: along(PREVIEW_H, h), width: THICKNESS, height: h };
  })();

  // ── Panels ─────────────────────────────────────────────────────────────────

  async function addPanel() {
    try {
      const created = await withProgress(() => panelNew());
      ids = created.ids;
      await load(created.id);
    } catch {
      status = $_("panel.saveError");
    }
  }

  async function removePanel() {
    confirmDelete = false;
    try {
      const removed = current;
      ids = await withProgress(() => panelDelete(removed));
      delete drafts[removed];
      delete bases[removed];
      await select(ids[0]);
    } catch {
      status = $_("panel.saveError");
    }
  }

  // ── Saving ─────────────────────────────────────────────────────────────────

  async function save() {
    status = "";
    drafts[current] = snapshot();
    // Every panel that changed, not only the one on show. When none did, the
    // one on show is applied anyway, as before.
    let wanted = ids.filter((id) => drafts[id] && fingerprint(drafts[id]) !== bases[id]);
    if (wanted.length === 0) wanted = [current];
    try {
      await withProgress(() =>
        panelApplyMany(
          wanted.map((id) => {
            const d = drafts[id];
            const s = d.settings;
            return {
              id,
              change: {
                ...s,
                oldPosition: d.old.position,
                // The position is only worked out again if the edge, the alignment or the length changed.
                positionChanged: s.edge !== d.old.edge || s.align !== d.old.align || s.length !== d.old.length,
                pluginIds: d.active.map((p) => p.id),
              },
            };
          }),
        ),
      );
      status = $_("panel.saved");
    } catch {
      status = $_("panel.saveError");
    }
    // What xfconf has now, for every panel.
    drafts = {};
    bases = {};
    await load(current);
  }

  // ── Plugins ────────────────────────────────────────────────────────────────

  async function changePlugins(job: () => Promise<unknown>) {
    try {
      await withProgress(job);
    } catch {
      status = $_("panel.saveError");
    }
    await load(current);
  }

  const move = (by: number) => {
    const from = active.findIndex((p) => p.id === selectedActive);
    const to = from + by;
    if (from < 0 || to < 0 || to >= active.length) return;
    const order = active.map((p) => p.id);
    order.splice(to, 0, order.splice(from, 1)[0]);
    void changePlugins(() => panelSetOrder(current, order));
  };

  const removeSelected = () => {
    if (selectedActive !== null) void changePlugins(() => panelRemovePlugin(current, selectedActive!));
  };

  const addModule = (module: string) => void changePlugins(() => panelAddPlugin(current, module));

  $: shownAvailable = available.filter((p) => p.name.toLowerCase().includes(query.trim().toLowerCase()));

  // ── Dragging a plugin of the list to another place ─────────────────────────
  // The new order is kept on screen and written with "Save & Apply".

  let dragging: { plugin: PanelPlugin; startY: number; y: number; moved: boolean; insertAt: number } | null = null;
  let listEl: HTMLElement;

  function startDrag(event: PointerEvent, plugin: PanelPlugin) {
    if (event.button !== 0) return;
    selectedActive = plugin.id;
    dragging = { plugin, startY: event.clientY, y: event.clientY, moved: false, insertAt: active.findIndex((p) => p.id === plugin.id) };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("keydown", onKey);
  }

  function stopDragging() {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("keydown", onKey);
    document.body.style.userSelect = "";
  }

  function onMove(event: PointerEvent) {
    if (!dragging) return;
    dragging.y = event.clientY;
    if (!dragging.moved && Math.abs(dragging.y - dragging.startY) > 5) {
      dragging.moved = true;
      document.body.style.userSelect = "none";
    }
    if (dragging.moved && listEl) {
      // The plugin lands after every row whose middle is above the pointer.
      const rows = Array.from(listEl.querySelectorAll<HTMLElement>("[data-active-row]"));
      const middles = rows.map((el) => el.getBoundingClientRect()).map((r) => r.top + r.height / 2);
      dragging.insertAt = middles.filter((m) => m < dragging!.y).length;
    }
    dragging = dragging;
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === "Escape") finishDrag(true);
  }

  function onUp() {
    finishDrag(false);
  }

  function finishDrag(cancel: boolean) {
    const current_ = dragging;
    dragging = null;
    stopDragging();
    if (!current_ || !current_.moved || cancel) return;
    const rest = active.filter((p) => p.id !== current_.plugin.id);
    active = [...rest.slice(0, current_.insertAt), current_.plugin, ...rest.slice(current_.insertAt)];
    status = $_("panel.unsaved");
  }

  $: listed = dragging?.moved ? active.filter((p) => p.id !== dragging!.plugin.id) : active;

  // ── Looks ──────────────────────────────────────────────────────────────────

  const card = "bg-surface border border-border rounded p-3 flex flex-col gap-2";
  const heading = "text-[0.88rem] font-semibold text-text-bright";
  const hint = "text-[0.76rem] font-normal text-text-muted ml-1.5";
  const label = "text-[0.76rem] text-text-muted leading-snug";
  const iconButton = "w-8 h-8 grid place-items-center rounded bg-surface-2 hover:bg-border text-text-bright disabled:opacity-40 disabled:hover:bg-surface-2";
</script>

<div class="flex flex-col gap-3 pb-2">
  <div class="flex items-baseline justify-between gap-3">
    <h2 class="text-[1.15rem] font-semibold text-text-bright">{$_("panel.title")}</h2>
    <span class="text-[0.78rem] text-text-muted truncate">{status}</span>
  </div>

  {#if loaded}
    <!-- The panels, and the button that saves everything. -->
    <div class="flex items-center gap-2">
      <div class="inline-flex rounded overflow-hidden border border-border">
        {#each ids as id}
          <button
            class="px-3.5 py-1.5 text-[0.82rem] transition-colors {current === id ? 'bg-primary text-text-bright font-semibold' : 'bg-surface-2 text-text-muted hover:text-text-bright'}"
            aria-pressed={current === id}
            on:click={() => select(id)}
          >
            {$_("panel.panelN", { values: { n: id } })}{#if changed[ids.indexOf(id)]}<span class="ml-1.5 inline-block w-1.5 h-1.5 rounded-full bg-current opacity-70" title={$_("panel.notApplied")}></span>{/if}
          </button>
        {/each}
      </div>
      <button class={iconButton} title={$_("panel.addPanel")} aria-label={$_("panel.addPanel")} on:click={addPanel}><Plus size={15} /></button>
      <button class={iconButton} title={$_("panel.deletePanel")} aria-label={$_("panel.deletePanel")} disabled={ids.length < 2} on:click={() => (confirmDelete = true)}><Minus size={15} /></button>
      <button class="ml-auto flex items-center gap-2 px-5 py-1.5 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={save}>
        <Save size={15} />{$_("panel.save")}
      </button>
    </div>

    <div class="flex gap-3 items-stretch">
      <!-- Where the panel will be on a small screen. -->
      <div class="{card} items-center justify-center shrink-0 w-[200px]">
        <div class="relative rounded border border-border bg-surface-2 shrink-0" style="width: {PREVIEW_W}px; height: {PREVIEW_H}px">
          <div class="absolute rounded-[1px]" style="background: #ff8800; left: {bar.left}px; top: {bar.top}px; width: {bar.width}px; height: {bar.height}px"></div>
        </div>
        <p class="text-[0.76rem] text-text-muted text-center leading-snug">{preview}</p>
      </div>

      <div class="{card} flex-1 min-w-0">
        <h3 class={heading}>{$_("panel.position.title")}<span class={hint}>{$_("panel.position.hint")}</span></h3>
        <div class="grid grid-cols-2 gap-x-4 gap-y-2 items-end">
          <div class="flex flex-col gap-1">
            <span class={label}>{$_("panel.position.edge")}</span>
            <Select value={edge} options={edgeOptions} label={$_("panel.position.edge")} on:change={(e) => changeEdge(e.detail)} />
          </div>
          <div class="flex flex-col gap-1">
            <span class={label}>{$_("panel.position.align")}</span>
            <Select value={String(align)} options={alignOptions} label={$_("panel.position.align")} on:change={(e) => (align = parseInt(e.detail, 10))} />
          </div>
          <div class="flex flex-col gap-1">
            <span class={label}>{$_("panel.position.length")}</span>
            <NumberField bind:value={length} min={1} max={100} label={$_("panel.position.length")} />
          </div>
          <div class="flex items-center justify-between gap-3 pb-1.5">
            <span class={label}>{$_("panel.position.lock")}</span>
            <Switch bind:checked={locked} label={$_("panel.position.lock")} />
          </div>
        </div>
      </div>
    </div>

    <div class="flex gap-3 items-stretch">
      <div class="{card} flex-1 min-w-0">
        <h3 class={heading}>{$_("panel.appearance.title")}<span class={hint}>{$_("panel.appearance.hint")}</span></h3>
        <div class="grid grid-cols-2 gap-x-4 gap-y-2 items-end">
          <div class="flex flex-col gap-1">
            <span class={label}>{$_("panel.appearance.mode")}</span>
            <Select value={String(mode)} options={modeOptions} label={$_("panel.appearance.mode")} on:change={(e) => (mode = parseInt(e.detail, 10))} />
          </div>
          <div class="flex flex-col gap-1">
            <span class={label}>{$_("panel.appearance.height")}</span>
            <NumberField bind:value={size} min={16} max={128} label={$_("panel.appearance.height")} />
          </div>
          <div class="flex flex-col gap-1">
            <span class={label}>{$_("panel.appearance.iconSize")}</span>
            <NumberField bind:value={iconSize} min={8} max={96} label={$_("panel.appearance.iconSize")} />
          </div>
          <div class="flex flex-col gap-1">
            <span class={label}>{$_("panel.appearance.rows")}</span>
            <NumberField bind:value={rows} min={1} max={6} label={$_("panel.appearance.rows")} />
          </div>
          <div class="flex items-center justify-between gap-3">
            <span class={label}>{$_("panel.appearance.dark")}</span>
            <Switch bind:checked={dark} label={$_("panel.appearance.dark")} />
          </div>
        </div>
      </div>

      <div class="{card} w-[230px] shrink-0">
        <h3 class={heading}>{$_("panel.behavior.title")}<span class={hint}>{$_("panel.behavior.hint")}</span></h3>
        <div class="flex flex-col gap-2">
          <div class="flex flex-col gap-1">
            <span class={label}>{$_("panel.behavior.autohide")}</span>
            <Select value={String(autohide)} options={autohideOptions} label={$_("panel.behavior.autohide")} on:change={(e) => (autohide = parseInt(e.detail, 10))} />
          </div>
          <div class="flex items-center justify-between gap-3 mt-1">
            <span class={label}>{$_("panel.behavior.autoExpand")}</span>
            <Switch bind:checked={lengthAdjust} label={$_("panel.behavior.autoExpand")} />
          </div>
        </div>
      </div>
    </div>

    <!-- The plugins of the panel, and the ones that can be added. -->
    <div class={card}>
      <h3 class={heading}>{$_("panel.plugins.title")}</h3>
      <div class="flex gap-4 items-stretch">
        <div class="w-[250px] shrink-0 flex flex-col gap-1.5">
          <span class={label}>{$_("panel.plugins.active")}</span>
          <div bind:this={listEl} class="content-scroll h-[220px] overflow-y-auto bg-surface-2 border border-border rounded p-1 flex flex-col gap-0.5">
            {#each listed as plugin, i (plugin.id)}
              {#if dragging?.moved && dragging.insertAt === i}
                <div class="h-8 rounded border-2 border-dashed border-primary shrink-0"></div>
              {/if}
              <button
                data-active-row
                class="flex items-center gap-2 px-2 py-1 rounded text-left transition-colors touch-none cursor-grab {selectedActive === plugin.id ? 'bg-primary/15' : 'hover:bg-border'}"
                title={plugin.module}
                on:pointerdown={(e) => startDrag(e, plugin)}
              >
                {#if plugin.icon}
                  <img src={plugin.icon} alt="" draggable="false" class="w-6 h-6 object-contain shrink-0 pointer-events-none" />
                {:else}
                  <AppWindow size={20} class="w-6 shrink-0 text-text-muted" />
                {/if}
                <span class="truncate text-[0.82rem] text-text">{plugin.name}</span>
              </button>
            {/each}
            {#if dragging?.moved && dragging.insertAt >= listed.length}
              <div class="h-8 rounded border-2 border-dashed border-primary shrink-0"></div>
            {/if}
          </div>
          <div class="flex justify-center gap-1.5">
            <button class={iconButton} title={$_("panel.plugins.moveUp")} aria-label={$_("panel.plugins.moveUp")} disabled={selectedActive === null} on:click={() => move(-1)}><ArrowUp size={15} /></button>
            <button class={iconButton} title={$_("panel.plugins.moveDown")} aria-label={$_("panel.plugins.moveDown")} disabled={selectedActive === null} on:click={() => move(1)}><ArrowDown size={15} /></button>
            <button class={iconButton} title={$_("panel.plugins.remove")} aria-label={$_("panel.plugins.remove")} disabled={selectedActive === null} on:click={removeSelected}><Trash2 size={15} /></button>
            <button class={iconButton} title={$_("panel.plugins.settings")} aria-label={$_("panel.plugins.settings")} disabled={selectedActive === null} on:click={() => panelPreferences(current)}><Settings size={15} /></button>
          </div>
        </div>

        <div class="flex-1 min-w-0 flex flex-col gap-1.5">
          <div class="flex items-baseline justify-between gap-3">
            <span class={label}>{$_("panel.plugins.available")}</span>
            <span class={label}>{$_("panel.plugins.doubleClick")}</span>
          </div>
          <div class="relative">
            <Search size={14} class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
            <input
              type="search"
              bind:value={query}
              placeholder={$_("panel.plugins.search")}
              class="w-full bg-surface-2 border border-border rounded pl-8 pr-2.5 py-[6px] text-[0.82rem] text-text-bright outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
            />
          </div>
          <div class="content-scroll h-[160px] overflow-y-auto bg-surface-2 border border-border rounded p-1 flex flex-col gap-0.5">
            {#each shownAvailable as plugin (plugin.module)}
              <button
                class="flex items-center gap-2 px-2 py-1 rounded text-left transition-colors {selectedAvailable === plugin.module ? 'bg-primary/15' : 'hover:bg-border'}"
                on:click={() => (selectedAvailable = plugin.module)}
                on:dblclick={() => addModule(plugin.module)}
              >
                {#if plugin.icon}
                  <img src={plugin.icon} alt="" draggable="false" class="w-6 h-6 object-contain shrink-0" />
                {:else}
                  <AppWindow size={20} class="w-6 shrink-0 text-text-muted" />
                {/if}
                <span class="min-w-0">
                  <span class="block truncate text-[0.82rem] text-text-bright">{plugin.name}</span>
                  {#if plugin.comment}<span class="block truncate text-[0.72rem] text-text-muted">{plugin.comment}</span>{/if}
                </span>
              </button>
            {/each}
          </div>
          <div class="flex justify-center">
            <button
              class="flex items-center gap-2 px-4 py-1.5 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem] disabled:opacity-50 disabled:hover:bg-surface-2"
              disabled={!selectedAvailable}
              on:click={() => selectedAvailable && addModule(selectedAvailable)}
            >
              <Plus size={15} />{$_("panel.plugins.add")}
            </button>
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>

<!-- The row that follows the pointer while a plugin is dragged. -->
{#if dragging?.moved}
  <div class="fixed z-50 pointer-events-none opacity-90 bg-surface border border-primary rounded px-2 py-1 flex items-center gap-2 text-[0.82rem] text-text-bright shadow-lg" style="left: 50%; top: {dragging.y}px; transform: translate(-50%, -50%)">
    {#if dragging.plugin.icon}<img src={dragging.plugin.icon} alt="" class="w-5 h-5 object-contain" />{/if}
    {dragging.plugin.name}
  </div>
{/if}

{#if confirmDelete}
  <Modal title={$_("panel.deleteTitle", { values: { name: $_("panel.panelN", { values: { n: current } }) } })} onClose={() => (confirmDelete = false)}>
    <p>{$_("panel.deleteText")}</p>
    <svelte:fragment slot="actions">
      <button data-autofocus class="px-4 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={() => (confirmDelete = false)}>{$_("common.cancel")}</button>
      <button class="px-4 py-2 rounded bg-danger hover:opacity-90 text-white text-[0.82rem] font-semibold" on:click={removePanel}>{$_("panel.deletePanel")}</button>
    </svelte:fragment>
  </Modal>
{/if}
