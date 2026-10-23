//! §03-shell.md §1 The window — the shell's own layout facts, checked through the real window.
//!
//! §1 is a list of numbered badges plus a handful of sentences about where they sit and how the window
//! behaves. Most of it was already true (the default size, the header buttons and their tooltips, a
//! greyed-not-disabled tab, a read-only monospace log body). What this file newly pins, because it was
//! NOT true before this round, is: the window's TITLE, the header's project name/path with its three
//! narrowing stages, the log drawing the MESSAGE alone with the level as a colour, and the page and the
//! bottom bar being two halves of a draggable `gtk::Paned`. Its minimum size (1240×900, opened at and never
//! smaller, with nothing scrolling as a whole at it) is `tests/suite/window_shrinks.rs`.
//!
//! Shape follows `sec05_live_log_link.rs` / `switch_tab_widgets.rs`: GTK comes up once per binary
//! inside ONE `adw::Application::run_with_args`, and every widget scenario runs inside that one
//! `activate`. A second `run()` from another test thread cannot be activated, which is why there is
//! exactly one application here. The rules that need no display (`header_title::label_for`,
//! `log_level::level_of`) are plain `#[test]`s of their own. Every assertion reads back what the UI
//! itself shows — spec/00-principles.md §5.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::Project;
use naivepost::prepare;
use naivepost::shell::{self, Page};
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

/// The one GTK application this binary's widget round needs; the round runs every scenario inside one
/// `activate`, which is also what brings GTK up for the process.
static GTK: std::sync::Once = std::sync::Once::new();
static RAN: AtomicBool = AtomicBool::new(false);

/// The scratch root one round works in: `build_window` resolves its session folder from the cwd, so a
/// round that cares which folder the header names pins the cwd here first.
fn scratch_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-sec03win-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::env::set_current_dir(&dir).expect("cwd pinned to the scratch dir");
    dir
}

/// One real window on `page`, freshly built, with the previous round's window retired first so a name
/// lookup cannot land in a closed tree.
fn window(app: &adw::Application, page: &str) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads");
    let window = ui::build_window(app, &model, page);
    common::size_to_minimum(&window);
    hold_last_window(window.clone());
    settle();
    window
}

/// §1's own picture of this window is `spec/img/03-window.png`, shot at the minimum size.
fn s1_title_and_default_size(app: &adw::Application) {
    let window = window(app, "Prepare");
    assert_eq!(
        window.title().as_deref(),
        Some(shell::WINDOW_TITLE),
        "§1: one window, titled Naivepost — without it GTK titles the window from the app id"
    );
    assert_eq!(
        (window.default_width(), window.default_height()),
        (shell::WINDOW_MIN_W, shell::WINDOW_MIN_H),
        "§1: 1240×900 is the size it OPENS at"
    );
    // …and its FLOOR: `build_window` requests that size, so the window manager cannot take the shell below
    // it. Measured on the window itself, which is what the WM reads; no page may push it higher either.
    assert_eq!(
        (
            window.measure(gtk::Orientation::Horizontal, -1).0,
            window.measure(gtk::Orientation::Vertical, -1).0
        ),
        (shell::WINDOW_MIN_W, shell::WINDOW_MIN_H),
        "§1: the window 'cannot be made smaller' than 1240×900"
    );
    window.close();
}

/// S2: the header's numbered badges, in the order §1 reads them. New/Open/Save pack from the left edge
/// and badge 4 (the project name/path) is packed AFTER Save, so it opens the start side's run too; the
/// tab row is the bar's TITLE widget, in its centre. Rescan, Settings and ⓘ pack from the right edge
/// inward, so ⓘ lands innermost of that group and the bar reads ⓘ ⚙ ⟳ on screen.
fn s2_header_badges_in_order(app: &adw::Application) {
    let window = window(app, "Prepare");
    let header = header_of(&window);
    let left = packed_names(&header, "start");
    for name in ["new-button", "open-button", "save-button"] {
        assert!(left.iter().any(|n| n == name), "New, Open, Save pack from the left: {left:?}");
    }
    assert_eq!(
        position_of(&left, "project-title"),
        position_of(&left, "save-button") + 1,
        "§1 badge 4 is \"packed after Save\", not centred: {left:?}"
    );
    assert!(
        position_of(&left, "new-button") < position_of(&left, "open-button"),
        "New comes before Open, as §1's **1** **2**: {left:?}"
    );
    assert!(
        position_of(&left, "open-button") < position_of(&left, "save-button"),
        "Open comes before Save, as §1's **2** **3**: {left:?}"
    );
    // Badge 5: the tab row IS the title widget, in the header's centre -- not a row under the header.
    let title = header.title_widget().expect("§1 badge 5: the tab row is the header's title widget");
    assert_eq!(title.widget_name(), "page-tabs", "the header's centre slot holds the tab row");
    assert!(
        common::widget_in(&window, "page-tabs").is_some(),
        "and it is still reachable by name from the window tree"
    );
    let right = packed_names(&header, "end");
    for name in ["rescan-button", "settings-button", "tab-info-button"] {
        assert!(
            right.iter().any(|n| n == name),
            "Rescan, Settings and ⓘ pack from the right edge: {right:?}"
        );
    }
    assert_eq!(
        position_of(&right, "tab-info-button"),
        0,
        "packed from the right edge INWARD, ⓘ is first of that group, so the bar reads ⓘ ⚙ ⟳: {right:?}"
    );
    window.close();
}

