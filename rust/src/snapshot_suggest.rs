//! The `05-suggest` screen's one network-free press: §F2.14 S2b, landed through the walk-back with
//! literal seconds — no model asked, no session files staged.

use crate::cut_effect_decisions::Candidate;
use crate::suggest;

pub fn stage() -> Option<crate::layout::Tree> {
    let dir = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    let _ = std::fs::remove_dir_all(&dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    crate::layout::Tree::new(dir).ok()
}

/// Press ▶ in the MODEL mode through the window's own door and answer what the status row shows.
pub fn press_through_page(window: &adw::ApplicationWindow) -> String {
    let landed = crate::ui::cut_suggest_model_run::press();
    crate::ui::window::set_status_line(window, &landed.said);
    landed.said
}

/// Land a suggestion through the page's own doors and answer the sentence the status row shows.
pub fn press_landed(window: &adw::ApplicationWindow) -> String {
    let words: Vec<f64> = vec![3.0, 17.0, 24.6, 38.4, 45.6, 57.4];
    let walk = suggest::Walk {
        talked: vec![(2.6, 17.4), (24.2, 38.8), (45.2, 57.8)],
        silences: Vec::new(),
        snap_points: words
            .iter()
            .copied()
            .map(|at| (at, Candidate::WordEdge))
            .collect(),
        marks: vec![(36.0, 37.0)],
    };
    // What a model would have stamped: seconds copied off the request's timeline, each one between two
    // words, which is exactly what §3.6 tells it not to do and what the walk-back exists to undo.
    let answer = suggest::apply(&[(4.0, 18.0), (24.0, 38.0), (45.0, 58.0)], &[], &[], &walk);
    crate::ui::cut_suggest_land::land_spans(window, &answer.segs);
    crate::ui::cut_suggest_land::land_effects(window, &answer);
    answer
        .log
        .first()
        .map(|line| line.trim_start_matches(">>> ").to_string())
        .unwrap_or_default()
}
