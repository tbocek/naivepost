//! F5.1 "Where it runs" (spec/08-produce.md): ▶ on the Produce page returns at once with the run open; the
//! render walks on a worker, and its ending lands on the status line and closes the bar it opened, so ⏹ can
//! reach the run while it walks (§03-shell §1). The first rewrite ran the whole render inside the click.
//!
//! The render is made slow on purpose: the scripted spawner (the program's own seam for "no ffmpeg here")
//! takes a while over every command, so a press that walked the render inline could not return in time.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::produce_render as render;
use naivepost::project::Project;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;

static RAN: AtomicBool = AtomicBool::new(false);

/// How long the scripted encoder takes over each command: long beside a press's own work.
const SLOW: Duration = Duration::from_millis(700);

/// How long the press may take to come back: the snapshot and the opening line, nothing more.
const PRESS_LIMIT: Duration = Duration::from_millis(600);

fn base_project() -> Project {
    let mut project = Project::default();
    project.sources.push(naivepost::project::Source {
        path: "/media/take.mp4".into(),
        footage: true,
        ..Default::default()
    });
    project
}

fn two_clips() -> Vec<Seg> {
    (0..2)
        .map(|n| Seg {
            s: n as f64 * 10.0,
            e: n as f64 * 10.0 + 10.0,
            cam: 0,
            ..Default::default()
        })
        .collect()
}

/// The slow encoder: each command's own output appears on disk after [`SLOW`].
fn slow_spawn() -> naivepost::produce_translate::SpawnSlot {
    std::sync::Arc::new(|command: &naivepost::produce_exec::Command| {
        std::thread::sleep(SLOW);
        if let Some(out) = command.argv.last() {
            let path = std::path::Path::new(out);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, b"a rendered file of some length");
        }
        Ok(())
    })
}

#[test]
fn f5_1_where_it_runs_the_press_returns_with_the_run_open() {
    let root = common::temp_dir_created("f51-where");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    common::one_app_round(|app| {
        let window = ui::build_window(app, &base_project(), Page::Produce.label());
        window.present();
        common::settle();
        let cut = Cut {
            segs: two_clips(),
            aspect: "16:9".into(),
            ..Default::default()
        };
        ui::seed_review_cut(&window, &cut);
        let mut state = ui::produce_page::read_state();
        state.clips = cut.segs.len();
        state.seconds = 20.0;
        ui::produce_page::set_state(state);
        ui::produce_page::refresh(&window);
        common::settle();
        naivepost::produce_translate::set_spawn_for_test(slow_spawn());

        let pressed = Instant::now();
        ui::play_button(&window)
            .expect("the run bar has a play-button")
            .emit_by_name::<()>("clicked", &[]);
        let took = pressed.elapsed();
        assert!(
            took < PRESS_LIMIT,
            "▶ came back after {took:?}: the render walked inside the click"
        );
        assert_eq!(
            ui::running_step(&window),
            Some(naivepost::run::Step::Produce),
            "the run is open while the render walks, so ⏹ can reach it"
        );
        let opening = ui::window_logs().into_iter().rev().find(|line| {
            line.starts_with(">>> transcoding ") || line.starts_with(">>> producing ")
        });
        assert!(
            opening.is_some(),
            "S4's opening line is said at the press, before the halves walk"
        );

        assert!(
            ui::background::settle_for_test(Duration::from_secs(120)),
            "the render's worker answered"
        );
        let ending = common::status_text(&window);
        assert!(
            ending == render::STAGE_DONE
                || ending == render::failure_log()
                || ending == render::stopped_log(),
            "S6's ending word lands on the status line when the walk does: {ending}"
        );
        assert_eq!(
            ui::running_step(&window),
            None,
            "and the run's own ending closed the bar"
        );
        RAN.store(true, Ordering::SeqCst);
        window.close();
        common::settle();
    });
    assert!(RAN.load(Ordering::SeqCst), "the press check never ran");
}
