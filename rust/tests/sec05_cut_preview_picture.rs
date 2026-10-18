//! §05-cut#1-screen badge **1** — the preview's picture and the click on it. `spec/inventory/cut.md` §A:
//! "preview (GtkPicture contain-fit, min height 160, click toggles play)". The panel drew its camera rect,
//! dim and overlays but showed no frame of the session, and nothing about it was clickable. This proves the
//! real widgets carry both: a decoded PNG whose intrinsic size is what the program wrote there, and a press
//! that reaches the transport's own door.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared state
//! (this window's player, line and recordings), so they run once inside a single `Application::run` and
//! report through flags the test asserts on — the shape `tests/cut_screen_toolbar_widgets.rs` uses.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_FRAME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_CLICK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The PNG's own size: the measurement the picture is checked against, so a squashed or missing frame cannot
/// pass by being merely present.
const FRAME_W: i32 = 96;
const FRAME_H: i32 = 54;

fn find_named(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if widget.widget_name() == name {
            return Some(widget.clone());
        }
        for child in widget.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
            if let Some(found) = walk(child, name) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

/// The folder `build_window` will treat as this window's session: the page asks `startup::session_dir(cwd)`,
/// so a test that wants its own frames found has to write them where that rule points.
fn startup_session_dir(root: &std::path::Path) -> std::path::PathBuf {
    // Same rule the page uses; `session.naivepost` beside the root is its default answer.
    naivepost::startup::session_dir(root)
}

/// Write one REAL PNG into the frames folder Prepare would have extracted into, named the way
/// `clock::frame_name` names a frame so `cut_tracks::frame_near` can pick it. A file of junk bytes would
/// not decode, and the check would then be about a failure rather than about the picture.
///
/// The extension is `.jpg`, which is what `frame_paths` lists (Prepare's own stamping rule, `frames.rs` S4:
/// "‹YYYY-MM-DD_HH-MM-SS.mmm›.jpg"), while the BYTES are a PNG: gdk-pixbuf decodes by content, so the real
/// image is real without needing a JPEG encoder in this test.
fn write_frame(tree: &naivepost::layout::Tree, base: &str, second: f64) -> std::path::PathBuf {
    let dir = tree.frames_dir(base);
    std::fs::create_dir_all(&dir).expect("frames folder");
    let name = format!("{}.jpg", naivepost::clock::frame_name(second));
    let path = dir.join(&name);
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, FRAME_W, FRAME_H).expect("a surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        cr.set_source_rgb(0.1, 0.4, 0.8);
        let _ = cr.paint();
    }
    let mut file = std::fs::File::create(&path).expect("the frame file");
    surface.write_to_png(&mut file).expect("a real PNG written");
    path
}

