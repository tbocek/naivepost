// F2.11 (Folds and rows) — THE WIRE. The rules live in `src/cut_fold.rs` and are proven by
// tests/cut_fold_rows.rs; this file proves the Cut page's real widgets reach them. Every check fires a
// named widget the way GTK does (`emit_by_name("clicked")`) and then asserts the SAME state the logic
// test asserts — the cut's own `folds`, the laid-out cells, `nrows`, the lane/pin/shift maps — rather
// than a painted pixel.
//
// Widgets fired by name: `fold-button-<i>` (S1 − / +), `fold-all-button` (S1 the gutter badge),
// `row-cross-<n>` (S2 an emptied bottom row) and `lane-cross-<name>` (S2 a cut lane).
//
// One `Application::run` per binary: `g_application_run` refuses a second claimant of the default main
// context, so every check runs inside one `activate` and reports through the flags asserted at the end.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Lane, Seg};
use naivepost::shell::Page;
use naivepost::timeline::{self, Recording, Span};
use naivepost::ui;
use naivepost::cut_fold;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_FOLD: AtomicBool = AtomicBool::new(false);
static RAN_UNFOLD: AtomicBool = AtomicBool::new(false);
static RAN_ALL: AtomicBool = AtomicBool::new(false);
static RAN_SAVED: AtomicBool = AtomicBool::new(false);
static RAN_NO_ROOM: AtomicBool = AtomicBool::new(false);
static RAN_ROW_CROSS: AtomicBool = AtomicBool::new(false);
static RAN_LANE_CROSS: AtomicBool = AtomicBool::new(false);

/// Pixels-per-second the placeholder strip (and therefore the badges) is drawn at.
const PPS: f64 = ui::TRACK_STRIP_PPS;

/// Kept footage on camera 0.
fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

/// A window on the Cut tab with `seeded` as its cut and `tape` as the session's filmed recordings.
/// The tape is written into the session's own `cut.json` beside a `recordings` sidecar the page reads
/// through `timeline::kept_footage_recordings`; here it is passed in so each check controls both
/// halves of the fixture rather than inheriting whatever the demo project happens to hold.
/// A window on the Cut tab with BOTH halves of the fixture declared: `tape` = what the SESSION filmed
/// (via `ui::set_session_recordings`, because a dropped stretch only exists inside a run longer than
/// the clips kept in it — see `ui::page_recordings`), and `seeded` = the cut under test. Both are set
/// before the badges are drawn, so the page's own list is what the checks read back.
fn cut_page_taped(
    app: &adw::Application,
    tape: &[Recording],
    seeded: &cut::Cut,
) -> adw::ApplicationWindow {
    // Release the PREVIOUS check's window before building this one. A closed GTK window is not
    // destroyed — its widget tree survives with every `camera-row-1` / `fold-badges` still parented to
    // it — and a name lookup from a new window can reach those leftovers, which makes the next append
    // collide (`gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed`). Dropping our
    // own last handle first lets them go.
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    // The tape first, then the cut: the badge list is built from the tape's runs, so ordering the two
    // the other way round would draw the previous check's gaps for one frame.
    ui::set_session_recordings(tape);
    ui::seed_review_cut(&window, seeded);
    ui::set_watched_row(None);
    ui::refresh_camera_rows(&window);
    ui::refresh_fold_badges(&window);
    settle();
    window
}

/// The common case: the session filmed 0–120 as one take.
fn cut_page(app: &adw::Application, seeded: &cut::Cut) -> adw::ApplicationWindow {
    cut_page_taped(app, &tape_for(seeded), seeded)
}

