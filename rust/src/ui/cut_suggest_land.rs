//! §05-cut#2 — "base = the last suggestion or what the page opened with" (`spec/05-cut.md` §2).
//!
//! The walk-back already exists as a pure rule ([`crate::suggest::apply`]) and the base rule already
//! exists as a pure rule ([`crate::cut::History::suggested`]); what was missing was the seam between
//! them — nothing in the program ever moved the base, so after a suggestion Revert still went back to
//! what the page opened with and dropped the model's work instead of only the hand edits made after it.
//!
//! This module is that seam, and only that: it turns a suggestion into the cut shape [`record_edit`]
//! takes, hands it to the window's own edit door (so the undo stack keeps the user's own steps, which is
//! `History::suggested`'s stated intent), and moves the base in the same write. Split out of `window.rs`,
//! which is past its size budget.

use glib::object::Cast;

use crate::cut::{Cut, Seg};
use crate::suggest::Applied;

/// The window's edit door, in the shape a caller outside `ui::window` can reach: publish the cut, record
/// one undo step, refresh the history buttons. `ui::window::record_edit` installs itself here at the end
/// of `build_window`, which is the same moment the page's own wires go on — so the program, not a test,
/// is what supplies it (rule: a seam the program itself uses).
type Recorder = fn(&adw::ApplicationWindow, &Cut);

thread_local! {
    static RECORDER: std::cell::Cell<Option<Recorder>> = const { std::cell::Cell::new(None) };
    /// The read side of the same seam: what the page is showing right now.
    static READ: std::cell::Cell<Option<fn() -> Cut>> = const { std::cell::Cell::new(None) };
}

/// §F2.14 S2a/S2b: which half of Suggest this press runs — see [`suggest_press`], which does both the
/// refusal and the mode.
/// What the Cut page's ▶ answered, so the shell can print it and decide whether the press ends here.
#[derive(Debug, Clone, PartialEq)]
pub struct SuggestAnswer {
    /// The sentence for the status line — a refusal's, or the words cut's status.
    pub said: String,
    /// Whether the press was REFUSED (§S1), which is what makes the caller log it.
    pub refused: bool,
    /// Whether the press is over. A suggestion is built here and now (§S2a needs no run), so today this
    /// is true either way; the field is what lets §S2b hand the bar a RUN instead of an answer without
    /// rewriting the shell's press handler.
    pub stops: bool,
}

/// §F2.14 S1 then S2: refuse, or build. The refusals come first (S1); the MODE is the next question the
/// press asks. Lives here, not in `ui::window`, because `window.rs` is at its size budget and this is
/// the suggest flow's decision, not the shell's.
pub fn suggest_press(
    window: &adw::ApplicationWindow,
    policy_: &crate::project::Policy,
    busy: bool,
) -> SuggestAnswer {
    use crate::project::CutMode;
    if let Some(reason) = refuse_suggest_run(window, busy) {
        return SuggestAnswer {
            said: reason,
            refused: true,
            stops: true,
        };
    }
    let said = match policy_.cut_mode.value {
        CutMode::Words => press_words(window),
        // §S2b: the model is asked, the answer is walked back, and only then does anything reach the
        // page. The bar is NOT opened either way — a suggestion is an answer, not a run — which is what
        // `stops: true` below says for both modes.
        CutMode::Model => press_model(window, busy),
    };
    SuggestAnswer {
        said,
        refused: false,
        stops: true,
    }
}

/// The window's own ▶ press for the words mode: resolve the session the refusal already accepted, then
/// hand the whole thing to §S2a. `String::new()` when there is no session at all — S1 refused that press
/// before anyone got here, so an empty answer means "nothing of mine to say", never "the cut failed".
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

/// The window's own ▶ press for the model mode (§S2b): ask, walk back, land, say what landed. The call
/// itself lives in [`crate::ui::cut_suggest_model_run`]; this is the half that owns the write.
///
/// Landing goes through [`land_spans`], the same door §S2a uses — recorder first, then the base moves,
/// then `cut.json` — because F2.13's one-↶ promise is one rule for every suggestion, and a second
/// landing path would be a second place to get that order wrong. The effects the walk-back clamped are
/// laid down by [`land_effect_pair`] right after, in their own undo step: they are §S3's second half and
/// §S3's log line names them separately (">>> …and N effect(s)"), so taking a suggestion back takes the
/// scenes and their decorations with it without pretending the two were one arithmetic.
fn press_model(window: &adw::ApplicationWindow, busy: bool) -> String {
    let made = crate::ui::cut_suggest_model_run::press(window, busy);
    if !made.wrote {
        // Nothing landed: §S2 S6's "no valid cut after 3 attempts", or a refusal the shell prints. The
        // cut on screen stands exactly as it was, because a failed suggestion has no claim on it.
        return made.said;
    }
    land_spans(window, &made.segs);
    land_effect_pair(window, &made.applied);
    // §S2b's `M2` node: the cut landed, so now the passes that work over a cut are asked — captions, speeds,
    // decorations, and only those the policy switched on. Their own sentences go to the log as they said them,
    // then §S3's effect line counts what actually landed (the walk-back's clamped effects plus theirs).
    let policy_ = crate::ui::session_policy(window);
    let pass_lines =
        crate::cut_suggest_passes::run_after_cut(window, &policy_, |said| {
            crate::ui::window::log_line(said)
        });
    for line in &pass_lines {
        crate::ui::window::log_line(line);
    }
    // The status row takes the short sentence (`suggest::apply`'s own first log line); the long pair of
    // `>>>` lines is already in the log, written by the module that computed them.
    if !made.said.is_empty() {
        if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
            status.set_text(&made.said);
        }
    }
    made.said
}

