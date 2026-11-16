import { writable } from "svelte/store";

// How many jobs are running right now. The bar at the bottom of the window is
// shown while there is at least one.
const running = writable(0);

export const busy = { subscribe: running.subscribe };

/** Runs a job and keeps the progress bar of the window on while it lasts. */
export async function withProgress<T>(job: () => Promise<T>): Promise<T> {
  running.update((n) => n + 1);
  try {
    return await job();
  } finally {
    running.update((n) => n - 1);
  }
}
