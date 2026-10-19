//! F2.7's three pictures, pinned by state instead of by eye.
//!
//! `spec/img/05-add.png`, `05-split.png` and `05-remove.png` each show ONE state of this flow. The completion
//! check that reopened this item found all three rendering byte-identical PNGs — the staging arm in
//! `snapshot_verbs` is what fixes that, and this file is what keeps it fixed: every test calls the same
//! `stage` the snapshot recipe calls and asserts on the cut, the selection and the status row the stage left
//! behind. A staging arm that quietly fell back to the bare fixture would fail here rather than ship three
//! copies of the same picture again.
//!
//! One application, one `connect_activate`, one `#[test]` (`tests/suite/f2_7_verb_badges.rs`'s shape): a second
//! fixture in one binary races the process-wide slots this page stores its cut in, so the four checks below
//! run inside the single activation, each in its own temp folder.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Cut;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_ADD: AtomicBool = AtomicBool::new(false);
static RAN_SPLIT: AtomicBool = AtomicBool::new(false);
static RAN_REMOVE: AtomicBool = AtomicBool::new(false);
static RAN_DISTINCT: AtomicBool = AtomicBool::new(false);

/// The cut this window is living on right now, through the page's public door (`ui::newest_review_cut` is
/// crate-private, and a test reads what a widget would read).
fn live_segs(window: &adw::ApplicationWindow) -> Vec<naivepost::cut::Seg> {
    ui::review_cut_of(window).segs
}

/// Fixture seconds for all three shots: two clips with a real 10 s hole (see `snapshot_verbs::seeded`).
const FIRST: (f64, f64) = (10.0, 30.0);
const SECOND: (f64, f64) = (40.0, 60.0);

