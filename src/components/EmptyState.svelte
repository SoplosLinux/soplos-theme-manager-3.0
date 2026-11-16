<script lang="ts">
  import type { ComponentType } from "svelte";

  export let icon: ComponentType;
  export let text: string;
</script>

<div class="h-full min-h-[18rem] flex flex-col items-center justify-center gap-4 text-center">
  <div class="w-16 h-16 rounded-2xl bg-surface-2 grid place-items-center text-primary">
    <svelte:component this={icon} size={30} />
  </div>
  <p class="text-[0.9rem] text-text-muted max-w-[28rem]">{text}</p>
</div>
