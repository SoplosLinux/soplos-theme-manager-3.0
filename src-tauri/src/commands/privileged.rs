// What needs the administrator: putting a theme where every user can use it.
//
// The window never runs as root and never sends a script. It writes a request
// with named operations, and pkexec starts this same program again as
// `--run-ops <file>` under the polkit action of the package. That second run
// never opens a window: it checks everything again as root, because the file
// can be written by any user, and only then does the work.

use super::packages::{cache_root, copy_tree};
use super::themes::{owner_package, SYSTEM_GTK_DIR, SYSTEM_ICONS_DIR};
use super::wallpapers::{has_image_extension, SYSTEM_WALLPAPERS_DIR};
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Copies an unpacked theme to /usr/share/themes or /usr/share/icons.
    InstallTheme { kind: String, src: String, name: String },
    /// Copies the pictures of a staged folder to /usr/share/backgrounds.
    InstallWallpapers { dir: String, #[serde(default)] sub: Option<String> },
    /// Removes a theme from /usr/share/themes or /usr/share/icons.
    RemoveTheme { kind: String, name: String },
    /// Sets keys of the [greeter] section of the LightDM greeter.
    WriteLoginScreen { updates: Vec<(String, String)> },
}

#[derive(Serialize, Deserialize)]
struct Request {
    ops: Vec<Op>,
}

// ── User side ────────────────────────────────────────────────────────────────

/// Asks pkexec to run the operations as root. One call is one password.
pub fn run_as_root(ops: &[Op]) -> Result<(), String> {
    let dir = cache_root();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("request-{}.json", std::process::id()));

    let request = serde_json::to_string(&Request { ops: ops.iter().map(clone_op).collect() }).map_err(|e| e.to_string())?;
    {
        use std::io::Write;
        let mut out = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&file).map_err(|e| e.to_string())?;
        out.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let output = Command::new("pkexec").arg(exe).arg("--run-ops").arg(&file).output();
    let _ = fs::remove_file(&file);

    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            // What the administrator step or pkexec said is the reason to show.
            let said = String::from_utf8_lossy(&o.stderr).trim().to_string();
            Err(match (said.is_empty(), o.status.code()) {
                (false, _) => said,
                (true, Some(126)) => "The administrator password was not given".to_string(),
                _ => "The administrator step did not finish".to_string(),
            })
        }
        Err(e) => Err(e.to_string()),
    }
}

fn clone_op(op: &Op) -> Op {
    match op {
        Op::InstallTheme { kind, src, name } => Op::InstallTheme { kind: kind.clone(), src: src.clone(), name: name.clone() },
        Op::InstallWallpapers { dir, sub } => Op::InstallWallpapers { dir: dir.clone(), sub: sub.clone() },
        Op::RemoveTheme { kind, name } => Op::RemoveTheme { kind: kind.clone(), name: name.clone() },
        Op::WriteLoginScreen { updates } => Op::WriteLoginScreen { updates: updates.clone() },
    }
}

// ── Root side ────────────────────────────────────────────────────────────────

// The home folder of a user, read from the password file.
fn home_of(uid: u32) -> Option<PathBuf> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        (fields.len() >= 6 && fields[2].parse::<u32>().ok() == Some(uid)).then(|| PathBuf::from(fields[5]))
    })
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 255 && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

// The cache folder of the user who asked, where the requests and the unpacked
// packages live.
fn user_cache(uid: u32) -> Result<PathBuf, String> {
    let home = home_of(uid).ok_or("Unknown user")?;
    home.join(".cache").join("soplos-theme-manager").canonicalize().map_err(|e| e.to_string())
}

