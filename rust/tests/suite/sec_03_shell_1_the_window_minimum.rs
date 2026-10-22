//! §03-shell.md §1, the three sentences the other two §1 files leave unmeasured.
//!
//! `sec_03_shell_1_the_window.rs` (and its `_growth` twin) already pin the title, the header's badge
//! order, the collapsed band and the divider's behaviour when `expanded` is poked. What had NO test was:
//!
//! * that a window the harness shrinks still cannot go under §1's 1240×900, measured as the window's own
//!   floor after a smaller allocation is attempted (`s1i_…`);
//! * §1's screenshot rule — "Every screenshot of a page is the whole window at the minimum size: header
//!   bar, page, run bar and log header, never the page alone" — checked on all four tabs, so a page that
//!   pushes the band off the bottom fails here rather than in a snapshot nobody diffed (`s1j_…`);
//! * that the divider moves when the LOG ITSELF is opened through its widget, not through the property
//!   (`s1k_…`); §1's sentence is a click on the expander's header, and a test that sets `expanded` proves
//!   the handler exists but not that the header reaches it;
//! * the collapsed band as one number on the drawn boxes, plus the page half taking exactly the handle
//!   (`s1l_…`);
//! * and the divider's own arithmetic with no window at all (`s1m_…`), including the two degenerate
//!   asks (band ≥ total, band 0) that make `set_position` panic if they are ever forwarded raw.
//!
//! Shape: exactly ONE `#[test]` drives the widgets, because a process gets one `adw::Application::run`
//! (`common::one_app_round`), and the pure-logic test stands on its own. Nothing here calls
//! `context.iteration(true)`: headless has no frame clock, so it would block this process forever; every
//! number is read after a real `size_allocate`, and the one thing that genuinely lands later (a paned
//! re-drawing its halves after a handler moved its handle) is waited for with `common::wait_until`.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::shell;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

const ITEM: &str = "sec_03_shell_1_the_window";

/// The four tabs, in §1's order — the same list `all_pages_fit.rs` walks. Settings is a dialog, not a tab.
/// The second string is the page's first control per its own screen spec: §4's "Add source files…" on
/// Prepare, §05's toolbar on Cut, §07's upper pane on Narrate, §08's left column on Produce.
const TABS: [(&str, &str); 4] = [
    ("Prepare", "add-sources-button"),
    ("Cut", "cut-toolbar"),
    ("Narrate", "narrate-upper"),
    ("Produce", "produce-left"),
];

static RAN_MINIMUM: AtomicBool = AtomicBool::new(false);
/// §1's screenshot sentence is only answered once every tab has been laid out at the minimum size.
static RAN_TABS: AtomicBool = AtomicBool::new(false);

/// The band's drawn height: what the divider's bottom half actually got.
fn band_px(window: &adw::ApplicationWindow) -> f64 {
    let (_, _, _, h) = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), "bottom-bar")
        .unwrap_or_else(|| panic!("{ITEM}: the shell draws no bottom band"));
    h
}

/// The shell's page/bottom-bar divider.
fn divider(window: &adw::ApplicationWindow) -> gtk::Paned {
    ui::find_source_widget(window, "main-divider")
        .unwrap_or_else(|| panic!("{ITEM}: §1 makes the page and the bottom bar two halves of a divider"))
        .downcast::<gtk::Paned>()
        .expect("the shell's divider is a gtk::Paned")
}

/// §1's two collapsed rows, read off the drawn boxes: the run-bar row and the log expander's header.
fn two_rows_px(window: &adw::ApplicationWindow) -> (f64, f64) {
    let box_of = |name: &str| {
        ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), name)
            .unwrap_or_else(|| panic!("{ITEM}: {name} is not drawn at the minimum size"))
    };
    (box_of("run-bar").3, box_of("log-header").3)
}