/// A window on the Cut page over a session whose frames exist on disk, cwd pinned to its own temp root so
/// nothing resolves the project through `rust/`.
fn cut_page(app: &adw::Application) -> (adw::ApplicationWindow, naivepost::layout::Tree) {
    release_last_window();
    let root = std::env::temp_dir().join(format!("cut-picture-w-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");

    // The project the window opens must BE the folder we put frames in: `line_root` (the page's own slot for
    // its folder) is what `cut_picture::refresh` asks, and it comes from the tree the window was built with.
    // The window opens over `startup::session_dir(cwd)`, so the session folder is named by that rule rather
    // than invented here: `line_root` (what `cut_picture::refresh` asks for the folder) comes from it.
    let session = startup_session_dir(&root);
    // A `.naivepost` folder is only a tree once its project file exists, and `build_window` fills the
    // window's folder slot from THAT tree — so the session has to be written before the window opens.
    let mut seed = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    seed.sources.iter_mut().for_each(|source| source.footage = true);
    std::fs::create_dir_all(&session).expect("session folder");
    naivepost::project::save(&seed, &session).expect("the session is a real project on disk");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder with its file is a project");

    let mut model = naivepost::project::load(&session).expect("the session reloads");
    // Point the footage row at this session so the Cut tab unlocks and the tape is ours.
    model.sources.iter_mut().for_each(|source| source.footage = true);
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    settle();
    (window, tree)
}

/// S1(a): the picture shows a decoded frame of the second the line stands on, at the PNG's own size, and
/// never smaller than §A's 160 px floor.
fn check_the_preview_shows_a_frame(app: &adw::Application) {
    let (window, tree) = cut_page(app);
    let tape = [naivepost::timeline::Recording { base: "lecture".into(), start: 0.0, end: 120.0 }];
    ui::set_session_recordings(&tape);
    let seeded = cut::Cut { segs: vec![Seg { s: 0.0, e: 60.0, ..Default::default() }], ..Default::default() };
    ui::seed_review_cut(&window, &seeded);
    let picture = find_named(&window, "preview-picture").expect("§A's GtkPicture is on the panel");
    let picture = picture.downcast::<gtk::Picture>().expect("it is a Picture");
    // Nothing extracted yet: the plate stays empty rather than showing a broken image.
    assert!(picture.paintable().is_none(), "no frames on disk means no picture, not a broken one");

    let written = write_frame(&tree, "lecture", 10.0);
    assert!(written.exists(), "the frame file the program will list exists");
    // Stand the line on that second through the page's own door, which is what repaints the frame.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 10.0 });
    naivepost::ui::cut_picture::refresh(&window);
    settle();

    let shown = picture.paintable().expect("the frame the line stands on is showing");
    assert_eq!(shown.intrinsic_width(), FRAME_W, "the picture carries the PNG's own width");
    assert_eq!(shown.intrinsic_height(), FRAME_H, "and its own height");
    // §A's floor is a number the widget answers with, not one the spec alone asserts.
    let (min_w, min_h) = picture.size_request();
    assert!(min_h >= naivepost::ui::cut_picture::PICTURE_MIN_HEIGHT, "{min_h} px tall");
    let _ = min_w;
    RAN_FRAME.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// S1(b): a click on the picture toggles play, through the same door ▶ uses, and a second click pauses.
fn check_a_click_on_the_picture_toggles_play(app: &adw::Application) {
    let (window, tree) = cut_page(app);
    ui::set_session_recordings(&[naivepost::timeline::Recording {
        base: "lecture".into(),
        start: 0.0,
        end: 120.0,
    }]);
    let seeded = cut::Cut { segs: vec![Seg { s: 0.0, e: 60.0, ..Default::default() }], ..Default::default() };
    ui::seed_review_cut(&window, &seeded);
    write_frame(&tree, "lecture", 0.0);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 0.0 });
    naivepost::ui::cut_picture::refresh(&window);
    settle();
    assert!(!ui::preview_player(&window).transport.playing, "a fresh preview is not playing");

    let button = find_named(&window, "preview-picture-button").expect("the picture is clickable");
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        ui::preview_player(&window).transport.playing,
        "the click started the preview"
    );
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        !ui::preview_player(&window).transport.playing,
        "the same click again paused it \u{2014} toggle, not restart"
    );
    RAN_CLICK.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn round() {
    common::one_app_round(|app| {
        println!("CHECK 1 the preview shows a frame of the line");
        check_the_preview_shows_a_frame(app);
        println!("CHECK 2 a click on the picture toggles play");
        check_a_click_on_the_picture_toggles_play(app);
    });
}

#[test]
fn sec_05_cut_1_screen_s1_the_preview_shows_a_frame_of_the_line_and_a_click_toggles_play() {
    round();
    for (flag, what) in [
        (&RAN_FRAME, "the frame-of-the-line check"),
        (&RAN_CLICK, "the click-toggles-play check"),
    ] {
        assert!(flag.load(std::sync::atomic::Ordering::SeqCst), "{what} never ran");
    }
}