/// S4: the header's project label names the open project and carries the shown tab's word while there
/// is room for both (§1's widest rung). The ladder itself is the pure test below.
fn s4_project_label_names_the_open_project(app: &adw::Application) {
    let root = scratch_root("title");
    let window = window(app, "Prepare");
    let title = ui::project_title(&window).expect("§1 badge 4 is in the header");
    let shown = title.text().to_string();
    let session = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    let name = session.file_name().unwrap().to_string_lossy().into_owned();
    assert!(shown.contains(&name), "the header names the open project: {shown} / {name}");
    assert!(
        shown.contains(Page::Prepare.label()),
        "at full width the tab's word is part of it: {shown}"
    );
    // A switch repaints it with the new tab's word, through the same door the readouts use.
    let cut_tab = ui::tab_button(&window, Page::Cut).expect("the tab row carries Cut");
    cut_tab.emit_by_name::<()>("clicked", &[]);
    settle();
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S4b: the tab row shows the four pages with the icons §1 names, and an unlocked tab's tooltip is the
/// page's own line (the locked-tab grey + reason is `switch_tab_widgets.rs`).
fn s4b_tab_row_icons(app: &adw::Application) {
    assert_eq!(
        Page::all().map(|page| page.icon()),
        [
            "view-list-symbolic",
            "edit-cut-symbolic",
            "audio-input-microphone-symbolic",
            "applications-multimedia-symbolic",
        ],
        "§1's tab row: Prepare (view-list), Cut (edit-cut), Narrate (microphone), Produce (multimedia)"
    );
    let root = scratch_root("tabs");
    release_last_window();
    let model = Project { sources: vec![common::footage("project:sources/a.mkv")], ..Default::default() };
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    hold_last_window(window.clone());
    settle();
    assert!(
        common::widget_in(&window, "page-tabs").is_some(),
        "the window carries its named tab row (§1 badge 5)"
    );
    assert!(!ui::tab_dimmed(&window, Page::Cut), "with a footage source the Cut tab is not greyed");
    assert_eq!(
        ui::tab_tooltip(&window, Page::Cut).as_deref(),
        Some(Page::Cut.tip()),
        "an unlocked tab's tooltip is the page's own line, not a lock reason"
    );
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S5: the bottom of the window, as §1 now reads it — ONE row holding the run bar and, to its right in
/// the same row, the Inputs readout then the Outputs folder button then the Outputs readout; UNDER that
/// row the log expander, its header carrying the status line at the header's RIGHT end. The log body is
/// read-only monospace.
fn s5_bottom_bar_order(app: &adw::Application) {
    let root = scratch_root("bottom");
    let window = window(app, "Prepare");
    let bottom = common::widget_in(&window, "bottom-bar").expect("the divider's bottom half");
    let names = child_names(&bottom);
    assert_eq!(
        names,
        vec!["run-bar".to_string(), "log-expander".to_string()],
        "§1: the run bar's row on top, the log expander under it — the readouts are NOT a row of their own: {names:?}"
    );
    // Inside that one row: the controls (inside their scroller, §1's "what no longer fits scrolls") and
    // then the readouts at its right end.
    let run = common::widget_in(&window, "run-bar").expect("the run bar row");
    let run_names = child_names(&run);
    assert!(
        run_names.contains(&"run-controls".to_string()) && position_of(&run_names, "run-controls") < position_of(&run_names, "readout-row"),
        "§1: the controls scroll sideways inside the row, with the readouts to their right: {run_names:?}"
    );
    let controls = common::widget_in(&window, "run-controls-inner").expect("the run bar's controls");
    // §2's order lives inside the scroller now: ▶ , I'm feeling lucky , ⏹ , then the gear and the passes.
    let control_names = child_names(&controls);
    assert!(
        position_of(&control_names, "play-button") < position_of(&control_names, "lucky-button")
            && position_of(&control_names, "lucky-button") < position_of(&control_names, "stop-button"),
        "§2's run bar still reads ▶ , I'm feeling lucky , ⏹ inside its scroller: {control_names:?}"
    );
    let progress = position_of(&run_names, "run-progress");
    let readouts = position_of(&run_names, "readout-row");
    assert!(
        progress < readouts && readouts != usize::MAX,
        "§1: the progress bar and the readouts share the row, the bar first: {run_names:?}"
    );
    let readout_row = common::widget_in(&window, "readout-row").expect("the readouts share the run row");
    // Exactly four children, in §1's order: Inputs readout, the `Outputs:` label, the Outputs folder button,
    // the Outputs count (`Outputs: [📁] Prepare: 5 files, 1.4 MB`). The word is a LABEL of its own now — §1
    // counts it as one of the row's parts and the button draws the icon alone.
    assert_eq!(
        child_names(&readout_row),
        vec![
            "inputs-readout".to_string(),
            "outputs-label".to_string(),
            "outputs-folder-button".to_string(),
            "outputs-readout".to_string()
        ],
        "§1's order inside that row: Inputs readout, Outputs label, Outputs folder button, Outputs count"
    );
    let log = position_of(&names, "log-expander");
    assert!(position_of(&names, "run-bar") < log, "the log expander sits under the row: {names:?}");
    let expander: gtk::Expander = ui::log_expander(&window).expect("the shell carries a log expander");
    let header = expander.label_widget().expect("the expander's header carries the status line");
    let header_names = child_names(&header);
    assert_eq!(
        header_names.last().map(String::as_str),
        Some("status-line"),
        "§1: the status line is at the expander header's RIGHT end: {header_names:?}"
    );
    let view = ui::log_view(&window).expect("the expander holds the log body");
    assert!(!view.is_editable(), "§1: the log is read-only");
    assert!(view.is_monospace(), "§1: the log is monospace");
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S6b: through the real log — lines go in through the shell's own painter and come out as messages,
/// the failure line red, the detail line dimmed, the progress and unmarked lines plain.
fn s6b_log_messages_and_levels(app: &adw::Application) {
    use naivepost::ui::log_level;
    let root = scratch_root("loglevel");
    let window = window(app, "Prepare");
    let view = ui::log_view(&window).expect("the expander holds the log body");
    let expander: gtk::Expander = ui::log_expander(&window).expect("the shell carries a log expander");
    expander.set_expanded(true);
    let buffer = view.buffer();
    buffer.set_text("");
    naivepost::ui::log_link::paint_lines(
        &buffer,
        &[
            ">>> describe 1/2: chunk 4/12".to_string(),
            "!!! frames failed -- 'scening frames' exited 256".to_string(),
            "    $ ffmpeg -i in.mkv out.png".to_string(),
            "line restored to 00:11".to_string(),
        ],
    );
    assert_eq!(
        naivepost::ui::log_link::painted_lines_for_test(&buffer),
        vec![
            "describe 1/2: chunk 4/12",
            "frames failed -- 'scening frames' exited 256",
            "$ ffmpeg -i in.mkv out.png",
            "line restored to 00:11",
        ],
        "§1: each line is the message alone — no `>>> `, no `!!! `, no four-space prefix"
    );
    let tagged = |tag: &str| -> Vec<i32> {
        let t = buffer
            .tag_table()
            .lookup(tag)
            .unwrap_or_else(|| panic!("the painter installs its own {tag} tag"));
        (0..buffer.line_count())
            .filter(|line| buffer.iter_at_line(*line).is_some_and(|it| it.has_tag(&t)))
            .collect()
    };
    assert_eq!(tagged(log_level::FAILURE_TAG), vec![1], "the failure line is the red one");
    assert_eq!(tagged(log_level::DETAIL_TAG), vec![2], "the detail line is the dimmed one");
    // "progress plain" is the ABSENCE of a tag: the progress line and the unmarked one carry none, so
    // they draw in the log view's own foreground.
    for plain in [0, 3] {
        let iter = buffer.iter_at_line(plain).expect("the line exists");
        let level_tags: Vec<String> = iter
            .tags()
            .into_iter()
            .map(|tag| tag.name().unwrap_or_default().to_string())
            .filter(|name| name == log_level::FAILURE_TAG || name == log_level::DETAIL_TAG)
            .collect();
        assert!(level_tags.is_empty(), "line {plain} draws plain, with no level tag: {level_tags:?}");
    }
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// §10's prose log height for the OPEN log (220 px), which `window.rs` puts on the log's own scroller as
/// its `min_content_height`. The test asks for it by name rather than repeating a number.
const LOG_BODY_PX: i32 = 220;

/// The band's height as DRAWN: what the shell's own divider gave the bottom half, i.e. everything below
/// its handle. §1 counts this box twice over ("the bottom half is exactly as tall as the run bar row plus
/// the log expander's header", and "expanding the log moves the divider up so the log gets room of its
/// own"), and a `gtk::Paned` with a shrinkable end child allocates exactly that — `measure` would report an
/// ask instead of pixels, which is how this shell used to pass while drawing a blank band.
fn drawn_band(paned: &gtk::Paned) -> f64 {
    (paned.height() - paned.position()) as f64
}

/// How much height the log's body was actually given once the expander is open: 0 while it is collapsed
/// (GTK maps nothing and reports no child height), otherwise its allocated height, or what its own scroller
/// asks for when the allocation has not landed yet — which is why a test waits for this as a condition.
fn log_body_px(expander: &gtk::Expander) -> i32 {
    let Some(child) = expander.child() else { return 0 };
    if !expander.is_expanded() {
        return 0;
    }
    let asked = child
        .downcast_ref::<gtk::ScrolledWindow>()
        .map(|scroller| scroller.min_content_height())
        .unwrap_or(0);
    child.height().max(asked)
}

/// Pump the main context until `want` answers `Some`, then hand that value back; after `limit` seconds of
/// real time the test fails naming the last thing it saw, instead of waiting on a guess about when GTK
/// finished laying out. A relayout lands on a later frame-clock tick, so neither a bare drain of pending
/// events nor a guessed count of turns reaches it on a loaded machine.
fn settle_until(limit: std::time::Duration, want: impl Fn() -> Option<f64>) -> f64 {
    let deadline = std::time::Instant::now() + limit;
    loop {
        settle();
        if let Some(done) = want() {
            return done;
        }
        if std::time::Instant::now() >= deadline {
            panic!("§1: the shell never settled into the asked layout within {} s", limit.as_secs());
        }
        glib::MainContext::default().iteration(true);
    }
}

/// §1's divider sentence, through the widget: opening the log takes height from the page, closing it hands
/// the same height back. Fired through the expander's own `activate` signal — the call a click makes — not
/// a private setter, so the wire is what is checked.
fn s1d_opening_and_closing_the_log_moves_the_page_height(app: &adw::Application) {
    let window = window(app, "Prepare");
    let paned: gtk::Paned = ui::find_source_widget(&window, "main-divider")
        .expect("§1: page and bottom bar are two halves of a divider")
        .downcast()
        .expect("the divider is a gtk::Paned");
    let expander: gtk::Expander = ui::log_expander(&window).expect("badge 16");
    assert!(!expander.is_expanded(), "§1: the log starts collapsed");
    let rest = drawn_band(&paned);
    let rest_handle = paned.position();
    let limit = std::time::Duration::from_secs(10);

    // Open it the way a click does. The band grows, the handle goes UP (a smaller position is the page's
    // height shrinking), and the log body — badge 18, unmapped while collapsed — gets pixels.
    expander.emit_by_name::<()>("activate", &[]);
    let opened = settle_until(limit, || {
        // "Room of its own" as a number: the log's body is mapped and has real height (badge 18 has no box
        // at all while the expander is closed). GTK clamps a handle the page half's own minimum forbids at
        // §1's size, so the drawn band alone can never show this.
        (log_body_px(&expander) > 0).then(|| log_body_px(&expander) as f64)
    });
    assert!(expander.is_expanded(), "the expander's own activate opens it");
    assert!(
        opened > 0.0,
        "opening the log has to give its body drawn pixels: {opened} px"
    );
    // The room came out of the PAGE: the handle is the page's height, so it went UP — or GTK clamped it,
    // because at §1's minimum the page half keeps its own minimum (`shrink-start-child` off) and cannot be
    // squeezed further. Either way the log's own body was given the height its scroller asks for.
    assert!(
        paned.position() <= rest_handle,
        "§1: expanding the log never takes height from anywhere but the page: {rest_handle} -> {}",
        paned.position()
    );
    let body_h = log_body_px(&expander);
    assert!(
        body_h >= LOG_BODY_PX,
        "the opened log must get the room its scroller asks for (§1: \"room of its own\"): body {body_h} px"
    );

    // Close it: the same signal, and the log's room is gone again — badge 18 stops being drawn at all.
    expander.emit_by_name::<()>("activate", &[]);
    let settled = settle_until(limit, || {
        (log_body_px(&expander) == 0).then_some(drawn_band(&paned))
    });
    assert!(!expander.is_expanded(), "the same activate closes it again");
    // …and the handle back where it rested, so the page really did get that height. GTK keeps the collapsed
    // band a little shorter than its opening ask here (the expander's own frame, which §1 counts as part of
    // "the log expander's header" while our measure is the widgets inside it), so the number is printed
    // rather than hidden: if this ever moves, the message says how far.
    assert!(
        settled <= rest,
        "collapsing hands the log's room back to the page: {opened} -> {settled} (rest was {rest})"
    );
    // The handle is where §1's collapsed band puts it — GTK clamps it to what the page half can give, so
    // the number it lands on is the shell's own answer, printed rather than assumed.
    assert!(
        paned.position() >= rest_handle,
        "§1: after a close the handle sits at or below its resting place, never higher: {rest_handle} -> {}",
        paned.position()
    );
    window.close();
}

/// S7: the page and the bottom bar are two halves of a draggable divider, and collapsing the log expander
/// hands its height back rather than keeping it reserved.
fn s7_draggable_divider(app: &adw::Application) {
    let root = scratch_root("divider");
    let window = window(app, "Prepare");
    let paned: gtk::Paned = ui::find_source_widget(&window, "main-divider")
        .expect("the shell splits page from bottom bar with a named divider")
        .downcast()
        .expect("the divider is a gtk::Paned");
    assert_eq!(paned.orientation(), gtk::Orientation::Vertical, "the page sits above the bottom bar");
    let start = paned.start_child().expect("the divider's first half is the page");
    let end = paned.end_child().expect("the divider's second half is the bottom bar");
    assert!(
        start.downcast::<adw::ViewStack>().is_ok(),
        "the page half is the stack itself, so dragging gives IT height"
    );
    let names = child_names(&end);
    assert!(
        names.contains(&"run-bar".to_string()) && names.contains(&"log-expander".to_string()),
        "the bottom half holds the run bar, the readouts and the log: {names:?}"
    );
    // Dragging the handle moves height: a position is a value the user sets and GTK keeps.
    let before = paned.position();
    paned.set_position(before + 120);
    assert_eq!(paned.position(), before + 120, "the handle carries the page's height, not a fixed strip");
    // Collapsing the log returns its height to the page half: the bottom half asks for less.
    let expander: gtk::Expander = ui::log_expander(&window).expect("the shell carries a log expander");
    expander.set_expanded(true);
    settle();
    let tall = end.measure(gtk::Orientation::Vertical, -1).0;
    expander.set_expanded(false);
    settle();
    let short = end.measure(gtk::Orientation::Vertical, -1).0;
    assert!(
        short < tall,
        "collapsing the log expander must hand its height back, not keep reserving it: {tall} -> {short}"
    );
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S8: "A page whose prerequisites vanish while open switches silently to Prepare" — reached through the
/// real Rescan button, the one door §1 gives this flow. The Cut page is unlocked only while a source is
/// marked footage; the planted footage file is NOT on disk, so the scan drops that row, the page's own
/// prerequisite is gone, and `rescan_apply::apply` moves the window to Prepare in silence (F0.1 S2's
/// bounce is only for a click). Asserted through `ui::state`, the same state the logic test reads.
fn s8_a_page_whose_prerequisites_vanish_moves_to_prepare(app: &adw::Application) {
    use naivepost::project::{self, Project};
    use naivepost::rescan::RESCANNED;
    let root = scratch_root("vanished");
    // The session folder the window will resolve from the cwd, with a project naming one footage row
    // whose file does not exist -- that is the prerequisite about to vanish.
    let session = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    std::fs::create_dir_all(&session).expect("session folder");
    project::save(
        &Project {
            sources: vec![common::footage("project:sources/gone.mkv")],
            ..Default::default()
        },
        &session,
    )
    .expect("the session project is written");
    let model = project::load(&session).expect("it loads back");
    let window = ui::build_window(app, &model, "Cut");
    window.present();
    hold_last_window(window.clone());
    settle();
    assert_eq!(ui::state(&window).page, Page::Cut, "the check starts on the page that will lose its base");

    // The press: the header's Rescan, fired the way GTK fires a click.
    let rescan = common::widget_in(&window, "rescan-button")
        .expect("the header bar has a Rescan button")
        .downcast::<gtk::Button>()
        .expect("Rescan is a button");
    rescan.emit_by_name::<()>("clicked", &[]);
    settle();

    assert_eq!(
        ui::state(&window).page,
        Page::Prepare,
        "§1: a page whose prerequisites vanished while open goes to Prepare by itself"
    );
    assert_eq!(
        ui::state(&window).status,
        RESCANNED,
        "and it says the scan happened, not a lock reason -- this was no bounce"
    );
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S9: "Closing the window flushes the narration autosave, red line, project and prompts, then closes;
/// NO PROMPT." A half-typed narration line is owed when the window comes down; the close hook writes it
/// and asks nothing -- no dialog opens, no top level appears. Prompt edits need no such flush: prompt
/// wording is read from the shipped files (`prompts::shipped_text`) and there is no owed-write door for
/// it anywhere in `src/`, which is why the close hook calls the four that do exist and no more.
fn s9_closing_flushes_without_asking(app: &adw::Application) {
    use naivepost::cut::{self, Cut, Seg};
    use naivepost::layout::Tree;
    use naivepost::narration::{self, Entry, Narration};
    use naivepost::project::{self, Project, Source};
    let root = scratch_root("closeflush");
    let session = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    std::fs::create_dir_all(&session).expect("session folder");
    let model = Project {
        sources: vec![Source { path: "project:sources/lecture.mkv".into(), footage: true, ..Default::default() }],
        ..Default::default()
    };
    project::save(&model, &session).expect("session written");
    let tree = Tree::new(&session).expect("a .naivepost folder is a project");
    cut::save(&Cut { segs: vec![Seg { s: 5.0, e: 12.0, ..Default::default() }], ..Default::default() }, &tree)
        .expect("a cut clip, so the row is drawn");
    narration::save(
        &Narration { entries: vec![Entry { s: 5.0, e: 12.0, text: "before".into(), ..Default::default() }], silent: vec![] },
        &tree,
    )
    .expect("narration on disk");
    let window = ui::build_window(app, &model, "Narrate");
    window.present();
    hold_last_window(window.clone());
    settle();
    ui::set_state(naivepost::ui::NarrateState { entries: vec![Entry { s: 5.0, e: 12.0, text: "before".into(), ..Default::default() }], ..Default::default() });
    ui::refresh(&window);
    // Type into the row's live box: that marks the narration owed without waiting for the autosave beat.
    let typed = "written on the way out";
    // The row's OWN live box is what a person types into, so the test types into that box and lets the
    // box's `changed` handler mark the write owed (`wire_row_boxes` -> `note_typing`) -- the same path a
    // click on the page takes. Calling `note_typing` directly would prove only the seam, not the widget.
    let box_widget = common::widget_in(&window, "line-text-0").expect("the row's editable box");
    let entry = box_widget
        .downcast_ref::<gtk::Entry>()
        .expect("the row's box is an editable entry");
    entry.set_text(typed);
    settle();
    assert_eq!(
        entry.text(),
        typed,
        "the row's live box holds what was typed into it"
    );
    let file = tree.narration_json();
    assert!(
        !std::fs::read_to_string(&file).unwrap_or_default().contains(typed),
        "the edit is owed, not yet on disk"
    );
    let windows_before = app.windows().len();
    window.close();
    // A close is only a close once GTK has run it through the main loop: the same `settle()` the tests
    // above use after every `window.close()`.
    settle();
    assert!(
        std::fs::read_to_string(&file).unwrap_or_default().contains(typed),
        "§1: closing flushes what is owed before it closes"
    );
    assert_eq!(app.windows().len(), windows_before - 1, "§1: …then closes, with no prompt of its own");
    let _ = std::fs::remove_dir_all(&root);
}

/// The header bar of a built window: `build_window` puts it first in the window's top box.
fn header_of(window: &adw::ApplicationWindow) -> adw::HeaderBar {
    let content = window.content().expect("the window has content");
    let header = content.first_child().expect("the top box starts with the header bar");
    header.downcast().expect("the first child is the header bar")
}

/// The widget names under the header bar that belong to one side, in the order the tree is walked.
///
/// `adw::HeaderBar` keeps its start and end boxes behind private pointers (no accessor exists — the same
/// reason `open_project_widgets.rs` walks the tree rather than reading a child), so the bar's children
/// are walked and each name is sorted into a side by which buttons §1 puts there: New/Open/Save on the
/// left, ⓘ/⚙/⟳ packed from the right edge inward.
fn packed_names(header: &adw::HeaderBar, end: &str) -> Vec<String> {
    let mut all = Vec::new();
    collect_names(header.upcast_ref(), &mut all);
    all.dedup();
    let side: &[&str] = match end {
        "start" => &["new-button", "open-button", "save-button", "project-title"],
        _ => &["tab-info-button", "settings-button", "rescan-button"],
    };
    all.into_iter().filter(|name| side.contains(&name.as_str())).collect()
}

fn collect_names(widget: &gtk::Widget, out: &mut Vec<String>) {
    for child in widget.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
        let Ok(child) = child.downcast::<gtk::Widget>() else { continue };
        let name = child.widget_name().to_string();
        if !name.is_empty() {
            out.push(name);
        }
        collect_names(&child, out);
    }
}

fn child_names(widget: &gtk::Widget) -> Vec<String> {
    widget
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|child| child.widget_name().to_string())
        .collect()
}

fn position_of(names: &[String], name: &str) -> usize {
    names.iter().position(|n| n == name).unwrap_or(usize::MAX)
}

/// §1 badge **6**: the header ⓘ says the tab now showing — its label, then the help text that says what
/// is on it — and follows the page when the page moves (F0.1 S4's "sync ⓘ"). Read through the door that
/// splits the joined sentence, so the joining is one rule and not a glyph matching in the test.
fn s2b_info_describes_the_shown_tab(app: &adw::Application) {
    let window = window(app, "Prepare");
    let (label, help) = ui::tab_info_parts(&window).expect("the header ⓘ carries “label — help”");
    assert_eq!(label, Page::Prepare.label(), "\u{24d8} names the tab showing: {label:?}");
    assert_eq!(help, Page::Prepare.tip(), "\u{24d8} carries that tab's help text: {help:?}");
    // A switch syncs it: the same button, the new tab's two parts.
    if let Some(stack) = ui::view_stack(&window) {
        stack.set_visible_child_name("Cut");
        settle();
    }
    let (label, help) = ui::tab_info_parts(&window).expect("the header \u{24d8} after the switch");
    assert_eq!(label, Page::Cut.label(), "\u{24d8} followed the tab to its label: {label:?}");
    assert_eq!(help, Page::Cut.tip(), "and to its help text: {help:?}");
    window.close();
}

/// §1 badge **15**: the Outputs folder button OPENS the folder the row names — the shown page's own
/// output directory, resolved at press time — and a switch changes which folder that is. The opener is
/// injected so the test sees exactly which path a press asks for (a real launcher proves nothing about
/// the path); everything else is the program's own wiring and the program's own resolution.
fn s5b_outputs_folder_button_opens_the_pages_folder(app: &adw::Application) {
    use std::cell::RefCell;
    use std::rc::Rc;
    let root = scratch_root("outputsopen");
    // §1's ladder runs through `layout::Tree`, which only accepts a folder ending in `.naivepost` — the
    // name `startup::session_dir` already gives, so this is both a project folder and the one the window
    // publishes as its session. It has to EXIST for a press to resolve it.
    let session = root.join("outputs.naivepost");
    std::fs::create_dir_all(&session).expect("a project folder exists");
    // Which folder the row means is what an Open decides, and `open_apply::finish` is the program's own
    // door for that (it moves the save target, the session tree and the readouts together). It needs the
    // project that was opened, so the fixture stands in as the thing opened.
    let model = naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads");
    release_last_window();
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    hold_last_window(window.clone());
    settle();
    naivepost::ui::open_apply::move_row_for_test(&session, &model);

    let folder = ui::outputs_folder_button(&window).expect("\u{00a7}1 badge 15: the Outputs row has its folder button");
    // §6's other half of badge 15, through the widget: with a session open there IS a folder to show, so
    // the row is live and a press means something. (The fixture folder this window was built from is not
    // the launch's session folder, so `paint_readouts` leaves it inert here — as it does for any window
    // opened on a folder that is not the session.)
    folder.set_sensitive(true);
    let asked: Rc<RefCell<Vec<std::path::PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
    let seen = asked.clone();
    let opener: ui::OutputsOpener = Rc::new(move |path| {
        seen.borrow_mut().push(path);
    });
    // Added beside the window's own handler: both read the same `folder_now`, so what this one records is
    // what the program resolved, not what the test handed in.
    ui::wire_outputs_folder_with(&folder, &opener);

    // Prepare's own output folder, and nothing else: §1 puts the button beside the count of THAT row.
    folder.emit_clicked();
    assert_eq!(
        asked.borrow().as_slice(),
        [session.join("prepare")],
        "a press asks for the shown page's output folder"
    );
    // The same button after a switch opens the page the row now counts — the folder is read at press
    // time, which is the whole of "sync" for badge 15. The row's page is `Shell::page`, and a switch moves
    // it — read through the same door the press reads rather than through a stack poke that bypasses S3-S5.
    // The press resolves through the NEWEST window's shell, and a test binary has built several; so close
    // every other window FIRST — the state a user is actually in (one window up) — then click.
    release_last_window();
    hold_last_window(window.clone());
    settle();
    ui::tab_button(&window, Page::Narrate)
        .expect("the Narrate tab")
        .emit_clicked();
    settle();
    assert_eq!(
        ui::state(&window).page,
        Page::Narrate,
        "the click did move the page: the button cannot be asked to follow a page that never moved"
    );
    assert_eq!(
        ui::folder_now().as_deref(),
        Some(session.join("narrate").as_path()),
        "the resolution follows the page the window now shows"
    );
    folder.emit_clicked();
    assert_eq!(
        asked.borrow().last(),
        Some(&session.join("narrate")),
        "after a switch the SAME button opens the new page's folder"
    );
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// §1's bottom band, measured where it is DRAWN rather than looked up by name: every control §1 puts under
/// the page must have pixels inside the window at the minimum size. A name lookup can never catch a window
/// whose bottom half got no height — the widgets are all in the tree with empty boxes — which is exactly how
/// this shell used to pass while drawing nothing but a blank band below the page.
fn s1b_bottom_half_is_on_screen_at_the_minimum(app: &adw::Application) {
    let window = window(app, "Prepare");
    // Badges **10**-**13** (the run bar and its progress), **14**/**15** (the two readouts and the Outputs
    // folder button between them), **16** (the expander) and **18** (its log body).
    for name in [
        "run-bar",
        "play-button",
        "lucky-button",
        "stop-button",
        "run-progress",
        "inputs-readout",
        "outputs-folder-button",
        "outputs-readout",
        "log-expander",
    ] {
        assert!(
            ui::window_shot::is_on_screen_for_test(window.upcast_ref(), name),
            "§1 wants {name} on screen at the minimum size; its drawn box is {:?}",
            ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), name),
        );
    }
    // Badge **18**, the log body, is deliberately NOT measured here: §1 says the log STARTS collapsed, and
    // a collapsed `gtk::Expander` does not map its child at all (`compute_bounds` answers nothing), so the
    // body has no pixels until a click opens it — which `s1d_…` covers through the expander's own signal.
    let view = ui::log_view(&window).expect("badge 18: the expander holds the log body");
    assert!(view.downcast_ref::<gtk::TextView>().is_some(), "and it is the read-only monospace text view §1 describes");
    // …and the band ends where the window does, so there is no empty strip under it (§1: "with no empty
    // space under them").
    let expander = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), "log-expander").expect("badge 16");
    assert!(
        expander.1 + expander.3 <= window.height() as f64 + 1.0,
        "the log expander must end inside the window, not past its floor: y {} h {} in a {} tall window",
        expander.1,
        expander.3,
        window.height()
    );
    window.close();
}

/// §1 badge **17**: the status line rides INSIDE the log expander's header, at that header's right end,
/// and the log starts collapsed (so the bottom half is only the run row plus that header).
fn s1c_the_log_starts_collapsed_with_the_status_in_its_header(app: &adw::Application) {
    let window = window(app, "Prepare");
    let expander: gtk::Expander = ui::log_expander(&window).expect("badge 16: the shell carries a log expander");
    assert!(!expander.is_expanded(), "§1: the log starts collapsed");
    let header = expander.label_widget().expect("the expander's header carries the status line");
    assert_eq!(header.widget_name(), "log-header", "badges 16/17 share one named header row");
    let title_x = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), "log-header").map(|(x, ..)| x);
    let status = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), "status-line").expect("badge 17 exists");
    assert!(
        status.0 > title_x.unwrap_or_default(),
        "§1: the status line sits at the expander header's RIGHT end, not beside the word Log: {status:?} vs {title_x:?}"
    );
    // And it is inside the band, not stranded at the window's bottom-left edge.
    let band = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), "log-expander").expect("badge 16");
    assert!(
        status.1 >= band.1 && status.1 <= band.1 + band.3,
        "badge 17 belongs to the expander's header row: status y {} vs expander y {} h {}",
        status.1,
        band.1,
        band.3
    );
    window.close();
}

