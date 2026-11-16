// Installing a GTK, icon or cursor theme from an archive (.tar.gz, .tgz,
// .tar.xz, .tar.bz2, .tar or .zip).
//
// The archive is unpacked into a folder of the cache of the program, never
// into /tmp (a memory disk that can be full), the themes that it holds are
// listed and the user chooses which to install and for whom. A package that
// needs a script to build its theme has no ready theme in it and yields no
// candidate: the scripts of a downloaded package are never run.

use super::privileged;
use super::themes::{self, is_icon_theme_index, read_theme_name, SYSTEM_GTK_DIR, SYSTEM_ICONS_DIR};
use serde::Serialize;
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// A package that unpacks to more than this is refused, so a hostile archive
// cannot fill the disk.
const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

#[derive(Serialize)]
pub struct Candidate {
    /// The place of the theme inside the unpacked package.
    pub rel: String,
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    /// "gtk", "icon" or "cursor".
    pub kind: String,
    #[serde(rename = "hasXfwm4")]
    pub has_xfwm4: bool,
}

#[derive(Serialize)]
pub struct Inspected {
    /// Names the unpacked package for the next calls.
    pub token: String,
    pub candidates: Vec<Candidate>,
    /// When the package has no theme of the kind asked for: what it holds
    /// instead, as "gtk", "icons", "cursors" or "windows" (cursors for Windows,
    /// which Xfce cannot use). It lets the window say where the package belongs.
    #[serde(rename = "otherKinds")]
    pub other_kinds: Vec<String>,
}

pub fn cache_root() -> PathBuf {
    themes::home().join(".cache").join("soplos-theme-manager")
}

fn package_dir(token: &str) -> Result<PathBuf, String> {
    // The window only ever gets back what this module made: pkg-<pid>-<nanos>.
    let valid = token.strip_prefix("pkg-").is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '-'));
    if !valid {
        return Err("Unknown package".to_string());
    }
    Ok(cache_root().join(token))
}

// ── Unpacking ────────────────────────────────────────────────────────────────

fn unsafe_path(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
}

// Whether a link stays inside the theme: it has to be relative and never climb
// out of the folder it is unpacked into.
pub fn link_stays_inside(target: &Path, depth_of_link_folder: usize) -> bool {
    let mut depth = depth_of_link_folder as i64;
    for component in target.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            Component::CurDir => {}
            _ => return false,
        }
    }
    true
}

pub(super) fn unpack_tar<R: Read>(reader: R, dest: &Path) -> Result<(), String> {
    let mut archive = tar::Archive::new(reader);
    archive.set_preserve_permissions(false);
    archive.set_unpack_xattrs(false);
    archive.set_overwrite(false);

    let mut total: u64 = 0;
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        if unsafe_path(&path) {
            return Err(format!("Unsafe path in archive: {}", path.display()));
        }
        total += entry.size();
        if total > MAX_UNPACKED_BYTES {
            return Err("The package is too big".to_string());
        }
        let kind = entry.header().entry_type();
        if kind.is_dir() || kind.is_file() {
            entry.unpack_in(dest).map_err(|e| e.to_string())?;
        } else if kind.is_symlink() {
            // A link is kept only when it points inside the theme.
            let inside = entry.link_name().ok().flatten().is_some_and(|t| link_stays_inside(&t, path.components().count().saturating_sub(1)));
            if inside {
                entry.unpack_in(dest).map_err(|e| e.to_string())?;
            }
        }
        // Hard links, devices and the rest are skipped.
    }
    Ok(())
}

