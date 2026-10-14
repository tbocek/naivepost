//! F4.1 ▶ Write and speak — the window seam of one narration run: what a press is refused, the bar it
//! paints, and the status lines it leaves. Split out of `window.rs`, which is past its size budget; its
//! doors are re-exported through `window`.
//!
//! The run's decisions are `crate::narrate_run`'s; what is here gathers them off this window's own state,
//! forwards them, and paints. Nothing in this block decides anything: the refusal order, which line is
//! spoken, and every sentence that ends up on screen belong to `narrate_run`. The narration call itself
//! arrives through a closure for the same reason `run_captions_pass_with_reply` exists — a test drives the
//! real route with a scripted answer and no socket.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::layout;
use crate::narrate_data;
use crate::narrate_off::{self};
use crate::narrate_preview;
use crate::narrate_run::{self, JOBS, JOB_NARRATION, JOB_SPEAKING, SPEAKING_STATUS, STAGE_THINKING, Speak};
use crate::narrate_screen;
use crate::narration::{self, Narration};
use crate::run;
use crate::project::Project;
use crate::runqueue;
use crate::startup;
use crate::ui::window as ui;

// --- F4.1 ▶ Write and speak -----------------------------------------------------------------
//
// The run's decisions are `crate::narrate_run`'s; what is here gathers them off this window's session,
// forwards them, and paints. Nothing in this block decides anything: the refusal order, which clips the
// one call covers, whether a line is spoken, and every sentence that ends up on screen all come from
// `narrate_run`. The narration call itself arrives through a closure for the same reason
// `run_captions_pass_with_reply` exists — a test drives the real route with a scripted answer.

/// S1 alone: does this press of ▶ get refused, and with which sentence? Reads three of the four gates
/// off the project folder; busy comes from the run bar rather than the project, so it is asked through
/// [`narrate_bar_busy`] and handed to [`narrate_run::refuse`], which owns the order.
pub fn refuse_narrate_run(project: &Project) -> Option<String> {
    refuse_narrate_run_with_busy(project, narrate_bar_busy())
}

/// The same four gates with `busy` handed in. Needed because the shell's ▶ forwards HERE, after
/// `runqueue::start_run` has already put this press's own run in the bar: reading that as "a run is
/// under way" would refuse the press with S1's PAUSING sentence for the run it just opened. The
/// handler's pre-press check (`narrate_asked` above) is where busy is honestly answered.
pub fn refuse_narrate_run_with_busy(project: &Project, busy: bool) -> Option<String> {
    let tree = narrate_session_tree();
    let has_cut = tree
        .as_ref()
        .and_then(|tree| cut::load(tree).ok())
        .is_some_and(|cut_| !cut_.segs.is_empty());
    let has_timeline = tree
        .as_ref()
        .is_some_and(|tree| tree.session_tsv().is_file());
    narrate_run::refuse(busy, has_cut, project.no_narration, has_timeline)
}

/// Whether this window's run bar counts as busy — the same question [`run::controls`] answers when it
/// picks ⏸ for the button face, read here so the refusal and the button cannot disagree.
pub(crate) fn narrate_bar_busy() -> bool {
    ui::RUN_BARS
        .with(|bars| bars.borrow().last().cloned())
        .is_some_and(|bar| {
            let bar = bar.borrow();
            run::controls(&bar.running, None).icon == run::PAUSE_ICON
        })
}

/// This session's project folder, resolved the way every other flow resolves it, so "this project" means
/// one thing app-wide. `None` when no project is open.
pub fn narrate_session_tree() -> Option<layout::Tree> {
    let dir = startup::session_dir(&std::env::current_dir().unwrap_or_default());
    layout::Tree::new(&dir).ok()
}

