//! §05-cut#1-screen — the FORM COLUMN's own shape, measured off a realized page: the eight idle readings
//! sit "label left and value right", the pinned heading (title left, ✕ at its right end) shows only while a
//! form is open, and the gutter stays a 30 px column inside each band rather than becoming a band of its own.
//!
//! `spec/05-cut.md` §1 ("Form column", "Tracks") and `spec/inventory/cut.md` §A. Before this round every
//! value label carried `xalign 0.0` and the grid was pinned to `Align::Start`, so all eight values printed
//! flush against their own labels with no right edge anywhere in the column — the reading column rendered
//! fine and was wrong about which side the numbers belong on. Every number here comes out of the realized
//! tree (`translate_coordinates`, `width()`), never off a screenshot.

use crate::common;
use adw::prelude::*;
use common::{fixture_dir, hold_last_window, release_last_window, settle};
use gtk4 as gtk;
use std::sync::atomic::{AtomicBool, Ordering};

static RAN_VALUES: AtomicBool = AtomicBool::new(false);
static RAN_IDLE_PAINTED: AtomicBool = AtomicBool::new(false);
static RAN_LADDER: AtomicBool = AtomicBool::new(false);
static RAN_HEADING: AtomicBool = AtomicBool::new(false);
static RAN_GUTTER: AtomicBool = AtomicBool::new(false);
static RAN_CEILING: AtomicBool = AtomicBool::new(false);
static RAN_TOP_READING: AtomicBool = AtomicBool::new(false);

/// How far a value's right edge may fall short of the grid's own right edge and still count as
/// right-aligned: a couple of pixels of padding, not a column's worth of slack. The bug put them
/// hundreds of px short.
const VALUE_RIGHT_SLACK_PX: i32 = 8;

