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
        .build();
    app.connect_activate(|app| {
        let model = naivepost::project::Project::default();
        naivepost::ui::build_window(app, &model, "").present();
    });
    app.run_with_args::<String>(&[]).into()
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