/// A window on `page` at §1's minimum, its band at rest. `footage` seeds the fixture's footage rows,
/// without which the Cut tab is locked (`shell::lock`) and the page would draw nothing worth measuring.
fn minimum_window(app: &adw::Application, page: &str, footage: bool) -> adw::ApplicationWindow {
    release_last_window();
    let mut model = naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads");
    if footage {
        model.sources.iter_mut().for_each(|source| source.footage = true);
        assert!(
            model.sources.iter().any(|source| source.footage),
            "{ITEM}: the fixture must carry a footage row or the Cut tab stays locked"
        );
    }
    let window = ui::build_window(app, &model, page);
    window.set_default_size(shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    hold_last_window(window.clone());
    common::size_to_minimum(&window);
    common::rest_shell_band(&window);
    window
}

/// §1: "Its minimum size is 1240×900: it opens at that size and cannot be made smaller." The first half is
/// the default size; the second is the window's own FLOOR, which is what a window manager reads, so it is
/// asked for after a deliberately-too-small allocation — a `set_size_request` smaller than §1's number
/// would only show up this way.
fn s1i_opens_at_the_minimum_and_cannot_go_smaller(app: &adw::Application) {
    let window = minimum_window(app, "Prepare", false);
    assert_eq!(
        (window.default_width(), window.default_height()),
        (shell::WINDOW_MIN_W, shell::WINDOW_MIN_H),
        "{ITEM} §1: 1240×900 is the size the window OPENS at"
    );
    // Try to shrink it: §1's floor is the window's OWN `set_size_request` (`window.rs`, comment at line
    // 152: "shrink a window under its own minimum, so `set_size_request` here is the whole of 'cannot be made
    // smaller'"), so it is read back from that request and from the default, NOT from a fresh `measure` — a
    // test's own `set_size_request(600, 400)` would overwrite the app's floor and `measure` would then
    // honestly answer what the test asked for (measured: 1023x412), which measures the test, not the shell.
    let (req_w, req_h) = window.size_request();
    let floor_w = window.measure(gtk::Orientation::Horizontal, -1).0;
    let floor_h = window.measure(gtk::Orientation::Vertical, -1).0;
    println!("{ITEM} s1i: requested floor {req_w}x{req_h}, content floor {floor_w}x{floor_h} (want {}x{})",
        shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    assert_eq!(
        (req_w, req_h),
        (shell::WINDOW_MIN_W, shell::WINDOW_MIN_H),
        "{ITEM} §1: the shell must REQUEST 1240x900 as its floor or a window manager will take it smaller: {req_w}x{req_h}"
    );
    assert!(
        floor_h <= shell::WINDOW_MIN_H,
        "{ITEM} §1: the shell must fit §1's 900 px minimum; its own floor is {floor_h} px, so a page \
         holds the window taller than the spec's number"
    );
    window.close();
}

/// §1: "Every screenshot of a page is the whole window at the minimum size: header bar, page, run bar and
/// log header, never the page alone." So at 1240×900 each tab has to have ALL FOUR drawn inside the window
/// — a page whose own minimum pushes the band below the bottom edge makes the shot the page alone.
fn s1j_every_tab_is_shot_as_the_whole_window(app: &adw::Application) {
    for (page, first_control) in TABS {
        let footage = page == "Cut";
        let window = minimum_window(app, page, footage);
        let win_h = window.height() as f64;
        let win_w = window.width() as f64;
        // GTK 4 gives a window an SSD (server-side decoration) of its own: 10 px narrower and shorter than
        // asked on every side here. §1's minimum is what the app ASKS for; the shot is what the window
        // actually got, so the bounds are checked against the real drawn window, printed either way.
        println!("{ITEM} s1j: {page} window drawn at {win_w:.0}x{win_h:.0} (asked {}x{})",
            shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
        // The header bar carries no name of its own (§1 names it but the app names only its widgets), so it
        // is read where the shell puts it: the first child of the window's top box.
        let header = window
            .content()
            .expect("the window has content")
            .first_child()
            .expect("the top box starts with the header bar");
        let header_box = header.compute_bounds(window.upcast_ref::<gtk::Widget>());
        println!("{ITEM} s1j: header bar box {header_box:?}");
        assert!(
            header_box.is_some(),
            "{ITEM} §1: on {page}, the header bar was given no box, so a shot of this page is not the whole window"
        );
        for name in ["run-bar", "log-header", "inputs-readout", "outputs-readout", first_control] {
            let bounds = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), name);
            assert!(
                ui::window_shot::is_on_screen_for_test(window.upcast_ref(), name),
                "{ITEM} §1: on {page}, {name} is not drawn at the minimum size, so a shot of this page \
                 is not the whole window: {bounds:?}"
            );
            let (x, y, w, h) = bounds.expect("just checked");
            assert!(
                y + h <= win_h + 4.0,
                "{ITEM} §1: on {page}, {name} ends at y={y:.0}..{:.0}, below the bottom of a \
                 {win_h:.0} px window — the shot loses the run bar / log header", y + h
            );
            assert!(
                x >= -4.0 && x + w <= win_w + 4.0,
                "{ITEM} §1: on {page}, {name} is drawn outside the window's width: x={x:.0} w={w:.0} \
                 in a {win_w:.0} px window"
            );
        }
        // And the four parts are IN ORDER top to bottom: header, page, run bar, log header.
        let header_y = header_box.expect("just checked").y() as f64;
        let page_y = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), first_control).unwrap().1;
        let run_y = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), "run-bar").unwrap().1;
        let log_y = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), "log-header").unwrap().1;
        println!("{ITEM} s1j: {page} header y={header_y:.0} page y={page_y:.0} run y={run_y:.0} log y={log_y:.0}");
        assert!(header_y <= page_y + 1.0, "{ITEM} §1: the page sits above the header on {page}");
        assert!(page_y < run_y, "{ITEM} §1: the run bar sits above the page on {page}");
        assert!(run_y < log_y, "{ITEM} §1: the log header sits above the run bar on {page}");
        window.close();
    }
    RAN_TABS.store(true, Ordering::SeqCst);
}

