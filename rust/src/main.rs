//! `--snapshot <screen> --project <fixture dir> --out <file.png>` renders one
//! screen headlessly; otherwise the app runs normally.
//!
//! Both paths need a display: inside this container that means `xvfb-run` with
//! `GSK_RENDERER=cairo`, which the justfile sets (see `just snapshot`).

use std::path::PathBuf;
use std::process::ExitCode;

use adw::prelude::*;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    // GTK's own `--help` only prints after a display opens, and this program is routinely started in a
    // container that has none, so the two modes are answered here instead: before any display is asked
    // for, and on stdout with exit 0.
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "naivepost — a cut-and-narrate editor for sessions already recorded.\n\
             \n\
             Usage:\n\
             \x20 naivepost [project.naivepost]\n\
             \x20     Open the window, optionally over one project folder (the desktop can\n\
             \x20     hand one over too). With no argument the last opened project is used.\n\
             \x20 naivepost --snapshot <screen> [--project <dir>] [--out <file.png>] [--shot-on <event>]\n\
             \x20     Render one screen to a PNG without a user, for checking against spec/img.\n\
             \x20     The picture is taken at <event>: `ready` (default), when no shown widget still\n\
             \x20     owes a setup step, or `open:<widget name>`, when that widget is shown too.\n\
             \x20     Debug builds only.\n\
             \x20     Screens: 03-window, 04-prepare, 05-cut, 06-preview, 07-narrate, 08-produce, …\n\
             \n\
             Both need a display; headless runs use `just snapshot <screen>` from rust/, which wraps\n\
             the call in xvfb-run with GSK_RENDERER=cairo.\n",
        );
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "--snapshot") {
        if !cfg!(debug_assertions) {
            eprintln!("--snapshot is a debug-build tool: run it through `just snapshot <screen>`");
            return ExitCode::FAILURE;
        }
        return match parse_snapshot(&args)
            .and_then(|(screen, dir, out, on)| naivepost::snapshot::run(&screen, &dir, &out, on))
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => naivepost::snapshot::exit_code(&err),
        };
    }

    let app = adw::Application::builder()
        .application_id(naivepost::ui::APP_ID)
        // HANDLES_OPEN is what lets a double-click reach `connect_open` at all: without the flag
        // gio treats a file argument as an error and refuses to start. The desktop hands over
        // `*.naivepost` folders because of the MIME package settings::install_mime_package writes.
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_activate(|app| naivepost::ui::launch::open_at_start(app, &[]));
    // Only the first file is opened — one window, one project — and the precedence behind that is
    // startup::decide's (F0.6), not anything spelled out here.
    app.connect_open(|app, files, _hint| {
        let handed: Vec<PathBuf> = files.iter().filter_map(|file| file.path()).collect();
        naivepost::ui::launch::open_at_start(app, &handed);
    });
    app.run_with_args::<String>(&[]).into()
}

// The launch itself lives in [`naivepost::ui::launch`] (F0.6): which project to open, what to leave
// remembered for the next start, and §7's desktop registration. `main` only builds the application
// and hands gio the two doors above.
fn parse_snapshot(
    args: &[String],
) -> Result<(String, PathBuf, PathBuf, naivepost::snapshot_ready::ShotOn), String> {
    let mut screen = None;
    let mut dir = None;
    let mut out = None;
    let mut on = naivepost::snapshot_ready::ShotOn::Ready;
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        let mut value = |name: &str| -> Result<String, String> {
            rest.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--snapshot" => screen = Some(value("--snapshot")?),
            "--project" => dir = Some(value("--project")?),
            "--out" => out = Some(value("--out")?),
            "--shot-on" => on = naivepost::snapshot_ready::ShotOn::parse(&value("--shot-on")?)?,
            _ => {}
        }
    }
    let screen = screen.ok_or("missing --snapshot <screen>")?;
    let dir = PathBuf::from(dir.ok_or("missing --project <dir>")?);
    let out = PathBuf::from(out.ok_or("missing --out <file.png>")?);
    Ok((screen, dir, out, on))
}
