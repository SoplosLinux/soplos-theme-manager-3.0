import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

/** The kinds of theme the pages manage. */
export type ThemeKind = "gtk" | "icons" | "cursors";

export interface ThemeEntry {
  name: string;
  displayName: string;
  system: boolean;
  hasXfwm4: boolean;
  /** A small picture as a data address, when there is one. */
  preview: string | null;
  /** Window, contents, accent and text colors of a GTK theme; empty when they cannot be read. */
  swatches: string[];
}

export interface Candidate {
  rel: string;
  name: string;
  displayName: string;
  kind: "gtk" | "icon" | "cursor";
  hasXfwm4: boolean;
}

export interface Inspected {
  token: string;
  candidates: Candidate[];
  /** With no theme of the kind asked for: what the package holds instead. */
  otherKinds: ("gtk" | "icons" | "cursors" | "windows")[];
}

export type Scope = "user" | "global";

export const listThemes = (kind: ThemeKind) => invoke<ThemeEntry[]>("list_themes", { kind });
/** The pictures that say what an icon theme (six icons) or a cursor theme (four cursors) looks like. */
export const themeSamples = (kind: ThemeKind, name: string) => invoke<string[]>("theme_samples", { kind, name });
export const activeTheme = (kind: ThemeKind) => invoke<string | null>("get_active_theme", { kind });
export const setActiveTheme = (kind: ThemeKind, name: string) => invoke<void>("set_active_theme", { kind, name });
export const themePackage = (kind: ThemeKind, name: string) => invoke<string | null>("theme_package", { kind, name });
export const deleteTheme = (kind: ThemeKind, name: string) => invoke<void>("delete_theme", { kind, name });
export const inspectPackage = (path: string, kind: ThemeKind) => invoke<Inspected>("inspect_package", { path, kind });
export const discardPackage = (token: string) => invoke<void>("discard_package", { token });
export const installCandidates = (token: string, rels: string[], kind: ThemeKind, scope: Scope) =>
  invoke<string[]>("install_candidates", { token, rels, kind, scope });

/** Asks for a package with the file chooser of the desktop. Null if cancelled. */
export async function pickPackage(filterName: string): Promise<string | null> {
  const chosen = await open({
    multiple: false,
    directory: false,
    filters: [{ name: filterName, extensions: ["tar.gz", "tgz", "tar.xz", "tar.bz2", "tar", "zip"] }],
  });
  return typeof chosen === "string" ? chosen : null;
}

export interface Wallpaper {
  path: string;
  name: string;
}

export const defaultWallpaperFolder = () => invoke<string | null>("default_wallpaper_folder");
export const scanWallpapers = (folder: string) => invoke<Wallpaper[]>("scan_wallpapers", { folder });
export const setWallpaper = (path: string) => invoke<void>("set_wallpaper", { path });
/** Installs pictures for the user alone or for everyone. Returns the folder they went to. */
export const installWallpapers = (paths: string[], everyone: boolean, sub?: string) =>
  invoke<string>("install_wallpapers", { paths, everyone, sub: sub || null });
/** The small copy of a wallpaper, made once and kept in the cache. Null if it cannot be made. */
export const makeThumbnail = (path: string) => invoke<string | null>("make_thumbnail", { path });

/** Asks for a folder with the file chooser of the desktop. Null if cancelled. */
export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  const chosen = await open({ multiple: false, directory: true, title, defaultPath });
  return typeof chosen === "string" ? chosen : null;
}

/** Asks for one or several pictures. An empty list if cancelled. */
export async function pickImages(title: string, filterName: string): Promise<string[]> {
  const chosen = await open({
    multiple: true,
    directory: false,
    title,
    filters: [{ name: filterName, extensions: ["jpg", "jpeg", "png", "webp", "bmp", "svg"] }],
  });
  return Array.isArray(chosen) ? chosen : typeof chosen === "string" ? [chosen] : [];
}

export interface DockApp {
  /** What the settings file of the dock holds for it. */
  desktopPath: string;
  name: string;
  icon: string | null;
}

export const dockActive = () => invoke<boolean>("dock_active");
export interface DockInfo {
  id: number;
  panel: number | null;
}
export const dockList = () => invoke<DockInfo[]>("dock_list");
export const dockPinned = (id?: number) => invoke<DockApp[]>("dock_pinned", { id });
export const dockAvailable = () => invoke<DockApp[]>("dock_available");
export const dockSave = (id: number | undefined, paths: string[]) => invoke<void>("dock_save", { id, paths });

