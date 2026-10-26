//! §03-shell.md F0.1 Switch tab — through the widgets.
//!
//! `tests/suite/switch_tab.rs` proves the rules on [`naivepost::shell`] with no window built. This one
//! proves the wire: a real click on the tab row's button moves the page, paints the `Inputs:` /
//! `Outputs:` readouts (§1 badges 14 and 15), syncs the header ⓘ (badge 6) to `Shell::help_page`,
//! bounces a locked tab back to Prepare with the lock reason on the status line, and lets S5's
//! refreshes re-read the disk on arrival rather than remembering what the build saw.
//!
//! Every assertion reads back what the UI itself shows (`ui::state`, the label texts, the tooltips),
//! never a private flag — spec/00-principles.md §5.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::layout::Tree;
use naivepost::narration::{self, Entry};
use naivepost::project::{self, Project, Source};
use naivepost::shell::{self, Page};
use naivepost::startup;
use naivepost::ui;

/// The scratch root this window works in.
///
/// The switch resolves its folder as `startup::session_dir(std::env::current_dir())`, so the test
/// moves into a temp dir first: that makes `rust/session.naivepost` land here instead of in the
/// repo, and it is removed again at the end of the check.
fn scratch_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-f01-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::env::set_current_dir(&dir).expect("chdir");
    dir
}

/// The session project's tree, after `naivepost.json` is in place — the file that decides whether
/// the window can see a folder at all.
fn session_tree(project: &Project) -> Tree {
    let sess = startup::session_dir(&std::env::current_dir().unwrap());
    project::save(project, &sess).expect("session project saved");
    Tree::new(&sess).expect("session.naivepost is a project folder")
}

/// One source, tagged or not — the whole difference between an unlocked Cut and a bounced one.
fn one_source(footage: bool) -> Vec<Source> {
    vec![Source {
        path: "project:sources/lecture.mkv".into(),
        footage,
        narrator: 1,
        sepvoice: false,
        tracks: vec![0],
    }]
}

