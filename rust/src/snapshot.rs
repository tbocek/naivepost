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
use crate::ui;

/// The bar's ▶ as the picture sees it: its icon, its tooltip, and whether ⏹ can be pressed — the three
/// properties `paint_run_bar` is the only writer of, read back off the widgets rather than recomputed
/// from [`crate::run::controls`] so a missed repaint shows up as a wrong face here.
fn face_of(window: &adw::ApplicationWindow) -> (String, String, bool) {
    let play = ui::play_button(window).expect("the run bar has a ▶");
    (
        play.icon_name().unwrap_or_default().to_string(),
        play.tooltip_text().unwrap_or_default().to_string(),
        ui::stop_button(window).expect("the run bar has a ⏹").is_sensitive(),
    )
}

/// Draw the run bar's ▶ at a legible size into a PNG on its own. A whole-window shot cannot show which
/// glyph the button wears: at 1334 px wide the icon is ~16 px and reads as a blue blob whether it is ▶ or
/// ⏸, so the face this screen exists to record is invisible in it. The widget has to be laid out first --
/// one painted inside a window that was never mapped produces no render node -- hence its own top level,
/// the way `03-new-confirm` paints a dialog rather than the window behind it.
fn write_face_png(face: &gtk::Button, out: &Path) -> Result<(), String> {
    // A copy, not the bar's own button: reparenting a widget out of a mapped window leaves it with no
    // render node, which is what made an earlier attempt of this print "window produced no render node".
    // The copy carries the same icon and tooltip because those ARE the face being recorded.
    let glyph = gtk::Button::from_icon_name(face.icon_name().unwrap_or_default().as_str());
    if let Some(tip) = face.tooltip_text() {
        glyph.set_tooltip_text(Some(tip.as_str()));
    }
    for class in face.css_classes() {
        glyph.add_css_class(class.as_str());
    }
    let holder = gtk::Window::new();
    holder.set_default_size(96, 72);
    holder.set_child(Some(&glyph));
    holder.present();
    // The map must happen before the frame is taken; `write_png` reads the widget's allocated size.
    let context = glib::MainContext::default();
    for _ in 0..64 {
        while context.pending() {
            context.iteration(false);
        }
    }
    let result = write_png(glyph.upcast_ref(), out);
    holder.close();
    result
}

/// Open the shell's log and paint what has been logged, through the same door a run repaints it from.
///
/// `paint_progress` is the only thing that expands the expander AND fills its body — it reads openness
/// off the run bar, so a snapshot with no run open cannot get an open log any other way. This hands it
/// the window's own widgets and the idle queue, asking for the one state a real run shows: expanded.
fn show_the_log(window: &adw::ApplicationWindow) -> Result<(), String> {
    let progress = ui::progress_bar(window).ok_or("the shell has no progress bar")?;
    let expander = ui::log_expander(window).ok_or("the shell has no log expander")?;
    let view = log_view(window).ok_or("the shell's log has no text view")?;
    let queue = ui::run_queue(window).ok_or("the shell has no run queue")?;
    let status = ui::find_status(window.upcast_ref()).ok_or("the shell has no status line")?;
    ui::window::paint_progress(
        &progress,
        &expander,
        &view,
        &queue.borrow(),
        crate::ui::shell_beats::StatusLine { status: &status, owned: false },
        true,
    );
    Ok(())
}

/// The log's own text view: the expander holds it even while collapsed, which a name search cannot see.
fn log_view(window: &adw::ApplicationWindow) -> Option<gtk::TextView> {
    ui::log_expander(window)?.child()?.downcast::<gtk::TextView>().ok()
}

/// Screen names the window can render: the spec's images, mapped onto a page.
fn screen_page(screen: &str) -> Option<&'static str> {
    match screen {
        "03-window" | "03-settings" | "03-sources" | "03-new-confirm" | "03-policy-form" => {
            Some("Prepare")
        }
        // §7's link line as the shell actually draws it, in the OPEN log. Program-side screen: there is
        // no `spec/img/03-log-link.png`; §1 only says which single line of the log is a link.
        "03-log-link" => Some("Prepare"),
        // §F1.1 S7/S8's face: the shell AFTER a Prepare run ended — bar at 1, "prepared — N files" on
        // the status row, the three folders named in an open log, Outputs counting what was written.
        // No `spec/img/` picture of it; §S7 spells the words, this shows them in place.
        "04-prepared" => Some("Prepare"),
        // §2's own picture is the RUN BAR at rest: ▶ with "Run this step — or resume what is paused",
        // ⏸ absent, and the gears still. It renders on Prepare because that is the page whose step ▶
        // stands for when nothing is playing (§F0.2 S4 → F1.1).
        "03-runbar" | "03-runbar-busy" => Some("Prepare"),
        "04-prepare" | "04-prompt-picker" => Some("Prepare"),
        "05-cut" | "05-add" | "05-fold" | "05-rows" | "05-split" | "05-trim" | "05-remove"
        // §F2.9's own screen: the page AFTER ⧉ Copy, ⧉ Paste (both branches and its refusal) and ⇲ Lane.
        // `spec/img` has no copy image, so this is a program-side screen like `03-log-link` / `04-prepared`.
        | "05-lanes" | "05-copy"
        // §F2.12's own screen likewise: three real inserts (BETWEEN, OVER, LANE) and S1's refusal. No
        // `spec/img/05-insert.png` exists, so `spec/inventory/cut.md` §G is the widget reference for it.
        | "05-insert"
        // §F2.14's own screen: ▶ in the MODEL mode — §S1's refusal beside a landed suggestion whose every
        // edge came from the app's snap points. No `spec/img/05-suggest.png` exists either.
        | "05-suggest" => Some("Cut"),
        "06-preview" | "06-zoom" | "06-speed" | "06-text" | "06-volume" | "06-label"
        | "06-lane" | "06-svg" | "06-effect-menu" => Some("Cut"),
        "07-narrate" | "07-take" => Some("Narrate"),
        "08-produce" | "08-words" => Some("Produce"),
        _ => None,
    }
}

