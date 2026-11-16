<script lang="ts">
  /** An on and off switch. */
  export let checked = false;
  export let label = "";
  export let disabled = false;
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  class="relative w-[34px] h-[19px] rounded-full transition-colors shrink-0 disabled:opacity-50 {checked ? 'bg-primary' : 'bg-surface-2 border border-border'}"
  on:click={() => (checked = !checked)}
>
  <span class="absolute top-[2px] left-[2px] w-[15px] h-[15px] rounded-full bg-white transition-transform {checked ? 'translate-x-[15px]' : ''}"></span>
</button>
