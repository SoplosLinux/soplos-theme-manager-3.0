// The saved themes: a look of the whole desktop (GTK theme, icons, cursor,
// fonts, window borders, wallpaper and panel) kept under
// ~/.config/soplos-theme-manager/themes/<name>/ so it can be applied again or
// shared as a .sth file (see bundle_io).
//
//   theme.conf      key=value, what to apply
//   metadata.json   name, author, date
//   preview/        preview.png (640x360), thumbnail.png, preview_full.png
//   assets/panel/   xfce4-panel.xml and the files of its plugins

use super::panel::{quit_panel, start_panel};
use super::previews::file_data_url;
use super::themes::home;
use super::{wallpapers, xfconf, xfconf_xml};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

pub const FORMAT: &str = "soplos-theme-bundle-v3";
const MAX_PREVIEW_BYTES: u64 = 3 * 1024 * 1024;

// One change of the desktop at a time.
static LOCK: Mutex<()> = Mutex::new(());

pub fn themes_root() -> PathBuf {
    home().join(".config").join("soplos-theme-manager").join("themes")
}

pub fn panel_files_dir() -> PathBuf {
    home().join(".config").join("xfce4").join("panel")
}

pub fn valid_name(name: &str) -> bool {
    let t = name.trim();
    !t.is_empty() && t == name && name.chars().count() <= 100 && !name.starts_with('.') && !name.contains('/') && !name.contains('\0') && !name.chars().any(|c| c.is_control())
}

// ── theme.conf ───────────────────────────────────────────────────────────────


#[derive(Default, Clone, Debug)]
pub struct Conf(pub Vec<(String, String)>);

impl Conf {
    pub fn parse(text: &str) -> Conf {
        let mut list = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                list.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        Conf(list)
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty())
    }
    pub fn set(&mut self, key: &str, value: &str) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value.to_string(),
            None => self.0.push((key.to_string(), value.to_string())),
        }
    }
    pub fn remove(&mut self, key: &str) {
        self.0.retain(|(k, _)| k != key);
    }
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (k, v) in &self.0 {
            // A value never carries a line break: it would add a key.
            out.push_str(&format!("{k}={}\n", v.replace(['\n', '\r'], " ")));
        }
        out
    }
    pub fn read(dir: &Path) -> Conf {
        Conf::parse(&fs::read_to_string(dir.join("theme.conf")).unwrap_or_default())
    }
}

// ── The list ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SavedTheme {
    pub name: String,
    pub author: String,
    pub created: String,
    pub preview: Option<String>,
    /// What the theme carries: "gtk", "icons", "cursor", "wallpaper", "panel".
    pub parts: Vec<String>,
}

fn parts_of(conf: &Conf) -> Vec<String> {
    let mut parts = Vec::new();
    for (key, part) in [("gtk_theme", "gtk"), ("icon_theme", "icons"), ("cursor_theme", "cursor"), ("wallpaper", "wallpaper")] {
        if conf.get(key).is_some() {
            parts.push(part.to_string());
        }
    }
    if conf.get("has_panel_config") == Some("true") {
        parts.push("panel".to_string());
    }
    parts
}

