//! F4.1 ▶ Write and speak — the window seam of one narration run: what a press is refused, the bar it
//! paints, and the status lines it leaves. Split out of `window.rs`, which is past its size budget; its
//! doors are re-exported through `window`.
//!
//! The run's decisions are `crate::narrate_run`'s; what is here gathers them off this window's own state,
//! forwards them, and paints. Nothing in this block decides anything: the refusal order, which line is
//! spoken, and every sentence that ends up on screen belong to `narrate_run`. The narration call itself
//! arrives through a closure for the same reason `run_captions_pass_with_reply` exists — a test drives the
//! real route with a scripted answer and no socket.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::layout;
use crate::narrate_data;
use crate::narrate_details;
use crate::narrate_run::{
    self, JOBS, JOB_NARRATION, JOB_SPEAKING, SPEAKING_STATUS, STAGE_THINKING,
};
use crate::narrate_screen;
use crate::narration::{self, Narration};
use crate::project::Project;
use crate::run;
use crate::runqueue;
use crate::ui::narrate_run_speak as narrate_speak;
use crate::ui::window as ui;

// --- F4.1 ▶ Write and speak -----------------------------------------------------------------
//
// The run's decisions are `crate::narrate_run`'s; what is here gathers them off this window's session,
// forwards them, and paints. Nothing in this block decides anything: the refusal order, which clips the
// one call covers, whether a line is spoken, and every sentence that ends up on screen all come from
// `narrate_run`. The narration call itself arrives through a closure for the same reason
// `run_captions_pass_with_reply` exists — a test drives the real route with a scripted answer.

/// S1 alone: does this press of ▶ get refused, and with which sentence? Reads two of the three gates
/// off the project folder; busy comes from the run bar rather than the project, so it is asked through
/// [`narrate_bar_busy`] and handed to [`narrate_run::refuse`], which owns the order.
pub fn refuse_narrate_run() -> Option<String> {
    refuse_narrate_run_with_busy(narrate_bar_busy())
}

/// The same three gates with `busy` handed in. Needed because the shell's ▶ forwards HERE, after
/// `runqueue::start_run` has already put this press's own run in the bar: reading that as "a run is
/// under way" would refuse the press with S1's PAUSING sentence for the run it just opened. The
/// handler's pre-press check (`narrate_asked` above) is where busy is honestly answered.
pub fn refuse_narrate_run_with_busy(busy: bool) -> Option<String> {
    let tree = narrate_session_tree();
    let has_cut = tree
        .as_ref()
        .and_then(|tree| cut::load(tree).ok())
        .is_some_and(|cut_| !cut_.segs.is_empty());
    let has_timeline = tree
        .as_ref()
        .is_some_and(|tree| tree.session_tsv().is_file());
    narrate_run::refuse(busy, has_cut, has_timeline)
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
    // This window's own folder first. Falling back to `narrate_preview_media::session_tree()` alone
    // resolves the session from the process cwd, which is what the LAST window pinned -- so a closing
    // window with another window in front of it (or a test that built its folder elsewhere) flushed the
    // typed lines into some other project's narration.json and this window's file stayed unwritten.
    if let Some(Some(tree)) = crate::ui::window_line_step::last_session_tree() {
        return Some(tree);
    }
    crate::narrate_preview_media::session_tree()
}

/// What the narration call is, handed to the run's worker: it gets the session's folder, the clips, and a
/// way to say how far its streamed answer has got (§F4.2), and answers the written lines. `Send` because it
/// runs on the worker (§03-shell §1, "The window thread never waits").
pub(crate) type NarrateReply = Box<
    dyn FnMut(
            &layout::Tree,
            &[(f64, f64)],
            &dyn Fn(u32, usize),
        ) -> Result<Vec<narrate_run::Written>, String>
        + Send,
>;