/// Let the main context run what the widget emissions queued.

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    // Found inside the holder this item draws into, via the same accessor the page uses, so the widget a
    // check fires is the one THIS window owns. A tree-wide `find_widget_by_name` can hand back another
    // tab's or another window's copy of a name — which is why the fold half reads its own box.
    if name == "undo-button" {
        // The undo stack lives in the toolbar, not in this item's holders; it is fired here only to show
        // a fold survives an Undo.
        return ui::line_step_button(window, name).expect("the toolbar drew its undo-button");
    }
    let holder = if name.starts_with("fold-") {
        ui::fold_badges_box(window).expect("the Cut page drew its fold-badges holder")
    } else {
        ui::camera_rows_box(window).expect("the Cut page drew its camera-rows box")
    };
    // Walk the holder's SUBTREE: a row cross is packed inside its `camera-row-<n>` box, not directly on
    // the rows box, so a direct-children scan would never see it. Scoped to this holder rather than the
    // whole window, which is what keeps the widget found THIS window's own.
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Button> {
        if node.widget_name() == name {
            return node.clone().downcast::<gtk::Button>().ok();
        }
        for child in node.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
            if let Some(found) = walk(&widget, name) {
                return Some(found);
            }
        }
        None
    }
    walk(holder.upcast_ref(), name)
        .unwrap_or_else(|| panic!("no widget named `{name}` in this window's holder"))
}

/// Two kept clips out of a 0–120 recording: drops a head (0–10), a hole (30–40) and a tail (60–120),
/// which is the same fixture `tests/cut_fold_rows.rs` folds. The whole session has to be FILMED for
/// those three stretches to exist at all — `dropped_gaps` walks the runs, and a run comes from a
/// recording, not from the segments — so the tape rides along as an insert marker over its own span:
/// inserts lay no footage down, so it contributes no clip and no gap of its own.
fn dropped() -> cut::Cut {
    let mut fresh = cut::Cut::default();
    fresh.segs = vec![clip(10.0, 30.0), clip(40.0, 60.0)];
    fresh
}

/// The page's own view of this cut: the two kept clips plus the tape they were cut out of, exactly as
/// `refresh_fold_badges` sees it — one recording spanning 0–120, hence one run.
fn page_view(cut_: &cut::Cut) -> Vec<Recording> {
    tape_for(cut_)
}

/// The recordings the page would draw rows for: the session's tape over 0–120.
fn tape_for(_cut_: &cut::Cut) -> Vec<Recording> {
    vec![Recording { base: "session-tape".to_string(), start: 0.0, end: 120.0 }]
}

/// The laid-out band the badges are placed on, from the SAME inputs `refresh_fold_badges` uses.
fn band_of(cut_: &cut::Cut) -> Span {
    let runs = timeline::filmed_runs(&tape_for(cut_));
    Span::new(timeline::cells(&runs, &cut_.folds), PPS, naivepost::cut_screen::GUTTER_PX)
}

/// The gap index whose stretch is `gap`, read off the WINDOW's own badge list — which is what
/// `refresh_fold_badges` stored when it drew them, so the position IS the button number minus one and
/// no check hard-codes a number that could drift with the cut.
fn gap_index(window: &adw::ApplicationWindow, gap: (f64, f64)) -> usize {
    ui::fold_gaps(window)
        .iter()
        .position(|held| *held == gap)
        .unwrap_or_else(|| panic!("no badge over {gap:?}: {:?}", ui::fold_gaps(window)))
}

/// S1: clicking the − on the 10 s hole folds it to a seam, stores the fold, takes the stretch's width
/// away, and prints the rule's own sentence into the status line.
fn f2_11_s1_a_click_on_the_minus_folds_the_hole(app: &adw::Application) {
    let window = cut_page(app, &dropped());
    let index = gap_index(&window, (30.0, 40.0));
    button(&window, &format!("fold-button-{}", index + 1)).emit_by_name::<()>("clicked", &[]);
    settle();

    let held = ui::review_fold_rows(&window);
    assert_eq!(held.folds, vec![[30.0, 40.0]], "the fold is stored on the cut");
    assert!(cut_fold::is_folded(&held.folds, (30.0, 40.0)));

    // The same cell/width assertions the logic test makes, recomputed off the window's own cut.
    // The SESSION's tape, which is what the page laid out (see `ui::page_recordings`) — recomputing
    // from the kept segments here would answer a different question than the one the badge drew.
    let runs = timeline::filmed_runs(tape_for(&held).as_slice());
    let cells = timeline::cells(&runs, &held.folds);
    assert!(
        cells.iter().any(|cell| cell.folded
            && (cell.start - 30.0).abs() < 1e-9
            && (cell.end - 40.0).abs() < 1e-9),
        "the band cuts a folded cell at the hole: {cells:?}"
    );
    let span = Span::new(cells, PPS, naivepost::cut_screen::GUTTER_PX);
    assert!(
        (span.x_of(32.0) - span.x_of(38.0)).abs() < 1e-9,
        "both seconds share the seam x — the stretch takes no width"
    );
    assert!(span.x_of(32.0) > span.x_of(29.0), "which sits where the footage before it ended");

    assert_eq!(
        status_text(&window),
        "folded 0:30 \u{2013} 0:40 (10.0 s)",
        "the status carries `cut_fold::toggle_fold`'s own sentence"
    );
    // And the badge flipped to a + over the same gap.
    let again = button(&window, &format!("fold-button-{}", index + 1));
    assert_eq!(again.label().unwrap_or_default().as_str(), "+", "a folded gap wears a +");
    window.close();
}

