//! The Narrate page's own pictures: `07-narrate` and `07-take`, staged through the page's publish door
//! (`set_state` + `refresh`) -- the door every flow calls after it edits the narration -- so the rows are
//! drawn by the page rather than painted here, plus the `narrate/` files that stop a later tab's `build()`
//! from resetting the shot. Split out of `snapshot.rs`, which is past the size another block fits in.

use std::path::Path;

use crate::cut;
use crate::layout;
use crate::ui;

/// The lane's envelope, staged so a shot of the band draws a wave without asking for ffmpeg.
///
/// The app's own walk is decode → fold → read (`decode_audio::ensure` → `wave_build::ensure` → `wave::read`),
/// and the first leg runs the CONFIGURED ffmpeg — this machine's `FFMPEG` setting may name a path that is not
/// there (the shipped `llm.conf` says `/opt/ffmpeg/ffmpeg`, the container has `/usr/bin/ffmpeg`), in which
/// case the decode refuses and §1's "reading the recording…" stands where the wave should be. So the bench
/// writes the two files the program's own writers would have produced — the 16 kHz decode marker and the wave
/// cache folded from it by [`crate::wave_build::ensure`] — and the band then reads its envelope through
/// `wave::read` exactly as it does live. Nothing here is painted fake; only the expensive leg is pre-made,
/// which is the same thing every other snapshot arm does when it stages a file instead of encoding one.
///
/// The decode is written at [`crate::transcribe::SAMPLE_RATE`] because that is the rate the fold assumes:
/// `wave_build::SAMPLES_PER_BUCKET` is `SAMPLE_RATE / PEAK_HZ`, exact at 80 samples per bucket.
pub(crate) fn stage_envelope(tree: &crate::layout::Tree, source: &std::path::Path) -> bool {
    if !source.is_file() {
        return false;
    }
    let lane = format!("voice:{}", crate::narrate_data::take_base(source));
    let decoded = tree.voice16k_wav(&lane);
    let seconds = crate::ui::narrate_band::recording_seconds().unwrap_or(45.0).round().max(1.0);
    let rate = crate::transcribe::SAMPLE_RATE;
    if crate::snapshot::write_silence_wav(&decoded, rate as u32, (seconds * rate as f64) as u32).is_err() {
        return false;
    }
    crate::wave_build::ensure(tree, &lane, source, &mut |_| {}).is_ok()
}

/// Stage the envelope for every file the shot's Narrate band may draw across — the built reference and the
/// narrator-1 recording, in the order [`crate::ui::narrate_band`] tries them — and say what it managed. The
/// app would build each through the CONFIGURED ffmpeg, which this machine may point at a path that is not
/// there; staging instead lets the band read a genuine cache (see [`stage_envelope`]).
pub(crate) fn stage_seeded_envelopes(tree: &crate::layout::Tree) -> bool {
    let mut staged = vec![stage_envelope(tree, &crate::narrate_data::base_reference(tree))];
    if let Ok(project) = crate::project::load(tree.dir()) {
        if let Some(recording) = crate::narrate_band_face::recording_path(tree, &project, "narrator1") {
            staged.push(stage_envelope(tree, &recording));
        }
    }
    let staged = staged.iter().any(|ok| *ok);
    println!("snapshot: band envelope staged = {staged}");
    staged
}

