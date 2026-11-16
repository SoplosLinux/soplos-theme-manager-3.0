// The picture of the user and the wallpaper that the login screen takes from
// the account. Both live in two places: the files of the home folder, which
// programs like Mugshot read, and AccountsService, which the greeter reads. They
// are kept the same, so a change is seen everywhere.

use super::themes::home;
use super::wallpapers::has_image_extension;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;

// AccountsService refuses a picture bigger than this.
const MAX_AVATAR_BYTES: u64 = 1024 * 1024;

fn face() -> PathBuf {
    home().join(".face")
}

fn user_object() -> Option<String> {
    let uid = fs::metadata(home()).ok()?.uid();
    Some(format!("/org/freedesktop/Accounts/User{uid}"))
}

// Calls a method of the account of this user in AccountsService.
fn accounts_call(method: &str, value: &str) -> bool {
    let Some(object) = user_object() else { return false };
    Command::new("gdbus")
        .args(["call", "--system", "--dest", "org.freedesktop.Accounts", "--object-path", &object, "--method", &format!("org.freedesktop.Accounts.User.{method}"), value])
        .output()
        .is_ok_and(|o| o.status.success())
}

const USER_INTERFACE: &str = "org.freedesktop.Accounts.User";
// The part of the account that the display managers use: the wallpaper.
const DISPLAY_INTERFACE: &str = "org.freedesktop.DisplayManager.AccountsService";

// Sets a text property of the account of this user.
fn accounts_set_property(interface: &str, name: &str, value: &str) -> bool {
    let Some(object) = user_object() else { return false };
    // A text has to be given to gdbus as a variant: <'text'>.
    let variant = format!("<'{}'>", value.replace('\\', "\\\\").replace('\'', "\\'"));
    Command::new("gdbus")
        .args(["call", "--system", "--dest", "org.freedesktop.Accounts", "--object-path", &object, "--method", "org.freedesktop.DBus.Properties.Set", interface, name, &variant])
        .output()
        .is_ok_and(|o| o.status.success())
}

// A property of the account of this user, as the text gdbus prints it.
fn accounts_property(interface: &str, name: &str) -> Option<String> {
    let object = user_object()?;
    let out = Command::new("gdbus")
        .args(["call", "--system", "--dest", "org.freedesktop.Accounts", "--object-path", &object, "--method", "org.freedesktop.DBus.Properties.Get", interface, name])
        .output()
        .ok()?;
    // Prints: (<'/path/to/file'>,)
    let text = String::from_utf8_lossy(&out.stdout);
    let start = text.find('\'')? + 1;
    let end = text.rfind('\'')?;
    (end > start).then(|| text[start..end].to_string())
}

/// Makes the wallpaper the one the greeter shows for this user, when it is set
/// to use the wallpaper of each user. Not being able to is not a failure: the
/// desktop wallpaper has been changed already.
pub fn publish_wallpaper(path: &str) {
    let _ = accounts_set_property(DISPLAY_INTERFACE, "BackgroundFile", path);
}

/// The wallpaper that Xfce shows now. Opens it to the window so it can be shown,
/// and with `publish` it is also given to the account, so the login screen shows
/// the same one when it is set to use the wallpaper of the user.
#[tauri::command(async)]
pub fn desktop_wallpaper(app: tauri::AppHandle, publish: bool) -> Option<String> {
    let path = super::bundles::current_wallpaper()?;
    app.asset_protocol_scope().allow_file(Path::new(&path)).ok()?;
    if publish {
        publish_wallpaper(&path);
    }
    Some(path)
}

/// The picture of the user: the file of the home folder, or the one that
/// AccountsService has. Opens it to the window so it can be shown.
#[tauri::command(async)]
pub fn user_avatar(app: tauri::AppHandle) -> Option<String> {
    let file = face();
    let path = if fs::metadata(&file).is_ok_and(|m| m.is_file()) {
        file
    } else {
        let icon = PathBuf::from(accounts_property(USER_INTERFACE, "IconFile")?);
        if !icon.is_file() {
            return None;
        }
        icon
    };
    app.asset_protocol_scope().allow_file(&path).ok()?;
    Some(path.to_string_lossy().to_string())
}

/// Sets the picture of the user: it is copied to ~/.face and given to
/// AccountsService. Returns the path to show.
#[tauri::command(async)]
pub fn set_user_avatar(app: tauri::AppHandle, path: String) -> Result<String, String> {
    let source = Path::new(&path);
    if !has_image_extension(source) || !fs::metadata(source).is_ok_and(|m| m.is_file()) {
        return Err("It is not a picture".to_string());
    }
    let size = fs::metadata(source).map_err(|e| e.to_string())?.len();
    if size > MAX_AVATAR_BYTES {
        return Err("The picture is bigger than 1 MiB".to_string());
    }

    let target = face();
    let same = source.canonicalize().ok().zip(target.canonicalize().ok()).is_some_and(|(a, b)| a == b);
    if !same {
        fs::copy(source, &target).map_err(|e| e.to_string())?;
    }
    // Some programs look for .face.icon instead; it is only made if it is free.
    let icon = home().join(".face.icon");
    if fs::symlink_metadata(&icon).is_err() {
        let _ = symlink(&target, &icon);
    }
    if !accounts_call("SetIconFile", &target.to_string_lossy()) {
        return Err("The picture was saved, but the login screen could not be told".to_string());
    }
    app.asset_protocol_scope().allow_file(&target).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reads the account of this computer; it changes nothing.
    #[test]
    fn the_account_of_this_computer_is_read() {
        println!("object: {:?}", user_object());
        println!("icon: {:?}", accounts_property(USER_INTERFACE, "IconFile"));
        println!("background: {:?}", accounts_property(DISPLAY_INTERFACE, "BackgroundFile"));
        assert!(user_object().is_some());
    }
}