/// A widget's horizontal extent in window coordinates: (left, right). One frame is shared by everything
/// on the page because `translate_coordinates` answers relative to the native surface.
#[allow(deprecated)] // 4.12's replacement needs a surface node this tree does not hand out
fn span(widget: &gtk::Widget) -> (i32, i32) {
    let native = widget
        .native()
        .expect("the page is inside a realized window");
    let (x, _) = widget
        .translate_coordinates(&native, 0.0, 0.0)
        .expect("the widget is laid out");
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
    seed.sources
        .iter_mut()
        .for_each(|source| source.footage = true);
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
        segs: vec![naivepost::cut::Seg {
            s: 0.0,
            e: 60.0,
            ..Default::default()
        }],
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
    let grid =
        common::find_named(&window, "cut-form").expect("§A's idle readings grid is on the page");
    let (grid_left, grid_right) = span(&grid);
    println!(
        "GRID cut-form spans x={grid_left}..{grid_right} ({} px)",
        grid.width()
    );
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
        let (grid_right, value_right) =
            if value_name == "thumb-minus" || value_name == "aspect-choice" {
                // A row whose value is a CONTROL: measure its whole CELL (the ladder's box for 🖼− number 🖼+,
                // the readout-plus-dropdown box for Aspect ratio), not the first widget inside it.
                let cell = value.parent().expect("the control lives in a box");
                let (_, cell_right) = span(&cell);
                (grid_right, cell_right)
            } else {
                let (_, right) = span(&value);
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

/// §1's idle column MEASURED AS PAINTED, not only as a tree: every reading the item lists must have real
/// pixels in a real Cut-page window, and the pinned heading must have none while nothing is open. The
/// label-left/value-right arithmetic is the check above; this one is the fact that the column is not an
/// empty white panel (which is what the render showed once the readings fell out of the paned's pane).
fn check_the_idle_column_paints_every_reading(app: &adw::Application) {
    let window = cut_page(app);
    let idle =
        common::find_named(&window, "cut-form").expect("the idle readings grid is on the page");
    assert!(idle.is_visible(), "an idle page shows its readings");
    let scroller = common::find_named(&window, "cut-form-scroller")
        .expect("§A's scroller holds the idle rows");
    assert!(
        scroller.height() > 0,
        "the scroller holding the readings was squeezed to nothing: {} px",
        scroller.height()
    );
    // One row per reading §1 lists, each with its key and its value on screen.
    let mut painted = 0;
    for label in naivepost::cut_screen::IDLE_FORM_ROWS {
        let name = naivepost::cut_screen::readout_widget(label);
        let key = common::find_named(&window, &format!("{name}-key"))
            .unwrap_or_else(|| panic!("the `{label}` row has no key label"));
        assert!(
            key.is_visible() && key.width() > 0,
            "`{label}`'s label is not painted: visible={} {} px",
            key.is_visible(),
            key.width()
        );
        let value = match label {
            "Thumbnails" => common::find_named(&window, "thumb-plus"),
            "Aspect ratio" => common::find_named(&window, "aspect-choice"),
            _ => common::find_named(&window, &name),
        }
        .unwrap_or_else(|| panic!("the `{label}` row has no value widget on screen"));
        assert!(
            value.is_visible() && value.width() > 0,
            "`{label}`'s value is not painted: visible={} {} px",
            value.is_visible(),
            value.width()
        );
        painted += 1;
    }
    assert_eq!(
        painted,
        naivepost::cut_screen::IDLE_FORM_ROWS.len(),
        "every reading §1 lists is on the idle column"
    );
    // And no heading: §1 says the idle column STARTS with the readings.
    let heading =
        common::find_named(&window, "cut-form-heading").expect("the pinned heading exists");
    assert!(
        !heading.is_visible(),
        "the idle column shows no pinned heading (§1)"
    );
    hold_last_window(window);
    RAN_IDLE_PAINTED.store(true, Ordering::SeqCst);
}

/// §1's ladder clicks on the real buttons: 🖼− / 🖼+ step the number they drive, and the readout in the
/// column moves with them (it is written by the same door). `cut_screen::thumb_up`/`thumb_down` own the
/// arithmetic (pinned in `cut_screen_readouts.rs`); this proves the buttons are wired to it and that the
/// label the user reads agrees, so the control can never drift from the number beside it.
fn check_the_thumbnail_ladder_steps_its_readout(app: &adw::Application) {
    let window = cut_page(app);
    // The Thumbnails row's value is the ladder's BOX (🖼− number 🖼+): the number is the BOX that carries
    // the row's readout name (`cut_form_column` names the control cell, not a label inside it), and
    // `press_thumb` repaints the number label that sits beside it.
    let cell = common::find_named(&window, "cut-readout-thumbnails")
        .expect("the Thumbnails row answers to its readout name");
    let number = || {
        fn find(w: &gtk::Widget) -> Option<String> {
            if let Some(label) = w.downcast_ref::<gtk::Label>() {
                // The ladder's two steps are buttons with labels too; the number is the label that is not
                // one of their glyphs.
                let text = label.text().trim().to_string();
                if !text.starts_with('\u{1f5bc}') {
                    return Some(text);
                }
            }
            for child in w.observe_children().iter::<glib::Object>().flatten() {
                if let Some(w) = child.downcast_ref::<gtk::Widget>() {
                    if let Some(text) = find(w) {
                        return Some(text);
                    }
                }
            }
            None
        }
        find(cell.upcast_ref()).expect("the ladder carries its number between the two steps")
    };
    let before = number();
    common::find_named(&window, "thumb-plus")
        .expect("§A's 🖼+ is on the column")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let bigger = number();
    assert_ne!(
        bigger, before,
        "🖼+ moves the number it drives: {before} -> {bigger}"
    );
    common::find_named(&window, "thumb-minus")
        .expect("§A's 🖼− is on the column")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        number(),
        before,
        "and 🖼− steps it back: {before} -> {bigger} -> {}",
        number()
    );
    hold_last_window(window);
    RAN_LADDER.store(true, Ordering::SeqCst);
}

/// §1's idle column at the MINIMUM window: the readings are shown from their FIRST line. The scroller is
/// squeezed to its 200 px floor there (the page cannot give it more), and a `gtk::ScrolledWindow` that gains
/// an adjustment keeps whatever value it last had — so once anything scrolled it, the column opened on the
/// TAIL of the eight rows and 'Thumbnails:', 'Aspect ratio:', 'Playhead:' and 'Selection:' were painted above
/// the visible edge (measured in `shots/05-cut.png`: three clipped pills at the top of the column). This is
/// the fact §1 states as "the idle column starts with the readings": the ladder buttons and the aspect
/// dropdown are MAPPED and inside the scroller's own box, the 'Thumbnails:' key sits ABOVE 'Aspect ratio:',
/// and every row's key is mapped.
fn check_the_idle_column_opens_on_its_first_reading(app: &adw::Application) {
    // BOTH sizes §1 allows the page to be seen at. Checking only the minimum is the hole that let the
    // render be wrong while this file was green: at §1's minimum the row-count floor covers the readings,
    // and in a taller window the form column is allocated a pane whose scroller was shorter than its own
    // body, so the shot opened on 'Aspect ratio:' with 'Thumbnails:' (badge **2**) scrolled away above it.
    let roomy = cut_page(app);
    column_opens_at_top(roomy, "at a 1440x900 window");
    let window = cut_page_at_minimum(app);
    column_opens_at_top(window.clone(), "at \u{a7}1's minimum");
    hold_last_window(window);
    RAN_TOP_READING.store(true, Ordering::SeqCst);
}

/// §1's "the idle column starts with the readings", measured on one realized page: the scroller sits at
/// the top of its body, the ladder buttons and the aspect dropdown are MAPPED inside its own box, and
/// 'Thumbnails:' reads ABOVE 'Aspect ratio:'.
fn column_opens_at_top(window: adw::ApplicationWindow, where_: &str) {
    // Open the page's own top/bottom divider at §A's 380 px — the SAME door a person's drag fires, and the
    // state the spec's picture is drawn in. Without it a test window settles the divider wherever the two
    // panes' own asks put it, which is why this column read wrong in the shot and right here: at §A's
    // opening the form column's pane is exactly 380 px, and a column whose scroller is shorter than its
    // own body loses its first reading at that height and at no other.
    if let Some(vsplit) =
        common::find_named(&window, "cut-page-split").and_then(|w| w.downcast::<gtk::Paned>().ok())
    {
        vsplit.set_position(naivepost::ui::cut_page_layout::SELF_SPLIT_TOP_PX);
        settle();
    }
    let scroller_box = common::find_named(&window, "cut-form-scroller")
        .expect("§A's scroller holds the idle rows");
    let scroller = scroller_box
        .clone()
        .downcast::<gtk::ScrolledWindow>()
        .expect("§A's scroller is a ScrolledWindow");
    let adjust = scroller.vadjustment();
    let grid =
        common::find_named(&window, "cut-form").expect("the idle readings grid is on the page");
    println!(
        "COLUMN {where_}: scroller {} px tall, body {} px tall, adjustment lower={} upper={} page={} value={}",
        scroller.height(),
        grid.height(),
        adjust.lower(),
        adjust.upper(),
        adjust.page_size(),
        adjust.value()
    );
    // Where the scroller's box is in window coordinates: a reading painted outside it is not on screen.
    let top = top_y(&scroller_box);
    let bottom = top + scroller.height();
    // The viewport must be at the top of its body AND tall enough to hold it whole: a column that starts
    // at its first line only because nothing scrolled it yet still loses that line as soon as anything does.
    crate::common::wait_until(
        &format!("the idle readings fit their viewport {where_}"),
        || adjust.page_size() >= grid.measure(gtk::Orientation::Vertical, -1).1 as f64,
    );
    assert!(
        adjust.value() <= adjust.lower() + 1.0,
        "the idle column is scrolled off its first reading {where_}: value={} lower={}",
        adjust.value(),
        adjust.lower()
    );
    for name in ["thumb-minus", "thumb-plus", "aspect-choice"] {
        let widget =
            common::find_named(&window, name).unwrap_or_else(|| panic!("{name} is on the column"));
        crate::common::wait_until(
            &format!("{name} is laid out (it has pixels of its own)"),
            || widget.width() > 0 && widget.height() > 0,
        );
        assert!(
            widget.is_mapped(),
            "{name} is built but never painted (mapped=false) — the column opens below it"
        );
        let y = top_y(&widget);
        assert!(
            y >= top - CLIP_SLACK_PX && y + widget.height() <= bottom + CLIP_SLACK_PX,
            "{name} is painted at y={y}..{}, outside the scroller's y={top}..{bottom}: the idle column \
             is showing the tail of the readings, not their first line",
            y + widget.height()
        );
    }
    // §1's order in the column: the thumbnail row comes before the aspect row, which comes before the clocks.
    let thumb_key = common::find_named(&window, "cut-readout-thumbnails-key")
        .expect("the Thumbnails row has its key label");
    let aspect_key = common::find_named(&window, "cut-readout-aspect-ratio-key")
        .expect("the Aspect ratio key label");
    let (thumb_top, aspect_top) = (top_y(&thumb_key), top_y(&aspect_key));
    println!("COLUMN order: 'Thumbnails:' y={thumb_top}, 'Aspect ratio:' y={aspect_top}");
    assert!(
        thumb_top < aspect_top,
        "'Thumbnails:' reads above 'Aspect ratio:' (§1's row order): {thumb_top} vs {aspect_top}"
    );
    for label in naivepost::cut_screen::IDLE_FORM_ROWS {
        let key = common::find_named(
            &window,
            &format!("{}-key", naivepost::cut_screen::readout_widget(label)),
        )
        .unwrap_or_else(|| panic!("the `{label}` row has no key label"));
        assert!(
            key.is_mapped(),
            "`{label}`'s key is built but not painted on the idle column ({where_})"
        );
    }
}

/// The preview's height ask can never push the page's divider past §A's opening. The ask IS the start
/// pane's minimum: ask above 380 px and the paned clamps its handle down to it, the pane overflows the
/// divider, and everything above the new handle position — §1's 'Thumbnails:' first — is clipped out of
/// what the page draws. Measured before this rule held: `cut-top` 417 px tall at y=45 inside a paned
/// whose own edge sat at y=82, and the shot opened on 'Aspect ratio:'.
fn check_the_preview_ask_cannot_open_past_the_split(app: &adw::Application) {
    let _ = app;
    let lane = naivepost::fx_lane::lane_height_px(1) as i32;
    // A pane bigger than §A's opening: the ceiling, not the pane, decides.
    let grown = naivepost::ui::cut_page_layout::preview_ask_ceiling(500, lane);
    assert!(
        grown + lane <= naivepost::ui::cut_page_layout::SELF_SPLIT_TOP_PX,
        "an over-large pane cannot ask the picture taller than \u{a7}A's 380 px opening: {grown} + {lane}"
    );
    // A pane smaller than the opening: the pane's own room decides, nothing is padded up.
    let tight = naivepost::ui::cut_page_layout::preview_ask_ceiling(240, lane);
    assert_eq!(
        tight + lane,
        240,
        "a short pane keeps its own room, not \u{a7}A's number: {tight} + {lane}"
    );
    // Never zero or negative: a pane no taller than the lane still gets a picture.
    assert!(
        naivepost::ui::cut_page_layout::preview_ask_ceiling(lane, lane) >= 1,
        "a degenerate pane still asks for at least one pixel of picture"
    );
    RAN_CEILING.store(true, Ordering::SeqCst);
}

/// A widget's top edge in window coordinates, the vertical twin of [`span`].
#[allow(deprecated)] // 4.12's replacement needs a surface node this tree does not hand out
fn top_y(widget: &gtk::Widget) -> i32 {
    let native = widget
        .native()
        .expect("the page is inside a realized window");
    let (_, y) = widget
        .translate_coordinates(&native, 0.0, 0.0)
        .expect("the widget is laid out");
    y as i32
}

/// How far past the scroller's edge a widget may sit and still count as on screen: a row's own border and
/// the sub-pixel rounding of `translate_coordinates`, not a row's worth of slack.
const CLIP_SLACK_PX: i32 = 4;

/// The same Cut page as [`cut_page`] but sized to §1's MINIMUM window, which is where the form column gets
/// its shortest pane and the clipping showed up.
fn cut_page_at_minimum(app: &adw::Application) -> adw::ApplicationWindow {
    let window = cut_page(app);
    common::size_to_minimum(&window);
    settle();
    window
}

/// §A's pinned heading: absent on the idle column, and once a form is open it carries the title at its
/// left and the ✕ at its right end. The form is opened through the program's own door — the chooser seam
/// `tests/suite/cut_form_frame_widgets.rs` uses, i.e. what the native file dialog's callback calls.
fn check_the_pinned_heading_shows_only_with_a_form(app: &adw::Application) {
    let window = cut_page(app);
    let heading =
        common::find_named(&window, "cut-form-heading").expect("§A's pinned heading row exists");
    assert!(
        !heading.is_visible(),
        "the idle column starts with the readings and NO heading (§1)"
    );
    // Left up, not closed: the checks that run after this one hold a window on purpose, and `cut_page`
    // closes the one the previous check left behind. Closing here would take the tree this round's other
    // checks are still reading (`hold_last_window` keeps the newest window alive for exactly that reason).

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
    assert!(
        naivepost::ui::insert_open(&window).is_some(),
        "the insert form is open"
    );

    assert!(heading.is_visible(), "an open form pins its heading");
    let title = common::find_named(&window, "cut-form-title")
        .and_then(|w| w.downcast::<gtk::Label>().ok())
        .expect("the heading carries a title label");
    let close = common::find_named(&window, "cut-form-close").expect("the heading carries the ✕");
    let words = title.text().to_string();
    assert!(
        !words.trim().is_empty(),
        "the open form's heading names what is open"
    );
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
    println!(
        "GUTTER cut_screen::GUTTER_PX = {gutter} px, cut_view::gutter_px() = {}",
        naivepost::ui::cut_view::gutter_px()
    );
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
        node.observe_children()
            .iter::<glib::Object>()
            .flatten()
            .any(|child| {
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
    // §1's order IN the gutter: the whole-cut fold-all badge is its first control, above every lane's own
    // sound switch ("fold-all badge, per-lane and per-row sound switches, empty-row cross"). And it has to
    // be PAINTED there: the rail is dark ink, so a badge left in the theme's dark label colour sits
    // dark-on-dark at the top of the column and reads as absent (what the render showed).
    crate::common::wait_until(
        "the fold-all badge is laid out (it has pixels of its own)",
        || badge.width() > 0 && badge.height() > 0,
    );
    assert!(
        badge.is_mapped(),
        "badge 23 is built but never painted (mapped=false): {} px at y={}",
        badge.height(),
        top_y(&badge)
    );
    let (badge_top, switch_top) = (top_y(&badge), top_y(&switch));
    println!("GUTTER order: fold-all y={badge_top}, `{lane}` switch y={switch_top}");
    assert!(
        badge_top <= switch_top,
        "the fold-all badge sits at the gutter's TOP, above the lane switches (§1's order): \
         badge y={badge_top} vs {lane} switch y={switch_top}"
    );
    RAN_GUTTER.store(true, Ordering::SeqCst);
}

fn round() {
    common::one_app_round(|app| {
        println!("CHECK 1 the eight idle readings are label left, value right");
        check_the_readings_are_label_left_value_right(app);
        println!("CHECK 1b every reading is PAINTED on the idle column, with no heading");
        check_the_idle_column_paints_every_reading(app);
        println!("CHECK 1d at the minimum window the column opens on its FIRST reading");
        check_the_idle_column_opens_on_its_first_reading(app);
        println!("CHECK 1c the thumbnail ladder clicks step the number it drives");
        check_the_thumbnail_ladder_steps_its_readout(app);
        println!("CHECK 2 the pinned heading shows only while a form is open");
        check_the_pinned_heading_shows_only_with_a_form(app);
        println!("CHECK 3 the gutter is a 30 px column, not a band");
        check_the_gutter_is_a_column_not_a_band(app);
        println!("CHECK 4 the preview's height ask cannot open the page past \u{a7}A's 380 px");
        check_the_preview_ask_cannot_open_past_the_split(app);
    });
}

#[test]
fn sec_05_cut_1_screen_the_idle_readings_sit_label_left_and_value_right() {
    common::pin_private_cwd();
    round();
    assert!(
        RAN_VALUES.load(Ordering::SeqCst),
        "the reading-alignment check never ran"
    );
}

#[test]
fn sec_05_cut_1_screen_the_idle_column_shows_its_readings() {
    common::pin_private_cwd();
    round();
    assert!(
        RAN_IDLE_PAINTED.load(Ordering::SeqCst),
        "the idle-column-painted check never ran"
    );
}

#[test]
fn sec_05_cut_1_screen_the_thumbnail_ladder_steps_the_number_it_drives() {
    common::pin_private_cwd();
    round();
    assert!(
        RAN_LADDER.load(Ordering::SeqCst),
        "the thumbnail ladder check never ran"
    );
}

#[test]
fn sec_05_cut_1_screen_the_pinned_heading_shows_only_while_a_form_is_open() {
    common::pin_private_cwd();
    round();
    assert!(
        RAN_HEADING.load(Ordering::SeqCst),
        "the pinned-heading check never ran"
    );
}

#[test]
fn sec_05_cut_1_screen_the_idle_column_paints_its_top_readings_as_well() {
    common::pin_private_cwd();
    round();
    assert!(
        RAN_TOP_READING.load(Ordering::SeqCst),
        "the minimum-window first-reading check never ran"
    );
}

#[test]
fn sec_05_cut_1_screen_the_gutter_is_a_30_px_column_not_a_band_of_its_own() {
    common::pin_private_cwd();
    round();
    assert!(
        RAN_GUTTER.load(Ordering::SeqCst),
        "the gutter check never ran"
    );
}

#[test]
fn sec_05_cut_1_screen_the_preview_ask_stays_inside_the_page_split() {
    common::pin_private_cwd();
    round();
    assert!(
        RAN_CEILING.load(Ordering::SeqCst),
        "the preview ask-ceiling check never ran"
    );
}
