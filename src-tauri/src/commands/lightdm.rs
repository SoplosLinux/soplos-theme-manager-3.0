// The login screen: the settings of the LightDM GTK greeter, which live in
// /etc/lightdm/lightdm-gtk-greeter.conf, a system file. Only the keys the page
// offers are ever changed, and the rest of the file, comments included, is kept.

use super::privileged;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use tauri::Manager;

pub const CONFIG_PATH: &str = "/etc/lightdm/lightdm-gtk-greeter.conf";
const SECTION: &str = "greeter";

/// The keys of the file that this page may change.
pub const EDITABLE_KEYS: [&str; 10] = [
    "background",
    "theme-name",
    "icon-theme-name",
    "cursor-theme-name",
    "default-user-image",
    "font-name",
    "position",
    "hide-user-image",
    "user-background",
    "clock-format",
];

// ── The file ─────────────────────────────────────────────────────────────────

fn section_name(line: &str) -> Option<&str> {
    line.trim().strip_prefix('[').and_then(|l| l.strip_suffix(']'))
}

// The key of a `key = value` line, as the greeter reads it: in lower case.
fn key_of(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
        return None;
    }
    let (key, value) = line.split_once('=').or_else(|| line.split_once(':'))?;
    Some((key.trim().to_lowercase(), value.trim().to_string()))
}

/// The settings of the [greeter] section.
pub fn parse_greeter(text: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    let mut inside = false;
    for line in text.lines() {
        if let Some(name) = section_name(line) {
            inside = name == SECTION;
        } else if inside {
            if let Some((key, value)) = key_of(line) {
                values.entry(key).or_insert(value);
            }
        }
    }
    values
}

/// The text of the file with the given keys of [greeter] set; an empty value
/// removes the key. Everything else is left exactly as it was.
pub fn apply_updates(text: &str, updates: &[(String, String)]) -> String {
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();

    // Where [greeter] starts and ends (the end is before the next section).
    let start = lines.iter().position(|l| section_name(l) == Some(SECTION));
    let start = match start {
        Some(s) => s,
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[{SECTION}]"));
            lines.len() - 1
        }
    };

    for (key, value) in updates {
        let end = lines.iter().enumerate().skip(start + 1).find(|(_, l)| section_name(l).is_some()).map(|(i, _)| i).unwrap_or(lines.len());
        let found: Vec<usize> = (start + 1..end).filter(|&i| key_of(&lines[i]).is_some_and(|(k, _)| k == *key)).collect();
        if value.is_empty() {
            // Highest first, so the numbers of the others stay right.
            for &i in found.iter().rev() {
                lines.remove(i);
            }
        } else if let Some(&first) = found.first() {
            lines[first] = format!("{key} = {value}");
            for &i in found.iter().skip(1).rev() {
                lines.remove(i);
            }
        } else {
            // After the last line with content of the section, so blank lines and
            // the comments of the next section stay where they are.
            let mut at = end;
            while at > start + 1 && lines[at - 1].trim().is_empty() {
                at -= 1;
            }
            lines.insert(at, format!("{key} = {value}"));
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

// ── What may be written ──────────────────────────────────────────────────────

fn valid_position(value: &str) -> bool {
    let part = |p: &str| {
        let Some((number, anchor)) = p.split_once("%,") else { return false };
        number.len() <= 3 && number.parse::<u32>().is_ok_and(|n| n <= 100) && ["start", "center", "end"].contains(&anchor)
    };
    let parts: Vec<&str> = value.split(' ').collect();
    (parts.len() == 1 || parts.len() == 2) && parts.iter().all(|p| part(p))
}

/// Checks the updates the same way in the window's side and as administrator:
/// only the keys of the page, one line each, and the values that the greeter
/// understands where they are of a fixed form.
pub fn validate(updates: &[(String, String)]) -> Result<(), String> {
    for (key, value) in updates {
        if !EDITABLE_KEYS.contains(&key.as_str()) {
            return Err(format!("Not a setting of the login screen: {key}"));
        }
        if value.len() > 1024 || value.chars().any(|c| c == '\n' || c == '\r' || c == '\0') {
            return Err(format!("Not a valid value for {key}"));
        }
        let ok = match key.as_str() {
            "position" => value.is_empty() || valid_position(value),
            "hide-user-image" | "user-background" => value.is_empty() || value == "true" || value == "false",
            _ => true,
        };
        if !ok {
            return Err(format!("Not a valid value for {key}"));
        }
    }
    Ok(())
}

/// Writes the settings into the file, as administrator. The file is replaced in
/// one step, so a failure never leaves it half written.
pub fn write_as_root(updates: &[(String, String)]) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    validate(updates)?;
    let path = Path::new(CONFIG_PATH);
    let current = fs::read_to_string(path).unwrap_or_default();
    let new = apply_updates(&current, updates);
    let dir = path.parent().ok_or("No folder")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let temp = dir.join(".lightdm-gtk-greeter.conf.soplos-new");
    fs::write(&temp, new).map_err(|e| e.to_string())?;
    fs::set_permissions(&temp, fs::Permissions::from_mode(0o644)).map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        e.to_string()
    })
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct LoginSettings {
    /// The keys of the [greeter] section as the file has them.
    pub values: BTreeMap<String, String>,
}