/// F4.1 S1–S7 on this window. With no scripted reply this is the LIVE path: ▶ telephones the address
/// the Settings hold through `narrate_live`, over real HTTP, three attempts at most (§F4.2). What can wait
/// (the call, the save, the speaking pass) runs on a worker; this returns once the run is handed over, with
/// the sentence of a refusal, or empty when the run goes on (its ending lands on the status line).
pub fn press_narrate_run(window: &adw::ApplicationWindow) -> String {
    // The scripted reply lives in a thread-local so the shell's own ▶ can reach it too: the real
    // button's handler calls THIS function, and a test that wants the written-and-spoken case drives
    // the button rather than the seam. Empty means "ask for real", which is what the app does. Both are
    // read HERE, on the window thread, and travel to the worker as values.
    let reply = NARRATE_SCRIPT.with(|cell| cell.borrow().clone());
    let slow = SLOW_NARRATE_SCRIPT.with(|cell| cell.borrow().clone());
    if reply.is_empty() || slow.is_some() {
        let project = live_project();
        return run_narrate_on(
            window,
            Box::new(move |tree, bounds, progress| {
                live_narrate_reply(tree, &project, slow.clone(), bounds, progress)
            }),
        );
    }
    run_narrate_on(
        window,
        Box::new(move |_tree, clips, _progress| {
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
        }),
    )
}

/// The live narration leg: with no scripted reply, ▶ comes here (§F4.2's call over real HTTP). All the
/// gathering and driving lives in `narrate_ask`, so this file stays a shell. Runs on the worker: the
/// project and the slow script are handed in, and the streamed progress goes out through `progress`.
pub(crate) fn live_narrate_reply(
    tree: &layout::Tree,
    project: &Project,
    slow: SlowScript,
    bounds: &[(f64, f64)],
    progress: &dyn Fn(u32, usize),
) -> Result<Vec<narrate_run::Written>, String> {
    if let Some((reply, per_clip)) = slow {
        // A scripted answer that closes one clip at a time, so ⏹ pressed while the model is still
        // "thinking" is seen by the next answer instead of landing after the run already ended.
        let mut written = Vec::new();
        for (clip, at, text, emotion) in reply {
            std::thread::sleep(per_clip);
            if crate::cancel_leg::cancelled() {
                return Err("stopped while writing".into());
            }
            let Some((start, end)) = bounds.get(clip.saturating_sub(1) as usize).copied() else {
                continue;
            };
            written.push(narrate_run::Written {
                start,
                end,
                at,
                text,
                emotion,
            });
        }
        return Ok(written);
    }
    crate::narrate_ask::run(tree, project, bounds, &mut |written, total| {
        // §F4.2's streaming progress onto the shell's bar, painted by the window thread when it lands.
        progress(written, total as usize)
    })
}

/// A scripted reply that answers slowly enough for a ⏹ press to land inside it: `per_clip` is spent
/// before each clip's line, and the live cancel switch is re-read between them. Without it a headless
/// run answers in microseconds and there is nothing left to stop.
///
/// Handing this an EMPTY list does not disarm it: the door stores an `Option`, so `&[]` still leaves an
/// (empty) slow script armed and [`narrate_script_loaded`] keeps answering true.
pub fn set_narrate_script_slow(reply: &[(u32, f64, &str, &str)], per_clip: std::time::Duration) {
    SLOW_NARRATE_SCRIPT.with(|cell| {
        *cell.borrow_mut() = Some((
            reply
                .iter()
                .map(|(clip, at, text, emotion)| {
                    (*clip, *at, text.to_string(), emotion.to_string())
                })
                .collect(),
            per_clip,
        ))
    });
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
                .map(|(clip, at, text, emotion)| {
                    (*clip, *at, text.to_string(), emotion.to_string())
                })
                .collect(),
        )
    });
}

/// Whether a scripted narration reply is loaded — exported so a test can assert the button really is
/// reading the script rather than falling through to the no-model path.
pub fn narrate_script_loaded() -> bool {
    !NARRATE_SCRIPT.with(|cell| cell.borrow().is_empty())
        || SLOW_NARRATE_SCRIPT.with(|cell| cell.borrow().is_some())
}

