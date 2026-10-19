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
        // The three trees belong in the help text because "where does this change go?" is the first
        // question a new checkout raises, and AGENT.md's answer is short enough to ship verbatim.
        let trees: Vec<&str> = naivepost::tree_roles::Role::all()
            .iter()
            .map(|role| role.sentence())
            .collect();
        println!(
            "naivepost — a cut-and-narrate editor for sessions already recorded.\n\
             \n\
             Usage:\n\
             \x20 naivepost [project.naivepost]\n\
             \x20     Open the window, optionally over one project folder (the desktop can\n\
             \x20     hand one over too). With no argument the last opened project is used.\n\
             \x20 naivepost --snapshot <screen> [--project <dir>] [--out <file.png>]\n\
             \x20     Render one screen to a PNG without a user, for checking against spec/img.\n\
             \x20     Screens: 03-window, 04-prepare, 05-cut, 06-preview, 07-narrate, 08-produce, …\n\
             \x20 naivepost --trees\n\
             \x20     Print which tree is which (app, prototype, spec) and where new work goes.\n\
             \x20     With paths, says which tree each one belongs to.\n\
             \n\
             Both need a display; headless runs use `just snapshot <screen>` from rust/, which wraps\n\
             the call in xvfb-run with GSK_RENDERER=cairo.\n\
             \n\
             Trees:\n\
             {}",
            trees.join("\n")
        );
        return ExitCode::SUCCESS;
    }

    // Which tree is which (AGENT.md), printed by the program rather than by a README: answered here,
    // before any display is asked for, so it works in a container that has none. With no path it prints
    // the three trees; with paths it answers the same rule for each one, which is the question that
    // actually comes up when somebody is holding a file: "can new work go here?".
    if let Some(at) = args.iter().position(|a| a == "--trees") {
        let asked: Vec<&String> = args[at + 1..]
            .iter()
            .take_while(|arg| !arg.starts_with("--"))
            .collect();
        if asked.is_empty() {
            for role in naivepost::tree_roles::Role::all() {
                println!("{}", role.sentence());
            }
        } else {
            for path in asked {
                match naivepost::tree_roles::role_of(std::path::Path::new(path)) {
                    Some(role) => println!(
                        "{path}: {} (new work here: {})",
                        role.sentence(),
                        role.takes_new_work()
                    ),
                    None => println!(
                        "{path}: in none of the three trees -- not the app, the prototype or the spec"
                    ),
                }
            }
        }
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "--snapshot") {        return match parse_snapshot(&args)
            .and_then(|(screen, dir, out)| naivepost::snapshot::run(&screen, &dir, &out))
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
fn parse_snapshot(args: &[String]) -> Result<(String, PathBuf, PathBuf), String> {
    let mut screen = None;
    let mut dir = None;
    let mut out = None;
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
            _ => {}
        }
    }
    let screen = screen.ok_or("missing --snapshot <screen>")?;
    let dir = PathBuf::from(dir.ok_or("missing --project <dir>")?);
    let out = PathBuf::from(out.ok_or("missing --out <file.png>")?);
    Ok((screen, dir, out))
}