/// Lay the walk-back's effects on the cut that just landed, as its own edit: publish, record, move the
/// base, save. No recorder installed means nothing to write — the same answer [`land`] gives.
pub fn land_effects(window: &adw::ApplicationWindow, answer: &Applied) {
    land_effect_pair(window, answer);
}

fn land_effect_pair(window: &adw::ApplicationWindow, answer: &Applied) {
    if answer.effects.is_empty() {
        // Nothing to add: the scenes landed and there were no decorations to propose. A second write
        // that changed nothing would be an undo step for no change, so ↶ would appear to do nothing.
        return;
    }
    let mut with_effects = screen_cut();
    with_effects.fx = answer.effects.clone();
    if let Some(recorder) = RECORDER.with(|slot| slot.get()) {
        recorder(window, &with_effects);
    }
    if let Some(Some(tree)) = session_tree() {
        let _ = crate::cut::save(&with_effects, &tree);
    }
}

/// The session folder this window's suggestion reads, or nothing at all when no session was opened.
/// `ui::window::narrate_session_tree` is the same question for the Narrate page; §F2.14 S1 answers "no
/// tree" with NO_TIMELINE rather than guessing a project.
type Session = Option<crate::layout::Tree>;

thread_local! {
    /// What ▶ on the Cut page should read as "this session": set by the program at the same moment the
    /// edit door is installed, replaced by a test that wants to point the flow at a throwaway project.
    static SESSION: std::cell::RefCell<Option<Session>> = const { std::cell::RefCell::new(None) };
}

/// Point the Cut page's ▶ at a session folder. The program calls it with none (the door then resolves
/// the folder itself); a test calls it with the `.naivepost` it just wrote.
pub fn set_session_for_test(tree: Option<crate::layout::Tree>) {
    SESSION.with(|slot| *slot.borrow_mut() = Some(tree));
}

/// The session folder: what a test pointed at, else the launch's own `session.naivepost` beside the
/// working directory — the resolution every other page's refusal door uses. `pub(crate)` because the
/// landing writes where this answers, so reader and writer cannot disagree about the project.
pub(crate) fn session_tree() -> Option<Option<crate::layout::Tree>> {
    SESSION.with(|slot| slot.borrow().clone()).or_else(|| {
        let dir = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
        crate::layout::Tree::new(&dir).ok().map(Some)
    })
}

/// The session folder as [`session_tree`] resolves it, for the other half of the suggest flow. The
/// model call needs the same folder the refusal checked, and asking twice would let the guard and the
/// work land on different projects.
pub fn session() -> Option<crate::layout::Tree> {
    session_tree().flatten()
}