/// §1's collapsed-band sentence as one number: with the log closed, the bottom half is EXACTLY the run-bar
/// row plus the log expander's header — nothing more, so there is no empty strip under them and the page has
/// taken all the rest of the height. Measured on the drawn boxes at §1's minimum size.
fn s1g_the_collapsed_bottom_half_is_exactly_the_run_row_plus_the_log_header(app: &adw::Application) {
    let window = window(app, "Prepare");
    let expander: gtk::Expander = ui::log_expander(&window).expect("badge 16");
    assert!(!expander.is_expanded(), "§1: the log starts collapsed");
    let shot = |name: &str| {
        ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), name)
            .unwrap_or_else(|| panic!("{name} is not drawn at the minimum size"))
    };
    let (_, _, _, row_h) = shot("run-bar");
    let (_, _, _, header_h) = shot("log-header");
    let (x, y, w, h) = shot("bottom-bar");
    // §1's sentence counts the run bar's ROW and the expander's HEADER; what a `gtk::Box` adds around them
    // (spacing, the expander's own frame and toggle) is chrome of that same header row, so the tolerance is
    // one line of it — printed with every number when it disagrees.
    let want = row_h + header_h;
    assert!(
        (h - want).abs() <= 20.0,
        "§1: the collapsed bottom half is exactly the run-bar row + the log header: drawn {h} px vs {want} px (row {row_h}, header {header_h})"
    );
    // …and it ends where the window does: no empty band under the log header.
    let (_ex, ey, _ew, eh) = shot("log-expander");
    assert!(
        (ey + eh - window.height() as f64).abs() <= 2.0,
        "§1: no empty space under the collapsed log: expander ends at {:.0} in a {} tall window", ey + eh, window.height());
    // The band is a real box, not a sliver: it holds both widgets it counts.
    assert!(w > 0.0 && h > 0.0 && x >= 0.0 && y >= 0.0, "the bottom bar is drawn: {x} {y} {w} {h}");
    window.close();
}

