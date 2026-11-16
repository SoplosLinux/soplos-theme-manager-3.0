// Sharing a theme as a .sth file (a zip) and installing one.
//
//   <name>/metadata.json   format "soplos-theme-bundle-v3"
//   <name>/theme.conf
//   <name>/preview/*.png
//   <name>/assets/panel/   xfce4-panel.xml and plugins/
//   <name>/assets/gtk.tar, icons.tar, cursor.tar   the themes it uses
//   <name>/assets/wallpaper/<file>
//
// A .sth file comes from anyone, so it is treated like any package: unpacked
// into the cache with the same limits, never run, and the themes in it are
// copied with the same rules as any other theme (links only inside it).

use super::bundles::{themes_root, valid_name, Conf, FORMAT};
use super::packages::{cache_root, copy_tree, unpack_tar, unpack_zip};
use super::privileged::{self, Op};
use super::themes::{home, user_gtk_dir, user_icons_dir, SYSTEM_GTK_DIR, SYSTEM_ICONS_DIR};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// Families that every system has; they are not carried in a theme file.
const STANDARD_ICONS: &[&str] = &["hicolor", "locolor", "gnome", "highcontrast", "adwaita"];

fn find_dir(name: &str, bases: &[PathBuf]) -> Option<PathBuf> {
    bases.iter().map(|b| b.join(name)).find(|p| p.is_dir())
}

fn gtk_bases() -> Vec<PathBuf> {
    vec![user_gtk_dir(), PathBuf::from(SYSTEM_GTK_DIR), PathBuf::from("/usr/local/share/themes")]
}

fn icon_bases() -> Vec<PathBuf> {
    vec![user_icons_dir(), home().join(".local/share/icons"), PathBuf::from(SYSTEM_ICONS_DIR), PathBuf::from("/usr/local/share/icons")]
}

// The theme and the ones it inherits from, minus the ones every system has.
fn icon_family(name: &str, out: &mut Vec<String>, depth: usize) {
    if depth > 8 || out.iter().any(|n| n == name) || STANDARD_ICONS.contains(&name.to_lowercase().as_str()) || !valid_name(name) {
        return;
    }
    let Some(dir) = find_dir(name, &icon_bases()) else { return };
    out.push(name.to_string());
    let index = fs::read_to_string(dir.join("index.theme")).unwrap_or_default();
    if let Some(line) = index.lines().find_map(|l| l.strip_prefix("Inherits=")) {
        for parent in line.split(',') {
            icon_family(parent.trim(), out, depth + 1);
        }
    }
}

fn write_tar(dest: &Path, folders: &[(String, PathBuf)]) -> Result<(), String> {
    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut builder = tar::Builder::new(file);
    builder.follow_symlinks(false);
    for (name, path) in folders {
        append_readable(&mut builder, path, Path::new(name), 0)?;
    }
    builder.finish().map_err(|e| e.to_string())
}

