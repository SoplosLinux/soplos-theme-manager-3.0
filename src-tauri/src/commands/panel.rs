// The Xfce panel: its panels, the settings of each one and its plugins. All of
// it lives in the xfce4-panel channel of xfconf, under /panels (the list of
// panels and, for each, `panel-N/...`) and /plugins (which module each plugin
// number is). The panel reads it again when it starts, so a change is made by
// writing the settings and restarting the panel.

use super::dock::language_keys;
use super::icons::IconFinder;
use super::previews::file_data_url;
use super::themes::home;
use super::xfconf;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CHANNEL: &str = "xfce4-panel";
const PLUGINS_DIR: &str = "/usr/share/xfce4/panel/plugins";
const ICON_SIZE: i64 = 24;
const MAX_ICON_BYTES: u64 = 512 * 1024;

// Only one change of the panel at a time: each one restarts it.
static PANEL_LOCK: Mutex<()> = Mutex::new(());

// ── xfconf ───────────────────────────────────────────────────────────────────

fn query(args: &[&str]) -> Option<std::process::Output> {
    Command::new("xfconf-query").args(["-c", CHANNEL]).args(args).output().ok()
}

fn get(prop: &str) -> Option<String> {
    let out = query(&["-p", prop])?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// The whole numbers of a property that is a list; a single value is a list of one.
fn get_array(prop: &str) -> Vec<u32> {
    let Some(out) = query(&["-p", prop]) else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.trim().parse::<u32>().ok()).collect()
}

fn set(prop: &str, kind: &str, value: &str) -> bool {
    query(&["-p", prop, "--create", "-t", kind, "-s", value]).is_some_and(|o| o.status.success())
}

// A list of whole numbers. A list of one is written as a list and not as a plain
// value: the panel cannot read /panels otherwise. An empty list removes the
// property, which is how an empty list is kept in xfconf.
fn set_array(prop: &str, values: &[u32]) -> bool {
    if values.is_empty() {
        remove(prop);
        return true;
    }
    let mut args: Vec<String> = vec!["-p".into(), prop.into(), "--create".into(), "--force-array".into()];
    for v in values {
        args.extend(["-t".into(), "int".into(), "-s".into(), v.to_string()]);
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    query(&refs).is_some_and(|o| o.status.success())
}

fn remove(prop: &str) {
    let _ = query(&["-p", prop, "-r", "--recursive"]);
}

/// The panels there are; one with the number 1 when xfconf has none.
fn panel_ids() -> Vec<u32> {
    let ids = get_array("/panels");
    if ids.is_empty() {
        vec![1]
    } else {
        ids
    }
}

fn check_panel(id: u32) -> Result<(), String> {
    if panel_ids().contains(&id) {
        Ok(())
    } else {
        Err("There is no such panel".to_string())
    }
}

// ── The screen ───────────────────────────────────────────────────────────────

// The size of the screen in pixels, with every monitor in it, as the panel takes
// it for its coordinates.
fn screen_size() -> (i64, i64) {
    let text = Command::new("xrandr").arg("--current").output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let found = text.lines().next().and_then(|l| l.split("current ").nth(1)).and_then(|rest| {
        let mut parts = rest.split(',').next()?.split('x');
        Some((parts.next()?.trim().parse::<i64>().ok()?, parts.next()?.trim().parse::<i64>().ok()?))
    });
    found.unwrap_or((1920, 1080))
}

// ── The position string: "p=8;x=960;y=1066" ─────────────────────────────────

// Where each value of `p` puts the panel. These are the snap positions of the
// panel window: 1 E, 2 NE, 3 EC, 4 SE, 5 W, 6 NW, 7 WC, 8 SW, 9 NC, 10 SC,
// 11 N and 12 S. A corner (NE, SE, NW, SW) is on the top or bottom edge for a
// horizontal panel and on the right or left one for a vertical panel.
fn edge_of_p(p: i64, horizontal: bool) -> &'static str {
    match p {
        1 | 3 => "right",
        5 | 7 => "left",
        9 | 11 => "top",
        10 | 12 => "bottom",
        2 => {
            if horizontal {
                "top"
            } else {
                "right"
            }
        }
        4 => {
            if horizontal {
                "bottom"
            } else {
                "right"
            }
        }
        6 => {
            if horizontal {
                "top"
            } else {
                "left"
            }
        }
        _ => {
            if horizontal {
                "bottom"
            } else {
                "left"
            }
        }
    }
}