/// A Cut-page window whose session and cut are the ones the three shots share, in its own temp folder so the
/// verb presses have a real `cut/cut.json` to write.
fn shot_window(app: &adw::Application, slug: &str) -> adw::ApplicationWindow {
    let root = std::env::temp_dir().join(format!(
        "naivepost-f27-shots-{}-{slug}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(session.join("cut")).expect("a session cut folder");
    std::fs::create_dir_all(session.join("prepare")).expect("a session prepare folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    // The stage seeds its own session and cut; seeding the same figure first is what lets a check say what
    // changed against the fixture rather than against an empty page.
    ui::reopen_history_on(&window, &fixture_cut());
    ui::seed_review_cut(&window, &fixture_cut());
    ui::refresh_camera_rows(&window);
    settle();
    window
}

/// The fixture the stage starts from, spelled here so an assertion can name what changed against it.
fn fixture_cut() -> Cut {
    Cut {
        segs: vec![
            naivepost::cut::Seg {
                s: FIRST.0,
                e: FIRST.1,
                cam: 0,
                ..Default::default()
            },
            naivepost::cut::Seg {
                s: SECOND.0,
                e: SECOND.1,
                cam: 0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

/// Footage spans kept, in list order: comparing coverage rather than segment lists, because a verb may split
/// a clip into pieces and still keep exactly the same footage.
fn kept(segs: &[naivepost::cut::Seg]) -> Vec<(f64, f64)> {
    segs.iter()
        .filter(|seg| seg.ins.is_empty() && seg.e > seg.s)
        .map(|seg| (seg.s, seg.e))
        .collect()
}

/// Total kept seconds — Split must not change this number whatever else it does.
fn total(segs: &[naivepost::cut::Seg]) -> f64 {
    kept(segs).iter().map(|(s, e)| e - s).sum()
}

/// The per-gap fold badges currently drawn over the band, by their widget names. `fold_gaps` answers every
/// dropped gap of the whole tape (head, holes, tail), which says nothing about THIS hole; the badges are what
/// §B actually put on screen, one `fold-button-<i>` per gap wide enough to carry a −.
fn fold_badges(window: &adw::ApplicationWindow) -> Vec<String> {
    let Some(band) = ui::fold_overlay(window) else {
        return Vec::new();
    };
    band.observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|child| child.widget_name().to_string())
        .filter(|name| name.starts_with("fold-button-"))
        .collect()
}

/// A named button anywhere in this window's tree.
fn button(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Button> {
    let content = window.content()?;
    find_named(content.upcast_ref(), name)?.downcast::<gtk::Button>().ok()
}

fn find_named(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(node) = child {
        if let Some(found) = find_named(&node, name) {
            return Some(found);
        }
        child = node.next_sibling();
    }
    None
}

/// S1 / the ＋ Add picture: the band lying in the 30–40 hole becomes kept footage, and the status row says
/// the plain one-row sentence the spec quotes. Asserted on the app's own state after the SAME call the
/// snapshot recipe makes, so the picture cannot drift from the rule. Returns which check ran.
fn check_add_shot(app: &adw::Application) {
    let window = shot_window(app, "add");
    let before = live_segs(&window);
    assert_eq!(total(&before), 40.0, "the fixture keeps two 20 s clips");
    let dir = fixture_dir();
    let said = naivepost::snapshot_verbs::stage(&window, "05-add", &dir);
    let line = said.last().cloned().unwrap_or_default();
    println!("ADD shot status: {line}");
    assert!(
        line.starts_with("added") && line.contains('\u{21b6}'),
        "the shot's status row carries §F2.7's add sentence, got {line:?}"
    );
    assert_eq!(
        status_text(&window),
        line,
        "and the painted status row says the same thing the stage returned"
    );
    let after = live_segs(&window);
    assert!(
        kept(&after)
            .iter()
            .any(|(s, e)| *s <= 32.0 && *e >= 37.0),
        "the selected stretch of the hole is now kept footage, kept = {:?}",
        kept(&after)
    );
    assert!(
        total(&after) > total(&before),
        "Add kept more than the fixture did: {} -> {}",
        total(&before),
        total(&after)
    );
    // The wire, not just the rule: the toolbar's own ＋ Add button, fired the way GTK does. Its band is gone
    // (Add spends the selection), so the honest answer here is §F2.7's no-band refusal — which is itself the
    // proof the click reached the verb rather than a no-op.
    let add = button(&window, "add-button").expect("the Cut page carries add-button");
    add.emit_by_name::<()>("clicked", &[]);
    println!("ADD button status: {}", status_text(&window));
    assert_eq!(
        status_text(&window),
        "drag a region on a track first",
        "with nothing selected the real button gives §F2.7's no-band refusal"
    );
    window.close();
}

/// S2 / the | Split picture: borders at BOTH ends of the band, nothing removed, and the selection still up
/// (that is how the person sees which borders they asked for).
fn check_split_shot(app: &adw::Application) {
    let window = shot_window(app, "split");
    let before = live_segs(&window);
    let dir = fixture_dir();
    let said = naivepost::snapshot_verbs::stage(&window, "05-split", &dir);
    let line = said.last().cloned().unwrap_or_default();
    println!("SPLIT shot status: {line}");
    assert!(
        line.starts_with("split at") && line.contains("scenes, was"),
        "the shot's status row names the borders it drew, got {line:?}"
    );
    assert!(
        line.contains("00:14") && line.contains("00:24"),
        "both ends of the band are named, since both were new borders: {line:?}"
    );
    assert!(
        ui::selection(&window).is_some(),
        "Split leaves the selection up on purpose, so the shot can show the band between its borders"
    );
    let after = live_segs(&window);
    assert_eq!(
        total(&after),
        total(&before),
        "| Split removes nothing: {} -> {}",
        total(&before),
        total(&after)
    );
    let edges: Vec<f64> = kept(&after).into_iter().flat_map(|(s, e)| [s, e]).collect();
    assert!(
        edges.contains(&14.0) && edges.contains(&24.0),
        "the two borders the sentence names are in the cut, edges = {edges:?}"
    );
    assert_eq!(
        kept(&after).len(),
        4,
        "one clip became two and the other stayed: {:?}",
        kept(&after)
    );
    window.close();
}

/// S3 / the － Remove picture: exactly the band goes, both remainders survive around the hole, the selection
/// clears, and the new gap is wide enough to wear a fold badge.
fn check_remove_shot(app: &adw::Application) {
    let window = shot_window(app, "remove");
    let before = live_segs(&window);
    let dir = fixture_dir();
    let said = naivepost::snapshot_verbs::stage(&window, "05-remove", &dir);
    let line = said.last().cloned().unwrap_or_default();
    println!("REMOVE shot status: {line}");
    assert!(
        line.starts_with("removed") && line.contains('\u{21b6}'),
        "the shot's status row is §F2.7's remove sentence, got {line:?}"
    );
    // The spec's wording has two branches and this fixture earns the second one: dropping 20–26 out of the
    // 10–30 clip leaves TWO pieces where there was one, so the sentence says "the scene it went through is two
    // now" instead of counting scenes (`cut_verbs::remove`, S3). Both branches carry the dropped seconds.
    assert!(
        line.contains("6.0 s") && line.contains("two now") && line.contains('\u{21b6}'),
        "it names the six seconds that went and says the clip became two: {line:?}"
    );
    assert!(
        ui::selection(&window).is_none(),
        "Remove spends the selection — the band it dropped is gone, so nothing should still be drawn"
    );
    let after = kept(&live_segs(&window));
    assert!(
        after
            .iter()
            .any(|(s, e)| (*s - FIRST.0).abs() < 1e-9 && (*e - 20.0).abs() < 1e-9),
        "the remainder before the band survives: {after:?}"
    );
    assert!(
        after
            .iter()
            .any(|(s, e)| (*s - 26.0).abs() < 1e-9 && (*e - FIRST.1).abs() < 1e-9),
        "and the remainder after it: {after:?}"
    );
    assert!(
        !after.iter().any(|(s, e)| *s < 26.0 && *e > 20.0),
        "nothing kept overlaps the dropped span 20-26: {after:?}"
    );
    let now = total(&live_segs(&window));
    assert!(
        now < total(&before),
        "the cut got shorter: {} -> {}",
        total(&before),
        now
    );
    // The gap the shot shows is foldable, read off the BADGES themselves rather than a guessed span: §B draws
    // one `fold-button-<i>` per gap wide enough to fold, so counting them in the band's own holder is the
    // badge's presence as a number (`refresh_fold_badges` ran inside the verb's report).
    let badges = fold_badges(&window);
    println!("REMOVE shot fold badges = {}", badges.join(","));
    assert!(
        !badges.is_empty(),
        "the hole Remove opened wears a fold badge over it"
    );
    window.close();
}

/// The defect in one assertion: three screens, three different results. Stated at state level because that is
/// what made the PNGs identical — the same segments, the same selection, the same status row.
fn check_the_three_shots_differ(app: &adw::Application) {
    let dir = fixture_dir();
    let mut lines: Vec<String> = Vec::new();
    let mut segs: Vec<Vec<(f64, f64)>> = Vec::new();
    for screen in ["05-add", "05-split", "05-remove"] {
        // Fresh fixture each time: staged in sequence on one window, shot 2 would inherit shot 1's edit and
        // differ for the wrong reason.
        let window = shot_window(app, &screen.replace('0', "z"));
        let said = naivepost::snapshot_verbs::stage(&window, screen, &dir);
        lines.push(said.last().cloned().unwrap_or_default());
        segs.push(kept(&live_segs(&window)));
        window.close();
    }
    println!("THREE statuses: {}", lines.join(" // "));
    for (a, b) in [(0usize, 1usize), (0, 2), (1, 2)] {
        assert_ne!(
            lines[a], lines[b],
            "shots {a} and {b} print the same sentence ({})",
            lines[a]
        );
        assert_ne!(
            segs[a], segs[b],
            "shots {a} and {b} left the same cut: {:?}",
            segs[a]
        );
    }
}

/// The four checks, all inside ONE activation of ONE application: a second `adw::Application` in a process is
/// refused (`g_application_run: assertion !must_quit_now failed`), GTK must be initialized before libadwaita
/// windows exist, and a window may only be added after the application started. A failure therefore surfaces
/// as an assertion message collected here rather than as a panic that escapes into GTK's C loop and aborts.
fn round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let failures = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log = failures.clone();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        let log = log.clone();
        app.connect_activate(move |app| {
            for (index, check) in [
                ("add", check_add_shot as fn(&adw::Application)),
                ("split", check_split_shot),
                ("remove", check_remove_shot),
                ("distinct", check_the_three_shots_differ),
            ] {
                if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(app))) {
                    let why = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "a non-string panic".to_string());
                    log.lock().unwrap().push(format!("{index}: {why}"));
                } else {
                    match index {
                        "add" => RAN_ADD.store(true, Ordering::SeqCst),
                        "split" => RAN_SPLIT.store(true, Ordering::SeqCst),
                        "remove" => RAN_REMOVE.store(true, Ordering::SeqCst),
                        _ => RAN_DISTINCT.store(true, Ordering::SeqCst),
                    }
                }
            }
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    let failed = failures.lock().unwrap().clone();
    assert!(failed.is_empty(), "shot checks failed -> {}", failed.join(" // "));
}

#[test]
fn f2_7_s1_the_add_shot_stages_a_gap_into_a_kept_scene() {
    round();
    assert!(RAN_ADD.load(Ordering::SeqCst), "the add-shot check never ran");
}

#[test]
fn f2_7_s2_the_split_shot_leaves_the_band_up_with_a_border_at_each_end() {
    round();
    assert!(
        RAN_SPLIT.load(Ordering::SeqCst),
        "the split-shot check never ran"
    );
}

#[test]
fn f2_7_s3_the_remove_shot_drops_the_selection_and_opens_a_foldable_gap() {
    round();
    assert!(
        RAN_REMOVE.load(Ordering::SeqCst),
        "the remove-shot check never ran"
    );
}

#[test]
fn f2_7_s5_the_three_shots_are_three_different_cuts_and_sentences() {
    round();
    assert!(
        RAN_DISTINCT.load(Ordering::SeqCst),
        "the three-shots-differ check never ran"
    );
}