pub(super) fn unpack_zip(file: fs::File, dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(format!("Unsafe path in archive: {}", entry.name()));
        };
        let target = dest.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        // A zip keeps a link as an entry whose content is where it points. It has
        // to be made a link again, or every icon that is a link becomes a text file.
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            let mut destination = String::new();
            entry.by_ref().take(4096).read_to_string(&mut destination).map_err(|e| e.to_string())?;
            let inside = link_stays_inside(Path::new(&destination), relative.components().count().saturating_sub(1));
            if inside && !destination.is_empty() {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                if fs::symlink_metadata(&target).is_err() {
                    symlink(&destination, &target).map_err(|e| e.to_string())?;
                }
            }
            continue;
        }
        total += entry.size();
        if total > MAX_UNPACKED_BYTES {
            return Err("The package is too big".to_string());
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = fs::File::create(&target).map_err(|e| e.to_string())?;
        io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn unpack(archive: &Path, dest: &Path) -> Result<(), String> {
    let lower = archive.to_string_lossy().to_lowercase();
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    if lower.ends_with(".zip") {
        unpack_zip(file, dest)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        unpack_tar(flate2::read::GzDecoder::new(file), dest)
    } else if lower.ends_with(".tar.xz") {
        unpack_tar(xz2::read::XzDecoder::new(file), dest)
    } else if lower.ends_with(".tar.bz2") {
        unpack_tar(bzip2::read::BzDecoder::new(file), dest)
    } else if lower.ends_with(".tar") {
        unpack_tar(file, dest)
    } else {
        Err("This kind of file is not a package".to_string())
    }
}

// ── Finding the themes in a package ──────────────────────────────────────────

// Every folder under `dir` (links are not followed) for which `wanted` says yes.
fn walk_dirs(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            out.push(entry.path());
            walk_dirs(&entry.path(), depth + 1, out);
        }
    }
}

fn candidate(root: &Path, dir: &Path, kind: &str) -> Candidate {
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let display_name = read_theme_name(&dir.join("index.theme")).unwrap_or_else(|| name.clone());
    Candidate {
        rel: dir.strip_prefix(root).map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        name,
        display_name,
        kind: kind.to_string(),
        has_xfwm4: dir.join("xfwm4").is_dir(),
    }
}

fn find_candidates(root: &Path, kind: &str) -> Vec<Candidate> {
    let mut all = vec![root.to_path_buf()];
    walk_dirs(root, 0, &mut all);

    let mut found: Vec<Candidate> = Vec::new();
    if kind == "gtk" {
        // A folder that has gtk-3.0 or gtk-2.0 inside is a GTK theme.
        let mut seen: Vec<PathBuf> = Vec::new();
        for marker in ["gtk-3.0", "gtk-2.0"] {
            for dir in all.iter().filter(|d| d.file_name().is_some_and(|n| n == marker)) {
                if let Some(theme_dir) = dir.parent() {
                    if !seen.iter().any(|s| s == theme_dir) {
                        seen.push(theme_dir.to_path_buf());
                        found.push(candidate(root, theme_dir, "gtk"));
                    }
                }
            }
        }
        found.sort_by_key(|c| c.display_name.to_lowercase());
    } else {
        // A folder with an index.theme is an icon theme, or a cursor theme if it
        // also has a cursors folder. Each page asks only for its own kind.
        let wanted = if kind == "cursors" { "cursor" } else { "icon" };
        for dir in &all {
            if is_icon_theme_index(&dir.join("index.theme")) {
                let found_kind = if dir.join("cursors").is_dir() { "cursor" } else { "icon" };
                if found_kind == wanted {
                    found.push(candidate(root, dir, found_kind));
                }
            }
        }
    }
    found
}

// The name of a package without its extension: what a theme gets called when it
// sits loose at the top of the package, with no folder around it.
fn package_stem(archive: &Path) -> String {
    let name = archive.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let lower = name.to_lowercase();
    for ext in [".tar.gz", ".tar.xz", ".tar.bz2", ".tgz", ".tar", ".zip"] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name
}

// Whether the folder is itself a theme of the kind: GTK themes have a gtk-3.0 or
// gtk-2.0 folder, icon and cursor themes an index.theme.
fn is_theme_folder(dir: &Path, kind: &str) -> bool {
    if kind == "gtk" {
        dir.join("gtk-3.0").is_dir() || dir.join("gtk-2.0").is_dir()
    } else {
        is_icon_theme_index(&dir.join("index.theme"))
    }
}

// Many packages hold the theme with no folder around it. Such a theme would take
// the name of the folder it was unpacked into, so it is moved into a folder that
// carries the name of the package.
fn wrap_loose_theme(dir: &Path, archive: &Path, kind: &str) -> io::Result<()> {
    if !is_theme_folder(dir, kind) {
        return Ok(());
    }
    let stem = package_stem(archive);
    let name = if stem.is_empty() || stem == "." || stem == ".." { "theme".to_string() } else { stem.replace('/', "-") };

    // Everything goes into a scratch folder first: the package may hold an entry
    // that is called like the new folder.
    let scratch = dir.join(".soplos-loose-theme");
    fs::create_dir(&scratch)?;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path() != scratch {
            fs::rename(entry.path(), scratch.join(entry.file_name()))?;
        }
    }
    fs::rename(&scratch, dir.join(name))
}

