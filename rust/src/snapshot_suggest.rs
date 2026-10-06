//! The `05-suggest` screen's one network-free press: §F2.14 S2b, landed through the walk-back with
//! literal seconds — no model asked, no session files staged.
//!
//! `snapshot.rs` is at its size budget and this needs a `Walk` built out of four fields, so it lives
//! here, beside `snapshot_insert.rs`, which exists for the same reason. Nothing in this file decides a
//! rule: the seconds below are only what a model would have typed and what the aligner would have
//! answered; every edge that reaches the cut is moved by [`crate::suggest::apply`], and the landing goes
//! through the page's own doors, so the shot shows §S3's arithmetic rather than restating it.

use crate::cut_effect_decisions::Candidate;
use crate::suggest;

/// Lay the session this shot needs where a real launch would find it: the working copy beside the
/// working directory, `session.naivepost`, which is exactly what [`crate::ui::cut_suggest_land`] resolves
/// when nothing has pointed it elsewhere. The folder is wiped and made fresh because a screenshot must not
/// inherit a previous run's `cut/cut.json` — that file is what `land_spans` writes, and the shot shows
/// what THIS press landed.
pub fn stage() -> Option<crate::layout::Tree> {
    let dir = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    let _ = std::fs::remove_dir_all(&dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    crate::layout::Tree::new(dir).ok()
}

/// Land a suggestion through the page's own doors and answer the sentence the status row shows.
/// `land_spans` is what `press_model` uses: recorder first, base second, `cut.json` third, so the shot
/// shows a cut that one ↶ takes back whole.
///
/// The walk is literals because the shot's subject IS §S3's arithmetic: staging a real transcript and a
/// real wave cache to reach the same function would make a picture depend on files it has no claim on,
/// and every rule they exercise is already pinned in `tests/f2_14_*`. What matters about the numbers is
/// their shape, not their values: three scenes the model stamped mid-word (so `snap_edge` moves an edge),
/// gaps wider than `suggest::SEAM_MAX_SECONDS` (so §S3 does NOT close them and three bands stay on the
/// lane), word edges within `cutpass::SNAP_TOLERANCE_SECONDS` 5.0 of a stamp, and one retake mark inside
/// the middle scene, which splits it and so proves the marks went through.
pub fn press_landed(window: &adw::ApplicationWindow) -> String {
    let words: Vec<f64> = vec![3.0, 17.0, 24.6, 38.4, 45.6, 57.4];
    let walk = suggest::Walk {
        talked: vec![(2.6, 17.4), (24.2, 38.8), (45.2, 57.8)],
        silences: Vec::new(),
        snap_points: words.iter().copied().map(|at| (at, Candidate::WordEdge)).collect(),
        marks: vec![(36.0, 37.0)],
    };
    // What a model would have stamped: seconds copied off the request's timeline, each one between two
    // words, which is exactly what §3.6 tells it not to do and what the walk-back exists to undo.
    let answer = suggest::apply(&[(4.0, 18.0), (24.0, 38.0), (45.0, 58.0)], &[], &[], &walk);
    crate::ui::cut_suggest_land::land_spans(window, &answer.segs);
    crate::ui::cut_suggest_land::land_effects(window, &answer);
    // The status row's short sentence, the way `Landed::landed_log` spells it (that one is private, and
    // copying one `trim_start_matches` is cheaper than widening a type for a screenshot).
    answer
        .log
        .first()
        .map(|line| line.trim_start_matches(">>> ").to_string())
        .unwrap_or_default()
}
