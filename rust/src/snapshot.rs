//! `--snapshot <screen> --project <fixture dir> --out <file.png>`: renders one screen without a host
//! display (run under `xvfb-run` with `GSK_RENDERER=cairo`). Screens are named after the spec's images.

use std::path::Path;
use std::process::ExitCode;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_line;
use crate::layout;
use crate::new_project;
use crate::project;
use crate::snapshot_face::{face_of, seed_snapshot_frame, show_the_log, write_face_png, write_png};
use crate::ui;

fn screen_page(screen: &str) -> Option<&'static str> {
    match screen {
        "03-window" | "03-settings" | "03-sources" | "03-new-confirm" => Some("Prepare"),
        // Program-side screens: no `spec/img` counterpart.
        "03-log-link" => Some("Prepare"),
        "04-prepared" => Some("Prepare"),
        "03-runbar" | "03-runbar-busy" => Some("Prepare"),
        "04-prepare" | "04-prompt-picker" => Some("Prepare"),
        "05-cut" | "05-add" | "05-fold" | "05-rows" | "05-split" | "05-trim" | "05-remove"
        | "05-lanes" | "05-copy" | "05-insert" | "05-suggest" | "05-words" => Some("Cut"),
        "06-preview" | "06-zoom" | "06-speed" | "06-text" | "06-volume" | "06-label"
        | "06-speeds" | "06-decorations" | "06-lane" | "06-svg" | "06-effect-menu" | "06-flows" => {
            Some("Cut")
        }
        "05-insert-form" => Some("Cut"),
        "07-narrate" | "07-take" => Some("Narrate"),
        "08-produce" | "08-words" => Some("Produce"),
        _ => None,
    }
}