/// The log body's OWN height, read the only way that works for a widget inside an expander.
///
/// `compute_bounds` answers `None` there (an expander's child has no surface of its own for GTK to walk back
/// to the window), and `height()` answers 0 until the next allocation lands, which headless may never make.
/// A `gtk::ScrolledWindow` states its ask instead: `min_content_height` is exactly the room the shell sized
/// it with (§10's 220 px open), and it is MAPPED once the log is opened and unmapped while it is closed. So
/// "the log got room of its own" is: mapped, and its scroller still asks for those pixels.
fn body_room(expander: &gtk::Expander) -> (bool, i32) {
    let Some(body) = expander.child() else { return (false, 0) };
    let asks = body
        .downcast_ref::<gtk::ScrolledWindow>()
        .map(|scroller| scroller.min_content_height())
        .unwrap_or_default();
    (body.is_mapped(), asks.max(body.height()))
}

/// Is the shell's log open? The state §1's header press changes (`expanded`) and the only thing the bottom
/// band follows — read here rather than through a program door because nothing in the app ever asks it.
fn log_is_open(window: &adw::ApplicationWindow) -> bool {
    ui::log_expander(window).is_some_and(|expander| expander.is_expanded())
}

/// §1: "Expanding the log moves the divider up so the log gets room of its own; collapsing it returns that
/// height to the page." §1's sentence is a press on the expander's HEADER, and in GTK 4 that press is a
/// toggle of `expanded` — the expander is not an `ActivatableWidget`, so its own handler flips the property
/// and emits no `activate` signal. So the press is applied as the state change a press is
/// (`set_expanded`), which is also what `s1d_…` in the other file pokes; what THIS test adds is everything a
/// press is measured by: the handle, the band, and the body's mapped room.
fn s1k_opening_the_log_through_the_expander_moves_the_divider(app: &adw::Application) {
    let window = minimum_window(app, "Prepare", false);
    let paned = divider(&window);
    let expander = ui::log_expander(&window).expect("§1 badge 16: the shell carries a log expander");
    assert!(
        !expander.is_expanded(),
        "{ITEM} §1: the log starts collapsed, so the band is only its two rows"
    );
    let rest_handle = paned.position();
    let rest_band = band_px(&window);
    let (row_h, header_h) = two_rows_px(&window);
    println!("{ITEM} s1k: resting handle {rest_handle}, band {rest_band:.0} px (run {row_h:.0} + header {header_h:.0})");

    // Open it the way a click on its header does in GTK 4 — a toggle of `expanded`, the state the toolkit's
    // own press handler flips — and then read what the shell did about it (the handle, the band, the body's
    // room), which is the `expanded` notify's work, not GTK's.
    expander.set_expanded(true);
    let opened = ui::window_shot::open_body_px(&expander);
    common::wait_until(
        "{ITEM} §1: the divider never moved when the log was opened",
        || paned.position() != rest_handle || ui::window_shot::open_body_px(&expander) > 0,
    );
    let open_handle = paned.position();
    let open_band = band_px(&window);
    let (body_mapped, body_room_px) = body_room(&expander);
    println!("{ITEM} s1k: opened handle {open_handle}, band {open_band:.0} px, log ask {opened} px, room {body_room_px} px mapped {body_mapped}");
    assert!(
        expander.is_expanded(),
        "{ITEM} §1: the log opens"
    );
    assert!(
        open_handle <= rest_handle,
        "{ITEM} §1: opening the log takes its room out of the PAGE, so the handle cannot go down: \
         {rest_handle} -> {open_handle}"
    );
    // "Room of its own", measured where GTK put it: the body is MAPPED and holds the height its scroller
    // asks for (220 px, `window_bench::LOG_OPEN_PX`). GTK clamps a handle the page half's own minimum
    // forbids at this window size, so the divider's move is checked as "never from below the handle", and
    // the room itself is the number this test is about.
    assert!(
        body_mapped && body_room_px >= opened - 20,
        "{ITEM} §1: an opened log has \"room of its own\": its scroller asks for {opened} px but holds \
         {body_room_px} px (mapped {body_mapped})"
    );
    // GTK 4's expander is not an `ActivatableWidget`: a header press toggles `expanded` inside the widget and
    // emits no `activate` signal, and gtk4-rs has neither a callable `activate()` nor an `activate-detail`, so
    // there is no door to fire from outside — the press IS the property change applied above. What is checked
    // here is that the state landed and stayed, which is what the band's rule reads.
    let open_now = log_is_open(&window);
    settle();
    println!("{ITEM} s1k: log open after the press -> {open_now}");
    assert!(
        open_now,
        "{ITEM} §1: the log is open after its header press"
    );
    assert!(
        open_handle < rest_handle || open_band > rest_band,
        "{ITEM} §1: opening the log changed nothing: handle {rest_handle} -> {open_handle}, band \
         {rest_band:.0} -> {open_band:.0} px (two rows {row_h:.0} + body {opened})"
    );
    // Close it the same way. A `gtk::Paned` only re-allocates its halves on the next allocation, which
    // headless nothing causes, so the harness re-lays the window out — the same double allocation
    // `common::rest_shell_band` does for a freshly built window — before reading what came back.
    expander.set_expanded(false);
    common::wait_until(
        "{ITEM} §1: collapsing the log never handed its room back to the page",
        || !expander.is_expanded() && (paned.position() - rest_handle).abs() <= 20,
    );
    common::size_to_minimum(&window);
    common::size_to_minimum(&window);
    let back_handle = paned.position();
    let back_band = band_px(&window);
    println!("{ITEM} s1k: closed again handle {back_handle}, band {back_band:.0} px");
    assert!(
        !expander.is_expanded(),
        "{ITEM} §1: closing the log again collapses it"
    );
    assert!(
        (back_band - (row_h + header_h)).abs() <= 20.0,
        "{ITEM} §1: back to the collapsed band of just its two rows: {back_band:.0} px vs {:.0} px",
        row_h + header_h
    );
    assert!(
        (back_handle - rest_handle).abs() <= 20,
        "{ITEM} §1: after a close the handle sits where it rested: {rest_handle} -> {back_handle}"
    );
    // A collapsed expander maps nothing: badge 18 has no box at all until it is opened again.
    let body = expander.child().expect("the expander holds the log body");
    assert!(
        !body.is_mapped(),
        "{ITEM} §1: a collapsed log keeps its body off screen"
    );
    window.close();
}

