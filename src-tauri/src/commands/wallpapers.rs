// The wallpapers: finding the pictures of a folder, setting one as the
// background of Xfce and installing new ones for every user.
//
// The pictures themselves are never sent to the window: it loads them from
// their files, through the asset protocol, and only the folder that was
// scanned (and the target of any link found in it) is opened to that protocol.

use super::packages::cache_root;
use super::privileged;
use super::themes::home;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

pub const EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "bmp", "svg"];
pub const SYSTEM_WALLPAPERS_DIR: &str = "/usr/share/backgrounds";

const MAX_DEPTH: usize = 32;
// The width of the small copies shown in the cards.
const THUMB_WIDTH: i32 = 400;
// What is staged for the administrator step is capped, like the packages.
const MAX_INSTALL_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Serialize)]
pub struct Wallpaper {
    pub path: String,
    /// The file name without its extension.
    pub name: String,
}

pub fn has_image_extension(path: &Path) -> bool {
    path.extension().is_some_and(|e| EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

/// The folders the program looks in, in this order; the first that exists is
/// the one it opens on.
fn default_folders() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/share/wallpapers"),
        PathBuf::from("/usr/share/backgrounds"),
        PathBuf::from("/usr/share/xfce4/backdrops"),
        home().join("Pictures"),
        home().join("Imágenes"),
    ]
}

/// The folder the page opens on, if any of the usual ones exists.
#[tauri::command(async)]
pub fn default_wallpaper_folder() -> Option<String> {
    default_folders().into_iter().find(|d| d.exists()).map(|d| d.to_string_lossy().to_string())
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            walk(&path, depth + 1, out);
        } else if has_image_extension(&path) && fs::metadata(&path).is_ok_and(|m| m.is_file()) {
            out.push(path);
        }
    }
}

/// The pictures of a folder and of the folders inside it, one entry for each
/// real file even when links lead to it twice. Opens the folder to the window.
#[tauri::command(async)]
pub fn scan_wallpapers(app: tauri::AppHandle, folder: String) -> Result<Vec<Wallpaper>, String> {
    let root = Path::new(&folder).canonicalize().map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("It is not a folder".to_string());
    }
    let scope = app.asset_protocol_scope();
    scope.allow_directory(&root, true).map_err(|e| e.to_string())?;

    let mut files = Vec::new();
    walk(&root, 0, &mut files);

    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut found = Vec::new();
    for path in files {
        let Ok(real) = path.canonicalize() else { continue };
        if !seen.insert(real.clone()) {
            continue;
        }
        // A link can lead out of the folder: that file is opened too.
        if !real.starts_with(&root) {
            let _ = scope.allow_file(&real);
        }
        found.push(Wallpaper {
            name: path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            path: path.to_string_lossy().to_string(),
        });
    }
    Ok(found)
}

// ── Setting the wallpaper ────────────────────────────────────────────────────

// The names of the connected monitors, as xfdesktop names them in its settings
// (`monitorHDMI-1`, `monitoreDP-1`...).
fn monitor_names() -> Vec<String> {
    let Ok(out) = Command::new("xrandr").arg("--listmonitors").output() else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .filter_map(|l| l.split_whitespace().last().map(|n| n.to_string()))
        .collect()
}