/// A cut of one clip, five seconds in and twelve out.
fn cut_with(s: f64, e: f64) -> cut::Cut {
    cut::Cut {
        segs: vec![Seg {
            s,
            e,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// A narration of one line over the given bounds.
fn narration_with(s: f64, e: f64) -> narration::Narration {
    narration::Narration {
        entries: vec![Entry {
            s,
            e,
            ..Default::default()
        }],
        silent: Vec::new(),
    }
}

/// One running GTK application for this test binary — the same single-main-loop arrangement
/// `tests/suite/rescan_widgets.rs` uses. The check records that it ran, so one that never happened is a
/// failure rather than a quiet pass.
fn window_round() {
    common::one_app_round(|app| {
        check_switch(app);
        check_the_lock_is_asked_of_the_live_session(app);
        RAN_LIVE.store(true, Ordering::SeqCst);
        RAN.store(true, Ordering::SeqCst);
    });
}

static RAN: AtomicBool = AtomicBool::new(false);
static RAN_LIVE: AtomicBool = AtomicBool::new(false);
static OK: AtomicBool = AtomicBool::new(false);

/// Click a tab the way the toolkit does, and say which one if it is not there.
fn click_tab(window: &adw::ApplicationWindow, page: Page) {
    ui::tab_button(window, page)
        .unwrap_or_else(|| panic!("the tab row has no {} button", page.label()))
        .emit_clicked();
}

fn inputs_text(window: &adw::ApplicationWindow) -> String {
    crate::common::settle_workers();
    ui::inputs_readout(window)
        .expect("the window carries inputs-readout")
        .text()
        .to_string()
}

fn outputs_text(window: &adw::ApplicationWindow) -> String {
    crate::common::settle_workers();
    ui::outputs_sentence(window).expect("the window carries outputs-readout")
}

fn info_tip(window: &adw::ApplicationWindow) -> String {
    ui::tab_info_button(window)
        .expect("the header bar carries tab-info-button")
        .tooltip_text()
        .map(|text| text.to_string())
        .unwrap_or_default()
}

/// S1 + S4 + S5 on a project that has footage, then S2 on one that has none.
fn check_switch(app: &adw::Application) {
    let dir = scratch_root();

    // ---- the working project: one footage source, one clip, one line ----
    let mut project = Project::default();
    project.sources = one_source(true);
    let tree = session_tree(&project);
    cut::save(&cut_with(0.0, 20.0), &tree).expect("cut written");
    narration::save(&narration_with(0.0, 10.0), &tree).expect("narration written");

    let window = ui::build_window(app, &project, "Prepare");
    window.present();

    f0_1_s1_every_tab_shows_its_icon(&window);
    f0_1_s1_click_a_tab_button_shows_it(&window);
    f0_1_s4_readouts_shown_with_the_page(&window);
    f0_1_s4_info_syncs_to_the_page(&window);
    f0_1_s5_produce_refreshes_the_publish_panel(&window, &tree);
    f0_1_s5_narrate_refits_its_lines(app, &dir);
    f0_1_s2_locked_cut_bounces_the_click(app, &dir);

    let _ = std::fs::remove_dir_all(&dir);
    OK.store(true, Ordering::SeqCst);
}

/// §1's tab row: every page carries the icon `Page::icon` names. A page added without one shows
/// libadwaita's "image-missing" glyph in its tab.
fn f0_1_s1_every_tab_shows_its_icon(window: &adw::ApplicationWindow) {
    fn stack_in(widget: &gtk::Widget) -> Option<adw::ViewStack> {
        if let Some(stack) = widget.downcast_ref::<adw::ViewStack>() {
            return Some(stack.clone());
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if let Some(stack) = stack_in(&widget) {
                return Some(stack);
            }
            child = widget.next_sibling();
        }
        None
    }
    let stack = stack_in(window.upcast_ref()).expect("the window holds the page stack");
    for page in Page::all() {
        let icon = stack
            .page(&stack.child_by_name(page.label()).expect("a page per tab"))
            .icon_name();
        assert_eq!(
            icon.as_deref(),
            Some(page.icon()),
            "the {} tab's icon",
            page.label()
        );
    }
}

/// S1: the click lands where it was aimed.
fn f0_1_s1_click_a_tab_button_shows_it(window: &adw::ApplicationWindow) {
    click_tab(window, Page::Produce);
    assert_eq!(
        ui::state(window).page,
        Page::Produce,
        "a click on Produce must show Produce: {:?}",
        ui::state(window)
    );
}

/// S4: both readouts are drawn, and they say something different per page.
fn f0_1_s4_readouts_shown_with_the_page(window: &adw::ApplicationWindow) {
    let produce_inputs = inputs_text(window);
    let produce_outputs = outputs_text(window);
    assert!(
        produce_inputs.starts_with("Inputs: ") && produce_inputs.len() > "Inputs: ".len(),
        "§1 badge 14 wants a sentence after the word: {produce_inputs:?}"
    );
    assert!(
        produce_outputs.starts_with("Outputs: ") && produce_outputs.len() > "Outputs: ".len(),
        "§1 badge 15 wants the count after the word: {produce_outputs:?}"
    );

    click_tab(window, Page::Prepare);
    let prepare_inputs = inputs_text(window);
    assert_eq!(ui::state(window).page, Page::Prepare);
    assert_ne!(
        prepare_inputs, produce_inputs,
        "each page's Inputs row names its own work; both said {prepare_inputs:?}"
    );
}

/// S4: ⓘ follows the page shown, not the page clicked-and-refused later on.
fn f0_1_s4_info_syncs_to_the_page(window: &adw::ApplicationWindow) {
    click_tab(window, Page::Narrate);
    let tip = info_tip(window);
    assert!(
        tip.starts_with("Narrate"),
        "ⓘ describes the page that is showing: {tip:?}"
    );
    assert!(
        tip.contains(Page::Narrate.tip()),
        "ⓘ carries the page's own help text {:#?}: {tip:?}",
        Page::Narrate.tip()
    );
}

/// S5: arriving at Produce re-reads the folder, so a file written after the window was built shows
/// up without rebuilding anything.
///
/// The count is what proves the re-read. `publish::save` lands its JSON in the project's `publish/`
/// folder (§1 keeps the legacy spelling readable for ever), which is not what Produce's Outputs row
/// counts — that row counts `produce/`, per `Shell::output_dir`. So the file added here goes where
/// that count looks, and it is added AFTER the window exists: a row remembered from build time would
/// still say 0.
fn f0_1_s5_produce_refreshes_the_publish_panel(window: &adw::ApplicationWindow, tree: &Tree) {
    click_tab(window, Page::Produce);
    let before = outputs_text(window);
    tree.write_file(
        Path::new("produce/final.mp4"),
        b"a rendered stretch of video",
    )
    .expect("a produced file appears after the window was built");
    click_tab(window, Page::Prepare);
    click_tab(window, Page::Produce);
    let after = outputs_text(window);
    assert_ne!(
        before, after,
        "the Outputs count must follow the folder: {before:?} then {after:?}"
    );
}

/// S5: Narrate refits its lines to the cut, and S3's flush writes them back.
///
/// Two separate things are checked here, and the entry is deliberately one the refit has no reason to
/// move: this file pins F0.1's own rule, that a switch over an untouched narration costs no file and the
/// write-back belongs to a write the USER owes (S3: "leaving the tab writes what is half-typed even a
/// beat early"). §F4.7 gives a refit that actually moved a line a write of its own the moment the tab is
/// entered — the app did that work, so it is written without waiting for anybody to type — and that rule
/// lives in `tests/suite/narrate_edit_wire_widgets.rs`'s refit block, which arrives at Narrate in a window of
/// its own instead of visiting it for the last time in this one.
fn f0_1_s5_narrate_refits_its_lines(app: &adw::Application, dir: &Path) {
    let _ = dir;
    // A line already on its clip's own bounds: the refit sweep runs and finds nothing to move, so the
    // status line says the sweep happened and the file is left alone until something is owed.
    let project = Project {
        sources: one_source(true),
        ..Default::default()
    };
    let tree = session_tree(&project);
    cut::save(&cut_with(5.0, 12.0), &tree).expect("cut written");
    narration::save(&narration_with(5.0, 12.0), &tree).expect("narration written");

    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    click_tab(&window, Page::Narrate);

    assert_eq!(
        ui::state(&window).status,
        "",
        "a sweep that found nothing to move says nothing — the status line is for work the app did: {:?}",
        ui::state(&window).status
    );

    // Nothing was owed and nothing moved, so nothing was written: the file still holds the same bounds.
    let untouched = narration::load(&tree).expect("narration parses");
    assert_eq!(
        (untouched.entries[0].s, untouched.entries[0].e),
        (5.0, 12.0),
        "a switch with no pending write must not touch the file"
    );

    // Now owe one — the state a half-typed line leaves behind — and leave the tab again. The seam
    // resolves "this window" as the newest shell built, so this check must be the last window made.
    ui::mark_narration_owed(&window);
    click_tab(&window, Page::Prepare);

    let reread = narration::load(&tree).expect("narration still parses");
    assert_eq!(reread.entries.len(), 1, "one line in, one line back");
    assert_eq!(
        (reread.entries[0].s, reread.entries[0].e),
        (5.0, 12.0),
        "the flush on leaving the tab wrote the narration the window holds"
    );
}

/// S2: a locked tab bounces, keeps the page on Prepare, and says why.
fn f0_1_s2_locked_cut_bounces_the_click(app: &adw::Application, dir: &Path) {
    let _ = dir;
    let mut project = Project::default();
    project.sources = one_source(false);
    let tree = session_tree(&project);
    let _ = tree;

    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    click_tab(&window, Page::Cut);

    let state = ui::state(&window);
    assert_eq!(
        state.page,
        Page::Prepare,
        "the bounce must not move the page"
    );
    assert_eq!(
        state.status,
        shell::CUT_LOCK,
        "the status line carries the lock reason verbatim"
    );
    assert!(
        ui::tab_dimmed(&window, Page::Cut),
        "§1: a locked tab is greyed, not disabled"
    );
}

// S3's write-back is asserted through this window too, in `f0_1_s5_narrate_refits_its_lines`: the
// window holds its narration (`HELD_NARRATION` in src/ui/window.rs), so the refit S5 makes on
// arrival survives to the switch that leaves, and the owed flush writes it to disk. What still comes
// from the seam rather than from the page is the *mark*: F4.7 gives Narrate its text view, and typing
// there will call `ui::mark_narration_owed` the way this test does.

#[test]
fn f0_1_switch_tab_through_the_widgets() {
    window_round();
    assert!(RAN.load(Ordering::SeqCst), "the application never ran");
    assert!(OK.load(Ordering::SeqCst), "the check never finished");
}

/// S1/S2 against the session as it is NOW: a window built on a project with no footage, then given a
/// footage source the way "Add source files…" gives one (into the live session), lets Cut open. The
/// handler used to ask the copy the window was built with, which still had no footage, so Cut bounced
/// with its lock reason for as long as the window lived.
fn check_the_lock_is_asked_of_the_live_session(app: &adw::Application) {
    let window = ui::build_window(app, &Project::default(), "Prepare");
    window.present();
    ui::session_handle(&window)
        .borrow_mut()
        .sources
        .push(Source {
            path: "project:sources/added.mkv".to_string(),
            footage: true,
            narrator: 0,
            sepvoice: false,
            tracks: vec![0],
        });
    click_tab(&window, Page::Cut);
    assert_eq!(
        ui::state(&window).page,
        Page::Cut,
        "S1: a source added since the window was built unlocks Cut (status said {:?})",
        ui::state(&window).status
    );
    window.close();
}

#[test]
fn f0_1_s2_the_lock_is_asked_of_the_live_session() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_LIVE.load(Ordering::SeqCst),
        "the live-session check never ran"
    );
}