/// §1's collapsed sentence as drawn numbers: the bottom half is EXACTLY the run-bar row plus the log
/// header, it ends where the window does, and the page half is exactly the handle.
fn s1l_collapsed_band_counts_only_its_two_rows(app: &adw::Application) {
    let window = minimum_window(app, "Prepare", false);
    let paned = divider(&window);
    let (row_h, header_h) = two_rows_px(&window);
    let (x, y, w, h) = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), "bottom-bar")
        .expect("the bottom band is drawn");
    println!("{ITEM} s1l: band y={y:.0} h={h:.0} in a {} px window (run {row_h:.0} + header {header_h:.0})", window.height());
    assert!(
        (h - (row_h + header_h)).abs() <= 20.0,
        "{ITEM} §1: the collapsed bottom half is the run-bar row + the log header and nothing else: \
         {h:.0} px vs {:.0} px", row_h + header_h
    );
    assert!(
        (y + h - window.height() as f64).abs() <= 2.0,
        "{ITEM} §1: no empty space under the collapsed log — the band ends at {:.0} in a {} px window",
        y + h, window.height()
    );
    assert!(x >= 0.0 && w > 0.0, "{ITEM} §1: the band is a real box: x={x:.0} w={w:.0}");
    // "the page takes all the rest of the height": the page half is the handle, to the pixel.
    let page_h = paned.start_child().expect("the divider has a page half").height();
    println!("{ITEM} s1l: page half {page_h} px vs handle {}", paned.position());
    assert!(
        (page_h - paned.position()).abs() <= 2,
        "{ITEM} §1: the page half is exactly the divider's position: {page_h} vs {}", paned.position()
    );
    window.close();
}

