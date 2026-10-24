mod app;
mod emojis;
mod fold;
mod glyphs;
mod instance;
mod settings;
mod x11;

slint::include_modules!();

fn main() {
    // `fastmoji` shows the picker, `fastmoji --toggle` toggles it (use this for a Wayland keybinding),
    // `fastmoji --background` only starts the daemon (autostart).
    let args: Vec<String> = std::env::args().skip(1).collect();
    const USAGE: &str = "usage: fastmoji [--toggle | --background | --help | --version]";
    for arg in &args {
        match arg.as_str() {
            "--toggle" | "--background" => {}
            "-h" | "--help" => return println!("{USAGE}"),
            "-V" | "--version" => return println!("fastmoji {}", env!("CARGO_PKG_VERSION")),
            _ => {
                eprintln!("unknown argument {arg:?}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let (command, show) = if has("--toggle") {
        ("toggle", true)
    } else if has("--background") {
        ("ping", false)
    } else {
        ("show", true)
    };

    // A running instance does the work; this process just forwards the command.
    if instance::send(command) {
        return;
    }

    // Autostart with residency turned off: there is nothing to keep running.
    if has("--background") && x11::is_x11_session() && !settings::Settings::load().keep_resident {
        return;
    }

    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .select()
        .expect("cannot start the Slint software renderer");
    app::run(show, command);
}