/// F4.1's body for THIS window and the live session's project. Both doors (the seam above and the
/// shell's ▶) come through here, so the two cannot drift apart. The run's ending is applied when its
/// worker answers ([`run_narrate_with`]); what this returns is a refusal's sentence, or empty.
pub(crate) fn run_narrate_on(window: &adw::ApplicationWindow, reply: NarrateReply) -> String {
    let project = live_project();
    // A bar already holding a run means the press arriving here opened it (the shell's ▶ forwards
    // after `start_run`), so it must not be read as "busy" — see `refuse_narrate_run_with_busy`.
    let opened_by_this_press = bar_is_running();
    let reason = if opened_by_this_press {
        refuse_narrate_run_with_busy(false)
    } else {
        refuse_narrate_run()
    };
    if let Some(reason) = reason {
        // The shell's ▶ opened a run for this press and leaves its closing to this flow.
        if opened_by_this_press {
            clear_narrate_bar();
            repaint_play_button(window);
        }
        say_on_status(window, &reason);
        return reason;
    }
    // A press that came through `run::RunBar::press` opened a run before arriving here; nothing
    // headless streams to close it, so the run closes it at its own end. Three writes follow the
    // close, because the handler paints from the bar AFTER the press returned and `end_run` leaves both
    // `running` and `status` empty: put the sentence back on the label, repaint ▶ from the idle bar,
    // and redraw the progress area so the stage the run painted is not wiped by that later paint.
    let held = window.clone();
    run_narrate_with(window, &project, reply, move |said: String| {
        if opened_by_this_press {
            clear_narrate_bar();
        }
        set_status_line(&held, &said);
        if opened_by_this_press {
            repaint_play_button(&held);
        }
    })
}