fn parse_position(text: &str) -> (i64, std::collections::HashMap<String, i64>) {
    let mut p = 8;
    let mut coords = std::collections::HashMap::new();
    for part in text.split(';') {
        if let Some((key, value)) = part.split_once('=') {
            if let Ok(n) = value.trim().parse::<i64>() {
                if key == "p" {
                    p = n;
                } else {
                    coords.insert(key.to_string(), n);
                }
            }
        }
    }
    (p, coords)
}

/// The edge a panel is on. A floating panel (p=0) has no edge of its own: a
/// horizontal one is on the top or the bottom, whichever half of the screen its
/// center is in; a vertical one on the left or the right the same way; and a
/// deskbar on the edge that its center is nearest to.
fn edge_of(text: &str, screen: (i64, i64), mode: u32) -> &'static str {
    let (p, coords) = parse_position(text);
    if p != 0 {
        return edge_of_p(p, mode == 0);
    }
    let (sw, sh) = screen;
    let cx = coords.get("x").copied().unwrap_or(sw / 2);
    let cy = coords.get("y").copied().unwrap_or(sh / 2);
    match mode {
        0 => {
            if cy < sh / 2 {
                "top"
            } else {
                "bottom"
            }
        }
        1 => {
            if cx < sw / 2 {
                "left"
            } else {
                "right"
            }
        }
        _ => {
            let distances = [("top", cy), ("bottom", sh - cy), ("left", cx), ("right", sw - cx)];
            distances.iter().min_by_key(|(_, d)| *d).map(|(e, _)| *e).unwrap_or("bottom")
        }
    }
}

/// 0 start, 1 center or 2 end: where the panel sits along its edge. A panel as
/// wide as the screen has no room to be anywhere else but the center; a shorter
/// one is at the start or the end when it touches that end of the screen, and
/// in the center otherwise.
fn align_of(text: &str, edge: &str, screen: (i64, i64), length: u32) -> u8 {
    if length >= 100 {
        return 1;
    }
    let (_, coords) = parse_position(text);
    let (sw, sh) = screen;
    let (middle, whole) = if edge == "top" || edge == "bottom" { (coords.get("x").copied().unwrap_or(sw / 2), sw) } else { (coords.get("y").copied().unwrap_or(sh / 2), sh) };
    let half = whole * length as i64 / 100 / 2;
    // A hundredth of the screen of play, for the rounding of the coordinates.
    let tolerance = whole / 100;
    // With almost no room left there is no telling the places apart.
    if whole - 2 * half <= 2 * tolerance {
        return 1;
    }
    if middle - half <= tolerance {
        0
    } else if middle + half >= whole - tolerance {
        2
    } else {
        1
    }
}

/// The position string for a panel. One as wide as the screen is snapped to its
/// edge (p=6, 8, 10 or 1) so the window manager keeps room for it; a shorter one
/// is floating (p=0) with the coordinates of its center, which is the only way
/// the panel respects a place that is not the whole edge.
fn build_position(edge: &str, align: u8, length: u32, size: u32, screen: (i64, i64)) -> String {
    if length >= 100 {
        // 6 and 8 are the top and bottom bars Xfce itself uses; 5 and 1 are the
        // left and right edges, which do not depend on the mode of the panel.
        let p = match edge {
            "top" => 6,
            "left" => 5,
            "right" => 1,
            _ => 8,
        };
        return format!("p={p};x=0;y=0");
    }
    let (sw, sh) = screen;
    let size = size as i64;
    let (cx, cy) = if edge == "top" || edge == "bottom" {
        let width = std::cmp::max(1, sw * length as i64 / 100);
        let cy = if edge == "top" { size / 2 } else { sh - size / 2 };
        let cx = match align {
            0 => width / 2,
            2 => sw - width / 2,
            _ => sw / 2,
        };
        (cx, cy)
    } else {
        let height = std::cmp::max(1, sh * length as i64 / 100);
        let cx = if edge == "left" { size / 2 } else { sw - size / 2 };
        let cy = match align {
            0 => height / 2,
            2 => sh - height / 2,
            _ => sh / 2,
        };
        (cx, cy)
    };
    format!("p=0;x={cx};y={cy}")
}