/// F4.1 S1–S7 on this window. With no scripted reply this is the LIVE path: ▶ telephones the address
/// the Settings hold through `narrate_live`, over real HTTP, three attempts at most (§F4.2).
pub fn press_narrate_run(window: &adw::ApplicationWindow) -> String {
    // The scripted reply lives in a thread-local so the shell's own ▶ can reach it too: the real
    // button's handler calls THIS function, and a test that wants the written-and-spoken case drives
    // the button rather than the seam. Empty means "ask for real", which is what the app does.
    let reply = NARRATE_SCRIPT.with(|cell| cell.borrow().clone());
    if reply.is_empty() {
        return run_narrate_on(window, live_narrate_reply);
    }
    run_narrate_on(window, move |_tree, clips| {
        Ok(reply
            .iter()
            .filter_map(|(clip, at, text, emotion)| {
                let (start, end) = *clips.get((*clip as usize).checked_sub(1)?)?;
                Some(narrate_run::Written {
                    start,
                    end,
                    at: *at,
                    text: text.clone(),
                    emotion: emotion.clone(),
                })
            })
            .collect())
    })
}

/// The live narration leg: with no scripted reply, ▶ comes here (§F4.2's call over real HTTP). All the
/// gathering and driving lives in `narrate_ask`, so this file stays a shell.
pub(crate) fn live_narrate_reply(
    tree: &layout::Tree,
    bounds: &[(f64, f64)],
) -> Result<Vec<narrate_run::Written>, String> {
    crate::narrate_ask::run(tree, &live_project(), bounds)
}

/// One narration reply handed to the shell's ▶ by a test: `(clip number from 1, clip-relative second,
/// text, emotion)`, the same shape `press_narrate_run_with_reply` takes. Set it, then click
/// `play-button`; the handler reads it here instead of telephoning F4.2's endpoint, which headless has
/// none. Clearing it puts ▶ back to the no-model behaviour.
/// Feed `press_narrate_run` a scripted narration reply instead of telephoning F4.2, and let it stand
/// until the next call replaces it. A test that wants the no-model behaviour passes an empty slice.
pub fn set_narrate_script(reply: &[(u32, f64, &str, &str)]) {
    NARRATE_SCRIPT.with(|cell| {
        cell.replace(
            reply
                .iter()
                .map(|(clip, at, text, emotion)| (*clip, *at, text.to_string(), emotion.to_string()))
                .collect(),
        )
    });
}

/// Whether a scripted narration reply is loaded — exported so a test can assert the button really is
/// reading the script rather than falling through to the no-model path.
pub fn narrate_script_loaded() -> bool {
    !NARRATE_SCRIPT.with(|cell| cell.borrow().is_empty())
}

/// F4.1's body for THIS window and the live session's project. Both doors — the seam above and the
/// shell's ▶ — come through here, so the two cannot drift apart.
pub(crate) fn run_narrate_on<FReply>(window: &adw::ApplicationWindow, reply: FReply) -> String
where
    FReply: FnMut(
        &layout::Tree,
        &[(f64, f64)],
    ) -> Result<Vec<narrate_run::Written>, String>,
{
    let project = live_project();
    // A bar already holding a run means the press arriving here opened it (the shell's ▶ forwards
    // after `start_run`), so it must not be read as "busy" — see `refuse_narrate_run_with_busy`.
    let opened_by_this_press = bar_is_running();
    let reason = if opened_by_this_press {
        refuse_narrate_run_with_busy(&project, false)
    } else {
        refuse_narrate_run(&project)
    };
    if let Some(reason) = reason {
        say_on_status(window, &reason);
        return reason;
    }
    // A press that came through `run::RunBar::press` opened a run before arriving here; nothing
    // headless streams to close it, so this call closes it at its own end. Three writes follow the
    // close, because the handler paints from the bar AFTER this returns and `end_run` leaves both
    // `running` and `status` empty: put the sentence back on the label, repaint ▶ from the idle bar,
    // and redraw the progress area so the stage the run painted is not wiped by that later paint.
    let said = run_narrate_with(window, &project, reply);
    if opened_by_this_press {
        clear_narrate_bar();
        set_status_line(window, &said);
        repaint_play_button(window);
    } else {
        set_status_line(window, &said);
    }
    said
}

