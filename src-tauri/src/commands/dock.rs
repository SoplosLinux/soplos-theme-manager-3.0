// The dock of Xfce is the Docklike Taskbar plugin of the panel. Its pinned
// applications live in ~/.config/xfce4/panel/docklike-<id>.rc, on a line
// `pinned=a;b;c;` whose entries are desktop file names without the extension
// (`thunar`) or full paths.

use super::icons::IconFinder;
use super::previews::file_data_url;
use super::themes::home;
use super::xfconf;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ICON_SIZE: i64 = 48;
const MAX_ICON_BYTES: u64 = 512 * 1024;

#[derive(Serialize)]
pub struct DockApp {
    /// What the .rc file holds for it: a desktop file name or a full path.
    #[serde(rename = "desktopPath")]
    pub desktop_path: String,
    pub name: String,
    /// The icon as a data address, when there is one.
    pub icon: Option<String>,
}

// ── The desktop files of the applications ────────────────────────────────────

fn application_dirs() -> Vec<PathBuf> {
    let home = home();
    vec![
        PathBuf::from("/usr/share/applications"),
        home.join(".local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
    ]
}

// The languages of the user, most wanted first, as the names a desktop file
// uses: for es_ES.UTF-8 they are "es_ES" and "es".
pub fn language_keys() -> Vec<String> {
    let mut keys = Vec::new();
    let mut add = |value: &str| {
        let base = value.split(['.', '@']).next().unwrap_or("");
        if base.is_empty() || base == "C" || base == "POSIX" {
            return;
        }
        for key in [base.to_string(), base.split('_').next().unwrap_or("").to_string()] {
            if !key.is_empty() && !keys.contains(&key) {
                keys.push(key);
            }
        }
    };
    if let Ok(list) = std::env::var("LANGUAGE") {
        list.split(':').for_each(&mut add);
    }
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            add(&value);
        }
    }
    keys
}

struct Entry {
    name: String,
    icon: Option<String>,
}

pub fn on_path(program: &str) -> bool {
    if program.starts_with('/') {
        return Path::new(program).exists();
    }
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).exists()))
}

// Reads a desktop file the way a desktop does and says no to the ones that are
// not meant to be shown: hidden, without a program to run, or for other desktops.
fn parse_entry(text: &str, languages: &[String], desktops: &[String]) -> Option<Entry> {
    let mut in_section = false;
    let mut name = None;
    let mut names: Vec<(String, String)> = Vec::new();
    let (mut icon, mut kind, mut try_exec, mut only_show_in, mut not_show_in) = (None, None, None, None, None);
    let (mut no_display, mut hidden) = (false, false);

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == "[Desktop Entry]";
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "Name" => name = Some(value.to_string()),
            "Icon" => icon = Some(value.to_string()),
            "Type" => kind = Some(value.to_string()),
            "TryExec" => try_exec = Some(value.to_string()),
            "OnlyShowIn" => only_show_in = Some(value.to_string()),
            "NotShowIn" => not_show_in = Some(value.to_string()),
            "NoDisplay" => no_display = value == "true",
            "Hidden" => hidden = value == "true",
            _ => {
                if let Some(lang) = key.strip_prefix("Name[").and_then(|k| k.strip_suffix(']')) {
                    names.push((lang.to_string(), value.to_string()));
                }
            }
        }
    }

    if kind.as_deref() != Some("Application") || no_display || hidden {
        return None;
    }
    let listed = |value: &Option<String>| value.as_ref().map(|v| v.split(';').filter(|s| !s.is_empty()).map(|s| s.to_uppercase()).collect::<Vec<_>>());
    if let Some(only) = listed(&only_show_in) {
        if !only.iter().any(|d| desktops.contains(d)) {
            return None;
        }
    }
    if let Some(not) = listed(&not_show_in) {
        if not.iter().any(|d| desktops.contains(d)) {
            return None;
        }
    }
    if try_exec.as_deref().is_some_and(|t| !t.is_empty() && !on_path(t)) {
        return None;
    }
    let localized = languages.iter().find_map(|lang| names.iter().find(|(l, _)| l == lang).map(|(_, v)| v.clone()));
    let name = localized.or(name).filter(|n| !n.is_empty())?;
    Some(Entry { name, icon })
}

fn current_desktops() -> Vec<String> {
    let list: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().split(':').filter(|s| !s.is_empty()).map(|s| s.to_uppercase()).collect();
    if list.is_empty() {
        vec!["XFCE".to_string()]
    } else {
        list
    }
}

// The desktop file an entry of the .rc names: a full path, or a file name with
// or without its extension in the usual folders.
fn resolve(entry: &str) -> Option<PathBuf> {
    let path = Path::new(entry);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_path_buf());
    }
    for dir in application_dirs() {
        for candidate in [format!("{entry}.desktop"), entry.to_string()] {
            let file = dir.join(&candidate);
            if file.is_file() {
                return Some(file);
            }
        }
    }
    None
}

struct Reader {
    languages: Vec<String>,
    desktops: Vec<String>,
    theme: String,
    finder: IconFinder,
}

