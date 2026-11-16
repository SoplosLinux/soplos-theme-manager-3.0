// The installed GTK, icon and cursor themes: which exist, which one is active
// and how to activate another. Each theme comes with a small preview.
//
// A directory is a GTK theme if it has a gtk-3.0 or gtk-2.0 folder (many real
// themes ship with no index.theme, so that file cannot be relied on). It is an
// icon or cursor theme if it has an index.theme; the cursor ones also have a
// cursors folder, the freedesktop Xcursor convention.

use super::gtk_colors;
use super::previews;
use super::xfconf;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SYSTEM_GTK_DIR: &str = "/usr/share/themes";
pub const SYSTEM_ICONS_DIR: &str = "/usr/share/icons";

// Folders that always exist as the base of the others; they are never listed.
const SKIP_GTK: &[&str] = &["default", "default-hdpi", "default-xhdpi", "emacs"];
const SKIP_ICONS: &[&str] = &["hicolor", "default", "locolor"];

#[derive(Serialize)]
pub struct ThemeEntry {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub system: bool,
    /// A GTK theme that also brings colors for the window borders.
    #[serde(rename = "hasXfwm4")]
    pub has_xfwm4: bool,
    /// A small picture as a data address, when one could be made.
    pub preview: Option<String>,
    /// The window, contents, accent and text colors of a GTK theme, when its
    /// stylesheets say them; a card is drawn with them.
    pub swatches: Vec<String>,
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

pub fn user_gtk_dir() -> PathBuf {
    home().join(".themes")
}

pub fn user_icons_dir() -> PathBuf {
    home().join(".icons")
}

/// The Name= line of an index.theme: the first one, as the 2.x program did.
pub fn read_theme_name(index: &Path) -> Option<String> {
    let text = fs::read_to_string(index).ok()?;
    text.lines().find_map(|l| l.strip_prefix("Name=").map(|v| v.trim().to_string()))
}

/// Whether an index.theme describes an icon or cursor theme. Its section is
/// [Icon Theme]; the index.theme of a GTK metatheme (such as Agata's) is a
/// desktop entry and says nothing about icons.
pub fn is_icon_theme_index(index: &Path) -> bool {
    fs::read_to_string(index).is_ok_and(|text| text.lines().any(|l| l.trim() == "[Icon Theme]"))
}

fn is_gtk_theme_dir(dir: &Path) -> bool {
    dir.join("gtk-3.0").is_dir() || dir.join("gtk-2.0").is_dir()
}

fn is_cursor_theme_dir(dir: &Path) -> bool {
    dir.join("cursors").is_dir()
}

/// The kinds the pages ask for: "gtk", "icons" and "cursors".
fn bases(kind: &str) -> Option<[(PathBuf, bool); 2]> {
    match kind {
        "gtk" => Some([(PathBuf::from(SYSTEM_GTK_DIR), true), (user_gtk_dir(), false)]),
        "icons" | "cursors" => Some([(PathBuf::from(SYSTEM_ICONS_DIR), true), (user_icons_dir(), false)]),
        _ => None,
    }
}

fn collect(kind: &str, with_previews: bool) -> Vec<ThemeEntry> {
    let Some(bases) = bases(kind) else { return Vec::new() };
    let skip = if kind == "gtk" { SKIP_GTK } else { SKIP_ICONS };
    let mut seen: HashMap<String, ThemeEntry> = HashMap::new();

    for (base, system) in bases {
        let Ok(read) = fs::read_dir(&base) else { continue };
        let mut dirs: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        dirs.sort();
        for dir in dirs {
            let Some(name) = dir.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
            if seen.contains_key(&name) || skip.contains(&name.to_lowercase().as_str()) || !dir.is_dir() {
                continue;
            }
            let wanted = match kind {
                "gtk" => is_gtk_theme_dir(&dir),
                "cursors" => is_icon_theme_index(&dir.join("index.theme")) && is_cursor_theme_dir(&dir),
                _ => is_icon_theme_index(&dir.join("index.theme")) && !is_cursor_theme_dir(&dir),
            };
            if !wanted {
                continue;
            }
            let display_name = read_theme_name(&dir.join("index.theme")).unwrap_or_else(|| name.clone());
            let preview = if with_previews {
                match kind {
                    "icons" => previews::icon_theme_preview(&name),
                    "cursors" => previews::cursor_theme_preview(&dir),
                    _ => None,
                }
            } else {
                None
            };
            // A GTK theme is drawn with its colors; the picture it brings, when it
            // brings one, is for the ones whose colors cannot be read.
            let swatches = if kind == "gtk" { gtk_colors::swatches(&dir, &name) } else { Vec::new() };
            let preview = if kind == "gtk" && swatches.is_empty() {
                ["thumbnail.png", "gtk-3.0/thumbnail.png", "gtk-4.0/thumbnail.png", "preview.png"]
                    .iter()
                    .map(|f| dir.join(f))
                    .find(|p| p.is_file())
                    .and_then(|p| previews::file_data_url(&p, 600 * 1024))
            } else {
                preview
            };
            seen.insert(
                name.clone(),
                ThemeEntry { name, display_name, system, has_xfwm4: dir.join("xfwm4").is_dir(), preview, swatches },
            );
        }
    }

    let mut list: Vec<ThemeEntry> = seen.into_values().collect();
    list.sort_by_key(|t| t.display_name.to_lowercase());
    list
}

/// The installed themes of a kind, by display name. The pictures of icon and
/// cursor themes are asked for one theme at a time (`theme_samples`), so the
/// list shows at once.
#[tauri::command(async)]
pub fn list_themes(kind: String) -> Vec<ThemeEntry> {
    collect(&kind, false)
}

/// The folder of an installed theme, the system one first, as the list has it.
fn theme_dir(kind: &str, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return None;
    }
    bases(kind)?.into_iter().map(|(base, _)| base.join(name)).find(|dir| dir.is_dir())
}