fn metadata_of(dir: &Path) -> serde_json::Value {
    fs::read_to_string(dir.join("metadata.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(serde_json::Value::Null)
}

fn text_of(value: &serde_json::Value, key: &str) -> String {
    value.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

#[tauri::command(async)]
pub fn list_saved_themes() -> Vec<SavedTheme> {
    let Ok(read) = fs::read_dir(themes_root()) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir() && p.join("theme.conf").is_file()).collect();
    dirs.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    dirs.iter()
        .filter_map(|dir| {
            let name = dir.file_name()?.to_string_lossy().to_string();
            let meta = metadata_of(dir);
            let preview = file_data_url(&dir.join("preview").join("preview.png"), MAX_PREVIEW_BYTES).or_else(|| file_data_url(&dir.join("preview").join("thumbnail.png"), MAX_PREVIEW_BYTES));
            Some(SavedTheme { author: text_of(&meta, "author"), created: text_of(&meta, "created"), preview, parts: parts_of(&Conf::read(dir)), name })
        })
        .collect()
}

#[tauri::command(async)]
pub fn remove_saved_theme(name: String) -> Result<(), String> {
    if !valid_name(&name) {
        return Err("Not a valid name".to_string());
    }
    let dir = themes_root().join(&name);
    if !dir.join("theme.conf").is_file() {
        return Err("It is not a saved theme".to_string());
    }
    fs::remove_dir_all(dir).map_err(|e| e.to_string())
}

/// Renames a saved theme. The error is "exists" when the new name is taken.
#[tauri::command(async)]
pub fn rename_saved_theme(name: String, new_name: String) -> Result<(), String> {
    if !valid_name(&name) || !valid_name(&new_name) {
        return Err("invalid".to_string());
    }
    let _guard = LOCK.lock().map_err(|_| "busy".to_string())?;
    let from = themes_root().join(&name);
    let to = themes_root().join(&new_name);
    if !from.join("theme.conf").is_file() {
        return Err("It is not a saved theme".to_string());
    }
    if name == new_name {
        return Ok(());
    }
    // A name that only changes in case is the same folder on some disks: only
    // a different folder counts as taken.
    if fs::symlink_metadata(&to).is_ok() && fs::canonicalize(&to).ok() != fs::canonicalize(&from).ok() {
        return Err("exists".to_string());
    }
    fs::rename(&from, &to).map_err(|e| e.to_string())?;
    // The name inside the file follows the folder; it is what an export carries.
    if let Ok(text) = fs::read_to_string(to.join("metadata.json")) {
        if let Ok(mut meta) = serde_json::from_str::<serde_json::Value>(&text) {
            meta["name"] = new_name.clone().into();
            let _ = fs::write(to.join("metadata.json"), serde_json::to_string_pretty(&meta).unwrap_or_default());
        }
    }
    Ok(())
}

// ── Plugin files of the panel ────────────────────────────────────────────────

// The number at the end of a plugin file or folder: docklike-28.rc -> 28.
pub fn plugin_id_of(name: &str) -> Option<u32> {
    let stem = name.split('.').next()?;
    let (head, tail) = stem.rsplit_once('-')?;
    (!head.is_empty() && head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit())).then(|| tail.parse().ok())?
}

fn copy_any(from: &Path, to: &Path) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(from)?;
    if meta.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_any(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else if meta.is_file() {
        fs::copy(from, to).map(|_| ())
    } else {
        Ok(())
    }
}

// The ids of the plugins that an xfce4-panel.xml defines.
fn plugin_ids_in(xml: &str) -> Vec<u32> {
    xfconf_xml::parse(xml)
        .map(|list| list.iter().filter_map(|s| s.path.strip_prefix("/plugins/plugin-").and_then(|r| (!r.contains('/')).then(|| r.parse().ok()).flatten())).collect())
        .unwrap_or_default()
}

// The numbers of the Docklike plugins in the panel, from one listing of xfconf.
fn docklike_ids() -> Vec<u32> {
    let Ok(out) = Command::new("xfconf-query").args(["-c", "xfce4-panel", "-lv"]).output() else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let (property, value) = (parts.next()?, parts.next()?);
            let id = property.strip_prefix("/plugins/plugin-")?;
            (value == "docklike").then(|| id.parse().ok()).flatten()
        })
        .collect()
}

fn docklike_pinned() -> Option<String> {
    let id = *docklike_ids().first()?;
    let text = fs::read_to_string(panel_files_dir().join(format!("docklike-{id}.rc"))).ok()?;
    text.lines().find(|l| l.starts_with("pinned=")).map(|l| l.to_string())
}

// ── Saving the current look ──────────────────────────────────────────────────

pub(crate) fn current_wallpaper() -> Option<String> {
    let listing = Command::new("xfconf-query").args(["-c", "xfce4-desktop", "-lv"]).output().ok()?;
    String::from_utf8_lossy(&listing.stdout).lines().filter(|l| l.contains("last-image")).find_map(|line| {
        let path = line.split_once(char::is_whitespace)?.1.trim();
        Path::new(path).is_file().then(|| path.to_string())
    })
}

fn convert_program() -> Option<&'static str> {
    ["convert", "magick"].into_iter().find(|p| super::dock::on_path(p))
}