/// Repaint ▶ and ⏹ from the bar's current state after a narrate run closed it, so neither button is
/// left showing the face of a run that has ended.
pub(crate) fn repaint_play_button(window: &adw::ApplicationWindow) {
    let Some(bar) = ui::RUN_BARS.with(|bars| bars.borrow().last().cloned()) else {
        return;
    };
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
    ui::RUN_BARS
        .with(|bars| bars.borrow().last().cloned())
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

/// What the run's worker reports while it runs, applied on the window thread in order.
enum NarrateNote {
    /// §F4.2's streamed progress: clips closed so far, out of how many.
    Streaming(u32, usize),
    /// S4 landed: the record as saved, how many lines were written, out of how many clips.
    Written(Narration, usize, usize),
    /// S6 opens: how many lines the speaking pass walks.
    Speaking(usize),
}

/// How the worker's part ended: the sentence for the status line, and whether the speaking job has to be
/// closed on the bar (it was opened at the start, and every ending after S5 must find it finished).
struct NarrateEnd {
    said: String,
    close_speaking: bool,
}

impl NarrateEnd {
    fn said(said: String) -> Self {
        NarrateEnd {
            said,
            close_speaking: false,
        }
    }
}

/// The shared body of both doors. Private so they cannot drift apart. Steps land in §F4.1's order, and
/// each one's wording comes from `narrate_run`, never from a string written here.
///
/// S1 to S3 run here, on the window thread: they read the window (the half-typed rows) and paint the bar.
/// The call, the save and the speaking pass run on a worker (§03-shell §1, "The window thread never
/// waits"); what they report lands through [`NarrateNote`], and `finish` gets the ending's sentence once it
/// is on the status line. The first rewrite ran the whole pass inside the ▶ handler.
pub(crate) fn run_narrate_with(
    window: &adw::ApplicationWindow,
    project: &Project,
    mut reply: NarrateReply,
    finish: impl FnOnce(String) + 'static,
) -> String {
    let Some(tree) = narrate_session_tree() else {
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        finish(said.clone());
        return said;
    };
    let cut_ = cut::load(&tree).unwrap_or_default();
    let existing = match narrate_data::load_or_previous(&tree) {
        Ok((record, false)) => record,
        // The current file is unreadable, so this run starts from the kept generation instead. Said out
        // loud because otherwise "started from one old line" and "a fresh session" look identical.
        Ok((record, true)) => {
            ui::log_line(&narrate_details::previous_fallback_log(
                &tree.narration_prev_json(),
            ));
            record
        }
        Err(why) => {
            ui::log_line(&format!("!!! narrate: {why}"));
            let said = narrate_run::failed_status("narration");
            say_on_status(window, &said);
            finish(said.clone());
            return said;
        }
    };
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

    // S2: pull half-typed rows into the record. The previous generation is NOT copied here — that waits
    // until a valid reply is in hand (see the save below), so a failed call cannot discard a good file.
    let record = pull_half_typed(window, &existing);
    // S3: save the project, open the two jobs, and put the reason on the log BEFORE the call goes out.
    if let Err(error) = crate::project::save(project, tree.dir()) {
        ui::log_line(&format!("!!! could not save the project -- {error}"));
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        finish(said.clone());
        return said;
    }
    ui::log_line(&plan.log);
    // A fresh run answers "did it pulse?" with "not yet": the flag belongs to this run, not the last.
    NARRATE_PULSED.with(|cell| cell.set(false));
    NARRATE_STAGES.with(|stages| stages.borrow_mut().clear());
    // A fresh run has written nothing yet, so a stop during the thinking stage names `narration`.
    NARRATE_WROTE.set(false);
    // The bar must stop pulsing the moment this run paints its own stage: the shell's ▶ stood a pulse
    // for the run it opened, and the thinking stage overwrites that face, so the needle is ours now.
    crate::ui::shell_beats::stop_pulse();
    // The WINDOW's own queue is the bar's number from here on: the shell painted it when ▶ opened the
    // run, `set_narrate_stage` writes `run-progress`' fraction from it, and `arm_pulse` below breaks
    // itself on it — so the run, the widget and the timer all read one fraction instead of three.
    // `NARRATE_QUEUE` stays as the fallback for a door with no shell (a snapshot, a bare seam call).
    let stage_queue = live_stage_queue(window);
    stage_queue.borrow_mut().reset();
    stage_queue.borrow_mut().phase(0.0, 0.5);
    stage_queue.borrow_mut().job(0, JOB_NARRATION, 1, JOBS);
    stage_queue
        .borrow_mut()
        .push(0, plan.clips.len().max(1), "clip");
    stage_queue.borrow_mut().prog(0, 0.0, STAGE_THINKING);
    stage_queue.borrow_mut().job(1, JOB_SPEAKING, 2, JOBS);
    NARRATE_QUEUE.with(|queue| *queue.borrow_mut() = stage_queue.borrow().clone());
    // S3: the bar opens at "thinking about it" and PULSES — nothing has closed yet, so the queue's
    // reading is 0.0 and only the shell's idle pulse can show the wait (§F4.1 S3: "pulses every 150 ms
    // until the first clip closes").
    set_narrate_stage(window, STAGE_THINKING, true);

    let clips = plan.clips.clone();
    let captions_only = plan.captions_only;
    let on_note = {
        let window = window.clone();
        move |note: NarrateNote| match note {
            NarrateNote::Streaming(written, of) => show_narrate_progress(&window, written, of),
            NarrateNote::Written(record, written, of) => {
                // S6's half-way fact the ⏹ handler can ask for: whether this run's first job closed, i.e.
                // whether lines were written before the stop (§F4.1 S7).
                NARRATE_WROTE.set(written > 0);
                // The lines written are the page's rows now: published and drawn at once, not only on the next
                // tab switch (the first rewrite updated the held narration alone, so ▶ looked like it wrote
                // nothing until the page was rebuilt).
                crate::ui::narrate_page::publish_lines(record.entries.clone());
                if let Some(held) = ui::HELD_NARRATION.with(|held| held.borrow().last().cloned()) {
                    *held.borrow_mut() = record;
                }
                crate::ui::narrate_page::refresh(&window);
                // The bar's stage follows the clips as they close; the final count lands in one step.
                set_narrate_stage(&window, &narrate_run::writing(written, of), false);
                let stage_queue = live_stage_queue(&window);
                stage_queue.borrow_mut().done(0, 1.0);
                NARRATE_QUEUE.with(|queue| *queue.borrow_mut() = stage_queue.borrow().clone());
            }
            NarrateNote::Speaking(lines) => {
                say_on_status(&window, SPEAKING_STATUS);
                live_stage_queue(&window)
                    .borrow_mut()
                    .push(1, lines.max(1), "line");
            }
        }
    };
    let done = {
        let window = window.clone();
        move |end: NarrateEnd| {
            if end.close_speaking {
                // Track 1 owns the second half of the phase (`phase(0.0, 0.5)`), so closing at full share
                // fills the bar; left open, it sits on `speaking 2/2` after a run that ended.
                let stage_queue = live_stage_queue(&window);
                stage_queue.borrow_mut().done(1, 1.0);
                NARRATE_QUEUE.with(|queue| *queue.borrow_mut() = stage_queue.borrow().clone());
            }
            say_on_status(&window, &end.said);
            finish(end.said);
        }
    };
    crate::ui::background::run_with_notes(
        "narrate",
        move |note: &dyn Fn(NarrateNote)| {
            // The run's `llm/` page is this thread's (§7 keeps one per thread).
            crate::exchange_leg::note_run_started();
            let end = narrate_on_worker(
                &tree,
                record,
                &mut reply,
                &clips,
                captions_only,
                &voice,
                note,
            );
            crate::exchange_leg::note_run_ended();
            end
        },
        on_note,
        done,
    );
    String::new()
}

/// S4 to S7 on the run's worker: the call, the save, the speaking pass. Reads no window state: what it
/// needs arrives as values, and what the window must show leaves as [`NarrateNote`]s.
#[allow(clippy::too_many_arguments)] // the run's whole hand-over: folder, record, call, clips, flag, voice, notes
fn narrate_on_worker(
    tree: &layout::Tree,
    record: Narration,
    reply: &mut NarrateReply,
    clips: &[(f64, f64)],
    captions_only: bool,
    voice: &str,
    note: &dyn Fn(NarrateNote),
) -> NarrateEnd {
    let written = match reply(tree, clips, &|written, of| {
        note(NarrateNote::Streaming(written, of))
    }) {
        Ok(lines) => lines,
        Err(error) => {
            ui::log_line(&format!("!!! narrate: the call failed -- {error}"));
            // ⏹ caught the call in flight: the ending names the stage, not a generic failure (§S7).
            return NarrateEnd::said(if crate::cancel_leg::cancelled() {
                narrate_run::stopped_status(JOB_NARRATION)
            } else {
                narrate_run::failed_status("narration")
            });
        }
    };
    // ⏹ landed while the model was answering: nothing is saved and nothing is spoken (§S7's
    // "<stage> stopped" — the stage is still `narration`, the speaking job has not opened).
    if crate::cancel_leg::cancelled() {
        return NarrateEnd::said(narrate_run::stopped_status(JOB_NARRATION));
    }
    if written.is_empty() {
        // No line came back: nothing is saved, nothing is spoken, and the run says so plainly.
        return NarrateEnd::said(narrate_run::Outcome::NothingWritten.status());
    }

    // S4: fold the written lines in (the silent list survives inside `fold_written`), save, publish.
    let record = narrate_run::fold_written(&record, &written);
    // The copy of what was there goes to `narrate/narration.prev.json` only now that a valid reply is in
    // hand (§07-narrate#3-data: it is the PREVIOUS generation). A call that failed, answered nothing or was
    // stopped never reaches here, so it cannot throw away a good file on the way to producing nothing.
    if let Err(why) = narrate_data::keep_previous(tree) {
        ui::log_line(&format!(
            "!!! could not keep the previous narration -- {why}"
        ));
        return NarrateEnd::said(narrate_run::failed_status("narration"));
    }
    if tree.narration_prev_json().is_file() {
        ui::log_line(&narrate_details::previous_kept_log(
            &tree.narration_prev_json().display().to_string(),
        ));
    }
    if let Err(error) = narration::save(&record, tree) {
        ui::log_line(&format!("!!! could not write narration.json -- {error}"));
        return NarrateEnd::said(narrate_run::failed_status("narration"));
    }
    ui::log_line(&narrate_run::written_log(written.len()));
    note(NarrateNote::Written(
        record.clone(),
        written.len(),
        clips.len(),
    ));

    // S5: a captions-only voice stops here: the lines exist and nothing is spoken. The speaking job was
    // opened with the run, so it is closed even though no line will ever be synthesized (§6).
    if captions_only {
        return NarrateEnd {
            said: narrate_run::captions_only_done(written.len()),
            close_speaking: true,
        };
    }

    // S6: the speaking pass, over the real wire. `narrate_run` decides WHICH lines get synthesized
    // (blank is a deliberate silence, a take already in narrate/tts/ costs nothing); this half asks
    // audio.cpp for each of them through `speak_leg::speak`, the same door the row's ▶ and the
    // preview's walk use — so "N line(s) spoken" can never be reported for lines nobody dialled.
    let to_speak = crate::narration::lines_to_speak(&record.entries);
    note(NarrateNote::Speaking(to_speak.len()));
    let language = crate::project::load(tree.dir())
        .map(|opened| opened.policy.language.value)
        .unwrap_or_default();
    let run = narrate_speak::speak_lines(
        &to_speak,
        |entry| {
            let key = narration::key_for(entry, Some(voice));
            tree.tts_wav(&narration::tts_file(&key)).is_file()
        },
        |entry| narrate_speak::dial_line(tree, voice, &language, entry),
    );
    for line in run
        .lines
        .iter()
        .filter_map(|line| narrate_speak::refused(line))
    {
        ui::log_line(&format!("!!! narrate: {line}"));
    }
    // ⏹ cut the pass short between two lines: the summary says what got done, and the ending names the
    // stage that stopped rather than claiming a finished narration (§S7).
    if run.stopped {
        ui::log_line(&narrate_speak::stopped_speaking_log(&run));
        return NarrateEnd {
            said: narrate_run::stopped_status(JOB_SPEAKING),
            close_speaking: true,
        };
    }
    ui::log_line(&narrate_run::spoken_log(run.spoken, run.cached));
    // S7: the ending goes on the STATUS LINE, which is where the prototype never put it.
    NarrateEnd {
        said: narrate_run::Outcome::Spoken {
            written: written.len(),
            spoken: run.spoken,
        }
        .status(),
        close_speaking: true,
    }
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
        let Some(found) =
            ui::find_widget_by_name(window.upcast_ref(), &format!("line-text-{index}"))
        else {
            continue;
        };
        let Ok(entry_box) = found.downcast::<gtk::TextView>() else {
            continue;
        };
        let typed = crate::ui::narrate_row::box_text(&entry_box);
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
/// Paint the narration run's stage on the bar: the fraction its queue has earned, the stage's own
/// words, and the tooltip that keeps counting while the stage says one short thing.
///
/// `pulse` stands F0.1/§2's 150 ms idle pulse (`runqueue::PROGRESS_PULSE`, the shell's one timer —
/// [`crate::ui::shell_beats::arm_pulse`], the same call the ▶ handler makes for a Prepare run) instead
/// of setting a fraction: while the run is thinking, the queue's reading is 0.0 by design — nothing has
/// closed — so a bar that is only given a number sits still at zero exactly as long as the model takes.
/// §F4.1 S3 asks for that wait to be visible as a pulse, and the timer ends itself the moment
/// [`crate::ui::shell_beats::should_pulse`] says there is a countable fraction, which is when the next
/// call here arrives with `pulse` false and hands the needle back.
pub(crate) fn set_narrate_stage(window: &adw::ApplicationWindow, stage: &str, pulse: bool) {
    NARRATE_STAGE.with(|cell| cell.replace(stage.to_string()));
    NARRATE_STAGES.with(|stages| stages.borrow_mut().push(stage.to_string()));
    let stage_queue = live_stage_queue(window);
    let fraction = stage_queue.borrow().fraction();
    let tip = stage_queue.borrow().text_and_tip().1;
    if let Some(bar) = ui::find_widget_by_name(window.upcast_ref(), "run-progress")
        .and_then(|w| w.downcast::<gtk::ProgressBar>().ok())
    {
        let pulsed = match (pulse, live_run_handles()) {
            (true, Some((run_bar, _))) => {
                // §2's other half: a run that just painted a stage owns the needle, so any pulse the
                // ▶ handler left standing goes down before this stage's own beat replaces it. One
                // timer at a time is what keeps an idle bar still (F0.1).
                crate::ui::shell_beats::stop_pulse();
                // `arm_pulse`'s beat must watch the SAME fraction this run advances, so the queue
                // handed over is [`live_stage_queue`]'s — the window's — not the timer's own copy.
                let queue = live_stage_queue(window);
                crate::ui::shell_beats::arm_pulse(&bar, &run_bar, &queue);
                true
            }
            // Nothing to pulse on: no window handles (a door with no shell) or the run already closed.
            // The fraction still goes on the bar, so the stage is never invisible.
            (true, None) => {
                crate::ui::shell_beats::stop_pulse();
                bar.set_fraction(fraction.clamp(0.0, 1.0));
                false
            }
            (false, _) => {
                // The counted stage: take the beat down here rather than waiting for its next tick, so
                // the bar cannot pulse one more time after the count landed.
                crate::ui::shell_beats::stop_pulse();
                bar.set_fraction(fraction.clamp(0.0, 1.0));
                false
            }
        };
        NARRATE_PULSED.with(|cell| cell.set(pulsed || cell.get()));
        bar.set_text(Some(stage));
        bar.set_show_text(true);
        if !tip.is_empty() {
            bar.set_tooltip_text(Some(&tip));
        }
    }
}

/// Whether the narration run's bar pulsed at any point in this run — the fact §F4.1 S3 asks for, which
/// GTK gives nobody a way to read back off a `ProgressBar`. Set by [`set_narrate_stage`] when it stands
/// the pulse timer; public so a check asserts the wait was shown rather than trusting a comment.
pub fn narrate_pulsed() -> bool {
    NARRATE_PULSED.with(|cell| cell.get())
}

/// The queue this run drives the bar through: the window's own (`ui::run_queue`) when a shell is open,
/// so `arm_pulse`, `paint_progress` and [`set_narrate_stage`] read one fraction; the private
/// [`NARRATE_QUEUE`] copy when the door was called with no shell behind it.
fn live_stage_queue(window: &adw::ApplicationWindow) -> Rc<RefCell<runqueue::Queue>> {
    ui::run_queue(window).unwrap_or_else(|| {
        Rc::new(RefCell::new(
            NARRATE_QUEUE.with(|queue| queue.borrow().clone()),
        ))
    })
}

/// A `glib`-free handle pair the run's bar and queue travel in.
type RunHandles = (Rc<RefCell<run::RunBar>>, Rc<RefCell<runqueue::Queue>>);

/// This window's live run bar and queue, the same newest-slot rule [`crate::ui::run_close`] uses to find
/// the widgets a close repaints. `None` when this thread has no built window.
fn live_run_handles() -> Option<RunHandles> {
    let bar = ui::RUN_BARS.with(|bars| bars.borrow().last().cloned())?;
    let queue = ui::WINDOW_QUEUE.with(|queues| queues.borrow().last().cloned())?;
    Some((bar, queue))
}

/// The stage text this run last put on the bar.
pub fn narrate_stage() -> String {
    NARRATE_STAGE.with(|cell| cell.borrow().clone())
}

/// The stages this run put on the bar, in order — including the ones a later stage painted over, which
/// is the only way a check can see "thinking about it" came FIRST. Appended by [`set_narrate_stage`]
/// and cleared when a run opens, so it is this run's history and not the process's.
pub fn narrate_stages() -> Vec<String> {
    NARRATE_STAGES.with(|stages| stages.borrow().clone())
}

/// Whether this window's narrate run got past the writing job: ⏹ reads this to name the stage it
/// caught (`narration stopped` before any line landed, `speaking stopped` after one did, §F4.1 S7).
pub fn narrate_wrote() -> bool {
    NARRATE_WROTE.get()
}

/// Stream one step of the writing job onto the bar: `written` of `of` clips have closed.
///
/// This ADVANCES the queue the run already opened (job 0, `narration 1/2`) rather than rebuilding it the
/// way [`show_narrate_stage`] does for a bare face — a rebuild would drop the speaking job the run has
/// already listed. The monotonic rule is not repeated here: `narrate_call::drive` clamps every reading
/// through [`crate::narrate_pass::progress_forward`] before calling back, so a retry resumes at the count
/// the first attempt reached instead of falling the bar back to zero.
pub fn show_narrate_progress(window: &adw::ApplicationWindow, written: u32, of: usize) {
    let stage = narrate_run::writing(written as usize, of);
    let stage_queue = live_stage_queue(window);
    stage_queue
        .borrow_mut()
        .prog(0, written as f64 / of.max(1) as f64, &stage);
    NARRATE_QUEUE.with(|queue| *queue.borrow_mut() = stage_queue.borrow().clone());
    set_narrate_stage(window, &stage, false);
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
        queue.prog(
            0,
            if of == 0 {
                0.0
            } else {
                written as f64 / of as f64
            },
            &narrate_run::writing(written, of),
        );
        queue.job(1, JOB_SPEAKING, 2, JOBS);
    });
    set_narrate_stage(window, &narrate_run::writing(written, of), false);
}