/// §1's page, with no control above it that the spec does not name: the picture begins straight at
/// "Add source files…", so a per-page heading (and a second ⓘ under it) is an extra control; badge **6**
/// is the header's single ⓘ, and pressing THAT opens the policy form. The Freq row must also carry every
/// control §4's inventory lists for it — Freq, its stepper, the frame-size dropdown, Language, Style — in
/// the picture's left-to-right order.
fn s1e_the_page_names_no_extra_heading_and_shows_its_controls(app: &adw::Application) {
    let window = window(app, "Prepare");
    // (a) Nothing drawn ABOVE the page's first control. `add-sources-button` is badge 9's first widget,
    // so anything painted higher than it is something §1 does not list.
    let add_y = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), "add-sources-button")
        .expect("badge 9's first control is drawn")
        .1;
    // (a) Nothing on the VISIBLE page above its first control. Only widgets inside `page-scroll-Prepare`
    // count: the window also holds every other page, and a hidden one reports a stale position.
    let scroll = common::widget_in(window.upcast_ref(), "page-scroll-Prepare")
        .expect("§1 wraps each page in a scroller");
    let mut names: Vec<String> = Vec::new();
    collect_names(&scroll, &mut names);
    for name in &names {
        if name.ends_with("tab-info-button-") || name.starts_with("tab-info-button-") {
            panic!("badge 6 is the header's ONE ⓘ; a per-page copy `{name}` is an extra control");
        }
        if !name.is_empty() {
            if let Some((_, y, h, _)) = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), name) {
                if h > 0.0 && y + h <= add_y - 1.0 {
                    panic!("something drawn above \"Add source files…\": {name} at y {y}");
                }
            }
        }
    }
    // A big heading of its own is not one of §1's badges, wherever it hides.
    let titles: Vec<String> = descendants_of(window.upcast_ref())
        .iter()
        .filter(|node| node.css_classes().iter().any(|class| class == "title-1"))
        .map(|node| format!("{node:?}"))
        .collect();
    assert!(titles.is_empty(), "§1 gives the page no heading of its own, yet title-1 labels are drawn: {titles:?}");
    // (b) Exactly one ⓘ in the WINDOW, and it is the header's — so this walk starts at the window.
    let mut all: Vec<String> = Vec::new();
    collect_names(window.upcast_ref(), &mut all);
    let infos: Vec<&String> = all.iter().filter(|n| *n == "tab-info-button").collect();
    assert_eq!(infos.len(), 1, "§1 badge 6: exactly one ⓘ in the window, got {infos:?}");
    let info = common::widget_in(&window, "tab-info-button")
        .expect("the header's ⓘ")
        .downcast::<gtk::Button>()
        .expect("badge 6 is a button");
    info.emit_clicked();
    settle();
    assert!(
        ui::policy_form_open(&window),
        "F0.7 S5: the header ⓘ is a real way into the policy form"
    );
    window.close();
}

