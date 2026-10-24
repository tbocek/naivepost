//! `--snapshot <screen> --project <fixture dir> --out <file.png>`.
//!
//! Renders one screen of the window without a host display: the caller runs it
//! under `xvfb-run` with `GSK_RENDERER=cairo` (see the justfile). The screens are
//! named after the spec's image files (`03-window`, `05-cut`, …) so every spec
//! image has a reproducible counterpart.

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

/// Screen names the window can render: the spec's images, mapped onto a page.
fn screen_page(screen: &str) -> Option<&'static str> {
    match screen {
        "03-window" | "03-settings" | "03-sources" | "03-new-confirm" => {
            Some("Prepare")
        }
        // §7's link line as the shell actually draws it, in the OPEN log. Program-side screen: there is
        // no `spec/img/03-log-link.png`; §1 only says which single line of the log is a link.
        "03-log-link" => Some("Prepare"),
        // §F1.1 S7/S8's face: the shell AFTER a Prepare run ended — bar at 1, "prepared — N files" on
        // the status row, the three folders named in an open log, Outputs counting what was written.
        // No `spec/img/` picture of it; §S7 spells the words, this shows them in place.
        "04-prepared" => Some("Prepare"),
        // §2's own picture is the RUN BAR at rest: ▶ with "Run this step — or resume what is paused",
        // ⏸ absent, and the gears still. It renders on Prepare because that is the page whose step ▶
        // stands for when nothing is playing (§F0.2 S4 → F1.1).
        "03-runbar" | "03-runbar-busy" => Some("Prepare"),
        "04-prepare" | "04-prompt-picker" => Some("Prepare"),
        "05-cut" | "05-add" | "05-fold" | "05-rows" | "05-split" | "05-trim" | "05-remove"
        // §F2.9's own screen: the page AFTER ⧉ Copy, ⧉ Paste (both branches and its refusal) and ⇲ Lane.
        // `spec/img` has no copy image, so this is a program-side screen like `03-log-link` / `04-prepared`.
        | "05-lanes" | "05-copy"
        // §F2.12's own screen likewise: three real inserts (BETWEEN, OVER, LANE) and S1's refusal. No
        // `spec/img/05-insert.png` exists, so `spec/inventory/cut.md` §G is the widget reference for it.
        | "05-insert"
        // §F2.14's own screen: ▶ in the MODEL mode — §S1's refusal beside a landed suggestion whose every
        // edge came from the app's snap points. No `spec/img/05-suggest.png` exists either.
        | "05-suggest"
        // §F2.14 S2a's own screen: ▶ in the WORDS mode, no model asked, so this is the page after the words
        // cut AND after §12 §4's walk-back ran on it — the hole between two trimmed runs closed because
        // somebody talked in it. No `spec/img/05-words.png`; `spec/inventory/cut.md`'s words line
        // ("cut by the words: N segments") is the widget reference for what the status row must read.
        | "05-words" => Some("Cut"),
        "06-preview" | "06-zoom" | "06-speed" | "06-text" | "06-volume" | "06-label"
        // §F3.10's pass screen: no `spec/img` counterpart, so it names itself after the flow.
        | "06-speeds"
        // §F3.11's, the same: driven through the page's own decorations-pass door.
        | "06-decorations"
        | "06-lane" | "06-svg" | "06-effect-menu"
        // §06-effects#3's own screen: the Cut page after all three post-cut passes have placed what they were
        // answered with (F3.9 + F3.10 + F3.11 on one lane). No `spec/img` counterpart.
        | "06-flows" => Some("Cut"),
        // §06-effects#5's own screen: the INSERT FORM with a card's own seconds already in it. No `spec/img`
        // counterpart, so `spec/inventory/cut.md` §G is again the widget reference (heading, declared field,
        // three radios, Seconds, Cancel + Insert it).
        | "05-insert-form" => Some("Cut"),
        "07-narrate" | "07-take" => Some("Narrate"),
        "08-produce" | "08-words" => Some("Produce"),
        _ => None,
    }
}

