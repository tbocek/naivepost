//! F4.1 ▶ Write and speak — the window seam of one narration run: what a press is refused, the bar it
//! paints, and the status lines it leaves. The run's decisions and sentences are `crate::narrate_run`'s;
//! the narration call arrives through a closure so a test drives the real route with a scripted answer.

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
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

/// S1 alone: does this press of ▶ get refused, and with which sentence? Busy comes from the run bar, via
/// [`narrate_bar_busy`], and goes to [`narrate_run::refuse`], which owns the order.
pub fn refuse_narrate_run() -> Option<String> {
    refuse_narrate_run_with_busy(narrate_bar_busy())
}

/// The same three gates with `busy` handed in: the shell's ▶ forwards HERE after `runqueue::start_run` has
/// put this press's own run in the bar, which must not read as "a run is under way" (S1's PAUSING sentence).
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

/// Whether this window's run bar counts as busy, as [`run::controls`] answers it for the button face.
pub(crate) fn narrate_bar_busy() -> bool {
    WINDOW_PAGE
        .with(|WindowPage { run_bars: bars, .. }| bars.borrow().last().cloned())
        .is_some_and(|bar| {
            let bar = bar.borrow();
            run::controls(&bar.running, None).icon == run::PAUSE_ICON
        })
}

/// This session's project folder, resolved as every other flow does. `None` when no project is open.
pub fn narrate_session_tree() -> Option<layout::Tree> {
    // This window's own folder first: `narrate_preview_media::session_tree()` alone resolves from the process
    // cwd (what the LAST window pinned), so typed lines could be flushed into another project's narration.json.
    if let Some(Some(tree)) = crate::ui::window_line_step::last_session_tree() {
        return Some(tree);
    }
    crate::narrate_preview_media::session_tree()
}

/// What the narration call is, handed to the run's worker: the session's folder, the clips, and a way to
/// report streamed progress (§F4.2). `Send` because it runs on the worker (§03-shell §1).
pub(crate) type NarrateReply = Box<
    dyn FnMut(
            &layout::Tree,
            &[(f64, f64)],
            &dyn Fn(u32, usize),
        ) -> Result<Vec<narrate_run::Written>, String>
        + Send,
>;

/// F4.1 S1–S7 on this window. With no scripted reply this is the LIVE path (real HTTP, three attempts at
/// most, §F4.2). The call, the save and the speaking pass run on a worker; this returns the sentence of a
/// refusal, or empty when the run goes on.
pub fn press_narrate_run(window: &adw::ApplicationWindow) -> String {
    // The scripted reply is thread-local so the shell's own ▶ reaches it too. Empty means "ask for real".
    // Both are read HERE, on the window thread, and travel to the worker as values.
    let reply = WINDOW_PAGE.with(
        |WindowPage {
             narrate_script: cell,
             ..
         }| cell.borrow().clone(),
    );
    let slow = WINDOW_PAGE.with(
        |WindowPage {
             slow_narrate_script: cell,
             ..
         }| cell.borrow().clone(),
    );
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

/// The live narration leg: with no scripted reply, ▶ comes here (§F4.2's call over real HTTP). Runs on the
/// worker; streamed progress goes out through `progress`.
pub(crate) fn live_narrate_reply(
    tree: &layout::Tree,
    project: &Project,
    slow: SlowScript,
    bounds: &[(f64, f64)],
    progress: &dyn Fn(u32, usize),
) -> Result<Vec<narrate_run::Written>, String> {
    if let Some((reply, per_clip)) = slow {
        // A scripted answer that closes one clip at a time, so ⏹ during "thinking" is seen by the next answer.
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
        progress(written, total as usize)
    })
}

/// A scripted reply slow enough for a ⏹ press to land inside it: `per_clip` is spent before each clip's
/// line, and the live cancel switch is re-read between them.
///
/// An EMPTY list does not disarm it: the door stores an `Option`, so `&[]` still leaves an empty slow
/// script armed and [`narrate_script_loaded`] keeps answering true.
pub fn set_narrate_script_slow(reply: &[(u32, f64, &str, &str)], per_clip: std::time::Duration) {
    WINDOW_PAGE.with(
        |WindowPage {
             slow_narrate_script: cell,
             ..
         }| {
            *cell.borrow_mut() = Some((
                reply
                    .iter()
                    .map(|(clip, at, text, emotion)| {
                        (*clip, *at, text.to_string(), emotion.to_string())
                    })
                    .collect(),
                per_clip,
            ))
        },
    );
}

