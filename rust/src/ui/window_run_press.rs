//! §02's run bar — the ▶ ⏹ 🎲 ▶✂ presses and their wires. Split out of `window.rs`, which is past its
//! size budget; its doors are re-exported through `window`.
//!
//! The decisions are `run`'s and `lucky_run`'s; what is here forwards a press, repaints the bar, and puts
//! the sentence on the status line.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::describe;
use crate::exchanges;
use crate::hand_edit;
use crate::joins;
use crate::layout;
use crate::prepare_run;
use crate::project::Project;
use crate::retakes;
use crate::run;
use crate::runqueue;
use crate::shell::Shell;
use crate::startup;
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
    let procs = procs.clone();
    play.connect_clicked(move |play| {
        // §2: ▶'s paint also gates the gears, so this handler looks them up alongside the button it was
        // handed. The run bar always draws a lucky-button; `expect` names that invariant rather than
        // inventing a stand-in widget for a window that does not have one.
        let lucky =
            ui::lucky_button(&ui::main_window()).expect("the run bar draws its lucky-button (§2)");
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
        let tab_play = crate::tab_states::play(shell.borrow().page, &live, has_cut, transport);
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
            paint_run_bar(
                play,
                &lucky,
                &stop_,
                &bar.borrow(),
                shell.borrow().page,
                transport,
            );
            status.set_text(&bar.borrow().status);
            return;
        }
        match tab_play.0 {
            crate::tab_states::Play::Transport { resume } => {
                // The face answers the transport this press LEFT, not the one it found: parking a
                // playing preview paints ▶ (it owes a resume).
                let held = ui::preview_player(&ui::main_window());
                let toggled = run::Transport {
                    playing: resume,
                    started: true,
                };
                ui::set_preview_state(&ui::main_window(), held.cut_only, held.reviewing, toggled);
                paint_run_bar(
                    play,
                    &lucky,
                    &stop_,
                    &bar.borrow(),
                    shell.borrow().page,
                    toggled,
                );
                return;
            }
            // "▶ refuses without a cut", and narration off before it (§2 lists both for Narrate). The
            // refusal is said here rather than opened, so no ⏸ shows for work that never began; the
            // pages' own gates (busy, no session timeline) still run in the block below.
            crate::tab_states::Play::Refused => {
                if let Some(reason) = tab_play.1 {
                    status.set_text(reason);
                    paint_run_bar(
                        play,
                        &lucky,
                        &stop_,
                        &bar.borrow(),
                        shell.borrow().page,
                        transport,
                    );
                    return;
                }
            }
            crate::tab_states::Play::Step(_) => {}
        }
        if prepare_asked {
            if let Some((log, sentence)) = prepare_run::refuse(&live) {
                ui::log_line(&log);
                status.set_text(&sentence);
                paint_run_bar(
                    play,
                    &lucky,
                    &stop_,
                    &bar.borrow(),
                    shell.borrow().page,
                    transport,
                );
                return;
            }
        }
        if narrate_asked {
            if let Some(reason) = ui::refuse_narrate_run(&live) {
                // S1 only: nothing was pulled, nothing was saved, no file was written or copied.
                status.set_text(&reason);
                paint_run_bar(
                    play,
                    &lucky,
                    &stop_,
                    &bar.borrow(),
                    shell.borrow().page,
                    transport,
                );
                return;
            }
        }
        if suggest_asked
            && crate::ui::cut_suggest_press::on_play(
                &ui::main_window(),
                bar.borrow().running.is_some(),
                transport.started,
                |sentence| status.set_text(sentence),
                || {
                    paint_run_bar(
                        play,
                        &lucky,
                        &stop_,
                        &bar.borrow(),
                        shell.borrow().page,
                        transport,
                    )
                },
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
                paint_run_bar(
                    play,
                    &lucky,
                    &stop_,
                    &bar.borrow(),
                    shell.borrow().page,
                    transport,
                );
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
            paint_run_bar(
                play,
                &lucky,
                &stop_,
                &bar.borrow(),
                shell.borrow().page,
                toggled,
            );
            return;
        }
        let mut refusal: Option<String> = None;
        // Whether this press may open a run. F1.1's refusals set it false so `start_run` is skipped
        // and the bar never shows a run that was refused before starting.
        let mut started = true;
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
                let derived = ui::derive_policy(&ui::main_window());
                if !derived.used_defaults {
                    ui::log_line(&format!("policy: {}", derived.status()));
                }
            }
            // F1.12 S2: the Cut step first asks whether `final.txt` was hand-edited since the marks
            // were written. The rule is entirely in `hand_edit::before_cut`; this handler only runs
            // it for the page that owns the marks and forwards what it says. Spec silent on a lucky
            // all-steps run (F0.4), so it is checked here too — the same press, the same question.
            if run::step(shell.borrow().page) == run::Step::Suggest {
                let dir = startup::session_dir(&std::env::current_dir().unwrap_or_default());
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
            // F1.1 S1-S4: Prepare's own start flow, run BEFORE the bookkeeping opens a run on it.
            // A refusal never becomes a run at all — `start_run` is not reached and the status line
            // carries the sentence instead. Every rule lives in `prepare_run`; this block only finds
            // the tree the window works in, prints what came back, and remembers a refusal for the
            // tail below (which would otherwise blank the line with the bar's empty status).
            if run::step(shell.borrow().page) == run::Step::Prepare {
                let dir = startup::session_dir(&std::env::current_dir().unwrap_or_default());
                // Ask about the live session, not the argument this window was built with: sources get
                // added and removed after that call, and a refusal about stale data is a wrong refusal.
                let asked = ui::PLAY_SESSION.with(|slots| {
                    slots
                        .borrow()
                        .last()
                        .map(|held| held.borrow().clone())
                        .unwrap_or_else(|| project.clone())
                });
                match layout::Tree::new(&dir)
                    .ok()
                    .map(|tree| (prepare_run::begin(&asked, &tree), tree))
                {
                    Some((prepare_run::Start::Refused { log, status }, _)) => {
                        ui::log_line(&log);
                        refusal = Some(status);
                        started = false;
                    }
                    Some((prepare_run::Start::SaveFailed { error }, _)) => {
                        ui::log_line(&format!("!!! could not save the project -- {error}"));
                        refusal = Some(prepare_run::failed_status(&error));
                        started = false;
                    }
                    // The opening lines go out whether or not a runner follows them. F1.7's Describe
                    // stage now speaks here too (see below), so the log is no longer the last word;
                    // what still has no runner behind it are the stages of F1.2/F1.3/F1.8-F1.10, which
                    // is why the bar sits at zero rather than pretending — and why ⏹ still has
                    // something to stop here.
                    Some((prepare_run::Start::Started { lines, .. }, tree)) => {
                        for line in &lines {
                            ui::log_line(line);
                        }
                        // S5: audio.cpp is asked before any stage runs. A server that is down or not
                        // serving what Prepare needs refuses the press here -- no run opens, and the
                        // sentence goes on the status line the way S1's refusals do.
                        if let Some(why) = super::prepare_guard::why_not(&asked) {
                            ui::log_line(&format!("!!! {why}"));
                            refusal = Some(why);
                            started = false;
                        }
                        // F1.7's Describe stage speaks here too: one forwarder per footage source,
                        // which PLANS what would be sent and prints it. Nothing is spawned and no
                        // vision server is contacted — the requests themselves arrive with the runner
                        // rounds; what lands now is the plan the person can read while ▶ is pressed.
                        let freq = asked.interval;
                        crate::stop_legs::enter_describe();
                        for source in &asked.sources {
                            if !source.footage {
                                continue;
                            }
                            // The lane Describe files a source's work under is its file name minus the
                            // extension — the same rule `prepare_run::lane` and `frames.rs` use, so
                            // this stage reads exactly the folder F1.6 wrote.
                            let lane = source
                                .path
                                .rsplit('/')
                                .next()
                                .unwrap_or(&source.path)
                                .split_once('.')
                                .map(|(stem, _)| stem.to_string())
                                .unwrap_or_else(|| source.path.clone());
                            for line in describe::stage_log_for(&tree, &lane, freq, &[]) {
                                ui::log_line(&line);
                            }
                        }
                        crate::stop_legs::leave_describe();
                        // F1.13's session word list is NOT built here: it dresses its words from files the
                        // stages write (the aligner's tokens, the raw transcript, `transcript.fixed.tsv`),
                        // so it is a row of the fix stage — see `fix_call::stage`, which puts it after the
                        // fix rows and before the merge row that runs the marking pass.
                        // F1.9's retake marking pass speaks here as well, through ONE entry point and
                        // only when the policy names this pass: under Joins or None the call returns
                        // nothing at all, so one press never speaks twice for two different passes.
                        // Marks come from the model, which is not contacted here; what lands without a
                        // server is S1's too-few-lines answer and the empty marks file it owes Cut.
                        for line in retakes::press_marking(
                            &tree,
                            &asked,
                            crate::policy::marking_pass_of(&asked.policy),
                        ) {
                            ui::log_line(&line);
                        }
                        // F1.10's join repair joins the same press through ONE entry point, gated the
                        // same way: under Retakes or None it returns nothing at all, so one press
                        // never speaks for two passes. The answers are the textedit model's and no
                        // server is contacted here; what lands without one is S1's too-few-words
                        // answer, S2's no-seam sentence, and the two files the pass owes Cut.
                        for line in joins::press_joins(
                            &tree,
                            &asked,
                            crate::policy::marking_pass_of(&asked.policy),
                        ) {
                            ui::log_line(&line);
                        }
                    }
                    // No project folder yet: nothing to clear and nothing to save into, so the press
                    // starts the run on the empty session like any other page does.
                    None => (),
                }
            }
            if !started {
                paint_run_bar(
                    play,
                    &lucky,
                    &stop_,
                    &bar.borrow(),
                    shell.borrow().page,
                    transport,
                );
                status.set_text(refusal.as_deref().unwrap_or(""));
                return;
            }
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
            if let Some(bill) = crate::ui::run_bill::line_for(run::step(shell.borrow().page)) {
                ui::log_line(&bill);
            }
            // §2: while nothing is countable the bar pulses; it stops itself when a track reports.
            crate::ui::shell_beats::arm_pulse(&progress, &bar, &queue);
            // F0.5 S2/S3: the step's own work is walked under the bookkeeping, its checkpoint asked
            // between every task; rules live in `run_stages`, this hands over the live session.
            // S3 snapshots the LIVE session: a source added after `build_window` is part of this run.
            let drive = crate::run_stages::drive_page(
                run::step(shell.borrow().page),
                &asked,
                &queue,
                &bar,
                &procs,
            );
            crate::run_stages::say_outcome(&drive, ui::log_line); // S4: a failed stage names itself in the log
                                                                  // §S7 + §S8: Prepare's ending — log lines, status sentence, page/gates/Cut refreshed.
            if run::step(shell.borrow().page) == run::Step::Prepare {
                super::prepare_guard::say_ending(&drive, &ui::main_window(), &asked, &bar, &queue);
            }
        }
        paint_run_bar(
            play,
            &lucky,
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
        if matches!(pressed, run::Pressed::Started { .. }) && started {
            crate::ui::run_close::close_step_run(
                &bar,
                &queue,
                &exchange_log,
                &status,
                &progress,
                &log,
                &log_view,
                None,
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
                owned: matches!(pressed, run::Pressed::Started { .. })
                    && refusal.is_none()
                    && !bar.borrow().status.is_empty(),
            },
            bar.borrow()
                .running
                .as_ref()
                .is_some_and(|run| run.log_expanded),
        );
        // Last write wins on the label: F1.12's refusal is why ▶ did not do what it was pressed for,
        // so it goes on screen after the bar's own (empty) status rather than under it.
        if let Some(reason) = refusal {
            status.set_text(&reason);
        }
        // F4.1: the Narrate step's own ▶. Forwarded last so the bar's opening paint cannot overwrite
        // the stage and the sentence the run wrote; the rules live in `narrate_run`, this only
        // forwards, as the Prepare and Suggest branches above do.
        if matches!(pressed, run::Pressed::Started { .. })
            && run::step(shell.borrow().page) == run::Step::Narrate
        {
            let _ = ui::press_narrate_run(&ui::main_window());
        }
        // F5.1: the Produce step's own ▶, forwarded the same way; rules and strings live in `produce_flow`.
        if matches!(pressed, run::Pressed::Started { .. })
            && run::step(shell.borrow().page) == run::Step::Produce
        {
            let _ = crate::ui::produce_page::press_produce_run(&ui::main_window());
        }
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
        // §2: the repaint below gates the gears too, so ⏹ looks them up like ▶'s handler does.
        let lucky = crate::ui::window_doors::lucky_button(&crate::ui::window::main_window())
            .expect("the run bar draws its lucky-button (§2)");
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
                &lucky,
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
        // S1: stop the player itself, not only the sentence (the leg decides if this page owns one).
        if let Some(stopped) = crate::stop_legs::stop_page_preview(shell.borrow().page, transport) {
            let held = ui::preview_player(&window);
            ui::set_preview_state(&window, held.cut_only, held.reviewing, stopped);
        }
        // S5: a stop inside Describe arms the next Prepare run to start over.
        if in_describe {
            crate::stop_legs::arm_describe_restart();
        }
        // S3/S4: report whatever is still out on the wire, off the stop's own clock (cancel_leg owns
        // the wording and the wait; window.rs only asks).
        crate::cancel_leg::report_abandoned(std::time::Duration::from_millis(200));
        // S3's kill: drain-and-signal, so nothing survives the press nor stays listed to be killed twice.
        crate::stop_legs::stop_registered(&mut procs.borrow_mut());
        // §2: the pulse belongs to the run, so it ends with the run rather than outliving it.
        crate::ui::shell_beats::stop_pulse();
        // Repaint both: stopping a run changes ▶'s face as well as ⏹'s sensitivity.
        paint_run_bar(
            &play,
            &lucky,
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
                .is_some_and(|run| run.log_expanded),
        );
    });
}
/// Where a press of the "I'm feeling lucky" gears is decided: ask [`lucky::Chain::start`] whether the
/// bar will allow it, then walk the chain one step at a time. The handler forwards and decides nothing
/// (spec/00-principles.md §5) — S1's refusal, S3's opening line, S4's skips and S6's end sentence all
/// come from `lucky.rs`.
///
/// Each step is handed over by switching to its page synchronously through the same `stack`/`guard`
/// path F0.1 uses (a step's own work belongs to the widgets of the page it runs on), logging
/// `>>> run: <Name>`, and then reporting the outcome back with [`lucky::Chain::next`]. Every body is the
/// real one — Prepare and Cut walk their stages through [`crate::run_stages::drive_page`], Narrate and
/// Produce go through their own flows — and all of that lives in [`crate::ui::lucky_run::press`].
/// F0.4's driver body: the refusal, the bookkeeping, every step's real work and the end sentence all
/// live in [`crate::ui::lucky_run`]; this handler only hands its handles over (spec/00-principles.md §5).
#[allow(clippy::too_many_arguments)] // 🎲 forwards to `lucky_run`, which decides across the whole bar
pub(crate) fn wire_lucky(
    lucky_button: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    shell: &Rc<RefCell<Shell>>,
    stack: &adw::ViewStack,
    guard: &Rc<std::cell::Cell<bool>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &Rc<RefCell<runqueue::Queue>>,
    exchange_log: &Rc<RefCell<exchanges::RunLog>>,
    procs: &Rc<RefCell<run::Subprocesses>>,
    project: Project,
) {
    let button = lucky_button.clone();
    let handles = crate::ui::lucky_run::Handles {
        bar: bar.clone(),
        shell: shell.clone(),
        stack: stack.clone(),
        guard: guard.clone(),
        status: status.clone(),
        progress: progress.clone(),
        log: log.clone(),
        log_view: log_view.clone(),
        queue: queue.clone(),
        exchange_log: exchange_log.clone(),
        procs: procs.clone(),
    };
    lucky_button.connect_clicked(move |_| crate::ui::lucky_run::press(&button, &handles, &project));
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
        // The button's face and its grey follow the state the press just changed (S1, §D's ⏸ rule).
        crate::ui::cut_cut_play::paint(&window);
    });
}
