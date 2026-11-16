<script lang="ts">
  import { onMount } from "svelte";
  import { _ } from "svelte-i18n";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { ImageOff, Save, User } from "lucide-svelte";
  import { lightdmRead, lightdmFonts, lightdmWrite, allowImage, listThemes, pickImage, userAvatar, setUserAvatar, desktopWallpaper, makeThumbnail } from "../lib/api";
  import { withProgress } from "../lib/progress";
  import Modal from "../components/Modal.svelte";
  import Select from "../components/Select.svelte";
  import NumberField from "../components/NumberField.svelte";
  import Switch from "../components/Switch.svelte";

  type Anchor = "start" | "center" | "end";
  const ANCHORS: Anchor[] = ["start", "center", "end"];
  // Where an offset goes when a cell of the grid is clicked: anchor alone would
  // barely move the box if the offset stayed where it was.
  const DEFAULT_OFFSET: Record<Anchor, number> = { start: 5, center: 50, end: 95 };

  // The page fills the height it has. Every size is worked out from it: the
  // natural height of the page grows with `density`, from H_COMPACT at 0 to
  // H_FULL at 1, so the density that makes it the height available is solved
  // here instead of being guessed. It goes a little past 1 for tall windows.
  const H_COMPACT = 550;
  const H_FULL = 718;
  let available = 0;
  $: density = Math.max(0, Math.min(1.4, (available - 8 - H_COMPACT) / (H_FULL - H_COMPACT)));
  const lerp = (a: number, b: number, d: number) => (a + (b - a) * d).toFixed(2);
  $: sizes = [
    `--pad:${lerp(12, 17, density)}px`,
    `--gap:${lerp(12, 17, density)}px`,
    `--row:${lerp(8, 13, density)}px`,
    `--fpy:${lerp(6, 9.5, density)}px`,
    `--fs:${lerp(0.82, 0.9, density)}rem`,
    `--ls:${lerp(0.76, 0.84, density)}rem`,
    `--nb:${lerp(28, 36, density)}px`,
    `--bpy:${lerp(6, 9, density)}px`,
    `--ts:${lerp(1.15, 1.3, density)}rem`,
  ].join(";");
  $: backgroundWidth = Math.round(160 + 56 * density);
  $: avatarSize = Math.round(30 + 16 * density);

  let loaded = false;
  let status = "";
  let saving = false;

  let background = "";
  let userImage = "";
  let gtkTheme = "";
  let iconTheme = "";
  let cursorTheme = "";
  let fontFamily = "";
  let fontSize = 10;
  let xAnchor: Anchor = "center";
  let yAnchor: Anchor = "center";
  let xOffset = 50;
  let yOffset = 50;
  // What the file had, to show it as it is and to leave it alone when the grid
  // cannot represent it and nobody touched it.
  let originalPosition = "";
  let shownX: Dimension = { value: 50, percent: true, negative: false, anchor: "center" };
  let shownY: Dimension = { value: 50, percent: true, negative: false, anchor: "center" };
  let representable = true;
  let panelAtBottom = false;
  let showAvatar = true;
  // Where the login screen takes its background from: the one that is chosen here
  // or the one of the desktop (the "user-background" key, which the greeter
  // takes as true when the file does not have it).
  let backgroundSource: "fixed" | "desktop" = "desktop";
  $: userBackground = backgroundSource === "desktop";
  let desktopImage: string | null = null;
  $: shownBackground = userBackground ? desktopImage ?? "" : background;

  // The previews show the small copy that the wallpapers page also uses (made
  // once and kept in the cache), not the picture of several megabytes. A
  // picture whose copy cannot be made shows itself.
  let copies: Record<string, string | null> = {};
  async function copyOf(path: string) {
    if (!path || path in copies) return;
    let copy: string | null = null;
    try {
      copy = await makeThumbnail(path);
    } catch {
      copy = null;
    }
    copies = { ...copies, [path]: copy };
  }
  $: void copyOf(shownBackground);
  $: void copyOf(userImage);
  $: sourceOptions = [
    { value: "fixed", label: $_("login.backgroundFixed") },
    { value: "desktop", label: $_("login.backgroundDesktop") },
  ];
  let clockFormat = "";

  let gtkThemes: string[] = [];
  let iconThemes: string[] = [];
  let cursorThemes: string[] = [];
  let fonts: string[] = [];

  // ── The position of the greeter ──
  //
  // lightdm-gtk-greeter reads "x y", each "value[%][,anchor]": a value in pixels
  // or in percent of the screen, negative to count from the right or the bottom,
  // and an anchor, the part of the login box that sits at that point (start,
  // center or end). It can also have a size after a ";". The grid below can show
  // percentages that are not negative; anything else in the file is kept as it
  // is until a cell of the grid or an offset is touched.

  type Dimension = { value: number; percent: boolean; negative: boolean; anchor: Anchor };

  function readDimension(text: string): Dimension | null {
    const [value, anchor] = text.split(",");
    const match = value.trim().match(/^(-?)(\d+)(%?)$/);
    if (!match) return null;
    const negative = match[1] === "-";
    const kind = (anchor ?? "").trim() as Anchor;
    return {
      value: parseInt(match[2], 10),
      percent: match[3] === "%",
      negative,
      // With no anchor the greeter uses the start for a plain value and the end for a negative one.
      anchor: ANCHORS.includes(kind) ? kind : negative ? "end" : "start",
    };
  }

  function readPosition(text: string | undefined): { x: Dimension; y: Dimension; sized: boolean } {
    const fallback: Dimension = { value: 50, percent: true, negative: false, anchor: "center" };
    if (!text) return { x: fallback, y: fallback, sized: false };
    const sized = text.includes(";");
    const [xs, ys] = text.split(";")[0].trim().split(/\s+/);
    const x = readDimension(xs) ?? fallback;
    return { x, y: (ys ? readDimension(ys) : null) ?? x, sized };
  }

  // The place of the login box on a screen, as the greeter works it out: the
  // point, moved by the anchor, and kept on the screen.
  function absolute(d: Dimension, screen: number, box: number): number {
    let at = d.percent ? Math.trunc((screen * d.value) / 100) : d.value;
    if (d.negative) at = screen - at;
    if (d.anchor === "end") at -= box;
    else if (d.anchor === "center") at -= Math.trunc(box / 2);
    if (at < 0) return 0;
    if (at + box > screen) return screen - box;
    return at;
  }

  function formatPosition(): string {
    const x = `${Math.round(xOffset)}%,${xAnchor}`;
    const y = `${Math.round(yOffset)}%,${yAnchor}`;
    return x === y ? x : `${x} ${y}`;
  }

  // "Noto Sans Bold 11" is a family and a size.
  function parseFont(value: string | undefined) {
    const match = (value ?? "").trim().match(/^(.*?)\s+(\d+(?:\.\d+)?)$/);
    if (match) return { family: match[1], size: parseFloat(match[2]) };
    return { family: (value ?? "").trim() || "Sans", size: 10 };
  }

  onMount(async () => {
    await withProgress(async () => {
      const [settings, gtk, icons, cursors, families] = await Promise.all([
        lightdmRead(),
        listThemes("gtk"),
        listThemes("icons"),
        listThemes("cursors"),
        lightdmFonts(),
      ]);
      gtkThemes = gtk.map((t) => t.name);
      iconThemes = icons.map((t) => t.name);
      cursorThemes = cursors.map((t) => t.name);
      fonts = families;

      const v = settings.values;
      background = v["background"] ?? "";
      userImage = v["default-user-image"] ?? "";
      for (const file of [background, userImage]) if (file) await allowImage(file).catch(() => {});
      gtkTheme = v["theme-name"] ?? "";
      iconTheme = v["icon-theme-name"] ?? "";
      cursorTheme = v["cursor-theme-name"] ?? "";
      const font = parseFont(v["font-name"]);
      fontFamily = font.family;
      fontSize = font.size;
      originalPosition = v["position"] ?? "";
      const position = readPosition(originalPosition);
      shownX = position.x;
      shownY = position.y;
      representable = position.x.percent && position.y.percent && !position.x.negative && !position.y.negative && !position.sized;
      xOffset = Math.min(100, position.x.value);
      yOffset = Math.min(100, position.y.value);
      xAnchor = position.x.anchor;
      yAnchor = position.y.anchor;
      panelAtBottom = (v["panel-position"] ?? "top") === "bottom";
      showAvatar = (v["hide-user-image"] ?? "false") !== "true";
      backgroundSource = (v["user-background"] ?? "true") === "true" ? "desktop" : "fixed";
      desktopImage = await desktopWallpaper(false).catch(() => null);
      clockFormat = v["clock-format"] ?? "";
      avatar = await userAvatar().catch(() => null);
    });
    loaded = true;
  });

  // A theme that the file names but is not installed stays in the list, so the
  // greeter is not silently switched to another one.
  const withCurrent = (list: string[], current: string) => (current && !list.includes(current) ? [...list, current] : list);
  $: gtkChoices = withCurrent(gtkThemes, gtkTheme);
  $: iconChoices = withCurrent(iconThemes, iconTheme);
  $: cursorChoices = withCurrent(cursorThemes, cursorTheme);
  $: fontChoices = withCurrent(fonts, fontFamily);

  async function chooseBackground() {
    const file = await pickImage($_("login.pickBackground"), $_("login.images"));
    if (!file) return;
    await allowImage(file).catch(() => {});
    background = file;
  }

  // The picture of the user: a file of the home folder and of the account, not
  // a setting of the greeter, so it is applied at once and asks first.
  let avatar: string | null = null;
  let avatarStamp = 0;
  let pendingAvatar: string | null = null;

  async function chooseAvatar() {
    const file = await pickImage($_("login.pickAvatar"), $_("login.images"));
    if (file) pendingAvatar = file;
  }

  async function confirmAvatar() {
    const file = pendingAvatar;
    pendingAvatar = null;
    if (!file) return;
    try {
      avatar = await withProgress(() => setUserAvatar(file));
      avatarStamp = Date.now();
      status = $_("login.avatarSaved");
    } catch (e) {
      status = $_("login.avatarError", { values: { e: String(e) } });
    }
  }

  async function chooseUserImage() {
    const file = await pickImage($_("login.pickUserImage"), $_("login.images"));
    if (!file) return;
    await allowImage(file).catch(() => {});
    userImage = file;
  }

  // From here on the grid decides the position, whatever the file had.
  function takeOver() {
    representable = true;
    shownX = { value: xOffset, percent: true, negative: false, anchor: xAnchor };
    shownY = { value: yOffset, percent: true, negative: false, anchor: yAnchor };
  }

  function pickAnchor(x: Anchor, y: Anchor) {
    xAnchor = x;
    yAnchor = y;
    xOffset = DEFAULT_OFFSET[x];
    yOffset = DEFAULT_OFFSET[y];
    takeOver();
  }

  // The value comes with the event: the bound variable is not updated yet.
  function changeOffset(axis: "x" | "y", value: number) {
    if (axis === "x") xOffset = value;
    else yOffset = value;
    takeOver();
  }

  // The preview is a screen of 1920 x 1080 shown at a tenth of its size, with the
  // top bar of the greeter and a login box of the size it usually has. The place
  // of the box is worked out as the greeter does it, so the preview shows where
  // the box really goes, including being kept on the screen and below the bar.
  const SCREEN_W = 1920;
  const SCREEN_H = 1080;
  const BOX_W = 400;
  const BOX_H = 300;
  const PANEL_H = 28;
  $: SCALE = 0.085 + 0.02 * density;
  $: boxX = absolute(shownX, SCREEN_W, BOX_W);
  $: rawY = absolute(shownY, SCREEN_H, BOX_H);
  // The greeter does not put the box over its bar.
  $: boxY = panelAtBottom ? (SCREEN_H - rawY - BOX_H < PANEL_H ? SCREEN_H - BOX_H - PANEL_H : rawY) : rawY < PANEL_H ? PANEL_H : rawY;

  async function save() {
    saving = true;
    status = "";
    try {
      // The same wallpaper as the desktop: the account gets the one Xfce shows now.
      if (userBackground) desktopImage = await desktopWallpaper(true).catch(() => null);
      await withProgress(() =>
        lightdmWrite({
          background,
          "default-user-image": userImage,
          "theme-name": gtkTheme,
          "icon-theme-name": iconTheme,
          "cursor-theme-name": cursorTheme,
          "font-name": fontFamily ? `${fontFamily} ${fontSize}` : "",
          // A position the grid cannot show is left as the file has it.
          position: representable ? formatPosition() : originalPosition,
          "hide-user-image": showAvatar ? "false" : "true",
          "user-background": userBackground ? "true" : "false",
          "clock-format": clockFormat.trim(),
        }),
      );
      status = $_("login.saved");
    } catch {
      status = $_("login.saveError");
    }
    saving = false;
  }

  // A picture that cannot be read is hidden: the greeter would not show it either.
  function hideBroken(event: Event) {
    (event.currentTarget as HTMLElement).style.display = "none";
  }

  const field = "w-full bg-surface-2 border border-border rounded px-2.5 py-[var(--fpy)] text-[length:var(--fs)] text-text-bright outline-none focus-visible:ring-2 focus-visible:ring-primary/60";
  const label = "text-[length:var(--ls)] text-text-muted leading-snug";
  const formLabel = "text-[length:var(--ls)] text-text-muted text-right";
  const small = "px-2.5 py-[var(--bpy)] rounded bg-surface-2 hover:bg-border text-text-bright text-[length:var(--ls)]";
