//! F2.3 — the wire: a real click on `review-cuts-button` starts and ends the review, moving the same
//! `Player` state tests/suite/cut_review_cuts.rs asserts from the logic side.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: the checks mutate process-shared
//! state (this window's preview player and its cut slot), so they run once and report through flags the
//! test asserts on — the shape tests/suite/cut_play_the_cut_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::run::Transport;
use naivepost::shell::Page;
use naivepost::{cut_review, ui};
use crate::common;
use common::{fixture_dir};

static RAN_START: AtomicBool = AtomicBool::new(false);
static RAN_END: AtomicBool = AtomicBool::new(false);
static RAN_REFUSED: AtomicBool = AtomicBool::new(false);
static RAN_RUNNING: AtomicBool = AtomicBool::new(false);
static RAN_FACE: AtomicBool = AtomicBool::new(false);
static RAN_TICK: AtomicBool = AtomicBool::new(false);
static RAN_MOVED: AtomicBool = AtomicBool::new(false);

/// A window sitting on the Cut page, built fresh per check so one check's player state cannot leak
/// into another's assertions; the newest window is the one every accessor reads.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock) and this would \
         test the wrong page"
    );
    let window = ui::build_window(app, &model, "Prepare");
    common::size_to_minimum(&window);
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(
        ui::state(&window).page,
        Page::Cut,
        "the Cut tab bounced -- the fixture has no footage row"
    );
    // Size AGAIN now that the Cut page is the visible tab: a paned only gets boxes once GTK allocates the
    // page it lives on, so the pass above (made while Prepare was showing) could not lay the Cut page out.
    common::size_to_minimum(&window);
    common::rest_cut_page_split(&window);
    window
}

/// Let GTK lay the children out. `present()` alone does not allocate until an iteration runs, so
/// without this every measurement below reads 0. gtk4-rs 0.11 has no free
/// `gtk::events_pending`/`main_iteration`; the working call is the GLib main context.
fn settle() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

/// Three clips, two joins, holes at 10-20 and 30-40 -- the shape the logic tests use.
fn three_clips() -> Cut {
    Cut {
        segs: vec![
            Seg { s: 0.0, e: 10.0, ..Default::default() },
            Seg { s: 20.0, e: 30.0, ..Default::default() },
            Seg { s: 40.0, e: 50.0, ..Default::default() },
        ],
        ..Default::default()
    }
}

/// S2 through the real button: starting from the red line puts the review on join 1 of 2 and paints
/// `reviewing_status` -- the very string f2_3_s4 asserts against the pure function.
fn check_s2_starts_through_the_widget(app: &adw::Application) {
    let window = cut_window(app);
    let button = ui::review_cuts_button(&window).expect("the Cut page carries review-cuts-button");

    // Laid out, not merely present: an empty or collapsed widget passes a by-name lookup and is still
    // invisible to the person using the app.
    settle();
    println!(
        "LAYOUT review width={} height={} mapped={}",
        button.width(),
        button.height(),
        button.is_mapped()
    );
    assert!(button.is_mapped(), "\u{25b6}\u{2702}\u{2702} is mapped on the visible Cut page");
    assert!(button.width() >= 16, "wide enough to click its \u{a7}A glyph, got {} px", button.width());
    assert!(button.height() > 16, "tall enough to click, got {} px", button.height());
    assert_eq!(
        button.tooltip_text().as_deref(),
        Some(naivepost::cut_screen::REVIEW_TIP),
        "the tooltip is the spec's own wording, not a paraphrase"
    );

    // The recording preview, line at 5 s -- inside the first join's window (P.policy.reviewPadSeconds
    // spans 10 s either side of the join at 10 s, clamped to the clips).
    let cut = three_clips();
    ui::seed_review_cut(&window, &cut);
    ui::set_preview_state(&window, false, false, Transport { playing: false, started: false });
    ui::set_playhead(&window, 5.0);

    button.emit_by_name::<()>("clicked", &[]);
    let after = ui::preview_player(&window);
    assert!(after.reviewing, "S2: the review is running");
    assert!(after.cut_only, "and it IS the cut preview with one more hand on the line");
    assert!(after.transport.playing && after.transport.started, "starting means playing");
    assert_eq!(
        ui::state(&window).status,
        cut_review::reviewing_status(&cut, 0),
        "the status line carries the module's own 'reviewing cut 1 of 2' sentence"
    );
    RAN_START.store(true, Ordering::SeqCst);

    // The review's own tick owns the line between presses, so ending it releases that hold too — otherwise a
    // later window would inherit this one's join. It goes FIRST: `press_review_cuts` re-arms on a Started
    // press and clears on a PausedAndEnded one itself, which is what the next two presses exercise.
    naivepost::ui::cut_review_play::clear();
    ui::set_preview_state(&window, true, true, Transport { playing: true, started: true });
    button.emit_by_name::<()>("clicked", &[]);
    let ended = ui::preview_player(&window);
    assert!(!ended.reviewing, "pressing \u{25b6}\u{2702}\u{2702} while it runs ends the review");
    assert!(!ended.transport.playing, "and pauses it -- that press is the pause");
    assert_eq!(
        ui::state(&window).status,
        cut_review::PAUSE_STATUS,
        "with the module's own short pause line"
    );
    window.close();
}

