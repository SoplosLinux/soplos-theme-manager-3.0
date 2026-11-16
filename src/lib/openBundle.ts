import { writable } from "svelte/store";

// A .sth file that the file manager asked to open and the Themes page has not
// taken yet. The page clears it when it starts the installation.
export const pendingBundle = writable<string | null>(null);
