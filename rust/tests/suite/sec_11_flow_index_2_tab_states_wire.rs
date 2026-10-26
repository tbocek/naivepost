//! §11-flow-index#2 Tab states — the WIRE round.
//!
//! `tests/suite/tab_states.rs` pins §2's table as a rule. What passes for that file proves nothing about the
//! program: the run bar has to actually become the preview's transport when a preview starts, and the
//! three Cut preview modes have to actually light one-at-a-time on the screen. Both are checked here by
//! pressing the real buttons and reading the state those handlers write.
//!
//! One application, one `connect_activate`, one `#[test]` per case: every check mutates process-shared
//! state (this window's preview player, its cut slot, the run bar), so each runs once inside the activate
//! hook and reports through a flag the test asserts on — the shape
//! `tests/suite/cut_review_cuts_widgets.rs` uses.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use common::{fixture_dir, settle};
use naivepost::cut::{Cut, Seg};
use naivepost::run::Transport;
use naivepost::shell::Page;
use naivepost::{cut, cut_screen, run, tab_states, ui};

static RAN_S4_TRANSPORT: AtomicBool = AtomicBool::new(false);
static RAN_S5_TRIO: AtomicBool = AtomicBool::new(false);
static RAN_S4_REFUSAL: AtomicBool = AtomicBool::new(false);

/// A player-shaped value for the pure §2 helpers, read off the window's real player.
fn preview_of(window: &adw::ApplicationWindow) -> cut_screen::Preview {
    let player = ui::preview_player(window);
    cut_screen::Preview {
        cut_only: player.cut_only,
        reviewing: player.reviewing,
    }
}

