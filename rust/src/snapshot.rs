//! `--snapshot <screen> --project <fixture dir> --out <file.png>`.
//!
//! Renders one screen of the window without a host display: the caller runs it
//! under `xvfb-run` with `GSK_RENDERER=cairo` (see the justfile). The screens are
//! named after the spec's image files (`03-window`, `05-cut`, …) so every spec
//! image has a reproducible counterpart.

use std::path::Path;
use std::process::ExitCode;

use adw::prelude::*;
use glib::translate::ToGlibPtr;
use gtk4 as gtk;

use crate::cut;
use crate::layout;
use crate::new_project;
use crate::project;
use crate::ui;

/// Screen names the window can render: the spec's images, mapped onto a page.
fn screen_page(screen: &str) -> Option<&'static str> {
    match screen {
        "03-window" | "03-settings" | "03-sources" | "03-new-confirm" | "03-policy-form" => {
            Some("Prepare")
        }
        "04-prepare" | "04-prompt-picker" => Some("Prepare"),
        "05-cut" | "05-add" | "05-fold" | "05-rows" | "05-split" | "05-trim" | "05-remove"
        | "05-lanes" => Some("Cut"),
        "06-preview" | "06-zoom" | "06-speed" | "06-text" | "06-volume" | "06-label"
        | "06-lane" | "06-effect-menu" => Some("Cut"),
        "07-narrate" | "07-take" => Some("Narrate"),
        "08-produce" | "08-words" => Some("Produce"),
        _ => None,
    }
}

/// Paint `widget` into a PNG. Called once the first frame is ready.
///
/// A cairo surface, not `gsk::Renderer::render_texture`: that one needs a realized
/// renderer, i.e. a native surface, and the headless run has none to offer. Any widget will do —
/// the confirm dialog's screen paints the dialog, not the window behind it.
fn write_png(widget: &gtk::Widget, out: &Path) -> Result<(), String> {
    let width = u32::try_from(widget.width()).unwrap_or(1).max(1);
    let height = u32::try_from(widget.height()).unwrap_or(1).max(1);

    let paintable = gtk::WidgetPaintable::new(Some(widget.upcast_ref::<gtk::Widget>()));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, width as f64, height as f64);
    let node = snapshot.to_node().ok_or("window produced no render node")?;

    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width as i32, height as i32)
        .map_err(|err| err.to_string())?;
    {
        let cr = cairo::Context::new(&surface).map_err(|err| err.to_string())?;
        // gsk_render_node_draw is the one call that paints a render node onto an
        // arbitrary cairo context; no realized renderer needed.
        unsafe { gsk::ffi::gsk_render_node_draw(node.as_ref().to_glib_none().0, cr.to_raw_none()) };
    }
    let mut file = std::fs::File::create(out).map_err(|err| err.to_string())?;
    surface.write_to_png(&mut file).map_err(|err| err.to_string())
}

pub fn run(screen: &str, dir: &Path, out: &Path) -> Result<(), String> {
    let page = screen_page(screen).ok_or_else(|| format!("unknown screen {screen}"))?;
    if !dir.join(project::PROJECT_FILE).is_file() {
        return Err(format!("no {} in {}", project::PROJECT_FILE, dir.display()));
    }
    let model = project::load(dir)?;

    gtk::init().map_err(|err| err.to_string())?;
    // Non-registering: a snapshot must not claim the app id or wait on a session bus.
    let app = gtk::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let out = out.to_path_buf();
    // Copied for the closures below: `screen` decides which widget gets painted and a borrow of
    // `run`'s argument cannot outlive this function's body.
    let screen = screen.to_string();
    // The dialog names the open project, which is the folder the snapshot was asked to render.
    let dir = dir.to_path_buf();
    // Built when the app has no windows yet: they are added at startup, so the
    // handler runs after GTK has emitted it.
    app.connect_activate(move |app| {
        let window = ui::build_window(app, &model, page);
        // The strip paints from this window's cut slot, so a snapshot must show the cut that lives in
        // the project it was asked to render — not whatever the working-copy folder happens to hold.
        // `--project` is that project, so its `cut/cut.json` is seeded over `build_window`'s
        // session-dir default here, before the frame is drawn. A missing or unreadable file is no cut
        // today (`cut::load` answers that with `Cut::default()`), which draws the empty hint.
        if let Some(loaded) = layout::Tree::new(&dir).ok().and_then(|tree| cut::load(&tree).ok()) {
            ui::seed_review_cut(&window, &loaded);
        }
        // F0.8's screen is the confirmation, not the window behind it: the dialog is what the spec
        // image shows, so it is what gets painted. The fixture is a named project folder, which is
        // why its body carries the "stays on disk as it is" paragraph.
        let subject: Option<gtk::Widget> = match screen.as_str() {
            // No parent: a transient of the window is kept alive by it, and this run ends when the
            // last window closes. Painted on its own, which is how the spec image shows it.
            "03-new-confirm" => {
                let shown = ui::new_project_confirm(None, &new_project::confirm_detail(&dir));
                shown.set_default_size(560, 240);
                shown.present();
                Some(shown.upcast())
            }
            // F0.7's screen is the policy form itself, not the window behind it: §03's image shows
            // only the table of fields, so this paints only the table. No parent for the same reason
            // as the confirmation above — the run ends when the last window closes.
            "03-policy-form" => {
                let shown = ui::policy_form::build(None, &model.policy);
                shown.set_default_size(720, 420);
                shown.present();
                Some(shown.upcast())
            }
            _ => None,
        };
        if subject.is_none() {
            window.present();
        }
        let painted = subject.clone().unwrap_or_else(|| window.clone().upcast());
        let out = out.clone();
        let app = app.clone();
        // One more cycle after the map: the map happens before the first frame is drawn. The paint
        // and the quit are synchronous here, so no second mapped window is ever waited on.
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            match write_png(&painted, &out) {
                Ok(()) => (),
                Err(err) => {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
            }
            app.quit();
        });
    });
    // The handler closes the last window, which quits the main loop. A nested
    // block_on here would recurse on the context that already runs it.
    app.run_with_args::<String>(&[]);

    OUT.with(|e| e.borrow_mut().take()).map_or(Ok(()), Err)
}

thread_local! {
    static OUT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub fn exit_code(err: &str) -> ExitCode {
    eprintln!("snapshot: {err}");
    ExitCode::FAILURE
}