/// The settings of the greeter now. A computer without the file has none.
#[tauri::command(async)]
pub fn lightdm_read() -> LoginSettings {
    let text = fs::read_to_string(CONFIG_PATH).unwrap_or_default();
    LoginSettings { values: parse_greeter(&text) }
}

/// The names of the font families that are installed, to choose one.
#[tauri::command(async)]
pub fn lightdm_fonts() -> Vec<String> {
    let Ok(out) = Command::new("fc-list").arg(":").arg("family").output() else { return Vec::new() };
    let mut families: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        // A line can name the same family in several languages: "Noto Sans,Noto Sans Regular".
        .filter_map(|l| l.split(',').next().map(|f| f.trim().to_string()))
        .filter(|f| !f.is_empty())
        .collect();
    families.sort_by_key(|f| f.to_lowercase());
    families.dedup();
    families
}

/// Opens a picture to the window so it can be shown: the background and the
/// image of the user are files anywhere on the disk.
#[tauri::command(async)]
pub fn allow_image(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let file = Path::new(&path);
    if !super::wallpapers::has_image_extension(file) || !file.is_file() {
        return Err("It is not a picture".to_string());
    }
    app.asset_protocol_scope().allow_file(file).map_err(|e| e.to_string())
}

/// Saves the settings of the login screen. It needs the administrator password.
#[tauri::command(async)]
pub fn lightdm_write(updates: BTreeMap<String, String>) -> Result<(), String> {
    let updates: Vec<(String, String)> = updates.into_iter().collect();
    validate(&updates)?;
    privileged::run_as_root(&[privileged::Op::WriteLoginScreen { updates }])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    const SAMPLE: &str = "# Greeter of the computer\n[greeter]\n# the picture\nbackground = /usr/share/backgrounds/old.jpg\nTheme-Name=Adwaita\nindicators = ~host;~spacer;~clock\nposition=50%,center\n\n[other]\nkeep = me\n";

    #[test]
    fn the_greeter_section_is_read() {
        let v = parse_greeter(SAMPLE);
        assert_eq!(v["background"], "/usr/share/backgrounds/old.jpg");
        assert_eq!(v["theme-name"], "Adwaita", "keys are read in lower case");
        assert_eq!(v["position"], "50%,center");
        assert!(!v.contains_key("keep"), "other sections are not the greeter");
    }

    #[test]
    fn only_the_given_keys_change_and_the_rest_is_kept() {
        let new = apply_updates(SAMPLE, &up(&[("background", "/b.png"), ("theme-name", "Nord"), ("font-name", "Sans 11"), ("position", "")]));
        assert!(new.starts_with("# Greeter of the computer\n[greeter]\n# the picture\n"), "comments stay");
        assert!(new.contains("background = /b.png\n"));
        assert!(new.contains("theme-name = Nord\n") && !new.contains("Theme-Name=Adwaita"));
        assert!(new.contains("indicators = ~host;~spacer;~clock\n"), "keys of the file that the page does not own stay");
        assert!(!new.contains("position"), "an empty value removes the key");
        assert!(new.contains("font-name = Sans 11\n[other]") || new.contains("font-name = Sans 11\n\n[other]"), "a new key goes inside the section, before the next one");
        assert!(new.ends_with("[other]\nkeep = me\n"), "the other sections stay");
        // Applying the same thing twice changes nothing more.
        assert_eq!(apply_updates(&new, &up(&[("background", "/b.png")])), new);
    }

    #[test]
    fn a_file_without_the_section_gets_it() {
        assert_eq!(apply_updates("", &up(&[("theme-name", "A")])), "[greeter]\ntheme-name = A\n");
        assert_eq!(apply_updates("[x]\na=1\n", &up(&[("theme-name", "A")])), "[x]\na=1\n\n[greeter]\ntheme-name = A\n");
    }

    #[test]
    fn what_cannot_be_written_is_refused() {
        assert!(validate(&up(&[("theme-name", "Nord"), ("position", "5%,start 95%,end"), ("hide-user-image", "true")])).is_ok());
        assert!(validate(&up(&[("greeter-hide-users", "true")])).is_err(), "not a key of the page");
        assert!(validate(&up(&[("theme-name", "a\n[x]\nb=1")])).is_err(), "no new lines: they would add settings");
        assert!(validate(&up(&[("position", "50%,middle")])).is_err());
        assert!(validate(&up(&[("position", "150%,start")])).is_err());
        assert!(validate(&up(&[("hide-user-image", "yes")])).is_err());
        assert!(validate(&up(&[("clock-format", "%a, %H:%M")])).is_ok());
    }

    #[test]
    fn the_fonts_of_this_computer_are_listed() {
        let fonts = lightdm_fonts();
        println!("{} families, first: {:?}", fonts.len(), fonts.iter().take(3).collect::<Vec<_>>());
        assert!(!fonts.is_empty());
        println!("settings now: {:?}", lightdm_read().values);
    }
}