// A number as xfconf prints it: with a comma for the decimals in some languages.
fn parse_number(value: Option<String>, default: i64) -> i64 {
    value.and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).map(|f| f.round() as i64).unwrap_or(default)
}

fn parse_bool(value: Option<String>) -> bool {
    value.as_deref() == Some("true")
}

// ── Reading ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PanelSettings {
    /// "top", "bottom", "left" or "right".
    pub edge: String,
    /// 0 start, 1 center, 2 end.
    pub align: u8,
    /// 0 horizontal, 1 vertical, 2 deskbar.
    pub mode: u32,
    pub size: u32,
    #[serde(rename = "iconSize")]
    pub icon_size: u32,
    /// Percent of the screen.
    pub length: u32,
    #[serde(rename = "lengthAdjust")]
    pub length_adjust: bool,
    pub rows: u32,
    /// 0 never, 1 intelligent, 2 always.
    pub autohide: u32,
    pub dark: bool,
    pub locked: bool,
    /// The position string as it is in xfconf; sent back to leave it alone.
    pub position: String,
}

fn read_settings(id: u32) -> PanelSettings {
    let key = |name: &str| format!("/panels/panel-{id}/{name}");
    let position = get(&key("position")).unwrap_or_default();
    let screen = screen_size();
    let mode = parse_number(get(&key("mode")), 0).clamp(0, 2) as u32;
    let edge = edge_of(&position, screen, mode);
    PanelSettings {
        edge: edge.to_string(),
        align: align_of(&position, edge, screen, parse_number(get(&key("length")), 100).clamp(1, 100) as u32),
        mode,
        size: parse_number(get(&key("size")), 42).max(1) as u32,
        icon_size: parse_number(get(&key("icon-size")), 24).max(1) as u32,
        length: parse_number(get(&key("length")), 100).clamp(1, 100) as u32,
        length_adjust: parse_bool(get(&key("length-adjust"))),
        rows: parse_number(get(&key("nrows")), 1).clamp(1, 6) as u32,
        autohide: parse_number(get(&key("autohide-behavior")), 0).clamp(0, 2) as u32,
        dark: parse_bool(get("/panels/dark-mode")),
        locked: parse_bool(get(&key("position-locked"))),
        position,
    }
}

// The .desktop file of a plugin, as plain `key = value` lines.
fn read_plugin_file(path: &Path) -> std::collections::HashMap<String, String> {
    let mut keys = std::collections::HashMap::new();
    if let Ok(text) = fs::read_to_string(path) {
        for line in text.lines() {
            if line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                keys.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    keys
}

fn plugin_name(keys: &std::collections::HashMap<String, String>, languages: &[String], fallback: &str) -> String {
    for lang in languages {
        if let Some(name) = keys.get(&format!("Name[{lang}]")).filter(|n| !n.is_empty()) {
            return name.clone();
        }
    }
    keys.get("Name").filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| fallback.to_string())
}

#[derive(Serialize)]
pub struct Plugin {
    /// The number of the plugin in the panel; 0 for one that is only available.
    pub id: u32,
    /// The name of its .desktop file without the extension: what xfconf stores.
    pub module: String,
    pub name: String,
    pub comment: String,
    pub icon: Option<String>,
}

struct PluginCatalog {
    languages: Vec<String>,
    finder: IconFinder,
    theme: String,
}

impl PluginCatalog {
    fn new() -> Self {
        PluginCatalog {
            languages: language_keys(),
            finder: IconFinder::new(),
            theme: xfconf::get("xsettings", "/Net/IconThemeName").unwrap_or_else(|| "hicolor".to_string()),
        }
    }

    fn icon(&mut self, name: &str) -> Option<String> {
        let file = if Path::new(name).is_absolute() { Path::new(name).to_path_buf() } else { self.finder.find(&self.theme, name, ICON_SIZE)? };
        file_data_url(&file, MAX_ICON_BYTES)
    }

    // The plugin that a module names, if it is installed.
    fn plugin(&mut self, id: u32, module: &str) -> Plugin {
        let keys = read_plugin_file(&Path::new(PLUGINS_DIR).join(format!("{module}.desktop")));
        let known = keys.contains_key("X-XFCE-Module");
        let name = if known { plugin_name(&keys, &self.languages, module) } else { module.to_string() };
        let icon_name = keys.get("Icon").cloned().unwrap_or_else(|| "application-x-executable".to_string());
        Plugin { id, module: module.to_string(), name, comment: keys.get("Comment").cloned().unwrap_or_default(), icon: self.icon(&icon_name) }
    }
}

/// Every plugin that can be added: the .desktop files of the plugins folder that
/// name a module, by name.
fn available_plugins() -> Vec<Plugin> {
    let Ok(read) = fs::read_dir(PLUGINS_DIR) else { return Vec::new() };
    let mut files: Vec<_> = read.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "desktop")).collect();
    files.sort();
    let mut catalog = PluginCatalog::new();
    let mut plugins: Vec<Plugin> = files
        .iter()
        .filter_map(|file| {
            let module = file.file_stem()?.to_string_lossy().to_string();
            let keys = read_plugin_file(file);
            keys.get("X-XFCE-Module").filter(|m| !m.is_empty())?;
            Some(catalog.plugin(0, &module))
        })
        .collect();
    plugins.sort_by_key(|p| p.name.to_lowercase());
    plugins
}