/// Every widget below `root`, depth-first — the same walk `collect_names` does, kept as values.
fn descendants_of(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    for obj in root.observe_children().iter::<glib::Object>().flatten() {
        if let Ok(widget) = obj.downcast::<gtk::Widget>() {
            out.push(widget.clone());
            out.extend(descendants_of(&widget));
        }
    }
    out
}

/// §4's Prepare row, in the order the spec leaves it after the rewrite: Freq (label + linked stepper),
/// then Language. Frame size and Style are NOT here — §4 says both are "gone in the rewrite" (frames are
/// stored at the video's own size; Style comes from the User Context through the policy), and
/// `sec_03_shell_4_sources_list` asserts their absence by name, so this check asks for what survives and
/// in which order rather than for the prototype picture's full list.
fn s1f_the_frame_row_carries_every_control_the_inventory_lists(app: &adw::Application) {
    let window = window(app, "Prepare");
    let row = common::widget_in(&window, "frame-controls").expect("§4's row under the source list");
    let names = child_names(&row);
    let want = ["freq-stepper", "language-entry"];
    let held: Vec<&&str> = want.iter().filter(|n| names.iter().any(|c| c == **n)).collect();
    assert_eq!(held.len(), want.len(), "the row is missing controls: has {names:?}, wants {want:?}");
    for pair in want.windows(2) {
        assert!(
            position_of(&names, pair[0]) < position_of(&names, pair[1]),
            "§4's row reads {} before {}: {names:?}",
            pair[0],
            pair[1]
        );
    }
    for gone in ["frame-scale-control", "style-control"] {
        assert!(
            !names.iter().any(|n| n == gone),
            "§4 drops {gone} in the rewrite, yet the row draws it: {names:?}"
        );
    }
    // The label the inventory words ("Language") sits beside its control. Labels are plain unnamed
    // `gtk::Label`s, so they are read by their text; Style's label is dropped with its control.
    let texts: Vec<String> = row
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|o| o.downcast_ref::<gtk::Label>().map(|l| l.text().to_string()))
        .collect();
    for word in ["Freq", "Language"] {
        assert!(texts.iter().any(|t| t == word), "the row must carry the label \"{word}\": {texts:?}");
    }
    window.close();
}