#[tauri::command]
pub fn screenshot_available() -> bool {
    super::dock::on_path("scrot") && convert_program().is_some()
}

// Shows the desktop, waits so the window of the program can be out of the way,
// takes the picture and makes the two sizes.
fn take_screenshot(preview: &Path) -> Result<(), String> {
    let _ = Command::new("notify-send").args(["-t", "3000", "Soplos Theme Manager", "Taking screenshot in 3 seconds..."]).output();
    let has_wmctrl = super::dock::on_path("wmctrl");
    if has_wmctrl {
        let _ = Command::new("wmctrl").args(["-k", "on"]).output();
    }
    std::thread::sleep(Duration::from_secs(3));
    let full = preview.join("preview_full.png");
    let shot = Command::new("scrot").arg("-o").arg(&full).output();
    if has_wmctrl {
        let _ = Command::new("wmctrl").args(["-k", "off"]).output();
    }
    if !shot.is_ok_and(|o| o.status.success()) || !full.is_file() {
        return Err("The screenshot could not be taken".to_string());
    }
    let program = convert_program().ok_or("ImageMagick is missing")?;
    for (file, size) in [("preview.png", "640x360"), ("thumbnail.png", "150x90")] {
        let ok = Command::new(program)
            .arg(&full)
            .args(["-resize", &format!("{size}^"), "-gravity", "center", "-extent", size])
            .arg(preview.join(file))
            .output()
            .is_ok_and(|o| o.status.success());
        if !ok {
            return Err("The picture could not be resized".to_string());
        }
    }
    Ok(())
}

