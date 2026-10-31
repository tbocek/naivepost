//! The Narrate page's own pictures: `07-narrate` and `07-take`, staged through the page's publish door
//! (`set_state` + `refresh`) -- the door every flow calls after it edits the narration -- so the rows are
//! drawn by the page rather than painted here, plus the `narrate/` files that stop a later tab's `build()`
//! from resetting the shot.

use std::path::Path;

use crate::cut;
use crate::layout;
use crate::ui;

/// The lane's envelope, staged so a shot of the band draws a wave without asking for ffmpeg.
pub(crate) fn stage_envelope(tree: &crate::layout::Tree, source: &std::path::Path) -> bool {
    if !source.is_file() {
        return false;
    }
    let lane = format!("voice:{}", crate::narrate_data::take_base(source));
    let decoded = tree.voice16k_wav(&lane);
    let seconds = crate::ui::narrate_band::recording_seconds()
        .unwrap_or(45.0)
        .round()
        .max(1.0);
    let rate = crate::transcribe::SAMPLE_RATE;
    if crate::snapshot::write_silence_wav(&decoded, rate as u32, (seconds * rate as f64) as u32)
        .is_err()
    {
        return false;
    }
    crate::wave_build::ensure(tree, &lane, source, &mut |_| {}).is_ok()
}

/// Stage the envelope for every file the shot's Narrate band may draw across — the built reference and the
/// narrator-1 recording, in the order [`crate::ui::narrate_band`] tries them — and say what it managed.
pub(crate) fn stage_seeded_envelopes(tree: &crate::layout::Tree) -> bool {
    let mut staged = vec![stage_envelope(
        tree,
        &crate::narrate_data::base_reference(tree),
    )];
    if let Ok(project) = crate::project::load(tree.dir()) {
        if let Some(recording) =
            crate::narrate_band_face::recording_path(tree, &project, "narrator1")
        {
            staged.push(stage_envelope(tree, &recording));
        }
    }
    let staged = staged.iter().any(|ok| *ok);
    println!("snapshot: band envelope staged = {staged}");
    staged
}