impl Reader {
    fn new() -> Self {
        Reader {
            languages: language_keys(),
            desktops: current_desktops(),
            theme: xfconf::get("xsettings", "/Net/IconThemeName").unwrap_or_else(|| "hicolor".to_string()),
            finder: IconFinder::new(),
        }
    }

    fn icon(&mut self, name: &str) -> Option<String> {
        let file = if Path::new(name).is_absolute() {
            PathBuf::from(name)
        } else {
            // Some files name the icon with its extension.
            let bare = ["png", "svg", "xpm"].iter().find_map(|e| name.strip_suffix(&format!(".{e}"))).unwrap_or(name);
            self.finder.find(&self.theme, bare, ICON_SIZE)?
        };
        file_data_url(&file, MAX_ICON_BYTES)
    }

    fn app(&mut self, shown_as: &str, file: &Path) -> Option<DockApp> {
        let text = fs::read_to_string(file).ok()?;
        let entry = parse_entry(&text, &self.languages, &self.desktops)?;
        let icon = entry.icon.as_deref().and_then(|name| self.icon(name));
        Some(DockApp { desktop_path: shown_as.to_string(), name: entry.name, icon })
    }
}

// ── The plugin and its settings file ─────────────────────────────────────────

// The numbers of the Docklike plugins in the panels, as xfconf has them, in
// numeric order. There can be several: one in each panel that acts as a dock.
fn plugin_ids() -> Vec<u32> {
    let Ok(out) = Command::new("xfconf-query").args(["-c", "xfce4-panel", "-lv"]).output() else { return Vec::new() };
    let mut ids: Vec<u32> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let (property, value) = (parts.next()?, parts.next()?);
            let id = property.strip_prefix("/plugins/plugin-")?;
            if value != "docklike" || id.contains('/') {
                return None;
            }
            id.parse().ok()
        })
        .collect();
    ids.sort_unstable();
    ids
}

fn panel_dir() -> PathBuf {
    home().join(".config/xfce4/panel")
}

fn rc_file(id: u32) -> PathBuf {
    panel_dir().join(format!("docklike-{id}.rc"))
}

// The file of a dock, made if the plugin has none yet. Without an id, or with
// one that is not a dock of the panel, the first dock is used; and without any
// Docklike in the panel, the first docklike file that exists.
fn config_file(wanted: Option<u32>) -> Option<PathBuf> {
    let ids = plugin_ids();
    if let Some(id) = wanted.filter(|w| ids.contains(w)).or_else(|| ids.first().copied()) {
        let file = rc_file(id);
        if !file.exists() {
            fs::create_dir_all(panel_dir()).ok()?;
            fs::write(&file, "[user]\npinned=\n").ok()?;
        }
        return Some(file);
    }
    let mut files: Vec<PathBuf> = fs::read_dir(panel_dir()).ok()?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("docklike-") && n.to_string_lossy().ends_with(".rc"))).collect();
    files.sort();
    files.into_iter().next()
}

/// A new Docklike starts as a copy of the dock that is already there: the one
/// of its own panel if that has one, else the first. Without this a new dock
/// would be empty and its applications would have to be pinned again.
pub fn inherit_dock(new_id: u32, panel: u32) {
    let file = rc_file(new_id);
    if file.exists() {
        return;
    }
    let others: Vec<u32> = plugin_ids().into_iter().filter(|id| *id != new_id).collect();
    let source = others.iter().copied().find(|id| super::panel::panel_of_plugin(*id) == Some(panel)).or_else(|| others.first().copied());
    if let Some(source) = source {
        if rc_file(source).is_file() {
            let _ = fs::create_dir_all(panel_dir());
            let _ = fs::copy(rc_file(source), file);
        }
    }
}

/// The entries of the `pinned=` line.
fn parse_pinned(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("pinned="))
        .flat_map(|v| v.split(';').map(|e| e.trim().to_string()).collect::<Vec<_>>())
        .filter(|e| !e.is_empty())
        .collect()
}