fn xfconf_ok(args: &[&str]) -> Result<(), String> {
    let out = Command::new("xfconf-query").args(["-c", "xfce4-desktop"]).args(args).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Sets the picture as the wallpaper of every screen and workspace of Xfce.
/// xfdesktop keeps a property for each monitor and workspace, but only after
/// the wallpaper of that monitor was changed once, so a computer that never
/// did (a real one with its own monitor names) has none to change: they are
/// made here for each monitor that is connected.
#[tauri::command(async)]
pub fn set_wallpaper(path: String) -> Result<(), String> {
    let file = Path::new(&path);
    if !has_image_extension(file) || !fs::metadata(file).is_ok_and(|m| m.is_file()) {
        return Err("It is not a picture".to_string());
    }
    let listing = Command::new("xfconf-query").args(["-c", "xfce4-desktop", "-lv"]).output().map_err(|e| e.to_string())?;
    let listing = String::from_utf8_lossy(&listing.stdout).to_string();

    // The places that get the picture: the ones xfdesktop has, and the ones of
    // the monitors that are connected.
    let mut places: Vec<String> = Vec::new();
    for line in listing.lines() {
        if let Some(property) = line.split_whitespace().next().filter(|p| p.ends_with("/last-image")) {
            places.push(property.trim_end_matches("/last-image").to_string());
        }
    }
    let workspaces = super::xfconf::get("xfwm4", "/general/workspace_count").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1).clamp(1, 32);
    let mut monitors = monitor_names();
    if monitors.is_empty() && places.is_empty() {
        monitors.push("0".to_string());
    }
    for monitor in monitors {
        for workspace in 0..workspaces {
            let place = format!("/backdrop/screen0/monitor{monitor}/workspace{workspace}");
            if !places.contains(&place) {
                places.push(place);
            }
        }
    }

    let mut changed = false;
    let mut last_error = String::new();
    for place in &places {
        match xfconf_ok(&["-p", &format!("{place}/last-image"), "--create", "-t", "string", "-s", &path]) {
            Ok(()) => {
                changed = true;
                // A style of "none" shows no picture at all: use the zoomed one.
                let style = format!("{place}/image-style");
                if super::xfconf::get("xfce4-desktop", &style).map_or(true, |v| v == "0") {
                    let _ = xfconf_ok(&["-p", &style, "--create", "-t", "int", "-s", "5"]);
                }
            }
            Err(e) => last_error = e,
        }
    }
    if !changed {
        return Err(if last_error.is_empty() { "Xfce has no wallpaper to change".to_string() } else { last_error });
    }
    let _ = Command::new("xfdesktop").arg("--reload").output();
    // The greeter shows this one too when it is set to use the wallpaper of each user.
    super::avatar::publish_wallpaper(&path);
    Ok(())
}

// ── Installing ───────────────────────────────────────────────────────────────

/// Installs pictures, for the user alone (`~/.local/share/backgrounds`, no
/// password) or for every user (`/usr/share/backgrounds`, through the
/// administrator step), in a folder of its own if `sub` is given. Returns the
/// folder they were put in.
///
/// For every user they are copied first, by the user, to the cache of the
/// program; the administrator step only copies from there, so a file the user
/// cannot read can never be installed through it.
#[tauri::command(async)]
pub fn install_wallpapers(paths: Vec<String>, everyone: bool, sub: Option<String>) -> Result<String, String> {
    let sub = sub.map(|s| s.trim().trim_matches('/').to_string()).filter(|s| !s.is_empty());
    let mut target = if everyone { PathBuf::from(SYSTEM_WALLPAPERS_DIR) } else { home().join(".local/share/backgrounds") };
    if let Some(sub) = &sub {
        for part in sub.split('/') {
            if part.is_empty() || part == "." || part == ".." || part.contains('\0') || part.len() > 255 {
                return Err(format!("Not a valid folder: {sub}"));
            }
            target.push(part);
        }
    }

    let pictures: Vec<&Path> = paths.iter().map(Path::new).filter(|p| has_image_extension(p) && fs::metadata(p).is_ok_and(|m| m.is_file())).collect();
    if pictures.is_empty() {
        return Err("There is no picture to install".to_string());
    }
    let total: u64 = pictures.iter().filter_map(|p| fs::metadata(p).ok()).map(|m| m.len()).sum();
    if total > MAX_INSTALL_BYTES {
        return Err("The pictures are too big".to_string());
    }

    if !everyone {
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        for src in pictures {
            let name = src.file_name().ok_or("Not a valid file name")?;
            fs::copy(src, target.join(name)).map_err(|e| e.to_string())?;
        }
        return Ok(target.to_string_lossy().to_string());
    }

    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let stage = cache_root().join(format!("wall-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        for src in pictures {
            let name = src.file_name().ok_or("Not a valid file name")?;
            fs::copy(src, stage.join(name)).map_err(|e| e.to_string())?;
        }
        privileged::run_as_root(&[privileged::Op::InstallWallpapers { dir: stage.to_string_lossy().to_string(), sub }])
    })();
    let _ = fs::remove_dir_all(&stage);
    result.map(|_| target.to_string_lossy().to_string())
}

// ── Thumbnails ───────────────────────────────────────────────────────────────