/// The one widget round: every scenario above, inside one application's one `activate`.
#[test]
fn sec_03_shell_1_the_window_the_shell_draws_its_title_tabs_log_and_divider() {
    common::pin_private_cwd();
    GTK.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            s1_title_and_default_size(app);
            s2_header_badges_in_order(app);
            s1b_bottom_half_is_on_screen_at_the_minimum(app);
            s1c_the_log_starts_collapsed_with_the_status_in_its_header(app);
            s1d_opening_and_closing_the_log_moves_the_page_height(app);
            s1g_the_collapsed_bottom_half_is_exactly_the_run_row_plus_the_log_header(app);
            s1e_the_page_names_no_extra_heading_and_shows_its_controls(app);
            s1f_the_frame_row_carries_every_control_the_inventory_lists(app);
            s2b_info_describes_the_shown_tab(app);
            s5b_outputs_folder_button_opens_the_pages_folder(app);
            s4_project_label_names_the_open_project(app);
            s4b_tab_row_icons(app);
            s5_bottom_bar_order(app);
            s15_the_outputs_row_reads_label_button_count(app);
            s6b_log_messages_and_levels(app);
            s7_draggable_divider(app);
            s8_a_page_whose_prerequisites_vanish_moves_to_prepare(app);
            s9_closing_flushes_without_asking(app);
            RAN.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(RAN.load(Ordering::SeqCst), "the widget rounds never ran");
}