pub fn run(screen: &str, dir: &Path, out: &Path, on: crate::snapshot_ready::ShotOn) -> Result<(), String> {
    let page = screen_page(screen).ok_or_else(|| format!("unknown screen {screen}"))?;
    if !dir.join(project::PROJECT_FILE).is_file() {
        return Err(format!("no {} in {}", project::PROJECT_FILE, dir.display()));
    }
    let model = project::load(dir)?;

    gtk::init().map_err(|err| err.to_string())?;
    // No animations in a shot: an overlay scrollbar still easing in was the only thing moving after ready
    // on two screens, and a picture taken mid-animation depends on when it was taken.
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    // Non-registering: a snapshot must not claim the app id or wait on a session bus.
    let app = gtk::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let out = out.to_path_buf();
    // Copied for the closures below: `screen` decides which widget gets painted and a borrow of
    // `run`'s argument cannot outlive this function's body.
    let screen = screen.to_string();
    // The dialog names the open project, which is the folder the snapshot was asked to render.
    let dir = dir.to_path_buf();
    // Built when the app has no windows yet: they are added at startup, so the
    // handler runs after GTK has emitted it.
    app.connect_activate(move |app| {
        // The bench opens on row 0, the User Context (§1 **5**), so every shot of this page would show
        // the same column: one short project note under a "User Context" heading and nothing else. That is
        // true here and in the reference picture, so it is not a defect to fix -- but it does mean the two
        // shots have to be compared on the row's own contents, not on a bench state. Measured with no
        // request for another row, `04-prepare` and `03-window` differ by 552 bytes and nothing else.
        // A screen that DOES want another row has to say so BEFORE the window is built: `bench_box` reads
        // this once and paints from it, and a later call would arrive after the heading was already drawn.
        // Both of these ask for row 2 (Describe): `spec/img/04-prepare.png` IS the Prepare page with the
        // Describe prompt open (its badge 2 labels the prompt text), and `spec/img/04-prompt-picker.png` is
        // that same row with its dropdown spread — the popup is its own surface, so the two shots come out
        // the same picture here (see the `04-prompt-picker` seed arm for why). `03-window` and `03-sources`
        // stay on the default row, which is the User Context (§1 **5**).
        // `04-prepare` asks for row 2 (Describe): `spec/img/04-prepare.png` IS the Prepare page with the
        // Describe prompt open (its badge 2 labels the prompt text). `04-prompt-picker` asks for NOTHING:
        // its reference is the User Context row with the dropdown spread, and a request left over from an
        // earlier shot would have this one open on another row (`BENCH_OPEN_ROW` reads once per window).
        if screen.as_str() == "04-prepare" {
            ui::set_bench_open_row(2);
        }
        // …and only `04-prepare` earns the mark on it: §1's badge 2 is that row WITH its "edited" mark and
        // a live Reset, which only exist while this machine holds an override. Before the window, because
        // the bench reads the settings once as it paints. NOT for `04-prompt-picker`: its reference
        // (`spec/img/04-prompt-picker.png`) is User Context with no mark and no Reset at all, so staging an
        // override there would draw two widgets the picture does not have.
        if screen.as_str() == "04-prepare" {
            crate::snapshot_bench::stage_edited_row(2);
        }
        // F2.10: a snapshot renders a PROJECT, so the folder it was asked for is taken on exactly as Open
        // takes one on — otherwise nothing can resolve a stored source path and the session's recordings
        // (stamped names + probed lengths) could never be laid out from the app's own rules. Taken on BEFORE
        // the window exists, because `build_window` seeds this window's session spans as it builds.
        ui::save_apply::note_open_for_snapshot(&dir);
        // `build_window` below resets that door to the working copy beside the launch, so a page that
        // resolves stored `project:` paths at PAINT time (the Produce plates) has to be told the folder
        // this shot renders directly — otherwise its pictures resolve beside the launch and draw missing.
        crate::ui::produce_paint::note_open_project(&dir);
        if screen_page(&screen) == Some("Narrate") {
            // BEFORE the window is shown: §A.7's visibility rule runs during `refresh`, and a scroller
            // hidden then stays hidden however many refreshes follow — so the reference has to be on
            // disk before the page first lays out, not after.
            if let Some(tree) = crate::ui::window::narrate_session_tree() {
                let band = seed_voice_reference(&tree);
                println!("snapshot: take band seeded = {band}");
            }
        }
        // §4's rows must be in the session BEFORE the page is built: `sources_list` draws one row per source
        // at build time, and a later repaint of a scroller whose band fits two rows still shows two rows. So
        // this screen builds over the staged lecture day; every other screen builds from `--project`.
        let staged_model =
            if matches!(screen.as_str(), "03-sources" | "04-prepare" | "04-prompt-picker") {
                // §1's badge **1** is "sources list and frame controls", so the two Prepare pictures have to show
                // ROWS and not an empty scroller: the same staged day `03-sources` renders.
                crate::snapshot_sources::staged()
            } else {
                model.clone()
            };
        let window = ui::build_window(app, &staged_model, page);
        // The strip paints from this window's cut slot, so a snapshot must show the cut that lives in
        // the project it was asked to render — not whatever the working-copy folder happens to hold.
        // `--project` is that project, so its `cut/cut.json` is seeded over `build_window`'s
        // session-dir default here, before the frame is drawn. A missing or unreadable file is no cut
        // today (`cut::load` answers that with `Cut::default()`), which draws the empty hint.
        if let Some(loaded) = layout::Tree::new(&dir).ok().and_then(|tree| cut::load(&tree).ok()) {
            ui::seed_review_cut(&window, &loaded);
            // The camera rows are built from the cut too, and `build_window` drew them before this seed
            // landed -- so a snapshot of any Cut-page screen has to refresh them or they show the empty
            // pre-seed list. (F2.10: `camera-rows`, its name plates, lens badge and per-lane badges.)
            ui::refresh_camera_rows(&window);
        }
        // F2.10's own picture is TWO cameras at once (`spec/img/05-rows.png`), which no fixture on disk
        // carries -- staged in its own module below, with the other self-staging Cut-page screens.
        // F2.11's own picture (`spec/img/05-fold.png`) is a WIDE dropped stretch folded to a seam. The
        // demo fixture cannot show one: its `cut/cut.json` keeps 0-9.5, 10-30 and 40-70 out of a tape
        // whose only filmed gap is 0.5 s = 2 px at the snapshot zoom, under `cut_fold::FOLD_MIN_PX`, so
        // no badge would be drawn at all. Laid out here instead -- the shared fixture file untouched:
        // the session filmed 0-120 as one take (that is what gives head/hole/tail anywhere to exist, see
        // `ui::page_recordings`), two clips are kept, the 30-40 hole is FOLDED so the bars meet at a
        // seam with a `+` on it, and the 60-120 tail is left open wearing a `-`.
        // §F2.5's own picture (`spec/img/05-lanes.png`) is the SAME page staged the way its
        // caption says the prototype was: "the lecture has no separate recordings, so 45 s of
        // another day's recording was added to a clip" — i.e. one lane overlapping the footage,
        // and one scene that has it silenced. The `05-cut` staging above already lays a lane
        // down; this arm restages it for the lanes shot so the plate, the camera row's own sound
        // strip (badge **1**) and the recorders' band (badge **2**) are all on screen together,
        // and drives the silence through the page's own door rather than faking a painted band.
        if screen == "05-lanes" {
            // 45 s of a separate recording, placed to overlap two scenes, the way the caption
            // stages it: the second scene silences it, the first does not.
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
            // The lane's file has to exist for its plate to carry a channel layout: `cut_lanes`
            // asks ffprobe for the first audio stream, and an absent file answers `None`, which
            // would leave the plate a bare name. A silent mono wav is the smallest file that
            // really is mono (same helper the Narrate shot uses for its voice reference). It
            // goes in the folder the resolution actually points at — `layout::resolve` with the
            // app root and the session folder as the project, which is what `channels_of` uses —
            // so the shot stages the file where the plate will look for it, not beside `--project`.
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
            ui::seed_review_cut(&window, &lanes_shot);
            ui::refresh_camera_rows(&window);
            // The plate text, printed the way the other staged screens print what they staged:
            // the string is the app's own (`cut_lanes::plates` over the seeded lanes), so the
            // shot's stdout says what badge **2** reads rather than leaving it to be squinted at.
            let plates = crate::cut_lanes::plates(&lanes_shot.lanes);
            println!("snapshot: 05-lanes plates | {}", plates.join(" | "));
            // The line sits inside the scene that hears nothing, so the shot shows the silenced
            // state the caption names, with the lane still drawn below the camera row.
            ui::set_line_position(&window, cut_line::LinePos { t: 40.0 });
            // Badge **1** shows a frame of the session (the demo fixture ships no extracted
            // frames), staged the same way the `05-cut` shot stages it, so the shot has a
            // picture above the recorders' band as the spec's picture does.
            seed_snapshot_frame(&dir, "session-tape", 0.0);
            ui::cut_picture::set_preview_camera_base("session-tape");
            ui::cut_picture::set_snapshot_dir(&dir);
        }
        // §F2.5's own picture (`spec/img/05-lanes.png`) is the recorders' band drawn DOWNSTAIRS,
        // in the strip under the ruler — not in the preview panel above. So the lane shot has to
        // reach the same door `repaint_page` uses after the layout settles: `fit_band_height_for_window`
        // grows `track-layers` (and the red line's layer) to the height the scene needs, which is
        // what makes the lane row and its plate land inside the painted allocation. Called here so
        // the shot's own drain repaints at that height; `install` sized it once with no cut yet.
        if screen == "05-lanes" {
            crate::ui::cut_tracks::fit_band_height_for_window(
                &window,
                &crate::ui::cut_view::band_scene_for(&window),
            );
            // Why the lane row did not reach an earlier version of this shot: the recorders'
            // band is painted DOWNSTAIRS, in the strip under the ruler, and at the snapshot's
            // zoom floor the whole session fits the visible tape, so the horizontal scrollbar
            // is correctly hidden (§1: the track scrollbar, badge **31**, appears only when the
            // tape is wider than the view). That is the same settled state the `05-cut` shot
            // already shows; the plate itself is printed on stdout above, where a check can read
            // it, and pinned in tests by `f2_5_s2_the_lane_plate_on_the_live_band_carries_the_layout`.
        }
        if screen == "05-cut" || screen == "05-fold" {
            // §1's badges 5-9 are greyed until a preview plays; the shot's own doors put them live
            // (`snapshot_layout::stage_cut_transport`).
            if screen == "05-cut" {
                crate::snapshot_layout::stage_cut_transport(&window);
            }
            let folded_segs = vec![
                cut::Seg { s: 10.0, e: 30.0, cam: 0, ..Default::default() },
                cut::Seg { s: 40.0, e: 60.0, cam: 0, ..Default::default() },
            ];
            // F2.8 S2: the strip's two new rows only exist when there is something to plate for each — the
            // recorders' band needs a lane and the wave strip needs the session tape (see
            // `cut_trim_press::painted_bands`). With no lane the whole layout collapses back to one painted
            // row, so a Cut-page shot could never show the rows this round added; one separate recording puts
            // both rows on screen, which is what `spec/img/05-trim.png`'s strip has.
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
            ui::seed_review_cut(&window, &folded);
            // The badge table only exists once the badges have been DRAWN (`refresh_fold_badges` is what
            // records which gap each numbered button stands for), so a fold requested before that first
            // draw has no index to fold — press the fold BEFORE refreshing, then refresh once.
            ui::refresh_fold_badges(&window);
            // One fold already applied, so the shot shows BOTH states at once: `+` waiting on the seam
            // where 0:30-0:40 was folded away, `-` over the still-open tail.
            ui::press_fold_gap(&window, 1);
            ui::refresh_camera_rows(&window);
            // Badge **1** shows a frame of the session, and Prepare's extracted frames are not part of the
            // demo fixture (it ships `cut/` only), so the shot writes one real PNG into the folder the page
            // lists frames from. Real bytes through gdk-pixbuf, named by `clock::frame_name` so the same
            // rule the band uses to pick a frame picks this one.
            seed_snapshot_frame(&dir, "session-tape", 0.0);
            // The frame reaches the panel through the page's own door: `refresh` asks which recording the
            // preview watches and where the line stands, so the shot first tells it WHICH session this is
            // (the same slot `page_recordings` answers from) before repainting.
            ui::cut_picture::set_preview_camera_base("session-tape");
            ui::cut_picture::set_snapshot_dir(&dir);
            // `refresh` is called AGAIN after the layout below (the Cut-page drain), because before
            // `present()` the panel answers 0x0 and a frame picked for an unmeasured plate is the wrong
            // ask; the second call is what actually paints badge 1.
            let _painted = crate::ui::cut_picture::refresh(&window);
            // Deepen the zoom through the toolbar's own + door (the same one a click reaches) so the tape
            // outgrows the view and badge **31** appears because the geometry says it must, not because
            // anything was forced visible: §A's rule is "hidden when everything fits", and at the opening
            // 4 px a second a short session does fit, which is why no Cut-page shot had ever shown the bar.
            // Six steps from 4 px/s land on 15 px/s (§A's factor 1.25), which is where the shot is measured
            // from: the badge-31 print in the Cut-page drain reports the pps it ended at and fails the render
            // if the bar is still not asked for there.
            for _ in 0..6 {
                ui::press_zoom(&window, true);
            }
        }
        // §2's own picture (`spec/img/03-runbar.svg`) is the run bar WITH WORK GOING ON: "the gears turn
        // while a run computes", and ▶ has become ⏸ "Pause". No snapshot could show that face until now
        // — `run()` drove nothing through `wire_play`, so every shot on disk was the idle one (proved by
        // `03-runbar` rendering byte-identical to `03-window`). Driven through the program's own doors,
        // the way `06-volume` opens its form, never by poking state:
        //   1. seed a cut so the Cut page has real kept material;
        //   2. press the page's own ▶ (F2.1). Its handler passes the filmed stretches it knows, which
        //      for a headless snapshot is none, so `preview::press_recording` starts from the red line --
        //      hence the line placed below first; with neither it answers S3's `NoFootage` and nothing
        //      plays (that refusal is what an earlier attempt of this shot printed on the status row);
        //   3. the preview now plays, so the bar's ▶ belongs to it.
        // The shot therefore ends on the BUSY face: the bar's ▶ drawn as ⏸ with tooltip "Pause", because
        // `run::controls` calls a page transport that is playing busy (§2). A paused-but-started preview
        // would read as ▶ again (`Transport::cued`) -- that is F0.2 S2's other face, pinned by
        // tests/suite/press_play_widgets.rs instead of by a picture.
        if screen == "03-runbar-busy" {
            // Seeded BEFORE the tab switch: the Cut page draws its strip from this window's cut slot,
            // and `run()` seeds it after `build_window` (see the note there).
            ui::seed_review_cut(
                &window,
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
            // The red line at 15 s: inside the first kept clip, so the preview has a place to start.
            // BOTH doors, because they are two different states in the app: `set_line_position` places the
            // line the page draws, and `set_playhead` stands the player's own clock there, which is what
            // `preview::press_recording` plays from (an earlier attempt placed the line only and the press
            // answered S3's `NoFootage` -- "nothing was filmed", printed on the status row of that shot).
            ui::set_line_position(&window, crate::cut_line::LinePos { t: 15.0 });
            ui::set_playhead(&window, 15.0);
            // The page's own ▶, through the click the widget fires -- the same body `wire_play_recording`
            // calls, so the bar's ⏸ answers a preview the app really began and not a state poke. The
            // assert goes on the player, which is the state both buttons read (`run::transport_for` gives
            // the Cut page its transport, and `run::controls` paints ⏸ from `playing`).
            ui::play_recording_button(&window).expect("the Cut page carries play-recording-button")
                .emit_by_name::<()>("clicked", &[]);
            // §F0.2 S2: the page's preview is going, so the bar's ▶ belongs to it. This press is the
            // program's own door and the one thing that repaints the bar (F0.3's ⏹ handler reads the player
            // the same way; nothing else calls `paint_run_bar` outside a bar-button press). It PAUSES the
            // preview and stops -- no snapshot, no step.
            ui::play_button(&window).expect("the run bar has a ▶").emit_by_name::<()>("clicked", &[]);
            let parked = ui::preview_player(&window).transport;
            let status = ui::state(&window).status;
            assert!(
                !parked.playing && parked.started,
                "S2 should have parked the preview: transport {parked:?}, status {status:?}"
            );
            // The face the parked state earns (`Transport::cued`: started, not playing, so ▶ still owes a
            // resume -- §2's tooltip says exactly that: "Run this step — or resume what is paused"), read
            // off the widget the shot is judged by rather than only off `run::controls`, so a bar that
            // forgot to repaint fails here instead of shipping the wrong picture.
            let parked_face = face_of(&window);
            assert_eq!(
                parked_face.0,
                crate::run::PLAY_ICON,
                "a parked preview leaves ▶ as the step's button (§2)"
            );
            assert_eq!(parked_face.1, crate::run::PLAY_TOOLTIP, "and §2's own word for it");
            assert!(parked_face.2, "a parked preview still has something to stop");
            // Now the shot's own subject: resume, and the bar is the BUSY face -- ⏸ "Pause", §2's "While a
            // run or page transport is busy it shows ⏸". The picture below is taken in this state.
            ui::play_button(&window).expect("the run bar has a ▶").emit_by_name::<()>("clicked", &[]);
            assert!(ui::preview_player(&window).transport.playing, "the second press resumes");
            let busy = face_of(&window);
            assert_eq!(busy.0, crate::run::PAUSE_ICON, "a playing preview turns ▶ into ⏸ (§2)");
            assert_eq!(busy.1, crate::run::PAUSE_TOOLTIP, "and the tooltip is §2's own word for it");
            assert!(busy.2, "and there is still something to stop");
            // Named as well as painted: a 16 px glyph inside a 1334 px window shot is not legible, so the
            // face this screen exists to record goes to `shots/03-runbar-busy.face.png` (the ▶ alone) and
            // is printed here for whoever runs the recipe.
            println!("snapshot: 03-runbar-busy face = {} {:?}, stop sensitive = {}", busy.0, busy.1, busy.2);
        }
        // §F2.12's own screen: three inserts staged through the chooser seam, the radios and Apply, then S1's
        // refusal last. All of it lives in `snapshot_insert` -- `run` is past the size another block fits in.
        if screen == "05-insert" || screen == "05-insert-form" {
            let said = crate::snapshot_insert::stage(&window, &dir);
            let words: Vec<String> = said.iter().map(|line| line.replace('|', "/")).collect();
            println!("snapshot: 05-insert | {}", words.join(" | "));
            println!("snapshot: 05-insert band | {}", crate::snapshot_insert::describe(&window));
            ui::cut_view::repaint_page(&window);
            ui::refresh_camera_rows(&window);
        }
        // §F2.9's screen is the Cut page AFTER the whole flow ran: ⧉ Copy took a stretch in hand, ⇲ Lane gave
        // it a row of its own, ⧉ Paste spliced the footage copy back as a `copy:` card, and a sound copy was
        // laid over the kept footage in one piece per stretch — plus S2's refusal, with the line in a hole.
        // Driven through the same doors the buttons fire (`tests/suite/cut_copy_paste_lane_widgets.rs` presses these
        // exact ones), never by poking state: what is on screen is what the verbs wrote.
        if screen == "05-copy" {
            // The session filmed 10-130 as one take that does NOT start at 0:00 — the case the paste has to get
            // right: a copied span asks the tape for the material under it, not for its head.
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
            // The separate recording a sound copy is taken FROM, named the way a lane or a wave plate names
            // one (its base). A sound paste refuses a recording the session's file list does not hold, so the
            // shot adds it to the session too — straight into the window's own project state, which is what
            // Inputs writes. (`session_handle` is the page's public handle on it.)
            ui::session_handle(&window).borrow_mut().sources.push(crate::project::Source {
                path: "mic-lav.wav".to_string(),
                footage: true,
                ..Default::default()
            });
            ui::seed_review_cut(&window, &kept);
            // Deepen the zoom first: at the opening 4 px/s a 12 s copy is 48 px and its card reads as a
            // smudge, while 15 px/s (where §A's ladder stops) gives every box here real width.
            for _ in 0..6 {
                ui::press_zoom(&window, true);
            }
            // S3's row from a hand (a lane survives the paste, so it goes first), after a footage copy:
            // a row of its own, windowing the file from the FILE second it was taken at — the conversion this
            // round fixed, and the one thing of this flow that is plainly visible on the strip (a spliced card
            // has zero width at every zoom: the cut is *opened* there, F2.9).
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
            // S2's footage branch: the same seconds in hand again, ⧉ Paste with the line 5 s into the second
            // clip — the `copy:` card, and a cut that got longer ("the cut is a, was b").
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
            // S2's sound branch: a 12 s stretch of the microphone copied at 0:36 and pasted at 0:24, so it
            // lies over TWO kept stretches (the 0:40-0:50 hole splits it) and the sentence says "stretches".
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
            // S2's refusal, honestly aimed: a hand that starts INSIDE the hole (copied at 0:42) and the line at
            // 0:41 — inside the 0:40-0:50 gap, where the cut keeps no footage at all (the pieces laid two steps
            // back stop at the hole's edge), so the paste refuses ("a sound needs a picture under it") and the
            // copy STAYS in hand. That is the last word on the status row, which is what a bad aim really shows.
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
            // The strip repaints on the view's own door; a snapshot that never queued one would paint the
            // pre-press picture.
            ui::cut_view::repaint_page(&window);
            // The sentences the flow earned are this screen's text: printing them makes a wrong or empty status
            // row show up here instead of as an unexplained picture.
            println!("snapshot: 05-copy | {copied} | {said} | {recopied} | {pasted}");
            println!("snapshot: 05-copy sound | {sound_copied} | {laid} | {held_again} | {refused}");
            ui::refresh_camera_rows(&window);
        }
        // §F2.14's own picture: what the Cut page is left with after a Suggest press — §S1's first
        // refusal sentence on the status row beside the landed suggestion whose every edge came from the
        // app's snap points. Both come from the doors ▶ fires (`refuse_suggest_run`, `press_landed`),
        // never by poking state; the other three refusals live in `tests/suite/f2_14_guard_order.rs`.
        if screen == "05-suggest" {
            let staged = crate::snapshot_suggest::stage();
            // The tape the landed scenes sit on. Without a declared session the strip has no seconds to lay
            // out and the shot would show four scenes in a window with no timeline under them.
            ui::set_session_recordings(&[crate::timeline::Recording {
                base: "session-tape".to_string(),
                start: 0.0,
                end: 62.0,
            }]);
            ui::set_session_policy_field(&window, "cutMode", "model", "this shot", project::Origin::User);
            // S1's refusal, honestly aimed: busy now. It goes on the status row through the door the press
            // uses, so the picture shows the sentence a person would read.
            let refused = ui::cut_suggest_land::refuse_suggest_run(&window, true)
                .unwrap_or_else(|| crate::suggest::BUSY.to_string());
            ui::window::set_status_line(&window, &refused);
            // And the press's own refusal, through the same door ▶ fires it: `busy: true` is the state a
            // And the press's own first refusal, through the door ▶ fires it. This shot's session folder is a
            // fresh scratch one with no transcript, so §S1's word gate answers before any ask is built and no
            // socket is dialled -- which is the honest thing a snapshot can show. The web-offer half of this
            // flow (no browser means no `web_search`/`web_read`, §2's reason line said once) is pinned where the
            // bytes are visible, in `tests/suite/sec_05_cut_8_suggest_wired.rs`.
            let pressed = crate::snapshot_suggest::press_through_page(&window, false);
            println!("snapshot: 05-suggest press said: {pressed}");
            // Then the walk-back's landing: literal seconds in, app-chosen edges out, no socket dialled.
            let landed = crate::snapshot_suggest::press_landed(&window);
            let segs = ui::review_cut_segs_count(&window);
            let seconds = crate::cut_screen::cut_seconds(&cut::Cut {
                segs: ui::review_cut_segs(&window),
                ..Default::default()
            });
            println!("snapshot: 05-suggest | {refused} | {landed} | {segs} segs | {} s", seconds.round());
            // The folder is the shot's own scratch, and its `cut.json` is what `land_spans` wrote: read it
            // back so a screenshot proves the landing reached a file, not only the widget.
            let written = staged
                .as_ref()
                .map(|tree| tree.cut_json().is_file())
                .unwrap_or(false);
            println!("snapshot: 05-suggest cut.json written: {written}");
            // The same three refreshes a mutating verb does after it publishes (see
            // `cut_verb_press`'s tail): the fold badges, the camera rows and the effects lane. Without them
            // the shot shows the page as it was before the landing — which is why this screen once drew
            // `Segments: 0` beside four landed scenes.
            ui::refresh_fold_badges(&window);
            ui::refresh_camera_rows(&window);
            ui::refresh_effects_lane(&window);
            // And the four readings that answer from the timeline: without this the shot below still said
            // `Segments: 0` / `Cut: 00:00` beside four landed scenes, because the form column is built once
            // for an empty cut and only a verb repaints it.
            crate::ui::window::refresh_cut_readouts(&window);
        }
        // §F2.14 S2a's own picture: the Cut page after ▶ in the WORDS mode. Nothing here is poked — the
        // session folder is staged through the program's own writers (`snapshot_words`), the cut is seeded
        // through `seed_review_cut`, and the press goes through `cut_suggest_words_run::press_words`, the door
        // ▶ fires. What the shot therefore shows is §S2a's sentence on the status row beside the scenes the
        // WALK landed, i.e. with the spoken hole between two trimmed runs closed (§12 §4's fourth bullet).
        if screen == "05-words" {
            let staged = crate::snapshot_words::stage();
            let project = staged.as_ref().and_then(crate::snapshot_words::stage_session);
            // The words arm reads its footage rows off the project the WINDOW holds (`session_handle`), so the
            // staged project replaces the `--project` fixture's after the window was built: `session_handle`
            // hands back this window's own handle, and overwriting it is how a snapshot hands the page another
            // session without a click on Open.
            // The tape the landed scenes sit on. Declared through the page's OWN rescan door (`refresh_page_spans`)
            // after this window's project has been swapped for the staged one: that door asks the OPEN project for
            // its footage rows and probes each, which is how a real Open puts a tape on the strip — a hand-listed
            // span would name seconds the painter has no probed recording for.
            if let Some(project) = project {
                // The words arm reads its footage rows off the project the WINDOW holds (`session_handle`), so
                // the staged project replaces the `--project` fixture's before the rescan below is asked.
                ui::session_handle(&window).replace(project);
                ui::cut_session::refresh_page_spans(&crate::snapshot_words::base_cut());
                ui::set_session_policy_field(&window, "cutMode", "words", "this shot", project::Origin::User);
                // Point the door at the staged folder, the same seam `land_spans` writes through. The page's cut
                // must be re-seeded HERE rather than left to `run`'s early seed, which reads the `--project`
                // fixture's own `cut.json` (three clips of another session) — the words arm cuts from the cut on
                // screen, so the two-scene base from the staged folder has to be what the page holds.
                ui::cut_suggest_land::point_session_at(staged.clone());
                // Publish the two-scene base through the PAGE'S OWN writer, not a state poke: `write_cut_at` is
                // §7's one writer for `cut.json` (and refuses mid-gesture, like every real edit), so a snapshot
                // that wants the page to open on this cut writes it the way an edit would and re-seeds the page
                // from what is now on disk. The early seed above cannot do it: it reads the `--project` fixture's
                // own `cut.json`, which is a different session's three clips.
                if let Some(tree) = staged.as_ref() {
                    crate::cut_save::write_cut_at(&crate::snapshot_words::base_cut(), tree)
                        .expect("snapshot: the staged cut.json is writable");
                }
                ui::seed_review_cut(&window, &crate::snapshot_words::base_cut());
                // The row list and the fold badges come from the cut the page now holds, so both are rebuilt
                // after the re-seed and before the press: a snapshot that skips this paints the pre-seed rows.
                ui::refresh_camera_rows(&window);
                ui::refresh_fold_badges(&window);
                let said = crate::snapshot_words::press_through_page(&window);
                println!(
                    "snapshot: 05-words | {said} | {} segs",
                    ui::review_cut_segs_count(&window)
                );
                // Proof the landing reached a file, not only the widget.
                let written = staged
                    .as_ref()
                    .map(|tree| tree.cut_json().is_file())
                    .unwrap_or(false);
                println!("snapshot: 05-words cut.json written: {written}");
                // The same tail a mutating verb does after it publishes (see `05-suggest`'s note): without
                // these the readouts say `Segments: 0` beside the landed scenes.
                ui::refresh_fold_badges(&window);
                ui::refresh_camera_rows(&window);
                ui::refresh_effects_lane(&window);
                crate::ui::window::refresh_cut_readouts(&window);
                // The strip's zoom is the floor the DECLARED tape gives (§A: "− stops where all of it is on
                // screen"), and the band has to be grown and repainted at the width the layout settled on — the
                // same two doors every Cut-page screen goes through before its pixels are taken (`repaint_page`
                // fits the band to the scene; a fresh window sized the band for no cut at all).
                let view = ui::cut_view::surface_width(&window);
                ui::cut_view::set_pps(ui::cut_view::floor_pps(view));
                ui::cut_view::repaint_page(&window);
            }
        }
        // The F3/F4 screens that stage their own state live in their own modules -- `run` is past the size
        // another staging block fits in (see `snapshot_insert` for the same split). They ran here, in the
        // order they ran inside `run`: after every Cut-page arm had laid its strip out, before the subject is
        // chosen and the window is sized and mapped.
        crate::snapshot_effects::paint(&window, &dir, &out, &screen);
        crate::snapshot_preview::paint(&window, &dir, &out, &screen);
        crate::snapshot_narrate::stage(&window, &dir, &screen);
        crate::snapshot_produce::paint(&window, &screen);
        // F2.7's three pictures stage their own subject too, through the doors the ＋ / | / － buttons fire
        // (see `snapshot_verbs`). Placed with the other self-staging screens rather than inline above:
        // `run` is past the size another staging block fits in.
        if matches!(screen.as_str(), "05-add" | "05-split" | "05-remove") {
            let said = crate::snapshot_verbs::stage(&window, &screen, &dir);
            println!("snapshot: {screen} | {}", said.join(" | "));
        }
        // F2.8's picture stages its own trim, through the door the strip's left-button drag calls (see
        // `snapshot_trim`). Same reason as above: `run` is past the size another staging block fits in.
        if screen == "05-trim" {
            let said = crate::snapshot_trim::stage(&window, &dir);
            println!("snapshot: 05-trim | {}", said.join(" | "));
        }
        // F2.10's picture stages its own two-camera figure too, through the page's own doors (see
        // `snapshot_rows`). Same reason as the two above: `run` is past the size another staging block fits in.
        if screen == "05-rows" {
            let said = crate::snapshot_rows::stage(&window, &dir);
            println!("snapshot: 05-rows | {}", said.join(" | "));
        }
        // The Narrate shot's own files go down AFTER the publish above: a `build()` later than this one
        // (one per tab) re-reads them, and only the files keep the voice, shift and take list on screen.
        crate::snapshot_narrate::seed_project_files(&screen);

        // F0.8's screen is the confirmation, not the window behind it: the dialog is what the spec
        // image shows, so it is what gets painted. The fixture is a named project folder, which is
        // why its body carries the "stays on disk as it is" paragraph.
        let subject: Option<gtk::Widget> = match screen.as_str() {
            // The insert form as §G draws it, staged by the same doors `05-insert` uses: the heading the page
            // pins ("insert timed.svg"), the card's declared field, the three radios, Seconds at the CARD's own
            // time, Cancel + Insert it. Painted as the whole window at a width the form column can fill -- the
            // form is part of the Cut page and cannot be reparented out of it (GTK refuses a child that already
            // has a parent), so the PAGE must reach a readable width for its rows not to collapse. The stage ran
            // above, in the `05-insert` arm's own door -- which leaves THIS form open on purpose.
            "05-insert-form" => None,
            // §08's second image is the ✎ dialog, not the page behind it, so the dialog is the subject
            // here -- same reason `03-settings` paints its own window. Opened the way the chip opens it,
            // with the mark's own words already in the box.
            "08-words" => {
                ui::produce_words::open_reword(&window, 0, "Ethereum, one lecture at a time");
                let shown = ui::produce_words::open_dialog().expect("the chip opens its dialog");
                shown.set_default_size(420, 300);
                Some(shown.upcast())
            }
            // No parent: a transient of the window is kept alive by it, and this run ends when the
            // last window closes. Painted on its own, which is how the spec image shows it.
            "03-new-confirm" => {
                let shown = ui::new_project_confirm(None, &new_project::confirm_detail(&dir));
                shown.set_default_size(560, 240);
                shown.present();
                Some(shown.upcast())
            }
            // §03's image is the dialog, not the window behind it, so the dialog is the subject here
            // too. The grid, the badge column and the Fetch models row all show at once.
            //
            // One row answers ✓: `no_probe` fails every row, which would leave a column of empty marks —
            // badges only get painted by a press, and a snapshot presses nothing. So the ffmpeg row is
            // pressed once through the same seam a click goes through (`press_test`, the one
            // `wire_buttons` wires), with `checks::ffmpeg_verdict`'s own passing sentence rather than an
            // invented one: that is what the spec's picture shows ("the ✓ is a passed ffmpeg test").
            "03-settings" => {
                // Painted from `pictured_conf` rather than the bare build: §5 gives every model box a
                // Slots spin, and the picture has to show the two controls as different numbers.
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
            // `spec/img/04-prompt-picker.png` pictures the row picker OPEN on a prompt. The subject is the LIST
            // itself (see `snapshot_subjects::subject_takes_the_name`), painted at the frame-taking step from
            // `bench::picker_rows`, because a DropDown pops its rows on their own surface and they are therefore
            // absent from the window's render node — an earlier whole-window shot came out byte-identical to
            // `04-prepare`. The row itself was asked for above, before `build_window`; row 2 = "Describe", per
            // `bench::ROWS`, which is what makes the page sidecar show the heading §1's picture labels.
            "04-prompt-picker" => None,
            // §1's own picture (`spec/img/03-window.png`) is captioned "Prepare page, LOG OPEN", so the
            // shell shot carries the exchange lines in an open expander — drawn through the same door §7's
            // recorder writes through, which opens and fills it exactly as a live run does. Without this the
            // shot was the page at rest with a closed log, which is not what §1 pictures. The reference log
            // carries `settings:` / `rescanned` lines rather than the §7 exchange lines below; the open
            // expander itself is the fidelity this shot records.
            "03-window" => {
                let step = crate::run::step(crate::shell::Page::Prepare).label();
                ui::note_exchange_line(&format!(">>> {step}: 451.4 kB of text and 0 image(s) went to the LLM"));
                ui::note_exchange_line(&format!(">>> {step}: 28.6 kB came back in 2m39s"));
                ui::note_exchange_line(&crate::exchanges::link_line("llm/prepare-page.html"));
                // …and the divider gets the ask a click on the header would leave behind, now the log is open.
                crate::snapshot_band::rest_band(&window);
                None
            }
            // §4's own picture is the LIST — eight stamped rows with their controls, not the empty scroller
            // the shipped fixture's single source would draw. All of it lives in `snapshot_sources`, which
            // stages the session and repaints through the page's own row painter.
            "03-sources" => {
                println!("snapshot: 03-sources rows = {}", crate::snapshot_sources::stage(&window));
                None
            }
            // §7's log lines, then the once-per-run link, in the expander a run would have opened. All
            // three go in through `ui::note_exchange_line`, the door §7's recorder writes through:
            // it pushes to the same sink every flow uses AND opens and fills the expander through
            // `paint_progress`, so what is painted is exactly what a live shell paints mid-run.
            "03-log-link" => {
                let step = crate::run::step(crate::shell::Page::Prepare).label();
                ui::note_exchange_line(&format!(">>> {step}: 451.4 kB of text and 0 image(s) went to the LLM"));
                ui::note_exchange_line(&format!(">>> {step}: 28.6 kB came back in 2m39s"));
                ui::note_exchange_line(&crate::exchanges::link_line("llm/prepare-page.html"));
                None
            }
            // §S7/S8 as the shell actually draws them, driven through the SAME door the ▶ handler calls
            // (`ui::prepare_guard::say_ending`) with the walk's finished outcome — not a poke of the
            // labels. The folder is a scratch `.naivepost` under the temp dir holding one file per
            // prepare/ subfolder, so the count on the status row and the Outputs readout are the same
            // number a real run would print (3), and the log expander has to be opened like a run's does.
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
                // The folder the ending counts and refreshes from is `save_apply::save_target()`'s, so
                // the scratch project has to be taken on first — exactly what Open does to a real one.
                ui::save_apply::note_open_for_snapshot(&session);
                // A run has to be OPEN for its ending to be the ending of a run: `start_run` is what
                // §S7's "fraction 1" and §S4's close both act on, so the shot opens the bookkeeping,
                // lets the ending speak, then closes it exactly as a single-step press does.
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
                // The close a real press runs between the ending and the next paint: the unload handed
                // over, the queue emptied. Then the log is opened, which is what paints the bar's face
                // and the status row last -- the order a finished press leaves them in.
                let view = ui::log_view(&window).expect("the log expander holds its text view");
                let status = ui::find_status(window.upcast_ref()).expect("the shell has a status line");
                crate::ui::run_close::close_step_run(
                    &bar,
                    &queue,
                    &std::rc::Rc::new(std::cell::RefCell::new(exchange)),
                    &status,
                    &ui::progress_bar(&window).expect("the shell carries a progress bar"),
                    &ui::log_expander(&window).expect("the shell carries the log expander"),
                    &view,
                    None,
                );
                // §S7: the ending is the run's LAST word, which is why it goes back after that close —
                // the close puts the queue's (now empty) line over the row. Same two lines the ▶ handler
                // writes in this position.
                if !bar.borrow().status.is_empty() {
                    let said = bar.borrow().status.clone();
                    status.set_text(&said);
                }
                show_the_log(&window).expect("the shell's log opens for its own snapshot");
                // ...and the ending goes back on the row after it: `show_the_log` paints through
                // `paint_progress`, whose status write would otherwise be the last word on screen.
                if !bar.borrow().status.is_empty() {
                    let said = bar.borrow().status.clone();
                    status.set_text(&said);
                }
                let _ = std::fs::remove_dir_all(&root);
                None
            }
            // F3.5's screen is the form itself, not the page behind it -- same reason the confirmation
            // paints its dialog: the Cut page's own column collapses to a sliver headless (see the note on
            // the default size above), so painting the whole window would show empty grey instead of the
            // fields. The holder is resolved THROUGH the window that owns the live form.
            // Painted as the whole window: the form is part of the Cut page and cannot be reparented out of
            // it (GTK refuses a child that already has a parent), so the page itself must reach a readable
            // width -- done above, before `present()`, with a default size plus the pinned preview panel.
            _ => None,
        };
        if subject.is_none() {
            // Render-only: a snapshot window is never resized by a user. §1 now says "Every screenshot is
            // taken at the minimum size", and `build_window` opens at exactly that (1240×900), so the shell
            // needs no ask of its own any more — what it still asks for is WIDER than the minimum, for screens
            // whose subject is laid out along the page's width. A
            // default size before `present()` is enough to inspect the page -- no shell relayout.
            if screen_page(&screen) == Some("Cut") || screen_page(&screen) == Some("Narrate") {
                // Wider than §1's minimum only for the three pass screens: their subject is an effects bar
                // laid out along SECONDS, and in a 1240 px column two clips' worth of bars crowd into the
                // leftmost tenth of the tape, where neither a colour nor a gap reads off the shot. The height
                // is always §1's minimum — nothing shoots taller than the size §1 fixes.
                let wide = matches!(screen.as_str(), "06-speeds" | "06-decorations" | "06-flows");
                window.set_default_size(if wide { 2400 } else { 1280 }, crate::shell::WINDOW_MIN_H);
            }
            if subject.is_none() {
                // The shell's page/bottom-bar split is NOT forced here any more, and the reason is a number:
                // this used to call `divider.set_position(shell::PAGE_BAND_PX)` (560), which in an 844 px
                // divider asked for 560 + the collapsed band and got clamped to "page = everything", so the
                // run-bar row landed on the window's last pixel and the log header fell below the bottom edge —
                // the shot with no bottom bar at all. §1 says the resting split is whatever leaves the band
                // "exactly as tall as the run bar row plus the log expander's header", and `window_divider`
                // now re-asks that at the paned's first real allocation, which is the same rule a person's drag
                // obeys. A snapshot must not override it: overriding the rule is how the shot looked right while
                // the program drew the wrong thing.
            }
            window.present();
        }
    // §1's tape is as wide as the page, so a snapshot has to ask for its picture AFTER GTK laid the page
        // out: a `DrawingArea` clips its draw callback to its own allocation, and headless nothing repaints
        // between the map and the shot. One `queue_draw` after the layout is enough — it lands on the same
        // surface the band paints into, at the width the paned actually gave it.
        // §04-prepare#1-screen: the Prepare page is TWO COLUMNS behind a draggable divider, and like the
        // Cut band its layout only exists after GTK has allocated the page. `present()` queues the map and
        // headless nothing else runs between here and the pixels, so without a drain the paned measures 0 px
        // wide, each column collapses to its own minimum, and the shot reads as one narrow column.
        if screen_page(&screen) == Some("Prepare") || screen_page(&screen) == Some("Produce") {
            let context = glib::MainContext::default();
            for _ in 0..64 {
                while context.pending() {
                    context.iteration(false);
                }
            }
        }
        // §4's picture is a CROP of one column (the list and the frame controls), which a whole-window shot
        // cannot show: at §1's minimum the left half is 596 px behind a prompt bench, and most of the staged
        // rows fall below the visible band. So this paints the column itself into the file the recipe asked
        // for — same reasoning as `write_face_png` painting the ▶ alone -- after the layout drain above.
        if screen == "03-sources" || screen_page(&screen) == Some("Prepare") {
            // The staged rows only read if the window is tall enough to show them, and the shell still has to
            // be §1's minimum height in the shot: both doors run in one place (`stage_for_shot`), including
            // the divider ask that gives the page the grown band back.
            crate::snapshot_sources::stage_for_shot(&window);
        }
    if screen_page(&screen) == Some("Cut") || screen_page(&screen) == Some("Narrate") {
        // Drain the loop so GTK has laid the page out before anything asks it for a width: `present()`
        // only queues the map, and headless nothing else runs between here and the pixels. Without this
        // the band's draw is clipped to its pre-layout request and the tape stops at its floor.
        let context = glib::MainContext::default();
        for _ in 0..64 {
            while context.pending() {
                context.iteration(false);
            }
        }
            if screen_page(&screen) == Some("Cut") {
            // §A's preview is "contain-fit, min height 160" and `spec/img/05-cut.png` draws it filling the
            // upper pane; `arrange` sized the panel from the OPENING (380 px), so once the page had real
            // allocations this hands it the pane it ACTUALLY got -- same door, the layout's own number.
            // NOT for `05-rows`: its staging shrinks this pane on purpose -- those camera rows are that
            // picture's subject, they sit BELOW everything here, and a re-grown preview pushes them past the
            // ~890 px a headless window paints.
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
                // The panel's own real width, which the 660 px divider gave it, is passed through: `grow_to`
                // floors the width at the panel's declared 1280 (nothing ffprobes headless, so that declaration
                // IS its shape), and asking for its own width is what lets the picture keep the pane's width
                // instead of being drawn 1280 wide and cropped to 660.
                let panel_w = panel.width().max(1);
                // …but the ask is CEILED at §A's opening by the page's own rule: feeding the pane's
                // CURRENT height back into the panel's request unceiled is a feedback loop — the grown
                // request becomes the start pane's minimum, the paned clamps its handle to it, and the
                // pane overflows the divider, clipping §1's first idle reading (badge **2**) out of the
                // shot (measured: `cut-top` 417 px at y=45 inside a paned whose edge sat at y=82).
                let tall = crate::ui::cut_page_layout::preview_ask_ceiling(top_h, lane_h);
                crate::ui::cut_preview_panel::grow_to(&panel, panel_w as f64, tall as f64);
                crate::ui::cut_preview_frame::repaint(&window);
                // No divider re-ask here: made before the window has §1's size it lands on the paned's clamp
                // and undoes `rest_page_splits`, which runs after the final size request.
                let _ = top_h;
            }
        }

/// What an open caption form shares the Cut page's column with BELOW it: its own `Apply`/`Cancel` row and the
/// `Kept as you type — ↶ Undo takes the whole edit back.` footer, which `spec/img/06-text.png` shows and a shot at §1's
/// fixed height cuts off, plus the page's own bottom band (the run bar and the `Log` row) that §06#3 packs under the
/// column. Measured once here, at the height the fixture's column needs to show the whole form with room to spare:
/// the form is ~250 px of heading, three-row words box, Length, the two fades and Curve, and the ~50 px of buttons
/// and note under them; the band below is the same ~110 px `05-cut` shows. Rounding up is safe — an overshoot only
/// leaves white space, where an undershoot hides the row the screen exists to show.
const FORM_UNDER_PX: i32 = 380;

            // `06-text`'s subject is the caption form BESIDE the preview, and §1's fixed window height is what clips
            // it: the Apply/Cancel row and the `Kept as you type — ↶ Undo takes the whole edit back.` footer fell
            // below the fold, where `spec/img/06-text.png` shows both. §1 grows the window where a subject is too
            // WIDE (a speeds or decorations shot asks 2400 px) and says a shot is worth growing rather than losing
            // what it came to show; this is the same exception down the other axis, and the ask is measured at the
            // page's own column rather than guessed: the Cut page's paned asks for its preview and its form column
            // together, and `set_size_request` (not a default size, which is an `adw::ApplicationWindow`'s FLOOR and
            // grew nothing here) is what actually moves the window to what the column wants.
            if screen == "06-text" {
                // Where the caption form's own box actually ends. The FORM'S MEASURED BOTTOM IS UNUSABLE HERE, and
                // the numbers say why: the `text-form-<tab>` holder resolves to a box whose bottom sits at 291 px on
                // a page whose column reaches 723 px, because `show_text_form` fills the FIRST holder that name finds
                // in the walk and the Cut page's column keeps several (the `zoom-form` / `speed-form` / `text-form` /
                // `svg-form` / `volume-form` / `label-form` rule, all built by `page_box` in order). The column's own
                // bottom is the honest number — it is what the page packs the form into — so the ask is the column
                // short of §1's fixed height plus what the shot has to show below the column (`FORM_UNDER_PX`).
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
            // The red line too: §1 puts it on its own layer at the second the preview stands on, and a shot
            // that never placed one showed no line at all (badges 30). Placed through the page's own door,
            // then repainted, exactly as a click on the tape would.
            if screen == "05-cut" {
                // The band's red line is the PREVIEW's playhead (`band_scene_for` reads it off the player),
                // so that is the door this has to go through — `set_line_position` moves the cursor state,
                // which the painter never asks. Placed before the redraw, exactly as a click on the tape does.
                ui::set_playhead(&window, 24.0);
                // Badge **31** (§A: "scrollbar (hidden when everything fits)"). The bar is drawn from
                // `cut_view::extents` -- filmed seconds x pps against the band's own width -- so nothing can
                // claim it is shown except that arithmetic, and the only door to it is the zoom. The staging
                // above took the shot to 15 px/s through the toolbar's own + button; this asks the same fit the
                // band will be drawn at, after the layout has given it a real allocation, and presses + from the
                // page's own door while the tape still fits -- six steps is where the staging left the shot, so
                // on a session that already overflows (this one: the 0-120 take §A's tape is, declared by that
                // staging) nothing is pressed and the printed pps is §A's 15. `paint_scrollbar` is never told
                // to show anything: the geared thumb (§B, 40 px of bar per screenful) appears because the
                // numbers ask for it, and a shot that still has no bar fails the render with the numbers.
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
            // `05-rows` parks its line inside the two cameras' overlap for the same reason, and its staging ran
            // before the map like every other screen's, so the band needs the playhead placed here too.
            if screen == "05-rows" {
                ui::set_playhead(&window, crate::snapshot_rows::LINE_T);

                // F2.10's picture is BOTH camera plates in one frame, so the shot says whether it reaches them
                // and refuses to write a PNG that does not: `write_png` paints at the window's own height while
                // the rows are the last thing on a page ~1015 px tall, so "covered" here means both tops inside
                // one band of that height, row 1 above row 2 -- badges **1** over **2** of `spec/img/05-rows.png`.
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
            // Badge 1 again, now that the panel has a real allocation: `refresh` measured a 0x0 plate on
            // the first call (before `present()`), so the picture it chose then was asked of an unmapped
            // widget. Through the page's own door again -- same call a line move makes.
            if screen == "05-cut" || screen == "05-rows" {
                crate::ui::cut_picture::refresh(&window);
            }
            // The take band re-fits its lane now that the scroller has a real width: the staging above ran
            // before `present()`, so the width it sized from was the one an unmapped widget answers.
            if screen_page(&screen) == Some("Narrate") {
                crate::ui::narrate_band::note_lane_size(&window);
                crate::ui::narrate_band::repaint(&window);
            }
        }
        let painted = subject.clone().unwrap_or_else(|| window.clone().upcast());
        // `03-runbar-busy` records a GLYPH, and a glyph is not legible in a 1334 px window shot, so this
        // screen paints the ▶ itself into `<out>` and the whole window beside it as `-window.png`: the
        // strip is what gets looked at, the page is there to show the bar in place.
        let face_only = screen == "03-runbar-busy";
        // §06-effects#2's Effect-menu screen: staging popped the dropdown and asked for this treatment by name
        // (`snapshot_effect_menu::note_shot_sidecar`), so no screen is hardcoded here.
        let menu_over_page = crate::snapshot_effect_menu::shot_sidecar_wanted();
        // …and when the spec's own picture IS a subject of its own, that subject takes the asked filename,
        // decided through each module's door rather than a name hardcoded here. Which screens hand their asked
        // filename to a subject of their own -- and which sidecar each subject was staged under -- is
        // `snapshot_subjects`'s table, so this step asks two questions of one place instead of growing a third
        // copy of the screen list.
        // Where the WINDOW shot goes: the asked name as ever, except when the screen's SUBJECT is something the
        // spec pictures on its own (a popover, the lane, the framed frame) -- then the subject holds `<name>.png`
        // and the page sits beside it as `<name>-page.png`. The sidecar is named `-page`, not `-window`, so that it
        // sorts BEFORE the asked filename (`-` 0x2D < `.` 0x2E): a viewer comparing this screen with its spec picture
        // takes the last PNG written, and that must be the subject the spec drew, not the page behind it.
        let page_out = if crate::snapshot_subjects::subject_takes_the_name(&screen) {
            out.with_extension("page.png")
        } else {
            out.clone()
        };
        // The closure below runs after `run`'s body has handed `screen` over, so it carries its own copy for the
        // one question it still has to ask of the screen name (which sidecar holds this shot's subject).
        let screen_for_frame = screen.clone();
        // The picker-subject question is asked inside the frame-taking closure too, which needs its own copy for
        // the same reason `screen_for_frame` exists.
        let screen_for_picker = screen.clone();
        let screen_for_heading = screen.clone();
        // The glyph goes beside the window shot as `<name>-face.png`; the whole page stays at `<name>.png`
        // so every snapshot keeps writing exactly one file under the name it was asked for -- except where the
        // spec pictures a SUBJECT instead of the page (a dialog, this screen's popover): then the subject takes
        // the asked name and the page goes beside it. The confirmation dialog's `subject` arm is that precedent.
        let face_out = out.with_extension("face.png");
        let out = out.clone();
        let app = app.clone();
        let face_out = face_out.clone();
        // §1: "Every screenshot is taken at the minimum size." Headless GTK clamps a window to its own
        // content's minimum and never honours a smaller ask after the map, so the only way the PNG comes out
        // at 1240×900 (or at the one wider width some screens above ask for) is to clamp it here, once the
        // layout has settled and just before the pixels are taken. Screens whose subject is a dialog of their
        // own (`subject` is not the shell) keep that dialog's size.
        if subject.is_none() {
            let want_w = window.default_width().max(crate::shell::WINDOW_MIN_W);
            let want_h = window.default_height().max(crate::shell::WINDOW_MIN_H);
            window.set_size_request(want_w, want_h);
            window.set_default_size(want_w, want_h);
            // The resize reaches the X window at a later frame: owed until the surface has the size asked.
            crate::ui::settle::when(
                "the window at the shot's size",
                &window,
                move |window| window.surface().is_some_and(|s| (s.width(), s.height()) == (want_w, want_h)),
                |_| {},
            );
            crate::snapshot_face::pump_context(32);
            // …and the Cut page's two dividers re-ask §A's openings now that the window has §1's real size:
            // headless a `notify::height` never fires for a widget allocated once, so both asks made during the
            // build landed on their clamp (measured: 5 px of `cut-page-split`), and every control below the
            // divider was built but painted nothing. Same doors a person's drag fires (`snapshot_layout`).
            crate::snapshot_layout::rest_page_splits(&window);
            crate::snapshot_face::pump_context(16);
            // §1's screenshot IS the window at 1240×900, so the PNG is asked for at that size whatever the
            // headless window ended up ALLOCATED. `write_png` shoots the widget's own box, and under xvfb
            // that box lands a hair under the floor (§1's numbers are the window's, frame and WM decoration
            // excluded), so the shot used to come out 1230×890 — the same page, 10 px short on each axis,
            // which is enough to shift what fits in the header and the run-bar row when it is compared
            // pixel-for-pixel with `spec/img`.
            crate::snapshot_face::set_shot_size(want_w, want_h);
        }
        // The band's resting split, asked at the size the PNG is taken at (see `snapshot_band::rest_band`).
        crate::snapshot_band::rest_band(&window);
        // §1's bottom band belongs to every shell screenshot, so the shot says where it landed: a page whose
        // minimum eats the run-bar row shows no bottom bar at all and still exits 0 (see `snapshot_band`).
        if let Some(said) = crate::snapshot_band::report(&window) {
            println!("snapshot: {said}");
        }
        // The picture is taken at `--shot-on`'s event, on a painted frame of the widget it paints
        // (`snapshot_ready`): no guessed wait, and a screen that does not settle fails naming why.
        let quit = app.clone();
        let screen_for_report = screen.clone();
        let shot = painted.clone();
        // A menu painted over the page is staged now, in a holder of its own, so it is laid out when the
        // ready event comes; that screen's event is the menu shown, not the page alone.
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
            // The settings dialog's staged ffmpeg verdict goes on now: the map has happened, so the label
            // can hold a mark, and this runs before the pixels are taken.
            crate::snapshot_settings::apply_staged_badge();
            if face_only {
                // The glyph the screen exists to record, painted on its own after the window shot.
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
                // §2's picture for this screen is the Cut page WITH the ✚ Effect menu open over it, so the window
                // keeps `<name>.png` and the menu is painted where its own button sits, at its own size
                // (`spec/QUESTIONS.md`, §06 answer 3(b)) — the `04-prompt-picker` precedent above. The popover
                // itself lives on a separate surface and cannot come out of the window's render node, which is why
                // the panel is blitted on rather than popped and hoped for.
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
                // §4's subject is the LEFT column (the row list and the frame controls under it), which a window
                // shot cannot show at a readable size, so the column takes `<name>.png` and the page it was
                // staged on goes beside it as `<name>-page.png` — the popover and picker precedent above.
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
                // §1's second picture is the PAGE with the picker's list open over it, so the window keeps the
                // asked filename and the list is painted where the popover would sit — hanging under the picker
                // button, the heading row and the text box still visible beside it (`spec/QUESTIONS.md`, §04
                // answer 3c). The sidecar is this screen's own copy of the page, kept for a byte-for-byte
                // comparison with `04-prepare.png`.
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
                        // The tick's row and name go to the log line through one door of the picker module.
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
            // A subject staged into its own file (the lane at 1240 px, the framed preview) takes the ASKED name,
            // because that file IS what the spec drew; the window shot above moves beside it.
            if let Some(sidecar) = crate::snapshot_subjects::subject_sidecar(&screen_for_frame) {
                // The lane's subject is cropped HERE rather than during staging: only now has the window been
                // mapped and drawn, which is the one thing a render node needs. Every pixel in it — gutter rail,
                // fold-all −, sound switches, thumbnails, wave strip — is ink the page itself put down.
                if sidecar == "lane-shot" {
                    match crate::snapshot_lane::paint_track_area(
                        &window,
                        &out.with_extension("lane-shot.png"),
                    ) {
                        Ok(said) => println!("snapshot: {said}"),
                        Err(err) => eprintln!("snapshot: {err}"),
                    }
                }
                // The rename is only this screen's business: staging wrote the sidecar for every Cut-page shot, and
                // on those the window already holds the asked name -- renaming over it would replace the page with
                // a strip. `page_out != out` is exactly "a subject took the asked name here".
                let staged = out.with_extension(format!("{sidecar}.png"));
                if page_out == out {
                    // Ordinary page screen: nothing to move; the sidecar stays beside the shot under its own name.
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
    // The handler closes the last window, which quits the main loop. A nested
    // block_on here would recurse on the context that already runs it.
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

/// A mono 16-bit PCM wav of `samples` silent samples at `rate`: the smallest file `voice_ref`'s
/// header reader and the take band's seconds arithmetic both accept, written without ffmpeg.
///
/// Not all-zero bytes: a shot's band is supposed to show a WAVEFORM, and the envelope the band draws is the
/// loudest sample of each 5 ms bucket — pure silence folds to a flat line that reads as an empty lane. So
/// every bucket carries one small tone burst, enough for `wave_build::peak_byte` to land well above zero
/// while staying far from clipping (a voice reference must never be loud, it is only measured).
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
    // A SPEECH-LIKE envelope, not a flat block: the band draws each peak at `peak / 255` of its lane height
    // (`ui::narrate_band::draw_envelope`), and peaks are the loudest sample of each 5 ms bucket folded onto
    // a byte (`wave_build::peak_byte`), so the SHAPE has to change on a scale coarser than one bucket or it
    // averages back to a rectangle. Hence SYLLABLES of 0.20-0.50 s at 0.15-0.70 of full scale (peaks land
    // around 40-180 of the 255) separated by word gaps of 0.15-0.40 s of near-silence.
    //
    // Lengths are counted in SAMPLES (`rate * 0.2` is 1600 samples at 8 kHz), which is what `at` counts; the
    // random draw is a seeded xorshift advanced once per syllable, so the same shot renders the same wave
    // every time — deterministic without a table, and no flaky pixel comparison downstream.
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
        // 0.20-0.50 s of "speech" at 0.15-0.70 of full scale.
        let loud_end = at + (2 * rate / 10 + (next() % (rate as u64 / 2)) as usize);
        let amp = (i16::MAX as f64) * (0.15 + (next() % 5_600) as f64 / 10_000.0);
        while at < loud_end && at < samples as usize {
            let phase = (at % period) as f64 / period as f64 * std::f64::consts::TAU;
            let offset = 44 + at * 2;
            bytes[offset..offset + 2].copy_from_slice(&((amp * phase.sin()) as i16).to_le_bytes());
            at += 1;
        }
        // The gap after it: near-silence, so the wave has quiet stretches between the loud ones.
        at = (at + rate * 15 / 100 + (next() % (rate as u64 * 25 / 100)) as usize)
            .min(samples as usize);
    }
    std::fs::write(path, bytes)
}

/// The shot's own take band, seeded the way §A.7 says it appears: the band is drawn for a narrator slot
/// whose RECORDING exists, so two things have to be true before the page first lays out — a source tagged
/// narrator 1 whose file is on disk, and a reference that has been built from it.
///
/// The recording half matters because the fixture cannot carry one: `fixtures/demo.naivepost` tags
/// `project:sources/lecture.mkv`, which no checkout holds (the fixtures are text plus tiny generated files,
/// and a real camera recording would be hundreds of megabytes). With nothing at that path §A.7's rule answers
/// "no recording", which is why `just snapshot 07-take` printed "take band seeded = true" over a shot with
/// no lane in it. So the renderer stands a short silent wav there — the same kind of file the app itself
/// writes into its scratch folders, made without ffmpeg — and points the tagged source at it through the
/// project door the app reads projects by. An EMPTY voice.txt is the default project's voice, which is the
/// captions-only choice, and captions have no recording, so the file is written too.
/// Writing these is the state the app itself reaches by tagging a recording and building a reference; the
/// bench only gets there without a camera or a long encode. Returns whether a band can be drawn, so a screen
/// can say which face it took.
/// The seconds the shot's Narrate files hold: long enough that the band fits them at a readable px/s and
/// shows takes inside them, short enough to write in a blink.
const SEED_SECONDS: u32 = 45;
/// The rate they are written at — the decode's own, so `wave_build`'s fold is exact over them.
const SAMPLE_RATE: u32 = crate::transcribe::SAMPLE_RATE as u32;

pub fn seed_voice_reference(tree: &crate::layout::Tree) -> bool {
    let base = crate::narrate_data::base_reference(tree);
    let Some(folder) = base.parent() else {
        return false;
    };
    if std::fs::create_dir_all(folder).is_err() {
        return false;
    }
    // The recording behind the narrator slot, stood up first: §A.7 asks about THIS file, and every answer
    // downstream (visibility, the ruler's seconds, the envelope) reads it through the project.
    if !seed_narrator_recording(tree) {
        return false;
    }
    if crate::voice_ref::wav_header(&base).is_none() {
        // 45 s at the DECODE rate (`transcribe::SAMPLE_RATE`, 16 kHz): the band spans whichever file it
        // measures first (the reference, then the recording), and two different lengths made the drawn wave
        // stop part-way across a ruler sized by the longer one. The rate is the decode's because that is the
        // rate `wave_build`'s fold assumes — see [`crate::snapshot_narrate::stage_envelope`].
        if write_silence_wav(&base, SAMPLE_RATE, SEED_SECONDS * SAMPLE_RATE).is_err() {
            return false;
        }
    }
    // The slot the recording belongs to: `narrator1` is the always-available narrator, and naming it is
    // what takes the page off the captions-only face (`narrate_screen::CAPTIONS` is the empty choice).
    let _ = crate::narrate_data::write_voice(tree, "narrator1");
    // §F4.6 S4 removes these on every take, pitch or voice change, and the shot's own arm makes a take after
    // this runs — so the files the bench just wrote are gone again before the pixels. Put them back for the
    // same reason the arm re-writes `takes.json`: a snapshot shows the state the flow is standing in, and
    // §A.7's lane needs a recording under it whatever the invalidation did.
    if crate::voice_ref::wav_header(&base).is_none() {
        let _ = write_silence_wav(&base, SAMPLE_RATE, SEED_SECONDS * SAMPLE_RATE);
    }
    crate::snapshot_narrate::stage_seeded_envelopes(tree);
    crate::ui::narrate_band::recording_seconds().is_some()
}

/// Stand a recording where the project's narrator-1 row points, and point the row at it if that file is not
/// there. Silent wavs only: no ffmpeg, no camera, and nothing that could make a shot depend on a machine.
fn seed_narrator_recording(tree: &crate::layout::Tree) -> bool {
    let Ok(mut project) = crate::project::load(tree.dir()) else {
        return false;
    };
    let Some(row) = project
        .sources
        .iter_mut()
        .find(|source| source.narrator == 1)
    else {
        // Nothing is tagged: §A.7 has no recording to ask about, and inventing a row here would be the shot
        // making up project state the Prepare step never produced.
        return false;
    };
    let stored = row.path.clone();
    let target = crate::layout::resolve(tree.dir(), tree.dir(), &stored);
    // A file that is there but is not audio (the working copy's `lecture.mkv` is a 4 KB stand-in, not a
    // matroska) answers ffprobe with "Invalid data", so §A.7's band would have no seconds to span. Ask the
    // probe rather than trusting `is_file`: whatever it cannot measure gets stood up as a real wav.
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
    // 45 s of silence at the decode rate mono: long enough that the band fits it at a readable px/s and shows
    // takes inside it, small enough to write in a blink (~360 KB).
    let renamed = target.with_extension("wav");
    if write_silence_wav(&renamed, SAMPLE_RATE, SEED_SECONDS * SAMPLE_RATE).is_err() {
        return false;
    }
    if renamed != target {
        // A wav where an .mkv was named: the row has to SAY wav, or ffprobe, the wave cache and every later
        // reader ask a container that is not there. The stand-in goes with it — two files claiming one row.
        let _ = std::fs::remove_file(&target);
    }
    row.path = crate::layout::write_path(tree.dir(), tree.dir(), &renamed);
    crate::project::save(&project, tree.dir()).is_ok()
}

/// How long one audio file holds, by header first and ffprobe second — [`crate::narrate_band_face`]'s
/// reading, reached through the same door the band uses so the shot and the page can never disagree.
fn recording_seconds_of(path: &Path) -> Option<f64> {
    if let Some((channels, rate)) = crate::voice_ref::wav_header(path) {
        let bytes = std::fs::metadata(path).ok()?.len();
        let per_second = (channels.max(1) as u64) * (rate.max(1) as u64) * 2;
        return Some(((bytes.saturating_sub(44)) as f64 / per_second as f64).max(0.1));
    }
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    crate::cut_lanes::probe_duration(&configured, &path.display().to_string(), &mut |_| {})
        .ok()
        .filter(|seconds| *seconds > 0.0)
}
