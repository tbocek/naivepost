//! §05-cut#1-screen — the FORM COLUMN's own shape, measured off a realized page: the eight idle readings
//! sit "label left and value right", the pinned heading (title left, ✕ at its right end) shows only while a
//! form is open, and the gutter stays a 30 px column inside each band rather than becoming a band of its own.
//!
//! `spec/05-cut.md` §1 ("Form column", "Tracks") and `spec/inventory/cut.md` §A. Before this round every
//! value label carried `xalign 0.0` and the grid was pinned to `Align::Start`, so all eight values printed
//! flush against their own labels with no right edge anywhere in the column — the reading column rendered
//! fine and was wrong about which side the numbers belong on. Every number here comes out of the realized
//! tree (`translate_coordinates`, `width()`), never off a screenshot.

use adw::prelude::*;
use gtk4 as gtk;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_VALUES: AtomicBool = AtomicBool::new(false);
static RAN_HEADING: AtomicBool = AtomicBool::new(false);
static RAN_GUTTER: AtomicBool = AtomicBool::new(false);

/// How far a value's right edge may fall short of the grid's own right edge and still count as
/// right-aligned: a couple of pixels of padding, not a column's worth of slack. The bug put them
/// hundreds of px short.
const VALUE_RIGHT_SLACK_PX: i32 = 8;

/// A widget's horizontal extent in window coordinates: (left, right). One frame is shared by everything
/// on the page because `translate_coordinates` answers relative to the native surface.
#[allow(deprecated)] // 4.12's replacement needs a surface node this tree does not hand out
fn span(widget: &gtk::Widget) -> (i32, i32) {
    let native = widget.native().expect("the page is inside a realized window");
    let (x, _) = widget.translate_coordinates(&native, 0.0, 0.0).expect("the widget is laid out");
    let left = x as i32;
    (left, left + widget.width())
}

