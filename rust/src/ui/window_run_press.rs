//! §02's run bar — the ▶ ⏹ 🎲 ▶✂ presses and their wires. Split out of `window.rs`, which is past its
//! size budget; its doors are re-exported through `window`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::exchanges;
use crate::hand_edit;
use crate::layout;
use crate::prepare_run;
use crate::project::Project;
use crate::run;
use crate::runqueue;
use crate::shell::Shell;
use crate::ui::window as ui;
use crate::ui::window::{paint_progress, paint_run_bar};

/// The transport every run-bar paint and the ▶ press read: this window's live preview, never a default, or `run::RunBar::press`'s S2
/// ("playback beats the page's action") could not fire while a preview plays or waits parked (§11 §2).
pub(crate) fn live_transport() -> run::Transport {
    ui::preview_player(&ui::main_window()).transport
}

/// Where a press of ▶ is decided: [`run::RunBar::press`] on the player's own transport (§F0.2 S2), then redraw and put the bar's sentence on the status line.
/// A refusal outlives this handler: `press` leaves its status empty when a run starts, so refusals are remembered.
#[allow(clippy::too_many_arguments)] // one ▶ press touches the whole bar: its buttons, queue, log and
                                     // progress widget, which belong to the window rather than to any one rule
pub(crate) fn wire_play(
    play: &gtk::Button,
    stop_: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    shell: &Rc<RefCell<Shell>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &Rc<RefCell<runqueue::Queue>>,
    exchange_log: &Rc<RefCell<exchanges::RunLog>>,
    procs: &Rc<RefCell<run::Subprocesses>>,
    project: &Project,
) {
    let bar = bar.clone();
    let shell = shell.clone();
    let status = status.clone();
    let project = project.clone();
    let stop_ = stop_.clone();
    let progress = progress.clone();
    let log = log.clone();
    let log_view = log_view.clone();
    let queue = queue.clone();
    let exchange_log = exchange_log.clone();
    let _ = procs;
    play.connect_clicked(move |play| {
        // The press's last words: repaint the bar, close an opened run, paint progress, put a refusal on the status line, forward Narrate's or Produce's ▶.
        let tail = {
            let play = play.clone();
            let stop_ = stop_.clone();
            let bar = bar.clone();
            let shell = shell.clone();
            let status = status.clone();
            let progress = progress.clone();
            let log = log.clone();
            let log_view = log_view.clone();
            let queue = queue.clone();
            let exchange_log = exchange_log.clone();
            std::rc::Rc::new(move |pressed_started: bool, refusal: Option<String>| {
                let play = &play;
                paint_run_bar(
                    play,
                    &stop_,
                    &bar.borrow(),
                    shell.borrow().page,
                    ui::preview_player(&ui::main_window()).transport,
                );
                status.set_text(&bar.borrow().status);
                // F0.5 S4: a single-step press closes what it opened. §S7: the ending is the run's LAST word, so it goes back after the close, which writes the queue's line
                // over the row. Narrate and Produce run on workers (§03-shell §1) and close the bar in their own endings, so ⏹ can still reach them.
                let closes_itself = matches!(
                    run::step(shell.borrow().page),
                    run::Step::Narrate | run::Step::Produce
                );
                if pressed_started && !closes_itself {
                    crate::ui::run_close::close_step_run(
                        &bar,
                        &queue,
                        &exchange_log,
                        &status,
                        &progress,
                        &log,
                        &log_view,
                    );
                    if run::step(shell.borrow().page) == run::Step::Prepare
                        && !bar.borrow().status.is_empty()
                    {
                        let said = bar.borrow().status.clone();
                        status.set_text(&said);
                    }
                }
                paint_progress(
                    &progress,
                    &log,
                    &log_view,
                    &queue.borrow(),
                    // §2: while this press owns the run its track line IS the status line; a bar sentence keeps the row.
                    crate::ui::shell_beats::StatusLine {
                        status: &status,
                        owned: pressed_started
                            && refusal.is_none()
                            && !bar.borrow().status.is_empty(),
                    },
                    bar.borrow()
                        .running
                        .as_ref()
                        .map_or(log.is_expanded(), |run| run.log_expanded),
                );
                // Last write wins on the label: F1.12's refusal explains why ▶ did nothing, so it goes after the bar's empty status.
                if let Some(reason) = refusal {
                    status.set_text(&reason);
                }
                // F4.1: the Narrate step's own ▶, forwarded last so the opening paint cannot overwrite what the run wrote; rules live in `narrate_run`.
                if pressed_started && run::step(shell.borrow().page) == run::Step::Narrate {
                    let _ = ui::press_narrate_run(&ui::main_window());
                }
                // F5.1: the Produce step's own ▶, forwarded the same way (`produce_flow`).
                if pressed_started && run::step(shell.borrow().page) == run::Step::Produce {
                    let _ = crate::ui::produce_page::press_produce_run(&ui::main_window());
                }
            })
        };
        let transport = ui::preview_player(&ui::main_window()).transport;
        // F1.1 S1: Prepare refuses BEFORE the bar opens, or ⏸/⏹ would show for work that never began; other pages fall through to `press` (F0.2).
        let prepare_asked = run::step(shell.borrow().page) == run::Step::Prepare;
        // F4.1's ▶ on the Narrate page: refusal asked before the bar opens, as for Prepare.
        let narrate_asked = run::step(shell.borrow().page) == run::Step::Narrate;
        // F5.1's ▶ on the Produce page, refused the same way before the bar opens (§F5.1 S1).
        let produce_asked = run::step(shell.borrow().page) == run::Step::Produce;
        // F2.14's ▶ on the Cut page is Suggest, refused or answered before the bar opens (`cut_suggest_press::on_play`).
        let suggest_asked = run::step(shell.borrow().page) == run::Step::Suggest;
        let live = ui::PLAY_SESSION.with(|slots| {
            slots
                .borrow()
                .last()
                .map(|held| held.borrow().clone())
                .unwrap_or_else(|| project.clone())
        });
        // §11 §2 is the table this press asks first: what ▶ means on THIS page now. Its transport answer lands BEFORE any page step, so a playing or
        // parked preview cannot be interrupted by Suggest, write-and-speak or render.
        let has_cut = !ui::review_cut_of(&ui::main_window()).segs.is_empty();
        let tab_play = crate::tab_states::play(shell.borrow().page, has_cut, transport);
        // §F0.2 S1: a run under way makes ▶ the pause button, ahead of both earlier branches: the page refusals (`narrate_run::refuse`, `produce_flow::refuse`,
        // Suggest) answer a busy bar with PAUSING, which would never pause and leave S1's `RESUMED` unreachable. `press` says which reading it is.
        if bar.borrow().running.is_some() {
            let pressed = bar
                .borrow_mut()
                .press(shell.borrow().page, transport, &live);
            debug_assert!(
                matches!(pressed, run::Pressed::ToggledPause { .. }),
                "a bar with a run under way can only toggle its pause here"
            );
            // The walk on Prepare's worker parks at its next checkpoint (§03-shell §1).
            crate::run_stages::mirror_switches(&bar.borrow());
            paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, transport);
            status.set_text(&bar.borrow().status);
            return;
        }
        match tab_play.0 {
            crate::tab_states::Play::Transport { resume } => {
                // Narrate's preview is the page's own, so the bar's ▶ is the page's ▶ (the door a click on the picture takes); writing only the window's transport record moved nothing.
                if narrate_asked {
                    let _ = crate::ui::narrate_page::press_narrate_play(&ui::main_window());
                    return;
                }
                // Cut's the same way: the lit one of ▶ / ▶✂ / ▶✂✂ pauses or resumes the player and the lanes; flipping the record alone left picture and sound playing.
                if suggest_asked {
                    let window = ui::main_window();
                    let held = ui::preview_player(&window);
                    let [recording, cut_lit, _review] =
                        crate::tab_states::preview_lits(&crate::cut_screen::Preview {
                            cut_only: held.cut_only,
                            reviewing: held.reviewing,
                        });
                    let lit = if recording {
                        ui::play_recording_button(&window)
                    } else if cut_lit {
                        ui::play_cut_button(&window)
                    } else {
                        ui::review_cuts_button(&window)
                    };
                    if let Some(button) = lit {
                        button.emit_clicked();
                        return;
                    }
                }
                // The face answers the transport this press LEFT, not the one it found.
                let held = ui::preview_player(&ui::main_window());
                let toggled = run::Transport {
                    playing: resume,
                    started: true,
                };
                ui::set_preview_state(&ui::main_window(), held.cut_only, held.reviewing, toggled);
                paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, toggled);
                return;
            }
            // "▶ refuses without a cut", and narration off before it (§2). Said here rather than opened, so no ⏸ shows; the pages' own gates still run below.
            crate::tab_states::Play::Refused => {
                if let Some(reason) = tab_play.1 {
                    status.set_text(reason);
                    paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, transport);
                    return;
                }
            }
            crate::tab_states::Play::Step(_) => {}
        }
        if prepare_asked {
            if let Some((log, sentence)) = prepare_run::refuse(&live) {
                ui::log_line(&log);
                status.set_text(&sentence);
                paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, transport);
                return;
            }
            // F0.14 S6: a profile's required field left empty is the User Context left unfinished.
            if let Some(sentence) = crate::ui::profile_form::refusal(&live) {
                ui::log_line(&format!("!!! {sentence}"));
                status.set_text(&sentence);
                paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, transport);
                return;
            }
        }
        if narrate_asked {
            if let Some(reason) = ui::refuse_narrate_run() {
                // S1 only: nothing was pulled, nothing was saved, no file was written or copied.
                status.set_text(&reason);
                paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, transport);
                return;
            }
        }
        // Read before the call: a borrow taken inside the condition lives through it, and Suggest's model mode opens the bar's run itself.
        let suggest_busy = bar.borrow().running.is_some();
        if suggest_asked
            && crate::ui::cut_suggest_press::on_play(
                &ui::main_window(),
                suggest_busy,
                transport.started,
                |sentence| status.set_text(sentence),
                || paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, transport),
            )
        {
            return;
        }

        if produce_asked {
            // §F4.7 (**Text**): flushed before Produce reads the file; the render gates on `narration.json`, so an unwritten typed line would be missing.
            crate::ui::narrate_text::flush_owed(&ui::main_window());
            let has_cut = !ui::review_cut_of(&ui::main_window()).segs.is_empty();
            if let Some(reason) =
                crate::produce_flow::refuse(bar.borrow().running.is_some(), has_cut)
            {
                // §F5.1 S1: refused before the bar opened — no ⏸ shows for a run that never began.
                status.set_text(reason);
                paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, transport);
                return;
            }
        }

        let pressed = bar
            .borrow_mut()
            .press(shell.borrow().page, transport, &project);
        // S2 stops here (no snapshot, no step) and gets no sentence: the button's face carries the toggle.
        if let run::Pressed::ToggledTransport { resume } = pressed {
            let held = ui::preview_player(&ui::main_window());
            // The face answers the transport this press LEFT: painting from `held.transport` would leave ⏸ up.
            let toggled = run::Transport {
                playing: resume,
                started: true,
            };
            ui::set_preview_state(&ui::main_window(), held.cut_only, held.reviewing, toggled);
            paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, toggled);
            return;
        }
        let mut refusal: Option<String> = None;
        // The ▶ door's derivation, when Prepare's worker owes it: the context and the live policy.
        let mut derive_on_worker: Option<(String, crate::project::Policy)> = None;
        // F0.5 S1: a press that opened a run also opened the bookkeeping (cancel context, queue, model log, log expander); only `Started` goes through `start_run`.
        if matches!(pressed, run::Pressed::Started { .. }) {
            // F0.7's second door: a context nobody has derived from is derived before the step reads it, so ▶ never runs on a lagging policy; only when the tracker says so.
            let context = ui::user_context(&ui::main_window());
            if ui::POLICY_TRACKERS
                .with(|trackers| trackers.borrow().last().cloned())
                .is_some_and(|tracker| tracker.borrow().needs_derive(&context))
            {
                // Prepare asks the live model on its worker (§03-shell §1); a test's own answerer answers at once, so it is asked here.
                if prepare_asked && !super::window_policy::answerer_installed() {
                    derive_on_worker =
                        Some((context.clone(), ui::session_policy(&ui::main_window())));
                } else {
                    let derived = ui::derive_policy(&ui::main_window());
                    if !derived.used_defaults {
                        ui::log_line(&format!("policy: {}", derived.status()));
                    }
                }
            }
            // F1.12 S2: the Cut step first asks whether `final.txt` was hand-edited since the marks were written (`hand_edit::before_cut`).
            if run::step(shell.borrow().page) == run::Step::Suggest {
                let dir = crate::startup::open_dir();
                if let Ok(tree) = layout::Tree::new(&dir) {
                    let sources: Vec<String> = project
                        .sources
                        .iter()
                        .map(|source| source.path.clone())
                        .collect();
                    if let Some(outcome) = hand_edit::before_cut(&tree, &sources) {
                        for line in &outcome.logs {
                            ui::log_line(line);
                        }
                        if outcome.refused {
                            // The refusal goes where a press's answer is read, held until after the tail's status write, which would blank it.
                            refusal = Some(
                                outcome
                                    .logs
                                    .last()
                                    .map(|line| {
                                        line.trim_start_matches('!').trim_start().to_string()
                                    })
                                    .unwrap_or_else(|| "the edit was refused".to_string()),
                            );
                        }
                    }
                }
            }
            // F1.1 S1-S4, S5's audio.cpp preflight and F1.7's plan are Prepare's worker's ([`super::prepare_worker`]). S3 snapshots the LIVE session: a source added after `build_window` is part of this run.
            let asked = ui::PLAY_SESSION.with(|s| {
                s.borrow()
                    .last()
                    .map(|h| h.borrow().clone())
                    .unwrap_or_else(|| project.clone())
            });
            let snapshot = run::snapshot_sources(&asked);
            runqueue::start_run(
                &mut bar.borrow_mut(),
                &mut queue.borrow_mut(),
                &mut exchange_log.borrow_mut(),
                run::step(shell.borrow().page),
                snapshot,
            );
            crate::ui::run_close::note_started_run(&bar);
            // §4's bill before the stages run: which models this step will ask and how many times (numbers from `run_bill`). Prepare's worker says its own ([`super::prepare_worker`]).
            if run::step(shell.borrow().page) != run::Step::Prepare {
                if let Some(bill) = crate::ui::run_bill::line_for(run::step(shell.borrow().page)) {
                    ui::log_line(&bill);
                }
            }
            // §2: while nothing is countable the bar pulses; it stops itself when a track reports.
            crate::ui::shell_beats::arm_pulse(&progress, &bar, &queue);
            // F0.5 S2/S3: the step's work is walked under the bookkeeping, a checkpoint between tasks (`run_stages`). Prepare's walk runs on a worker (§03-shell §1):
            // the bar is painted open now and lines, queue and ending land as it goes. Other steps plan no stages here.
            if run::step(shell.borrow().page) == run::Step::Prepare {
                let ask = super::prepare_worker::Ask {
                    asked,
                    derive: derive_on_worker.take(),
                };
                let start_queue = queue.borrow().clone();
                crate::run_stages::mirror_switches(&bar.borrow());
                let on_note = {
                    let queue = queue.clone();
                    let mut counted_at = String::new();
                    move |note: super::prepare_worker::Note| match note {
                        super::prepare_worker::Note::Policy(policy, derived, context) => {
                            super::window_policy::apply_derivation(policy, derived, &context)
                        }
                        super::prepare_worker::Note::Queue(walked, stage) => {
                            *queue.borrow_mut() = walked;
                            if stage != counted_at {
                                counted_at = stage.clone();
                                let session = ui::PLAY_SESSION
                                    .with(|s| s.borrow().last().map(|h| h.borrow().clone()));
                                if let Some(session) = session {
                                    let window = crate::ui::window::main_window();
                                    crate::ui::window::paint_readouts_for(
                                        &window,
                                        &session,
                                        &crate::startup::open_dir(),
                                    );
                                }
                            }
                            // F1.7 S5: a ⏹ inside Describe arms the restart, so ⏹ must know the stage.
                            if stage == "describe" {
                                crate::stop_legs::enter_describe();
                            } else {
                                crate::stop_legs::leave_describe();
                            }
                        }
                    }
                };
                let finish = {
                    let bar = bar.clone();
                    let queue = queue.clone();
                    let tail = tail.clone();
                    move |answer: super::prepare_worker::Answer| {
                        crate::stop_legs::leave_describe();
                        let refusal = match answer {
                            super::prepare_worker::Answer::Refused(sentence) => Some(sentence),
                            super::prepare_worker::Answer::Walked(drive, asked) => {
                                // S4: a failed stage names itself in the log; §S7 + §S8: Prepare's ending.
                                crate::run_stages::say_outcome(&drive, ui::log_line);
                                super::prepare_guard::say_ending(
                                    &drive,
                                    &ui::main_window(),
                                    &asked,
                                    &bar,
                                    &queue,
                                );
                                None
                            }
                        };
                        tail(true, refusal);
                    }
                };
                super::background::run_with_notes(
                    "prepare",
                    move |note| super::prepare_worker::work(ask, start_queue, note),
                    on_note,
                    finish,
                );
                paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, transport);
                status.set_text(&bar.borrow().status);
                return;
            }
            let _ = &asked;
            let drive = crate::run_stages::Drive::Done { ran: 0, skipped: 0 };
            crate::run_stages::say_outcome(&drive, ui::log_line);
        }
        tail(matches!(pressed, run::Pressed::Started { .. }), refusal);
    });
}