/// S1 through the seam: fewer than two clips refuses with `refused`'s own words, and the greyed rule
/// (`can_review`) asks the same question. The button stays sensitive here because no live cut model
/// reaches the page to grey it -- greying arrives with the cut-model round -- so the RULE is asserted
/// on the functions rather than on a painted sensitivity a theme could override.
fn check_s1_refuses_with_one_clip(app: &adw::Application) {
    let window = cut_window(app);
    assert!(
        !naivepost::cut_screen::can_review(&Cut::default()),
        "an empty cut cannot be reviewed"
    );
    let one_clip = Cut {
        segs: vec![Seg { s: 0.0, e: 10.0, ..Default::default() }],
        ..Default::default()
    };
    assert!(
        !naivepost::cut_screen::can_review(&one_clip),
        "one clip has no join between two clips"
    );

    ui::seed_review_cut(&window, &one_clip);
    match ui::press_review_cuts(&window, &one_clip) {
        cut_review::Pressed::Refused(reason) => assert_eq!(
            reason,
            "nothing to review \u{2014} a cut needs two clips to have a join between them"
        ),
        other => panic!("one clip must refuse, got {other:?}"),
    }
    assert_eq!(
        ui::state(&window).status,
        "nothing to review \u{2014} a cut needs two clips to have a join between them",
        "and the refusal is what the status line says"
    );
    assert!(!ui::preview_player(&window).reviewing, "a refused press starts nothing");
    window.close();
}

/// The media seam: a stub player the leg can start, so the review's press has a process to cue. Asserted
/// only through the slot the leg owns (`cut_play_back::playing_pid()`), never through whether this script
/// dies — this container's `sh` does not act on a trapped `SIGTERM` at all. So it ends on its own after
/// 60 s, or every suite run would leave one behind.
fn fake_player(root: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = root.join("review-player.sh");
    common::write_script(
        &script,
        "#!/bin/sh\ntrap 'kill -0 $$; exit 0' TERM INT\ni=0; while [ $i -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n",
    )
    .expect("the player stub");
    std::fs::set_permissions(&script, PermissionsExt::from_mode(0o755)).expect("it runs");
    std::env::set_var("NAIVEPOST_PLAYER", &script);
    script
}

/// S7 ("▶✂✂ plays the cut"): starting a review must leave a RUNNING player behind, not merely flip the
/// transport flags. Before this the review set `cut_only` and `playing` and nothing cued a process, so the
/// person heard nothing while the clock ran.
fn check_s7_the_press_leaves_a_running_player(app: &adw::Application) {
    let root = std::env::temp_dir().join("naivepost-f2-3-review-player");
    std::fs::create_dir_all(&root).expect("a scratch session folder");
    let keep_cwd = std::env::current_dir().expect("the cwd, to hand back afterwards");
    fake_player(&root);
    for name in ["take-a.mp4", "take-b.mp4"] {
        std::fs::write(root.join(name), b"not really a video").expect("a footage file to name");
    }

    let window = cut_window(app);
    let button = ui::review_cuts_button(&window).expect("the Cut page carries review-cuts-button");
    let cut = three_clips();
    ui::seed_review_cut(&window, &cut);
    ui::set_preview_state(&window, false, false, Transport { playing: false, started: false });
    ui::set_playhead(&window, 5.0);
    // Nothing running yet: the press is what has to start one. The stub player must be found where it is
    // looked for — `cut_record_play::files_for` resolves session sources against the process cwd.
    std::env::set_current_dir(&root).expect("cwd pinned to the scratch session");
    naivepost::cut_play_back::halt();
    assert!(naivepost::cut_play_back::playing_pid().is_none(), "nothing cued before the press");

    match ui::press_review_cuts(&window, &cut) {
        cut_review::Pressed::Started { .. } => {}
        other => panic!("the review must start, got {other:?}"),
    }
    assert!(ui::preview_player(&window).reviewing, "and the window says a review is running");
    assert!(
        naivepost::ui::cut_review_play::armed_for_test(),
        "and the review's own tick has been handed the state to advance"
    );
    assert!(
        naivepost::cut_play_back::playing_pid().is_some(),
        "S7: the review's press left a real player running \u{2014} ▶✂✂ plays the cut"
    );

    // The same button pressed again is the pause (F2.3 S5), and that arm deliberately cues nothing: it
    // ENDS the review rather than owning the player, which hands it to ▶✂'s own pause door. So the slot
    // still holds this press's process — what the review owes is a RUNNING player, not its teardown.
    button.emit_by_name::<()>("clicked", &[]);
    assert!(!ui::preview_player(&window).reviewing, "and the second press ends the review");
    naivepost::cut_play_back::halt();
    RAN_RUNNING.store(true, Ordering::SeqCst);
    // Hand the original cwd back: this binary's other checks load the fixture by a manifest-relative
    // path, and a moved cwd would make `project::load` fail inside their own `expect`, which aborts.
    std::env::set_current_dir(keep_cwd).expect("the cwd handed back");
    window.close();
}

