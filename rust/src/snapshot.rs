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
use crate::cut_line;
use crate::layout;
use crate::new_project;
use crate::project;
use crate::shell::Page;
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
        | "06-lane" | "06-svg" | "06-effect-menu" => Some("Cut"),
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
        // A screen that wants the bench to open somewhere other than row 0 must say so BEFORE the
        // window is built: `bench_box` reads this once and paints from it, and a later call would
        // arrive after the heading was already drawn.
        if screen == "04-prompt-picker" {
            ui::set_bench_open_row(2);
        }
        let window = ui::build_window(app, &model, page);
        // The strip paints from this window's cut slot, so a snapshot must show the cut that lives in
        // the project it was asked to render — not whatever the working-copy folder happens to hold.
        // `--project` is that project, so its `cut/cut.json` is seeded over `build_window`'s
        // session-dir default here, before the frame is drawn. A missing or unreadable file is no cut
        // today (`cut::load` answers that with `Cut::default()`), which draws the empty hint.
        if let Some(loaded) = layout::Tree::new(&dir).ok().and_then(|tree| cut::load(&tree).ok()) {
            ui::seed_review_cut(&window, &loaded);
            // The camera rows are built from the cut too, and `build_window` drew them before this seed
            // landed -- so a snapshot of any Cut-page screen has to refresh them or they show the empty
            // pre-seed list. (F2.10: `camera-rows`, its name plates, lens badge and per-lane badges.)
            ui::refresh_camera_rows(&window);
        }
        // F2.10's own picture is TWO cameras at once (`spec/img/05-rows.png`), which no fixture on disk
        // carries -- the demo project's `cut/cut.json` is one camera in three clips. A snapshot asked for
        // by that image's name therefore lays the figure out here: camera 1 for 0-37, camera 2 overlapping
        // from 20 with the shift correction its name plate shows, one lane so there is a speaker badge and
        // a gutter switch, the line inside the overlap, and row 2 watched so the dashed outline is in shot.
        if screen == "05-rows" {
            let mut two_cameras = cut::Cut::default();
            two_cameras.segs = vec![
                cut::Seg { s: 0.0, e: 37.0, cam: 0, ..Default::default() },
                cut::Seg { s: 20.0, e: 37.0, cam: 1, ..Default::default() },
            ];
            two_cameras.lanes = vec![cut::Lane {
                name: "cam1".into(),
                src: "cam1".into(),
                at: 20.0,
                off: 0.0,
                dur: 17.0,
            }];
            two_cameras.shift.insert("cam1".to_string(), -19.0);
            ui::seed_review_cut(&window, &two_cameras);
            ui::set_line_position(&window, cut_line::LinePos { t: 25.0 });
            ui::press_watch_row(&window, 1);
            ui::refresh_camera_rows(&window);
        }
        // F2.11's own picture (`spec/img/05-fold.png`) is a WIDE dropped stretch folded to a seam. The
        // demo fixture cannot show one: its `cut/cut.json` keeps 0-9.5, 10-30 and 40-70 out of a tape
        // whose only filmed gap is 0.5 s = 2 px at the snapshot zoom, under `cut_fold::FOLD_MIN_PX`, so
        // no badge would be drawn at all. Laid out here instead -- the shared fixture file untouched:
        // the session filmed 0-120 as one take (that is what gives head/hole/tail anywhere to exist, see
        // `ui::page_recordings`), two clips are kept, the 30-40 hole is FOLDED so the bars meet at a
        // seam with a `+` on it, and the 60-120 tail is left open wearing a `-`.
        if screen == "05-cut" || screen == "05-fold" {
            let mut folded = cut::Cut::default();
            folded.segs = vec![
                cut::Seg { s: 10.0, e: 30.0, cam: 0, ..Default::default() },
                cut::Seg { s: 40.0, e: 60.0, cam: 0, ..Default::default() },
            ];
            ui::set_session_recordings(&[crate::timeline::Recording {
                base: "session-tape".to_string(),
                start: 0.0,
                end: 120.0,
            }]);
            ui::seed_review_cut(&window, &folded);
            // The badge table only exists once the badges have been DRAWN (`refresh_fold_badges` is what
            // records which gap each numbered button stands for), so a fold requested before that first
            // draw has no index to fold — press the fold BEFORE refreshing, then refresh once.
            ui::refresh_fold_badges(&window);
            // One fold already applied, so the shot shows BOTH states at once: `+` waiting on the seam
            // where 0:30-0:40 was folded away, `-` over the still-open tail.
            ui::press_fold_gap(&window, 1);
            ui::refresh_camera_rows(&window);
        }
        // F3.5's screen is the drawing form OPEN, so the shot must drive the same doors the widgets use
        // (`tests/svg_drawing_widgets.rs` fires exactly these) rather than poke state: a picture painted
        // from a shortcut would not be the picture the app draws.
        if screen == "06-svg" {
            // A line exists and points at 81 s, so the heading reads `SVG at 1:21` -- spelled by
            // `fx_svg::form_title`, never hardcoded here.
            ui::note_place(true);
            ui::set_line_position(&window, cut_line::LinePos { t: 81.0 });
            // File first, as S1/S2 order it; then a DRAWN box (not the click default) lands and opens the
            // form, which is the state the spec's FORM node describes.
            ui::svg_chosen(&window, Some("assets/logo.svg"));
            ui::svg_drag_ended(&window, (24.0, 24.0), (150.0, 96.0));
            // No Apply here: Apply closes the form, and the screen F3.5 names IS the form. The record it would
            // write is proven by `tests/svg_drawing_widgets.rs`; the shot shows what the user is being asked.
        }
        // F3.6's screen is the volume form OPEN (`spec/img/06-volume.png`), so the shot drives the same door
        // the dropdown row fires (`tests/volume_form_widgets.rs` presses exactly this) rather than poking state.
        if screen == "06-volume" {
            // A placed line 81 s in, so the heading reads the band the press opens with -- spelled by
            // `fx_volume::form_title`, never hardcoded here. Two seconds from the line is `LINE_SECONDS`.
            ui::note_place(true);
            ui::set_line_position(&window, cut_line::LinePos { t: 81.0 });
            let said = ui::press_volume_item(&window);
            // No Apply here either: Apply closes the form, and the screen §F3.6 names IS the form. The record
            // it would write, and that one ↶ takes back, is proven by `tests/volume_form_widgets.rs`.
            let _ = said;
        }
        // F3.7's screen is the label form OPEN (`spec/img/06-label.png`), so the shot drives the same door the
        // dropdown row fires (`tests/label_form_widgets.rs` presses exactly this) rather than poking state.
        if screen == "06-label" {
            // A placed line 81 s in, so the heading names that moment -- spelled by `fx_label::form_title`,
            // never hardcoded here. The Name field shows empty because the name is the question the form asks:
            // an unnamed mark is refused at Apply ("nothing is placed until then"), so prefilling one would put
            // a word on the lane nobody typed.
            ui::note_place(true);
            ui::set_line_position(&window, cut_line::LinePos { t: 81.0 });
            let said = ui::press_label_item(&window);
            // No Apply here either: Apply closes the form, and the screen §F3.7 names IS the form. The mark it
            // would write, and that one ↶ takes back, is proven by `tests/label_form_widgets.rs`.
            let _ = said;
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
            // §03's image is the dialog, not the window behind it, so the dialog is the subject here
            // too. `no_probe` answers every row, so the marks read ✗ — which is what makes this shot
            // worth looking at: the grid, the badge column and the Fetch models row all show at once.
            "03-settings" => {
                let shown = ui::settings::build(None, ui::settings::no_probe());
                shown.present();
                Some(shown.upcast())
            }
            // `spec/img/04-prompt-picker.png` pictures the row picker OPEN on a prompt, so this
            // shot is the same Prepare window with the bench started on the Describe row instead of
            // the User Context. The subject stays the window — the picker only makes sense in place,
            // beside the title and Reset it drives — and the row itself was requested above, before
            // `build_window`. Row 2 = "Describe", per `bench::ROWS`.
            "04-prompt-picker" => None,
            // F3.5's screen is the form itself, not the page behind it -- same reason `03-policy-form`
            // paints its dialog: the Cut page's own column collapses to a sliver headless (see the note on
            // the default size above), so painting the whole window would show empty grey instead of the
            // fields. The holder is resolved THROUGH the window that owns the live form.
            // Painted as the whole window: the form is part of the Cut page and cannot be reparented out of
            // it (GTK refuses a child that already has a parent), so the page itself must reach a readable
            // width -- done above, before `present()`, with a default size plus the pinned preview panel.
            _ => None,
        };
        if subject.is_none() {
            // Render-only: a snapshot window is never resized by a user, and the Cut page collapses into one
            // unreadable narrow column when nothing asks for width. `05-cut` reaches a wide layout today only
            // because its fold/camera-row refreshes happen to widen it; the `06-*` screens never do. A
            // default size before `present()` is enough to inspect the page -- no shell relayout.
            if screen_page(&screen) == Some("Cut") {
                window.set_default_size(1240, 900);
            }
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
