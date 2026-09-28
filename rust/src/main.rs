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
             \x20 naivepost --snapshot <screen> [--project <dir>] [--out <file.png>]\n\
             \x20     Render one screen to a PNG without a user, for checking against spec/img.\n\
             \x20     Screens: 03-window, 04-prepare, 05-cut, 06-preview, 07-narrate, 08-produce, …\n\
             \n\
             Both need a display; headless runs use `just snapshot <screen>` from rust/, which wraps\n\
             the call in xvfb-run with GSK_RENDERER=cairo."
        );
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "--snapshot") {
        return match parse_snapshot(&args)
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
    app.connect_activate(|app| open_at_start(app, &[]));
    // Only the first file is opened — one window, one project — and the precedence behind that is
    // startup::decide's (F0.6), not anything spelled out here.
    app.connect_open(|app, files, _hint| {
        let handed: Vec<PathBuf> = files.iter().filter_map(|file| file.path()).collect();
        open_at_start(app, &handed);
    });
    app.run_with_args::<String>(&[]).into()
}

/// Which project this launch opens and shows (F0.6), decided by [`naivepost::startup::decide`].
///
/// The root is the folder the program was started from, which is what F0.6's S2 keys its remembered
/// project by and where S3 looks for the working copy. A blank session loads as an empty project
/// and writes nothing; a folder that cannot be read opens blank too, for now — saying *why* on the
/// status line is F0.9 (Open a project), along with remembering what this launch opened
/// (`rememberProject`) and taking over a handed `naivepost.json` as its folder. New Project is
/// F0.8's.
fn open_at_start(app: &adw::Application, handed: &[PathBuf]) {
    let root = std::env::current_dir().unwrap_or_default();
    // The environment is read here and handed to settings.rs as strings, which is how that module
    // keeps its own tests off the real `$HOME`. No config folder at all means no settings.
    let paths = settings_paths();
    // §6's "legacy `<root>/llm.conf`, migrated once": a machine that has been cutting for months
    // keeps its endpoints without retyping them. A migration that cannot be done must not cost the
    // launch either, which is the same rule the read below states for an unreadable file.
    if let Some(paths) = paths.as_ref() {
        let _ = naivepost::settings::migrate_legacy(paths, &root);
    }
    // A settings file that cannot be read must not cost the user their session: the fallback opens
    // the working copy like any other machine's, and the broken file stays for them to look at.
    let conf = paths
        .as_ref()
        .and_then(|paths| naivepost::settings::read(paths).ok())
        .unwrap_or_default();

    let project = match naivepost::startup::decide(&root, &conf, handed) {
        // A blank session is no file at all, so there is nothing to read.
        naivepost::startup::Opened::Blank { .. } => naivepost::project::Project::default(),
        // The other three name a folder to open, and all three are opened the same way.
        opened => {
            let path = match &opened {
                naivepost::startup::Opened::Desktop { path }
                | naivepost::startup::Opened::Remembered { path }
                | naivepost::startup::Opened::Session { path } => path,
                // Unreachable: `Blank` was taken above. A blank session is what an unreadable
                // folder opens as too, which F0.9 replaces with a reason on the status line.
                naivepost::startup::Opened::Blank { path } => path,
            };
            naivepost::project::load(path).unwrap_or_default()
        }
    };
    // "" is Prepare, the page a session always starts on (§1).
    naivepost::ui::build_window(app, &project, "").present();
}

/// Where this machine keeps `llm.conf`, or `None` when there is nowhere to keep anything.
fn settings_paths() -> Option<naivepost::settings::Paths> {
    let var = |name: &str| std::env::var(name).ok();
    // `data_home` has no environment variable of its own; settings.rs documents it as the caller's
    // override, so it stays `None` and `$XDG_DATA_HOME`/`$HOME` answer for the data folder.
    naivepost::settings::paths_from(
        var("XDG_CONFIG_HOME").as_deref(),
        var("HOME").as_deref(),
        None,
        var("XDG_DATA_HOME").as_deref(),
    )
}

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
