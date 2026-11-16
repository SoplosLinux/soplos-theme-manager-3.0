<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { ChevronDown } from "lucide-svelte";

  type Option = string | { value: string; label: string };

  /** The chosen value; it is one of the `options`. */
  export let value: string;
  /** Plain texts, or a value with the text that is shown for it. */
  export let options: Option[];
  export let label = "";

  // Whoever uses it can listen to the choice instead of binding the value.
  const dispatch = createEventDispatcher<{ change: string }>();
  function changed(event: Event) {
    dispatch("change", (event.currentTarget as HTMLSelectElement).value);
  }
</script>

<!-- The native select drawn by the toolkit does not follow the colors of the
     program, so it is stripped and given the look of the other fields. -->
<div class="relative w-full min-w-0">
  <select
    bind:value
    on:change={changed}
    aria-label={label}
    class="w-full appearance-none bg-surface-2 border border-border rounded pl-2.5 pr-8 py-[var(--fpy,6px)] text-[length:var(--fs,0.82rem)] text-text-bright outline-none cursor-pointer focus-visible:ring-2 focus-visible:ring-primary/60"
  >
    {#each options as option}
      {#if typeof option === "string"}
        <option value={option}>{option}</option>
      {:else}
        <option value={option.value}>{option.label}</option>
      {/if}
    {/each}
  </select>
  <ChevronDown size={14} class="absolute right-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
</div>