/// A small copy of a wallpaper, made once and kept in the cache, so the window
/// does not decode a picture of several megabytes for every card each time the
/// page is shown. The name of the copy changes when the picture does. Returns
/// the path of the copy, opened to the window; nothing if it cannot be made.
#[tauri::command(async)]
pub fn make_thumbnail(app: tauri::AppHandle, path: String) -> Option<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let source = fs::canonicalize(&path).ok()?;
    if !has_image_extension(&source) {
        return None;
    }
    let meta = fs::metadata(&source).ok()?;
    let mut hasher = DefaultHasher::new();
    // Bumped when the way the copies are made changes, so the old ones are
    // made again: the copies of pictures with transparency were saved grey.
    "thumbnail-2".hash(&mut hasher);
    source.hash(&mut hasher);
    meta.len().hash(&mut hasher);
    meta.modified().ok().hash(&mut hasher);
    let dir = cache_root().join("thumbs");
    let dest = dir.join(format!("{:016x}.jpg", hasher.finish()));
    if !dest.is_file() {
        fs::create_dir_all(&dir).ok()?;
        let mut picture = gdk_pixbuf::Pixbuf::from_file_at_scale(&source, THUMB_WIDTH, -1, true).ok()?;
        // gdk-pixbuf saves a picture with an alpha channel (a PNG with
        // transparency, an SVG) as a grey JPEG. It is put on a flat dark
        // background first, which keeps the colours.
        if picture.has_alpha() {
            let (w, h) = (picture.width(), picture.height());
            let flat = gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, false, 8, w, h)?;
            picture.composite_color(&flat, 0, 0, w, h, 0.0, 0.0, 1.0, 1.0, gdk_pixbuf::InterpType::Bilinear, 255, 0, 0, 16, 0x1a1a1a, 0x1a1a1a);
            picture = flat;
        }
        // Written under another name first: two requests for one picture never
        // leave a half written file.
        let part = dir.join(format!("{:016x}.{}.part", hasher.finish(), std::process::id()));
        picture.savev(&part, "jpeg", &[("quality", "85")]).ok()?;
        fs::rename(&part, &dest).ok()?;
    }
    app.asset_protocol_scope().allow_directory(&dir, false).ok()?;
    Some(dest.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn only_pictures_are_taken_and_a_linked_file_counts_once() {
        let dir = cache_root().join(format!("test-wall-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub/deep")).unwrap();
        for f in ["a.JPG", "b.png", "sub/c.webp", "sub/deep/d.svg", "notes.txt", "sub/e.bmp"] {
            fs::write(dir.join(f), "x").unwrap();
        }
        symlink("../a.JPG", dir.join("sub/link-to-a.jpg")).unwrap();
        symlink("nowhere.png", dir.join("broken.png")).unwrap();

        let mut files = Vec::new();
        walk(&dir, 0, &mut files);
        let names: Vec<String> = files.iter().map(|p| p.strip_prefix(&dir).unwrap().to_string_lossy().to_string()).collect();
        assert!(names.contains(&"a.JPG".to_string()));
        assert!(names.contains(&"sub/deep/d.svg".to_string()));
        assert!(!names.iter().any(|n| n.ends_with("notes.txt")));
        assert!(!names.iter().any(|n| n.contains("broken")), "a broken link is not a picture");

        // The link and its target are one real file.
        let real: HashSet<PathBuf> = files.iter().filter_map(|p| p.canonicalize().ok()).collect();
        assert_eq!(real.len(), files.len() - 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_extension_check_ignores_case() {
        assert!(has_image_extension(Path::new("/x/y.PNG")));
        assert!(has_image_extension(Path::new("/x/y.WebP")));
        assert!(!has_image_extension(Path::new("/x/y.gif")));
        assert!(!has_image_extension(Path::new("/x/png")));
    }
}

#[cfg(test)]
mod thumbnail_tests {
    use super::*;

    #[test]
    fn a_big_picture_becomes_a_small_copy() {
        let Some(big) = fs::read_dir(SYSTEM_WALLPAPERS_DIR).ok().and_then(|d| d.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "jpg"))) else { return };
        let picture = gdk_pixbuf::Pixbuf::from_file_at_scale(&big, THUMB_WIDTH, -1, true).unwrap();
        assert!(picture.width() <= THUMB_WIDTH);
        let out = cache_root().join(format!("test-thumb-{}.jpg", std::process::id()));
        fs::create_dir_all(cache_root()).unwrap();
        picture.savev(&out, "jpeg", &[("quality", "85")]).unwrap();
        assert!(fs::metadata(&out).unwrap().len() < fs::metadata(&big).unwrap().len());
        let _ = fs::remove_file(&out);
    }
}