pub fn run(
    screen: &str,
    dir: &Path,
    out: &Path,
    on: crate::snapshot_ready::ShotOn,
) -> Result<(), String> {
    let page = screen_page(screen).ok_or_else(|| format!("unknown screen {screen}"))?;
    if !dir.join(project::PROJECT_FILE).is_file() {
        return Err(format!("no {} in {}", project::PROJECT_FILE, dir.display()));
    }
    let model = project::load(dir)?;

    gtk::init().map_err(|err| err.to_string())?;
    // No animations: a picture taken mid-animation depends on when it was taken.
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    // Non-registering: a snapshot must not claim the app id or wait on a session bus.
    let app = gtk::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let out = out.to_path_buf();
    let screen = screen.to_string();
    let dir = dir.to_path_buf();
    app.connect_activate(move |app| {
        // `bench_box` reads the requested row once, so ask before the window is built. `04-prompt-picker` asks
        // for nothing: a request left over from an earlier shot would open another row.
        if screen.as_str() == "04-prepare" {
            ui::set_bench_open_row(2);
        }
        // Only `04-prepare` shows the override mark and Reset; stage it before the window (read once).
        if screen.as_str() == "04-prepare" {
            crate::snapshot_bench::stage_edited_row(2);
        }
        // Take the folder on as Open does, before the window exists: `build_window` seeds session spans from it.
        crate::startup::set_open_dir(&dir);
        // `build_window` resets this door, and the Produce plates resolve `project:` paths at paint time.
        crate::ui::produce_paint::note_open_project(&dir);
        if screen_page(&screen) == Some("Narrate") {
            // Before the window is shown: a scroller hidden during `refresh` (§A.7) stays hidden.
            if let Some(tree) = crate::ui::window::narrate_session_tree() {
                let band = seed_voice_reference(&tree);
                println!("snapshot: take band seeded = {band}");
            }
        }
        // §4's rows must be in the session before the page is built (`sources_list` draws at build time).
        let staged_model =
            if matches!(screen.as_str(), "03-sources" | "04-prepare" | "04-prompt-picker") {
                crate::snapshot_sources::staged()
            } else {
                model.clone()
            };
        let window = ui::build_window(app, &staged_model, page);
        // Seed the project's own `cut/cut.json` over `build_window`'s default; a missing file draws the empty hint.
        if let Some(loaded) = layout::Tree::new(&dir).ok().and_then(|tree| cut::load(&tree).ok()) {
            ui::seed_review_cut( &loaded);
            // `build_window` drew the camera rows before the seed landed.
            ui::refresh_camera_rows(&window);
        }
        if screen == "05-lanes" {
            let lanes_shot = cut::Cut {
                segs: vec![
                    cut::Seg { s: 10.0, e: 30.0, cam: 0, ..Default::default() },
                    cut::Seg { s: 30.0, e: 60.0, cam: 0, quiet: vec!["mic-lav".into()], ..Default::default() },
                ],
                lanes: vec![cut::Lane {
                    name: "mic-lav".to_string(),
                    src: "project:sources/mic-lav.wav".to_string(),
                    at: 12.0,
                    off: 0.0,
                    dur: 45.0,
                }],
                ..Default::default()
            };
            ui::set_session_recordings(&[crate::timeline::Recording {
                base: "session-tape".to_string(),
                start: 0.0,
                end: 120.0,
            }]);
            // The plate's channel layout needs the lane file to exist (ffprobe), where `layout::resolve` points.
            let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let wav = crate::layout::resolve(
                &root,
                &crate::startup::session_dir(&root),
                "project:sources/mic-lav.wav",
            );
            if let Some(folder) = wav.parent() {
                let _ = std::fs::create_dir_all(folder);
            }
            let _ = write_silence_wav(&wav, 8000, 8000 * 45);
            ui::seed_review_cut( &lanes_shot);
            ui::refresh_camera_rows(&window);
            let plates = crate::cut_lanes::plates(&lanes_shot.lanes);
            println!("snapshot: 05-lanes plates | {}", plates.join(" | "));
            ui::set_line_position(&window, cut_line::LinePos { t: 40.0 });
            seed_snapshot_frame(&dir, "session-tape", 0.0);
            ui::cut_picture::set_preview_camera_base("session-tape");
            ui::cut_picture::set_snapshot_dir(&dir);
        }
        // §F2.5's band is drawn under the ruler: grow `track-layers` as `repaint_page` does after layout settles.
        if screen == "05-lanes" {
            crate::ui::cut_tracks::fit_band_height_for_window(
                &window,
                &crate::ui::cut_view::band_scene_for(),
            );
        }
        if screen == "05-cut" || screen == "05-fold" {
            if screen == "05-cut" {
                crate::snapshot_layout::stage_cut_transport(&window);
            }
            let folded_segs = vec![
                cut::Seg { s: 10.0, e: 30.0, cam: 0, ..Default::default() },
                cut::Seg { s: 40.0, e: 60.0, cam: 0, ..Default::default() },
            ];
            // F2.8 S2: the band and wave rows exist only with a lane and the session tape.
            let folded = cut::Cut {
                segs: folded_segs,
                lanes: vec![cut::Lane {
                    name: "mic-lav".to_string(),
                    src: "project:sources/mic-lav.wav".to_string(),
                    at: 12.0,
                    off: 0.0,
                    dur: 46.0,
                }],
                ..Default::default()
            };
            ui::set_session_recordings(&[crate::timeline::Recording {
                base: "session-tape".to_string(),
                start: 0.0,
                end: 120.0,
            }]);
            ui::seed_review_cut( &folded);
            // Badges are only recorded once drawn, so refresh before pressing a fold.
            ui::refresh_fold_badges(&window);
            ui::press_fold_gap(&window, 1);
            ui::refresh_camera_rows(&window);
            // The fixture ships no extracted frames; write one PNG named by `clock::frame_name`.
            seed_snapshot_frame(&dir, "session-tape", 0.0);
            // Tell the page which session the preview watches before repainting.
            ui::cut_picture::set_preview_camera_base("session-tape");
            ui::cut_picture::set_snapshot_dir(&dir);
            // Called again after layout: before `present()` the panel is 0x0, so the first frame pick is wrong.
            let _painted = crate::ui::cut_picture::refresh(&window);
            // Zoom via the toolbar's + door so badge **31** appears because the tape outgrows the view (§A);
            // six steps from 4 px/s land on 15 px/s.
            for _ in 0..6 {
                ui::press_zoom(&window, true);
            }
        }
        // §2's busy face, driven through the program's own doors: seed a cut, place the red line, press the
        // page's ▶ (without a line `press_recording` answers `NoFootage`). Ends on ⏸ "Pause".
        if screen == "03-runbar-busy" {
            // Seeded before the tab switch: `run()` seeds the cut slot only after `build_window`.
            ui::seed_review_cut(
                &cut::Cut {
                    segs: vec![
                        cut::Seg { s: 10.0, e: 30.0, cam: 0, ..Default::default() },
                        cut::Seg { s: 40.0, e: 60.0, cam: 0, ..Default::default() },
                    ],
                    ..Default::default()
                },
            );
            ui::tab_button(&window, crate::shell::Page::Cut)
                .expect("the shell has a Cut tab")
                .emit_clicked();
            // Both doors: `set_line_position` places the drawn line, `set_playhead` the player's clock that
            // `preview::press_recording` plays from (the line alone answers `NoFootage`).
            ui::set_line_position(&window, crate::cut_line::LinePos { t: 15.0 });
            ui::set_playhead(&window, 15.0);
            ui::play_recording_button(&window).expect("the Cut page carries play-recording-button")
                .emit_by_name::<()>("clicked", &[]);
            // §F0.2 S2: this press is the one thing that repaints the bar; it pauses the preview.
            ui::play_button(&window).expect("the run bar has a ▶").emit_by_name::<()>("clicked", &[]);
            let parked = ui::preview_player().transport;
            let status = ui::state(&window).status;
            assert!(
                !parked.playing && parked.started,
                "S2 should have parked the preview: transport {parked:?}, status {status:?}"
            );
            // `Transport::cued`: ▶ still owes a resume. Read off the widget so a bar that forgot to repaint fails here.
            let parked_face = face_of(&window);
            assert_eq!(
                parked_face.0,
                crate::run::PLAY_ICON,
                "a parked preview leaves ▶ as the step's button (§2)"
            );
            assert_eq!(parked_face.1, crate::run::PLAY_TOOLTIP, "and §2's own word for it");
            assert!(parked_face.2, "a parked preview still has something to stop");
            ui::play_button(&window).expect("the run bar has a ▶").emit_by_name::<()>("clicked", &[]);
            assert!(ui::preview_player().transport.playing, "the second press resumes");
            let busy = face_of(&window);
            assert_eq!(busy.0, crate::run::PAUSE_ICON, "a playing preview turns ▶ into ⏸ (§2)");
            assert_eq!(busy.1, crate::run::PAUSE_TOOLTIP, "and the tooltip is §2's own word for it");
            assert!(busy.2, "and there is still something to stop");
            // A 16 px glyph is illegible in a full-window shot, so the face goes to `shots/03-runbar-busy.face.png`.
            println!("snapshot: 03-runbar-busy face = {} {:?}, stop sensitive = {}", busy.0, busy.1, busy.2);
        }
        // Staged in `snapshot_insert`.
        if screen == "05-insert" || screen == "05-insert-form" {
            let said = crate::snapshot_insert::stage(&window, &dir);
            let words: Vec<String> = said.iter().map(|line| line.replace('|', "/")).collect();
            println!("snapshot: 05-insert | {}", words.join(" | "));
            println!("snapshot: 05-insert band | {}", crate::snapshot_insert::describe());
            ui::cut_view::repaint_page(&window);
            ui::refresh_camera_rows(&window);
        }
        // §F2.9: the Cut page after the whole flow, driven through the doors the buttons fire, never by poking state.
        if screen == "05-copy" {
            // One take that does NOT start at 0:00: a paste must ask the tape for the material under the span.
            ui::set_session_recordings(&[crate::timeline::Recording {
                base: "session-tape".to_string(),
                start: 10.0,
                end: 130.0,
            }]);
            let kept = cut::Cut {
                segs: vec![
                    cut::Seg { s: 10.0, e: 40.0, cam: 0, ..Default::default() },
                    cut::Seg { s: 50.0, e: 80.0, cam: 0, ..Default::default() },
                ],
                ..Default::default()
            };
            // A sound paste refuses a recording the session's file list does not hold, so add it to the session too.
            ui::session_handle().borrow_mut().sources.push(crate::project::Source {
                path: "mic-lav.wav".to_string(),
                footage: true,
                ..Default::default()
            });
            ui::seed_review_cut( &kept);
            // Deepen the zoom: at 4 px/s a 12 s copy is a smudge.
            for _ in 0..6 {
                ui::press_zoom(&window, true);
            }
            // S3: a lane survives the paste, so it goes first. Windows the file from the FILE second it was taken at.
            ui::draw_selection(
                &window,
                crate::cut_select::Surface::PictureRow(0),
                None,
                20.0,
                32.0,
            );
            let copied = ui::press_copy(&window);
            ui::set_line_position(&window, cut_line::LinePos { t: 70.0 });
            let said = ui::press_lane(&window);
            ui::draw_selection(
                &window,
                crate::cut_select::Surface::PictureRow(0),
                None,
                20.0,
                32.0,
            );
            let recopied = ui::press_copy(&window);
            ui::set_line_position(&window, cut_line::LinePos { t: 55.0 });
            let pasted = ui::press_paste(&window);
            // S2's sound branch: lies over TWO kept stretches (the 0:40-0:50 hole splits it).
            ui::draw_selection(
                &window,
                crate::cut_select::Surface::WaveStrip,
                Some("mic-lav.wav"),
                36.0,
                48.0,
            );
            let sound_copied = ui::press_copy(&window);
            ui::set_line_position(&window, cut_line::LinePos { t: 24.0 });
            let laid = ui::press_paste(&window);
            // S2's refusal: a hand starting inside the hole with the line at 0:41, so the copy STAYS in hand.
            ui::draw_selection(
                &window,
                crate::cut_select::Surface::WaveStrip,
                Some("mic-lav.wav"),
                42.0,
                46.0,
            );
            let held_again = ui::press_copy(&window);
            ui::set_line_position(&window, cut_line::LinePos { t: 41.0 });
            let refused = ui::press_paste(&window);
            // Queue the repaint the view's own door would; otherwise the pre-press picture is painted.
            ui::cut_view::repaint_page(&window);
            println!("snapshot: 05-copy | {copied} | {said} | {recopied} | {pasted}");
            println!("snapshot: 05-copy sound | {sound_copied} | {laid} | {held_again} | {refused}");
            ui::refresh_camera_rows(&window);
        }
        // §F2.14: §S1's first refusal beside a suggestion landed through the doors ▶ fires (`refuse_suggest_run`, `press_landed`).
        if screen == "05-suggest" {
            let staged = crate::snapshot_suggest::stage();
            // Without a declared session the strip has no seconds to lay out.
            ui::set_session_recordings(&[crate::timeline::Recording {
                base: "session-tape".to_string(),
                start: 0.0,
                end: 62.0,
            }]);
            ui::window_policy::write_policy_field( "cutMode", "model", "this shot", project::Origin::User);
            let refused = ui::cut_suggest_land::refuse_suggest_run( true)
                .unwrap_or_else(|| crate::suggest::BUSY.to_string());
            ui::window::set_status_line(&window, &refused);
            // A fresh scratch session folder has no transcript, so §S1's word gate answers before any socket is dialled.
            let pressed = crate::snapshot_suggest::press_through_page(&window);
            println!("snapshot: 05-suggest press said: {pressed}");
            let landed = crate::snapshot_suggest::press_landed(&window);
            let segs = ui::review_cut_segs_count();
            let seconds = crate::cut_screen::cut_seconds(&cut::Cut {
                segs: ui::review_cut_segs(),
                ..Default::default()
            });
            println!("snapshot: 05-suggest | {refused} | {landed} | {segs} segs | {} s", seconds.round());
            // Read back so the shot proves the landing reached a file.
            let written = staged
                .as_ref()
                .map(|tree| tree.cut_json().is_file())
                .unwrap_or(false);
            println!("snapshot: 05-suggest cut.json written: {written}");
            // The refreshes a mutating verb does after publishing (see `cut_verb_press`'s tail).
            ui::refresh_fold_badges(&window);
            ui::refresh_camera_rows(&window);
            ui::refresh_effects_lane(&window);
            // The form column is built once for an empty cut; only a verb repaints it.
            crate::ui::window::refresh_cut_readouts(&window);
        }
        // §F2.14 S2a: ▶ in the WORDS mode through `cut_suggest_words_run::press_words`, with the session folder
        // staged by `snapshot_words`.
        if screen == "05-words" {
            let staged = crate::snapshot_words::stage();
            let project = staged.as_ref().and_then(crate::snapshot_words::stage_session);
            // The words arm reads footage rows off the project the WINDOW holds, so swap in the staged one, then
            // rescan via `refresh_page_spans` as a real Open does.
            if let Some(project) = project {
                ui::session_handle().replace(project);
                ui::cut_session::refresh_page_spans(&crate::snapshot_words::base_cut());
                ui::window_policy::write_policy_field( "cutMode", "words", "this shot", project::Origin::User);
                // Re-seed the page's cut here: `run`'s early seed read the `--project` fixture's `cut.json`.
                ui::cut_suggest_land::point_session_at(staged.clone());
                // Publish through `write_cut_at`, §7's one writer for `cut.json`.
                if let Some(tree) = staged.as_ref() {
                    crate::cut_save::write_cut_at(&crate::snapshot_words::base_cut(), tree)
                        .expect("snapshot: the staged cut.json is writable");
                }
                ui::seed_review_cut( &crate::snapshot_words::base_cut());
                ui::refresh_camera_rows(&window);
                ui::refresh_fold_badges(&window);
                let said = crate::snapshot_words::press_through_page(&window);
                println!(
                    "snapshot: 05-words | {said} | {} segs",
                    ui::review_cut_segs_count()
                );
                let written = staged
                    .as_ref()
                    .map(|tree| tree.cut_json().is_file())
                    .unwrap_or(false);
                println!("snapshot: 05-words cut.json written: {written}");
                ui::refresh_fold_badges(&window);
                ui::refresh_camera_rows(&window);
                ui::refresh_effects_lane(&window);
                crate::ui::window::refresh_cut_readouts(&window);
                // The zoom floor is set by the DECLARED tape (§A); grow and repaint the band at the settled width.
                let view = ui::cut_view::surface_width(&window);
                ui::cut_view::set_pps(ui::cut_view::floor_pps(view));
                ui::cut_view::repaint_page(&window);
            }
        }
        // Screens that stage their own state live in their own modules, run after every Cut-page arm laid out its strip.
        crate::snapshot_effects::paint(&window, &dir, &out, &screen);
        crate::snapshot_preview::paint(&window, &dir, &out, &screen);
        crate::snapshot_narrate::stage(&window, &dir, &screen);
        crate::snapshot_produce::paint(&window, &screen, &dir);
        if matches!(screen.as_str(), "05-add" | "05-split" | "05-remove") {
            let said = crate::snapshot_verbs::stage(&window, &screen, &dir);
            println!("snapshot: {screen} | {}", said.join(" | "));
        }
        if screen == "05-trim" {
            let said = crate::snapshot_trim::stage(&window, &dir);
            println!("snapshot: 05-trim | {}", said.join(" | "));
        }
        if screen == "05-rows" {
            let said = crate::snapshot_rows::stage(&window, &dir);
            println!("snapshot: 05-rows | {}", said.join(" | "));
        }
        // Written AFTER the publish above: a later `build()` (one per tab) re-reads these files.
        crate::snapshot_narrate::seed_project_files(&screen);

        // F0.8's subject is the confirmation dialog, not the window behind it.
        let subject: Option<gtk::Widget> = match screen.as_str() {
            // Staged by the `05-insert` arm; this form stays open on purpose. Painted as the whole window since a
            // form cannot be reparented out of the Cut page.
            "05-insert-form" => None,
            // §08's ✎ dialog is the subject, opened as the chip opens it.
            "08-words" => {
                ui::produce_words::open_reword(&window, 0, "Ethereum, one lecture at a time");
                let shown = ui::produce_words::open_dialog().expect("the chip opens its dialog");
                shown.set_default_size(420, 300);
                Some(shown.upcast())
            }
            // No parent: a transient is kept alive by the window, and the run ends when the last window closes.
            "03-new-confirm" => {
                let shown = ui::new_project_confirm(None, &new_project::confirm_detail(&dir));
                shown.set_default_size(560, 240);
                shown.present();
                Some(shown.upcast())
            }
            // `no_probe` fails every row; press the ffmpeg row once through `press_test` with `checks::ffmpeg_verdict`'s
            // own sentence, as the spec's picture shows a passed ffmpeg test.
            "03-settings" => {
                // §5's picture is of a configured machine.
                let shown = ui::settings::build_from(
                    None,
                    ui::settings::no_probe(),
                    &crate::snapshot_settings::pictured_conf(),
                );
                shown.present();
                crate::snapshot_settings::remember(&shown);
                crate::snapshot_settings::stage_ffmpeg_row();
                crate::snapshot_settings::stage_model_list(&shown);
                Some(shown.upcast())
            }
            // The subject is the picker LIST (see `snapshot_subjects::subject_takes_the_name`): a DropDown pops its
            // rows on their own surface, absent from the window's render node. Row 2 = "Describe" per `bench::ROWS`.
            "04-prompt-picker" => None,
            // §1's picture is captioned "Prepare page, LOG OPEN": open and fill the expander through the door §7's recorder writes through.
            "03-window" => {
                let step = crate::run::step(crate::shell::Page::Prepare).label();
                ui::note_exchange_line(&format!(">>> {step}: 451.4 kB of text and 0 image(s) went to the LLM"));
                ui::note_exchange_line(&format!(">>> {step}: 28.6 kB came back in 2m39s"));
                ui::note_exchange_line(&crate::exchanges::link_line("llm/prepare-page.html"));
                crate::snapshot_band::rest_band(&window);
                None
            }
            "03-sources" => {
                println!("snapshot: 03-sources rows = {}", crate::snapshot_sources::stage(&window));
                None
            }
            // Lines go in through `ui::note_exchange_line`, which fills the expander as a live run does.
            "03-log-link" => {
                let step = crate::run::step(crate::shell::Page::Prepare).label();
                ui::note_exchange_line(&format!(">>> {step}: 451.4 kB of text and 0 image(s) went to the LLM"));
                ui::note_exchange_line(&format!(">>> {step}: 28.6 kB came back in 2m39s"));
                ui::note_exchange_line(&crate::exchanges::link_line("llm/prepare-page.html"));
                None
            }
            // §S7/S8 driven through the ▶ handler's door (`ui::prepare_guard::say_ending`) over a scratch
            // `.naivepost` holding one file per prepare/ subfolder.
            "04-prepared" => {
                let root = std::env::temp_dir().join(format!("naivepost-snap-04-prepared-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&root);
                let session = root.join("session.naivepost");
                for rel in [
                    "prepare/inputs/lecture/meta.env",
                    "prepare/describe/lecture/events.tsv",
                    "prepare/transcript/lecture.tsv",
                ] {
                    let path = session.join(rel);
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&path, "x");
                }
                let bar = ui::run_bar(&window).expect("the window carries a run bar");
                let queue = ui::run_queue(&window).expect("the window carries a run queue");
                // The ending counts from `save_apply::save_target()`, so take the scratch project on first, as Open does.
                crate::startup::set_open_dir(&session);
                // A run has to be OPEN for its ending to be one: `start_run` is what §S7's "fraction 1" and §S4's close act on.
                let mut exchange = crate::exchanges::RunLog::new();
                crate::runqueue::start_run(
                    &mut bar.borrow_mut(),
                    &mut queue.borrow_mut(),
                    &mut exchange,
                    crate::run::Step::Prepare,
                    crate::run::snapshot_sources(&model),
                );
                crate::ui::run_close::note_started_run(&bar);
                ui::prepare_guard::say_ending(
                    &crate::run_stages::Drive::Done { ran: 4, skipped: 0 },
                    &window,
                    &model,
                    &bar,
                    &queue,
                );
                let view = ui::log_view(&window).expect("the log expander holds its text view");
                let status = ui::find_status(window.upcast_ref()).expect("the shell has a status line");
                crate::ui::run_close::close_step_run(
                    &bar,
                    &queue,
                    &status,
                    &ui::progress_bar(&window).expect("the shell carries a progress bar"),
                    &ui::log_expander(&window).expect("the shell carries the log expander"),
                    &view,
                );
                // §S7: the ending is the run's LAST word, so it goes back after the close.
                if !bar.borrow().status.is_empty() {
                    let said = bar.borrow().status.clone();
                    status.set_text(&said);
                }
                show_the_log(&window).expect("the shell's log opens for its own snapshot");
                // `show_the_log` paints through `paint_progress`, whose status write would otherwise be the last word.
                if !bar.borrow().status.is_empty() {
                    let said = bar.borrow().status.clone();
                    status.set_text(&said);
                }
                let _ = std::fs::remove_dir_all(&root);
                None
            }
            // F3.5's subject is the form itself; the Cut page's own column collapses to a sliver headless, so the
            // page needs a readable width, set above before `present()`.
            _ => None,
        };
        if subject.is_none() {
            // §1: every screenshot is taken at the minimum size, which `build_window` opens at; some screens ask for wider.
            if screen_page(&screen) == Some("Cut") || screen_page(&screen) == Some("Narrate") {
                // Wider only for the pass screens: effects bars laid out along seconds crowd into the leftmost tenth at 1240 px.
                let wide = matches!(screen.as_str(), "06-speeds" | "06-decorations" | "06-flows");
                window.set_default_size(if wide { 2400 } else { 1280 }, crate::shell::WINDOW_MIN_H);
            }
            if subject.is_none() {
            }
            window.present();
        }
    // Headless, nothing repaints between the map and the shot: drain the loop so GTK lays the page out
    // (a `DrawingArea` clips to its allocation, and the Prepare paned measures 0 px before it).
        if screen_page(&screen) == Some("Prepare") || screen_page(&screen) == Some("Produce") {
            let context = glib::MainContext::default();
            for _ in 0..64 {
                while context.pending() {
                    context.iteration(false);
                }
            }
        }
        // §4's picture is a CROP of one column, which a whole-window shot cannot show.
        if screen == "03-sources" || screen_page(&screen) == Some("Prepare") {
            crate::snapshot_sources::stage_for_shot(&window);
        }
    if screen_page(&screen) == Some("Cut") || screen_page(&screen) == Some("Narrate") {
        let context = glib::MainContext::default();
        for _ in 0..64 {
            while context.pending() {
                context.iteration(false);
            }
        }
            if screen_page(&screen) == Some("Cut") {
            // §A's preview fills the upper pane: hand the panel the pane it ACTUALLY got. Not for `05-rows`,
            // whose staging shrinks this pane on purpose (its camera rows would be pushed past ~890 px).
            if let (Some(panel), Some(split)) = (
                ui::find_source_widget(&window, "preview-panel"),
                ui::find_source_widget(&window, "cut-page-split")
                    .and_then(|w| w.downcast::<gtk::Paned>().ok()),
            ) {
                let top_h = split
                    .start_child()
                    .map(|child| child.height().max(1))
                    .unwrap_or(1);
                let lane_h = crate::fx_lane::lane_height_px(1) as i32;
                // Pass the panel's own width: `grow_to` floors it at the declared 1280 (nothing ffprobes headless).
                let panel_w = panel.width().max(1);
                // Ceiled at §A's opening: feeding the pane's current height back unceiled loops, and the pane overflows the divider.
                let tall = crate::ui::cut_page_layout::preview_ask_ceiling(top_h, lane_h);
                crate::ui::cut_preview_panel::grow_to(&panel, panel_w as f64, tall as f64);
                crate::ui::cut_preview_frame::repaint(&window);
                // No divider re-ask here: it would undo `rest_page_splits`, which runs after the final size request.
                let _ = top_h;
            }
        }

/// Room below the caption form in the Cut column: Apply/Cancel row, footer, and the bottom band.
/// Overshoot only leaves white space; undershoot hides the row the screen shows.
const FORM_UNDER_PX: i32 = 380;

            // `06-text`'s form falls below §1's fixed height; grow the window with `set_size_request` (a default
            // size is only an `adw::ApplicationWindow` floor) to what the page's column wants.
            if screen == "06-text" {
                // The `text-form-<tab>` holder is unusable here (`show_text_form` fills the FIRST holder of that name);
                // the column's own bottom plus `FORM_UNDER_PX` is the honest number.
                let column_bottom = ui::find_source_widget(&window, "cut-page-split")
                    .and_then(|w| w.downcast::<gtk::Paned>().ok())
                    .and_then(|split| split.end_child())
                    .and_then(|end| end.compute_bounds(&window).map(|r| (r.y() + r.height()) as i32))
                    .unwrap_or(0);
                let window_bottom = window
                    .compute_bounds(&window)
                    .map(|r| (r.y() + r.height()) as i32)
                    .unwrap_or(0);
                let short = (column_bottom + FORM_UNDER_PX - window_bottom).max(0);
                if short > 0 {
                    let want_h = window.default_height() + short;
                    window.set_size_request(window.default_width().max(crate::shell::WINDOW_MIN_W), want_h);
                    window.set_default_size(window.default_width(), want_h);
                    crate::snapshot_face::pump_context(16);
                }
                println!(
                    "snapshot: 06-text | column bottom {column_bottom}, window bottom {window_bottom}, grew {short} px to {} px",
                    window.default_height()
                );
            }
            if screen == "05-cut" {
                // The band's red line is the PREVIEW's playhead (`band_scene_for` reads the player), not the cursor state.
                ui::set_playhead(&window, 24.0);
                // Badge **31** (§A): shown only because `cut_view::extents` says the tape overflows. Press + while it
                // still fits; on this 0-120 take it already overflows, so the printed pps is §A's 15.
                let band_view = crate::ui::cut_view::surface_width(&window);
                let (open_total, open_view) = crate::ui::cut_view::extents(band_view);
                let mut steps = 0;
                while crate::ui::cut_view::fits(
                    crate::ui::cut_view::extents(band_view).0,
                    crate::ui::cut_view::extents(band_view).1,
                ) && steps < 6
                {
                    ui::press_zoom(&window, true);
                    steps += 1;
                }
                let (over_total, over_view) = crate::ui::cut_view::extents(band_view);
                let shown = !crate::ui::cut_view::fits(over_total, over_view);
                println!(
                    "snapshot: 05-cut badge 31 | {:.1} px/s after {steps} more + step(s) over the toolbar's own door: {over_total:.0} px of film over a {over_view:.0} px view (opened at {open_total:.0} over {open_view:.0}) -- the bar shows: {shown}",
                    crate::ui::cut_view::pps()
                );
                if !shown {
                    let why = format!(
                        "05-cut shot shows no scrollbar: after {steps} + steps the tape is still {over_total:.0} px inside a {over_view:.0} px view"
                    );
                    eprintln!("snapshot: {why}");
                    OUT.with(|e| *e.borrow_mut() = Some(why));
                }
            }
            if screen == "05-rows" {
                ui::set_playhead(&window, crate::snapshot_rows::LINE_T);

                // F2.10: refuse to write a PNG that does not cover both camera plates, rows 1 over 2 as in `spec/img/05-rows.png`.
                let tops = crate::snapshot_rows::row_tops(&window);
                let (first, second) = (tops[0], tops[1]);
                let covers = crate::snapshot_rows::shot_covers(&window, first, window.height());
                let ordered = crate::snapshot_rows::rows_in_order(&window) == Some(true);
                println!(
                    "snapshot: 05-rows | rows y={first}/{second}, frame {}x{}, covers both: {covers}, row 1 on top: {ordered}",
                    window.width(),
                    window.height()
                );
                if !(covers && ordered) {
                    let why = format!(
                        "05-rows shot does not cover both camera rows (y={first} / {second}, frame {}x{}, row 1 on top: {ordered})",
                        window.width(),
                        window.height()
                    );
                    eprintln!("snapshot: {why}");
                    OUT.with(|e| *e.borrow_mut() = Some(why));
                }
            }
            crate::ui::cut_view::repaint_page(&window);
            // Badge 1 again: the first `refresh` measured a 0x0 plate before `present()`.
            if screen == "05-cut" || screen == "05-rows" {
                crate::ui::cut_picture::refresh(&window);
            }
            // The staging ran before `present()`, so the take band's width was an unmapped widget's.
            if screen_page(&screen) == Some("Narrate") {
                crate::ui::narrate_band::note_lane_size(&window);
                crate::ui::narrate_band::repaint(&window);
            }
        }
        let painted = subject.clone().unwrap_or_else(|| window.clone().upcast());
        // A glyph is illegible in a window shot: paint the ▶ into `<out>` and the window beside it as `-window.png`.
        let face_only = screen == "03-runbar-busy";
        let menu_over_page = crate::snapshot_effect_menu::shot_sidecar_wanted();
        // Screens whose spec picture is a subject of its own (popover, lane, framed frame) take the asked
        // filename (`snapshot_subjects`' table); the page goes beside it as `<name>-page.png`, named so it sorts
        // BEFORE the asked filename: a viewer takes the last PNG written, which must be the subject.
        let page_out = if crate::snapshot_subjects::subject_takes_the_name(&screen) {
            out.with_extension("page.png")
        } else {
            out.clone()
        };
        let screen_for_frame = screen.clone();
        let screen_for_picker = screen.clone();
        let screen_for_heading = screen.clone();
        let face_out = out.with_extension("face.png");
        let out = out.clone();
        let app = app.clone();
        let face_out = face_out.clone();
        // §1: headless GTK never honours a smaller size after the map, so clamp to the minimum once the layout
        // has settled. Dialog subjects keep their own size.
        if subject.is_none() {
            let want_w = window.default_width().max(crate::shell::WINDOW_MIN_W);
            let want_h = window.default_height().max(crate::shell::WINDOW_MIN_H);
            window.set_size_request(want_w, want_h);
            window.set_default_size(want_w, want_h);
            crate::ui::settle::when(
                "the window at the shot's size",
                &window,
                move |window| window.surface().is_some_and(|s| (s.width(), s.height()) == (want_w, want_h)),
                |_| {},
            );
            crate::snapshot_face::pump_context(32);
            // Re-ask §A's openings at §1's real size: headless `notify::height` never fires for a widget allocated once (`snapshot_layout`).
            crate::snapshot_layout::rest_page_splits(&window);
            crate::snapshot_face::pump_context(16);
            crate::snapshot_face::set_shot_size(want_w, want_h);
        }
        crate::snapshot_band::rest_band(&window);
        // A page whose minimum eats the run-bar row still exits 0, so report where the bottom band landed.
        if let Some(said) = crate::snapshot_band::report(&window) {
            println!("snapshot: {said}");
        }
        let quit = app.clone();
        let screen_for_report = screen.clone();
        let shot = painted.clone();
        let staged_menu = if menu_over_page {
            match crate::snapshot_effect_menu::stage_open_over_window(&window) {
                Ok(menu) => Some(menu),
                Err(err) => {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                    None
                }
            }
        } else {
            None
        };
        let also: Vec<gtk::Widget> = staged_menu.iter().map(|menu| menu.holder()).collect();
        let on = match (&on, &staged_menu) {
            (crate::snapshot_ready::ShotOn::Ready, Some(_)) => crate::snapshot_ready::ShotOn::Open(crate::snapshot_effect_menu::SHOT_MENU_NAME.to_string()),
            _ => on.clone(),
        };
        crate::snapshot_ready::shoot_when_ready(&shot, also, on, move || {
            crate::snapshot_settings::apply_staged_badge();
            if face_only {
                if let Err(err) = write_png(&painted, &out) {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
                let play = ui::play_button(&crate::ui::window::main_window()).expect("the run bar has a ▶");
                if let Err(err) = write_face_png(&play, &face_out) {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
            } else if menu_over_page {
                // §2: the window keeps `<name>.png` and the ✚ Effect menu is painted where its button sits; a popover
                // lives on a separate surface and cannot come out of the render node.
                let painted_menu = match &staged_menu {
                    Some(menu) => crate::snapshot_effect_menu::paint_staged(&window, menu, &out),
                    None => Err("the ✚ Effect menu was not staged".to_string()),
                };
                match painted_menu {
                    Ok((rows, panel, at)) => println!(
                        "snapshot: 06-effect-menu | {rows} rows over the page, panel {}x{} at x={},y={}",
                        panel.0, panel.1, at.0, at.1
                    ),
                    Err(err) => {
                        eprintln!("snapshot: {err}");
                        OUT.with(|e| *e.borrow_mut() = Some(err));
                    }
                }
                let page_out = out.with_extension("page.png");
                if let Err(err) = write_png(&painted, &page_out) {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
            } else if crate::snapshot_subjects::subject_takes_the_name(&screen_for_picker)
                && crate::snapshot_subjects::is_sources_subject(&screen_for_picker) {
                if let Err(err) = crate::snapshot_sources::write_column_png(&window, &out) {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
                let page_out = out.with_extension("page.png");
                if let Err(err) = write_png(&painted, &page_out) {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
            } else if screen_for_picker == "04-prompt-picker" {
                // §1: the window keeps the asked filename and the picker list is painted hanging under its button; the
                // sidecar is a copy of the page for comparison with `04-prepare.png`.
                let painted_here = match crate::snapshot_prompt_picker::paint_open_over_window(&window, &out) {
                    Ok((rows, tick)) => {
                        println!("snapshot: {tick} ({rows} rows), over the page");
                        true
                    }
                    Err(err) => {
                        println!(
                            "snapshot: the open list could not be painted over the page ({err}; {}) -- painting the list alone",
                            crate::snapshot_prompt_picker::probe(
                                &ui::find_source_widget(&window, "bench-picker").expect("the picker")
                            )
                        );
                        false
                    }
                };
                if !painted_here {
                    match crate::snapshot_prompt_picker::paint(&window, &out) {
                        Ok((rows, tick)) => println!("snapshot: {tick} ({rows} rows)"),
                        Err(err) => {
                            eprintln!("snapshot: {err}");
                            OUT.with(|e| *e.borrow_mut() = Some(err));
                        }
                    }
                }
                let page_out = out.with_extension("page.png");
                if let Err(err) = write_png(&painted, &page_out) {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
            } else if screen_for_heading == "04-prepare" {
                let heading_out = out.with_extension("heading.png");
                match crate::snapshot_bench::paint_heading(&window, &heading_out) {
                    Ok(report) => println!("snapshot: 04-prepare heading staged -- {report}"),
                    Err(err) => {
                        eprintln!("snapshot: {err}");
                        OUT.with(|e| *e.borrow_mut() = Some(err));
                    }
                }
                if let Err(err) = write_png(&painted, &page_out) {
                    eprintln!("snapshot: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
            } else if let Err(err) = write_png(&painted, &page_out) {
                eprintln!("snapshot: {err}");
                OUT.with(|e| *e.borrow_mut() = Some(err));
            }
            if let Some(sidecar) = crate::snapshot_subjects::subject_sidecar(&screen_for_frame) {
                // Cropped here, not during staging: a render node needs the window mapped and drawn.
                if sidecar == "lane-shot" {
                    match crate::snapshot_lane::paint_track_area(
                        &window,
                        &out.with_extension("lane-shot.png"),
                    ) {
                        Ok(said) => println!("snapshot: {said}"),
                        Err(err) => eprintln!("snapshot: {err}"),
                    }
                }
                // Staging wrote the sidecar for every Cut-page shot; rename only where `page_out != out`.
                let staged = out.with_extension(format!("{sidecar}.png"));
                if page_out == out {
                } else if let Err(err) = std::fs::rename(&staged, &out) {
                    eprintln!("snapshot: the {sidecar} image could not take {}: {err}", out.display());
                    OUT.with(|e| *e.borrow_mut() = Some(format!("{sidecar}: {err}")));
                }
            }
        }, move |result| {
            match result {
                Ok(said) => println!("snapshot: {screen_for_report} {said}"),
                Err(err) => {
                    eprintln!("snapshot: {screen_for_report}: {err}");
                    OUT.with(|e| *e.borrow_mut() = Some(err));
                }
            }
            quit.quit();
        });
    });
    // The handler closes the last window; a nested block_on would recurse on the running context.
    app.run_with_args::<String>(&[]);

    OUT.with(|e| e.borrow_mut().take()).map_or(Ok(()), Err)
}

thread_local! {
    static OUT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub fn exit_code(err: &str) -> ExitCode {
    eprintln!("snapshot: {err}");
    ExitCode::FAILURE
}

/// A mono 16-bit PCM wav of `samples` samples at `rate`, written without ffmpeg. Each 5 ms bucket carries a
/// small tone burst so `wave_build::peak_byte` lands above zero without clipping: pure silence draws a flat line.
pub fn write_silence_wav(path: &Path, rate: u32, samples: u32) -> std::io::Result<()> {
    let data = (samples * 2) as usize;
    let mut bytes = Vec::with_capacity(44 + data);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data as u32).to_le_bytes());
    bytes.resize(44 + data, 0);
    // Speech-like envelope: syllables of 0.20-0.50 s separated by 0.15-0.40 s gaps, so the shape changes on a
    // scale coarser than one 5 ms bucket. Lengths in SAMPLES; a seeded xorshift keeps every render identical.
    let period = (rate / 200).max(1) as usize;
    let mut rng: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let rate = rate as usize;
    let mut at = 0usize;
    while at < samples as usize {
        let loud_end = at + (2 * rate / 10 + (next() % (rate as u64 / 2)) as usize);
        let amp = (i16::MAX as f64) * (0.15 + (next() % 5_600) as f64 / 10_000.0);
        while at < loud_end && at < samples as usize {
            let phase = (at % period) as f64 / period as f64 * std::f64::consts::TAU;
            let offset = 44 + at * 2;
            bytes[offset..offset + 2].copy_from_slice(&((amp * phase.sin()) as i16).to_le_bytes());
            at += 1;
        }
        at = (at + rate * 15 / 100 + (next() % (rate as u64 * 25 / 100)) as usize)
            .min(samples as usize);
    }
    std::fs::write(path, bytes)
}

/// Seconds the shot's Narrate files hold: enough for the band to show takes at a readable px/s.
const SEED_SECONDS: u32 = 45;
const SAMPLE_RATE: u32 = crate::transcribe::SAMPLE_RATE as u32;

pub fn seed_voice_reference(tree: &crate::layout::Tree) -> bool {
    let base = tree.voice_ref_base_wav();
    let Some(folder) = base.parent() else {
        return false;
    };
    if std::fs::create_dir_all(folder).is_err() {
        return false;
    }
    if !seed_narrator_recording(tree) {
        return false;
    }
    if crate::voice_ref::wav_header(&base).is_none() {
        // At the DECODE rate (`transcribe::SAMPLE_RATE`) that `wave_build`'s fold assumes; a different length from
        // the reference stops the wave part-way across the ruler.
        if write_silence_wav(&base, SAMPLE_RATE, SEED_SECONDS * SAMPLE_RATE).is_err() {
            return false;
        }
    }
    // `narrator1` takes the page off the captions-only face (`narrate_screen::CAPTIONS`).
    let _ = crate::narrate_data::write_voice(tree, "narrator1");
    // §F4.6 S4 removes these on every take, and the shot's arm takes one after this runs: put them back.
    if crate::voice_ref::wav_header(&base).is_none() {
        let _ = write_silence_wav(&base, SAMPLE_RATE, SEED_SECONDS * SAMPLE_RATE);
    }
    crate::snapshot_narrate::stage_seeded_envelopes(tree);
    crate::ui::narrate_band::recording_seconds().is_some()
}

/// Stand a recording where the project's narrator-1 row points, pointing the row at it if that file is not there.
fn seed_narrator_recording(tree: &crate::layout::Tree) -> bool {
    let Ok(mut project) = crate::project::load(tree.dir()) else {
        return false;
    };
    let Some(row) = project
        .sources
        .iter_mut()
        .find(|source| source.narrator == 1)
    else {
        return false;
    };
    let stored = row.path.clone();
    let target = crate::layout::resolve(tree.dir(), tree.dir(), &stored);
    // The working copy's `lecture.mkv` is a stand-in ffprobe rejects; ask the probe, not `is_file`.
    if target.is_file() && recording_seconds_of(&target).is_some() {
        return true;
    }
    let parent = match target.parent() {
        Some(parent) => parent,
        None => return false,
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    let renamed = target.with_extension("wav");
    if write_silence_wav(&renamed, SAMPLE_RATE, SEED_SECONDS * SAMPLE_RATE).is_err() {
        return false;
    }
    if renamed != target {
        // The row has to SAY wav, or ffprobe and the wave cache ask a container that is not there.
        let _ = std::fs::remove_file(&target);
    }
    row.path = crate::layout::write_path(tree.dir(), tree.dir(), &renamed);
    crate::project::save(&project, tree.dir()).is_ok()
}

/// One audio file's seconds, via [`crate::narrate_band_face`]'s reading so shot and page agree.
fn recording_seconds_of(path: &Path) -> Option<f64> {
    if let Some(seconds) = crate::voice_ref::wav_seconds(path) {
        return Some(seconds.max(0.1));
    }
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    crate::cut_lanes::probe_duration(&configured, &path.display().to_string(), &mut |_| {})
        .ok()
        .filter(|seconds| *seconds > 0.0)
}