export interface LoginSettings {
  /** The keys of the [greeter] section as the file has them. */
  values: Record<string, string>;
}

export const lightdmRead = () => invoke<LoginSettings>("lightdm_read");
export const lightdmFonts = () => invoke<string[]>("lightdm_fonts");
export const lightdmWrite = (updates: Record<string, string>) => invoke<void>("lightdm_write", { updates });
export const allowImage = (path: string) => invoke<void>("allow_image", { path });
export const userAvatar = () => invoke<string | null>("user_avatar");
export const setUserAvatar = (path: string) => invoke<string>("set_user_avatar", { path });
export const desktopWallpaper = (publish: boolean) => invoke<string | null>("desktop_wallpaper", { publish });

/** Asks for one picture with the file chooser of the desktop. Null if cancelled. */
export async function pickImage(title: string, filterName: string): Promise<string | null> {
  const chosen = await open({
    multiple: false,
    directory: false,
    title,
    filters: [{ name: filterName, extensions: ["jpg", "jpeg", "png", "webp", "bmp", "svg"] }],
  });
  return typeof chosen === "string" ? chosen : null;
}

export interface PanelSettings {
  edge: "top" | "bottom" | "left" | "right";
  /** 0 start, 1 center, 2 end. */
  align: number;
  /** 0 horizontal, 1 vertical, 2 deskbar. */
  mode: number;
  size: number;
  iconSize: number;
  length: number;
  lengthAdjust: boolean;
  rows: number;
  /** 0 never, 1 intelligent, 2 always. */
  autohide: number;
  dark: boolean;
  locked: boolean;
  position: string;
}

export interface PanelPlugin {
  id: number;
  module: string;
  name: string;
  comment: string;
  icon: string | null;
}

export interface PanelState {
  settings: PanelSettings;
  active: PanelPlugin[];
}

export interface PanelChange extends Omit<PanelSettings, "position"> {
  oldPosition: string;
  positionChanged: boolean;
  pluginIds: number[];
}

export const panelList = () => invoke<number[]>("panel_list");
export const panelRead = (id: number) => invoke<PanelState>("panel_read", { id });
export const panelAvailable = () => invoke<PanelPlugin[]>("panel_available");
export const panelApply = (id: number, change: PanelChange) => invoke<void>("panel_apply", { id, change });
/** Applies the changes of several panels, restarting the panel once. */
export const panelApplyMany = (changes: { id: number; change: PanelChange }[]) => invoke<void>("panel_apply_many", { changes });
export const panelSetOrder = (id: number, ids: number[]) => invoke<void>("panel_set_order", { id, ids });
export const panelRemovePlugin = (id: number, pluginId: number) => invoke<void>("panel_remove_plugin", { id, pluginId });
export const panelAddPlugin = (id: number, module: string) => invoke<number>("panel_add_plugin", { id, module });
export const panelPreferences = (id: number) => invoke<void>("panel_preferences", { id });
export const panelNew = () => invoke<{ id: number; ids: number[] }>("panel_new");
export const panelDelete = (id: number) => invoke<number[]>("panel_delete", { id });

export type ThemePart = "gtk" | "icons" | "cursor" | "wallpaper" | "panel";

export interface SavedTheme {
  name: string;
  author: string;
  created: string;
  preview: string | null;
  /** What the theme carries. */
  parts: ThemePart[];
}

export const listSavedThemes = () => invoke<SavedTheme[]>("list_saved_themes");
export const removeSavedTheme = (name: string) => invoke<void>("remove_saved_theme", { name });
export const saveCurrentTheme = (name: string, screenshot: boolean) => invoke<void>("save_current_theme", { name, screenshot });
/** Returns the parts that could not be set. */
export const applySavedTheme = (name: string) => invoke<string[]>("apply_saved_theme", { name });
export const screenshotAvailable = () => invoke<boolean>("screenshot_available");
export const exportSavedTheme = (name: string, dest: string) => invoke<void>("export_saved_theme", { name, dest });
export const bundleInfo = (path: string) => invoke<string>("bundle_info", { path });
export const installBundle = (path: string, scope: Scope) => invoke<string>("install_bundle", { path, scope });
export const restoreBaseThemes = () => invoke<number>("restore_base_themes");
export const seedFirstRun = () => invoke<void>("seed_first_run");
export const takePendingBundle = () => invoke<string | null>("take_pending_bundle");
export const renameSavedTheme = (name: string, newName: string) => invoke<void>("rename_saved_theme", { name, newName });