/// Repaint ▶ and ⏹ from the bar's current state after a narrate run closed it, so neither button is
/// left showing the face of a run that has ended.
pub(crate) fn repaint_play_button(window: &adw::ApplicationWindow) {
    let Some(bar) = ui::RUN_BARS.with(|bars| bars.borrow().last().cloned()) else { return };
    let drawn = run::controls(&bar.borrow().running, None);
    if let Some(play) = ui::play_button(window) {
        play.set_icon_name(drawn.icon);
        play.set_tooltip_text(Some(drawn.tooltip));
    }
    // ⏹'s sensitivity comes from the same call, as `paint_run_bar` draws them together: with the run
    // closed there is nothing left to end, so it greys out again.
    if let Some(stop_) = ui::stop_button(window) {
        stop_.set_sensitive(drawn.stop_sensitive);
    }
}

/// Whether this window's run bar currently holds a run — the same slot [`clear_narrate_bar`] empties.
pub(crate) fn bar_is_running() -> bool {
    ui::RUN_BARS.with(|bars| bars.borrow().last().cloned())
        .is_some_and(|bar| bar.borrow().running.is_some())
}

/// Write one sentence on this window's status line without logging it (the run already logged it).
/// Put this window's status row to `said` — the one door every page uses.
pub fn set_status_line(window: &adw::ApplicationWindow, said: &str) {
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(said);
    }
}

/// Put this window's run bar back to idle after a narrate run that ran to its end in one go, through
/// the one close door F0.5 S4 gives every run (running off, controls, the audio unload handed over).
pub(crate) fn clear_narrate_bar() {
    crate::ui::run_close::close_last_narrate_run();
}

/// The live project this window runs under — the newest slot, the same rule `session_policy` reads.
/// Public to the crate so the Produce page's F5.1 seam reads the session rather than a widget copy of it.
pub(crate) fn live_project() -> Project {
    ui::PLAY_SESSION
        .with(|slots| slots.borrow().last().cloned())
        .map(|held| held.borrow().clone())
        .unwrap_or_default()
}