/// One narration reply handed to the shell's ▶ by a test: `(clip number from 1, clip-relative second, text,
/// emotion)`. The handler reads it instead of telephoning F4.2's endpoint; an empty slice restores the
/// no-model behaviour.
pub fn set_narrate_script(reply: &[(u32, f64, &str, &str)]) {
    WINDOW_PAGE.with(
        |WindowPage {
             narrate_script: cell,
             ..
         }| {
            cell.replace(
                reply
                    .iter()
                    .map(|(clip, at, text, emotion)| {
                        (*clip, *at, text.to_string(), emotion.to_string())
                    })
                    .collect(),
            )
        },
    );
}

/// Whether a scripted narration reply is loaded, so a test can assert ▶ reads the script.
pub fn narrate_script_loaded() -> bool {
    !WINDOW_PAGE.with(
        |WindowPage {
             narrate_script: cell,
             ..
         }| cell.borrow().is_empty(),
    ) || WINDOW_PAGE.with(
        |WindowPage {
             slow_narrate_script: cell,
             ..
         }| cell.borrow().is_some(),
    )
}

/// F4.1's body for THIS window and the live session's project; both doors come through here. The run's
/// ending is applied when its worker answers ([`run_narrate_with`]); this returns a refusal's sentence, or empty.
pub(crate) fn run_narrate_on(window: &adw::ApplicationWindow, reply: NarrateReply) -> String {
    let project = live_project();
    // A bar already holding a run means this press opened it (shell's ▶ forwards after `start_run`): not "busy".
    let opened_by_this_press = bar_is_running();
    let reason = if opened_by_this_press {
        refuse_narrate_run_with_busy(false)
    } else {
        refuse_narrate_run()
    };
    if let Some(reason) = reason {
        if opened_by_this_press {
            crate::ui::run_close::close_window_run();
            repaint_play_button(window);
        }
        say_on_status(window, &reason);
        return reason;
    }
    // A press through `run::RunBar::press` opened a run; nothing headless streams to close it, so close it
    // here. Three writes follow, because the handler paints from the bar AFTER the press returned and `end_run`
    // leaves `running` and `status` empty: restore the label, repaint ▶ from the idle bar, redraw the progress area.
    let held = window.clone();
    run_narrate_with(window, &project, reply, move |said: String| {
        if opened_by_this_press {
            crate::ui::run_close::close_window_run();
        }
        set_status_line(&held, &said);
        if opened_by_this_press {
            repaint_play_button(&held);
        }
    })
}

/// Repaint ▶ and ⏹ from the bar's current state after a narrate run closed it.
pub(crate) fn repaint_play_button(window: &adw::ApplicationWindow) {
    let Some(bar) =
        WINDOW_PAGE.with(|WindowPage { run_bars: bars, .. }| bars.borrow().last().cloned())
    else {
        return;
    };
    let drawn = run::controls(&bar.borrow().running, None);
    if let Some(play) = ui::play_button(window) {
        play.set_icon_name(drawn.icon);
        play.set_tooltip_text(Some(drawn.tooltip));
    }
    // ⏹'s sensitivity comes from the same call: with the run closed it greys out again.
    if let Some(stop_) = ui::stop_button(window) {
        stop_.set_sensitive(drawn.stop_sensitive);
    }
}

/// Whether this window's run bar currently holds a run, the slot [`run_close::close_window_run`] empties.
pub(crate) fn bar_is_running() -> bool {
    WINDOW_PAGE
        .with(|WindowPage { run_bars: bars, .. }| bars.borrow().last().cloned())
        .is_some_and(|bar| bar.borrow().running.is_some())
}

/// Write one sentence on this window's status line without logging it (the run already logged it).
pub fn set_status_line(window: &adw::ApplicationWindow, said: &str) {
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(said);
    }
}

/// The live project this window runs under: the newest slot, as `window_policy::live_session_policy` reads it.
/// Public to the crate so the Produce page's F5.1 seam reads the session rather than a widget copy.
pub(crate) fn live_project() -> Project {
    WINDOW_PAGE
        .with(
            |WindowPage {
                 play_session: slots,
                 ..
             }| slots.borrow().last().cloned(),
        )
        .map(|held| held.borrow().clone())
        .unwrap_or_default()
}

enum NarrateNote {
    Streaming(u32, usize),
    Written(Narration, usize, usize),
    Speaking(usize),
}

/// How the worker's part ended: the status-line sentence, and whether the speaking job has to be closed
/// on the bar (opened at the start, so every ending after S5 must find it finished).
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

