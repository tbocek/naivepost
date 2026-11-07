//! The Produce page's run: the press on ▶, the encode, and what the page does when it finishes.

use crate::ui::narrate_page::say;
use crate::ui::produce_page::*;
use crate::ui::produce_page_rows::{session_tree, words_half};
use adw::prelude::*;

/// The F5.1 run, driven from the page; every decision and string is `produce_flow`'s (spec/00-principles.md §5).
/// The halves arrive as closures because headless there is no sd.cpp or ffmpeg. The gate and snapshot are read here
/// on the window thread; the halves walk on a worker and the run's ending closes the bar, so ⏹ reaches it (§03-shell §1).
/// Answers the sentence when the press ended here, empty while it goes on.
pub fn press_produce_run(window: &adw::ApplicationWindow) -> String {
    crate::ui::produce_languages::note_context_at_press();
    let s = read_state();
    let project = crate::ui::window::live_project();
    let tree = crate::layout::Tree::new(crate::startup::open_dir()).ok();
    // S3: the Cut page's own segments win, so an unsaved tweak still renders (`window_preview::newest_review_cut`).
    let cut = match tree.as_ref() {
        Some(tree) => crate::produce_flow::what_is_the_cut(
            Some(crate::ui::window_preview::newest_review_cut()),
            tree,
        ),
        None => crate::ui::window_preview::newest_review_cut(),
    };
    if cut.segs.is_empty() {
        crate::ui::run_close::close_window_run();
        return say(window, crate::cut::NO_CUT_YET);
    }
    let settings = produce_settings(&s);
    let Some(tree) = tree.as_ref() else {
        return finish_produce(
            window,
            &s,
            &project,
            cut,
            settings,
            crate::produce_flow::Gate::Encode,
        );
    };
    // S2: up to date is the stamp's question, asked over the narration too (§F5.3): `gate_stamp` reads
    // `narrate/narration.json` itself, and measures the shape with the helper the snapshot uses.
    let voice = if s.voice.is_empty() {
        None
    } else {
        Some(s.voice.as_str())
    };
    let current = crate::produce_stamp::gate_stamp(
        tree,
        &settings,
        &cut,
        &project.sources,
        voice,
        crate::ui::produce_snapshot::first_footage_shape(
            tree,
            &crate::ui::cut_folds::page_recordings(),
        ),
    );
    let gate = crate::produce_flow::gate(
        crate::produce_stamp::skip_encode(
            crate::produce_stamp::read_stamp(tree).as_deref(),
            &current,
        ),
        tree.final_video(container_name(parse_container(&s.container)))
            .exists(),
    );
    finish_produce(window, &s, &project, cut, settings, gate)
}

/// The S2 answer acted on, then S4–S6. A `ConfirmOverwrite` gate opens the dialog and the encode runs from its
/// response handler on Overwrite (`encode_and_finish`).
pub(crate) fn finish_produce(
    window: &adw::ApplicationWindow,
    s: &ProduceState,
    project: &crate::project::Project,
    cut: crate::cut::Cut,
    settings: crate::project::Produce,
    gate: crate::produce_flow::Gate,
) -> String {
    if gate == crate::produce_flow::Gate::ConfirmOverwrite {
        let video = crate::layout::Tree::new(crate::startup::open_dir())
            .map(|t| t.final_video(container_name(parse_container(&s.container))))
            .unwrap_or_default();
        let (win, st, prj, ct, set) = (
            window.clone(),
            s.clone(),
            project.clone(),
            cut.clone(),
            settings.clone(),
        );
        let asked_on = window.clone();
        return crate::ui::produce_confirm::confirm_then(
            window,
            &video,
            move || {
                encode_and_finish(
                    &win,
                    &st,
                    &prj,
                    &ct,
                    &set,
                    crate::produce_flow::Gate::Encode,
                );
            },
            move |asked| {
                crate::ui::run_close::close_window_run();
                if let Some(status) = crate::ui::window::find_status(asked_on.upcast_ref()) {
                    status.set_text(asked);
                }
            },
        );
    }
    encode_and_finish(window, s, project, &cut, &settings, gate)
}

/// F5.6's answer onto the page once the run lands: what the words half wrote to `publish.json` replaces what the page
/// showed, and the plates repaint. A part the run left empty keeps the page's.
pub(crate) fn take_written_publish(window: &adw::ApplicationWindow) {
    let tree = session_tree();
    if !crate::publish::is_written(&tree) {
        return;
    }
    let Ok(written) = crate::publish::load(&tree) else {
        return;
    };
    mutate(|s| {
        let take = |held: &mut String, from: &str| {
            if !from.trim().is_empty() {
                *held = from.to_string();
            }
        };
        take(&mut s.title, &written.title);
        take(&mut s.thumb_title, &written.thumb_title);
        take(&mut s.instruction, &written.prompt);
        take(&mut s.negative, &written.negative);
        take(&mut s.description, &written.description);
        if !written.frames.is_empty() {
            s.frames = written.frames.clone();
            s.own = written.own;
        }
        // The seeding travels with the printed title, or the plate stays untitled while `thumbnail.png` carries it.
        s.title_seeded = s.title_seeded || written.title_seeded;
        if written.title_box.is_some() {
            s.title_box = written.title_box;
        }
        if !written.texts.is_empty() {
            s.texts = written.texts.clone();
        }
    });
    keep_record();
    refresh(window);
}