/// The shared body of both doors. Private so they cannot drift apart. Steps land in §F4.1's order, and
/// each one's wording comes from `narrate_run`, never from a string written here.
pub(crate) fn run_narrate_with<FReply>(
    window: &adw::ApplicationWindow,
    project: &Project,
    mut reply: FReply,
) -> String
where
    FReply: FnMut(
        &layout::Tree,
        &[(f64, f64)],
    ) -> Result<Vec<narrate_run::Written>, String>,
{
    let Some(tree) = narrate_session_tree() else {
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        return said;
    };
    let cut_ = cut::load(&tree).unwrap_or_default();
    let existing = narration::load(&tree).unwrap_or_default();
    let voice = narrate_data::read_voice(&tree);
    let rewrite = project.policy.narration_rewrite.value;
    // The clips this run writes for are the VISIBLE SEGMENTS of the cut in play order. NOT
    // `cut_play::kept_runs`: that MERGES touching segments for preloading, so three adjoining clips
    // would read as one and every clip number above 1 the model named would fall outside the batch.
    let mut clips: Vec<(f64, f64)> = cut_
        .segs
        .iter()
        .filter(|seg| seg.e > seg.s)
        .map(|seg| (seg.s, seg.e))
        .collect();
    clips.sort_by(|a, b| a.0.total_cmp(&b.0));
    let plan = narrate_run::plan_run(rewrite, &clips, &existing, &voice);

    // S2: pull half-typed rows into the record, then keep the previous generation before anything
    // overwrites it. Both happen before the save so the file on disk matches what the run believes.
    let mut record = pull_half_typed(window, &existing);
    if let Err(error) = narrate_data::keep_previous(&tree) {
        ui::log_line(&format!("!!! could not keep narration.prev.json -- {error}"));
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        return said;
    }
    // S3: save the project, open the two jobs, and put the reason on the log BEFORE the call goes out.
    if let Err(error) = crate::project::save(project, tree.dir()) {
        ui::log_line(&format!("!!! could not save the project -- {error}"));
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        return said;
    }
    ui::log_line(&plan.log);
    NARRATE_QUEUE.with(|queue| {
        let mut queue = queue.borrow_mut();
        queue.reset();
        queue.phase(0.0, 0.5);
        queue.job(0, JOB_NARRATION, 1, JOBS);
        queue.push(0, plan.clips.len().max(1), "clip");
        queue.prog(0, 0.0, STAGE_THINKING);
        queue.job(1, JOB_SPEAKING, 2, JOBS);
    });
    set_narrate_stage(window, STAGE_THINKING);

    let written = match reply(&tree, &plan.clips) {
        Ok(lines) => lines,
        Err(error) => {
            ui::log_line(&format!("!!! narrate: the call failed -- {error}"));
            let said = narrate_run::failed_status("narration");
            say_on_status(window, &said);
            return said;
        }
    };
    if written.is_empty() {
        // No line came back: nothing is saved, nothing is spoken, and the run says so plainly.
        let said = narrate_run::Outcome::NothingWritten.status();
        say_on_status(window, &said);
        return said;
    }

    // S4: fold the written lines in (the silent list survives inside `fold_written`), save, publish.
    record = narrate_run::fold_written(&record, &written);
    if let Err(error) = narration::save(&record, &tree) {
        ui::log_line(&format!("!!! could not write narration.json -- {error}"));
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        return said;
    }
    if let Some(held) = ui::HELD_NARRATION.with(|held| held.borrow().last().cloned()) {
        *held.borrow_mut() = record.clone();
    }
    // The bar's stage follows the clips as they close; headless they all arrive with the one reply, so
    // this is the final count in one step.
    set_narrate_stage(window, &narrate_run::writing(written.len(), plan.clips.len()));
    ui::log_line(&narrate_run::written_log(written.len()));
    NARRATE_QUEUE.with(|queue| queue.borrow_mut().done(0, 1.0));

    // S5: a captions-only voice stops here — the lines exist and nothing is spoken.
    if plan.captions_only {
        let said = narrate_run::captions_only_done(written.len());
        say_on_status(window, &said);
        return said;
    }

    // S6: the speaking pass. F4.4 owns the POST /v1/audio/speech request; `narrate_tts` builds the
    // request, the options and the take path but makes no call yet, so this seam reports what WOULD be
    // synthesized and counts the cache honestly -- the cached half is a real file check, so a warm
    // project really does report zero work.
    say_on_status(window, SPEAKING_STATUS);
    // F4.8: the lines this pass may speak come through `narrate_off::lines_to_speak`, the one seam the
    // tick owns — with narration off it answers empty, so nothing is synthesized behind the tick's back,
    // and a blank line stays a deliberate silence that is never spoken.
    let to_speak = narrate_off::lines_to_speak(live_project().no_narration, &record.entries);
    NARRATE_QUEUE
        .with(|queue| queue.borrow_mut().push(1, to_speak.len().max(1), "line"));
    let speaks = narrate_run::speak_pass(&voice, to_speak.as_slice(), |entry| {
        let key = narration::key_for(entry, Some(&voice));
        tree.tts_wav(&narration::tts_file(&key)).is_file()
    });
    for (index, speak) in speaks.iter().enumerate() {
        if *speak == Speak::Synthesize {
            ui::log_line(&narrate_preview::synthesizing(index));
        }
    }
    let (spoken, cached) = narrate_run::tally(&speaks);
    ui::log_line(&narrate_run::spoken_log(spoken, cached));

    // S7: the ending goes on the STATUS LINE, which is where the prototype never put it.
    let said = narrate_run::Outcome::Spoken {
        written: written.len(),
        spoken,
    }
    .status();
    say_on_status(window, &said);
    said
}