/// Stage the shot's narration or take band onto `window`'s Narrate page, through the doors its widgets
/// fire. Returns true when this module owned `screen`.
pub(crate) fn stage(window: &adw::ApplicationWindow, dir: &Path, screen: &str) -> bool {
    // §07-narrate's own picture (`spec/img/07-narrate.png`) is FOUR written lines with the app's own
    // times, estimates and one red fit warning. The fixture cannot show them: `fixtures/demo.naivepost`
    // carries no `narrate/narration.json`, so a bare shot of the page draws an empty lines column.
    // Seeded here the way `06-lane` seeds its bars -- the shared fixture file untouched -- through the
    // page's own publish door (`set_state` + `refresh`), which is what every flow calls after it edits
    // the narration; the rows are then drawn by `draw_rows` from these entries, not painted here.
    if screen == "07-narrate" {
        let mut state = ui::NarrateState::default();
        // Four clips, one line each, laid out on the spec image's seconds. `s`/`e` are the clip bounds
        // copied from the cut (§4), `at` the placement inside the clip, so the row's time reads
        // `s + at` as `time_field` spells it: 3.5, 43.6, 102.2 and 439.1.
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
        // The clips these lines hang on, copied from the entries' own bounds: `shell::refit`, which
        // runs when the page is entered, orphans a line whose clip no longer exists (and an orphaned
        // line reads as one the cut threw away). A snapshot that wants four rows therefore has to show
        // four kept clips, which is what §1's image shows too.
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
        // publish them. `shell::refit` is what a live page gets these from; a seeded shot has to state
        // them, exactly as it states the entries and the segs above.
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
        // F4.1's bar face is part of this screen now: the run has written its clips, so the progress
        // bar reads `writing N/M clips` rather than sitting idle. Driven through the same seam the
        // run paints from (not a poke), with the reply empty because headless there is no model --
        // only the stage text and fraction are wanted here, and the door's own status line is not.
        ui::show_narrate_stage(window, 2, 4);
        return true;
    }

    // §08-produce#1-screen's own picture (`spec/img/08-produce.png`) is a page with work on it:
    // images in the row, an instruction written, settings chosen and both readouts filled. The
    // fixture cannot show that -- `fixtures/demo.naivepost` carries no `publish.json`, so a bare
    // shot draws the empty-images note and two blank readouts. Seeded here the way `07-narrate`
    // seeds its lines: through the page's own publish door (`set_state` + `refresh`), which is what
    // every flow calls after it edits the publish state, so the slots, the rows and the readouts
    // are drawn by the page from this state rather than painted here.
    // The rows above are published through `set_state`, a process-wide copy, and the shell builds the
    // Narrate page's box once PER TAB, so any later `build()` seeds itself from what the project's files
    // hold. For a fixture with none, that answer is "nothing on disk", which would reset the published
    // voice, shift and take list back to their defaults. Writing the two files here is what §07-narrate#3-data
    // says a real project has; the renderer is only saying the shot's project did.
    // §F4.6's own picture (`spec/img/07-take.png`) is the voice column with ONE hand-picked take on
    // the band. §F4.6's caption spells what the shot has to show — "a 2.6 s take" and the status
    // "take added: 0:00–0:03 — 1 take(s), 2.6 s of reference (14 s is plenty)" — so this arm seeds a
    // 2.6 s take and says the sentence through the same door the ＋ press says it from
    // (`press_take_add`, which adds the selection to the state, repaints and writes the status). The
    // band, the voice and its wav come from `seed_voice_reference`, which ran before the window built.
    if screen == "07-take" {
        // The CUT this shot needs is the demo project's own three clips, not an empty one: the arm runs
        // after `build_window`'s seed, and `refresh` re-reads the state on every repaint, so the arm's
        // `has_cut: true` promised a cut the page never had. With no clips the right column showed the
        // "no narration lines yet — ＋ puts one at the playhead" hint and the Inputs readout read
        // "0 clip(s) · 00:00", and the take band's own picture had nothing beside it. `cut::load` is
        // the same door `05-cut` goes through, so the footage, the clock and the readout all agree with
        // the project the shot was asked to render.
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
        // would vanish before the pixels. So the seed writes the file and publishes the same span.
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
        // of the way across the plate. Re-fit now that the band knows its seconds.
        crate::ui::narrate_band::note_lane_size(window);
        // 0.0–2.6 s: 2.6 s of reference, the spec image's own take, said through the same door the
        // ＋ press says it from — which now spells §F4.6's own sentence ("take added: 0:00–0:03 — 1
        // take(s), 2.6 s of reference (14 s is plenty)"): the span of the seconds just dragged, then
        // §6's commit status judged against `P.machine.refWantSeconds`.
        // `mm_ss` floors, so the span this quotes for these seconds reads "00:00–00:02"; a take
        // placed at 1.0–3.6 would quote "00:01–00:03" and the shot's status would not line up with
        // its own caption's seconds.
        ui::press_take_add(window, (0.0, 2.6));
        // §F4.6 S4 is right and the shot has to live with it: making a take removes the built reference, and
        // `snapshot.rs::seed_voice_reference` ran BEFORE this arm. Re-seed after the press so the lane the
        // picture is about has its recording under it — §A.7 asks whether the VOICE has a recording, which
        // an invalidation does not change, and the band's own visibility rule now agrees.
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
    // reset the shot to "own", 0.0 and an empty band. So the renderer writes the files the shot's project
    // is supposed to have left behind — the same three the spec lists — rather than only pretending to.
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