fn now_iso() -> String {
    Command::new("date").arg("+%Y-%m-%dT%H:%M:%S").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn author_name() -> String {
    std::env::var("USER").unwrap_or_default()
}

/// Saves the look of the desktop as a theme. A theme of that name is never
/// overwritten: the error is "exists".
#[tauri::command(async)]
pub fn save_current_theme(name: String, screenshot: bool) -> Result<(), String> {
    if !valid_name(&name) {
        return Err("invalid".to_string());
    }
    let _guard = LOCK.lock().map_err(|_| "busy".to_string())?;
    let root = themes_root();
    let target = root.join(&name);
    if target.exists() {
        return Err("exists".to_string());
    }
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let stage = root.join(format!(".saving-{}", std::process::id()));
    let _ = fs::remove_dir_all(&stage);
    let result = write_current(&stage, &name, screenshot);
    match result {
        Ok(()) => fs::rename(&stage, &target).map_err(|e| {
            let _ = fs::remove_dir_all(&stage);
            e.to_string()
        }),
        Err(e) => {
            let _ = fs::remove_dir_all(&stage);
            Err(e)
        }
    }
}

fn write_current(stage: &Path, name: &str, screenshot: bool) -> Result<(), String> {
    fs::create_dir_all(stage.join("preview")).map_err(|e| e.to_string())?;
    let mut conf = Conf::default();
    let gtk = xfconf::get("xsettings", "/Net/ThemeName");
    let wm = xfconf::get("xfwm4", "/general/theme").or_else(|| gtk.clone());
    let pairs = [
        ("gtk_theme", gtk),
        ("icon_theme", xfconf::get("xsettings", "/Net/IconThemeName")),
        ("cursor_theme", xfconf::get("xsettings", "/Gtk/CursorThemeName")),
        ("gtk_font", xfconf::get("xsettings", "/Gtk/FontName")),
        ("cursor_size", xfconf::get("xsettings", "/Gtk/CursorThemeSize")),
        ("wm_theme", wm),
        ("wm_button_layout", xfconf::get("xfwm4", "/general/button_layout")),
        ("wm_title_font", xfconf::get("xfwm4", "/general/title_font")),
        ("wallpaper", current_wallpaper()),
    ];
    for (key, value) in pairs {
        if let Some(value) = value {
            conf.set(key, &value);
        }
    }

    // The panel: its settings as xfconf keeps them, and the files of its plugins.
    let xml_file = home().join(".config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml");
    if let Ok(xml) = fs::read_to_string(&xml_file) {
        let ids = plugin_ids_in(&xml);
        let panel = stage.join("assets").join("panel");
        let plugins = panel.join("plugins");
        fs::create_dir_all(&plugins).map_err(|e| e.to_string())?;
        fs::write(panel.join("xfce4-panel.xml"), xml).map_err(|e| e.to_string())?;
        if let Ok(read) = fs::read_dir(panel_files_dir()) {
            for entry in read.flatten() {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if plugin_id_of(&file_name).is_some_and(|id| ids.contains(&id)) {
                    let _ = copy_any(&entry.path(), &plugins.join(&file_name));
                }
            }
        }
        conf.set("has_panel_config", "true");
    }
    fs::write(stage.join("theme.conf"), conf.text()).map_err(|e| e.to_string())?;

    let meta = serde_json::json!({
        "name": name, "version": "1.0", "author": author_name(), "created": now_iso(),
        "app_version": env!("CARGO_PKG_VERSION"), "format": FORMAT,
    });
    fs::write(stage.join("metadata.json"), serde_json::to_string_pretty(&meta).unwrap_or_default()).map_err(|e| e.to_string())?;

    if screenshot {
        take_screenshot(&stage.join("preview"))?;
    }
    Ok(())
}

// ── Applying a theme ─────────────────────────────────────────────────────────

fn apply_panel(dir: &Path) -> Result<(), String> {
    let panel = dir.join("assets").join("panel");
    let xml = fs::read_to_string(panel.join("xfce4-panel.xml")).map_err(|e| e.to_string())?;
    let ids = plugin_ids_in(&xml);
    // Read completely before the panel is touched.
    xfconf_xml::parse(&xml)?;

    // The applications pinned to the dock belong to the user, not to the theme.
    let pinned = docklike_pinned();

    quit_panel();
    xfconf_xml::replace_channel("xfce4-panel", &xml)?;

    // The files of the plugins: the ones of the old panel go, the ones of the
    // theme come, and no old file can be taken by a new plugin of the same number.
    let files = panel_files_dir();
    fs::create_dir_all(&files).map_err(|e| e.to_string())?;
    if let Ok(read) = fs::read_dir(&files) {
        for entry in read.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if plugin_id_of(&file_name).is_some() {
                let path = entry.path();
                let _ = if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
            }
        }
    }
    if let Ok(read) = fs::read_dir(panel.join("plugins")) {
        for entry in read.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if plugin_id_of(&file_name).is_some_and(|id| ids.contains(&id)) {
                let _ = copy_any(&entry.path(), &files.join(&file_name));
            }
        }
    }
    if let Some(line) = pinned {
        write_pinned(&line, &ids);
    }
    start_panel();
    Ok(())
}

// Puts the pinned line of the user in the dock of the new panel.
fn write_pinned(line: &str, ids: &[u32]) {
    let Some(id) = docklike_ids().into_iter().find(|id| ids.contains(id)) else { return };
    let file = panel_files_dir().join(format!("docklike-{id}.rc"));
    let text = fs::read_to_string(&file).unwrap_or_else(|_| "[user]\n".to_string());
    let mut done = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            if l.starts_with("pinned=") {
                done = true;
                line.to_string()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !done {
        out.push(line.to_string());
    }
    let _ = fs::write(&file, out.join("\n") + "\n");
}

fn apply_look(conf: &Conf) -> Vec<&'static str> {
    let mut failed = Vec::new();
    let mut text = |channel: &str, prop: &str, key: &str, fallback: Option<&str>, label: &'static str| {
        if let Some(value) = conf.get(key).or(fallback) {
            if !xfconf::set_string(channel, prop, value) {
                failed.push(label);
            }
        }
    };
    text("xsettings", "/Net/ThemeName", "gtk_theme", None, "gtk");
    text("xsettings", "/Net/IconThemeName", "icon_theme", None, "icons");
    text("xsettings", "/Gtk/CursorThemeName", "cursor_theme", None, "cursor");
    text("xsettings", "/Gtk/FontName", "gtk_font", None, "font");
    text("xfwm4", "/general/theme", "wm_theme", conf.get("gtk_theme"), "borders");
    text("xfwm4", "/general/button_layout", "wm_button_layout", None, "buttons");
    text("xfwm4", "/general/title_font", "wm_title_font", None, "title");
    if let Some(size) = conf.get("cursor_size").and_then(|s| s.parse::<i64>().ok()) {
        if !xfconf::set_typed("xsettings", "/Gtk/CursorThemeSize", "int", &size.to_string()) {
            failed.push("cursor_size");
        }
    }
    failed
}

