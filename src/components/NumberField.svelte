<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";
  import { Minus, Plus } from "lucide-svelte";

  /** A whole number between `min` and `max`, changed with two big buttons, the
      arrow keys or by typing. The arrows the toolkit draws in a number field are
      too small to hit and do not follow the colors of the program. */
  export let value: number;
  export let min = 0;
  export let max = 100;
  export let step = 1;
  export let label = "";
  export let disabled = false;

  const dispatch = createEventDispatcher<{ change: number }>();
  const REPEAT_AFTER = 400; // ms before holding a button repeats
  const REPEAT_EVERY = 70;

  let timer: ReturnType<typeof setTimeout> | undefined;
  let repeating: ReturnType<typeof setInterval> | undefined;

  const clamp = (n: number) => Math.max(min, Math.min(max, Math.round(Number.isFinite(n) ? n : min)));

  function set(next: number) {
    const clamped = clamp(next);
    if (clamped !== value) {
      value = clamped;
      dispatch("change", value);
    }
  }

  function press(direction: number) {
    if (disabled) return;
    set(value + direction * step);
    stop();
    // Holding the button goes on changing the value.
    timer = setTimeout(() => {
      repeating = setInterval(() => set(value + direction * step), REPEAT_EVERY);
    }, REPEAT_AFTER);
  }

  function stop() {
    clearTimeout(timer);
    clearInterval(repeating);
    timer = repeating = undefined;
  }

  onDestroy(stop);

  function keydown(event: KeyboardEvent) {
    if (event.key === "ArrowUp") {
      event.preventDefault();
      set(value + step);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      set(value - step);
    }
  }

  const button =
    "w-[var(--nb,28px)] h-[var(--nb,28px)] grid place-items-center text-text-muted hover:text-text-bright hover:bg-border transition-colors disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-text-muted touch-none";
</script>

<div class="inline-flex w-fit items-stretch bg-surface-2 border border-border rounded overflow-hidden {disabled ? 'opacity-60' : ''}">
  <button class="{button} border-r border-border" {disabled} tabindex="-1" aria-label="−" on:pointerdown={() => press(-1)} on:pointerup={stop} on:pointerleave={stop} on:pointercancel={stop}>
    <Minus size={13} />
  </button>
  <input
    type="text"
    inputmode="numeric"
    {disabled}
    aria-label={label}
    class="w-11 text-center bg-transparent text-[length:var(--fs,0.82rem)] text-text-bright outline-none focus-visible:bg-surface"
    value={value}
    on:change={(e) => {
      const typed = parseInt(e.currentTarget.value, 10);
      // What is not a number leaves the value as it was.
      if (!Number.isNaN(typed)) set(typed);
      // Whatever was typed shows the value that was kept.
      e.currentTarget.value = String(value);
    }}
    on:keydown={keydown}
  />
  <button class="{button} border-l border-border" {disabled} tabindex="-1" aria-label="+" on:pointerdown={() => press(1)} on:pointerup={stop} on:pointerleave={stop} on:pointercancel={stop}>
    <Plus size={13} />
  </button>
</div>