/// A window on the Cut page over a session with footage, sized the way a real window is (a Cut-page
/// layout collapses to one column without an explicit size before `present()`). Its own temp root:
/// `sec_05_cut_1_screen_layout.rs` re-points the process-wide cwd too, and the two files must not read
/// each other's session.
fn cut_page(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let root = std::env::temp_dir().join(format!("cut-column-w-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");
    let session = naivepost::startup::session_dir(&root);
    let mut seed = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    seed.sources.iter_mut().for_each(|source| source.footage = true);
    std::fs::create_dir_all(&session).expect("session folder");
    naivepost::project::save(&seed, &session).expect("the session is a real project on disk");

    let window = naivepost::ui::build_window(app, &seed, "Prepare");
    hold_last_window(window.clone());
    window.set_default_size(1440, 900);
    window.present();
    naivepost::ui::tab_button(&window, naivepost::shell::Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    naivepost::ui::set_session_recordings(&[naivepost::timeline::Recording {
        base: "lecture".into(),
        start: 0.0,
        end: 120.0,
    }]);
    let seeded = naivepost::cut::Cut {
        segs: vec![naivepost::cut::Seg { s: 0.0, e: 60.0, ..Default::default() }],
        ..Default::default()
    };
    naivepost::ui::seed_review_cut(&window, &seeded);
    // §1 badge 29 is "the row's sound switch", and `refresh_camera_rows` draws one gutter switch per cut
    // LANE (`cut_.lanes`), so the page needs a lane to have a switch at all. The demo fixture ships none, and
    // the band's own left edge is measured from the fold badge (§B puts both in the same 30 px column).
    let mut with_lane = seeded;
    with_lane.lanes = vec![naivepost::cut::Lane {
        name: "mic-lav".into(),
        src: String::new(),
        at: 0.0,
        off: 0.0,
        dur: 30.0,
    }];
    naivepost::ui::seed_review_cut(&window, &with_lane);
    // A silent-everywhere lane would show its switch OFF; press the door the switch itself fires so the
    // widget on screen carries the state the rule writes (and the wire is exercised by a real click below).
    naivepost::ui::press_gutter_switch(&window, "mic-lav");
    settle();
    window
}

/// §1's "idle readings, one per line, label left and value right": for each of the eight rows the key's
/// right edge sits at or left of its value's left edge, and the value ends where the grid ends.
fn check_the_readings_are_label_left_value_right(app: &adw::Application) {
    let window = cut_page(app);
    let grid = common::find_named(&window, "cut-form").expect("§A's idle readings grid is on the page");
    let (grid_left, grid_right) = span(&grid);
    println!("GRID cut-form spans x={grid_left}..{grid_right} ({} px)", grid.width());
    assert!(
        grid_right > grid_left,
        "the grid has a width to align values against: {} px",
        grid.width()
    );
    for label in naivepost::cut_screen::IDLE_FORM_ROWS {
        let name = naivepost::cut_screen::readout_widget(label);
        // Two rows carry a CONTROL as their value rather than a plain label (the thumbnail ladder, the
        // aspect dropdown), and for those the row's whole value cell is what has to end at the column's
        // right edge -- `cut_form_column` drops the row's plain label when it attaches a control, so §1
        // still gets one line with one value per reading.
        let key_box = format!("{name}-key");
        let (key_name, value_name): (&str, &str) = match label {
            // The two rows whose value is a CONTROL rather than a plain label. The thumbnail ladder's own
            // box is the value cell and its first child answers to `cut-readout-thumbnails`, so THAT row's
            // label carries `-key`; the aspect dropdown replaces its readout outright.
            "Thumbnails" => (&key_box, "thumb-minus"),
            "Aspect ratio" => (&key_box, "aspect-choice"),
            _ => (&key_box, &name),
        };
        let key = common::find_named(&window, key_name)
            .unwrap_or_else(|| panic!("the `{label}` row has no key label"));
        let value = common::find_named(&window, value_name)
            .unwrap_or_else(|| panic!("the `{label}` row has no value ({value_name})"));
        // The grid's own right edge is the column's: the pane ends where the divider sits, and §1 wants
        // each value reading at that end. (`cut-form` fills its pane -- see `cut_form_column`.)
        let (grid_right, value_right) = if value_name == "thumb-minus" || value_name == "aspect-choice" {
            // A row whose value is a CONTROL: measure its whole CELL (the ladder's box for 🖼− number 🖼+,
            // the readout-plus-dropdown box for Aspect ratio), not the first widget inside it.
            let cell = value.parent().expect("the control lives in a box");
            let (_, cell_right) = span(&cell);
            (grid_right, cell_right)
        } else {            let (_, right) = span(&value);
            (grid_right, right)
        };
        // The grid's own right edge is the column's: the pane ends where the divider sits, and §1 wants
        // each value reading at that end. (`cut-form` fills its pane -- see `cut_form_column`.)
        let (key_left, key_right) = span(&key);
        let (value_left, _) = span(&value);
        println!(
            "ROW {label}: key x={key_left}..{key_right}, value x={value_left}..{value_right}, grid right {grid_right}"
        );
        assert!(
            key_right <= value_left + 1,
            "`{label}`'s label sits LEFT of its value: key right {key_right} vs value left {value_left}"
        );
        assert!(
            value_right >= grid_right - VALUE_RIGHT_SLACK_PX,
            "`{label}`'s value reads at the RIGHT end of the column: value right {value_right} vs \
             grid right {grid_right} ({} px short, slack {VALUE_RIGHT_SLACK_PX})",
            grid_right - value_right
        );
    }
    RAN_VALUES.store(true, Ordering::SeqCst);
}

/// §A's pinned heading: absent on the idle column, and once a form is open it carries the title at its
/// left and the ✕ at its right end. The form is opened through the program's own door — the chooser seam
/// `tests/suite/cut_form_frame_widgets.rs` uses, i.e. what the native file dialog's callback calls.
fn check_the_pinned_heading_shows_only_with_a_form(app: &adw::Application) {
    let window = cut_page(app);
    let heading = common::find_named(&window, "cut-form-heading").expect("§A's pinned heading row exists");
    assert!(!heading.is_visible(), "the idle column starts with the readings and NO heading (§1)");

    let video = {
        let dir = std::env::temp_dir().join(format!("np-column-sting-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("video dir");
        let path = dir.join("sting.mp4");
        std::fs::write(&path, b"not really a video, only a name").expect("video written");
        path
    };
    let said = naivepost::ui::insert_chosen_with_length(&window, &video, 20.0, Some(7.5));
    settle();
    assert!(said.contains("sting.mp4"), "{said}");
    assert!(naivepost::ui::insert_open(&window).is_some(), "the insert form is open");

    assert!(heading.is_visible(), "an open form pins its heading");
    let title = common::find_named(&window, "cut-form-title")
        .and_then(|w| w.downcast::<gtk::Label>().ok())
        .expect("the heading carries a title label");
    let close = common::find_named(&window, "cut-form-close").expect("the heading carries the ✕");
    let words = title.text().to_string();
    assert!(!words.trim().is_empty(), "the open form's heading names what is open");
    let (title_left, title_right) = span(title.upcast_ref());
    let (close_left, close_right) = span(&close);
    println!("HEADING title \"{words}\" x={title_left}..{title_right}, close x={close_left}..{close_right}");
    // A widget that has not been allocated yet answers 0 px wide, which says nothing about WHERE the ✕ sits.
    // Force a real allocation through the page's own repaint and drain the loop until the cross has pixels,
    // failing after `common::WAIT_LIMIT` if it never does -- never a guess about how many turns are enough.
    naivepost::ui::cut_preview_frame::repaint(&window);
    crate::common::wait_until(
        "the pinned heading is laid out (the ✕ gets its own width)",
        || close.width() > 0 && title.width() > 0,
    );
    let (title_left, title_right) = span(title.upcast_ref());
    let (close_left, close_right) = span(&close);
    println!("HEADING laid out: title x={title_left}..{title_right}, close x={close_left}..{close_right}");
    assert!(
        close_left >= title_right,
        "the ✕ sits at the heading's RIGHT end, after the title: close left {close_left} vs title right {title_right}"
    );
    assert!(
        close_right > close_left,
        "the ✕ has pixels of its own: {} px",
        close.width()
    );
    RAN_HEADING.store(true, Ordering::SeqCst);
}

/// §1's gutter: \"a 30 px column down the left edge of every band, NOT a band of its own\". So the number
/// is one constant the band offsets by, and no widget anywhere on the page is a gutter band.
fn check_the_gutter_is_a_column_not_a_band(app: &adw::Application) {
    let window = cut_page(app);
    let gutter = naivepost::cut_screen::GUTTER_PX;
    println!("GUTTER cut_screen::GUTTER_PX = {gutter} px, cut_view::gutter_px() = {}", naivepost::ui::cut_view::gutter_px());
    assert_eq!(gutter, 30.0, "§1's gutter is 30 px");
    assert_eq!(
        naivepost::ui::cut_view::gutter_px(),
        30.0,
        "the band asks its width of the same constant, so band and badges cannot disagree"
    );
    for name in ["gutter-band", "gutter-column", "gutter-row", "gutter-strip"] {
        assert!(
            common::find_named(&window, name).is_none(),
            "`{name}` would be a band of its own, which §1 rules out: the gutter is the first 30 px of \
             each band's timeline space"
        );
    }
    // What DOES live in the gutter are the switches and badges, drawn over the band's own first 30 px.
    let layers = common::find_named(&window, "track-layers")
        .expect("the gutter belongs to the band's layer stack, which paints it in its first 30 px");
    // §B's rail is IN that stack: the whole-cut badge and the lane switches are overlay children of
    // `track-layers`, not boxes the page packs beside it — a direct-children scan would miss them, because
    // `gtk::Overlay` keeps each overlay child in its own private layout child.
    fn inside(node: &gtk::Widget, name: &str) -> bool {
        if node.widget_name() == name {
            return true;
        }
        node.observe_children().iter::<glib::Object>().flatten().any(|child| {
            child
                .downcast_ref::<gtk::Widget>()
                .is_some_and(|child| inside(child, name))
        })
    }
    for rail in ["gutter-badges", "gutter-switches"] {
        assert!(
            common::find_named(&window, rail).is_some(),
            "`{rail}` is the gutter's control rail (§B: fold-all badge, sound switches, empty-row ✕)"
        );
        assert!(
            inside(&layers, rail),
            "`{rail}` must be a descendant of `track-layers`: §1's gutter is the band's own left edge, \
             not a column the page stacks next to it"
        );
    }
    println!("GUTTER rails inside track-layers: gutter-badges + gutter-switches");
    // And the controls §B names are really ON that rail at the band's left edge, not packed beside it:
    // badge 23 (the whole-cut fold badge) on `gutter-badges`, badge 29 (the row's sound switch) on
    // `gutter-switches`. Positions come from the realized tree, so a holder that exists but draws nothing
    // off-screen cannot pass this.
    let band_left = span(&layers).0;
    let badge = common::find_named(&window, "fold-all-button")
        .expect("\u{a7}1 badge 23: the fold-all badge lives in the gutter");
    let (badge_left, badge_right) = span(&badge);
    println!("GUTTER badge 23 x={badge_left}..{badge_right}, band left {band_left}");
    assert!(
        badge_left >= band_left && badge_right <= band_left + naivepost::cut_screen::GUTTER_PX as i32,
        "the fold-all badge sits inside the gutter's {} px: {badge_left}..{badge_right} vs band {band_left}",
        naivepost::cut_screen::GUTTER_PX
    );
    let lane = "mic-lav";
    let switch = common::find_named(&window, &format!("gutter-switch-{lane}"))
        .unwrap_or_else(|| panic!("\u{a7}1 badge 29: `{lane}` has no gutter switch"));
    // The real widget, fired the way the toolkit fires it: pressing the rail's switch must reach the
    // hearing rule, which is what makes this the wire as well as the position check.
    if let Ok(toggle) = switch.clone().downcast::<gtk::ToggleButton>() {
        toggle.emit_by_name::<()>("toggled", &[]);
        settle();
    }
    let (left, right) = span(&switch);
    println!("GUTTER badge 29 {lane} x={left}..{right}, band left {band_left}");
    assert!(
        left >= band_left && right <= band_left + naivepost::cut_screen::GUTTER_PX as i32,
        "`gutter-switch-{lane}` must be inside the gutter column: {left}..{right} vs band {band_left}"
    );
    RAN_GUTTER.store(true, Ordering::SeqCst);
}

fn round() {
    common::one_app_round(|app| {
        println!("CHECK 1 the eight idle readings are label left, value right");
        check_the_readings_are_label_left_value_right(app);
        println!("CHECK 2 the pinned heading shows only while a form is open");
        check_the_pinned_heading_shows_only_with_a_form(app);
        println!("CHECK 3 the gutter is a 30 px column, not a band");
        check_the_gutter_is_a_column_not_a_band(app);
    });
}

#[test]
fn sec_05_cut_1_screen_the_idle_readings_sit_label_left_and_value_right() {
    common::pin_private_cwd();
    round();
    assert!(RAN_VALUES.load(Ordering::SeqCst), "the reading-alignment check never ran");
}

#[test]
fn sec_05_cut_1_screen_the_pinned_heading_shows_only_while_a_form_is_open() {
    common::pin_private_cwd();
    round();
    assert!(RAN_HEADING.load(Ordering::SeqCst), "the pinned-heading check never ran");
}

#[test]
fn sec_05_cut_1_screen_the_gutter_is_a_30_px_column_not_a_band_of_its_own() {
    common::pin_private_cwd();
    round();
    assert!(RAN_GUTTER.load(Ordering::SeqCst), "the gutter check never ran");
}
