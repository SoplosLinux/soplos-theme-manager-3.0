mod commands;
mod system_theme;

/// Privileged mode: pkexec starts the program again as `--run-ops <file>` and
/// main() hands over here before any window exists.
pub fn run_ops_file(path: &str) -> i32 {
    commands::privileged::run_ops_file(path)
}

// A .sth file given on the command line (a double click in the file manager).
fn sth_in(args: &[String]) -> Option<String> {
    args.iter().skip(1).find(|a| a.to_lowercase().ends_with(".sth") && std::path::Path::new(a).is_file()).cloned()
}

// The file that opened the program, until the window asks for it.
static PENDING: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[tauri::command]
fn take_pending_bundle() -> Option<String> {
    PENDING.lock().ok().and_then(|mut p| p.take())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Some(file) = sth_in(&std::env::args().collect::<Vec<_>>()) {
        if let Ok(mut pending) = PENDING.lock() {
            *pending = Some(file);
        }
    }
    tauri::Builder::default()
        // The single instance plugin goes first: it has to stop a second
        // launch before anything else runs. Two windows must never rewrite
        // the same desktop settings at once.
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            use tauri::{Emitter, Manager};
            // A relative path is relative to where the second launch was made.
            let args: Vec<String> = args.into_iter().map(|a| if std::path::Path::new(&a).is_relative() { std::path::Path::new(&cwd).join(&a).to_string_lossy().to_string() } else { a }).collect();
            if let Some(file) = sth_in(&args) {
                let _ = app.emit("open-bundle", file);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            system_theme::get_system_theme,
            take_pending_bundle,
            commands::bundles::list_saved_themes,
            commands::bundles::remove_saved_theme,
            commands::bundles::save_current_theme,
            commands::bundles::apply_saved_theme,
            commands::bundles::rename_saved_theme,
            commands::bundles::screenshot_available,
            commands::bundle_io::export_saved_theme,
            commands::bundle_io::install_bundle,
            commands::bundle_io::bundle_info,
            commands::bundle_io::restore_base_themes,
            commands::bundle_io::seed_first_run,
            commands::themes::list_themes,
            commands::themes::theme_samples,
            commands::themes::get_active_theme,
            commands::themes::set_active_theme,
            commands::themes::theme_package,
            commands::themes::delete_theme,
            commands::packages::inspect_package,
            commands::packages::discard_package,
            commands::packages::install_candidates,
            commands::avatar::user_avatar,
            commands::avatar::set_user_avatar,
            commands::avatar::desktop_wallpaper,
            commands::wallpapers::default_wallpaper_folder,
            commands::wallpapers::scan_wallpapers,
            commands::wallpapers::set_wallpaper,
            commands::wallpapers::install_wallpapers,
            commands::wallpapers::make_thumbnail,
            commands::dock::dock_active,
            commands::dock::dock_list,
            commands::dock::dock_pinned,
            commands::dock::dock_available,
            commands::dock::dock_save,
            commands::lightdm::lightdm_read,
            commands::lightdm::lightdm_fonts,
            commands::lightdm::lightdm_write,
            commands::lightdm::allow_image,
            commands::panel::panel_list,
            commands::panel::panel_read,
            commands::panel::panel_available,
            commands::panel::panel_apply,
            commands::panel::panel_apply_many,
            commands::panel::panel_set_order,
            commands::panel::panel_remove_plugin,
            commands::panel::panel_add_plugin,
            commands::panel::panel_preferences,
            commands::panel::panel_new,
            commands::panel::panel_delete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Soplos Theme Manager");
}