/// One line of a scripted F4.1 reply: `(clip number from 1, clip-relative seconds, text, emotion)`.
pub type ScriptedLine = (u32, f64, String, String);

/// A slow scripted reply — its lines and the time spent before each one, see [`set_narrate_script_slow`].
pub type SlowScript = Option<(Vec<ScriptedLine>, std::time::Duration)>;

thread_local! {
    /// The narration run's own view of the bar: `narration 1/2` then `speaking 2/2`, each owning half
    /// (§F4.1 S3/S6). Held here so the two doors cannot drift apart.
    static NARRATE_QUEUE: std::cell::RefCell<runqueue::Queue> =
        std::cell::RefCell::new(runqueue::Queue::new());
    /// A scripted F4.1 narration reply for the shell's ▶ — see `set_narrate_script`.
    static NARRATE_SCRIPT: std::cell::RefCell<Vec<ScriptedLine>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// A slow scripted reply — see [`set_narrate_script_slow`].
    static SLOW_NARRATE_SCRIPT: std::cell::RefCell<SlowScript> = const { std::cell::RefCell::new(None) };
    static NARRATE_STAGE: std::cell::RefCell<String> =
        const { std::cell::RefCell::new(String::new()) };
    /// Whether this run pulsed the bar while thinking — see [`narrate_pulsed`].
    static NARRATE_PULSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Whether the last narrate run reached its speaking job (its first job closed). ⏹ reads this to say
    /// `narration stopped` or `speaking stopped` (§F4.1 S7).
    static NARRATE_WROTE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Every stage this run painted, oldest first — see [`narrate_stages`].
    static NARRATE_STAGES: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Put a sentence on this window's status line, the widget every other door writes, and log it too.
pub(crate) fn say_on_status(window: &adw::ApplicationWindow, said: &str) {
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(said);
    }
    ui::log_line(said);
}
