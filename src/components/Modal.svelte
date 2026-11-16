<script lang="ts">
  import { onMount } from "svelte";
  import { fade, scale } from "svelte/transition";

  /** Called when the dialog asks to close: Escape, or a click outside of it. */
  export let onClose: () => void;
  export let title: string;

  let box: HTMLDivElement;

  // The first button of the dialog gets the focus, so Enter confirms it.
  onMount(() => {
    box?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
  });
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
  role="button"
  tabindex="-1"
  transition:fade|global={{ duration: 120 }}
  on:click={onClose}
  on:keydown={(e) => {
    if (e.key === "Escape") onClose();
  }}
>
  <!-- A click inside the dialog must not reach the backdrop, which closes it. -->
  <div
    class="w-full max-w-[440px] max-h-full"
    role="presentation"
    transition:scale|global={{ duration: 140, start: 0.96 }}
    on:click|stopPropagation
    on:keydown|stopPropagation={(e) => {
      if (e.key === "Escape") onClose();
    }}
  >
    <div
      bind:this={box}
      class="max-h-full overflow-y-auto bg-surface border border-border rounded-xl p-6 flex flex-col gap-4 shadow-2xl"
      role="dialog"
      aria-modal="true"
      aria-label={title}
    >
      <h2 class="text-text-bright font-semibold text-[1.05rem]">{title}</h2>
      <div class="text-[0.85rem] text-text flex flex-col gap-3">
        <slot />
      </div>
      <div class="flex flex-wrap justify-end gap-2 mt-1">
        <slot name="actions" />
      </div>
    </div>
  </div>
</div>