/// The shared body of both doors. Steps land in §F4.1's order; each wording comes from `narrate_run`.
///
/// S1 to S3 run on the window thread (they read the half-typed rows and paint the bar). The call, the save
/// and the speaking pass run on a worker (§03-shell §1); what they report lands through [`NarrateNote`],
/// and `finish` gets the ending's sentence once it is on the status line.
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
        // The current file is unreadable, so this run starts from the kept generation; said out loud so it does
        // not look like a fresh session.
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
    // The clips are the VISIBLE SEGMENTS in play order, NOT `cut_play::kept_runs`: that MERGES touching
    // segments for preloading, so clip numbers above 1 would fall outside the batch.
    let mut clips: Vec<(f64, f64)> = cut_
        .segs
        .iter()
        .filter(|seg| seg.e > seg.s)
        .map(|seg| (seg.s, seg.e))
        .collect();
    clips.sort_by(|a, b| a.0.total_cmp(&b.0));
    let plan = narrate_run::plan_run(rewrite, &clips, &existing, &voice);

    // S2: pull half-typed rows into the record. The previous generation is copied only once a valid reply
    // is in hand (see the save below), so a failed call cannot discard a good file.
    let record = pull_half_typed(window, &existing);
    // S3: save the project, open the two jobs, and log the reason BEFORE the call goes out.
    if let Err(error) = crate::project::save(project, tree.dir()) {
        ui::log_line(&format!("!!! could not save the project -- {error}"));
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        finish(said.clone());
        return said;
    }
    ui::log_line(&plan.log);
    // The flag belongs to this run, not the last.
    WINDOW_PAGE.with(
        |WindowPage {
             narrate_pulsed: cell,
             ..
         }| cell.set(false),
    );
    WINDOW_PAGE.with(
        |WindowPage {
             narrate_stages: stages,
             ..
         }| stages.borrow_mut().clear(),
    );
    // A fresh run has written nothing yet, so a stop during thinking names `narration`.
    WINDOW_PAGE.with(|page| page.narrate_wrote.set(false));
    // The shell's ▶ stood a pulse for the run it opened; the thinking stage overwrites that face, so stop it.
    crate::ui::shell_beats::stop_pulse();
    // The WINDOW's queue is the bar's number from here on (`set_narrate_stage`, `arm_pulse` and the run all read
    // one fraction). `NARRATE_QUEUE` stays as the fallback for a door with no shell (a snapshot, a bare seam call).
    let stage_queue = live_stage_queue(window);
    stage_queue.borrow_mut().reset();
    stage_queue.borrow_mut().phase(0.0, 0.5);
    stage_queue.borrow_mut().job(0, JOB_NARRATION, 1, JOBS);
    stage_queue
        .borrow_mut()
        .push(0, plan.clips.len().max(1), "clip");
    stage_queue.borrow_mut().prog(0, 0.0, STAGE_THINKING);
    stage_queue.borrow_mut().job(1, JOB_SPEAKING, 2, JOBS);
    WINDOW_PAGE.with(
        |WindowPage {
             narrate_queue: queue,
             ..
         }| *queue.borrow_mut() = stage_queue.borrow().clone(),
    );
    // S3: the bar opens at "thinking about it" and PULSES (the queue reads 0.0 until a clip closes; §F4.1 S3).
    set_narrate_stage(window, STAGE_THINKING, true);

    let clips = plan.clips.clone();
    let captions_only = plan.captions_only;
    let on_note = {
        let window = window.clone();
        move |note: NarrateNote| match note {
            NarrateNote::Streaming(written, of) => show_narrate_progress(&window, written, of),
            NarrateNote::Written(record, written, of) => {
                // Whether this run's first job closed, i.e. whether lines were written before a stop (§F4.1 S7).
                WINDOW_PAGE.with(|page| page.narrate_wrote.set(written > 0));
                crate::ui::narrate_page::publish_lines(record.entries.clone());
                if let Some(held) = WINDOW_PAGE.with(
                    |WindowPage {
                         held_narration: held,
                         ..
                     }| held.borrow().last().cloned(),
                ) {
                    *held.borrow_mut() = record;
                }
                crate::ui::narrate_page::refresh(&window);
                // The bar's stage follows the clips as they close.
                set_narrate_stage(&window, &narrate_run::writing(written, of), false);
                let stage_queue = live_stage_queue(&window);
                stage_queue.borrow_mut().done(0, 1.0);
                WINDOW_PAGE.with(
                    |WindowPage {
                         narrate_queue: queue,
                         ..
                     }| *queue.borrow_mut() = stage_queue.borrow().clone(),
                );
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
                // Track 1 owns the second half of the phase (`phase(0.0, 0.5)`); closing at full share fills the bar.
                let stage_queue = live_stage_queue(&window);
                stage_queue.borrow_mut().done(1, 1.0);
                WINDOW_PAGE.with(
                    |WindowPage {
                         narrate_queue: queue,
                         ..
                     }| *queue.borrow_mut() = stage_queue.borrow().clone(),
                );
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

/// S4 to S7 on the run's worker. Reads no window state: inputs arrive as values, and what the window must
/// show leaves as [`NarrateNote`]s.
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
    // ⏹ landed while the model was answering: nothing saved or spoken (§S7).
    if crate::cancel_leg::cancelled() {
        return NarrateEnd::said(narrate_run::stopped_status(JOB_NARRATION));
    }
    if written.is_empty() {
        return NarrateEnd::said(narrate_run::Outcome::NothingWritten.status());
    }

    // S4: fold the written lines in (the silent list survives inside `fold_written`), save, publish.
    let record = narrate_run::fold_written(&record, &written);
    // The previous generation is copied to `narrate/narration.prev.json` only now that a valid reply is in
    // hand (§07-narrate#3-data), so a failed or stopped call cannot throw away a good file.
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

    // S5: a captions-only voice stops here, but the speaking job (opened with the run) is still closed (§6).
    if captions_only {
        return NarrateEnd {
            said: narrate_run::captions_only_done(written.len()),
            close_speaking: true,
        };
    }

    // S6: the speaking pass. `narrate_run` decides WHICH lines get synthesized; this asks audio.cpp through
    // `speak_leg::speak`, the door the row's ▶ and the preview's walk use.
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
    // ⏹ cut the pass short: the ending names the stage that stopped (§S7).
    if run.stopped {
        ui::log_line(&narrate_speak::stopped_speaking_log(&run));
        return NarrateEnd {
            said: narrate_run::stopped_status(JOB_SPEAKING),
            close_speaking: true,
        };
    }
    ui::log_line(&narrate_run::spoken_log(run.spoken, run.cached));
    NarrateEnd {
        said: narrate_run::Outcome::Spoken {
            written: written.len(),
            spoken: run.spoken,
        }
        .status(),
        close_speaking: true,
    }
}

/// S2: read the row boxes back into the record, so a half-typed line still reaches the file. Empty boxes are
/// left as they were (clearing a box is not deleting a line; F4.7's 🗑 is). Rows are read by name
/// (`line-text-<n>`) because `refresh` rebuilds them whole. The box is write-only: nothing prints a second
/// back into it, the time field owns the number.
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

/// Paint the narration run's stage on the bar: the fraction its queue has earned, the stage's own words, and
/// the tooltip that keeps counting.
///
/// `pulse` stands the 150 ms idle pulse (`runqueue::PROGRESS_PULSE`, [`crate::ui::shell_beats::arm_pulse`])
/// instead of a fraction: while thinking the queue reads 0.0 by design, so a bar given only a number sits
/// still (§F4.1 S3). The timer ends itself once [`crate::ui::shell_beats::should_pulse`] says there is a
/// countable fraction.
pub(crate) fn set_narrate_stage(window: &adw::ApplicationWindow, stage: &str, pulse: bool) {
    WINDOW_PAGE.with(
        |WindowPage {
             narrate_stage: cell,
             ..
         }| cell.replace(stage.to_string()),
    );
    WINDOW_PAGE.with(
        |WindowPage {
             narrate_stages: stages,
             ..
         }| stages.borrow_mut().push(stage.to_string()),
    );
    let stage_queue = live_stage_queue(window);
    let fraction = stage_queue.borrow().fraction();
    let tip = stage_queue.borrow().text_and_tip().1;
    if let Some(bar) = ui::find_widget_by_name(window.upcast_ref(), "run-progress")
        .and_then(|w| w.downcast::<gtk::ProgressBar>().ok())
    {
        let pulsed = match (pulse, live_run_handles()) {
            (true, Some((run_bar, _))) => {
                // A run that painted a stage owns the needle: stop any pulse the ▶ handler left (one timer at a time, F0.1).
                crate::ui::shell_beats::stop_pulse();
                // `arm_pulse`'s beat must watch the SAME fraction, so hand it [`live_stage_queue`]'s (the window's).
                let queue = live_stage_queue(window);
                crate::ui::shell_beats::arm_pulse(&bar, &run_bar, &queue);
                true
            }
            // No window handles or the run already closed: the fraction still goes on the bar.
            (true, None) => {
                crate::ui::shell_beats::stop_pulse();
                bar.set_fraction(fraction.clamp(0.0, 1.0));
                false
            }
            (false, _) => {
                // Take the beat down here so the bar cannot pulse once more after the count landed.
                crate::ui::shell_beats::stop_pulse();
                bar.set_fraction(fraction.clamp(0.0, 1.0));
                false
            }
        };
        WINDOW_PAGE.with(
            |WindowPage {
                 narrate_pulsed: cell,
                 ..
             }| cell.set(pulsed || cell.get()),
        );
        bar.set_text(Some(stage));
        bar.set_show_text(true);
        if !tip.is_empty() {
            bar.set_tooltip_text(Some(&tip));
        }
    }
}

/// Whether the run's bar pulsed at any point, the fact §F4.1 S3 asks for and GTK cannot read back.
/// Set by [`set_narrate_stage`] when it stands the pulse timer.
pub fn narrate_pulsed() -> bool {
    WINDOW_PAGE.with(
        |WindowPage {
             narrate_pulsed: cell,
             ..
         }| cell.get(),
    )
}

/// The queue this run drives the bar through: the window's own (`ui::run_queue`) when a shell is open,
/// else the private [`NARRATE_QUEUE`] copy.
fn live_stage_queue(window: &adw::ApplicationWindow) -> Rc<RefCell<runqueue::Queue>> {
    ui::run_queue(window).unwrap_or_else(|| {
        Rc::new(RefCell::new(WINDOW_PAGE.with(
            |WindowPage {
                 narrate_queue: queue,
                 ..
             }| queue.borrow().clone(),
        )))
    })
}

type RunHandles = (Rc<RefCell<run::RunBar>>, Rc<RefCell<runqueue::Queue>>);

/// This window's live run bar and queue (newest-slot rule of [`crate::ui::run_close`]); `None` with no built window.
fn live_run_handles() -> Option<RunHandles> {
    let bar =
        WINDOW_PAGE.with(|WindowPage { run_bars: bars, .. }| bars.borrow().last().cloned())?;
    let queue = WINDOW_PAGE.with(
        |WindowPage {
             window_queue: queues,
             ..
         }| queues.borrow().last().cloned(),
    )?;
    Some((bar, queue))
}

pub fn narrate_stage() -> String {
    WINDOW_PAGE.with(
        |WindowPage {
             narrate_stage: cell,
             ..
         }| cell.borrow().clone(),
    )
}

/// The stages this run put on the bar, in order, including ones a later stage painted over (so a check can
/// see "thinking about it" came FIRST). Cleared when a run opens.
pub fn narrate_stages() -> Vec<String> {
    WINDOW_PAGE.with(
        |WindowPage {
             narrate_stages: stages,
             ..
         }| stages.borrow().clone(),
    )
}

/// Whether this run got past the writing job: ⏹ reads it to name the stage it caught (§F4.1 S7).
pub fn narrate_wrote() -> bool {
    WINDOW_PAGE.with(|page| page.narrate_wrote.get())
}

/// Stream one step of the writing job onto the bar: `written` of `of` clips have closed.
///
/// This ADVANCES the queue the run opened rather than rebuilding it as [`show_narrate_stage`] does, which
/// would drop the speaking job. `narrate_call::drive` already clamps readings through
/// [`crate::narrate_pass::progress_forward`], so a retry never falls the bar back to zero.
pub fn show_narrate_progress(window: &adw::ApplicationWindow, written: u32, of: usize) {
    let stage = narrate_run::writing(written as usize, of);
    let stage_queue = live_stage_queue(window);
    stage_queue
        .borrow_mut()
        .prog(0, written as f64 / of.max(1) as f64, &stage);
    WINDOW_PAGE.with(
        |WindowPage {
             narrate_queue: queue,
             ..
         }| *queue.borrow_mut() = stage_queue.borrow().clone(),
    );
    set_narrate_stage(window, &stage, false);
}

pub type ScriptedLine = (u32, f64, String, String);

pub type SlowScript = Option<(Vec<ScriptedLine>, std::time::Duration)>;

/// Put a sentence on this window's status line, the widget every other door writes, and log it too.
pub(crate) fn say_on_status(window: &adw::ApplicationWindow, said: &str) {
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(said);
    }
    ui::log_line(said);
}
