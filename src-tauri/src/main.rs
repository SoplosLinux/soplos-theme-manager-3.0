// The entry point hands over to the library so tests can reuse it.

fn main() {
    // Privileged mode: pkexec starts this same program again, under the polkit
    // action of the package, to do what needs the administrator. It has to be
    // checked before Tauri starts, because it never opens a window.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--run-ops" {
        std::process::exit(soplos_theme_manager_lib::run_ops_file(&args[2]));
    }

    // The DMA-BUF renderer of WebKitGTK gives protocol errors on some drivers
    // and blank windows on virtual GPUs, so it is off unless the user turned
    // it on.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    soplos_theme_manager_lib::run();
}