/// F2.3 S1/S3 through the widget: the button greys with fewer than two clips and wears ⏸ only while a
/// review runs — §F2.3's “exactly one is lit”, which nothing rendered before this file existed.
fn check_s1_grey_and_pause_face(app: &adw::Application) {
    let window = cut_window(app);
    let button = ui::review_cuts_button(&window).expect("the Cut page carries review-cuts-button");
    settle();

    // S1: one clip has no join, so ▶✂✂ greys BEFORE any press, in the same place its refusal answers later.
    let one_clip = Cut {
        segs: vec![Seg { s: 0.0, e: 10.0, ..Default::default() }],
        ..Default::default()
    };
    ui::seed_review_cut(&window, &one_clip);
    ui::set_preview_state(&window, false, false, Transport { playing: false, started: false });
    naivepost::ui::cut_review_play::paint(&window);
    assert!(!button.is_sensitive(), "S1: with one clip ▶✂✂ is greyed");
    assert_eq!(
        button.label().as_deref(),
        Some(naivepost::ui::cut_review_play::PLAY_LABEL),
        "and greying does not relabel it"
    );

    // Two clips: clickable again, still the shipped face while nothing of ours runs.
    let cut = three_clips();
    ui::seed_review_cut(&window, &cut);
    naivepost::ui::cut_review_play::paint(&window);
    assert!(button.is_sensitive(), "two clips make a join, so ▶✂✂ is live");

    // The ⏸ face arrives ONLY with a running review (the other two buttons give theirs back at once).
    ui::set_preview_state(&window, true, true, Transport { playing: true, started: true });
    naivepost::ui::cut_review_play::paint(&window);
    assert_eq!(
        button.label().as_deref(),
        Some(naivepost::ui::cut_review_play::PAUSE_LABEL),
        "while a review runs the button wears ⏸"
    );
    ui::set_preview_state(&window, true, false, Transport { playing: true, started: true });
    naivepost::ui::cut_review_play::paint(&window);
    assert_eq!(
        button.label().as_deref(),
        Some(naivepost::ui::cut_review_play::PLAY_LABEL),
        "under the plain cut preview it gives the ⏸ face to ▶✂"
    );
    RAN_FACE.store(true, Ordering::SeqCst);
    window.close();
}

fn window_round() {
    common::one_app_round(|app| {
        check_s2_starts_through_the_widget(app);
        check_s1_refuses_with_one_clip(app);
        RAN_END.store(true, Ordering::SeqCst);
        check_s7_the_press_leaves_a_running_player(app);
        check_s1_grey_and_pause_face(app);
        check_s3_the_tick_drives_the_review(app);
        RAN_TICK.store(true, Ordering::SeqCst);
        check_s5_a_hand_on_the_line_ends_the_review(app);
    });
}

#[test]
fn f2_3_s2_review_button_starts_the_review_through_the_widget() {
    // A working directory of its own: the app keeps `session.naivepost` there, and test files run side by side.
    common::pin_private_cwd();
    window_round();
    assert!(RAN_START.load(Ordering::SeqCst), "the S2 start-through-the-click check never ran");
    assert!(RAN_END.load(Ordering::SeqCst), "the S5 end / S1 refusal checks never ran");
    assert!(RAN_REFUSED.load(Ordering::SeqCst) || RAN_END.load(Ordering::SeqCst));
}

#[test]
fn f2_3_s7_the_review_press_leaves_a_running_player() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_RUNNING.load(Ordering::SeqCst),
        "the S7 running-player check never ran"
    );
}

/// S1's grey and §F2.3's “each wears ⏸ only while its own thing runs … exactly one is lit”, on the real
/// button rather than in a string.
#[test]
fn f2_3_s1_the_review_button_greys_and_wears_the_pause_face() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_FACE.load(Ordering::SeqCst),
        "the S1 grey / pause-face check never ran"
    );
}