fn run_op(op: &Op, cache: &Path) -> Result<(), String> {
    match op {
        Op::InstallTheme { kind, src, name } => {
            let base = match kind.as_str() {
                "gtk" => Path::new(SYSTEM_GTK_DIR),
                "icons" => Path::new(SYSTEM_ICONS_DIR),
                _ => return Err(format!("Unknown kind of theme: {kind}")),
            };
            if !valid_name(name) {
                return Err(format!("Not a valid theme name: {name}"));
            }
            // Only what is inside the cache of the user who asked can be copied.
            let src = Path::new(src).canonicalize().map_err(|e| e.to_string())?;
            if !src.starts_with(cache) || !src.is_dir() {
                return Err("The theme is not inside the cache of the program".to_string());
            }
            let dest = base.join(name);
            if dest.exists() {
                return Ok(());
            }
            copy_tree(&src, &dest, true, 0).map_err(|e| {
                // A half copy is worse than none.
                let _ = fs::remove_dir_all(&dest);
                e.to_string()
            })
        }
        Op::WriteLoginScreen { updates } => super::lightdm::write_as_root(updates),
        Op::RemoveTheme { kind, name } => {
            let base = match kind.as_str() {
                "gtk" => Path::new(SYSTEM_GTK_DIR),
                "icons" => Path::new(SYSTEM_ICONS_DIR),
                _ => return Err(format!("Unknown kind of theme: {kind}")),
            };
            if !valid_name(name) {
                return Err(format!("Not a valid theme name: {name}"));
            }
            let dir = base.join(name);
            let meta = fs::symlink_metadata(&dir).map_err(|e| e.to_string())?;
            // Checked again as root: what a package installed is not removed by hand.
            if let Some(package) = owner_package(&dir) {
                return Err(format!("The theme belongs to the package {package}"));
            }
            if meta.file_type().is_symlink() {
                fs::remove_file(&dir).map_err(|e| e.to_string())
            } else if meta.is_dir() {
                fs::remove_dir_all(&dir).map_err(|e| e.to_string())
            } else {
                Err("It is not a theme".to_string())
            }
        }
        Op::InstallWallpapers { dir, sub } => {
            // Only the staged folder of the user who asked can be copied.
            let dir = Path::new(dir).canonicalize().map_err(|e| e.to_string())?;
            if !dir.starts_with(cache) || !dir.is_dir() {
                return Err("The pictures are not inside the cache of the program".to_string());
            }
            let mut target = PathBuf::from(SYSTEM_WALLPAPERS_DIR);
            // A folder of its own below /usr/share/backgrounds, made of plain names.
            if let Some(sub) = sub {
                for part in sub.split('/') {
                    if !valid_name(part) {
                        return Err(format!("Not a valid folder: {sub}"));
                    }
                    target.push(part);
                }
            }
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name().to_string_lossy().to_string();
                // Only plain picture files; a link or a folder is never followed.
                if !entry.file_type().map_err(|e| e.to_string())?.is_file() || !valid_name(&name) || !has_image_extension(&entry.path()) {
                    continue;
                }
                let dest = target.join(&name);
                // A picture of the same name is replaced, as the 2.x program did.
                if fs::symlink_metadata(&dest).is_ok_and(|m| m.is_file() || m.file_type().is_symlink()) {
                    fs::remove_file(&dest).map_err(|e| e.to_string())?;
                }
                fs::copy(entry.path(), &dest).map_err(|e| e.to_string())?;
                fs::set_permissions(&dest, fs::Permissions::from_mode(0o644)).map_err(|e| e.to_string())?;
            }
            Ok(())
        }
    }
}

/// Called from main() before Tauri starts when pkexec starts the program again.
/// Returns the exit code.
pub fn run_ops_file(path: &str) -> i32 {
    let uid = match std::env::var("PKEXEC_UID").ok().and_then(|v| v.parse::<u32>().ok()) {
        Some(uid) => uid,
        None => {
            eprintln!("This mode is only for pkexec");
            return 2;
        }
    };
    let result = (|| -> Result<(), String> {
        let cache = user_cache(uid)?;
        let file = Path::new(path).canonicalize().map_err(|e| e.to_string())?;
        let meta = fs::metadata(&file).map_err(|e| e.to_string())?;
        // The request has to be a file of the user who asked, in their cache.
        if !file.starts_with(&cache) || !meta.is_file() || meta.uid() != uid {
            return Err("The request is not valid".to_string());
        }
        let request: Request = serde_json::from_str(&fs::read_to_string(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        for op in &request.ops {
            run_op(op, &cache)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}