// Unpacks a package into `dir` and lists the themes of the kind in it.
fn inspect_into(archive: &Path, kind: &str, dir: &Path) -> Result<Vec<Candidate>, String> {
    unpack(archive, dir)?;
    wrap_loose_theme(dir, archive, kind).map_err(|e| e.to_string())?;
    Ok(find_candidates(dir, kind))
}

// What a package holds besides the kind that was asked for, so a package opened on
// the wrong page can be told what it is.
fn other_kinds(dir: &Path, asked: &str) -> Vec<String> {
    let mut kinds: Vec<String> = ["gtk", "icons", "cursors"].iter().filter(|k| **k != asked && !find_candidates(dir, k).is_empty()).map(|k| k.to_string()).collect();
    // Cursors for Windows are plain .cur and .ani files.
    let mut folders = vec![dir.to_path_buf()];
    walk_dirs(dir, 0, &mut folders);
    let windows = folders.iter().any(|f| {
        fs::read_dir(f).is_ok_and(|read| {
            read.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name().to_string_lossy().to_lowercase();
                name.ends_with(".cur") || name.ends_with(".ani")
            })
        })
    });
    if windows && kinds.is_empty() {
        kinds.push("windows".to_string());
    }
    kinds
}

/// Unpacks a package and lists the themes in it. `kind` is "gtk", "icons" or
/// "cursors". The themes may be anywhere in the package: loose at the top, in a
/// folder, or in folders inside folders.
#[tauri::command(async)]
pub fn inspect_package(path: String, kind: String) -> Result<Inspected, String> {
    if kind != "gtk" && kind != "icons" && kind != "cursors" {
        return Err("Unknown kind of theme".to_string());
    }
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let token = format!("pkg-{}-{nanos}", std::process::id());
    let dir = package_dir(&token)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let candidates = match inspect_into(Path::new(&path), &kind, &dir) {
        Ok(found) => found,
        Err(e) => {
            let _ = fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let other_kinds = if candidates.is_empty() { other_kinds(&dir, &kind) } else { Vec::new() };
    if candidates.is_empty() {
        let _ = fs::remove_dir_all(&dir);
    }
    Ok(Inspected { token, candidates, other_kinds })
}

/// Throws away an unpacked package the user decided not to install.
#[tauri::command(async)]
pub fn discard_package(token: String) {
    if let Ok(dir) = package_dir(&token) {
        let _ = fs::remove_dir_all(dir);
    }
}

// ── Installing ───────────────────────────────────────────────────────────────

// Whether a link of a theme may be kept when the theme is copied. It has to be
// relative and may point inside the theme, or into a theme next to it in the
// package: icon themes often share their folders with a sibling (Gruvbox Plus
// Light points into Gruvbox Plus Dark). Both end up in the same folder, so such
// a link keeps working. Climbing higher than that folder is never allowed.
// `depth` is how deep the folder that holds the link is inside the theme.
pub fn link_ok_in_theme(target: &Path, depth: usize, theme_parent: &Path) -> bool {
    let mut level = depth as i64; // -1 is the folder that holds the themes
    for component in target.components() {
        match component {
            Component::Normal(name) => {
                if level == -1 && !fs::symlink_metadata(theme_parent.join(name)).is_ok_and(|m| m.is_dir()) {
                    return false;
                }
                level += 1;
            }
            Component::ParentDir => {
                level -= 1;
                if level < -1 {
                    return false;
                }
            }
            Component::CurDir => {}
            _ => return false,
        }
    }
    true
}

/// Copies a theme folder. Links are kept only when they point inside the theme
/// or to a sibling theme of the package (see link_ok_in_theme); anything that is
/// not a file, a folder or such a link is skipped. As root the copy gets the
/// usual permissions of a system file: readable by everybody, and executable
/// only where the original was.
pub fn copy_tree(src: &Path, dst: &Path, as_root: bool, depth: usize) -> io::Result<()> {
    let parent = src.parent().unwrap_or(src).to_path_buf();
    copy_dir(src, dst, as_root, depth, &parent)
}

fn copy_dir(src: &Path, dst: &Path, as_root: bool, depth: usize, theme_parent: &Path) -> io::Result<()> {
    if depth > MAX_DEPTH {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "The theme is nested too deep"));
    }
    fs::create_dir(dst)?;
    if as_root {
        fs::set_permissions(dst, fs::Permissions::from_mode(0o755))?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir(&from, &to, as_root, depth + 1, theme_parent)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to)?;
            if as_root {
                let executable = fs::metadata(&from)?.permissions().mode() & 0o111 != 0;
                fs::set_permissions(&to, fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }))?;
            }
        } else if file_type.is_symlink() {
            let target = fs::read_link(&from)?;
            if link_ok_in_theme(&target, depth, theme_parent) {
                symlink(&target, &to)?;
            }
        }
    }
    Ok(())
}