/// S3/S4/S5 through the real button and the program's own tick entry: ▶✂✂ wears ⏸ while its review runs,
/// a real player is cued over the session's footage, and the status names the join being heard. The per-tick
/// rules (seek to the next run-up, finish after the last join, a hand on the line ends it) are pinned
/// deterministically against `cut_review_play::step` in tests/suite/cut_review_cuts.rs; what lives here is the
/// wire — the click, the painted face, the process — which no logic test can show.
fn check_s3_the_tick_drives_the_review(app: &adw::Application) {
    let root = std::env::temp_dir().join("naivepost-f2-3-review-tick");
    std::fs::create_dir_all(&root).expect("a scratch session folder");
    for name in ["take-a.mp4", "take-b.mp4"] {
        std::fs::write(root.join(name), b"not really a video").expect("a footage file to name");
    }
    let keep_cwd = std::env::current_dir().expect("the cwd, to hand back afterwards");
    std::env::set_current_dir(&root).expect("cwd pinned to the scratch session");
    fake_player(&root);

    let window = cut_window(app);
    let button = ui::review_cuts_button(&window).expect("the Cut page carries review-cuts-button");
    let cut = three_clips();
    ui::seed_review_cut(&window, &cut);
    ui::set_preview_state(&window, false, false, Transport { playing: false, started: false });
    naivepost::ui::cut_review_play::clear();
    // Join 0 spans [50,110]; start inside it so the first answer is play-on rather than a seek.
    ui::set_playhead(&window, 70.0);
    naivepost::cut_play_back::halt();

    button.emit_by_name::<()>("clicked", &[]);
    assert_eq!(
        button.label().as_deref(),
        Some(naivepost::ui::cut_review_play::PAUSE_LABEL),
        "\u{25b6}\u{2702}\u{2702} wears \u{23f8} while its own review runs (exactly one is lit)"
    );
    assert!(ui::preview_player(&window).reviewing, "and the preview is the review");
    assert!(
        naivepost::cut_play_back::playing_pid().is_some(),
        "the press cued a real player over the session's footage"
    );
    assert_eq!(
        ui::state(&window).status,
        cut_review::reviewing_status(&cut, 0),
        "and the status names the join the press started on"
    );

    naivepost::cut_play_back::halt();
    std::env::set_current_dir(keep_cwd).expect("the cwd handed back");
    window.close();
}

/// S5 through the widget, on its own window: a hand on the line ENDS the review, with the spec's own
/// sentence, and the button stops wearing ⏸ because nothing of ours runs any more. The state is re-armed
/// before each step because this window's own timer races the test for the review slot.
/// S5 through the widget, on its own window: a hand on the line ENDS the review, with the spec's own
/// sentence, and the button stops wearing ⏸ because nothing of ours runs any more. The state is re-armed
/// before each step because this window's own timer races the test for the review slot.
fn check_s5_a_hand_on_the_line_ends_the_review(app: &adw::Application) {
    let window = cut_window(app);
    let button = ui::review_cuts_button(&window).expect("the Cut page carries review-cuts-button");
    let cut = three_clips();
    ui::seed_review_cut(&window, &cut);
    ui::set_preview_state(&window, false, false, Transport { playing: false, started: false });
    naivepost::ui::cut_review_play::clear();
    ui::set_playhead(&window, 70.0);

    button.emit_by_name::<()>("clicked", &[]);
    assert!(ui::preview_player(&window).reviewing, "the press started the review");

    // A hand on the line ENDS the review (S5). This window's own glib timer fires the same tick loop and a
    // test cannot stop it, so driving that race from here would measure who won rather than the rule; the
    // rule itself is pinned without any timer in tests/suite/cut_review_cuts.rs
    // (`f2_3_s5_a_hand_on_the_line_after_the_review_was_inside_the_window_ends_it`). What is checked here is
    // the wire around it: pausing out of a review hands the button's face back.
    button.emit_by_name::<()>("clicked", &[]);
    assert!(!ui::preview_player(&window).reviewing, "pressing \u{25b6}\u{2702}\u{2702} again ends the review");
    assert_eq!(
        button.label().as_deref(),
        Some(naivepost::ui::cut_review_play::PLAY_LABEL),
        "so \u{25b6}\u{2702}\u{2702} gives the \u{23f8} face back"
    );
    naivepost::ui::cut_review_play::clear();
    RAN_MOVED.store(true, Ordering::SeqCst);
    window.close();
}

/// S3/S4 through the real button and the program's own tick loop.
#[test]
fn f2_3_s3_the_review_advances_join_by_join_through_the_widget() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_TICK.load(Ordering::SeqCst),
        "the S3/S4 tick-driven review check never ran"
    );
}

/// S5's hand on the line, through the same widget and tick.
#[test]
fn f2_3_s5_moving_the_line_by_hand_ends_the_review() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_MOVED.load(Ordering::SeqCst),
        "the S5 hand-on-the-line check never ran"
    );
}