/// The pictures that say what an icon theme or a cursor theme looks like, as
/// data addresses: six icons, or four cursors.
#[tauri::command(async)]
pub fn theme_samples(kind: String, name: String) -> Vec<String> {
    match kind.as_str() {
        "icons" => previews::icon_theme_samples(&name),
        "cursors" => theme_dir(&kind, &name).map(|dir| previews::cursor_theme_samples(&dir)).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The theme that is active now, as Xfce has it.
#[tauri::command(async)]
pub fn get_active_theme(kind: String) -> Option<String> {
    match kind.as_str() {
        "gtk" => xfconf::get("xsettings", "/Net/ThemeName"),
        "icons" => xfconf::get("xsettings", "/Net/IconThemeName"),
        "cursors" => xfconf::get("xsettings", "/Gtk/CursorThemeName"),
        _ => None,
    }
}

/// Activates an installed theme. A GTK theme that brings window borders sets
/// them too, so the two always match.
#[tauri::command(async)]
pub fn set_active_theme(kind: String, name: String) -> Result<(), String> {
    // Only a theme that is installed can be activated.
    let installed = collect(&kind, false);
    if !installed.iter().any(|t| t.name == name) {
        return Err("The theme is not installed".to_string());
    }
    let ok = match kind.as_str() {
        "gtk" => {
            let ok = xfconf::set_string("xsettings", "/Net/ThemeName", &name);
            if ok && installed.iter().any(|t| t.name == name && t.has_xfwm4) {
                xfconf::set_string("xfwm4", "/general/theme", &name);
            }
            ok
        }
        "icons" => xfconf::set_string("xsettings", "/Net/IconThemeName", &name),
        "cursors" => xfconf::set_string("xsettings", "/Gtk/CursorThemeName", &name),
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err("Xfce did not accept the theme".to_string())
    }
}
/// The package of Debian that a path belongs to, if any. A theme that a package
/// installed is removed with the package, never by hand.
pub fn owner_package(path: &Path) -> Option<String> {
    let out = Command::new("dpkg-query").arg("-S").arg(path).output().ok()?;
    if !out.status.success() {
        return None;
    }
    // "package[:arch]: /path"
    let line = String::from_utf8_lossy(&out.stdout).lines().next()?.to_string();
    let package = line.split(": ").next()?.split(':').next()?.trim().to_string();
    (!package.is_empty()).then_some(package)
}

fn folder_of(kind: &str, name: &str, system: bool) -> Option<PathBuf> {
    let [(system_base, _), (user_base, _)] = bases(kind)?;
    Some(if system { system_base } else { user_base }.join(name))
}

/// The package that brought an installed theme, if it came with one.
#[tauri::command(async)]
pub fn theme_package(kind: String, name: String) -> Option<String> {
    let entry = collect(&kind, false).into_iter().find(|t| t.name == name)?;
    entry.system.then(|| folder_of(&kind, &name, true)).flatten().and_then(|dir| owner_package(&dir))
}

/// Removes an installed theme. The one that is active is never removed, and a
/// system theme that a package installed is refused. A theme of the user goes
/// at once; one for every user needs the administrator password.
#[tauri::command(async)]
pub fn delete_theme(kind: String, name: String) -> Result<(), String> {
    let entry = collect(&kind, false).into_iter().find(|t| t.name == name).ok_or("The theme is not installed")?;
    if get_active_theme(kind.clone()).as_deref() == Some(name.as_str()) {
        return Err("The theme is active".to_string());
    }
    let dir = folder_of(&kind, &name, entry.system).ok_or("Unknown kind of theme")?;

    if !entry.system {
        // A link to a folder is removed as a link, never followed.
        return match fs::symlink_metadata(&dir) {
            Ok(m) if m.file_type().is_symlink() => fs::remove_file(&dir).map_err(|e| e.to_string()),
            Ok(_) => fs::remove_dir_all(&dir).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
    }
    if let Some(package) = owner_package(&dir) {
        return Err(format!("package:{package}"));
    }
    // Cursors and icons share their folders.
    let folder_kind = if kind == "gtk" { "gtk" } else { "icons" };
    super::privileged::run_as_root(&[super::privileged::Op::RemoveTheme { kind: folder_kind.to_string(), name }])
}

#[cfg(test)]
mod tests {
    use super::*;

    // Runs against the themes installed on the computer that runs the tests.
    #[test]
    fn the_installed_themes_are_listed_with_previews() {
        for kind in ["gtk", "icons", "cursors"] {
            let list = collect(kind, true);
            let with_preview = list.iter().filter(|t| t.preview.is_some()).count();
            println!("{kind}: {} themes, {with_preview} with a preview; first: {:?}", list.len(), list.first().map(|t| t.display_name.clone()));
            assert!(list.windows(2).all(|w| w[0].display_name.to_lowercase() <= w[1].display_name.to_lowercase()));
        }
        assert!(collect("icons", true).iter().any(|t| t.preview.is_some()), "no icon theme got a preview");
    }

    // Runs against this computer, and only ever removes a theme the test made.
    #[test]
    fn a_theme_of_a_package_is_refused_and_one_of_the_user_is_removed() {
        if let Some(package) = owner_package(Path::new("/usr/share/icons/Adwaita")) {
            assert_eq!(delete_theme("cursors".to_string(), "Adwaita".to_string()), Err(format!("package:{package}")));
        }
        assert_eq!(owner_package(Path::new("/nonexistent/folder")), None);

        let made_icons_dir = !user_icons_dir().exists();
        let name = format!("zz-soplos-test-{}", std::process::id());
        let dir = user_icons_dir().join(&name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.theme"), "[Icon Theme]\nName=Test\n").unwrap();
        assert!(collect("icons", false).iter().any(|t| t.name == name));
        assert_eq!(delete_theme("icons".to_string(), name.clone()), Ok(()));
        assert!(!dir.exists());
        assert_eq!(delete_theme("icons".to_string(), name), Err("The theme is not installed".to_string()));
        if made_icons_dir {
            let _ = fs::remove_dir(user_icons_dir());
        }
    }
}
