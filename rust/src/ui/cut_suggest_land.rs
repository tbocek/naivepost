//! §05-cut#2 — "base = the last suggestion or what the page opened with" (`spec/05-cut.md` §2).

use glib::object::Cast;

use crate::cut::{self, Cut, Seg};
use crate::suggest::Applied;
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

pub(crate) type Recorder = fn(&adw::ApplicationWindow, &Cut);

/// §F2.14 S2a/S2b: which half of Suggest this press runs — see [`suggest_press`], which does both
/// the refusal and the mode.
#[derive(Debug, Clone, PartialEq)]
pub struct SuggestAnswer {
    pub said: String,
    /// Whether the press was REFUSED (§S1), which is what makes the caller log it.
    pub refused: bool,
    pub stops: bool,
}

/// §F2.14 S1 then S2: refuse, or build. Lives here, not in `ui::window`, because `window.rs` is at
/// its size budget and this is the suggest flow's decision, not the shell's.
pub fn suggest_press(
    window: &adw::ApplicationWindow,
    policy_: &crate::project::Policy,
    busy: bool,
) -> SuggestAnswer {
    use crate::project::CutMode;
    if let Some(reason) = refuse_suggest_run(busy) {
        return SuggestAnswer {
            said: reason,
            refused: true,
            stops: true,
        };
    }
    let said = match crate::policy::cut_mode_of(&policy_) {
        CutMode::Words => press_words(window),
        // §S2b: the model is asked, the answer is walked back, and only then does anything reach
        // the page.
        CutMode::Model => press_model(window),
    };
    SuggestAnswer {
        said,
        refused: false,
        stops: true,
    }
}

/// The window's own ▶ press for the words mode: resolve the session the refusal already accepted,
/// then hand the whole thing to §S2a.
fn press_words(window: &adw::ApplicationWindow) -> String {
    let made = crate::cut_suggest_words_run::press_words(window);
    // §S2a splits the answer: the log takes the long line (written inside `press_words`), the status
    // line takes `cut by the words: N segments`. An empty cut landed nothing, so it says nothing.
    if !made.is_empty() {
        if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
            status.set_text(&made);
        }
    }
    made
}

/// The window's own ▶ press for the model mode (§S2b): ask, walk back, land, say what landed.
fn press_model(window: &adw::ApplicationWindow) -> String {
    let (tree, parts) = match crate::ui::cut_suggest_model_run::press_parts() {
        Ok(got) => got,
        Err(why) => return crate::ui::cut_suggest_model_run::blocked(why).said,
    };
    open_run();
    let held = window.clone();
    crate::ui::background::run(
        "suggest",
        move || crate::ui::cut_suggest_model_run::ask_on_worker(tree, parts),
        move |made| land_model(&held, made),
    );
    String::new()
}

fn open_run() {
    let Some(bar) = WINDOW_PAGE.with(
        |WindowPage {
             window_bar: bars, ..
         }| bars.borrow().last().cloned(),
    ) else {
        return;
    };
    let Some(queue) = WINDOW_PAGE.with(
        |WindowPage {
             window_queue: slots,
             ..
         }| slots.borrow().last().cloned(),
    ) else {
        return;
    };
    let project = crate::ui::window::session_handle().borrow().clone();
    crate::runqueue::start_run(
        &mut bar.borrow_mut(),
        &mut queue.borrow_mut(),
        &mut crate::exchanges::RunLog::new(),
        crate::run::Step::Suggest,
        crate::run::snapshot_sources(&project),
    );
    crate::ui::run_close::note_started_run(&bar);
    crate::run_stages::mirror_switches(&bar.borrow());
}

/// The model answered: land what it proposed, then hand the passes after the cut to a worker.
fn land_model(window: &adw::ApplicationWindow, made: crate::ui::cut_suggest_model_run::Landed) {
    if !made.wrote {
        // Nothing landed: §S2 S6's "no valid cut after 3 attempts", or a refusal. The cut on screen
        // stands exactly as it was, because a failed suggestion has no claim on it.
        end_run(window, &made.said);
        return;
    }
    if crate::cancel_leg::cancelled() {
        end_run(window, "stopped \u{2014} the cut stands as it was");
        return;
    }
    land(window, &made.applied);
    // §S2b's `M2` node: the cut landed, so now the passes that work over a cut are asked.
    let inputs = crate::ui::window::pass_inputs();
    if crate::cut_suggest_passes::order(&inputs.policy).is_empty() || crate::cancel_leg::cancelled()
    {
        end_run(window, &made.said);
        return;
    }
    let cut_ = crate::ui::window::newest_review_cut();
    let held = window.clone();
    crate::ui::background::run(
        "suggest passes",
        move || {
            crate::cut_suggest_passes::walk_after_cut(&inputs, &cut_, crate::ui::window::log_line)
        },
        move |outs| {
            for line in crate::cut_suggest_passes::land_after_cut(&held, &outs) {
                crate::ui::window::log_line(&line);
            }
            end_run(&held, &made.said);
        },
    );
}