/// S2: read the row boxes back into the record, so a half-typed line that was never committed still
/// reaches the file the run saves. Empty boxes are left exactly as they were -- clearing a box is not
/// the same act as deleting a line (F4.7's 🗑 is), so an empty box never erases an entry. Rows are read
/// by name (`line-text-<n>`, one per entry in the page's sorted order) rather than from a remembered
/// handle, because `refresh` rebuilds the rows whole. The box is write-only: `parse_box` reads a `@N`
/// out of the tag, and nothing here prints a second back into it -- the time field owns the number.
pub fn pull_half_typed(window: &adw::ApplicationWindow, existing: &Narration) -> Narration {
    let mut record = existing.clone();
    for index in 0..record.entries.len() {
        let Some(found) = ui::find_widget_by_name(window.upcast_ref(), &format!("line-text-{index}")) else {
            continue;
        };
        let Ok(entry_box) = found.downcast::<gtk::Entry>() else {
            continue;
        };
        let typed = entry_box.text().to_string();
        if typed.trim().is_empty() {
            continue;
        }
        let parsed = narrate_screen::parse_box(&typed);
        let entry = &mut record.entries[index];
        match parsed.role {
            // A placement tag makes the line a caption and clears the emotion, so no entry carries both.
            narrate_screen::BoxRole::Caption => {
                entry.pos = if parsed.tag.is_empty() {
                    "bottom".to_string()
                } else {
                    parsed.tag
                };
                entry.emotion = String::new();
            }
            _ => {
                entry.emotion = parsed.tag;
            }
        }
        entry.text = parsed.body;
    }
    record.sort();
    record
}

/// What the bar's label reads right now, and where it is painted. Kept in thread-locals so the doors can
/// report the sequence without having captured the builder's locals, and so a test can read the stages
/// back instead of trusting a comment.
pub(crate) fn set_narrate_stage(window: &adw::ApplicationWindow, stage: &str) {
    NARRATE_STAGE.with(|cell| cell.replace(stage.to_string()));
    let fraction = NARRATE_QUEUE.with(|queue| queue.borrow().fraction());
    let tip = NARRATE_QUEUE.with(|queue| queue.borrow().text_and_tip().1);
    if let Some(bar) = ui::find_widget_by_name(window.upcast_ref(), "run-progress")
        .and_then(|w| w.downcast::<gtk::ProgressBar>().ok())
    {
        bar.set_fraction(fraction.clamp(0.0, 1.0));
        bar.set_text(Some(stage));
        bar.set_show_text(true);
        if !tip.is_empty() {
            bar.set_tooltip_text(Some(&tip));
        }
    }
}

/// The stage text this run last put on the bar.
pub fn narrate_stage() -> String {
    NARRATE_STAGE.with(|cell| cell.borrow().clone())
}

/// Put the narration run's bar face on screen without running the run: `narration 1/2` open, two of
/// four clips closed. Only the snapshot needs this -- headless there is no model to answer, but §07's
/// screen shows a page that has just been written, and an idle bar would misrepresent it. Paints
/// through the same `set_narrate_stage` the run itself uses, so the shot cannot show a state the run
/// never produces.
pub fn show_narrate_stage(window: &adw::ApplicationWindow, written: usize, of: usize) {
    NARRATE_QUEUE.with(|queue| {
        let mut queue = queue.borrow_mut();
        queue.reset();
        queue.phase(0.0, 0.5);
        queue.job(0, JOB_NARRATION, 1, JOBS);
        queue.push(0, of.max(1), "clip");
        queue.prog(0, if of == 0 { 0.0 } else { written as f64 / of as f64 }, &narrate_run::writing(written, of));
        queue.job(1, JOB_SPEAKING, 2, JOBS);
    });
    set_narrate_stage(window, &narrate_run::writing(written, of));
}

thread_local! {
    /// The narration run's own view of the bar: `narration 1/2` then `speaking 2/2`, each owning half
    /// (§F4.1 S3/S6). Held here so the two doors cannot drift apart.
    static NARRATE_QUEUE: std::cell::RefCell<runqueue::Queue> =
        std::cell::RefCell::new(runqueue::Queue::new());
    /// A scripted F4.1 narration reply for the shell's ▶ — see `set_narrate_script`.
    static NARRATE_SCRIPT: std::cell::RefCell<Vec<(u32, f64, String, String)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static NARRATE_STAGE: std::cell::RefCell<String> =
        const { std::cell::RefCell::new(String::new()) };
}

/// Put a sentence on this window's status line, the widget every other door writes, and log it too.
pub(crate) fn say_on_status(window: &adw::ApplicationWindow, said: &str) {
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(said);
    }
    ui::log_line(said);
}
