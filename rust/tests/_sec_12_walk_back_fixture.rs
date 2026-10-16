#![allow(dead_code)]

//! The session folder §12 §4's live walk-back checks dial, shared by the two one-scenario-per-binary files
//! (`sec_12_decisions_4_walk_back_live.rs` and `sec_12_decisions_4_walk_back_live_b.rs`) because libadwaita
//! initialises only inside the one `run_with_args` a process gets.
//!
//! What makes it a walk-back fixture rather than any other words-mode session: the two scenes are split by a
//! silence short enough to be a seam and long enough for the envelope to report, so the trimmed runs come out
//! with a spoken hole between them that `words_cut` leaves open and `close_holes` shuts. Every number below is
//! chosen against a named constant in `src/suggest.rs`, restated in the comments so a later change to one of
//! those constants fails this fixture loudly instead of quietly making it prove nothing.

use naivepost::layout::Tree;
use naivepost::project::Project;
use naivepost::wave::Wave;

use crate::press_fixture::Fixture;

/// Peak rate of the envelope this fixture writes, and the bucket length it follows. `wave::encode` keeps whole
/// hertz, so 5 Hz means every peak covers 0.2 s and the silence edges below land on exact buckets.
const HZ: f64 = 5.0;
/// A byte under `SILENCE_THRESHOLD_DB` (`dbfs(0)` is minus infinity), i.e. quiet enough to cut in.
const QUIET: u8 = 0;
/// A byte that is sound by a wide margin, so rounding cannot make it quiet.
const LOUD: u8 = 200;
/// Where the second scene starts, on both clocks: the recording has no hand shift and no offset, so
/// `timeline::session_at(local, seg.s, 0, 0)` is just `seg.s + local` and a file second IS a session second.
pub const SCENE_GAP_START: f64 = 6.0;
/// How long the quiet between the two scenes runs, and therefore how wide the seam between the trimmed runs is.
/// Two bounds decide it: over `cut_wave_edges::SILENCE_MIN_SECONDS` (0.4 s) so the envelope reports it as a
/// place to cut rather than as the gap between two words, and under `suggest::SEAM_MAX_SECONDS` (1.5 s) so the
/// walk is allowed to close it. It also stays under `suggest::DEAD_AIR_MAX_SECONDS` (8.0 s), which is what
/// keeps `remove_dead_air` from taking the silence out before the walk ever sees it.
pub const GAP_SECONDS: f64 = 1.0;
/// The last word of the FIRST scene, in that tape's own seconds. `runs_of` keeps only words inside the run it
/// is reading (`t < span.1`), so this sits just before the scene stops at 5.0 — and with
/// `suggest::TRIM_REACH_SECONDS` (0.4 s) reaching past each end, the trimmed first run stops at 4.9.
pub const FIRST_LAST: f64 = 4.5;
/// The first word of the SECOND scene, in that tape's own seconds. `runs_of` moves every word onto the session
/// clock with `session_at(local, seg.s, 0, 0)` = `seg.s + local`, so this lands at 6.1, which is 1.2 s after
/// the first trimmed run ends: under `SEAM_MAX_SECONDS` (1.5 s), so `close_holes` may join the two, and over
/// the envelope's silence floor (0.4 s), so the hole is a pause the speaker made rather than the gap between
/// two words.
pub fn second_first_local() -> f64 {
    0.1
}
/// The first word of the first group.
pub fn first_word() -> f64 {
    1.0
}

/// The page's cut for both live checks: TWO scenes with a real gap between them, because
/// [`timeline::filmed_runs`] merges touching spans — adjacent scenes are one filmed stretch and therefore one
/// spoken run, and `close_holes` only has something to close when two trimmed runs are separated on the
/// session clock. Both scenes read the same `cam0` word list, each on its own tape seconds.
pub fn base_cut() -> naivepost::cut::Cut {
    naivepost::cut::Cut {
        segs: vec![
            naivepost::cut::Seg { s: 0.0, e: SCENE_GAP_START - GAP_SECONDS, cam: 0, ..Default::default() },
            naivepost::cut::Seg { s: SCENE_GAP_START, e: 30.0, cam: 0, ..Default::default() },
        ],
        ..Default::default()
    }
}

/// A session folder with everything the words door reads: `cam0` words for the trim (two stretches, one per
/// scene, split by the same gap the envelope calls silent), one mark outside both trimmed spans so the walk
/// has a materialisation (§1's fourth row demands one) without cutting the hole back open, and a wave cache
/// whose envelope measures that silence.
pub fn words_fixture(tag: &str) -> Fixture {
    let f = Fixture::fresh(tag)
        .with_timeline()
        .with_words(&[
            ("hello", first_word()),
            ("there", FIRST_LAST),
            // The second scene reads the SAME tape seconds, moved onto the session clock by its own `seg.s`
            // (`timeline::session_at`), so these are file seconds: `second_first_local` → 6.1 and 3.0 → 9.0.
            ("half", second_first_local()),
            ("end", 3.0),
        ])
        .with_marks(&[crate::press_fixture::retake(25.0, 26.0)]);
    write_envelope(&f.tree, &f.project);
    f
}

/// Write the lane's envelope through the program's own writer, stamped against the recording it describes:
/// talk, silence, talk. `read_lanes`/`silences_of` read exactly this file, so the silence is measured off a
/// cached wave rather than handed to the code under test. The lane sits at session second 0 —
/// `fix_transcripts::placement` answers 0 for a single unstamped source — so the buckets below are session
/// seconds directly.
fn write_envelope(tree: &Tree, project: &Project) {
    let name = "lecture.mkv";
    std::fs::create_dir_all(tree.sources_dir()).expect("sources folder");
    let path = tree.sources_dir().join(name);
    std::fs::write(&path, [7u8; 4_096]).expect("recording file");
    let meta = std::fs::metadata(&path).expect("recording stats");
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut peaks = vec![LOUD; ((SCENE_GAP_START - GAP_SECONDS) * HZ) as usize];
    peaks.extend(vec![QUIET; (GAP_SECONDS * HZ) as usize]);
    // Loud again for the rest of the lane, which is where the second group of words sits.
    peaks.extend(vec![LOUD; ((30.0 - SCENE_GAP_START) * HZ) as usize]);
    let wave = Wave { hz: HZ, chans: vec![peaks] };
    naivepost::wave::write(tree, "lecture", &wave, meta.len() as i64, mtime)
        .expect("write the wave cache");
    // The project's footage row must name this recording, or `read_lanes` has no lane to read at all.
    let mut next = project.clone();
    next.sources = vec![naivepost::project::Source {
        path: format!("project:sources/{name}"),
        footage: true,
        ..Default::default()
    }];
    let json = serde_json::to_string_pretty(&next).expect("project serialises");
    std::fs::write(tree.dir().join(naivepost::project::PROJECT_FILE), json)
        .expect("rewrite naivepost.json with the footage row");
}