// ── Restarting the panel ─────────────────────────────────────────────────────

fn process_running() -> bool {
    Command::new("pgrep").args(["-x", "xfce4-panel"]).output().is_ok_and(|o| o.status.success())
}

// Whether the panel answers on the session bus, which is what
// `xfce4-panel --quit` talks to. A panel that is still starting or already
// shutting down has a process but no name there.
fn panel_answers() -> bool {
    let out = Command::new("gdbus")
        .args(["call", "--session", "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus", "--method", "org.freedesktop.DBus.NameHasOwner", "org.xfce.Panel"])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).contains("true"),
        // Without gdbus the process is all there is to go by.
        _ => process_running(),
    }
}

/// Asks the panel to quit and waits until it is gone. Does nothing when the
/// panel does not answer: `xfce4-panel --quit` then opens an error window of
/// its own.
pub fn quit_panel() {
    if panel_answers() {
        let _ = Command::new("xfce4-panel").arg("--quit").output();
    }
    let deadline = Instant::now() + Duration::from_secs(6);
    while process_running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(300));
}

/// Starts the panel and waits until it is running.
pub fn start_panel() {
    let mut command = Command::new("xfce4-panel");
    command.current_dir(home()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // A panel of its own, not tied to this window.
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    let _ = command.spawn();
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if process_running() {
            std::thread::sleep(Duration::from_millis(300));
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Quits the panel and starts it again.
pub fn restart_panel() {
    quit_panel();
    start_panel();
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// The numbers of the panels there are.
#[tauri::command(async)]
pub fn panel_list() -> Vec<u32> {
    panel_ids()
}

#[derive(Serialize)]
pub struct PanelState {
    pub settings: PanelSettings,
    pub active: Vec<Plugin>,
}

/// The settings of a panel and the plugins it has, in order.
#[tauri::command(async)]
pub fn panel_read(id: u32) -> Result<PanelState, String> {
    check_panel(id)?;
    let mut catalog = PluginCatalog::new();
    let active = get_array(&format!("/panels/panel-{id}/plugin-ids"))
        .into_iter()
        .map(|plugin_id| {
            let module = get(&format!("/plugins/plugin-{plugin_id}")).unwrap_or_default();
            catalog.plugin(plugin_id, &module)
        })
        .collect();
    Ok(PanelState { settings: read_settings(id), active })
}

/// The plugins that can be added to a panel.
#[tauri::command(async)]
pub fn panel_available() -> Vec<Plugin> {
    available_plugins()
}

#[derive(Deserialize)]
pub struct PanelChange {
    pub edge: String,
    pub align: u8,
    pub mode: u32,
    pub size: u32,
    #[serde(rename = "iconSize")]
    pub icon_size: u32,
    pub length: u32,
    #[serde(rename = "lengthAdjust")]
    pub length_adjust: bool,
    pub rows: u32,
    pub autohide: u32,
    pub dark: bool,
    pub locked: bool,
    /// The position string that xfconf had, and whether the edge, the alignment
    /// or the length were changed: without a change it is left as it was.
    #[serde(rename = "oldPosition")]
    pub old_position: String,
    #[serde(rename = "positionChanged")]
    pub position_changed: bool,
    /// The plugins of the panel, in order.
    #[serde(rename = "pluginIds")]
    pub plugin_ids: Vec<u32>,
}

fn valid(change: &PanelChange) -> Result<(), String> {
    let ok = ["top", "bottom", "left", "right"].contains(&change.edge.as_str())
        && change.align <= 2
        && change.mode <= 2
        && (1..=256).contains(&change.size)
        && (1..=256).contains(&change.icon_size)
        && (1..=100).contains(&change.length)
        && (1..=6).contains(&change.rows)
        && change.autohide <= 2
        && !change.old_position.contains(['\n', '\0']);
    if ok {
        Ok(())
    } else {
        Err("Not a valid setting of the panel".to_string())
    }
}

/// Writes the settings of a panel and the list of its plugins, without
/// restarting the panel. A plugin that is not in the list any more loses its
/// settings. Whether everything could be set.
fn write_panel(id: u32, change: &PanelChange) -> bool {
    // When the edge changed, the mode follows it: a horizontal panel on a side
    // is drawn in the wrong place. A deskbar stays as it is.
    let mode = match (change.position_changed, change.mode, change.edge.as_str()) {
        (true, 0, "left" | "right") => 1,
        (true, 1, "top" | "bottom") => 0,
        _ => change.mode,
    };
    let position = if change.position_changed { build_position(&change.edge, change.align, change.length, change.size, screen_size()) } else { change.old_position.clone() };
    let key = |name: &str| format!("/panels/panel-{id}/{name}");
    let mut ok = true;
    ok &= set(&key("position"), "string", &position);
    ok &= set(&key("mode"), "uint", &mode.to_string());
    ok &= set(&key("size"), "uint", &change.size.to_string());
    ok &= set(&key("icon-size"), "uint", &change.icon_size.to_string());
    ok &= set(&key("length"), "double", &format!("{:.1}", change.length as f64));
    ok &= set(&key("length-adjust"), "bool", if change.length_adjust { "true" } else { "false" });
    ok &= set(&key("nrows"), "uint", &change.rows.to_string());
    ok &= set(&key("autohide-behavior"), "uint", &change.autohide.to_string());
    ok &= set("/panels/dark-mode", "bool", if change.dark { "true" } else { "false" });
    ok &= set(&key("position-locked"), "bool", if change.locked { "true" } else { "false" });
    ok &= set(&key("enable-struts"), "bool", "true");

    for old in get_array(&key("plugin-ids")) {
        if !change.plugin_ids.contains(&old) {
            remove(&format!("/plugins/plugin-{old}"));
        }
    }
    ok &= set_array(&key("plugin-ids"), &change.plugin_ids);
    ok
}

/// Writes the settings of a panel and the list of its plugins, and restarts the
/// panel.
#[tauri::command(async)]
pub fn panel_apply(id: u32, change: PanelChange) -> Result<(), String> {
    check_panel(id)?;
    valid(&change)?;
    let _guard = PANEL_LOCK.lock().map_err(|_| "The panel is busy".to_string())?;
    let ok = write_panel(id, &change);
    restart_panel();
    if ok {
        Ok(())
    } else {
        Err("Some settings could not be applied".to_string())
    }
}

#[derive(Deserialize)]
pub struct PanelChangeFor {
    pub id: u32,
    pub change: PanelChange,
}

/// Writes the settings of several panels and restarts the panel once, not once
/// for each of them. All the changes are checked before the first is written.
#[tauri::command(async)]
pub fn panel_apply_many(changes: Vec<PanelChangeFor>) -> Result<(), String> {
    if changes.is_empty() {
        return Ok(());
    }
    for c in &changes {
        check_panel(c.id)?;
        valid(&c.change)?;
    }
    let _guard = PANEL_LOCK.lock().map_err(|_| "The panel is busy".to_string())?;
    let mut ok = true;
    for c in &changes {
        ok &= write_panel(c.id, &c.change);
    }
    restart_panel();
    if ok {
        Ok(())
    } else {
        Err("Some settings could not be applied".to_string())
    }
}

/// Puts the plugins of a panel in a new order.
#[tauri::command(async)]
pub fn panel_set_order(id: u32, ids: Vec<u32>) -> Result<(), String> {
    check_panel(id)?;
    let _guard = PANEL_LOCK.lock().map_err(|_| "The panel is busy".to_string())?;
    // Only the plugins the panel already has, none more and none less.
    let current = get_array(&format!("/panels/panel-{id}/plugin-ids"));
    let mut a = current.clone();
    let mut b = ids.clone();
    a.sort_unstable();
    b.sort_unstable();
    if a != b {
        return Err("The plugins are not those of the panel".to_string());
    }
    set_array(&format!("/panels/panel-{id}/plugin-ids"), &ids);
    restart_panel();
    Ok(())
}

/// Takes a plugin out of a panel, with its settings.
#[tauri::command(async)]
pub fn panel_remove_plugin(id: u32, plugin_id: u32) -> Result<(), String> {
    check_panel(id)?;
    let _guard = PANEL_LOCK.lock().map_err(|_| "The panel is busy".to_string())?;
    let key = format!("/panels/panel-{id}/plugin-ids");
    let remaining: Vec<u32> = get_array(&key).into_iter().filter(|p| *p != plugin_id).collect();
    set_array(&key, &remaining);
    remove(&format!("/plugins/plugin-{plugin_id}"));
    restart_panel();
    Ok(())
}

// The next number for a plugin: above every plugin of every panel, and above the
// ones that xfconf still has settings of.
fn next_plugin_id() -> u32 {
    let mut highest = 0;
    for panel in panel_ids() {
        highest = highest.max(get_array(&format!("/panels/panel-{panel}/plugin-ids")).into_iter().max().unwrap_or(0));
    }
    if let Some(out) = query(&["-l"]) {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(rest) = line.trim().strip_prefix("/plugins/plugin-") {
                if let Ok(n) = rest.split('/').next().unwrap_or("").parse::<u32>() {
                    highest = highest.max(n);
                }
            }
        }
    }
    highest + 1
}

/// The panel a plugin is in.
pub(crate) fn panel_of_plugin(plugin_id: u32) -> Option<u32> {
    panel_ids().into_iter().find(|panel| get_array(&format!("/panels/panel-{panel}/plugin-ids")).contains(&plugin_id))
}

/// Adds a plugin, by its module, at the end of a panel. Returns its number.
#[tauri::command(async)]
pub fn panel_add_plugin(id: u32, module: String) -> Result<u32, String> {
    check_panel(id)?;
    if !available_plugins().iter().any(|p| p.module == module) {
        return Err("There is no such plugin".to_string());
    }
    let _guard = PANEL_LOCK.lock().map_err(|_| "The panel is busy".to_string())?;
    let new_id = next_plugin_id();
    set(&format!("/plugins/plugin-{new_id}"), "string", &module);
    let key = format!("/panels/panel-{id}/plugin-ids");
    let mut ids = get_array(&key);
    ids.push(new_id);
    set_array(&key, &ids);
    // A new dock brings the applications of the one that exists.
    if module == "docklike" {
        super::dock::inherit_dock(new_id, id);
    }
    restart_panel();
    Ok(new_id)
}

/// Opens the preferences of the panel, where each plugin has its own settings.
/// Xfce 4.20 has no way to open the settings of one plugin from another program.
#[tauri::command(async)]
pub fn panel_preferences(id: u32) -> Result<(), String> {
    check_panel(id)?;
    use std::os::unix::process::CommandExt;
    let mut command = Command::new("xfce4-panel");
    command.arg(format!("--preferences={id}")).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    command.process_group(0);
    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct NewPanel {
    pub id: u32,
    pub ids: Vec<u32>,
}

/// Adds a panel: along the top, as wide as the screen and empty.
#[tauri::command(async)]
pub fn panel_new() -> Result<NewPanel, String> {
    let _guard = PANEL_LOCK.lock().map_err(|_| "The panel is busy".to_string())?;
    let ids = panel_ids();
    let id = ids.iter().max().copied().unwrap_or(1) + 1;
    let mut all = ids;
    all.push(id);
    all.sort_unstable();
    let key = |name: &str| format!("/panels/panel-{id}/{name}");
    set(&key("position"), "string", &build_position("top", 1, 100, 30, screen_size()));
    set(&key("size"), "uint", "30");
    set(&key("icon-size"), "uint", "22");
    set(&key("length"), "double", "100.0");
    set(&key("length-adjust"), "bool", "true");
    set(&key("nrows"), "uint", "1");
    set(&key("autohide-behavior"), "uint", "0");
    set(&key("position-locked"), "bool", "false");
    set(&key("enable-struts"), "bool", "true");
    // The list of panels goes last, so the panel sees a complete configuration.
    set_array("/panels", &all);
    restart_panel();
    Ok(NewPanel { id, ids: all })
}

/// Deletes a panel with the plugins that no other panel uses. Returns what is left.
#[tauri::command(async)]
pub fn panel_delete(id: u32) -> Result<Vec<u32>, String> {
    let ids = panel_ids();
    if ids.len() <= 1 || !ids.contains(&id) {
        return Err("The last panel cannot be deleted".to_string());
    }
    let _guard = PANEL_LOCK.lock().map_err(|_| "The panel is busy".to_string())?;
    let left: Vec<u32> = ids.into_iter().filter(|p| *p != id).collect();
    let used_elsewhere: Vec<u32> = left.iter().flat_map(|p| get_array(&format!("/panels/panel-{p}/plugin-ids"))).collect();
    for plugin in get_array(&format!("/panels/panel-{id}/plugin-ids")) {
        if !used_elsewhere.contains(&plugin) {
            remove(&format!("/plugins/plugin-{plugin}"));
        }
    }
    remove(&format!("/panels/panel-{id}"));
    set_array("/panels", &left);
    restart_panel();
    Ok(left)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (i64, i64) = (1920, 1080);

    #[test]
    fn the_edge_comes_from_the_position_string() {
        assert_eq!(edge_of("p=8;x=960;y=1066", SCREEN, 0), "bottom");
        assert_eq!(edge_of("p=6;x=0;y=0", SCREEN, 0), "top");
        assert_eq!(edge_of("p=5;x=0;y=0", SCREEN, 1), "left");
        assert_eq!(edge_of("p=1;x=0;y=0", SCREEN, 1), "right");
        // p=10 is the bottom center, the dock of Xfce: never the left edge.
        assert_eq!(edge_of("p=10;x=0;y=0", SCREEN, 0), "bottom");
        // A corner follows the mode of the panel.
        assert_eq!(edge_of("p=6;x=0;y=0", SCREEN, 1), "left");
        assert_eq!(edge_of("p=8;x=0;y=0", SCREEN, 1), "left");
        assert_eq!(edge_of("p=2;x=0;y=0", SCREEN, 1), "right");
        assert_eq!(edge_of("p=4;x=0;y=0", SCREEN, 0), "bottom");
        assert_eq!(edge_of("", SCREEN, 0), "bottom", "without a position, the bottom, as the panel has it by default");
        // A floating panel is on the edge its center is closest to, by its mode.
        assert_eq!(edge_of("p=0;x=960;y=20", SCREEN, 0), "top");
        assert_eq!(edge_of("p=0;x=960;y=1060", SCREEN, 0), "bottom");
        assert_eq!(edge_of("p=0;x=20;y=540", SCREEN, 1), "left");
        assert_eq!(edge_of("p=0;x=1900;y=540", SCREEN, 1), "right");
        assert_eq!(edge_of("p=0;x=20;y=54", SCREEN, 1), "left", "a short vertical panel at the start is still on the left");
        assert_eq!(edge_of("p=0;x=500;y=1060", SCREEN, 2), "bottom", "a deskbar goes by the nearest edge");
    }

    #[test]
    fn the_alignment_is_where_the_panel_touches_the_screen() {
        // As wide as the screen: only the center.
        assert_eq!(align_of("p=8;x=960;y=1066", "bottom", SCREEN, 100), 1);
        assert_eq!(align_of("p=8;x=0;y=0", "bottom", SCREEN, 100), 1);
        // Shorter: at the start, at the end or in between.
        assert_eq!(align_of("p=0;x=96;y=1066", "bottom", SCREEN, 10), 0);
        assert_eq!(align_of("p=0;x=1824;y=1066", "bottom", SCREEN, 10), 2);
        assert_eq!(align_of("p=0;x=960;y=1066", "bottom", SCREEN, 10), 1);
        assert_eq!(align_of("p=0;x=300;y=1066", "bottom", SCREEN, 10), 1, "near the start is not at the start");
        assert_eq!(align_of("p=0;x=20;y=54", "left", SCREEN, 10), 0);
        assert_eq!(align_of("p=0;x=20;y=1026", "left", SCREEN, 10), 2);
    }

    #[test]
    fn a_position_that_is_built_is_read_back_the_same() {
        for edge in ["top", "bottom", "left", "right"] {
            let mode = if edge == "top" || edge == "bottom" { 0 } else { 1 };
            for align in 0..=2u8 {
                for length in [10u32, 20, 60, 80] {
                    let text = build_position(edge, align, length, 42, SCREEN);
                    assert_eq!(edge_of(&text, SCREEN, mode), edge, "{text}");
                    assert_eq!(align_of(&text, edge, SCREEN, length), align, "{edge} {align} {length}: {text}");
                }
                // With 99% there is no room to tell the places apart: it reads as the center.
                let text = build_position(edge, align, 99, 42, SCREEN);
                assert_eq!(edge_of(&text, SCREEN, mode), edge, "{text}");
                assert_eq!(align_of(&text, edge, SCREEN, 99), 1, "{text}");
            }
        }
        // A panel as wide as the screen is snapped to its edge.
        assert_eq!(build_position("bottom", 1, 100, 42, SCREEN), "p=8;x=0;y=0");
        assert_eq!(build_position("top", 0, 100, 42, SCREEN), "p=6;x=0;y=0");
        assert_eq!(build_position("left", 2, 100, 42, SCREEN), "p=5;x=0;y=0");
        assert_eq!(build_position("right", 1, 100, 42, SCREEN), "p=1;x=0;y=0");
        // A shorter one floats, with the coordinates of its center.
        assert_eq!(build_position("bottom", 1, 50, 42, SCREEN), "p=0;x=960;y=1059");
        assert_eq!(build_position("top", 0, 50, 42, SCREEN), "p=0;x=480;y=21");
        assert_eq!(build_position("bottom", 2, 50, 42, SCREEN), "p=0;x=1440;y=1059");
    }

    #[test]
    fn numbers_are_read_with_a_comma_too() {
        assert_eq!(parse_number(Some("100,000000".into()), 5), 100);
        assert_eq!(parse_number(Some("42".into()), 5), 42);
        assert_eq!(parse_number(Some("24.6".into()), 5), 25);
        assert_eq!(parse_number(None, 5), 5);
        assert_eq!(parse_number(Some("abc".into()), 5), 5);
    }

    #[test]
    fn the_name_of_a_plugin_is_in_the_language_of_the_user() {
        let mut keys = std::collections::HashMap::new();
        keys.insert("Name".to_string(), "Clock".to_string());
        keys.insert("Name[es]".to_string(), "Reloj".to_string());
        assert_eq!(plugin_name(&keys, &["es_ES".to_string(), "es".to_string()], "clock"), "Reloj");
        assert_eq!(plugin_name(&keys, &["fr".to_string()], "clock"), "Clock");
        assert_eq!(plugin_name(&std::collections::HashMap::new(), &[], "clock"), "clock");
    }

    #[test]
    fn the_changes_that_make_no_sense_are_refused() {
        let good = PanelChange {
            edge: "top".into(),
            align: 1,
            mode: 0,
            size: 30,
            icon_size: 22,
            length: 100,
            length_adjust: true,
            rows: 1,
            autohide: 0,
            dark: false,
            locked: false,
            old_position: "p=6;x=0;y=0".into(),
            position_changed: false,
            plugin_ids: vec![1, 2],
        };
        assert!(valid(&good).is_ok());
        let mut bad = PanelChange { edge: "middle".into(), ..good };
        assert!(valid(&bad).is_err());
        bad.edge = "top".into();
        bad.length = 0;
        assert!(valid(&bad).is_err());
        bad.length = 100;
        bad.old_position = "p=6\n[x]".into();
        assert!(valid(&bad).is_err());
    }

    // Reads the panel of this computer; it changes nothing.
    #[test]
    fn the_panel_of_this_computer_is_read() {
        let ids = panel_list();
        println!("panels: {ids:?}");
        let state = panel_read(ids[0]).unwrap();
        let s = &state.settings;
        println!("edge={} align={} mode={} size={} icon={} length={} adjust={} rows={} autohide={} dark={} locked={} position={}", s.edge, s.align, s.mode, s.size, s.icon_size, s.length, s.length_adjust, s.rows, s.autohide, s.dark, s.locked, s.position);
        println!("active: {:?}", state.active.iter().map(|p| format!("{}:{}{}", p.id, p.name, if p.icon.is_some() { "*" } else { "" })).collect::<Vec<_>>());
        let available = panel_available();
        println!("available: {} plugins, {} with an icon", available.len(), available.iter().filter(|p| p.icon.is_some()).count());
        assert!(!available.is_empty());
        assert!(state.active.iter().all(|p| !p.module.is_empty()));
    }
}