/// Applies a saved theme to the desktop. The panel is restarted when the theme
/// carries one. Returns the parts that could not be set.
#[tauri::command(async)]
pub fn apply_saved_theme(name: String) -> Result<Vec<String>, String> {
    if !valid_name(&name) {
        return Err("Not a valid name".to_string());
    }
    let _guard = LOCK.lock().map_err(|_| "busy".to_string())?;
    let dir = themes_root().join(&name);
    if !dir.join("theme.conf").is_file() {
        return Err("It is not a saved theme".to_string());
    }
    let conf = Conf::read(&dir);
    let mut failed: Vec<String> = Vec::new();
    // The look goes first and the panel after it: the panel starts once, with
    // the final GTK and icon themes. The other way round, the themes changed
    // while the plugins of the new panel were still loading, and some of them
    // (the status tray) closed unexpectedly and asked to be restarted.
    failed.extend(apply_look(&conf).into_iter().map(String::from));
    if conf.get("has_panel_config") == Some("true") && dir.join("assets/panel/xfce4-panel.xml").is_file() {
        if apply_panel(&dir).is_err() {
            failed.push("panel".to_string());
        }
    }
    if let Some(wallpaper) = conf.get("wallpaper") {
        if wallpapers::set_wallpaper(wallpaper.to_string()).is_err() {
            failed.push("wallpaper".to_string());
        }
    }
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_that_climb_or_hide_are_refused() {
        for bad in ["", " a", "a/b", "..", ".hidden", "a\nb"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        assert!(valid_name("soplos-modern-black"));
        assert!(valid_name("Mi tema 2"));
    }

    #[test]
    fn a_theme_is_renamed_and_never_over_another() {
        let root = themes_root();
        let (a, b) = (format!("zz-test-a-{}", std::process::id()), format!("zz-test-b-{}", std::process::id()));
        for n in [&a, &b] {
            fs::create_dir_all(root.join(n)).unwrap();
            fs::write(root.join(n).join("theme.conf"), "gtk_theme=X\n").unwrap();
            fs::write(root.join(n).join("metadata.json"), format!("{{\"name\":\"{n}\"}}")).unwrap();
        }
        assert_eq!(rename_saved_theme(a.clone(), b.clone()), Err("exists".to_string()));
        let c = format!("zz-test-c-{}", std::process::id());
        rename_saved_theme(a.clone(), c.clone()).unwrap();
        assert!(!root.join(&a).exists() && root.join(&c).join("theme.conf").is_file());
        assert!(fs::read_to_string(root.join(&c).join("metadata.json")).unwrap().contains(&c));
        for n in [&b, &c] {
            let _ = fs::remove_dir_all(root.join(n));
        }
    }

    #[test]
    fn the_plugin_number_is_taken_from_the_end() {
        assert_eq!(plugin_id_of("docklike-28.rc"), Some(28));
        assert_eq!(plugin_id_of("launcher-17"), Some(17));
        assert_eq!(plugin_id_of("xfce4-clipman-actions.xml"), None);
        assert_eq!(plugin_id_of("whiskermenu-1.rc"), Some(1));
    }

    #[test]
    fn the_conf_keeps_its_keys_and_drops_line_breaks() {
        let mut c = Conf::parse("# c\ngtk_theme=A\nwallpaper=/x y.jpg\n");
        assert_eq!(c.get("wallpaper"), Some("/x y.jpg"));
        c.set("gtk_font", "Ubuntu\nevil=1");
        assert_eq!(Conf::parse(&c.text()).get("evil"), None);
    }

    #[test]
    fn what_a_theme_carries_is_listed() {
        let c = Conf::parse("gtk_theme=A\ncursor_theme=C\nwallpaper=/a.jpg\nhas_panel_config=true\n");
        assert_eq!(parts_of(&c), ["gtk", "cursor", "wallpaper", "panel"]);
    }
}