/// Stage the shot's narration or take band onto `window`'s Narrate page, through the doors its widgets
/// fire.
pub(crate) fn stage(window: &adw::ApplicationWindow, dir: &Path, screen: &str) -> bool {
    // §07-narrate's own picture (`spec/img/07-narrate.png`) is FOUR written lines with the app's own
    // times, estimates and one red fit warning.
    if screen == "07-narrate" {
        let mut state = ui::NarrateState::default();
        // Four clips, one line each, laid out on the spec image's seconds.
        state.entries = vec![
            crate::narration::Entry {
                s: 3.0,
                e: 28.0,                    at: 0.5,
                text: "Welcome back: lecture two of the blockchain series is all about Ethereum."
                    .into(),
                emotion: "calm".into(),
                // `pos` stays empty on purpose: a non-empty one makes `status` answer `Caption`, which
                // prints the caption sentence and hides the estimated end time §1's rows show.
                ..Default::default()
            },
            crate::narration::Entry {
                s: 30.0,
                e: 70.0,
                at: 13.6,
                text: "First a quick look back at Bitcoin, and at what a 51 percent attack really costs.".into(),
                emotion: "happy=0.6, calm=0.4".into(),
                ..Default::default()
            },
            crate::narration::Entry {
                s: 90.0,
                e: 140.0,
                at: 12.2,
                text: "Then the Ethereum basics: gas, smart contracts, accounts and the architecture behind them.".into(),
                emotion: "curious".into(),
                ..Default::default()
            },
            // Long enough at `SPEECH_CHARS_PER_SECOND` to run past its clip, so the last row wears the
            // red mark and prints `fit_warning`'s overrun sentence -- the thing §1's item 21 shows.
            crate::narration::Entry {
                s: 435.0,
                e: 445.0,
                at: 4.1,
                text: "And right at the end of this clip, a long line that will not fit: the page warns that it runs past the clip before anything is spoken, so it can be rewritten shorter.".into(),
                emotion: "excited".into(),
                ..Default::default()
            },
        ];
        state.length = state
            .entries
            .iter()
            .map(|entry| entry.e - entry.s)
            .sum::<f64>();
        state.segs = state
            .entries
            .iter()
            .map(|entry| cut::Seg {
                s: entry.s,
                e: entry.e,
                cam: 0,
                ..Default::default()
            })
            .collect();
        state.voice = "narrator1".into();
        state.takes = vec![(1.0, 6.0)];
        state.pitch = 0.0;
        state.clips = 4;
        // §1's shot shows what ▶ would still have to fix, and the row's own red warning was drawn in
        // the shot while the Inputs row stayed bare -- the two read the same counts, so the shot must
        // publish them.
        state.unwritten = 1;
        state.off_cut = 1;
        state.has_cut = true;
        state.has_timeline = true;
        state.narrators = 1;
        // The clock reads where the shot's line sits, so the two faces agree with the rows below them.
        state.session = 40.0;
        state.cut_at = 33.0;
        ui::set_state(state);
        ui::refresh(window);
        // The take this shot shows is written to `narrate/takes.json` as well as published, because
        // `refresh` re-seeds the page's take list from that file on every repaint — a state-only take
        // vanishes before the pixels (that is why this lane once drew no take at all).
        if crate::ui::window::narrate_session_tree().is_some() {
            // §1's badge **10** is the take band, and its visibility rule (§A.7) is "a narrator slot WITH a
            // recording", asked of the PROJECT.
            crate::ui::window_line_step::push_session_tree(layout::Tree::new(dir).ok());
            let tree =
                layout::Tree::new(dir).expect("the asked project is a folder ending .naivepost");
            let band = crate::snapshot::seed_voice_reference(&tree);
            println!("snapshot: 07-narrate take band re-seeded = {band}");
            if let Some(key) = crate::ui::narrate_seed::take_recording_key(&tree) {
                let _ = crate::narrate_data::save_takes(
                    &tree,
                    &key,
                    &[crate::narrate_data::Take { s: 1.0, e: 6.0 }],
                );
            }
        }
        // §1's picture is a page whose narration has been WRITTEN, so the run bar must not be mid-flight:
        crate::ui::window::set_status_line(
            window,
            &crate::narrate_run::Outcome::Spoken {
                written: 4,
                spoken: 4,
            }
            .status(),
        );
        crate::ui::run_close::close_last_narrate_run();
        ui::refresh(window);
        return true;
    }

    // §08-produce#1-screen's own picture (`spec/img/08-produce.png`) is a page with work on it:
    if screen == "07-take" {
        // The CUT this shot needs is the demo project's own three clips, not an empty one: the arm runs
        // after `build_window`'s seed, and `refresh` re-reads the state on every repaint, so the arm's
        // `has_cut: true` promised a cut the page never had.
        let clips = layout::Tree::new(dir)
            .ok()
            .and_then(|tree| cut::load(&tree).ok())
            .map(|loaded| {
                ui::seed_review_cut(window, &loaded);
                loaded.segs.len()
            })
            .unwrap_or(0);
        let footage = layout::Tree::new(dir)
            .ok()
            .and_then(|tree| cut::load(&tree).ok())
            .map(|loaded| loaded.segs.iter().map(|seg| seg.e - seg.s).sum())
            .unwrap_or(0.0);
        // The 2.6 s take this shot is captioned with is ALSO written to `narrate/takes.json`, because
        // `refresh` re-seeds the page's take list from that file on every repaint — a state-only take
        // would vanish before the pixels.
        let seeded = crate::ui::window::narrate_session_tree()
            .as_ref()
            .and_then(crate::ui::narrate_seed::take_recording_key)
            .and_then(|key| {
                let tree = crate::ui::window::narrate_session_tree()?;
                crate::narrate_data::save_takes(
                    &tree,
                    &key,
                    &[crate::narrate_data::Take { s: 0.0, e: 2.6 }],
                )
                .ok()
            })
            .is_some();
        let state = ui::NarrateState {
            voice: "narrator1".into(),
            pitch: 0.0,
            narrators: 1,
            has_cut: clips > 0,
            has_timeline: true,
            clips,
            length: footage,
            takes: if seeded { vec![(0.0, 2.6)] } else { vec![] },
            ..Default::default()
        };
        ui::set_state(state);
        ui::refresh(window);
        // The lane fitted itself against a 0-second view (the scroller was still hidden then, and an
        // unmapped lane reports 0 px wide), so it asked for its 240 px minimum and the wave stopped a fifth
        // of the way across the plate.
        crate::ui::narrate_band::note_lane_size(window);
        // 0.0–2.6 s: 2.6 s of reference, the spec image's own take, said through the same door the
        // ＋ press says it from — which now spells §F4.6's own sentence ("take added: 0:00–0:03 — 1
        // take(s), 2.6 s of reference (14 s is plenty)"): the span of the seconds just dragged, then
        // §6's commit status judged against `P.machine.refWantSeconds`.
        ui::press_take_add(window, (0.0, 2.6));
        // §F4.6 S4 is right and the shot has to live with it: making a take removes the built reference, and
        // `snapshot.rs::seed_voice_reference` ran BEFORE this arm.
        if let Some(tree) = crate::ui::window::narrate_session_tree() {
            crate::snapshot::seed_voice_reference(&tree);
        }
        ui::refresh(window);
        return true;
    }
    false
}

/// Write the `narrate/` files the shot's project is supposed to have left behind: the shell builds the
/// Narrate box once PER TAB, so a `build()` later than the stage above re-reads them, and a fixture with
/// none would reset the published voice, shift and take list back to their defaults.
pub(crate) fn seed_project_files(screen: &str) {
    if screen != "07-narrate" {
        return;
    }
    // The page seeds its voice, shift and takes from the project's own `narrate/` files (§07-narrate#3-data,
    // `narrate_page::build`), and that seed runs AFTER the publish below: the shell builds the Narrate box
    // once PER TAB, so a `build()` later than this one re-reads the files and a fixture with none would
    // reset the shot to "own", 0.0 and an empty band.
    if let Some(tree) = crate::ui::window::narrate_session_tree() {
        let _ = crate::narrate_data::write_voice(&tree, "narrator1");
        if let Some(recording) = crate::ui::narrate_seed::take_recording_key(&tree) {
            let _ = crate::narrate_data::save_takes(
                &tree,
                &recording,
                &[crate::narrate_data::Take { s: 1.0, e: 6.0 }],
            );
        }
    }
}