/// Three clips: two joins, so ▶✂✂ is sensitive and ▶✂ has something to skip.
fn three_clips() -> Cut {
    Cut {
        segs: vec![
            Seg {
                s: 0.0,
                e: 10.0,
                ..Default::default()
            },
            Seg {
                s: 20.0,
                e: 30.0,
                ..Default::default()
            },
            Seg {
                s: 40.0,
                e: 50.0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

/// A fake player: the review's press cues a real process, and without one the press logs a playback
/// failure instead. Same stub the F2.3 widget test installs.
fn fake_player(root: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let script = root.join("tab-states-player.sh");
    common::write_script(
        &script,
        "#!/bin/sh\ntrap 'kill -0 $$; exit 0' TERM INT\ni=0; while [ $i -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n",
    )
    .expect("the player stub");
    std::fs::set_permissions(&script, PermissionsExt::from_mode(0o755)).expect("it runs");
    std::env::set_var("NAIVEPOST_PLAYER", &script);
}

/// A window sitting on the Cut page of a session with footage and a non-empty cut — §2's second column
/// open, which is the only state the third column's Cut row can be asked in.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked and this checks the wrong page"
    );
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    settle();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    settle();
    assert_eq!(ui::state(&window).page, Page::Cut, "the Cut tab bounced");
    window
}

/// A session folder with two named footage files, so a review press has files to cue.
fn scratch_session(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("naivepost-sec-11-2-{}", tag));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a scratch session folder");
    for name in ["take-a.mp4", "take-b.mp4"] {
        std::fs::write(root.join(name), b"not really a video").expect("a footage file to name");
    }
    root
}

/// S4 through the real buttons: while the Cut preview is started, the run bar's ▶ IS the preview's
/// transport — it pauses and resumes it and opens no Suggest run; ⏹ hands the button back to the step.
/// This is the part the previous round's tests could not catch, because no paint site ever gave
/// `RunBar::press` a live transport, so S2 (transport beats the page action) could never fire.
fn check_s4_the_run_bar_becomes_the_transport(app: &adw::Application) {
    let window = cut_window(app);
    let cut = three_clips();
    ui::seed_review_cut(&window, &cut);
    // Recording preview, cued: the state a person is in after clicking the timeline, which is exactly
    // when §2 says the button stopped meaning "Suggest".
    ui::set_preview_state(
        &window,
        false,
        false,
        Transport {
            playing: false,
            started: true,
        },
    );
    assert_eq!(
        tab_states::play(Page::Cut, true, ui::preview_player(&window).transport).0,
        tab_states::Play::Transport { resume: true },
        "§2's rule: started but parked, so the transport owns ▶"
    );

    let play = ui::play_button(&window).expect("the run bar has a ▶");
    play.emit_by_name::<()>("clicked", &[]);
    settle();
    let after = ui::preview_player(&window).transport;
    assert!(after.playing, "▶ resumed the parked preview: {after:?}");
    assert!(
        ui::run_bar(&window)
            .expect("the window carries a run bar")
            .borrow()
            .running
            .is_none(),
        "and no Suggest run opened underneath it"
    );

    // Playing: the same press parks it again rather than starting the step.
    play.emit_by_name::<()>("clicked", &[]);
    let parked = ui::preview_player(&window).transport;
    assert!(
        !parked.playing && parked.started,
        "the next press parked it again: {parked:?}"
    );
    assert_eq!(
        ui::play_tooltip(&window).as_deref(),
        Some(run::PLAY_TOOLTIP),
        "the bar's face went back to ▶ while the preview waits"
    );
    assert!(
        ui::run_bar(&window)
            .expect("still the same bar")
            .borrow()
            .running
            .is_none(),
        "a parked preview still owns the button, so still no run"
    );

    // ⏹ ends it, and only then does ▶ mean the page's step again (§2: "then the preview's transport
    // until ⏹"). Read off the player, not the transport ⏹ was handed: the player is what the press wrote.
    ui::stop_button(&window)
        .expect("the run bar has a ⏹")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let stopped = ui::preview_player(&window).transport;
    assert!(!stopped.playing, "⏹ stopped the preview");
    assert!(
        !stopped.started,
        "⏹ ENDED the preview rather than parking it: {stopped:?}"
    );
    // §2's "until ⏹" ends here: a preview nobody started any more is not a transport, so the button
    // belongs to the page's step again, read off the player ⏹ just wrote. The first rewrite's ⏹ kept
    // `started` up, so ▶ stayed the preview's and Suggest was never reachable again.
    assert_eq!(
        tab_states::play(Page::Cut, true, stopped).0,
        tab_states::Play::Step(run::Step::Suggest),
        "after ⏹ the rule says Suggest again"
    );
    assert!(
        ui::run_bar(&window)
            .expect("still the same bar")
            .borrow()
            .running
            .is_none(),
        "and nothing so far has opened a run"
    );
    assert_eq!(
        ui::play_tooltip(&window).as_deref(),
        Some(run::PLAY_TOOLTIP),
        "and the bar paints that answer"
    );
    window.close();
    RAN_S4_TRANSPORT.store(true, Ordering::SeqCst);
}

/// S5 through the real buttons: the three preview modes are one-lit-plus-one-⏸. Pressing ▶✂ while a
/// review runs switches over without stopping, the lit slot moves, and exactly one button wears ⏸.
fn check_s5_exactly_one_preview_mode_is_lit(app: &adw::Application) {
    let root = scratch_session("trio");
    fake_player(&root);
    let keep_cwd = std::env::current_dir().expect("the cwd, to hand back");
    std::env::set_current_dir(&root).expect("cwd pinned to the scratch session");

    let window = cut_window(app);
    let cut = three_clips();
    ui::seed_review_cut(&window, &cut);
    naivepost::ui::cut_review_play::clear();
    ui::set_preview_state(
        &window,
        false,
        false,
        Transport {
            playing: false,
            started: false,
        },
    );
    // Join 0 spans [50,110]: inside it, so ▶✂✂ plays on rather than seeking first.
    ui::set_playhead(&window, 70.0);
    naivepost::cut_play_back::halt();

    // ▶ recording first: the recording slot is lit and the other two say "Play".
    ui::set_preview_state(
        &window,
        false,
        false,
        Transport {
            playing: true,
            started: true,
        },
    );
    crate::suite::sec_11_flow_index_2_tab_states_wire::ui::cut_record_play::paint_button(&window);
    crate::suite::sec_11_flow_index_2_tab_states_wire::ui::cut_cut_play::paint_both_skipping_buttons(&window);
    crate::suite::sec_11_flow_index_2_tab_states_wire::ui::cut_record_play::paint_button(&window);
    crate::suite::sec_11_flow_index_2_tab_states_wire::ui::cut_cut_play::paint_both_skipping_buttons(&window);
    let recording = ui::play_recording_button(&window).expect("the page has ▶");
    let cut_button = ui::play_cut_button(&window).expect("the page has ▶✂");
    let review = ui::review_cuts_button(&window).expect("the page has ▶✂✂");
    assert!(
        recording
            .label()
            .as_deref()
            .unwrap()
            .starts_with('\u{23f8}'),
        "the running recording wears ⏸: {:?}",
        recording.label()
    );
    assert_eq!(
        cut_button.label().as_deref(),
        Some(naivepost::ui::cut_cut_play::PLAY_LABEL),
        "and ▶✂ still says Play the cut"
    );

    // ▶✂✂: the review takes the lit slot.
    review.emit_by_name::<()>("clicked", &[]);
    settle();
    let preview = preview_of(&window);
    assert!(
        cut_screen::review_lit(&preview),
        "the review is the lit mode: {preview:?}"
    );
    assert!(
        !cut_screen::play_cut_lit(&preview),
        "▶✂ is not lit at the same time: {preview:?}"
    );
    assert!(
        tab_states::exactly_one_preview_lit(&preview),
        "§2: one lit — {preview:?}"
    );
    assert!(
        review.label().as_deref().unwrap().starts_with('\u{23f8}'),
        "and ▶✂✂ is the one wearing ⏸: {:?}",
        review.label()
    );

    // ▶✂ switches over WITHOUT stopping (F2.2 S3): the lit slot moves to the plain cut, still exactly
    // one is lit, and ▶✂✂ goes back to its shipped label because it no longer owns the ⏸. The state is
    // set the way `cut_play::pressed`'s ReviewEnded leaves it (`switch_over` clears `reviewing` and
    // keeps `playing`), and painted through the same door the ▶✂ handler paints from; the switch's own
    // arithmetic is pinned in `tests/suite/cut_play_the_cut.rs` and walked on the widget in
    // `tests/suite/cut_play_the_cut_widgets.rs`, which is why it is seeded here rather than pressed.
    naivepost::ui::cut_review_play::clear();
    ui::set_preview_state(
        &window,
        true,
        false,
        Transport {
            playing: true,
            started: true,
        },
    );
    naivepost::ui::cut_cut_play::paint_both_skipping_buttons(&window);
    crate::suite::sec_11_flow_index_2_tab_states_wire::ui::cut_record_play::paint_button(&window);
    let preview = preview_of(&window);
    assert!(
        cut_screen::play_cut_lit(&preview) && !cut_screen::review_lit(&preview),
        "the lit slot moved to ▶✂: {preview:?}"
    );
    assert!(
        ui::preview_player(&window).transport.playing,
        "a switch-over carries the preview on without stopping"
    );
    assert!(
        tab_states::exactly_one_preview_lit(&preview),
        "and exactly one is lit through the switch: {preview:?}"
    );
    assert_eq!(
        cut_button.label().as_deref(),
        Some(naivepost::ui::cut_cut_play::PAUSE_LABEL),
        "so ▶✂ is the button that wears ⏸ now"
    );
    assert_eq!(
        review.label().as_deref(),
        Some(naivepost::ui::cut_review_play::PLAY_LABEL),
        "and ▶✂✂ gave the ⏸ face back because it no longer owns the review"
    );
    assert_eq!(
        recording.label().as_deref(),
        Some(naivepost::ui::cut_record_play::PLAY_LABEL),
        "while ▶ says Play the recording again"
    );

    // The program's own door agrees with the state the widgets were painted from.
    let [recording_lit, cut_lit, review_lit] = tab_states::preview_lits(&preview);
    assert_eq!(
        [recording_lit, cut_lit, review_lit],
        [false, true, false],
        "preview_lits is the one decision the three buttons read"
    );

    naivepost::cut_play_back::halt();
    std::env::set_current_dir(keep_cwd).expect("the cwd handed back");
    window.close();
    RAN_S5_TRIO.store(true, Ordering::SeqCst);
}

/// S4's other half on the two tabs that open anyway: with no cut, ▶ says `cut::NO_CUT_YET` — the
/// refusal §2 puts on the button rather than on the tab. (The same sentence is pinned as a rule in
/// `tests/suite/tab_states.rs` and by `tests/suite/produce_run_widgets.rs` (1) for Produce; what is new here is
/// that the §2 switch in `wire_play` is what says it, on Narrate, through the real ▶.)
fn check_s4_no_cut_refuses_on_narrate(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    settle();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_clicked();
    settle();
    assert_eq!(
        ui::state(&window).page,
        Page::Narrate,
        "Narrate opens always"
    );
    // No cut in this window's session: seed an empty one so "without a cut" is a fact, not a default.
    ui::seed_review_cut(&window, &Cut::default());
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        common::status_text(&window),
        cut::NO_CUT_YET,
        "the run bar's ▶ refuses without a cut, in §3's own sentence"
    );
    assert!(
        ui::run_bar(&window)
            .expect("the window carries a run bar")
            .borrow()
            .running
            .is_none(),
        "a refused press opens no run, so no ⏸ shows for work that never began"
    );
    window.close();
    RAN_S4_REFUSAL.store(true, Ordering::SeqCst);
}

/// The three checks share one application and one activate pass, in this order, because each builds its
/// own window and the newest window is the one every `ui::` accessor reads — exactly how
/// `tests/suite/cut_review_cuts_widgets.rs` sequences its windows inside one round. A check closes its window
/// before the next builds one.
fn window_round() {
    common::one_app_round(|app| {
        check_s4_the_run_bar_becomes_the_transport(app);
        check_s5_exactly_one_preview_mode_is_lit(app);
        check_s4_no_cut_refuses_on_narrate(app);
    });
}

#[test]
fn sec_11_flow_index_2_tab_states_s4_the_run_bar_becomes_the_preview_transport() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_S4_TRANSPORT.load(Ordering::SeqCst),
        "the S4 transport check never ran"
    );
    assert!(
        RAN_S5_TRIO.load(Ordering::SeqCst),
        "the S5 trio check never ran"
    );
    assert!(
        RAN_S4_REFUSAL.load(Ordering::SeqCst),
        "the Narrate no-cut refusal check never ran"
    );
}