// Adds a folder to a tar. A file that the user cannot read (a backup left by
// root inside a system theme) is left out instead of failing the whole theme.
fn append_readable(builder: &mut tar::Builder<fs::File>, path: &Path, name: &Path, depth: usize) -> Result<(), String> {
    if depth > 64 {
        return Ok(());
    }
    builder.append_dir(name, path).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(path).map_err(|e| e.to_string())?.flatten() {
        let inner = name.join(entry.file_name());
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            append_readable(builder, &entry.path(), &inner, depth + 1)?;
        } else if ty.is_file() {
            if let Ok(mut file) = fs::File::open(entry.path()) {
                builder.append_file(&inner, &mut file).map_err(|e| e.to_string())?;
            }
        } else if ty.is_symlink() {
            let target = fs::read_link(entry.path()).map_err(|e| e.to_string())?;
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            builder.append_link(&mut header, &inner, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn add_file<W: Write + std::io::Seek>(zip: &mut zip::ZipWriter<W>, name: &str, path: &Path) -> Result<(), String> {
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).compression_level(Some(1));
    zip.start_file(name, options).map_err(|e| e.to_string())?;
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    std::io::copy(&mut file, zip).map_err(|e| e.to_string())?;
    Ok(())
}

fn add_tree<W: Write + std::io::Seek>(zip: &mut zip::ZipWriter<W>, prefix: &str, dir: &Path, depth: usize) -> Result<(), String> {
    if depth > 8 {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            add_tree(zip, &name, &entry.path(), depth + 1)?;
        } else if ty.is_file() {
            add_file(zip, &name, &entry.path())?;
        }
    }
    Ok(())
}

/// Writes the theme and what it uses (GTK theme, icons, cursor, wallpaper) to a
/// .sth file. Everything it needs is read; nothing needs the administrator.
#[tauri::command(async)]
pub fn export_saved_theme(name: String, dest: String) -> Result<(), String> {
    if !valid_name(&name) {
        return Err("Not a valid name".to_string());
    }
    let dir = themes_root().join(&name);
    if !dir.join("theme.conf").is_file() {
        return Err("It is not a saved theme".to_string());
    }
    let work = cache_root().join(format!("export-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = build_bundle(&name, &dir, &work, Path::new(&dest));
    let _ = fs::remove_dir_all(&work);
    result
}

fn build_bundle(name: &str, dir: &Path, work: &Path, dest: &Path) -> Result<(), String> {
    let mut conf = Conf::read(dir);

    let mut icons: Vec<String> = Vec::new();
    if let Some(icon) = conf.get("icon_theme") {
        icon_family(icon, &mut icons, 0);
    }
    let mut cursors: Vec<String> = Vec::new();
    if let Some(cursor) = conf.get("cursor_theme") {
        icon_family(cursor, &mut cursors, 0);
        cursors.retain(|c| !icons.contains(c));
    }
    let named = |names: &[String]| -> Vec<(String, PathBuf)> { names.iter().filter_map(|n| find_dir(n, &icon_bases()).map(|p| (n.clone(), p))).collect() };

    let gtk_tar = work.join("gtk.tar");
    let icons_tar = work.join("icons.tar");
    let cursor_tar = work.join("cursor.tar");
    let mut have = (false, false, false);
    if let Some(gtk) = conf.get("gtk_theme").filter(|g| valid_name(g)) {
        if let Some(path) = find_dir(gtk, &gtk_bases()) {
            write_tar(&gtk_tar, &[(gtk.to_string(), path)])?;
            have.0 = true;
        }
    }
    if !icons.is_empty() {
        write_tar(&icons_tar, &named(&icons))?;
        have.1 = true;
    }
    if !cursors.is_empty() {
        write_tar(&cursor_tar, &named(&cursors))?;
        have.2 = true;
    }

    // The wallpaper travels as a file name; where it came from is kept so the
    // same picture is used on a machine that has it.
    let wallpaper = conf.get("wallpaper").map(PathBuf::from).filter(|p| p.is_file());
    if let Some(path) = &wallpaper {
        conf.set("wallpaper_source_path", &path.to_string_lossy());
        conf.set("wallpaper", &path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    }

    let tmp = dest.with_extension("sth.part");
    let file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).compression_level(Some(1));
    let write = (|| -> Result<(), String> {
        zip.start_file(format!("{name}/theme.conf"), options).map_err(|e| e.to_string())?;
        zip.write_all(conf.text().as_bytes()).map_err(|e| e.to_string())?;
        // The metadata of the theme, with the format that marks it as ours.
        let mut meta: serde_json::Value = fs::read_to_string(dir.join("metadata.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
        meta["name"] = name.into();
        meta["format"] = FORMAT.into();
        zip.start_file(format!("{name}/metadata.json"), options).map_err(|e| e.to_string())?;
        zip.write_all(serde_json::to_string_pretty(&meta).unwrap_or_default().as_bytes()).map_err(|e| e.to_string())?;
        if dir.join("preview").is_dir() {
            add_tree(&mut zip, &format!("{name}/preview"), &dir.join("preview"), 0)?;
        }
        if dir.join("assets/panel").is_dir() {
            add_tree(&mut zip, &format!("{name}/assets/panel"), &dir.join("assets/panel"), 0)?;
        }
        for (present, file, tar) in [(have.0, "gtk.tar", &gtk_tar), (have.1, "icons.tar", &icons_tar), (have.2, "cursor.tar", &cursor_tar)] {
            if present {
                add_file(&mut zip, &format!("{name}/assets/{file}"), tar)?;
            }
        }
        if let Some(path) = &wallpaper {
            add_file(&mut zip, &format!("{name}/assets/wallpaper/{}", path.file_name().unwrap_or_default().to_string_lossy()), path)?;
        }
        Ok(())
    })();
    let finished = write.and_then(|_| zip.finish().map(|_| ()).map_err(|e| e.to_string()));
    match finished {
        Ok(()) => fs::rename(&tmp, dest).map_err(|e| e.to_string()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

// ── Installing ───────────────────────────────────────────────────────────────

/// The name inside a .sth file, when it is one of ours. Nothing is unpacked.
pub fn bundle_name(path: &Path) -> Result<String, String> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?).map_err(|_| "invalid".to_string())?;
    let mut found = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|_| "invalid".to_string())?;
        let parts: Vec<&str> = entry.name().split('/').collect();
        if parts.len() == 2 && parts[1] == "metadata.json" {
            found = Some(entry.name().to_string());
            break;
        }
    }
    let entry_name = found.ok_or("invalid")?;
    let mut text = String::new();
    archive.by_name(&entry_name).map_err(|_| "invalid".to_string())?.take(64 * 1024).read_to_string(&mut text).map_err(|_| "invalid".to_string())?;
    let meta: serde_json::Value = serde_json::from_str(&text).map_err(|_| "invalid".to_string())?;
    let name = meta.get("name").and_then(|n| n.as_str()).unwrap_or("");
    if meta.get("format").and_then(|f| f.as_str()) != Some(FORMAT) || !valid_name(name) {
        return Err("invalid".to_string());
    }
    Ok(name.to_string())
}

#[tauri::command(async)]
pub fn bundle_info(path: String) -> Result<String, String> {
    bundle_name(Path::new(&path))
}

fn stamp() -> String {
    format!("{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0))
}

// Unpacks a tar of the bundle, plain or compressed, into a folder.
fn unpack_inner_tar(tar: &Path, dest: &Path) -> Result<(), String> {
    let mut magic = [0u8; 2];
    fs::File::open(tar).and_then(|mut f| f.read_exact(&mut magic)).map_err(|e| e.to_string())?;
    let file = fs::File::open(tar).map_err(|e| e.to_string())?;
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    if magic == [0x1f, 0x8b] {
        unpack_tar(flate2::read::GzDecoder::new(file), dest)
    } else {
        unpack_tar(file, dest)
    }
}

// The folders at the top of an unpacked tar (the themes), as real folders.
fn top_folders(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
    read.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| (e.file_name().to_string_lossy().to_string(), e.path())).filter(|(n, _)| valid_name(n)).collect()
}

/// Installs a .sth file. `scope` is "user" (their own folders) or "global"
/// (every user, one password). The themes that already exist are left alone.
#[tauri::command(async)]
pub fn install_bundle(path: String, scope: String) -> Result<String, String> {
    install_bundle_at(Path::new(&path), &scope, &themes_root())
}

pub fn install_bundle_at(path: &Path, scope: &str, store: &Path) -> Result<String, String> {
    if scope != "user" && scope != "global" {
        return Err("Unknown scope".to_string());
    }
    let name = bundle_name(path)?;
    let stage = cache_root().join(format!("bundle-{}", stamp()));
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let result = install_from(path, &name, scope, &stage, store);
    let _ = fs::remove_dir_all(&stage);
    result.map(|_| name)
}

fn install_from(path: &Path, name: &str, scope: &str, stage: &Path, store: &Path) -> Result<(), String> {
    let unpacked = stage.join("zip");
    fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
    unpack_zip(fs::File::open(path).map_err(|e| e.to_string())?, &unpacked)?;
    // The folder of the bundle is the one that holds metadata.json.
    let root = fs::read_dir(&unpacked)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.join("metadata.json").is_file())
        .ok_or("invalid")?;
    let mut conf = Conf::read(&root);
    if conf.0.is_empty() {
        return Err("invalid".to_string());
    }

    let x = stage.join("x");
    let mut ops: Vec<Op> = Vec::new();
    let mut installs: Vec<(String, String, PathBuf)> = Vec::new(); // kind, name, folder
    for (tar, sub, kind) in [("gtk.tar", "gtk", "gtk"), ("icons.tar", "icons", "icons"), ("cursor.tar", "cursor", "icons")] {
        let file = root.join("assets").join(tar);
        if fs::symlink_metadata(&file).is_ok_and(|m| m.is_file()) {
            unpack_inner_tar(&file, &x.join(sub))?;
            for (n, p) in top_folders(&x.join(sub)) {
                installs.push((kind.to_string(), n, p));
            }
        }
    }
    for (kind, n, folder) in &installs {
        let (system, user) = if kind == "gtk" { (Path::new(SYSTEM_GTK_DIR), user_gtk_dir()) } else { (Path::new(SYSTEM_ICONS_DIR), user_icons_dir()) };
        if system.join(n).exists() {
            continue;
        }
        if scope == "global" {
            ops.push(Op::InstallTheme { kind: kind.clone(), src: folder.to_string_lossy().to_string(), name: n.clone() });
        } else if !user.join(n).exists() {
            fs::create_dir_all(&user).map_err(|e| e.to_string())?;
            copy_tree(folder, &user.join(n), false, 0).map_err(|e| {
                let _ = fs::remove_dir_all(user.join(n));
                e.to_string()
            })?;
        }
    }

    // The wallpaper: the original when this machine has it, else the one in the file.
    let mut wallpaper: Option<String> = None;
    if let Some(source) = conf.get("wallpaper_source_path").filter(|p| Path::new(p).is_file()) {
        wallpaper = Some(source.to_string());
    } else if let Some(file) = conf.get("wallpaper") {
        let file_name = Path::new(file).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let bundled = root.join("assets").join("wallpaper").join(&file_name);
        if valid_name(&file_name) && fs::symlink_metadata(&bundled).is_ok_and(|m| m.is_file()) {
            if scope == "global" {
                let dir = stage.join("wp");
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                fs::copy(&bundled, dir.join(&file_name)).map_err(|e| e.to_string())?;
                ops.push(Op::InstallWallpapers { dir: dir.to_string_lossy().to_string(), sub: Some(format!("soplos-themes/{name}")) });
                wallpaper = Some(format!("/usr/share/backgrounds/soplos-themes/{name}/{file_name}"));
            } else {
                let dir = home().join(".local/share/backgrounds/soplos-themes").join(name);
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let dest = dir.join(&file_name);
                if !dest.exists() {
                    fs::copy(&bundled, &dest).map_err(|e| e.to_string())?;
                }
                wallpaper = Some(dest.to_string_lossy().to_string());
            }
        } else if Path::new(file).is_file() {
            wallpaper = Some(file.to_string());
        }
    }

    if !ops.is_empty() {
        privileged::run_as_root(&ops)?;
    }

    // The entry of the theme in the list, replacing one of the same name.
    conf.remove("wallpaper_source_path");
    match wallpaper {
        Some(w) => conf.set("wallpaper", &w),
        None => conf.remove("wallpaper"),
    }
    let entry = stage.join("entry");
    fs::create_dir_all(&entry).map_err(|e| e.to_string())?;
    fs::write(entry.join("theme.conf"), conf.text()).map_err(|e| e.to_string())?;
    fs::copy(root.join("metadata.json"), entry.join("metadata.json")).map_err(|e| e.to_string())?;
    for sub in ["preview", "assets/panel"] {
        let from = root.join(sub);
        if from.is_dir() {
            copy_plain(&from, &entry.join(sub), 0).map_err(|e| e.to_string())?;
        }
    }
    fs::create_dir_all(store).map_err(|e| e.to_string())?;
    let target = store.join(name);
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    }
    // The cache and the store may be on different disks.
    if fs::rename(&entry, &target).is_err() {
        copy_plain(&entry, &target, 0).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// Plain files and folders only; the links of a file we do not trust are dropped.
fn copy_plain(from: &Path, to: &Path, depth: usize) -> std::io::Result<()> {
    if depth > 16 {
        return Ok(());
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)?.flatten() {
        let ty = entry.file_type()?;
        let dest = to.join(entry.file_name());
        if ty.is_dir() {
            copy_plain(&entry.path(), &dest, depth + 1)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

// ── The themes that come with the program ────────────────────────────────────

fn base_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/usr/share/soplos-theme-manager/base-themes")];
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join("base-themes");
    if dev.is_dir() {
        dirs.push(dev);
    }
    dirs
}

fn base_files() -> Vec<PathBuf> {
    let Some(dir) = base_dirs().into_iter().find(|d| d.is_dir()) else { return Vec::new() };
    let mut files: Vec<PathBuf> = fs::read_dir(dir).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "sth")).collect()).unwrap_or_default();
    files.sort();
    files
}

/// Installs the themes that come with the program and are missing from the
/// list. Returns how many were added; the error "none" means the program has
/// none to give.
#[tauri::command(async)]
pub fn restore_base_themes() -> Result<usize, String> {
    let files = base_files();
    if files.is_empty() {
        return Err("none".to_string());
    }
    let mut added = 0;
    for file in files {
        let Ok(name) = bundle_name(&file) else { continue };
        if themes_root().join(&name).join("theme.conf").is_file() {
            continue;
        }
        if install_bundle_at(&file, "user", &themes_root()).is_ok() {
            added += 1;
        }
    }
    Ok(added)
}

/// On the first run, when there is no list of themes yet, fills it with the
/// themes that come with the program.
#[tauri::command(async)]
pub fn seed_first_run() {
    if !themes_root().exists() {
        let _ = restore_base_themes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_of_the_program_is_read_and_installed_into_a_scratch_store() {
        let files = base_files();
        let Some(file) = files.iter().find(|f| f.to_string_lossy().contains("soplos-unity")) else { return };
        assert_eq!(bundle_name(file).unwrap(), "soplos-unity");
        let store = cache_root().join(format!("test-store-{}", stamp()));
        // The pictures and themes go to the user's own folders, which are
        // skipped when they already exist, so only the entry is checked.
        let name = install_bundle_at(file, "user", &store).unwrap();
        let conf = Conf::read(&store.join(&name));
        assert!(conf.get("gtk_theme").is_some());
        assert!(conf.get("wallpaper_source_path").is_none());
        assert!(store.join(&name).join("preview/preview.png").is_file());
        assert!(store.join(&name).join("assets/panel/xfce4-panel.xml").is_file());
        let _ = fs::remove_dir_all(store);
    }

    // Installs every theme that comes with the program. It writes into the home
    // folder, so it only runs when asked for, with HOME pointing to a scratch folder.
    #[test]
    #[ignore]
    fn every_base_theme_installs() {
        for file in base_files() {
            let store = themes_root();
            let name = install_bundle_at(&file, "user", &store).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            let conf = Conf::read(&store.join(&name));
            assert!(conf.get("gtk_theme").is_some(), "{name}");
            let wallpaper = conf.get("wallpaper").unwrap_or_else(|| panic!("{name}: no wallpaper"));
            assert!(Path::new(wallpaper).is_file(), "{name}: {wallpaper}");
        }
    }

    // A file given in SOPLOS_TEST_STH, installed as the user of another computer would.
    #[test]
    #[ignore]
    fn a_given_file_installs_with_its_own_wallpaper() {
        let file = PathBuf::from(std::env::var("SOPLOS_TEST_STH").unwrap());
        let name = install_bundle_at(&file, "user", &themes_root()).unwrap();
        let conf = Conf::read(&themes_root().join(&name));
        let wallpaper = conf.get("wallpaper").unwrap();
        assert!(wallpaper.contains("/.local/share/backgrounds/soplos-themes/"), "{wallpaper}");
        assert!(Path::new(wallpaper).is_file(), "{wallpaper}");
    }

    #[test]
    fn a_theme_of_this_computer_is_exported_and_read_back() {
        if !themes_root().join("soplos-unity/theme.conf").is_file() {
            return;
        }
        let dir = cache_root().join(format!("test-export-{}", stamp()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("out.sth");
        export_saved_theme("soplos-unity".to_string(), file.to_string_lossy().to_string()).unwrap();
        assert_eq!(bundle_name(&file).unwrap(), "soplos-unity");
        let names: Vec<String> = zip::ZipArchive::new(fs::File::open(&file).unwrap()).unwrap().file_names().map(String::from).collect();
        assert!(names.iter().any(|n| n.ends_with("assets/gtk.tar")));
        assert!(names.iter().any(|n| n.ends_with("assets/icons.tar")));
        assert!(names.iter().any(|n| n.ends_with("preview/preview.png")));
        // And it installs again (the themes exist here, so only the entry is made).
        let store = dir.join("store");
        assert_eq!(install_bundle_at(&file, "user", &store).unwrap(), "soplos-unity");
        assert!(store.join("soplos-unity/assets/panel/xfce4-panel.xml").is_file());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_wallpaper_from_the_home_folder_travels_inside_the_file() {
        let conf = Conf::read(&themes_root().join("soplos-2.0"));
        let Some(wallpaper) = conf.get("wallpaper").filter(|w| w.starts_with("/home/") && Path::new(w).is_file()) else { return };
        let dir = cache_root().join(format!("test-wallpaper-{}", stamp()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("out.sth");
        export_saved_theme("soplos-2.0".to_string(), file.to_string_lossy().to_string()).unwrap();
        let wanted = format!("assets/wallpaper/{}", Path::new(wallpaper).file_name().unwrap().to_string_lossy());
        let mut zip = zip::ZipArchive::new(fs::File::open(&file).unwrap()).unwrap();
        let entry = zip.by_name(&format!("soplos-2.0/{wanted}")).expect("the wallpaper is not in the file");
        assert_eq!(entry.size(), fs::metadata(wallpaper).unwrap().len());
        drop(entry);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_zip_that_is_not_a_theme_is_refused() {
        let dir = cache_root().join(format!("test-bad-{}", stamp()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x.sth");
        let mut zip = zip::ZipWriter::new(fs::File::create(&file).unwrap());
        zip.start_file("a/metadata.json", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(br#"{"name":"a","format":"other"}"#).unwrap();
        zip.finish().unwrap();
        assert!(bundle_name(&file).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