fn end_run(window: &adw::ApplicationWindow, said: &str) {
    crate::ui::run_close::close_window_run();
    if !said.is_empty() {
        if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
            status.set_text(said);
        }
    }
}

pub fn land_effects(window: &adw::ApplicationWindow, answer: &Applied) {
    if answer.effects.is_empty() {
        return;
    }
    let mut with_effects = screen_cut();
    with_effects.fx = answer.effects.clone();
    if let Some(recorder) = CUT_PAGE.with(|CutPage { recorder: slot, .. }| slot.get()) {
        recorder(window, &with_effects);
    }
    if let Some(Some(tree)) = session_tree() {
        if let Err(why) = crate::cut_save::write_cut_at(&with_effects, &tree) {
            crate::ui::window_logs::say_failure(&format!("!!! could not save the cut -- {why}"));
        }
    }
}

pub(crate) type Session = Option<crate::layout::Tree>;

pub fn point_session_at(tree: Option<crate::layout::Tree>) {
    CUT_PAGE.with(|CutPage { session: slot, .. }| *slot.borrow_mut() = Some(tree));
}

pub(crate) fn session_tree() -> Option<Option<crate::layout::Tree>> {
    CUT_PAGE
        .with(|CutPage { session: slot, .. }| slot.borrow().clone())
        .or_else(crate::ui::window_line_step::last_session_tree)
        .or_else(|| {
            let dir = crate::startup::open_dir();
            crate::layout::Tree::new(&dir).ok().map(Some)
        })
}

pub fn session() -> Option<crate::layout::Tree> {
    session_tree().flatten()
}

/// §F2.14 S1: why ▶ did not ask for a cut, in the flowchart's fixed order, or `None` when the press
/// may run.
pub fn refuse_suggest_run(busy: bool) -> Option<String> {
    let tree = match session_tree() {
        Some(Some(tree)) => tree,
        // No folder to read at all: there is no timeline, which is §S1's third refusal, not a
        // fourth.
        _ => return Some(crate::suggest::NO_TIMELINE.to_string()),
    };
    // F1.12 S2 first: THIS press is what notices an edited `final.txt`, and remaking the marks from it is
    // its answer — not a refusal of it. The remake writes `retakes.tsv`, whose mtime then answers
    // `hand_edit::edited` with "no", so §S1's hand-edit question below is asked of fresh stamps and only
    // refuses while an edit really still stands (a remake over the ceiling writes nothing). Doing this
    // before the flag is read is the whole ordering: the other way round the Cut page could never remake
    // through its own ▶, because the first thing it said was "press Revert first".
    if crate::hand_edit::edited(mtime(&tree.final_txt()), mtime(&tree.retakes_tsv())) {
        let sources = crate::ui::window::session_sources();
        if let Some(outcome) = crate::hand_edit::before_cut(&tree, &sources) {
            for line in &outcome.logs {
                crate::ui::window::log_line(line);
            }
            // The ceiling refusal is THIS press's answer and it goes on the status line, not into
            // §S1's "press Revert first": nothing was marked, so what stands is the model's marks
            // and the person's own edit is what they have to fix. `refuse` below would otherwise
            // overwrite it with advice that does not apply.
            if outcome.refused {
                let said = outcome
                    .logs
                    .last()
                    .map(|line| line.trim_start_matches('!').trim_start().to_string())
                    .unwrap_or_else(|| "the edit was refused".to_string());
                return Some(said);
            }
        }
    }
    let hand_edits = crate::hand_edit::edited(mtime(&tree.final_txt()), mtime(&tree.retakes_tsv()));
    let has_timeline = tree.session_txt().exists();
    if let Some(refuse) = crate::suggest::refuse(busy, hand_edits, has_timeline) {
        return Some(refuse.said().to_string());
    }
    let states = crate::cut_suggest_words::read_word_states(
        &tree,
        &crate::ui::window::newest_review_cut(),
        &crate::ui::cut_folds::page_recordings(),
    );
    let (may_run, sentence) = crate::cut_suggest_words::press_answer(&states);
    (!may_run).then_some(sentence)
}

