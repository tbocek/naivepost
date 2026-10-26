//! §02's run bar — the ▶ ⏹ 🎲 ▶✂ presses and their wires. Split out of `window.rs`, which is past its
//! size budget; its doors are re-exported through `window`.
//!
//! The decisions are `run`'s; what is here forwards a press, repaints the bar, and puts
//! the sentence on the status line.

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

/// The transport every run-bar paint and the ▶ press read: this window's live preview, never a
/// default. `run::RunBar::press`'s S2 ("playback beats the page's action") can only fire from what
/// the player actually holds — a paint site handing it `Transport::default()` would keep ▶ running the
/// page's step while a preview plays or waits parked (§11 §2: "then the preview's transport until ⏹").
pub(crate) fn live_transport() -> run::Transport {
    ui::preview_player(&ui::main_window()).transport
}

/// Where a press of ▶ is decided: [`run::RunBar::press`] on the held player's own transport (§F0.2 S2,
/// which ⏹ reads the same way), then redraw and put the bar's sentence on the status line. A refusal
/// outlives this handler: `press` leaves its status empty when a run starts, so refusals are remembered.
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
        // The press's last words: the bar repainted, an opened run closed, the progress painted, a refusal put
        // on the status line, and Narrate's or Produce's own ▶ forwarded. Said at once by every press, and by
        // Prepare's when its worker answers.
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
                // F0.5 S4: a single-step press closes what it opened (running off, the unload handed over,
                // the queue's line onto the status row). Narrate and Produce close theirs in their own flow.
                // §S7: the ending is the run's LAST word, so it goes back after that close — the close writes
                // the queue's line over the row, and `say_ending` already put the ending into `bar.status`.
                // Narrate's and Produce's runs go on on their workers (§03-shell §1) and their own endings
                // close the bar (`window_narrate::run_narrate_on`, `produce_page::encode_and_finish`), so ⏹
                // can still reach them.
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
                    // §2: while this press owns the run its track line IS the status line; a bar sentence
                    // (the empty opening line, F1.12's refusal) keeps the row instead of being overwritten.
                    crate::ui::shell_beats::StatusLine {
                        status: &status,
                        owned: pressed_started
                            && refusal.is_none()
                            && !bar.borrow().status.is_empty(),
                    },
                    // A run still open says whether its log is open; a closed one leaves the log as the
                    // person has it (`paint_closed`'s rule): the first rewrite folded it at every run's end,
                    // and the log on screen stopped there.
                    bar.borrow()
                        .running
                        .as_ref()
                        .map_or(log.is_expanded(), |run| run.log_expanded),
                );
                // Last write wins on the label: F1.12's refusal is why ▶ did not do what it was pressed for,
                // so it goes on screen after the bar's own (empty) status rather than under it.
                if let Some(reason) = refusal {
                    status.set_text(&reason);
                }
                // F4.1: the Narrate step's own ▶. Forwarded last so the bar's opening paint cannot overwrite
                // the stage and the sentence the run wrote; the rules live in `narrate_run`, this only
                // forwards, as the Prepare and Suggest branches above do.
                if pressed_started && run::step(shell.borrow().page) == run::Step::Narrate {
                    let _ = ui::press_narrate_run(&ui::main_window());
                }
                // F5.1: the Produce step's own ▶, forwarded the same way; rules and strings live in `produce_flow`.
                if pressed_started && run::step(shell.borrow().page) == run::Step::Produce {
                    let _ = crate::ui::produce_page::press_produce_run(&ui::main_window());
                }
            })
        };
        // §F0.2 S2 belongs to the page's preview, so this press asks the player what it holds.
        let transport = ui::preview_player(&ui::main_window()).transport;
        // F1.1 S1: Prepare refuses at its own start, BEFORE the bar is opened — a run that started
        // and was then sorry would leave ⏸/⏹ showing for work that never began. So the refusal is
        // asked first and short-circuits here; every other page (and every non-refused press) falls
        // through to `press`, whose precedence F0.2 owns.
        let prepare_asked = run::step(shell.borrow().page) == run::Step::Prepare;
        // F4.1's ▶ is the same button on the Narrate page: the refusal is asked BEFORE the bar opens,
        // exactly as Prepare does above, so a refused run never shows ⏸ for work that never began.
        let narrate_asked = run::step(shell.borrow().page) == run::Step::Narrate;
        // F5.1's ▶ on the Produce page, refused the same way before the bar opens (§F5.1 S1).
        let produce_asked = run::step(shell.borrow().page) == run::Step::Produce;
        // F2.14's ▶ on the Cut page is Suggest, refused or answered before the bar opens; the whole seam
        // (including why a started preview is not this press) is `cut_suggest_press::on_play`.
        let suggest_asked = run::step(shell.borrow().page) == run::Step::Suggest;
        let live = ui::PLAY_SESSION.with(|slots| {
            slots
                .borrow()
                .last()
                .map(|held| held.borrow().clone())
                .unwrap_or_else(|| project.clone())
        });
        // §11 §2 is the table this press asks first: what ▶ means on THIS page right now. Its transport
        // answer ("Suggest, until the preview started; then the preview's transport until ⏹") therefore
        // lands BEFORE any page step is asked — a playing or parked preview cannot be interrupted by
        // Suggest, write-and-speak or render starting underneath it. `Play::Step` falls through to the
        // per-page refusals and the bar below, which stay as they were.
        let has_cut = !ui::review_cut_of(&ui::main_window()).segs.is_empty();
        let tab_play = crate::tab_states::play(shell.borrow().page, has_cut, transport);
        // §F0.2 S1: a run under way makes ▶ the pause button, and that branch comes before BOTH
        // earlier branches — the tab's transport answer above and every page's own "busy" refusal
        // below (`narrate_run::refuse`, `produce_flow::refuse` and the Suggest seam all answer a busy
        // bar with PAUSING, which would say "pausing…" twice, never actually pause, and leave S1's
        // second reading (`RESUMED`) unreachable). The flowchart asks "a run under way?" first, so
        // the bar owns this press; `press` says which of the two readings it is.
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
                // Narrate's preview is the page's own (its players, its `playing`), so the bar's ▶ is the page's
                // ▶, the door a click on the picture takes; it publishes the transport and paints the bar.
                // Writing only the window's transport record (below) moved nothing the page plays: a paused
                // preview stayed paused under "▶ plays on from here".
                if narrate_asked {
                    let _ = crate::ui::narrate_page::press_narrate_play(&ui::main_window());
                    return;
                }
                // Cut's the same way: the lit one of ▶ / ▶✂ / ▶✂✂ is pressed, so its own handler pauses or
                // resumes the player and the lanes. Flipping the record alone stopped the line while the
                // picture and its sound played on.
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
                // The face answers the transport this press LEFT, not the one it found: parking a
                // playing preview paints ▶ (it owes a resume).
                let held = ui::preview_player(&ui::main_window());
                let toggled = run::Transport {
                    playing: resume,
                    started: true,
                };
                ui::set_preview_state(&ui::main_window(), held.cut_only, held.reviewing, toggled);
                paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, toggled);
                return;
            }
            // "▶ refuses without a cut", and narration off before it (§2 lists both for Narrate). The
            // refusal is said here rather than opened, so no ⏸ shows for work that never began; the
            // pages' own gates (busy, no session timeline) still run in the block below.
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
        // Read before the call: a borrow taken inside the condition lives through it, and Suggest's model mode
        // opens the bar's run itself (its asks run on a worker, §03-shell §1).
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
            // §F4.7 (**Text**): "flushed ... before Produce reads the file" — the render keys its gate on
            // `narration.json`, so a line typed and not yet written would be missing from what it stamps.
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
            // The face answers the transport this press LEFT, not the one it found: parking a playing
            // preview paints ▶ (it owes a resume), and painting from `held.transport` would leave ⏸ up.
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
        // F0.5 S1: a press that opened a run also opened the bookkeeping — fresh cancel context,
        // empty queue, model log closed, log expander open. A pause or a transport toggle is not a
        // new run, so only `Started` goes through `start_run`.
        if matches!(pressed, run::Pressed::Started { .. }) {
            // F0.7's second door: a context nobody has derived from yet is derived before the step
            // reads it, so ▶ never runs on a policy that lags what the person just typed. Only when
            // the tracker says so — an unchanged context costs nothing, and the LLM cache makes a
            // repeated identical request free besides.
            let context = ui::user_context(&ui::main_window());
            if ui::POLICY_TRACKERS
                .with(|trackers| trackers.borrow().last().cloned())
                .is_some_and(|tracker| tracker.borrow().needs_derive(&context))
            {
                // Prepare asks the live model on its run's worker (§03-shell §1); a test's own answerer
                // answers at once, so it is asked here as before.
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
            // F1.12 S2: the Cut step first asks whether `final.txt` was hand-edited since the marks
            // were written. The rule is entirely in `hand_edit::before_cut`; this handler only runs
            // it for the page that owns the marks and forwards what it says.
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
                            // The refusal sentence goes where a press's answer is read — held until
                            // after the tail's own status write, which would otherwise blank it.
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
            // F1.1 S1-S4 (the save, the stale outputs cleared), S5's audio.cpp preflight and F1.7's plan are
            // Prepare's worker's ([`super::prepare_worker`]): each can wait on the disk or a server.
            // S3 snapshots the LIVE session: a source added after `build_window` is part of this run.
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
            // §4's bill, before the stages run: which models this step is about to ask and how many
            // times, so a person sees what a press costs the same place they see what it did. Nothing
            // here counts twice — every number comes from the module that owns it (see `run_bill`).
            // Prepare's bill counts the frames on disk, so its worker says it ([`super::prepare_worker`]).
            if run::step(shell.borrow().page) != run::Step::Prepare {
                if let Some(bill) = crate::ui::run_bill::line_for(run::step(shell.borrow().page)) {
                    ui::log_line(&bill);
                }
            }
            // §2: while nothing is countable the bar pulses; it stops itself when a track reports.
            crate::ui::shell_beats::arm_pulse(&progress, &bar, &queue);
            // F0.5 S2/S3: the step's own work is walked under the bookkeeping, its checkpoint asked
            // between every task; rules live in `run_stages`. Prepare's walk runs on a worker (§03-shell §1,
            // "The window thread never waits"): the bar is painted open now, and the walk's lines, queue and
            // ending land as it goes. The other steps plan no stages here (`run_stages::plan`).
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
                            // §1: the Inputs/Outputs readouts count what the run has written so far, recounted
                            // as each stage begins (on a worker, `paint_readouts`), not only at the run's end:
                            // the first rewrite showed "0 lines" through a whole run that had transcribed them.
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

/// Where a press of ⏹ is decided: ask [`run::RunBar::press_stop`], then redraw both buttons and put
/// the bar's sentence on the status line. The handler forwards and decides nothing
/// (spec/00-principles.md §5) — which of S1/S2/S3 applies, and what each one stops, is run.rs's rule.
///
/// `in_describe` is `false` here because the shell does not know which stage is running; F1.7's own
/// round passes the real value when it drives Describe (S5 arms "describe from the start").
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
        // F4.5 S6: while the Narrate preview is running, ⏹ belongs to the PREVIEW, not to a run. Stopping it
        // here stops both players and hands ▶ back to the step; only when no preview is going does the press
        // mean "end the long job", which is `run::RunBar::press_stop`'s rule unchanged.
        if crate::ui::narrate_page::narrate_preview_playing() {
            let said =
                crate::ui::narrate_page::press_preview_stop(&crate::ui::window::main_window());
            // Repaint from the page's own state now that nothing is playing: ▶ goes back to the step's face
            // and ⏹ loses its sensitivity unless a run is under way.
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
            // The media stops with the page's state, all of it: the player, the cut's spare, the lanes.
            // The first rewrite only flipped the flag, so ⏹ left the video playing behind a ▶ face.
            crate::ui::cut_cut_play::pause_media();
            crate::lane_hear::halt_all();
            crate::ui::cut_record_play::paint_transport(&window);
        }
        // S5: a stop inside Describe arms the next Prepare run to start over.
        if in_describe {
            crate::stop_legs::arm_describe_restart();
        }
        // S3/S4: report whatever is still out on the wire, off the stop's own clock (cancel_leg owns
        // the wording and the wait; window.rs only asks).
        // The 200 ms it waits for them is a worker's, not the window's (§03-shell §1).
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
        // §2: the pulse belongs to the run, so it ends with the run rather than outliving it.
        crate::ui::shell_beats::stop_pulse();
        // Repaint both: stopping a run changes ▶'s face as well as ⏹'s sensitivity.
        paint_run_bar(
            &play,
            stop_,
            &bar.borrow(),
            shell.borrow().page,
            live_transport(),
        );
        // §F4.1 S7: on the Narrate page the stage's own ending outranks `stopping…`. The door that ran
        // is `run_narrate_on`, and it ends with `narration stopped` / `speaking stopped` for the stage ⏹
        // caught (`window_narrate.rs`); the generic STOPPING sentence says nothing about which half died.
        let narrate_stage = run::step(shell.borrow().page) == run::Step::Narrate;
        let stage_status = narrate_stage.then(|| {
            crate::narrate_run::stopped_status(if ui::narrate_wrote() {
                crate::narrate_run::JOB_SPEAKING
            } else {
                crate::narrate_run::JOB_NARRATION
            })
        });
        // Only write a sentence when there is one: S2's "nothing more" must leave an existing status
        // alone rather than blanking it.
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
/// F2.1: the Cut page's ▶ forwards to [`ui::press_play_recording`], which forwards to
/// [`preview::press_recording`] and paints. The handler takes the window it was wired on, cloned into
/// the closure the way [`wire_settings`] does, so `build_window` needs nothing threaded through.
pub(crate) fn wire_play_recording(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        // The filmed stretches come from the page's own recordings now (`cut_record_play::runs_now`), so a
        // session with footage plays instead of always hearing "nothing was filmed"; what to start or stop
        // with is that module's, not this handler's.
        let runs = crate::ui::cut_record_play::runs_now();
        let pressed = ui::press_play_recording(&window, &runs);
        crate::ui::cut_record_play::after_press(&window, &pressed, &runs);
    });
}
/// F2.5 S6: hand the slider's number to [`set_preview_volume`] and nothing else. The widget decides no
/// part of the rule — it does not clamp, scale or remember — because the one volume is owned by
/// `cut_hear::PreviewVolume` and every preview reads that same value back.
pub(crate) fn wire_preview_volume(scale: &gtk::Scale, window: &adw::ApplicationWindow) {
    let window = window.clone();
    scale.connect_value_changed(move |slider| {
        ui::set_preview_volume(&window, slider.value());
    });
}
/// F2.2: the Cut page's ▶✂ forwards to [`ui::press_play_cut`], taking the window cloned into the
/// closure exactly as [`wire_play_recording`] does.
pub(crate) fn wire_play_cut(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        // The cut this press plays is the one this window holds — `seed_review_cut`'s slot, read back
        // through `ui::newest_review_cut`. F2.10 S3 hangs on that: ▶ must release a watched row from the
        // same cut the page drew, so pressing with an empty stand-in would refuse instead of handing
        // the preview back.
        let _ = ui::press_play_cut(&window, &ui::newest_review_cut());
        // The faces and the grey follow the state the press just changed (S1, §D's ⏸ rule), all three:
        // ending a review changes ▶✂✂'s face as well as this one's.
        crate::ui::cut_record_play::paint_transport(&window);
    });
}