</script>

<div class="h-full" bind:clientHeight={available}>
<div class="flex flex-col" style="{sizes}; gap: var(--gap)">
  <div class="flex items-baseline justify-between gap-3">
    <h2 class="font-semibold text-text-bright" style="font-size: var(--ts)">{$_("login.title")}</h2>
    <span class="text-[0.8rem] text-text-muted truncate">{status}</span>
  </div>
  <p class="text-[0.78rem] text-text-muted -mt-2">{$_("login.hint")}</p>

  {#if loaded}
    <div class="bg-surface border border-border rounded flex gap-4 flex-wrap" style="padding: var(--pad)">
      <div class="flex flex-col gap-1.5 shrink-0" style="width: {backgroundWidth}px">
        <div class="bg-surface-2 rounded overflow-hidden grid place-items-center" style="width: {backgroundWidth}px; height: {Math.round(backgroundWidth * 0.5625)}px">
          {#if shownBackground}
            {#if shownBackground in copies}
              <img src={convertFileSrc(copies[shownBackground] ?? shownBackground)} alt="" class="w-full h-full object-cover" on:error={hideBroken} />
            {/if}
          {:else}
            <ImageOff size={26} class="text-text-muted" />
          {/if}
        </div>
        <button class={small} disabled={userBackground} title={userBackground ? $_("login.backgroundDesktopHint") : ""} on:click={chooseBackground}>{$_("login.changeBackground")}</button>
        <div class="flex items-center gap-2">
          <div class="bg-surface-2 rounded overflow-hidden grid place-items-center shrink-0" style="width: {avatarSize}px; height: {avatarSize}px">
            {#if userImage}
              {#if userImage in copies}
                <img src={convertFileSrc(copies[userImage] ?? userImage)} alt="" class="w-full h-full object-contain" on:error={hideBroken} />
              {/if}
            {:else}
              <User size={18} class="text-text-muted" />
            {/if}
          </div>
          <button class="{small} flex-1 min-w-0 truncate whitespace-nowrap" title={$_("login.defaultImageHint")} on:click={chooseUserImage}>{$_("login.changeUserImage")}</button>
        </div>
        <div class="flex items-center gap-2">
          <div class="bg-surface-2 rounded-full overflow-hidden grid place-items-center shrink-0" style="width: {avatarSize}px; height: {avatarSize}px">
            {#if avatar}
              <img src="{convertFileSrc(avatar)}?t={avatarStamp}" alt="" class="w-full h-full object-cover" on:error={hideBroken} />
            {:else}
              <User size={18} class="text-text-muted" />
            {/if}
          </div>
          <button class="{small} flex-1 min-w-0 truncate whitespace-nowrap" title={$_("login.avatarHint")} on:click={chooseAvatar}>{$_("login.changeAvatar")}</button>
        </div>
      </div>

      <div class="flex-1 min-w-[300px] grid grid-cols-[auto_1fr] gap-x-3 content-start items-center" style="row-gap: var(--row)">
        <span class={formLabel}>{$_("login.gtkTheme")}</span>
        <Select bind:value={gtkTheme} options={gtkChoices} label={$_("login.gtkTheme")} />
        <span class={formLabel}>{$_("login.iconTheme")}</span>
        <Select bind:value={iconTheme} options={iconChoices} label={$_("login.iconTheme")} />
        <span class={formLabel}>{$_("login.cursorTheme")}</span>
        <Select bind:value={cursorTheme} options={cursorChoices} label={$_("login.cursorTheme")} />
        <span class={formLabel}>{$_("login.userBackground")}</span>
        <Select bind:value={backgroundSource} options={sourceOptions} label={$_("login.userBackground")} />
        <span class={formLabel}>{$_("login.font")}</span>
        <div class="flex gap-2">
          <Select bind:value={fontFamily} options={fontChoices} label={$_("login.font")} />
          <div class="shrink-0" title={$_("login.fontSize")}><NumberField bind:value={fontSize} min={6} max={40} label={$_("login.fontSize")} /></div>
        </div>
      </div>
    </div>

    <div class="bg-surface border border-border rounded flex flex-col" style="padding: var(--pad); gap: var(--row)">
      <h3 class="text-[0.88rem] font-semibold text-text-bright">{$_("login.position")}</h3>
      <div class="flex gap-6 items-start">
        <!-- The screen and the nine cells are one piece: the cell that is clicked is
             where the login box goes, and the box shows where that ends up. -->
        <div class="relative shrink-0 border border-border rounded bg-surface-2 overflow-hidden" style="width: {SCREEN_W * SCALE}px; height: {SCREEN_H * SCALE}px" title={$_("login.previewTitle")}>
          <div class="absolute left-0 right-0 bg-border" style="height: {PANEL_H * SCALE}px; {panelAtBottom ? 'bottom' : 'top'}: 0"></div>
          <div class="absolute grid grid-cols-3 grid-rows-3 inset-0">
            {#each ANCHORS as y}
              {#each ANCHORS as x}
                <button
                  class="border border-dashed transition-colors {representable && xAnchor === x && yAnchor === y ? 'border-primary bg-primary/15' : 'border-border/60 hover:bg-primary/10'}"
                  aria-pressed={representable && xAnchor === x && yAnchor === y}
                  title={$_("login.anchorTitle", { values: { v: $_(`login.vertical.${y}`), h: $_(`login.horizontal.${x}`) } })}
                  aria-label={$_("login.anchorTitle", { values: { v: $_(`login.vertical.${y}`), h: $_(`login.horizontal.${x}`) } })}
                  on:click={() => pickAnchor(x, y)}
                ></button>
              {/each}
            {/each}
          </div>
          <div class="absolute rounded-[2px] pointer-events-none shadow" style="background: #ff8800; width: {BOX_W * SCALE}px; height: {BOX_H * SCALE}px; left: {boxX * SCALE}px; top: {boxY * SCALE}px"></div>
        </div>

        <div class="flex-1 min-w-[300px] grid grid-cols-2 gap-x-5 items-end" style="row-gap: var(--row)">
          {#if !representable}
            <p class="col-span-2 text-[0.78rem] text-text-muted leading-snug">{$_("login.keptAsIs", { values: { position: originalPosition } })}</p>
          {/if}

          <div class="flex flex-col gap-1">
            <span class={label}>{$_("login.horizontalOffset")}</span>
            <NumberField bind:value={xOffset} min={0} max={100} disabled={!representable} label={$_("login.horizontalOffset")} on:change={(e) => changeOffset("x", e.detail)} />
          </div>
          <div class="flex flex-col gap-1">
            <span class={label}>{$_("login.verticalOffset")}</span>
            <NumberField bind:value={yOffset} min={0} max={100} disabled={!representable} label={$_("login.verticalOffset")} on:change={(e) => changeOffset("y", e.detail)} />
          </div>

          <div class="flex items-center justify-between gap-3">
            <span class={label}>{$_("login.showAvatar")}</span>
            <Switch bind:checked={showAvatar} label={$_("login.showAvatar")} />
          </div>

          <div class="col-span-2 flex flex-col gap-1">
            <span class={label}>{$_("login.clockFormat")}</span>
            <input type="text" class="{field} !w-[200px]" placeholder="%a, %H:%M" bind:value={clockFormat} />
          </div>
        </div>
      </div>
    </div>

    <div class="flex justify-end">
      <button
        class="flex items-center gap-2 px-5 py-[var(--bpy)] rounded bg-primary hover:bg-primary-hover text-text-bright text-[length:var(--fs)] font-semibold disabled:opacity-60"
        disabled={saving}
        on:click={save}
      >
        <Save size={15} />{$_("login.save")}
      </button>
    </div>
  {/if}
</div>
</div>

{#if pendingAvatar}
  <Modal title={$_("login.avatarConfirmTitle")} onClose={() => (pendingAvatar = null)}>
    <p>{$_("login.avatarConfirmText")}</p>
    <svelte:fragment slot="actions">
      <button class="px-5 py-2 rounded bg-surface-2 hover:bg-border text-text-bright text-[0.82rem]" on:click={() => (pendingAvatar = null)}>{$_("common.cancel")}</button>
      <button data-autofocus class="px-6 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={confirmAvatar}>{$_("common.ok")}</button>
    </svelte:fragment>
  </Modal>
{/if}
