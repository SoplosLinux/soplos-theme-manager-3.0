<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { Minus, Square, Copy, X } from "lucide-svelte";
  import { _ } from "svelte-i18n";

  export let maximized = false;

  const win = getCurrentWindow();
  const base = "w-7 h-7 grid place-items-center rounded text-text-muted transition-colors";
</script>

<div class="flex items-center gap-0.5">
  <button class="{base} hover:bg-surface hover:text-text-bright" title={$_("app.window.minimize")} aria-label={$_("app.window.minimize")} on:click={() => win.minimize()}>
    <Minus size={14} />
  </button>
  <button
    class="{base} hover:bg-surface hover:text-text-bright"
    title={maximized ? $_("app.window.restore") : $_("app.window.maximize")}
    aria-label={maximized ? $_("app.window.restore") : $_("app.window.maximize")}
    on:click={() => win.toggleMaximize()}
  >
    <svelte:component this={maximized ? Copy : Square} size={12} />
  </button>
  <button class="{base} hover:bg-danger hover:text-white" title={$_("app.window.close")} aria-label={$_("app.window.close")} on:click={() => win.close()}>
    <X size={15} />
  </button>
</div>