/// S1: a second click on the + opens what the first closed — folds empty, no folded cell left, and the
/// status says the other half of the same sentence.
fn f2_11_s1_a_second_click_unfolds_it_again(app: &adw::Application) {
    let window = cut_page(app, &dropped());
    let index = gap_index(&window, (30.0, 40.0));
    let name = format!("fold-button-{}", index + 1);
    button(&window, &name).emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(ui::review_fold_rows(&window).folds.len(), 1);

    button(&window, &name).emit_by_name::<()>("clicked", &[]);
    settle();

    let held = ui::review_fold_rows(&window);
    assert!(held.folds.is_empty(), "nothing left folded: {:?}", held.folds);
    // The SESSION's tape, which is what the page laid out (see `ui::page_recordings`) — recomputing
    // from the kept segments here would answer a different question than the one the badge drew.
    let runs = timeline::filmed_runs(tape_for(&held).as_slice());
    assert!(
        timeline::cells(&runs, &held.folds).iter().all(|cell| !cell.folded),
        "the band lays the gap out again"
    );
    assert_eq!(status_text(&window), "unfolded 0:30 \u{2013} 0:40 (10.0 s)");
    window.close();
}

/// S1: the gutter badge folds every foldable gap at once and unfolds them all on the next press —
/// including the head and tail, which fold like any other dropped stretch.
fn f2_11_s1_the_gutter_badge_folds_and_unfolds_all(app: &adw::Application) {
    let window = cut_page(app, &dropped());
    let gaps = ui::fold_gaps(&window);
    assert_eq!(gaps.len(), 3, "head, hole and tail all wear badges: {gaps:?}");

    button(&window, "fold-all-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let held = ui::review_fold_rows(&window);
    assert_eq!(held.folds.len(), 3, "every dropped gap folded: {:?}", held.folds);
    for gap in &gaps {
        assert!(cut_fold::is_folded(&held.folds, *gap), "{gap:?} still open");
    }
    assert!(status_text(&window).starts_with("folded 3 seams"), "{}", status_text(&window));

    button(&window, "fold-all-button").emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(ui::review_fold_rows(&window).folds.is_empty(), "and opened again");
    assert!(status_text(&window).starts_with("unfolded 3 seams"), "{}", status_text(&window));
    window.close();
}

/// S1: the fold IS saved — `folds` comes back out of `cut/cut.json` through the project's own load path
/// — and it is NOT an undo step: `Snapshot` leaves `folds` alone, so Ctrl+Z's rule restores the seven
/// edited fields while the fold stays exactly where the person left it.
fn f2_11_s1_a_fold_is_saved_and_never_undone(app: &adw::Application) {
    let window = cut_page(app, &dropped());
    let index = gap_index(&window, (30.0, 40.0));
    button(&window, &format!("fold-button-{}", index + 1)).emit_by_name::<()>("clicked", &[]);
    settle();

    // Read back from disk the way the page reads it on open.
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    let tree = naivepost::layout::Tree::new(&dir).expect("the session tree exists after a save");
    let reloaded = cut::load(&tree).expect("cut.json reads back");
    assert_eq!(reloaded.folds, vec![[30.0, 40.0]], "the fold survived the round trip");

    // An edit that DOES belong to the undo stack, then Undo: the segments go back, the fold stays.
    ui::note_edit(&window);
    let before_segs = ui::review_fold_rows(&window).segs.clone();
    assert_eq!(before_segs.len(), 2);
    button(&window, "undo-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let after = ui::review_fold_rows(&window);
    assert_eq!(after.folds, vec![[30.0, 40.0]], "Undo left the view where it was");
    assert_eq!(after.segs.len(), 2, "and only the edited fields were restored");
    window.close();
}

/// S1: a gap narrower than `cut_fold::FOLD_MIN_PX` gets NO widget at all — not a greyed one, none —
/// because the badge would be wider than the thing it folds.
fn f2_11_s1_a_tiny_gap_gets_no_widget(app: &adw::Application) {
    let mut tight = cut::Cut::default();
    // 0.5 s of head at 4 px/s is 2 px: under the 20 px floor.
    tight.segs = vec![clip(0.5, 30.0), clip(40.0, 60.0)];
    let window = cut_page(app, &tight);
    let gaps = ui::fold_gaps(&window);
    assert!(!gaps.contains(&(0.0, 0.5)), "the 2 px head wore no badge: {gaps:?}");
    assert!(gaps.contains(&(30.0, 40.0)), "while the 40 px hole did: {gaps:?}");
    // No numbered button anywhere stands for the too-narrow gap.
    for i in 1..=gaps.len() {
        assert_ne!(
            ui::folded_gap_at(&window, i - 1),
            Some((0.0, 0.5)),
            "no `fold-button-{i}` folds a gap with no room"
        );
    }
    assert!(
        ui::folded_gap_at(&window, gaps.len()).is_none(),
        "and nothing past the list answers"
    );
    window.close();
}

/// S2: the ✕ on an emptied bottom row takes it off the band (the `nrows` floor is spent), while the
/// cross over a row that still carries footage is insensitive — the refusal is visible, not hidden.
fn f2_11_s2_the_row_cross_removes_an_emptied_bottom_row(app: &adw::Application) {
    let mut seeded = dropped();
    // Hold row 1 open with the floor so there IS an empty bottom row to take away.
    seeded.nrows = 2;
    let window = cut_page(app, &seeded);
    assert_eq!(ui::review_fold_rows(&window).nrows, 2);

    // Row 0 holds footage: its cross is dead.
    assert!(
        !button(&window, "row-cross-1").is_sensitive(),
        "a row with footage offers no ✕"
    );
    // Row 1 (index 1) is empty and is the bottom row: its cross is live.
    assert!(button(&window, "row-cross-2").is_sensitive(), "the emptied bottom row's ✕ is live");

    button(&window, "row-cross-2").emit_by_name::<()>("clicked", &[]);
    settle();
    let after = ui::review_fold_rows(&window);
    assert_eq!(after.nrows, 1, "the floor that held the row is spent");
    let recordings = timeline::kept_footage_recordings(&after);
    assert_eq!(
        timeline::row_count(&recordings, &after),
        1,
        "and the page draws one row now"
    );
    assert!(status_text(&window).contains("removed the empty row 2"), "{}", status_text(&window));
    window.close();
}

/// S2: a lane's ✕ removes ALL FOUR things the spec names in one press — the lane entry, its pin, its
/// shift, and its pictures and sound — leaving another lane's business untouched.
fn f2_11_s2_the_lane_cross_takes_pin_shift_pictures_and_sound(app: &adw::Application) {
    const LANE: &str = "Second pass";
    let mut seeded = dropped();
    seeded.lanes = vec![
        Lane {
            name: LANE.to_string(),
            src: "project:media/pass.mp4".into(),
            at: 0.0,
            off: 0.0,
            dur: 30.0,
        },
        Lane {
            name: "Desk mic".to_string(),
            src: "project:media/mic.wav".into(),
            at: 0.0,
            off: 0.0,
            dur: 30.0,
        },
    ];
    seeded.rows.insert(LANE.to_string(), 1);
    seeded.shift.insert(LANE.to_string(), -19.0);
    // Its picture, its sound laid over footage, and another lane's scene.
    seeded.segs.push(Seg {
        s: 5.0,
        e: 15.0,
        ins: "project:media/pass.mp4".into(),
        dur: 10.0,
        ..Default::default()
    });
    seeded.segs.push(Seg {
        s: 70.0,
        e: 80.0,
        lane: LANE.to_string(),
        ..Default::default()
    });
    seeded.segs.push(Seg {
        s: 85.0,
        e: 95.0,
        lane: "Desk mic".to_string(),
        ..Default::default()
    });
    let window = cut_page(app, &seeded);

    button(&window, &format!("lane-cross-{LANE}")).emit_by_name::<()>("clicked", &[]);
    settle();
    let after = ui::review_fold_rows(&window);

    // 1. the lane entry
    assert!(after.lanes.iter().all(|lane| lane.name != LANE), "the lane is gone: {:?}", after.lanes);
    // 2. its pin
    assert!(!after.rows.contains_key(LANE), "its pin went with it: {:?}", after.rows);
    // 3. its shift
    assert!(!after.shift.contains_key(LANE), "and its shift: {:?}", after.shift);
    // 4. its pictures AND its sound
    assert!(
        after.segs.iter().all(|seg| seg.ins != "project:media/pass.mp4"),
        "its picture is off the cut"
    );
    assert!(
        after.segs.iter().all(|seg| seg.lane != LANE),
        "and its laid-over sound too"
    );
    // Another lane's scene survives untouched.
    assert!(
        after.segs.iter().any(|seg| seg.lane == "Desk mic"),
        "the other lane keeps its scene"
    );
    assert_eq!(after.lanes.len(), 1, "and the other lane itself");
    assert!(status_text(&window).contains("removed the Second pass lane"), "{}", status_text(&window));
    window.close();
}

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            f2_11_s1_a_click_on_the_minus_folds_the_hole(app);
            RAN_FOLD.store(true, Ordering::SeqCst);
            f2_11_s1_a_second_click_unfolds_it_again(app);
            RAN_UNFOLD.store(true, Ordering::SeqCst);
            f2_11_s1_the_gutter_badge_folds_and_unfolds_all(app);
            RAN_ALL.store(true, Ordering::SeqCst);
            f2_11_s1_a_fold_is_saved_and_never_undone(app);
            RAN_SAVED.store(true, Ordering::SeqCst);
            f2_11_s1_a_tiny_gap_gets_no_widget(app);
            RAN_NO_ROOM.store(true, Ordering::SeqCst);
            f2_11_s2_the_row_cross_removes_an_emptied_bottom_row(app);
            RAN_ROW_CROSS.store(true, Ordering::SeqCst);
            f2_11_s2_the_lane_cross_takes_pin_shift_pictures_and_sound(app);
            RAN_LANE_CROSS.store(true, Ordering::SeqCst);
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_11_wire_the_fold_and_row_widgets_reach_the_rules_through_real_clicks() {
    // Pin cwd to our own temp root BEFORE anything builds: the presses resolve the project through
    // `startup::session_dir(current_dir())`, and leaving cwd at `rust/` writes a stray
    // `rust/session.naivepost/` into the repo. It is also where `save_folds` writes `cut.json`.
    let root = std::env::temp_dir().join(format!("np-folds-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    window_round();
    assert!(RAN_FOLD.load(Ordering::SeqCst), "the − click check never ran");
    assert!(RAN_UNFOLD.load(Ordering::SeqCst), "the + click check never ran");
    assert!(RAN_ALL.load(Ordering::SeqCst), "the gutter-badge check never ran");
    assert!(RAN_SAVED.load(Ordering::SeqCst), "the saved-and-not-undone check never ran");
    assert!(RAN_NO_ROOM.load(Ordering::SeqCst), "the no-room check never ran");
    assert!(RAN_ROW_CROSS.load(Ordering::SeqCst), "the row-cross check never ran");
    assert!(RAN_LANE_CROSS.load(Ordering::SeqCst), "the lane-cross check never ran");
}