/// The window round: one `adw::Application`, one `activate`, every widget check inside it.
#[test]
fn sec_03_shell_1_the_window_minimum_the_shell_keeps_its_minimum_and_its_shot() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        s1i_opens_at_the_minimum_and_cannot_go_smaller(app);
        s1j_every_tab_is_shot_as_the_whole_window(app);
        s1k_opening_the_log_through_the_expander_moves_the_divider(app);
        s1l_collapsed_band_counts_only_its_two_rows(app);
        RAN_MINIMUM.store(true, Ordering::SeqCst);
    });
    assert!(RAN_MINIMUM.load(Ordering::SeqCst), "{ITEM}: the widget round never ran");
    assert!(RAN_TABS.load(Ordering::SeqCst), "{ITEM}: the four-tab shot check never ran");
}

/// The divider's arithmetic with no window in sight. `rest_split` forwards `total - band` to
/// `gtk::Paned::set_position`, which PANICS when GTK's own clamp comes out inverted, so the two degenerate
/// asks (a band as tall as the divider, a band of nothing) must be clamped here rather than passed on.
#[test]
fn sec_03_shell_1_the_window_m1_the_band_rule_arithmetic() {
    use naivepost::ui::window_shot::rest_position;
    // The normal case: the handle is the page's height, i.e. everything the band does not take.
    assert_eq!(rest_position(844, 54), 790, "§1: an 844 px divider with a 54 px band gives the page 790");
    assert_eq!(rest_position(844, 0), 844, "no band at all: the page has the whole divider");
    // The two asks a headless window really does produce, both of which would panic inside the paned.
    assert_eq!(rest_position(5, 274), 0, "a band taller than the divider cannot eat below zero");
    assert_eq!(rest_position(0, 0), 0, "an unallocated divider has no split to honour");
    assert_eq!(rest_position(844, 844), 0, "a band the divider's own height leaves the page nothing");
    // And never an answer outside the divider, whichever way the numbers come.
    for (total, band) in [(844, 1), (844, 843), (100, 100), (1, 1), (10, 0)] {
        let handle = rest_position(total, band);
        assert!(
            handle >= 0 && handle <= total,
            "§1: a resting handle must sit inside the divider: {total} - {band} = {handle}"
        );
    }
}