/// Where a press of ⏹ is decided: ask [`run::RunBar::press_stop`], redraw both buttons and put the bar's sentence on the status line (spec/00-principles.md §5).
///
/// `in_describe` is `false` here because the shell does not know the stage; F1.7's round passes the real value (S5 arms "describe from the start").
#[allow(clippy::too_many_arguments)] // ⏹ must reach every widget ▶ painted, plus the run's own state
pub(crate) fn wire_stop(
    stop_: &gtk::Button,
    play: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    shell: &Rc<RefCell<Shell>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &Rc<RefCell<runqueue::Queue>>,
    procs: &Rc<RefCell<run::Subprocesses>>,
) {
    let bar = bar.clone();
    let shell = shell.clone();
    let status = status.clone();
    let procs = procs.clone();
    let play = play.clone();
    let progress = progress.clone();
    let log = log.clone();
    let log_view = log_view.clone();
    let queue = queue.clone();
    stop_.connect_clicked(move |stop_| {
        // F4.5 S6: while the Narrate preview runs, ⏹ belongs to the PREVIEW: it stops both players and hands ▶ back to the step; otherwise it is `run::RunBar::press_stop`'s rule.
        if crate::ui::narrate_page::narrate_preview_playing() {
            let said =
                crate::ui::narrate_page::press_preview_stop(&crate::ui::window::main_window());
            // Repaint from the page's own state: ▶ back to the step's face, ⏹ insensitive unless a run is under way.
            paint_run_bar(
                &play,
                stop_,
                &bar.borrow(),
                shell.borrow().page,
                live_transport(),
            );
            status.set_text(&said);
            return;
        }
        // S1 reads the player's real transport; `press_stop` cannot know what is playing.
        let window = crate::ui::window::main_window();
        let transport = ui::preview_player(&window).transport;
        let in_describe = crate::stop_legs::describe_stage();
        let stopped = bar.borrow_mut().press_stop(
            shell.borrow().page,
            transport,
            in_describe,
            &mut procs.borrow_mut(),
        );
        // The walk on Prepare's worker reads the stop between two tasks (§03-shell §1).
        crate::run_stages::mirror_switches(&bar.borrow());
        // S1: stop the player itself, not only the sentence (the leg decides if this page owns one).
        if let Some(stopped) = crate::stop_legs::stop_page_preview(shell.borrow().page, transport) {
            let held = ui::preview_player(&window);
            // A stopped review is over, as a paused one is (F2.3 S5).
            ui::set_preview_state(&window, held.cut_only, false, stopped);
            if held.reviewing {
                crate::ui::cut_review_play::clear();
            }
            crate::ui::cut_cut_play::pause_media();
            crate::lane_hear::halt_all();
            crate::ui::cut_record_play::paint_transport(&window);
        }
        // S5: a stop inside Describe arms the next Prepare run to start over.
        if in_describe {
            crate::stop_legs::arm_describe_restart();
        }
        // S3/S4: report whatever is still out on the wire, off the stop's own clock (`cancel_leg` owns the wording); the 200 ms wait is a worker's (§03-shell §1).
        crate::ui::background::run(
            "stop",
            || {
                crate::cancel_leg::report_abandoned(std::time::Duration::from_millis(200));
            },
            |()| {},
        );
        // S3's kill: drain-and-signal, so nothing survives the press nor stays listed to be killed twice.
        crate::stop_legs::stop_registered(&mut procs.borrow_mut());
        // And the children the worker's walk has running, which this window's list never held.
        crate::stop_legs::signal(&crate::run_stages::take_running_children());
        // §2: the pulse ends with the run.
        crate::ui::shell_beats::stop_pulse();
        // Repaint both: stopping a run changes ▶'s face as well as ⏹'s sensitivity.
        paint_run_bar(
            &play,
            stop_,
            &bar.borrow(),
            shell.borrow().page,
            live_transport(),
        );
        // §F4.1 S7: on Narrate the stage's own ending (`narration stopped` / `speaking stopped`, from `run_narrate_on`) outranks the generic STOPPING.
        let narrate_stage = run::step(shell.borrow().page) == run::Step::Narrate;
        let stage_status = narrate_stage.then(|| {
            crate::narrate_run::stopped_status(if ui::narrate_wrote() {
                crate::narrate_run::JOB_SPEAKING
            } else {
                crate::narrate_run::JOB_NARRATION
            })
        });
        // Only write a sentence when there is one: S2's "nothing more" must not blank the status.
        if matches!(stopped, run::Stopped::RunStopped { .. }) {
            if let Some(stage) = stage_status.as_deref() {
                status.set_text(stage);
            } else {
                status.set_text(stopped.status());
            }
        } else if !stopped.status().is_empty() {
            status.set_text(stopped.status());
        }
        paint_progress(
            &progress,
            &log,
            &log_view,
            &queue.borrow(),
            crate::ui::shell_beats::StatusLine {
                status: &status,
                owned: false,
            }, // nothing repaints over "stopping…"
            bar.borrow()
                .running
                .as_ref()
                .map_or(log.is_expanded(), |run| run.log_expanded),
        );
    });
}
/// F2.1: the Cut page's ▶ forwards to [`ui::press_play_recording`], then [`preview::press_recording`]; the window is cloned into the closure as [`wire_settings`] does.
pub(crate) fn wire_play_recording(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        // The filmed stretches come from the page's own recordings (`cut_record_play::runs_now`), which also decides what to start or stop.
        let runs = crate::ui::cut_record_play::runs_now();
        let pressed = ui::press_play_recording(&window, &runs);
        crate::ui::cut_record_play::after_press(&window, &pressed, &runs);
    });
}
/// F2.5 S6: hand the slider's number to [`set_preview_volume`] and nothing else: the one volume is owned by `cut_hear::PreviewVolume`.
pub(crate) fn wire_preview_volume(scale: &gtk::Scale, window: &adw::ApplicationWindow) {
    let window = window.clone();
    scale.connect_value_changed(move |slider| {
        ui::set_preview_volume(&window, slider.value());
    });
}
/// F2.2: the Cut page's ▶✂ forwards to [`ui::press_play_cut`], window cloned as in [`wire_play_recording`].
pub(crate) fn wire_play_cut(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        // The cut this press plays is the window's (`seed_review_cut`'s slot via `ui::newest_review_cut`): F2.10 S3 needs ▶ to release a watched row from the same cut the page drew.
        let _ = ui::press_play_cut(&window, &ui::newest_review_cut());
        // Faces and grey follow the state the press changed (S1, §D's ⏸ rule), all three: ending a review changes ▶✂✂'s face too.
        crate::ui::cut_record_play::paint_transport(&window);
    });
}