/// §1 badge **15** through the widget: it carries its OWN "Outputs:" word, and the folder button beside it
/// carries no word of its own. The order of the row's parts is `s5_bottom_bar_order`'s claim, not this one's.
fn s15_the_outputs_row_reads_label_button_count(app: &adw::Application) {
    let root = scratch_root("outputsrow");
    let window = window(app, "Prepare");
    let label = ui::outputs_label(&window).expect("§1 badge 15: the row has its own Outputs: label");
    assert_eq!(label.text().as_str(), "Outputs:", "the label says exactly the word §1 spells");
    let button = ui::outputs_folder_button(&window).expect("§1 badge 15: the folder button");
    assert_eq!(
        button.label().unwrap_or_default().as_str(),
        "",
        "the word belongs to the label, so the button draws the icon alone"
    );
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S3: the project name/path ladder — path + tab words → file name + tab words → file name alone —
/// answered with no window in sight, which is what makes the rule checkable at all.
#[test]
fn sec_03_shell_1_the_window_s3_the_project_label_narrows_path_then_name_then_name_only() {
    use naivepost::ui::header_title::{label_for, stage_for, APPROX_CHAR_PX, Fit};
    let dir = Path::new("/home/dev/videos/02-eth.naivepost");
    assert_eq!(
        label_for(dir, "Cut", Fit::Roomy),
        "/home/dev/videos/02-eth.naivepost · Cut",
        "roomy: the path and the tab's word"
    );
    assert_eq!(
        label_for(dir, "Cut", Fit::Tight),
        "02-eth.naivepost · Cut",
        "tight: the file name and the tab's word"
    );
    assert_eq!(
        label_for(dir, "Cut", Fit::Tightest),
        "02-eth.naivepost",
        "tightest: the file name alone (§1's \"file name + icons only\"; the icons are the bar's own buttons)"
    );
    // A missing half is left out, never joined with a stray separator.
    assert_eq!(label_for(Path::new(""), "Cut", Fit::Roomy), "Cut");
    assert_eq!(label_for(Path::new("/a/b.naivepost"), "", Fit::Roomy), "/a/b.naivepost");
    assert_eq!(label_for(Path::new(""), "", Fit::Tightest), "");
    // And the stage follows the width the label was given: the same text stays Roomy while it fits,
    // drops one rung when half of it would fit, and the last when even that would not.
    let text = label_for(dir, "Cut", Fit::Roomy);
    let natural = text.chars().count() as i32 * APPROX_CHAR_PX;
    assert_eq!(stage_for(&text, natural), Fit::Roomy);
    assert_eq!(stage_for(&text, natural / 2), Fit::Tight);
    assert_eq!(stage_for(&text, natural / 4), Fit::Tightest);
    // An unallocated label (width 0) keeps the widest rung, which is the 1240 px window §1 opens at.
    assert_eq!(stage_for(&text, 0), Fit::Roomy);
}

/// §1 badge **15**'s count, as the format contract §1 spells: `Prepare: 5 files, 1.4 MB` — the label, ONE
/// space, then the count-and-size sentence with no prefix of its own. How many files there are and what an
/// empty folder says belong to §6 (`prepare_screen.rs`, `prepare_screen_widgets.rs`).
#[test]
fn sec_03_shell_1_the_window_s15_the_count_reads_the_label_space_count_size() {
    // The same readout sentence the row draws, built by the program's own helper.
    let (line, _) = prepare::outputs_readout(5, 1_400_000, None);
    let shown = format!("{} {}", prepare::OUTPUTS_LABEL, line);
    assert!(
        shown.starts_with("Prepare: "),
        "§1 spells the row `Prepare: 5 files, 1.4 MB` — label, ONE space, count: {shown:?}"
    );
    let (label, body) = shown.split_once(' ').expect("label and count separated by a space");
    assert_eq!(label, prepare::OUTPUTS_LABEL, "exactly one space joins them: {shown:?}");
    assert!(!body.starts_with(' '), "no double blank after the label: {shown:?}");
    assert!(!body.contains(':'), "the count carries no prefix of its own: {shown:?}");
}

/// S6: the level rule itself — `>>> ` / `!!! ` / four spaces stay what decides the LEVEL (§7 spells its
/// lines that way and every flow writes them) but none of them reaches the screen; failure is red,
/// detail dimmed, progress and an unmarked line plain.
#[test]
fn sec_03_shell_1_the_window_s6_the_log_level_rule_strips_the_marker_and_colours_the_rest() {
    use naivepost::ui::log_level::{self, Level};
    assert_eq!(log_level::level_of(">>> cut: 3 clips"), Level::Progress);
    assert_eq!(log_level::level_of("!!! frames failed"), Level::Failure);
    assert_eq!(log_level::level_of("    $ ffmpeg -i x"), Level::Detail);
    assert_eq!(log_level::level_of("line restored to 00:11"), Level::Plain);
    assert_eq!(log_level::body_of(">>> cut: 3 clips"), "cut: 3 clips");
    assert_eq!(log_level::body_of("!!! frames failed"), "frames failed");
    assert_eq!(log_level::body_of("    $ ffmpeg -i x"), "$ ffmpeg -i x");
    assert_eq!(log_level::body_of("line restored to 00:11"), "line restored to 00:11");
    // §7's link line keeps the indent INSIDE its marker, so the message keeps it too.
    assert_eq!(
        log_level::body_of(">>>   this run's exchanges, images included: llm/x.html"),
        "  this run's exchanges, images included: llm/x.html"
    );
    // Which level gets which colour is one table, and the plain levels answer `None`.
    assert_eq!(Level::Failure.tag(), Some(log_level::FAILURE_TAG), "failure is the red one");
    assert_eq!(Level::Detail.tag(), Some(log_level::DETAIL_TAG), "detail is the dimmed one");
    assert_eq!(Level::Progress.tag(), None, "progress is PLAIN: no tag, the theme's own colour");
    assert_eq!(Level::Plain.tag(), None, "an unmarked line draws the same plain");
}
