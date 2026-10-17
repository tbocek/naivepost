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
        "03-window" | "03-settings" | "03-sources" | "03-new-confirm" | "03-policy-form" => {
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
        | "06-lane" | "06-svg" | "06-effect-menu" => Some("Cut"),
        // §06-effects#5's own screen: the INSERT FORM with a card's own seconds already in it. No `spec/img`
        // counterpart, so `spec/inventory/cut.md` §G is again the widget reference (heading, declared field,
        // three radios, Seconds, Cancel + Insert it).
        | "05-insert-form" => Some("Cut"),
        "07-narrate" | "07-take" => Some("Narrate"),
        "08-produce" | "08-words" => Some("Produce"),
        _ => None,
    }
}



pub fn run(screen: &str, dir: &Path, out: &Path) -> Result<(), String> {
    let page = screen_page(screen).ok_or_else(|| format!("unknown screen {screen}"))?;
    if !dir.join(project::PROJECT_FILE).is_file() {
        return Err(format!("no {} in {}", project::PROJECT_FILE, dir.display()));
    }
    let model = project::load(dir)?;

    gtk::init().map_err(|err| err.to_string())?;
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
        // Both of these ask for row 2 (Describe): §1's own picture of the Prepare page is the page *with
        // the Describe prompt open*, and the picker shot is that same row with its dropdown spread.
        if matches!(screen.as_str(), "04-prepare" | "04-prompt-picker") {
            ui::set_bench_open_row(2);
        }
        // F2.10: a snapshot renders a PROJECT, so the folder it was asked for is taken on exactly as Open
        // takes one on — otherwise nothing can resolve a stored source path and the session's recordings
        // (stamped names + probed lengths) could never be laid out from the app's own rules. Taken on BEFORE
        // the window exists, because `build_window` seeds this window's session spans as it builds.
        ui::save_apply::note_open_for_snapshot(&dir);
        if screen_page(&screen) == Some("Narrate") {
            // BEFORE the window is shown: §A.7's visibility rule runs during `refresh`, and a scroller
            // hidden then stays hidden however many refreshes follow — so the reference has to be on
            // disk before the page first lays out, not after.
            if let Some(tree) = crate::ui::window::narrate_session_tree() {
                let band = seed_voice_reference(&tree);
                println!("snapshot: take band seeded = {band}");
            }
        }
        let window = ui::build_window(app, &model, page);
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
        // carries -- the demo project's `cut/cut.json` is one camera in three clips. A snapshot asked for
        // by that image's name therefore lays the figure out here: camera 1 for 0-37, camera 2 overlapping
        // from 20 with the shift correction its name plate shows, one lane so there is a speaker badge and
        // a gutter switch, the line inside the overlap, and row 2 watched so the dashed outline is in shot.
        if screen == "05-rows" {
            let two_cameras = cut::Cut {
                segs: vec![
                    cut::Seg { s: 0.0, e: 37.0, cam: 0, ..Default::default() },
                    cut::Seg { s: 20.0, e: 37.0, cam: 1, ..Default::default() },
                ],
                lanes: vec![cut::Lane {
                    name: "cam1".into(),
                    src: "cam1".into(),
                    at: 20.0,
                    off: 0.0,
                    dur: 17.0,
                }],
                shift: [("cam1".to_string(), -19.0)].into_iter().collect(),
                ..Default::default()
            };
            ui::seed_review_cut(&window, &two_cameras);
            ui::set_line_position(&window, cut_line::LinePos { t: 25.0 });
            ui::press_watch_row(&window, 1);
            ui::refresh_camera_rows(&window);
        }
        // F2.11's own picture (`spec/img/05-fold.png`) is a WIDE dropped stretch folded to a seam. The
        // demo fixture cannot show one: its `cut/cut.json` keeps 0-9.5, 10-30 and 40-70 out of a tape
        // whose only filmed gap is 0.5 s = 2 px at the snapshot zoom, under `cut_fold::FOLD_MIN_PX`, so
        // no badge would be drawn at all. Laid out here instead -- the shared fixture file untouched:
        // the session filmed 0-120 as one take (that is what gives head/hole/tail anywhere to exist, see
        // `ui::page_recordings`), two clips are kept, the 30-40 hole is FOLDED so the bars meet at a
        // seam with a `+` on it, and the 60-120 tail is left open wearing a `-`.
        if screen == "05-cut" || screen == "05-fold" {
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
            crate::ui::cut_picture::refresh(&window);
            // Badge **31** is "hidden when everything fits", and at the opening 4 px a second this session
            // does fit — so no snapshot of the Cut page had ever shown the bar. Deepen the zoom through the
            // toolbar's own + door (the same one a click reaches) until the tape outgrows the view: the bar
            // then appears because the geometry says it must, not because anything was forced visible.
            // Six steps from 4 px/s lands on 15 px/s (§A's factor 1.25), which is also what §2's floor now
            // clamps to once − is walked back, so the shot shows the band at a zoom the ladder really stops at.
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
        // tests/press_play_widgets.rs instead of by a picture.
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
        // Driven through the same doors the buttons fire (`tests/cut_copy_paste_lane_widgets.rs` presses these
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
        // never by poking state; the other three refusals live in `tests/f2_14_guard_order.rs`.
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
            // bytes are visible, in `tests/sec_05_cut_8_suggest_wired.rs`.
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
                ui::cut_suggest_land::set_session_for_test(staged.clone());
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
        crate::snapshot_effects::paint(&window, &screen);
        crate::snapshot_preview::paint(&window, &dir, &screen);
        crate::snapshot_narrate::stage(&window, &dir, &screen);
        crate::snapshot_produce::paint(&window, &screen);
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
            "05-insert-form" => {
                window.set_default_size(760, 900);
                None
            }
            // §08's second image is the ✎ dialog, not the page behind it, so the dialog is the subject
            // here -- same reason `03-settings` paints its own window. Opened the way the chip opens it,
            // with the mark's own words already in the box.
            "08-words" => {
                ui::produce_words::open_reword(&window, 0, "Ethereum, one lecture at a time");
                let shown = ui::produce_words::open_dialog().expect("the chip opens its dialog");
                shown.set_default_size(420, 220);
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
            // F0.7's screen is the policy form itself, not the window behind it: §03's image shows
            // only the table of fields, so this paints only the table. No parent for the same reason
            // as the confirmation above — the run ends when the last window closes.
            "03-policy-form" => {
                let shown = ui::policy_form::build(None, &model.policy);
                // Six rows now (the five switches plus §12 entry 21's target length). The height is MEASURED
                // rather than guessed: an `adw::Window` that is not yet mapped answers its own minimum, and a
                // fixed 420 px clipped the last row once the sixth arrived.
                let wanted = shown.measure(gtk::Orientation::Vertical, -1).1;
                shown.set_default_size(720, wanted.max(500));
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
            // `spec/img/04-prompt-picker.png` pictures the row picker OPEN on a prompt, so this
            // shot is the same Prepare window with the bench started on the Describe row instead of
            // the User Context. The subject stays the window — the picker only makes sense in place,
            // beside the title and Reset it drives — and the row itself was requested above, before
            // `build_window`. Row 2 = "Describe", per `bench::ROWS`.
            //
            // The open list itself cannot be captured here: a GtkDropDown pops its rows as its OWN
            // top-level surface, and [`write_png`] paints one widget into one PNG, so pressing the
            // picker for the shot would change nothing the picture can show (it was tried: the file came
            // out the same picture as `04-prepare`). The page's picker is right as it is — §1's badge 3
            // is a row picker — so what this shot proves is the row chosen, not the popup.
            "04-prompt-picker" => None,
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
            // F3.5's screen is the form itself, not the page behind it -- same reason `03-policy-form`
            // paints its dialog: the Cut page's own column collapses to a sliver headless (see the note on
            // the default size above), so painting the whole window would show empty grey instead of the
            // fields. The holder is resolved THROUGH the window that owns the live form.
            // Painted as the whole window: the form is part of the Cut page and cannot be reparented out of
            // it (GTK refuses a child that already has a parent), so the page itself must reach a readable
            // width -- done above, before `present()`, with a default size plus the pinned preview panel.
            _ => None,
        };
        if subject.is_none() {
            // Render-only: a snapshot window is never resized by a user, and the Cut page collapses into one
            // unreadable narrow column when nothing asks for width. `05-cut` reaches a wide layout today only
            // because its fold/camera-row refreshes happen to widen it; the `06-*` screens never do. A
            // default size before `present()` is enough to inspect the page -- no shell relayout.
            if screen_page(&screen) == Some("Cut") || screen_page(&screen) == Some("Narrate") {
                // 1280x800 rather than the Cut page's 1240x900: at 900 the Narrate shot renders ~3000 px
                // tall and every control reads as a hairline. This is closer to §1's own screenshot ratio.
                // The two pass screens (`06-speeds`, `06-decorations`) are the exception, and this is the ONE
                // place their width is asked for: their subject is an effects bar laid out along SECONDS, and in
                // a narrow column two clips' worth of bars crowd into the leftmost tenth of the tape, where
                // neither a colour nor a gap reads off the shot. So they keep 2400 instead of being pulled back.
                // `05-insert-form` keeps its own smaller size, asked where its subject is chosen above.
                let wide = matches!(screen.as_str(), "06-speeds" | "06-decorations");
                if screen != "05-insert-form" {
                    window.set_default_size(if wide { 2400 } else { 1280 }, 800);
                }
            }
            window.present();
        }
        // §1's tape is as wide as the page, so a snapshot has to ask for its picture AFTER GTK laid the page
        // out: a `DrawingArea` clips its draw callback to its own allocation, and headless nothing repaints
        // between the map and the shot. One `queue_draw` after the layout is enough — it lands on the same
        // surface the band paints into, at the width the paned actually gave it.
        if screen_page(&screen) == Some("Cut") {
            // Drain the loop so GTK has laid the page out before anything asks it for a width: `present()`
            // only queues the map, and headless nothing else runs between here and the pixels. Without this
            // the band's draw is clipped to its pre-layout request and the tape stops at its floor.
            let context = glib::MainContext::default();
            for _ in 0..64 {
                while context.pending() {
                    context.iteration(false);
                }
            }
            // The red line too: §1 puts it on its own layer at the second the preview stands on, and a shot
            // that never placed one showed no line at all (badges 30). Placed through the page's own door,
            // then repainted, exactly as a click on the tape would.
            if screen == "05-cut" {
                // The band's red line is the PREVIEW's playhead (`band_scene_for` reads it off the player),
                // so that is the door this has to go through — `set_line_position` moves the cursor state,
                // which the painter never asks. Placed before the redraw, exactly as a click on the tape does.
                ui::set_playhead(&window, 24.0);
            }
            crate::ui::cut_view::repaint_page(&window);
        }
        // A dialog sized for its content needs the same drain the Cut tape needs: `present()` only queues the
        // map, so headless the window is still at its requested size when the shot is taken and GTK warns that
        // the content exceeds it. `03-policy-form` grew a sixth row (§12 entry 21) and started clipping.
        if screen == "03-policy-form" {
            // The form is a separate top level; `subject` holds it. Ask for the height its content needs,
            // measured after the map: an unmapped `adw::Window` answers 412 for any request, so a fixed default
            // size silently cut off the last row when §12 entry 21's sixth arrived.
            let form = subject.clone().expect("this screen paints the form itself");
            let context = glib::MainContext::default();
            for _ in 0..64 {
                while context.pending() {
                    context.iteration(false);
                }
            }
            let wanted = form.measure(gtk::Orientation::Vertical, -1).1;
            if let Some(top) = form.root().and_downcast::<gtk::Window>() {
                top.set_default_size(720, wanted.max(500));
            }
            for _ in 0..8 {
                while context.pending() {
                    context.iteration(false);
                }
            }
        }
        let painted = subject.clone().unwrap_or_else(|| window.clone().upcast());
        // `03-runbar-busy` records a GLYPH, and a glyph is not legible in a 1334 px window shot, so this
        // screen paints the ▶ itself into `<out>` and the whole window beside it as `-window.png`: the
        // strip is what gets looked at, the page is there to show the bar in place.
        let face_only = screen == "03-runbar-busy";
        // The glyph goes beside the window shot as `<name>-face.png`; the whole page stays at `<name>.png`
        // so every snapshot keeps writing exactly one file under the name it was asked for.
        let face_out = out.with_extension("face.png");
        let out = out.clone();
        let app = app.clone();
        let face_out = face_out.clone();
        // One more cycle after the map: the map happens before the first frame is drawn. The paint
        // and the quit are synchronous here, so no second mapped window is ever waited on.
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
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
            } else if let Err(err) = write_png(&painted, &out) {
                eprintln!("snapshot: {err}");
                OUT.with(|e| *e.borrow_mut() = Some(err));
            }
            app.quit();
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
    std::fs::write(path, bytes)
}

/// The shot's own take band, seeded the way §A.7 says it appears: the band is drawn only for a narrator
/// slot whose recording is on disk, so `voice_ref_base.wav` has to exist before the page first lays out.
/// An EMPTY voice.txt is the default project's voice, which is the captions-only choice — and captions
/// have no recording, so the shot used to render a bare voice picker with no band under it.
/// Writing the file is the state the app itself reaches by building a reference; the bench only gets
/// there without ffmpeg. Returns whether a band can be drawn, so a screen can say which face it took.
pub fn seed_voice_reference(tree: &crate::layout::Tree) -> bool {
    let base = crate::narrate_data::base_reference(tree);
    let Some(folder) = base.parent() else { return false };
    if std::fs::create_dir_all(folder).is_err() {
        return false;
    }
    if crate::voice_ref::wav_header(&base).is_none() {
        // 8 kHz is the smallest rate `voice_ref::wav_header` accepts, and 60 s is plenty of band to fit.
        if write_silence_wav(&base, 8_000, 60 * 8_000).is_err() {
            return false;
        }
    }
    // The slot the recording belongs to: `narrator1` is the always-available narrator, and naming it is
    // what takes the page off the captions-only face (`narrate_screen::CAPTIONS` is the empty choice).
    let _ = crate::narrate_data::write_voice(tree, "narrator1");
    crate::ui::narrate_band::recording_seconds().is_some()
}