pub(crate) fn snapshot_run(
    s: &ProduceState,
    project: &crate::project::Project,
    cut: &crate::cut::Cut,
    settings: &crate::project::Produce,
) -> (
    crate::ui::produce_snapshot::Snapshot,
    crate::produce_flow::Run,
) {
    let snap = crate::ui::produce_snapshot::gather(
        &session_tree(),
        cut,
        settings,
        project,
        &crate::ui::cut_folds::page_recordings(),
        crate::ui::produce_snapshot::publish_of_state(s),
    );
    let run = crate::produce_flow::Run {
        cut: cut.clone(),
        lines: snap.lines.clone(),
        // Cloned: F5.5 S5 runs after the render and needs the container and codec.
        settings: settings.clone(),
        sources: crate::run::snapshot_sources(project),
        // The crop, title box and image row have no page row, so merge `publish.json`'s record or the thumbnail redraws without its crop.
        publish: Some(crate::ui::produce_snapshot::publish_of(&snap)),
        aspect: String::new(),
        clips: s.clips,
        publish_written: s.publish_written,
        voice: s.voice.clone(),
    };
    let run = crate::produce_flow::Run {
        aspect: run.cut.aspect.clone(),
        ..run
    };
    (snap, run)
}

pub(crate) fn press_run() -> Option<crate::produce_flow::Run> {
    crate::ui::produce_languages::note_context_at_press();
    let cut = crate::ui::window_preview::newest_review_cut();
    if cut.segs.is_empty() {
        return None;
    }
    let project = crate::project::load(session_tree().dir()).ok()?;
    Some(snapshot_run(&read_state(), &project, &cut, &row_settings()).1)
}

pub(crate) fn encode_and_finish(
    window: &adw::ApplicationWindow,
    s: &ProduceState,
    project: &crate::project::Project,
    cut: &crate::cut::Cut,
    settings: &crate::project::Produce,
    gate: crate::produce_flow::Gate,
) -> String {
    let language = project.policy.language.value.clone();
    let skip = gate == crate::produce_flow::Gate::Skip;
    // S3: everything the halves read is gathered here, once, on the GUI thread; a leg re-reading the window mid-render would see later moves.
    let (snap, run) = snapshot_run(s, project, cut, settings);
    if skip {
        crate::ui::window::log_line(crate::produce_flow::SKIP_LOG);
    } else {
        crate::ui::window::log_line(&crate::produce_flow::opening_line(&run, false));
    }
    let carried = crate::ui::produce_worker::Carried::take();
    let settings = settings.clone();
    let w = window.clone();
    let open = std::rc::Rc::new(std::cell::Cell::new(true));
    let still_open = std::rc::Rc::clone(&open);
    crate::ui::produce_progress::follow(window, move || still_open.get());
    crate::ui::background::run(
        "produce",
        move || {
            carried.load();
            // The run's `llm/` page is this thread's (§7 keeps one per thread).
            crate::exchange_leg::note_run_started();
            if skip {
                if let Err(why) = words_half(&run) {
                    crate::ui::window::log_line(&crate::produce_flow::words_failed(&why));
                }
                crate::ui::produce_languages::tag_page(&settings, &language);
                crate::exchange_leg::note_run_ended();
                return None;
            }
            // Both halves through `run_with`; a words failure is logged and never stops the render (F5.6, F5.2).
            let ending = crate::produce_flow::run_with(
                &run,
                |run| match words_half(run) {
                    Ok(()) => (true, None),
                    Err(why) => {
                        crate::ui::window::log_line(&crate::produce_flow::words_failed(&why));
                        (false, Some(why))
                    }
                },
                |run| crate::ui::produce_snapshot::render_half(run, &snap),
            );
            // F5.5 S5: after both halves; S6 keeps the render's error as the verdict.
            crate::ui::produce_languages::tag_page(&settings, &language);
            crate::exchange_leg::note_run_ended();
            Some(ending)
        },
        move |ending| {
            open.set(false);
            take_written_publish(&w);
            let dir = crate::startup::open_dir();
            if let Ok(project) = crate::project::load(&dir) {
                crate::ui::window::paint_readouts_for(&w, &project, &dir);
            }
            let Some(ending) = ending else {
                crate::ui::run_close::close_window_run();
                say(&w, crate::produce_flow::SKIP_LOG);
                return;
            };
            crate::ui::window::log_line(&ending.log);
            if !ending.progress.is_empty() {
                crate::ui::window::log_line(&ending.progress);
            }
            crate::ui::run_close::close_window_run();
            say(&w, ending.status);
        },
    );
    String::new()
}
