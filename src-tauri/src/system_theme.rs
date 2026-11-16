// The light or dark theme of the Xfce desktop, so the window can follow it.
// The program only runs on Xfce, whose theme lives in the xsettings channel
// of xfconf.

use std::process::Command;

fn xsettings(property: &str) -> Option<String> {
    let out = Command::new("xfconf-query").args(["-c", "xsettings", "-p", property]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// "dark", "light" or "unknown" when Xfce does not answer.
#[tauri::command]
pub fn get_system_theme() -> String {
    // A theme that asks for its dark variant says so explicitly.
    if xsettings("/Net/PreferDarkTheme").as_deref() == Some("true") {
        return "dark".to_string();
    }
    match xsettings("/Net/ThemeName") {
        // Xfce has no color scheme setting, so the name of the GTK theme is
        // what there is to go by: this is a guess from words such as "dark".
        Some(name) => {
            let name = name.to_lowercase();
            if name.contains("dark") || name.contains("black") {
                "dark".to_string()
            } else {
                "light".to_string()
            }
        }
        None => "unknown".to_string(),
    }
}
