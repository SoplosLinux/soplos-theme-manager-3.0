// The small pictures that show what an icon or a cursor theme looks like: the
// folder of an icon theme and the arrow of a cursor theme. They are sent to
// the window as data addresses, so no file of the theme has to be reachable
// from the web page.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::fs;
use super::icons::IconFinder;
use std::path::Path;

// The size the pictures are shown at, in pixels.
const PREVIEW_SIZE: i64 = 28;
// A folder icon bigger than this is not worth sending for a picture this small.
const MAX_ICON_BYTES: u64 = 300 * 1024;

// The two conventional names of the plain arrow of a cursor theme.
const CURSOR_NAMES: [&str; 2] = ["left_ptr", "default"];

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

/// A picture file as a data address. None if it is bigger than `max_bytes`, or
/// of a kind a web page cannot show.
pub fn file_data_url(file: &Path, max_bytes: u64) -> Option<String> {
    let mime = match file.extension()?.to_string_lossy().to_lowercase().as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => return None,
    };
    if fs::metadata(file).ok()?.len() > max_bytes {
        return None;
    }
    Some(data_url(mime, &fs::read(file).ok()?))
}

// ── Icon themes ──────────────────────────────────────────────────────────────

/// The folder icon of an icon theme, as a data address.
pub fn icon_theme_preview(theme: &str) -> Option<String> {
    let file = IconFinder::new().find(theme, "folder", PREVIEW_SIZE)?;
    file_data_url(&file, MAX_ICON_BYTES)
}

// ── Cursor themes ────────────────────────────────────────────────────────────

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

// Reads the plain arrow out of an Xcursor file: a small header, a table of
// images of several sizes, each stored as premultiplied ARGB32. Returns the
// width, the height and straight RGBA pixels of the image closest to the size
// wanted.
fn parse_xcursor(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if data.get(0..4)? != b"Xcur" {
        return None;
    }
    let header_size = u32_at(data, 4)? as usize;
    let count = u32_at(data, 12)? as usize;

    let mut best: Option<(i64, usize)> = None;
    for i in 0..count {
        let at = header_size + i * 12;
        let (kind, subtype, position) = (u32_at(data, at)?, u32_at(data, at + 4)?, u32_at(data, at + 8)?);
        // 0xfffd0002 is an image chunk; its subtype is the nominal size.
        if kind == 0xfffd_0002 {
            let distance = (subtype as i64 - PREVIEW_SIZE).abs();
            if best.map_or(true, |(d, _)| distance < d) {
                best = Some((distance, position as usize));
            }
        }
    }
    let (_, position) = best?;

    let chunk_header = u32_at(data, position)? as usize;
    let (width, height) = (u32_at(data, position + 16)?, u32_at(data, position + 20)?);
    if width == 0 || height == 0 || width > 256 || height > 256 {
        return None;
    }
    let pixels = data.get(position + chunk_header..position + chunk_header + (width * height * 4) as usize)?;

    // The file holds B, G, R, A premultiplied; a picture wants straight R, G, B, A.
    let mut rgba = Vec::with_capacity(pixels.len());
    for px in pixels.chunks_exact(4) {
        let (b, g, r, a) = (px[0] as u32, px[1] as u32, px[2] as u32, px[3] as u32);
        let un = |c: u32| if a == 0 { 0 } else { (c * 255 / a).min(255) as u8 };
        rgba.extend_from_slice(&[un(r), un(g), un(b), a as u8]);
    }
    Some((width, height, rgba))
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// A cursor of a theme, drawn from its own Xcursor file, as a data address:
/// the first of `names` that the theme has.
fn cursor_sample(theme_dir: &Path, names: &[&str]) -> Option<String> {
    for name in names {
        let file = theme_dir.join("cursors").join(name);
        if !file.is_file() {
            continue;
        }
        let Ok(data) = fs::read(&file) else { continue };
        if let Some((width, height, rgba)) = parse_xcursor(&data) {
            if let Some(png) = encode_png(width, height, &rgba) {
                return Some(data_url("image/png", &png));
            }
        }
    }
    None
}

/// The arrow of a cursor theme, drawn from its own Xcursor file, as a data
/// address.
pub fn cursor_theme_preview(theme_dir: &Path) -> Option<String> {
    cursor_sample(theme_dir, &CURSOR_NAMES)
}

// ── Samples for the cards ────────────────────────────────────────────────────

// The icons that say what an icon theme looks like: a folder, the home, a
// terminal, the settings, a browser and a text editor.
const ICON_SLOTS: [&[&str]; 6] = [
    &["folder"],
    &["user-home", "go-home"],
    &["utilities-terminal", "terminal"],
    &["preferences-system", "systemsettings"],
    &["internet-web-browser", "web-browser", "firefox"],
    &["accessories-text-editor", "text-editor"],
];

// The cursors that say what a cursor theme looks like: the arrow, the hand,
// the text bar and the busy one.
const CURSOR_SLOTS: [&[&str]; 4] = [&["left_ptr", "default", "arrow"], &["pointer", "hand2", "hand1"], &["text", "xterm", "ibeam"], &["wait", "watch", "progress"]];

// The size the icons of a card are shown at, in pixels.
const SAMPLE_ICON_SIZE: i64 = 36;

/// Six icons of an icon theme, as data addresses; the ones it does not have
/// are left out.
pub fn icon_theme_samples(theme: &str) -> Vec<String> {
    let mut finder = IconFinder::new();
    ICON_SLOTS
        .iter()
        .filter_map(|names| names.iter().find_map(|n| finder.find(theme, n, SAMPLE_ICON_SIZE).and_then(|f| file_data_url(&f, MAX_ICON_BYTES))))
        .collect()
}

/// Four cursors of a cursor theme, as data addresses; the ones it does not
/// have are left out.
pub fn cursor_theme_samples(theme_dir: &Path) -> Vec<String> {
    CURSOR_SLOTS.iter().filter_map(|names| cursor_sample(theme_dir, names)).collect()
}