/// Write one extracted frame into the folder Prepare would have put it in, so a Cut-page shot has a real
/// picture to show. PNG bytes under the `.jpg` name `cut_tracks::frame_paths` lists (Prepare's own stamping
/// rule, `frames.rs` S4): gdk-pixbuf decodes by content, and no JPEG encoder is linked here.
fn seed_snapshot_frame(dir: &std::path::Path, base: &str, second: f64) {
    let Ok(tree) = layout::Tree::new(dir) else { return };
    let dir = tree.frames_dir(base);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{}.jpg", crate::clock::frame_name(second)));
    // A frame already on disk (one Prepare really extracted) is the better picture; write only when absent.
    if path.exists() {
        return;
    }
    let Ok(surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, 160, 90) else {
        return;
    };
    {
        let Ok(cr) = cairo::Context::new(&surface) else { return };
        cr.set_source_rgb(0.08, 0.35, 0.6);
        let _ = cr.paint();
    }
    let Ok(mut file) = std::fs::File::create(&path) else { return };
    let _ = surface.write_to_png(&mut file);
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

/// Write one encoder row's default into the page's state, by the row name §A gives it. The page keeps
/// its settings as spelled strings, so this is the one place that maps a §A row to its field.
fn set_setting_row(state: &mut ui::produce_page::ProduceState, row: &str, value: &str) {
    match row {
        "Container" => state.container = value.to_string(),
        "Codec" => state.codec = value.to_string(),
        "Preset" => state.preset = value.to_string(),
        "Resolution" => state.resolution = value.to_string(),
        "Frame rate" => state.frame_rate = value.to_string(),
        "Audio" => state.audio = value.to_string(),
        "Subtitles" => state.subtitles = value.to_string(),
        _ => (),
    }
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
        // The bench opens on row 0, the User Context (§1 **5**), so every shot of this page would show
        // the same column: one short project note under a "User Context" heading and nothing else. That is
        // true here and in the reference picture, so it is not a defect to fix -- but it does mean the two
        // shots have to be compared on the row's own contents, not on a bench state. Measured with no
        // request for another row, `04-prepare` and `03-window` differ by 552 bytes and nothing else.
        // A screen that DOES want another row has to say so BEFORE the window is built: `bench_box` reads
        // this once and paints from it, and a later call would arrive after the heading was already drawn.
        // Both of these ask for row 2 (Describe): §1's own picture of the Prepare page is the page *with
        // the Describe prompt open*, and the picker shot is that same row with its dropdown spread.
        if matches!(screen.as_str(), "04-prepare" | "04-prompt-picker") {
            ui::set_bench_open_row(2);
        }
        // F2.10: a snapshot renders a PROJECT, so the folder it was asked for is taken on exactly as Open
        // takes one on — otherwise nothing can resolve a stored source path and the session's recordings
        // (stamped names + probed lengths) could never be laid out from the app's own rules. Taken on BEFORE
        // the window exists, because `build_window` seeds this window's session spans as it builds.
        ui::save_apply::note_open_for_snapshot(&dir);
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
            let two_cameras = cut::Cut {
                segs: vec![
                    cut::Seg { s: 0.0, e: 37.0, cam: 0, ..Default::default() },
                    cut::Seg { s: 20.0, e: 37.0, cam: 1, ..Default::default() },
                ],
                lanes: vec![cut::Lane {
                    name: "cam1".into(),
                    src: "cam1".into(),
                    at: 20.0,
                    off: 0.0,
                    dur: 17.0,
                }],
                shift: [("cam1".to_string(), -19.0)].into_iter().collect(),
                ..Default::default()
            };
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
            let folded_segs = vec![
                cut::Seg { s: 10.0, e: 30.0, cam: 0, ..Default::default() },
                cut::Seg { s: 40.0, e: 60.0, cam: 0, ..Default::default() },
            ];
            // F2.8 S2: the strip's two new rows only exist when there is something to plate for each — the
            // recorders' band needs a lane and the wave strip needs the session tape (see
            // `cut_trim_press::painted_bands`). With no lane the whole layout collapses back to one painted
            // row, so a Cut-page shot could never show the rows this round added; one separate recording puts
            // both rows on screen, which is what `spec/img/05-trim.png`'s strip has.
            let folded = cut::Cut {
                segs: folded_segs,
                lanes: vec![cut::Lane {
                    name: "mic-lav".to_string(),
                    src: "project:sources/mic-lav.wav".to_string(),
                    at: 12.0,
                    off: 0.0,
                    dur: 46.0,
                }],
                ..Default::default()
            };
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
            // Badge **1** shows a frame of the session, and Prepare's extracted frames are not part of the
            // demo fixture (it ships `cut/` only), so the shot writes one real PNG into the folder the page
            // lists frames from. Real bytes through gdk-pixbuf, named by `clock::frame_name` so the same
            // rule the band uses to pick a frame picks this one.
            seed_snapshot_frame(&dir, "session-tape", 0.0);
            // The frame reaches the panel through the page's own door: `refresh` asks which recording the
            // preview watches and where the line stands, so the shot first tells it WHICH session this is
            // (the same slot `page_recordings` answers from) before repainting.
            ui::cut_picture::set_preview_camera_base("session-tape");
            ui::cut_picture::set_snapshot_dir(&dir);
            crate::ui::cut_picture::refresh(&window);
            // Badge **31** is "hidden when everything fits", and at the opening 4 px a second this session
            // does fit — so no snapshot of the Cut page had ever shown the bar. Deepen the zoom through the
            // toolbar's own + door (the same one a click reaches) until the tape outgrows the view: the bar
            // then appears because the geometry says it must, not because anything was forced visible.
            // Six steps from 4 px/s lands on 15 px/s (§A's factor 1.25), which is also what §2's floor now
            // clamps to once − is walked back, so the shot shows the band at a zoom the ladder really stops at.
            for _ in 0..6 {
                ui::press_zoom(&window, true);
            }
        }
        // §2's own picture (`spec/img/03-runbar.svg`) is the run bar WITH WORK GOING ON: "the gears turn
        // while a run computes", and ▶ has become ⏸ "Pause". No snapshot could show that face until now
        // — `run()` drove nothing through `wire_play`, so every shot on disk was the idle one (proved by
        // `03-runbar` rendering byte-identical to `03-window`). Driven through the program's own doors,
        // the way `06-volume` opens its form, never by poking state:
        //   1. seed a cut so the Cut page has real kept material;
        //   2. press the page's own ▶ (F2.1). Its handler passes the filmed stretches it knows, which
        //      for a headless snapshot is none, so `preview::press_recording` starts from the red line --
        //      hence the line placed below first; with neither it answers S3's `NoFootage` and nothing
        //      plays (that refusal is what an earlier attempt of this shot printed on the status row);
        //   3. the preview now plays, so the bar's ▶ belongs to it.
        // The shot therefore ends on the BUSY face: the bar's ▶ drawn as ⏸ with tooltip "Pause", because
        // `run::controls` calls a page transport that is playing busy (§2). A paused-but-started preview
        // would read as ▶ again (`Transport::cued`) -- that is F0.2 S2's other face, pinned by
        // tests/press_play_widgets.rs instead of by a picture.
        if screen == "03-runbar-busy" {
            // Seeded BEFORE the tab switch: the Cut page draws its strip from this window's cut slot,
            // and `run()` seeds it after `build_window` (see the note there).
            ui::seed_review_cut(
                &window,
                &cut::Cut {
                    segs: vec![
                        cut::Seg { s: 10.0, e: 30.0, cam: 0, ..Default::default() },
                        cut::Seg { s: 40.0, e: 60.0, cam: 0, ..Default::default() },
                    ],
                    ..Default::default()
                },
            );
            ui::tab_button(&window, crate::shell::Page::Cut)
                .expect("the shell has a Cut tab")
                .emit_clicked();
            // The red line at 15 s: inside the first kept clip, so the preview has a place to start.
            // BOTH doors, because they are two different states in the app: `set_line_position` places the
            // line the page draws, and `set_playhead` stands the player's own clock there, which is what
            // `preview::press_recording` plays from (an earlier attempt placed the line only and the press
            // answered S3's `NoFootage` -- "nothing was filmed", printed on the status row of that shot).
            ui::set_line_position(&window, crate::cut_line::LinePos { t: 15.0 });
            ui::set_playhead(&window, 15.0);
            // The page's own ▶, through the click the widget fires -- the same body `wire_play_recording`
            // calls, so the bar's ⏸ answers a preview the app really began and not a state poke. The
            // assert goes on the player, which is the state both buttons read (`run::transport_for` gives
            // the Cut page its transport, and `run::controls` paints ⏸ from `playing`).
            ui::play_recording_button(&window).expect("the Cut page carries play-recording-button")
                .emit_by_name::<()>("clicked", &[]);
            // §F0.2 S2: the page's preview is going, so the bar's ▶ belongs to it. This press is the
            // program's own door and the one thing that repaints the bar (F0.3's ⏹ handler reads the player
            // the same way; nothing else calls `paint_run_bar` outside a bar-button press). It PAUSES the
            // preview and stops -- no snapshot, no step.
            ui::play_button(&window).expect("the run bar has a ▶").emit_by_name::<()>("clicked", &[]);
            let parked = ui::preview_player(&window).transport;
            let status = ui::state(&window).status;
            assert!(
                !parked.playing && parked.started,
                "S2 should have parked the preview: transport {parked:?}, status {status:?}"
            );
            // The face the parked state earns (`Transport::cued`: started, not playing, so ▶ still owes a
            // resume -- §2's tooltip says exactly that: "Run this step — or resume what is paused"), read
            // off the widget the shot is judged by rather than only off `run::controls`, so a bar that
            // forgot to repaint fails here instead of shipping the wrong picture.
            let parked_face = face_of(&window);
            assert_eq!(
                parked_face.0,
                crate::run::PLAY_ICON,
                "a parked preview leaves ▶ as the step's button (§2)"
            );
            assert_eq!(parked_face.1, crate::run::PLAY_TOOLTIP, "and §2's own word for it");
            assert!(parked_face.2, "a parked preview still has something to stop");
            // Now the shot's own subject: resume, and the bar is the BUSY face -- ⏸ "Pause", §2's "While a
            // run or page transport is busy it shows ⏸". The picture below is taken in this state.
            ui::play_button(&window).expect("the run bar has a ▶").emit_by_name::<()>("clicked", &[]);
            assert!(ui::preview_player(&window).transport.playing, "the second press resumes");
            let busy = face_of(&window);
            assert_eq!(busy.0, crate::run::PAUSE_ICON, "a playing preview turns ▶ into ⏸ (§2)");
            assert_eq!(busy.1, crate::run::PAUSE_TOOLTIP, "and the tooltip is §2's own word for it");
            assert!(busy.2, "and there is still something to stop");
            // Named as well as painted: a 16 px glyph inside a 1334 px window shot is not legible, so the
            // face this screen exists to record goes to `shots/03-runbar-busy.face.png` (the ▶ alone) and
            // is printed here for whoever runs the recipe.
            println!("snapshot: 03-runbar-busy face = {} {:?}, stop sensitive = {}", busy.0, busy.1, busy.2);
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
        // F3.8's screen is the LANE being worked on (`spec/img/06-lane.png`), which the fixture cannot show:
        // its `cut/cut.json` carries `fx: []`, so every shot of the Cut page draws an empty lane. Seeded here
        // the way `05-fold` seeds its hole -- the shared fixture file untouched -- and driven through the same
        // doors the widgets fire (`tests/band_hold_widgets.rs` presses exactly these) rather than by poking
        // state: a lane painted from a shortcut would not be the lane the app draws.
        if screen == "06-lane" {
            // Wide bars, at the same 4 px/s the spec's own lane is drawn with: 50 s is 200 px, far past both
            // `fx_band::GRIP_MIN_PX` (30) and `KILL_MIN_PX` (32), so the grips and the middle ✕ are drawn at a
            // size the shot can actually be read at (a 10 s / 40 px bar renders, but as a 2 glyph sliver).
            let lane_fx = vec![
                cut::Fx {
                    kind: "zoom".to_string(),
                    t: 5.0,
                    dur: 50.0,
                    ..Default::default()
                },
                // A narrow one for contrast: 6 s is 24 px, under both thresholds, so it offers no grips and no ✕.
                cut::Fx {
                    kind: "speed".to_string(),
                    t: 70.0,
                    dur: 60.0,
                    rate: 0.5,
                    ..Default::default()
                },
                cut::Fx {
                    kind: "label".to_string(),
                    t: 140.0,
                    dur: 6.0,
                    text: "intro".to_string(),
                    ..Default::default()
                },
            ];
            let lane = cut::Cut {
                segs: vec![
                    cut::Seg { s: 0.0, e: 30.0, cam: 0, ..Default::default() },
                    cut::Seg { s: 30.0, e: 60.0, cam: 0, ..Default::default() },
                ],
                fx: lane_fx,
                ..Default::default()
            };
            ui::seed_review_cut(&window, &lane);
            ui::set_line_position(&window, cut_line::LinePos { t: 140.0 });
            // Pick the first band UP through the press seam (a middle grab, 20 px of travel), so the shot shows
            // the held state: the white outline, the line walked to the band's start, and the
            // "Zoom picked up" sentence the lane prints.
            ui::press_effect_bar(&window, 0, 100.0, 200.0, 20.0, false);
        }
        // §F2.12's own screen: three inserts staged through the chooser seam, the radios and Apply, then S1's
        // refusal last. All of it lives in `snapshot_insert` -- `run` is past the size another block fits in.
        if screen == "05-insert" {
            let said = crate::snapshot_insert::stage(&window, &dir);
            let words: Vec<String> = said.iter().map(|line| line.replace('|', "/")).collect();
            println!("snapshot: 05-insert | {}", words.join(" | "));
            println!("snapshot: 05-insert band | {}", crate::snapshot_insert::describe(&window));
            ui::cut_view::repaint_page(&window);
            ui::refresh_camera_rows(&window);
        }
        // §F2.9's screen is the Cut page AFTER the whole flow ran: ⧉ Copy took a stretch in hand, ⇲ Lane gave
        // it a row of its own, ⧉ Paste spliced the footage copy back as a `copy:` card, and a sound copy was
        // laid over the kept footage in one piece per stretch — plus S2's refusal, with the line in a hole.
        // Driven through the same doors the buttons fire (`tests/cut_copy_paste_lane_widgets.rs` presses these
        // exact ones), never by poking state: what is on screen is what the verbs wrote.
        if screen == "05-copy" {
            // The session filmed 10-130 as one take that does NOT start at 0:00 — the case the paste has to get
            // right: a copied span asks the tape for the material under it, not for its head.
            ui::set_session_recordings(&[crate::timeline::Recording {
                base: "session-tape".to_string(),
                start: 10.0,
                end: 130.0,
            }]);
            let kept = cut::Cut {
                segs: vec![
                    cut::Seg { s: 10.0, e: 40.0, cam: 0, ..Default::default() },
                    cut::Seg { s: 50.0, e: 80.0, cam: 0, ..Default::default() },
                ],
                ..Default::default()
            };
            // The separate recording a sound copy is taken FROM, named the way a lane or a wave plate names
            // one (its base). A sound paste refuses a recording the session's file list does not hold, so the
            // shot adds it to the session too — straight into the window's own project state, which is what
            // Inputs writes. (`session_handle` is the page's public handle on it.)
            ui::session_handle(&window).borrow_mut().sources.push(crate::project::Source {
                path: "mic-lav.wav".to_string(),
                footage: true,
                ..Default::default()
            });
            ui::seed_review_cut(&window, &kept);
            // Deepen the zoom first: at the opening 4 px/s a 12 s copy is 48 px and its card reads as a
            // smudge, while 15 px/s (where §A's ladder stops) gives every box here real width.
            for _ in 0..6 {
                ui::press_zoom(&window, true);
            }
            // S3's row from a hand (a lane survives the paste, so it goes first), after a footage copy:
            // a row of its own, windowing the file from the FILE second it was taken at — the conversion this
            // round fixed, and the one thing of this flow that is plainly visible on the strip (a spliced card
            // has zero width at every zoom: the cut is *opened* there, F2.9).
            ui::draw_selection(
                &window,
                crate::cut_select::Surface::PictureRow(0),
                None,
                20.0,
                32.0,
            );
            let copied = ui::press_copy(&window);
            ui::set_line_position(&window, cut_line::LinePos { t: 70.0 });
            let said = ui::press_lane(&window);
            // S2's footage branch: the same seconds in hand again, ⧉ Paste with the line 5 s into the second
            // clip — the `copy:` card, and a cut that got longer ("the cut is a, was b").
            ui::draw_selection(
                &window,
                crate::cut_select::Surface::PictureRow(0),
                None,
                20.0,
                32.0,
            );
            let recopied = ui::press_copy(&window);
            ui::set_line_position(&window, cut_line::LinePos { t: 55.0 });
            let pasted = ui::press_paste(&window);
            // S2's sound branch: a 12 s stretch of the microphone copied at 0:36 and pasted at 0:24, so it
            // lies over TWO kept stretches (the 0:40-0:50 hole splits it) and the sentence says "stretches".
            ui::draw_selection(
                &window,
                crate::cut_select::Surface::WaveStrip,
                Some("mic-lav.wav"),
                36.0,
                48.0,
            );
            let sound_copied = ui::press_copy(&window);
            ui::set_line_position(&window, cut_line::LinePos { t: 24.0 });
            let laid = ui::press_paste(&window);
            // S2's refusal, honestly aimed: a hand that starts INSIDE the hole (copied at 0:42) and the line at
            // 0:41 — inside the 0:40-0:50 gap, where the cut keeps no footage at all (the pieces laid two steps
            // back stop at the hole's edge), so the paste refuses ("a sound needs a picture under it") and the
            // copy STAYS in hand. That is the last word on the status row, which is what a bad aim really shows.
            ui::draw_selection(
                &window,
                crate::cut_select::Surface::WaveStrip,
                Some("mic-lav.wav"),
                42.0,
                46.0,
            );
            let held_again = ui::press_copy(&window);
            ui::set_line_position(&window, cut_line::LinePos { t: 41.0 });
            let refused = ui::press_paste(&window);
            // The strip repaints on the view's own door; a snapshot that never queued one would paint the
            // pre-press picture.
            ui::cut_view::repaint_page(&window);
            // The sentences the flow earned are this screen's text: printing them makes a wrong or empty status
            // row show up here instead of as an unexplained picture.
            println!("snapshot: 05-copy | {copied} | {said} | {recopied} | {pasted}");
            println!("snapshot: 05-copy sound | {sound_copied} | {laid} | {held_again} | {refused}");
            ui::refresh_camera_rows(&window);
        }
        // §F2.14's own picture: what the Cut page is left with after a Suggest press — §S1's first
        // refusal sentence on the status row beside the landed suggestion whose every edge came from the
        // app's snap points. Both come from the doors ▶ fires (`refuse_suggest_run`, `press_landed`),
        // never by poking state; the other three refusals live in `tests/f2_14_guard_order.rs`.
        if screen == "05-suggest" {
            let staged = crate::snapshot_suggest::stage();
            // The tape the landed scenes sit on. Without a declared session the strip has no seconds to lay
            // out and the shot would show four scenes in a window with no timeline under them.
            ui::set_session_recordings(&[crate::timeline::Recording {
                base: "session-tape".to_string(),
                start: 0.0,
                end: 62.0,
            }]);
            ui::set_session_policy_field(&window, "cutMode", "model", "this shot", project::Origin::User);
            // S1's refusal, honestly aimed: busy now. It goes on the status row through the door the press
            // uses, so the picture shows the sentence a person would read.
            let refused = ui::cut_suggest_land::refuse_suggest_run(&window, true)
                .unwrap_or_else(|| crate::suggest::BUSY.to_string());
            ui::window::set_status_line(&window, &refused);
            // Then the walk-back's landing: literal seconds in, app-chosen edges out, no socket dialled.
            let landed = crate::snapshot_suggest::press_landed(&window);
            let segs = ui::review_cut_segs_count(&window);
            let seconds = crate::cut_screen::cut_seconds(&cut::Cut {
                segs: ui::review_cut_segs(&window),
                ..Default::default()
            });
            println!("snapshot: 05-suggest | {refused} | {landed} | {segs} segs | {} s", seconds.round());
            // The folder is the shot's own scratch, and its `cut.json` is what `land_spans` wrote: read it
            // back so a screenshot proves the landing reached a file, not only the widget.
            let written = staged
                .as_ref()
                .map(|tree| tree.cut_json().is_file())
                .unwrap_or(false);
            println!("snapshot: 05-suggest cut.json written: {written}");
            // The same three refreshes a mutating verb does after it publishes (see
            // `cut_verb_press`'s tail): the fold badges, the camera rows and the effects lane. Without them
            // the shot shows the page as it was before the landing — which is why this screen once drew
            // `Segments: 0` beside four landed scenes.
            ui::refresh_fold_badges(&window);
            ui::refresh_camera_rows(&window);
            ui::refresh_effects_lane(&window);
            // And the four readings that answer from the timeline: without this the shot below still said
            // `Segments: 0` / `Cut: 00:00` beside four landed scenes, because the form column is built once
            // for an empty cut and only a verb repaints it.
            crate::ui::window::refresh_cut_readouts(&window);
        }
        // §07-narrate's own picture (`spec/img/07-narrate.png`) is FOUR written lines with the app's own
        // times, estimates and one red fit warning. The fixture cannot show them: `fixtures/demo.naivepost`
        // carries no `narrate/narration.json`, so a bare shot of the page draws an empty lines column.
        // Seeded here the way `06-lane` seeds its bars -- the shared fixture file untouched -- through the
        // page's own publish door (`set_state` + `refresh`), which is what every flow calls after it edits
        // the narration; the rows are then drawn by `draw_rows` from these entries, not painted here.
        if screen == "07-narrate" {
            let mut state = ui::NarrateState::default();
            // Four clips, one line each, laid out on the spec image's seconds. `s`/`e` are the clip bounds
            // copied from the cut (§4), `at` the placement inside the clip, so the row's time reads
            // `s + at` as `time_field` spells it: 3.5, 43.6, 102.2 and 439.1.
            state.entries = vec![
                crate::narration::Entry {
                    s: 3.0,
                    e: 28.0,                    at: 0.5,
                    text: "Welcome back: lecture two of the blockchain series is all about Ethereum."
                        .into(),
                    emotion: "calm".into(),
                    // `pos` stays empty on purpose: a non-empty one makes `status` answer `Caption`, which
                    // prints the caption sentence and hides the estimated end time §1's rows show.
                    ..Default::default()
                },
                crate::narration::Entry {
                    s: 30.0,
                    e: 70.0,
                    at: 13.6,
                    text: "First a quick look back at Bitcoin, and at what a 51 percent attack really costs.".into(),
                    emotion: "happy=0.6, calm=0.4".into(),
                    ..Default::default()
                },
                crate::narration::Entry {
                    s: 90.0,
                    e: 140.0,
                    at: 12.2,
                    text: "Then the Ethereum basics: gas, smart contracts, accounts and the architecture behind them.".into(),
                    emotion: "curious".into(),
                    ..Default::default()
                },
                // Long enough at `SPEECH_CHARS_PER_SECOND` to run past its clip, so the last row wears the
                // red mark and prints `fit_warning`'s overrun sentence -- the thing §1's item 21 shows.
                crate::narration::Entry {
                    s: 435.0,
                    e: 445.0,
                    at: 4.1,
                    text: "And right at the end of this clip, a long line that will not fit: the page warns that it runs past the clip before anything is spoken, so it can be rewritten shorter.".into(),
                    emotion: "excited".into(),
                    ..Default::default()
                },
            ];
            // The clips these lines hang on, copied from the entries' own bounds: `shell::refit`, which
            // runs when the page is entered, orphans a line whose clip no longer exists (and an orphaned
            // line reads as one the cut threw away). A snapshot that wants four rows therefore has to show
            // four kept clips, which is what §1's image shows too.
            state.segs = state
                .entries
                .iter()
                .map(|entry| cut::Seg {
                    s: entry.s,
                    e: entry.e,
                    cam: 0,
                    ..Default::default()
                })
                .collect();
            state.voice = "narrator1".into();
            state.takes = vec![(1.0, 6.0)];
            state.pitch = 0.0;
            state.clips = 4;
            state.length = 480.0;
            state.has_cut = true;
            state.has_timeline = true;
            state.narrators = 1;
            // The clock reads where the shot's line sits, so the two faces agree with the rows below them.
            state.session = 40.0;
            state.cut_at = 33.0;
            ui::set_state(state);
            ui::refresh(&window);
            // F4.1's bar face is part of this screen now: the run has written its clips, so the progress
            // bar reads `writing N/M clips` rather than sitting idle. Driven through the same seam the
            // run paints from (not a poke), with the reply empty because headless there is no model --
            // only the stage text and fraction are wanted here, and the door's own status line is not.
            ui::show_narrate_stage(&window, 2, 4);
        }
        // §08-produce#1-screen's own picture (`spec/img/08-produce.png`) is a page with work on it:
        // images in the row, an instruction written, settings chosen and both readouts filled. The
        // fixture cannot show that -- `fixtures/demo.naivepost` carries no `publish.json`, so a bare
        // shot draws the empty-images note and two blank readouts. Seeded here the way `07-narrate`
        // seeds its lines: through the page's own publish door (`set_state` + `refresh`), which is what
        // every flow calls after it edits the publish state, so the slots, the rows and the readouts
        // are drawn by the page from this state rather than painted here.
        if screen == "08-produce" {
            let mut state = ui::produce_page::read_state();
            state.frames = (1..=3)
                .map(|n| format!("project:produce/publish/frame-{n}.jpg"))
                .collect();
            state.instruction = "Make the speaker mid-gesture at the whiteboard, keep the room dark \u{2014} \
no words, no logos.".into();
            state.negative = "watermarks, lettering, extra limbs".into();
            state.title = "Ethereum, one lecture at a time".into();
            state.thumb_title = state.title.clone();
            // One marked box, so the shot shows the text-overlay editor's ✎ chip rather than an
            // empty chip row (§A: "press its ✎ to reword or remove it").
            state.texts = vec![crate::project::TextMark {
                cx: 0.5,
                cy: 0.25,
                wf: 1.0,
                hf: 0.4,
                text: "one lecture at a time".into(),
            }];
            // §1 prints the title across the picture, so the shot's picture carries its words.
            state.title_seeded = true;
            // The crop box the base wears: §A draws it only when the base's shape differs from the
            // video's, and both are unknown in the fixture, so a bare shot shows no crop box at all.
            state.base_shape = Some((1080, 1920));
            state.video_shape = (1920, 1080);
            state.description = "Lecture two of the series: gas, smart contracts and accounts, with the \
architecture behind them.
".into();
            // §A's Default column, spelled from the rules module so the shot agrees with the tests.
            for (row, value) in crate::produce_screen::defaults() {
                match row {
                    "Container" | "Codec" | "Preset" | "Resolution" | "Frame rate" | "Audio"
                    | "Subtitles" => set_setting_row(&mut state, row, value),
                    _ => (),
                }
            }
            state.game_volume = 0.22;
            state.crf = 24.0;
            state.blurred_edges = true;
            state.clips = 7;
            state.seconds = 254.0;
            state.to_speak = 5;
            state.publish_written = true;
            state.out_files = 9;
            state.out_bytes = 41 * 1024 * 1024;
            ui::produce_page::set_state(state);
            ui::produce_page::refresh(&window);
        }
        // F0.8's screen is the confirmation, not the window behind it: the dialog is what the spec
        // image shows, so it is what gets painted. The fixture is a named project folder, which is
        // why its body carries the "stays on disk as it is" paragraph.
        let subject: Option<gtk::Widget> = match screen.as_str() {
            // §08's second image is the ✎ dialog, not the page behind it, so the dialog is the subject
            // here -- same reason `03-settings` paints its own window. Opened the way the chip opens it,
            // with the mark's own words already in the box.
            "08-words" => {
                ui::produce_words::open_reword(&window, 0, "Ethereum, one lecture at a time");
                let shown = ui::produce_words::open_dialog().expect("the chip opens its dialog");
                shown.set_default_size(420, 220);
                Some(shown.upcast())
            }
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
            // too. The grid, the badge column and the Fetch models row all show at once.
            //
            // One row answers ✓: `no_probe` fails every row, which would leave a column of empty marks —
            // badges only get painted by a press, and a snapshot presses nothing. So the ffmpeg row is
            // pressed once through the same seam a click goes through (`press_test`, the one
            // `wire_buttons` wires), with `checks::ffmpeg_verdict`'s own passing sentence rather than an
            // invented one: that is what the spec's picture shows ("the ✓ is a passed ffmpeg test").
            "03-settings" => {
                // Painted from `pictured_conf` rather than the bare build: §5 gives every model box a
                // Slots spin, and the picture has to show the two controls as different numbers.
                let shown = ui::settings::build_from(
                    None,
                    ui::settings::no_probe(),
                    &crate::snapshot_settings::pictured_conf(),
                );
                shown.present();
                crate::snapshot_settings::remember(&shown);
                crate::snapshot_settings::stage_ffmpeg_row();
                Some(shown.upcast())
            }
            // `spec/img/04-prompt-picker.png` pictures the row picker OPEN on a prompt, so this
            // shot is the same Prepare window with the bench started on the Describe row instead of
            // the User Context. The subject stays the window — the picker only makes sense in place,
            // beside the title and Reset it drives — and the row itself was requested above, before
            // `build_window`. Row 2 = "Describe", per `bench::ROWS`.
            //
            // The open list itself cannot be captured here: a GtkDropDown pops its rows as its OWN
            // top-level surface, and [`write_png`] paints one widget into one PNG, so pressing the
            // picker for the shot would change nothing the picture can show (it was tried: the file came
            // out the same picture as `04-prepare`). The page's picker is right as it is — §1's badge 3
            // is a row picker — so what this shot proves is the row chosen, not the popup.
            "04-prompt-picker" => None,
            // §7's log lines, then the once-per-run link, in the expander a run would have opened. All
            // three go in through `ui::note_exchange_line`, the door §7's recorder writes through:
            // it pushes to the same sink every flow uses AND opens and fills the expander through
            // `paint_progress`, so what is painted is exactly what a live shell paints mid-run.
            "03-log-link" => {
                let step = crate::run::step(crate::shell::Page::Prepare).label();
                ui::note_exchange_line(&format!(">>> {step}: 451.4 kB of text and 0 image(s) went to the LLM"));
                ui::note_exchange_line(&format!(">>> {step}: 28.6 kB came back in 2m39s"));
                ui::note_exchange_line(&crate::exchanges::link_line("llm/prepare-page.html"));
                None
            }
            // §S7/S8 as the shell actually draws them, driven through the SAME door the ▶ handler calls
            // (`ui::prepare_guard::say_ending`) with the walk's finished outcome — not a poke of the
            // labels. The folder is a scratch `.naivepost` under the temp dir holding one file per
            // prepare/ subfolder, so the count on the status row and the Outputs readout are the same
            // number a real run would print (3), and the log expander has to be opened like a run's does.
            "04-prepared" => {
                let root = std::env::temp_dir().join(format!("naivepost-snap-04-prepared-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&root);
                let session = root.join("session.naivepost");
                for rel in [
                    "prepare/inputs/lecture/meta.env",
                    "prepare/describe/lecture/events.tsv",
                    "prepare/transcript/lecture.tsv",
                ] {
                    let path = session.join(rel);
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&path, "x");
                }
                let bar = ui::run_bar(&window).expect("the window carries a run bar");
                let queue = ui::run_queue(&window).expect("the window carries a run queue");
                // The folder the ending counts and refreshes from is `save_apply::save_target()`'s, so
                // the scratch project has to be taken on first — exactly what Open does to a real one.
                ui::save_apply::note_open_for_snapshot(&session);
                // A run has to be OPEN for its ending to be the ending of a run: `start_run` is what
                // §S7's "fraction 1" and §S4's close both act on, so the shot opens the bookkeeping,
                // lets the ending speak, then closes it exactly as a single-step press does.
                let mut exchange = crate::exchanges::RunLog::new();
                crate::runqueue::start_run(
                    &mut bar.borrow_mut(),
                    &mut queue.borrow_mut(),
                    &mut exchange,
                    crate::run::Step::Prepare,
                    crate::run::snapshot_sources(&model),
                );
                crate::ui::run_close::note_started_run(&bar);
                ui::prepare_guard::say_ending(
                    &crate::run_stages::Drive::Done { ran: 4, skipped: 0 },
                    &window,
                    &model,
                    &bar,
                    &queue,
                );
                // The close a real press runs between the ending and the next paint: the unload handed
                // over, the queue emptied. Then the log is opened, which is what paints the bar's face
                // and the status row last -- the order a finished press leaves them in.
                let view = log_view(&window).expect("the log expander holds its text view");
                let status = ui::find_status(window.upcast_ref()).expect("the shell has a status line");
                crate::ui::run_close::close_step_run(
                    &bar,
                    &queue,
                    &std::rc::Rc::new(std::cell::RefCell::new(exchange)),
                    &status,
                    &ui::progress_bar(&window).expect("the shell carries a progress bar"),
                    &ui::log_expander(&window).expect("the shell carries the log expander"),
                    &view,
                    None,
                );
                // §S7: the ending is the run's LAST word, which is why it goes back after that close —
                // the close puts the queue's (now empty) line over the row. Same two lines the ▶ handler
                // writes in this position.
                if !bar.borrow().status.is_empty() {
                    let said = bar.borrow().status.clone();
                    status.set_text(&said);
                }
                show_the_log(&window).expect("the shell's log opens for its own snapshot");
                // ...and the ending goes back on the row after it: `show_the_log` paints through
                // `paint_progress`, whose status write would otherwise be the last word on screen.
                if !bar.borrow().status.is_empty() {
                    let said = bar.borrow().status.clone();
                    status.set_text(&said);
                }
                let _ = std::fs::remove_dir_all(&root);
                None
            }
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
            if screen_page(&screen) == Some("Cut") || screen_page(&screen) == Some("Narrate") {
                // 1280x800 rather than the Cut page's 1240x900: at 900 the Narrate shot renders ~3000 px
                // tall and every control reads as a hairline. This is closer to §1's own screenshot ratio.
                window.set_default_size(1280, 800);
            }
            window.present();
        }
        // §1's tape is as wide as the page, so a snapshot has to ask for its picture AFTER GTK laid the page
        // out: a `DrawingArea` clips its draw callback to its own allocation, and headless nothing repaints
        // between the map and the shot. One `queue_draw` after the layout is enough — it lands on the same
        // surface the band paints into, at the width the paned actually gave it.
        if screen_page(&screen) == Some("Cut") {
            // Drain the loop so GTK has laid the page out before anything asks it for a width: `present()`
            // only queues the map, and headless nothing else runs between here and the pixels. Without this
            // the band's draw is clipped to its pre-layout request and the tape stops at its floor.
            let context = glib::MainContext::default();
            for _ in 0..64 {
                while context.pending() {
                    context.iteration(false);
                }
            }
            // The red line too: §1 puts it on its own layer at the second the preview stands on, and a shot
            // that never placed one showed no line at all (badges 30). Placed through the page's own door,
            // then repainted, exactly as a click on the tape would.
            if screen == "05-cut" {
                // The band's red line is the PREVIEW's playhead (`band_scene_for` reads it off the player),
                // so that is the door this has to go through — `set_line_position` moves the cursor state,
                // which the painter never asks. Placed before the redraw, exactly as a click on the tape does.
                ui::set_playhead(&window, 24.0);
            }
            crate::ui::cut_view::repaint_page(&window);
        }
        let painted = subject.clone().unwrap_or_else(|| window.clone().upcast());
        // `03-runbar-busy` records a GLYPH, and a glyph is not legible in a 1334 px window shot, so this
        // screen paints the ▶ itself into `<out>` and the whole window beside it as `-window.png`: the
        // strip is what gets looked at, the page is there to show the bar in place.
        let face_only = screen == "03-runbar-busy";
        // The glyph goes beside the window shot as `<name>-face.png`; the whole page stays at `<name>.png`
        // so every snapshot keeps writing exactly one file under the name it was asked for.
        let face_out = out.with_extension("face.png");
        let out = out.clone();
        let app = app.clone();
        let face_out = face_out.clone();
        // One more cycle after the map: the map happens before the first frame is drawn. The paint
        // and the quit are synchronous here, so no second mapped window is ever waited on.
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            // The settings dialog's staged ffmpeg verdict goes on now: the map has happened, so the label
            // can hold a mark, and this runs before the pixels are taken.
            crate::snapshot_settings::apply_staged_badge();
            if face_only {
                // The glyph the screen exists to record, painted on its own after the window shot.
                if let Err(err) = write_png(&painted, &out) {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
                let play = ui::play_button(&crate::ui::window::main_window()).expect("the run bar has a ▶");
                if let Err(err) = write_face_png(&play, &face_out) {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
            } else if let Err(err) = write_png(&painted, &out) {
                eprintln!("snapshot: {err}");
                OUT.with(|e| *e.borrow_mut() = Some(err));
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
