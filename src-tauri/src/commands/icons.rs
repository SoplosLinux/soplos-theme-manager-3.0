// Finding the file of an icon by its name, the way a desktop does: look in the
// icon theme, then in the themes it inherits from, then in hicolor, and pick the
// folder whose size is closest to the one wanted. It serves the folder of an
// icon theme and the icons of the applications of the dock.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// Folders of icons that are searched, in this order.
fn search_dirs() -> Vec<PathBuf> {
    let home = super::themes::home();
    vec![home.join(".icons"), home.join(".local/share/icons"), PathBuf::from("/usr/share/icons"), PathBuf::from("/usr/local/share/icons")]
}

// One folder of a theme with the rules that say which sizes it serves.
struct SizedDir {
    path: PathBuf,
    size: i64,
    min: i64,
    max: i64,
    kind: String,
    threshold: i64,
}

impl SizedDir {
    // How far this folder is from the size wanted, by the rules of the
    // freedesktop icon theme specification.
    fn distance(&self, wanted: i64) -> i64 {
        match self.kind.as_str() {
            "Fixed" => (wanted - self.size).abs(),
            "Scalable" => {
                if wanted < self.min {
                    self.min - wanted
                } else if wanted > self.max {
                    wanted - self.max
                } else {
                    0
                }
            }
            _ => {
                if (wanted - self.size).abs() <= self.threshold {
                    0
                } else {
                    (wanted - self.size).abs()
                }
            }
        }
    }
}

struct ThemeData {
    dirs: Vec<SizedDir>,
    inherits: Vec<String>,
}

type Sections = HashMap<String, HashMap<String, String>>;

fn parse_ini(path: &Path) -> Sections {
    let mut sections = Sections::new();
    let Ok(text) = fs::read_to_string(path) else { return sections };
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name.to_string();
        } else if let Some((key, value)) = line.split_once('=') {
            sections.entry(current.clone()).or_default().insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    sections
}

fn load_theme(name: &str) -> Option<ThemeData> {
    let mut dirs = Vec::new();
    let mut inherits: Vec<String> = Vec::new();
    let mut found = false;
    for base in search_dirs() {
        let theme_dir = base.join(name);
        let sections = parse_ini(&theme_dir.join("index.theme"));
        let Some(main) = sections.get("Icon Theme") else { continue };
        found = true;
        if inherits.is_empty() {
            inherits = main.get("Inherits").map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
        }
        for dir in main.get("Directories").map(String::as_str).unwrap_or("").split(',') {
            let dir = dir.trim();
            let Some(section) = sections.get(dir) else { continue };
            let number = |key: &str, default: i64| section.get(key).and_then(|v| v.parse().ok()).unwrap_or(default);
            let size = number("Size", 0);
            dirs.push(SizedDir {
                path: theme_dir.join(dir),
                size,
                min: number("MinSize", size),
                max: number("MaxSize", size),
                kind: section.get("Type").cloned().unwrap_or_else(|| "Threshold".to_string()),
                threshold: number("Threshold", 2),
            });
        }
    }
    found.then_some(ThemeData { dirs, inherits })
}

/// Looks icons up by name. It remembers the themes it has read, so finding
/// hundreds of icons reads each theme only once.
pub struct IconFinder {
    themes: HashMap<String, Option<ThemeData>>,
}

impl IconFinder {
    pub fn new() -> Self {
        IconFinder { themes: HashMap::new() }
    }

    fn theme(&mut self, name: &str) -> Option<&ThemeData> {
        self.themes.entry(name.to_string()).or_insert_with(|| load_theme(name)).as_ref()
    }

    fn find_in(&mut self, theme: &str, icon: &str, size: i64, depth: u8, visited: &mut Vec<String>) -> Option<PathBuf> {
        if depth > 6 || visited.iter().any(|v| v == theme) {
            return None;
        }
        visited.push(theme.to_string());
        let (best, parents) = {
            let data = self.theme(theme)?;
            let mut best: Option<(i64, bool, PathBuf)> = None;
            for dir in &data.dirs {
                for (ext, is_svg) in [("svg", true), ("png", false)] {
                    let file = dir.path.join(format!("{icon}.{ext}"));
                    if !file.is_file() {
                        continue;
                    }
                    let distance = dir.distance(size);
                    // At the same distance a vector picture wins: it scales without loss.
                    let better = match &best {
                        None => true,
                        Some((d, svg, _)) => distance < *d || (distance == *d && is_svg && !svg),
                    };
                    if better {
                        best = Some((distance, is_svg, file));
                    }
                }
            }
            (best, data.inherits.clone())
        };
        if let Some((_, _, file)) = best {
            return Some(file);
        }
        for parent in parents {
            if let Some(found) = self.find_in(&parent, icon, size, depth + 1, visited) {
                return Some(found);
            }
        }
        None
    }

    /// The file of the icon `name` for `size` pixels, looking in `theme`, in the
    /// themes it inherits from, in hicolor and in /usr/share/pixmaps.
    pub fn find(&mut self, theme: &str, name: &str, size: i64) -> Option<PathBuf> {
        let mut visited = Vec::new();
        if let Some(found) = self.find_in(theme, name, size, 0, &mut visited) {
            return Some(found);
        }
        if let Some(found) = self.find_in("hicolor", name, size, 0, &mut visited) {
            return Some(found);
        }
        for ext in ["svg", "png"] {
            let file = Path::new("/usr/share/pixmaps").join(format!("{name}.{ext}"));
            if file.is_file() {
                return Some(file);
            }
        }
        None
    }
}