/// §F2.14 S1: why ▶ did not ask for a cut, in the flowchart's fixed order, or `None` when the press may
/// run. Answers busy → hand edits → no session timeline → the word gate, and returns the sentence the
/// page prints verbatim; every one of them is another module's constant, so the wording cannot drift.
///
/// `busy` arrives from the caller BEFORE the bar opens, exactly as `refuse_narrate_run_with_busy` needs:
/// reading the bar after `press` would refuse this press for the run it just started.
pub fn refuse_suggest_run(window: &adw::ApplicationWindow, busy: bool) -> Option<String> {
    let tree = match session_tree() {
        Some(Some(tree)) => tree,
        // No folder to read at all: there is no timeline, which is §S1's third refusal, not a fourth.
        _ => return Some(crate::suggest::NO_TIMELINE.to_string()),
    };
    // F1.12 S2 first: THIS press is what notices an edited `final.txt`, and remaking the marks from it is
    // its answer — not a refusal of it. The remake writes `retakes.tsv`, whose mtime then answers
    // `hand_edit::edited` with "no", so §S1's hand-edit question below is asked of fresh stamps and only
    // refuses while an edit really still stands (a remake over the ceiling writes nothing). Doing this
    // before the flag is read is the whole ordering: the other way round the Cut page could never remake
    // through its own ▶, because the first thing it said was "press Revert first".
    if crate::hand_edit::edited(mtime(&tree.final_txt()), mtime(&tree.retakes_tsv())) {
        let sources = crate::ui::window::session_sources(window);
        if let Some(outcome) = crate::hand_edit::before_cut(&tree, &sources) {
            for line in &outcome.logs {
                crate::ui::window::log_line(line);
            }
            // The ceiling refusal is THIS press's answer and it goes on the status line, not into §S1's
            // "press Revert first": nothing was marked, so what stands is the model's marks and the
            // person's own edit is what they have to fix. `refuse` below would otherwise overwrite it
            // with advice that does not apply.
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
    // The three flags are the pure rule's; only the word gate below needs more than a flag.
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

/// A file's stamp, or `None` when it is absent — which is how "there are no marks yet" reaches
/// [`crate::hand_edit::edited`] without a second spelling of "read the mtime".
fn mtime(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Install the edit door. Called once per window build, from `ui::window::build_window`.
pub fn install(read: fn() -> Cut, recorder: Recorder) {
    READ.with(|slot| slot.set(Some(read)));
    RECORDER.with(|slot| slot.set(Some(recorder)));
}

/// The cut on screen, as the installed door answers it. Falls back to an empty cut only when no window has
/// been built, which is the state in which nothing can be landed anyway.
fn screen_cut() -> Cut {
    READ.with(|slot| slot.get()).map(|read| read()).unwrap_or_default()
}

/// The cut a suggestion lands as: the walk-back's segments, the inserts it kept (hand-placed cards are
/// nobody's to trim — §7: "Suggest replaces the footage half and the effects, keeps inserts"), and the
/// effects as clamped against that cut. Everything else the cut holds — the lane windows, the shift map,
/// the row pins, the folds — carries over from `before`, because a suggestion proposes scenes and
/// decorations, not the page's layout decisions.
pub fn suggested_cut(before: &Cut, answer: &Applied) -> Cut {
    let mut landed = before.clone();
    landed.segs = answer
        .segs
        .iter()
        .map(|(s, e)| Seg {
            s: *s,
            e: *e,
            ..Default::default()
        })
        .chain(inserts_of(before).into_iter().chain(answer.kept_inserts.iter().cloned()))
        .collect();
    landed.fx = answer.effects.clone();
    landed
}

/// Hand-placed cards in the cut on screen — the ones a suggestion must not touch (§7). Read by
/// [`suggested_cut`] on every landing, so the answer to "which segs are cards" is spelled once.
fn inserts_of(cut: &Cut) -> Vec<Seg> {
    cut.segs.iter().filter(|seg| seg.is_insert()).cloned().collect()
}

/// Land the WORDS cut (§S2a): the same door as [`land`], from the plain segments a no-model cut
/// produced. Hand-placed inserts carry over — a words cut proposes scenes, it has no claim on a card
/// a person placed — and the base moves with the cut, so Revert keeps the words cut and drops only
/// hand edits made after it.
pub fn land_spans(window: &adw::ApplicationWindow, spans: &[(f64, f64)]) -> Cut {
    let answer = Applied {
        segs: spans.to_vec(),
        kept_inserts: Vec::new(),
        effects: Vec::new(),
        log: Vec::new(),
    };
    let landed = suggested_cut(&screen_cut(), &answer);
    // The same order `land` keeps for the model path, for the same reason: one undo step for the cut,
    // then the base moves so Revert keeps the words cut and drops only hand edits after it. The write
    // is here rather than in the caller because a landing that skipped it would leave the next launch
    // opening a cut the page has already shown and logged.
    if let Some(recorder) = RECORDER.with(|slot| slot.get()) {
        recorder(window, &landed);
    }
    if let Some(Some(tree)) = session_tree() {
        let _ = crate::cut::save(&landed, &tree);
    }
    landed
}

/// Land a suggestion on `window`: the cut is published through the page's own edit door, and the base moves
/// to it in the same write, because §2 says the base IS the last suggestion.
///
/// Returns the landed cut so the caller can log or repaint from exactly what was written. `answer` is what
/// [`crate::suggest::apply`] produced from the model's segments — this function never re-decides any of it.
pub fn land(window: &adw::ApplicationWindow, answer: &Applied) -> Cut {
    let landed = suggested_cut(&screen_cut(), answer);
    // The recorder first: one undo step for the suggestion, so ↶ can take the suggestion back the way it
    // takes any other edit. Then the base moves, so Revert keeps this suggestion and drops only the hand
    // edits made after it — the order is the whole rule; the other way round leaves Revert answering with
    // the cut from before the model spoke.
    if let Some(recorder) = RECORDER.with(|slot| slot.get()) {
        recorder(window, &landed);
        return landed;
    }
    landed
}