// The folder of an unpacked package that the window named, and nothing outside
// of the package.
fn theme_dir(package: &Path, rel: &str) -> Result<PathBuf, String> {
    let path = package.join(rel).canonicalize().map_err(|e| e.to_string())?;
    let root = package.canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(&root) || !path.is_dir() {
        return Err("The theme is not in the package".to_string());
    }
    Ok(path)
}

/// Installs the chosen themes. `kind` is "gtk", "icons" or "cursors" (icons and
/// cursors share their folders); `scope` is "user"
/// (their own folder, no password) or "global" (for every user, one password).
/// A theme that already exists is never overwritten. Returns the display names
/// of the ones that were left alone. The unpacked package is always removed.
#[tauri::command(async)]
pub fn install_candidates(token: String, rels: Vec<String>, kind: String, scope: String) -> Result<Vec<String>, String> {
    let package = package_dir(&token)?;
    let result = install_from(&package, &rels, &kind, &scope);
    let _ = fs::remove_dir_all(&package);
    result
}

fn install_from(package: &Path, rels: &[String], kind: &str, scope: &str) -> Result<Vec<String>, String> {
    let (system_base, user_base) = match kind {
        "gtk" => (PathBuf::from(SYSTEM_GTK_DIR), themes::user_gtk_dir()),
        "icons" | "cursors" => (PathBuf::from(SYSTEM_ICONS_DIR), themes::user_icons_dir()),
        _ => return Err("Unknown kind of theme".to_string()),
    };
    // The administrator step only knows "gtk" and "icons": the folder to fill.
    let folder_kind = if kind == "gtk" { "gtk" } else { "icons" };

    let mut chosen: Vec<(String, String, PathBuf)> = Vec::new(); // name, display name, folder
    for rel in rels {
        let dir = theme_dir(package, rel)?;
        let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let display = read_theme_name(&dir.join("index.theme")).unwrap_or_else(|| name.clone());
        chosen.push((name, display, dir));
    }

    let mut skipped: Vec<String> = Vec::new();
    match scope {
        "user" => {
            fs::create_dir_all(&user_base).map_err(|e| e.to_string())?;
            for (name, display, dir) in &chosen {
                if user_base.join(name).exists() || system_base.join(name).exists() {
                    skipped.push(display.clone());
                } else {
                    copy_tree(dir, &user_base.join(name), false, 0).map_err(|e| e.to_string())?;
                }
            }
        }
        "global" => {
            let mut ops: Vec<privileged::Op> = Vec::new();
            for (name, display, dir) in &chosen {
                if system_base.join(name).exists() {
                    skipped.push(display.clone());
                } else {
                    ops.push(privileged::Op::InstallTheme { kind: folder_kind.to_string(), src: dir.to_string_lossy().to_string(), name: name.clone() });
                }
            }
            if !ops.is_empty() {
                privileged::run_as_root(&ops)?;
            }
        }
        _ => return Err("Unknown installation scope".to_string()),
    }
    Ok(skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(name: &str) -> PathBuf {
        // The cache of the program, not /tmp.
        let dir = cache_root().join(format!("test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn links_must_stay_inside() {
        assert!(link_stays_inside(Path::new("other.png"), 0));
        assert!(link_stays_inside(Path::new("../sibling/x.png"), 1));
        assert!(!link_stays_inside(Path::new("../x.png"), 0));
        assert!(!link_stays_inside(Path::new("/etc/passwd"), 3));
        assert!(!link_stays_inside(Path::new("a/../../x"), 0));
    }

    #[test]
    fn unsafe_paths_are_refused() {
        assert!(unsafe_path(Path::new("../evil")));
        assert!(unsafe_path(Path::new("a/../../evil")));
        assert!(unsafe_path(Path::new("/etc/evil")));
        assert!(!unsafe_path(Path::new("Theme/gtk-3.0/gtk.css")));
    }

    #[test]
    fn links_inside_a_zip_come_back_as_links() {
        let dir = scratch("ziplinks");
        let archive = dir.join("Icons.zip");
        {
            let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("Icons/index.theme", options).unwrap();
            zip.write_all(b"[Icon Theme]\nName=Icons\n").unwrap();
            zip.start_file("Icons/scalable/folder.svg", options).unwrap();
            zip.write_all(b"<svg/>").unwrap();
            zip.add_symlink("Icons/16/folder.svg", "../scalable/folder.svg", options).unwrap();
            zip.add_symlink("Icons/16/escape.svg", "../../../etc/passwd", options).unwrap();
            zip.add_symlink("Icons/16/absolute.svg", "/etc/passwd", options).unwrap();
            zip.finish().unwrap();
        }
        let out = dir.join("out");
        fs::create_dir(&out).unwrap();
        unpack(&archive, &out).unwrap();

        let link = out.join("Icons/16/folder.svg");
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "it must be a link, not a text file");
        assert_eq!(fs::read_to_string(&link).unwrap(), "<svg/>", "and it must lead to the picture");
        assert!(fs::symlink_metadata(out.join("Icons/16/escape.svg")).is_err());
        assert!(fs::symlink_metadata(out.join("Icons/16/absolute.svg")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tar_gz_with_a_gtk_theme_is_found_and_installed_for_the_user() {
        let dir = scratch("gtk");
        let archive = dir.join("pack.tar.gz");
        {
            let file = fs::File::create(&archive).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            for (path, body) in [("Nordic/gtk-3.0/gtk.css", "x"), ("Nordic/xfwm4/themerc", "y"), ("Nordic/index.theme", "[Desktop Entry]\nName=Nordic Dark\n")] {
                let mut header = tar::Header::new_gnu();
                header.set_size(body.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder.append_data(&mut header, path, body.as_bytes()).unwrap();
            }
            builder.into_inner().unwrap().finish().unwrap().flush().unwrap();
        }
        let out = dir.join("out");
        fs::create_dir(&out).unwrap();
        unpack(&archive, &out).unwrap();

        let found = find_candidates(&out, "gtk");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Nordic");
        assert_eq!(found[0].display_name, "Nordic Dark");
        assert!(found[0].has_xfwm4);

        let target = dir.join("installed");
        copy_tree(&out.join("Nordic"), &target, false, 0).unwrap();
        assert!(target.join("gtk-3.0/gtk.css").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_link_to_a_sibling_theme_is_kept_but_nothing_higher() {
        let dir = scratch("sibling");
        fs::create_dir_all(dir.join("Dark/apps")).unwrap();
        fs::create_dir_all(dir.join("Light/apps")).unwrap();
        fs::write(dir.join("Dark/apps/a.svg"), "x").unwrap();
        symlink("../Dark/apps", dir.join("Light/shared")).unwrap(); // from the root of Light
        symlink("../../Dark/apps", dir.join("Light/apps/shared")).unwrap(); // one level deeper, as Gruvbox does
        symlink("../Missing/apps", dir.join("Light/missing")).unwrap(); // no such sibling
        symlink("../../etc/passwd", dir.join("Light/high")).unwrap(); // above the folder of the themes
        symlink("../../..", dir.join("Light/apps/up")).unwrap();

        let dst = dir.join("installed");
        fs::create_dir(&dst).unwrap();
        copy_tree(&dir.join("Light"), &dst.join("Light"), false, 0).unwrap();
        assert!(fs::symlink_metadata(dst.join("Light/shared")).is_ok(), "link from the root of the theme");
        assert!(fs::symlink_metadata(dst.join("Light/apps/shared")).is_ok(), "link from a deeper folder");
        assert!(fs::symlink_metadata(dst.join("Light/missing")).is_err());
        assert!(fs::symlink_metadata(dst.join("Light/high")).is_err());
        assert!(fs::symlink_metadata(dst.join("Light/apps/up")).is_err());

        // Once the sibling is installed next to it, the link works.
        copy_tree(&dir.join("Dark"), &dst.join("Dark"), false, 0).unwrap();
        assert!(dst.join("Light/apps/shared/a.svg").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn copying_drops_links_that_leave_the_theme() {
        let dir = scratch("links");
        let src = dir.join("theme");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("real.txt"), "a").unwrap();
        symlink("real.txt", src.join("ok-link")).unwrap();
        symlink("../real.txt", src.join("sub/ok-up")).unwrap();
        symlink("/etc/passwd", src.join("bad-abs")).unwrap();
        symlink("../../outside", src.join("bad-up")).unwrap();

        let dst = dir.join("copy");
        copy_tree(&src, &dst, false, 0).unwrap();
        assert!(fs::symlink_metadata(dst.join("ok-link")).is_ok());
        assert!(fs::symlink_metadata(dst.join("sub/ok-up")).is_ok());
        assert!(fs::symlink_metadata(dst.join("bad-abs")).is_err());
        assert!(fs::symlink_metadata(dst.join("bad-up")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn icons_and_cursors_are_told_apart() {
        let dir = scratch("icons");
        fs::create_dir_all(dir.join("Pack/Icons/cursors")).unwrap();
        fs::write(dir.join("Pack/Icons/index.theme"), "[Icon Theme]\nName=Cur\n").unwrap();
        fs::create_dir_all(dir.join("Pack/Look")).unwrap();
        fs::write(dir.join("Pack/Look/index.theme"), "[Icon Theme]\nName=Look\n").unwrap();
        assert_eq!(find_candidates(&dir, "icons").iter().map(|c| c.display_name.clone()).collect::<Vec<_>>(), vec!["Look"]);
        assert_eq!(find_candidates(&dir, "cursors").iter().map(|c| c.display_name.clone()).collect::<Vec<_>>(), vec!["Cur"]);
        let _ = fs::remove_dir_all(&dir);
    }

    // Builds a package holding the given files and opens it as the window does.
    fn open_package(name: &str, kind: &str, files: &[(&str, &str)]) -> Vec<Candidate> {
        let dir = scratch(&format!("pkg-{}", name.replace('.', "_")));
        let archive = dir.join(name);
        if name.ends_with(".zip") {
            let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            for (path, body) in files {
                zip.start_file(*path, zip::write::SimpleFileOptions::default()).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        } else {
            let encoder = flate2::write::GzEncoder::new(fs::File::create(&archive).unwrap(), flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            for (path, body) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(body.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder.append_data(&mut header, path, body.as_bytes()).unwrap();
            }
            builder.into_inner().unwrap().finish().unwrap();
        }
        let out = dir.join("out");
        fs::create_dir(&out).unwrap();
        let found = inspect_into(&archive, kind, &out).unwrap();
        let _ = fs::remove_dir_all(&dir);
        found
    }

    fn names(found: &[Candidate]) -> Vec<String> {
        found.iter().map(|c| c.name.clone()).collect()
    }

    #[test]
    fn a_gtk_theme_loose_at_the_top_takes_the_name_of_the_package() {
        let found = open_package("My-Theme_1.2.zip", "gtk", &[("gtk-3.0/gtk.css", "x"), ("xfwm4/themerc", "y"), ("index.theme", "[Desktop Entry]\nName=My Theme\n")]);
        assert_eq!(names(&found), vec!["My-Theme_1.2"]);
        assert_eq!(found[0].display_name, "My Theme");
        assert!(found[0].has_xfwm4);
    }

    #[test]
    fn an_icon_theme_loose_at_the_top_takes_the_name_of_the_package() {
        let found = open_package("Fancy-Icons.tar.gz", "icons", &[("index.theme", "[Icon Theme]\nName=Fancy\n"), ("scalable/apps/a.svg", "x")]);
        assert_eq!(names(&found), vec!["Fancy-Icons"]);
    }

    #[test]
    fn themes_in_a_folder_keep_their_own_names() {
        let found = open_package("Pack.zip", "gtk", &[("Nordic/gtk-3.0/gtk.css", "x"), ("Nordic-dark/gtk-3.0/gtk.css", "x")]);
        assert_eq!(names(&found), vec!["Nordic", "Nordic-dark"]);
    }

    #[test]
    fn themes_in_folders_inside_folders_are_found() {
        let found = open_package("repo-main.tar.gz", "gtk", &[("repo-main/themes/Foo/gtk-3.0/gtk.css", "x"), ("repo-main/themes/Bar/gtk-2.0/gtkrc", "x"), ("repo-main/README.md", "x")]);
        assert_eq!(names(&found), vec!["Bar", "Foo"]);
        let found = open_package("repo.zip", "icons", &[("repo/a/b/Deep-Icons/index.theme", "[Icon Theme]\nName=Deep\n"), ("repo/a/Other/index.theme", "[Icon Theme]\nName=Other\n")]);
        assert_eq!(names(&found), vec!["Other", "Deep-Icons"].iter().map(|s| s.to_string()).collect::<Vec<_>>());
    }

    #[test]
    fn the_index_theme_of_a_gtk_metatheme_is_not_an_icon_theme() {
        let gtk_only = [
            ("themes/Agata-Nord-Dark/index.theme", "[Desktop Entry]\nType=X-GNOME-Metatheme\nName=Agata\n\n[X-GNOME-Metatheme]\nIconTheme=Adwaita\n"),
            ("themes/Agata-Nord-Dark/gtk-3.0/gtk.css", "x"),
        ];
        assert!(open_package("Agata.tar.gz", "icons", &gtk_only).is_empty());
        assert!(open_package("Agata.tar.gz", "cursors", &gtk_only).is_empty());
        assert_eq!(names(&open_package("Agata.tar.gz", "gtk", &gtk_only)), vec!["Agata-Nord-Dark"]);
    }

    // What a package holds that is not of the kind asked for.
    fn elsewhere(name: &str, kind: &str, files: &[(&str, &str)]) -> Vec<String> {
        let dir = scratch(&format!("else-{}", name.replace('.', "_")));
        let archive = dir.join(name);
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        for (path, body) in files {
            zip.start_file(*path, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let out = dir.join("out");
        fs::create_dir(&out).unwrap();
        assert!(inspect_into(&archive, kind, &out).unwrap().is_empty());
        let kinds = other_kinds(&out, kind);
        let _ = fs::remove_dir_all(&dir);
        kinds
    }

    #[test]
    fn a_package_opened_on_the_wrong_page_says_what_it_is() {
        let gtk = [("Agata/index.theme", "[Desktop Entry]\nType=X-GNOME-Metatheme\n"), ("Agata/gtk-3.0/gtk.css", "x")];
        assert_eq!(elsewhere("Agata.zip", "icons", &gtk), vec!["gtk"]);
        assert_eq!(elsewhere("Agata.zip", "cursors", &gtk), vec!["gtk"]);
        let icons = [("Pack/index.theme", "[Icon Theme]\nName=P\n")];
        assert_eq!(elsewhere("Pack.zip", "gtk", &icons), vec!["icons"]);
        let cursors = [("Cur/index.theme", "[Icon Theme]\nName=C\n"), ("Cur/cursors/left_ptr", "x")];
        assert_eq!(elsewhere("Cur.zip", "icons", &cursors), vec!["cursors"]);
        let windows = [("Bibata-Windows/Arrow.cur", "x"), ("Bibata-Windows/Busy.ani", "x")];
        assert_eq!(elsewhere("Win.zip", "cursors", &windows), vec!["windows"]);
        assert!(elsewhere("Docs.zip", "gtk", &[("README.md", "x")]).is_empty());
    }

    #[test]
    fn a_package_without_a_theme_gives_nothing() {
        assert!(open_package("Docs.zip", "gtk", &[("README.md", "x"), ("src/build.sh", "x")]).is_empty());
        assert!(open_package("Win-Cursors.zip", "cursors", &[("Theme/Arrow.cur", "x"), ("Theme/Busy.ani", "x")]).is_empty());
    }
}