fn mtime(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

pub fn install(read: fn() -> Cut, recorder: Recorder) {
    CUT_PAGE.with(|CutPage { read: slot, .. }| slot.set(Some(read)));
    CUT_PAGE.with(|CutPage { recorder: slot, .. }| slot.set(Some(recorder)));
}

fn screen_cut() -> Cut {
    CUT_PAGE
        .with(|CutPage { read: slot, .. }| slot.get())
        .map(|read| read())
        .unwrap_or_default()
}

/// The cut a suggestion lands as: the walk-back's segments, the inserts it kept (hand-placed cards
/// are nobody's to trim — §7: "Suggest replaces the footage half and the effects, keeps inserts"),
/// and the effects as clamped against that cut.
pub fn suggested_cut(before: &Cut, answer: &Applied) -> Cut {
    let mut landed = before.clone();
    // The footage half is the walk-back's own seconds.
    let footage: Vec<Seg> = answer
        .segs
        .iter()
        .map(|(s, e)| Seg {
            s: *s,
            e: *e,
            ..Default::default()
        })
        .chain(answer.kept_inserts.iter().cloned())
        .collect();
    landed.segs = cut::keep_inserts(&footage, &before.segs);
    // §9's row, asked of every landed scene — the ones this reply proposed and the hand-placed
    // cards carried over.
    let rows: Vec<i32> = crate::cut_suggest_words::depended_cam_slots(before);
    for scene in landed.segs.iter_mut() {
        scene.cam = crate::effect_overlap::suggested_scene_cam_of(scene, &rows);
    }
    landed.fx = answer.effects.clone();
    follow_the_speaker(&mut landed);
    landed
}

/// §F2.15: with two camera rows and Prepare's who.json, every landed scene is shown from the camera of
/// whoever is speaking. Part of the suggestion, so its one Undo takes it back with the rest.
fn follow_the_speaker(landed: &mut Cut) {
    let recordings = crate::ui::page_recordings();
    let rows = crate::timeline::rows_for(&recordings, landed);
    let mut distinct = rows.clone();
    distinct.sort_unstable();
    distinct.dedup();
    // One camera row: nothing to choose between, nothing said.
    if distinct.len() < 2 {
        return;
    }
    let Some(Some(tree)) = session_tree() else {
        return;
    };
    match crate::cut_follow::follow(&tree, &landed.segs, &recordings, &rows) {
        Ok((segs, logs)) => {
            landed.segs = segs;
            for line in &logs {
                crate::ui::window::log_line(line);
            }
        }
        Err(why) => {
            crate::ui::window::log_line(&format!(">>> cameras: nothing to follow -- {why}"))
        }
    }
}

/// At most one breath at each join of a suggestion (§ suggestions, [`crate::cut_breaths`]): read off the recordings
/// the joins fall in; with no session folder the suggestion lands as it came.
fn with_one_breath(answer: &Applied) -> Applied {
    let Some(Some(tree)) = session_tree() else {
        return answer.clone();
    };
    let project = crate::ui::window::session_handle().borrow().clone();
    let (segs, logs) =
        crate::cut_breaths::apply(&tree, &project, &answer.segs, &answer.kept_inserts);
    for line in &logs {
        crate::ui::window::log_line(line);
    }
    Applied {
        segs,
        ..answer.clone()
    }
}

/// Land the WORDS cut (§S2a): the same door as [`land`], from the plain segments a no-model cut
/// produced.
pub fn land_spans(window: &adw::ApplicationWindow, spans: &[(f64, f64)]) -> Cut {
    let answer = with_one_breath(&Applied {
        segs: spans.to_vec(),
        kept_inserts: Vec::new(),
        effects: Vec::new(),
        log: Vec::new(),
    });
    let landed = suggested_cut(&screen_cut(), &answer);
    // The same order `land` keeps for the model path, for the same reason: one undo step for the
    // cut, then the base moves so Revert keeps the words cut and drops only hand edits after it.
    if let Some(recorder) = CUT_PAGE.with(|CutPage { recorder: slot, .. }| slot.get()) {
        recorder(window, &landed);
    }
    if let Some(Some(tree)) = session_tree() {
        if let Err(why) = crate::cut_save::write_cut_at(&landed, &tree) {
            crate::ui::window_logs::say_failure(&format!("!!! could not save the cut -- {why}"));
        }
    }
    landed
}

/// Land a suggestion on `window`: the cut is published through the page's own edit door, and the
/// base moves to it in the same write, because §2 says the base IS the last suggestion. `answer` is
/// what [`crate::suggest::apply`] produced from the model's segments.
pub fn land(window: &adw::ApplicationWindow, answer: &Applied) -> Cut {
    let answer = &with_one_breath(answer);
    let landed = suggested_cut(&screen_cut(), answer);
    // The recorder first: one undo step for the suggestion, so ↶ can take the suggestion back the
    // way it takes any other edit.
    if let Some(recorder) = CUT_PAGE.with(|CutPage { recorder: slot, .. }| slot.get()) {
        recorder(window, &landed);
    }
    if let Some(Some(tree)) = session_tree() {
        if let Err(why) = crate::cut_save::write_cut_at(&landed, &tree) {
            crate::ui::window_logs::say_failure(&format!("!!! could not save the cut -- {why}"));
        }
    }
    landed
}