/// The file with its `pinned=` line replaced by the entries; the rest is kept.
fn rewrite_pinned(text: &str, entries: &[String]) -> String {
    let line = format!("pinned={};\n", entries.join(";"));
    let mut out = String::new();
    let mut written = false;
    for existing in text.lines() {
        if existing.starts_with("pinned=") {
            out.push_str(&line);
            written = true;
        } else {
            out.push_str(existing);
            out.push('\n');
        }
    }
    if !written {
        out.push_str(&line);
    }
    out
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Whether the Docklike plugin is in the panel.
#[tauri::command(async)]
pub fn dock_active() -> bool {
    !plugin_ids().is_empty()
}

#[derive(Serialize)]
pub struct DockInfo {
    /// The number of the plugin.
    pub id: u32,
    /// The panel it is in.
    pub panel: Option<u32>,
}

/// Every dock of the panels, in order.
#[tauri::command(async)]
pub fn dock_list() -> Vec<DockInfo> {
    plugin_ids().into_iter().map(|id| DockInfo { id, panel: super::panel::panel_of_plugin(id) }).collect()
}

/// The applications pinned to the dock, in order. An entry that names no
/// application that is installed and meant to be shown is left out.
#[tauri::command(async)]
pub fn dock_pinned(id: Option<u32>) -> Vec<DockApp> {
    let Some(file) = config_file(id) else { return Vec::new() };
    let Ok(text) = fs::read_to_string(&file) else { return Vec::new() };
    let mut reader = Reader::new();
    parse_pinned(&text).into_iter().filter_map(|entry| resolve(&entry).and_then(|path| reader.app(&entry, &path))).collect()
}

/// Every application that can be pinned, by name.
#[tauri::command(async)]
pub fn dock_available() -> Vec<DockApp> {
    let mut reader = Reader::new();
    let mut seen: Vec<String> = Vec::new();
    let mut apps: Vec<DockApp> = Vec::new();
    for dir in application_dirs() {
        let Ok(read) = fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "desktop")).collect();
        files.sort();
        for file in files {
            let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if seen.contains(&name) {
                continue;
            }
            seen.push(name);
            if let Some(app) = reader.app(&file.to_string_lossy(), &file) {
                apps.push(app);
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Saves the pinned applications and restarts the panel to show them.
#[tauri::command(async)]
pub fn dock_save(id: Option<u32>, paths: Vec<String>) -> Result<(), String> {
    // The file is one line of entries split by ";", so none can hold one.
    if paths.iter().any(|p| p.is_empty() || p.contains(';') || p.contains('\n') || p.contains('\0')) {
        return Err("Not a valid entry".to_string());
    }
    let file = config_file(id).ok_or("The dock has no settings file")?;
    let text = fs::read_to_string(&file).unwrap_or_default();
    fs::write(&file, rewrite_pinned(&text, &paths)).map_err(|e| e.to_string())?;
    super::panel::restart_panel();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_pinned_line_is_read_and_rewritten_keeping_the_rest() {
        let text = "[user]\nsomething=1\npinned=google-chrome;thunar;vlc;\nother=2\n";
        assert_eq!(parse_pinned(text), vec!["google-chrome", "thunar", "vlc"]);
        let new = rewrite_pinned(text, &keys(&["vlc", "/usr/share/applications/x.desktop"]));
        assert_eq!(new, "[user]\nsomething=1\npinned=vlc;/usr/share/applications/x.desktop;\nother=2\n");
        // Without the line, it is added; an empty dock leaves an empty line.
        assert_eq!(rewrite_pinned("[user]\n", &keys(&["a"])), "[user]\npinned=a;\n");
        assert_eq!(parse_pinned(&rewrite_pinned(text, &[])), Vec::<String>::new());
    }

    #[test]
    fn a_desktop_file_gives_its_name_in_the_language_of_the_user() {
        let text = "[Desktop Entry]\nType=Application\nName=Files\nName[es]=Archivos\nName[es_ES]=Archivos de Spain\nName[de]=Dateien\nIcon=thunar\n\n[Desktop Action x]\nName=Not this\n";
        let desktops = keys(&["XFCE"]);
        assert_eq!(parse_entry(text, &keys(&["es_ES", "es"]), &desktops).unwrap().name, "Archivos de Spain");
        assert_eq!(parse_entry(text, &keys(&["es"]), &desktops).unwrap().name, "Archivos");
        assert_eq!(parse_entry(text, &keys(&["fr"]), &desktops).unwrap().name, "Files");
        assert_eq!(parse_entry(text, &[], &desktops).unwrap().icon.as_deref(), Some("thunar"));
    }

    #[test]
    fn what_is_not_meant_to_be_shown_is_left_out() {
        let desktops = keys(&["XFCE"]);
        let entry = |extra: &str| parse_entry(&format!("[Desktop Entry]\nType=Application\nName=A\n{extra}"), &[], &desktops).is_some();
        assert!(entry(""));
        assert!(!entry("NoDisplay=true\n"));
        assert!(!entry("Hidden=true\n"));
        assert!(!entry("OnlyShowIn=GNOME;KDE;\n"));
        assert!(entry("OnlyShowIn=GNOME;XFCE;\n"));
        assert!(!entry("NotShowIn=XFCE;\n"));
        assert!(!entry("TryExec=/no/such/program\n"));
        assert!(entry("TryExec=sh\n"));
        assert!(parse_entry("[Desktop Entry]\nType=Link\nName=A\n", &[], &desktops).is_none());
    }

    #[test]
    fn the_dock_of_this_computer_is_read() {
        println!("active: {}", dock_active());
        let pinned = dock_pinned(None);
        println!("pinned: {:?}", pinned.iter().map(|a| format!("{}{}", a.name, if a.icon.is_some() { "*" } else { "" })).collect::<Vec<_>>());
        let available = dock_available();
        let with_icon = available.iter().filter(|a| a.icon.is_some()).count();
        println!("available: {} apps, {with_icon} with an icon", available.len());
        assert!(!available.is_empty());
    }
}
