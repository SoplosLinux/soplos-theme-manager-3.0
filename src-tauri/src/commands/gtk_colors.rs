// The colors of a GTK theme, read from its stylesheets, so a card can draw a
// small window with them. A theme that says them as plain colors gets four:
// the window, the contents, the accent and the text.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// The `@define-color` lines of a stylesheet and of the ones it imports.
fn read_css(path: &Path, defs: &mut HashMap<String, String>, depth: u8) {
    let Ok(text) = fs::read_to_string(path) else { return };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("@define-color") {
            let rest = rest.trim();
            if let Some((name, value)) = rest.split_once(char::is_whitespace) {
                defs.entry(name.to_string()).or_insert_with(|| value.trim().trim_end_matches(';').trim().to_string());
            }
        } else if line.starts_with("@import") && depth < 2 {
            // The file of `@import "x.css";` and of `@import url("x.css");`.
            let target = line
                .split(|c| c == '"' || c == '\'')
                .nth(1)
                .map(str::to_string)
                .or_else(|| line.split_once('(').and_then(|(_, rest)| rest.split_once(')')).map(|(t, _)| t.trim().to_string()));
            if let (Some(parent), Some(target)) = (path.parent(), target) {
                if !target.is_empty() && !target.starts_with("resource:") {
                    read_css(&parent.join(target), defs, depth + 1);
                }
            }
        }
    }
}

fn is_css_color(value: &str) -> bool {
    let value = value.trim();
    (value.starts_with('#') && matches!(value.len(), 4 | 7 | 9) && value[1..].chars().all(|c| c.is_ascii_hexdigit()))
        || ((value.starts_with("rgb(") || value.starts_with("rgba(")) && value.ends_with(')'))
}

fn resolve_color(defs: &HashMap<String, String>, name: &str, depth: u8) -> Option<String> {
    if depth > 5 {
        return None;
    }
    let value = defs.get(name)?;
    match value.strip_prefix('@') {
        Some(other) => resolve_color(defs, other, depth + 1),
        None if is_css_color(value) => Some(value.trim().to_string()),
        None => None,
    }
}

/// The window color, the color of the contents, the accent and the text of a GTK
/// theme, when its stylesheets say them as plain colors.
fn stylesheet_colors(dir: &Path) -> Vec<String> {
    let mut defs: HashMap<String, String> = HashMap::new();
    for rel in ["gtk-3.0/gtk.css", "gtk-4.0/gtk.css", "gtk-3.0/colors.css", "gtk-4.0/colors.css"] {
        read_css(&dir.join(rel), &mut defs, 0);
    }
    let pick = |names: &[&str]| names.iter().find_map(|n| resolve_color(&defs, n, 0));
    let bg = pick(&["window_bg_color", "theme_bg_color", "bg_color", "base_color"]);
    let accent = pick(&["accent_bg_color", "theme_selected_bg_color", "selected_bg_color", "accent_color"]);
    let (Some(bg), Some(accent)) = (bg, accent) else { return Vec::new() };
    let view = pick(&["view_bg_color", "theme_base_color", "base_color"]).unwrap_or_else(|| bg.clone());
    let fg = pick(&["window_fg_color", "theme_fg_color", "fg_color", "text_color"]).unwrap_or_else(|| "#888888".to_string());
    vec![bg, view, accent, fg]
}

/// The themes GTK has inside itself (Adwaita, Adwaita-dark and HighContrast)
/// leave a stylesheet that says it is no longer used, or that only imports the
/// one that is inside GTK.
fn is_builtin_stub(dir: &Path) -> bool {
    fs::read_to_string(dir.join("gtk-3.0/gtk.css")).is_ok_and(|text| text.len() < 400 && (text.contains("no longer used") || text.contains("resource://")))
}

/// The colors of those themes, as GTK 3 draws them, to the nearest the
/// stylesheets of GTK let us say: window, contents, accent and text.
fn builtin_palette(id: &str) -> Vec<String> {
    let id = id.to_lowercase();
    let colors: [&str; 4] = if id.contains("highcontrastinverse") {
        ["#000000", "#000000", "#ffffff", "#ffffff"]
    } else if id.contains("highcontrast") {
        ["#ffffff", "#ffffff", "#000000", "#000000"]
    } else if id.contains("dark") {
        ["#353535", "#2d2d2d", "#15539e", "#eeeeec"]
    } else {
        ["#f6f5f4", "#ffffff", "#3584e4", "#2e3436"]
    };
    colors.iter().map(|c| c.to_string()).collect()
}

/// The four colors of the GTK theme in `dir`, or none when they cannot be read.
pub fn swatches(dir: &Path, id: &str) -> Vec<String> {
    let colors = stylesheet_colors(dir);
    if colors.is_empty() && is_builtin_stub(dir) {
        return builtin_palette(id);
    }
    colors
}
