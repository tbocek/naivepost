//! F2.1 S1 / F2.2 S5 — only ▶✂ silences the sound past a kept stretch's end; ▶ plays everything, cuts and all, also
//! when it takes over from ▶✂ without a new cue. First rewrite: after a ▶✂ the dropped stretches played silent in ▶.
//! The window's own ▶ tick is what clears it, so the check runs the real window.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::play_built_in::silenced_from;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;

static RAN: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("fixtures");
    dir.push("demo.naivepost");
    dir
}

/// Let the window's loops run for `ms`.
fn run_for(ms: u64) {
    let context = glib::MainContext::default();
    let until = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn transport(playing: bool) -> naivepost::run::Transport {
    naivepost::run::Transport {
        playing,
        started: playing,
    }
}

fn check(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    common::size_to_minimum(&window);
    ui::tab_button(&window, Page::Cut)
        .expect("a Cut tab")
        .emit_clicked();
    common::wait_for_stable_layout(&window);

    // ▶✂ is the preview and a stretch's end is silenced in the stream (set on the player directly: this window plays
    // no file to put a session second on). Paused, so no loop moves it: it holds while ▶✂ is the preview.
    ui::set_preview_state(true, false, transport(false));
    naivepost::play_built_in::silence_from(Some(5.0));
    run_for(250);
    assert_eq!(
        silenced_from(),
        Some(5.0),
        "while ▶✂ is the preview, the stretch's end holds"
    );

    // Pressing ▶ clears it in the same moment, before ▶ plays a sample: no tick has run when this is read.
    let runs = [(0.0, 60.0)];
    let _ = ui::press_play_recording(&window, &runs);
    assert_eq!(
        silenced_from(),
        None,
        "▶ plays the dropped stretches from its first sample"
    );

    // And when ▶ takes over without a press (mid-play, no new cue), its own tick clears it.
    ui::set_preview_state(true, false, transport(false));
    naivepost::play_built_in::silence_from(Some(5.0));
    ui::set_preview_state(false, false, transport(true));
    run_for(250);
    assert_eq!(silenced_from(), None, "▶ plays the dropped stretches too");
    window.close();
}

#[test]
fn f2_1_s1_play_the_recording_is_never_silenced_by_a_cut_stretch() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        check(app);
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the check never ran");
}
