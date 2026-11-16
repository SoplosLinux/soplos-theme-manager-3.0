// Reads and writes the settings of Xfce through xfconf-query. Every setting of
// the program lives in one of its channels (xsettings for the GTK, icon and
// cursor themes, xfwm4 for the window borders).

use std::process::Command;

/// The value of a property, or None when it is not set or xfconf does not answer.
pub fn get(channel: &str, property: &str) -> Option<String> {
    let out = Command::new("xfconf-query").args(["-c", channel, "-p", property]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// Sets a string property, creating it when it does not exist yet.
pub fn set_string(channel: &str, property: &str, value: &str) -> bool {
    Command::new("xfconf-query")
        .args(["-c", channel, "-p", property, "--create", "-t", "string", "-s", value])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Sets a property of any type xfconf-query knows ("int", "bool", "string"...).
pub fn set_typed(channel: &str, property: &str, kind: &str, value: &str) -> bool {
    Command::new("xfconf-query")
        .args(["-c", channel, "-p", property, "--create", "-t", kind, "-s", value])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
