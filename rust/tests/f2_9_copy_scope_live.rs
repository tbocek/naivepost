// F2.9 — the footage/sound fork is AIMABLE on the painted band, and a paste asks the SOURCE for its file
// second instead of guessing from the session clock.
//
// Two things the completion check found missing, both pinned here:
//
// * S1's scope came only from `cut_select::PLACEHOLDER_BANDS`, so no y on the real band ever answered
//   `Surface::WaveStrip` and ⧉ Copy could not be aimed at a recording's sound. `cut_tracks::bands_for` now
//   splits each camera row into picture + wave strip, so a press in the lower third answers `WaveStrip` and
//   `sound_scope` names THAT row's own camera (`cam<N>`), never a stand-in literal.
// * S2/S3 passed the SESSION second straight through as a FILE second, so on any recording that does not
//   start at 0:00 a pasted span asked the tape for material from later than the person pointed at.
//   `cut_copy::file_seconds` converts through §B's `at(t)` now, shift included, clamped at the tape's head.
//
// Pure half needs no window; the geometry half builds one because `Geometry::of` reads the page's zoom.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{self, Cut, Seg};
use naivepost::cut_copy::{self as cp, Hand};
use naivepost::cut_select::{Scope, Surface};
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;
use naivepost::ui::cut_select_press::{self, Geometry};

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle};

static RAN_FILE_SECONDS: AtomicBool = AtomicBool::new(false);
static RAN_BAND_SCOPE: AtomicBool = AtomicBool::new(false);
static RAN_FORK: AtomicBool = AtomicBool::new(false);
static RAN_LANE_FILMED: AtomicBool = AtomicBool::new(false);
static RAN_PIECES_NOT_STAND_IN: AtomicBool = AtomicBool::new(false);
static RAN_LANE_OFF_PERSISTS: AtomicBool = AtomicBool::new(false);

/// §B's `at(t)`, asked of the hand: which second INSIDE the source do these seconds come from?
fn s2_the_paste_asks_the_source_for_the_file_second_under_it() {
    let filmed = vec![
        Recording { base: "cam0".to_string(), start: 0.0, end: 30.0 },
        Recording { base: "mic".to_string(), start: 30.0, end: 90.0 },
    ];
    let plain = Cut::default();
    // A recording that starts AFTER zero is the case that used to be wrong: session 40 on a tape that opened
    // at 0:30 is the tape's tenth second, not its fortieth.
    let mic = Hand { from: 40.0, length: 6.0, scope: Scope::Sound { recording: "mic".to_string() } };
    assert_eq!(cp::file_seconds(&plain, &filmed, &mic), 10.0, "session 40 on a tape starting at 30 is file 10");
    // The hand correction rides along: a slid recording's material starts elsewhere (§B's `shift`).
    let slid = Cut {
        shift: [("mic".to_string(), 5.0)].into_iter().collect(),
        ..Default::default()
    };
    assert_eq!(cp::file_seconds(&slid, &filmed, &mic), 15.0, "and +5 s of slide makes it file 15");
    // Before the recording began there is nothing earlier on that lane: 0, not -10.
    let early = Hand { from: 20.0, length: 6.0, scope: Scope::Sound { recording: "mic".to_string() } };
    assert_eq!(cp::file_seconds(&plain, &filmed, &early), 0.0, "a span older than its tape clamps to its head");
    // Footage has no single source, so the RUN holding the opening second is converted.
    let foot = Hand { from: 10.0, length: 6.0, scope: Scope::Footage { row: 0 } };
    assert_eq!(cp::file_seconds(&plain, &filmed, &foot), 10.0, "cam0 opens at 0, so session 10 is file 10");
    let late_foot = Hand { from: 40.0, length: 6.0, scope: Scope::Footage { row: 0 } };
    assert_eq!(cp::file_seconds(&plain, &filmed, &late_foot), 10.0, "the run holding 40 is mic (starts 30)");
    // Unknown recording / no tapes at all: 0, the honest "nothing to convert against".
    let ghost = Hand { from: 40.0, length: 6.0, scope: Scope::Sound { recording: "gone".to_string() } };
    assert_eq!(cp::file_seconds(&plain, &filmed, &ghost), 0.0, "a file that left the session has no offset");
    assert_eq!(cp::file_seconds(&plain, &[], &foot), 0.0, "no filmed runs, no conversion");
    println!("f2_9 file_seconds: mic@40=10 (+shift5=15), early=0, cam0@10=10, mic-run@40=10, ghost=0");
    RAN_FILE_SECONDS.store(true, Ordering::SeqCst);
}

/// ⇲ Lane's "nothing is rolling at m:ss" must ask the FILMED runs, not the kept ones: a second inside a
/// dropped stretch of a take really was filmed, and refusing it would be refusing something the tape has.
fn s3_a_lane_starts_where_the_session_actually_filmed() {
    let hand = Hand { from: 20.0, length: 8.0, scope: Scope::Footage { row: 0 } };
    // One take, 0-120, but the cut keeps only 10-40: 60 is inside the take and outside every kept clip.
    let takes = vec![Recording { base: "session-tape".to_string(), start: 0.0, end: 120.0 }];
    let filmed = naivepost::timeline::filmed_runs(&takes);
    let kept: Vec<(f64, f64)> = vec![(10.0, 40.0)];
    assert!(
        matches!(cp::lane_start(Some(60.0), &hand, &filmed), cp::LaneStart::Start(60.0)),
        "a second inside a take may open a row even where the cut kept nothing"
    );
    assert!(
        !matches!(cp::lane_start(Some(60.0), &hand, &kept), cp::LaneStart::Start(_)),
        "…which the kept-footage stand-in would have refused — that is the difference being pinned"
    );
    // Outside every take, nothing is rolling and the refusal says so.
    match cp::lane_start(Some(200.0), &hand, &filmed) {
        cp::LaneStart::Refusal(said) => assert!(
            said.contains("nothing is rolling at"),
            "the refusal names what is missing: {said}"
        ),
        other => panic!("a line past the last take must refuse, got {other:?}"),
    }
    println!("f2_9 lane_start: filmed 0-120 accepts 60 (kept-only refuses it); 200 refuses");
    RAN_LANE_FILMED.store(true, Ordering::SeqCst);
}

/// The band's own geometry decides the scope, and the two halves of a camera row answer differently. Then the
/// same scopes are shown to fork the way S2 says they must.
fn s1_a_picture_row_is_footage_and_a_wave_strip_names_that_rows_recording() {
    let app = adw::Application::builder()
        .application_id("org.gtk.naivepost.test.f29scope")
        .build();
    let done = std::cell::RefCell::new(None);
    let out = done.clone();
    app.connect_activate(move |app| {
        let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
        let window = ui::build_window(app, &model, "Prepare");
        window.present();
        ui::tab_button(&window, Page::Cut)
            .expect("the shell has a Cut tab")
            .emit_clicked();
        // Two cameras' worth of kept footage, so the band draws two rows and a wave strip under each.
        let seeded = Cut {
            segs: vec![
                Seg { s: 0.0, e: 30.0, cam: 0, ..Default::default() },
                Seg { s: 5.0, e: 25.0, cam: 1, ..Default::default() },
            ],
            ..Default::default()
        };
        ui::seed_review_cut(&window, &seeded);
        ui::set_session_recordings(&[
            Recording { base: "cam0".to_string(), start: 0.0, end: 30.0 },
            Recording { base: "cam1".to_string(), start: 5.0, end: 25.0 },
        ]);
        settle();

        let geo = Geometry::of(&window);
        assert!(geo.bands.wave_height > 0.0, "the painted band reserves a wave strip per row");
        let pitch = geo.bands.row_height + geo.bands.wave_height;
        let picture_y = geo.bands.first_picture + geo.bands.row_height / 2.0;
        let wave_y = geo.bands.first_picture + geo.bands.row_height + geo.bands.wave_height / 2.0;
        // Row 1's pair, to show the answer follows the ROW rather than being one global strip.
        let wave_y_row1 = wave_y + pitch;

        assert_eq!(geo.surface_at(picture_y), Surface::PictureRow(0), "the upper part is the picture row");
        assert_eq!(geo.surface_at(wave_y), Surface::WaveStrip, "the lower third is that row's wave strip");
        assert!(geo.names_ground(wave_y), "a wave-strip press is named ground, not blank gutter");
        assert_eq!(geo.surface_at(wave_y_row1), Surface::WaveStrip, "row 1 has one too");

        // The scope a drag from there gets, and the recording it names — off the ROW, never a placeholder.
        let cut_ = ui::review_cut_of(&window);
        let sound = cut_select_press::sound_scope(&geo, Surface::WaveStrip, &cut_, wave_y);
        let expected = cut_select_press::camera_for_row(&cut_, 0).expect("row 0 has a camera");
        assert_eq!(sound.as_deref(), Some(expected.as_str()), "row 0's strip names row 0's camera");
        assert_ne!(expected.as_str(), "", "and it is a real name, not an empty stand-in");
        let sound_row1 = cut_select_press::sound_scope(&geo, Surface::WaveStrip, &cut_, wave_y_row1);
        assert_eq!(
            sound_row1.as_deref(),
            cut_select_press::camera_for_row(&cut_, 1).as_deref(),
            "row 1's strip names row 1's camera, so two rows give two different answers"
        );
        assert_ne!(sound, sound_row1, "two cameras, two sounds");
        println!("f2_9 scope: picture_y -> PictureRow(0) [footage]; wave_y -> WaveStrip [{sound:?}], row1 [{sound_row1:?}]");

        // And the fork itself: the same rule ⧉ Copy applies to whatever scope the band produced.
        let footage_band = cp::copy(Some(&naivepost::cut_select::Selection {
            start: 10.0,
            end: 20.0,
            scope: Scope::Footage { row: 0 },
        }));
        let card = match footage_band {
            cp::Take::Taken(hand) => cp::spliced(&hand, 44.0),
            other => panic!("ten seconds of footage is copyable, got {other:?}"),
        };
        assert_eq!(card.ins, "copy:10.000", "footage pastes as a spliced copy card");
        assert_eq!((card.s, card.e), (44.0, 44.0), "spliced means s == e: the cut is OPENED there");
        assert!(card.lane.is_empty(), "a card names no lane");

        let sound_band = cp::copy(Some(&naivepost::cut_select::Selection {
            start: 10.0,
            end: 20.0,
            scope: Scope::Sound { recording: expected.clone() },
        }));
        let hand = match sound_band {
            cp::Take::Taken(hand) => hand,
            other => panic!("ten seconds of a recording is copyable, got {other:?}"),
        };
        let laid_cut = Cut { segs: vec![Seg { s: 0.0, e: 30.0, cam: 0, ..Default::default() }], ..Default::default() };
        let pieces = cp::lay_pieces(&laid_cut, "cam0.mkv", &hand, 12.0, 12.0);
        assert_eq!(pieces.len(), 1, "one kept stretch under the span, one piece");
        assert_eq!(pieces[0].lane, expected, "the piece stands in for the recording it was copied from");
        assert_eq!(pieces[0].ins, "cam0.mkv", "and names that file as its asset");
        assert!(pieces[0].s < pieces[0].e, "a piece covers seconds, unlike a spliced card");
        println!("f2_9 fork: footage -> {} at {}..{}; sound -> {} over {}..{}", card.ins, card.s, card.e, pieces[0].lane, pieces[0].s, pieces[0].e);
        RAN_BAND_SCOPE.store(true, Ordering::SeqCst);
        RAN_FORK.store(true, Ordering::SeqCst);
        out.borrow_mut().replace(app.clone());
        app.quit();
    });
    app.run_with_args::<String>(&[]);
    assert!(RAN_BAND_SCOPE.load(Ordering::SeqCst), "the band-scope check ran");
    assert!(RAN_FORK.load(Ordering::SeqCst), "the fork check ran");
}

#[test]
/// S2's "the strip paints the PASTE, not `footage_stretches`": a laid sound piece is a real clip that names
/// the recording it stands in for and starts mid-file (`ss`) — what a stand-in built from kept footage could
/// never produce, since a stand-in has no file and no offset.
fn s2_the_strip_draws_the_pieces_the_paste_laid() {
    let filmed = vec![Recording { base: "mic".to_string(), start: 30.0, end: 90.0 }];
    // A hole in the kept footage under the span: the paste covers two stretches, so two pieces arrive.
    let laid = Cut {
        segs: vec![
            Seg { s: 40.0, e: 46.0, cam: 0, ..Default::default() },
            Seg { s: 46.0, e: 52.0, cam: 0, ..Default::default() },
        ],
        ..Default::default()
    };
    let hand = Hand { from: 40.0, length: 12.0, scope: Scope::Sound { recording: "mic".to_string() } };
    let file_seconds = cp::file_seconds(&laid, &filmed, &hand);
    assert_eq!(file_seconds, 10.0, "session 40 on a tape opening at 0:30 is the tape's tenth second");
    let pieces = cp::lay_pieces(&laid, "project:mic.mkv", &hand, 40.0, file_seconds);
    assert_eq!(pieces.len(), 2, "both stretches under the span got a piece");
    for (i, piece) in pieces.iter().enumerate() {
        assert_eq!(piece.lane, "mic", "piece {i} stands in for the recording it was copied from");
        assert_eq!(piece.ins, "project:mic.mkv", "and names the FILE a stand-in would have left blank");
        assert!(piece.s < piece.e, "piece {i} covers seconds");
    }
    // The offset moves WITH the piece: the second stretch resumes mid-file, past the hole.
    assert_eq!(pieces[0].ss, 10.0, "the first piece opens at the converted file second");
    assert_eq!(pieces[1].ss, 16.0, "the next resumes six file-seconds in");
    println!(
        "f2_9 pieces: {} of them, ss {:?} (lane {:?}, file {:?})",
        pieces.len(),
        pieces.iter().map(|p| p.ss).collect::<Vec<_>>(),
        pieces[0].lane,
        pieces[0].ins
    );
    // Nothing under the span means nothing painted: the honest zero, not a full-width stand-in.
    let bare = Cut::default();
    assert!(cp::lay_pieces(&bare, "project:mic.mkv", &hand, 40.0, file_seconds).is_empty(), "no picture, no piece");
    RAN_PIECES_NOT_STAND_IN.store(true, Ordering::SeqCst);
}

/// Stage a temp root as the cwd: `save_insert_cut` writes through `std::env::current_dir()`, so the reopen
/// check pins it here and reads the same file back (the idiom tests/f2_7_verbs_live.rs uses).
fn stage(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("naivepost-f29-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(session.join("cut")).expect("a session cut folder");
    std::fs::create_dir_all(session.join("prepare")).expect("a session prepare folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    root
}

/// S3's "the row is a window on the FILE": ⇲ Lane files a row whose `off` is the converted FILE second, and
/// it survives a reopen — §B's four keys, with `off` the one that may be dropped and here must not be.
fn s3_the_lane_windows_the_file_and_reopens_with_it() {
    let root = stage("laneoff");
    let app = adw::Application::builder()
        .application_id("org.gtk.naivepost.test.f29laneoff")
        .build();
    let done = std::cell::RefCell::new(None);
    let out = done.clone();
    app.connect_activate(move |app| {
        let dir = fixture_dir();
        let model = naivepost::project::load(&dir).expect("fixture loads");
        let window = ui::build_window(app, &model, "Prepare");
        window.present();
        ui::tab_button(&window, Page::Cut)
            .expect("the shell has a Cut tab")
            .emit_clicked();
        let seeded = Cut {
            segs: vec![Seg { s: 0.0, e: 60.0, cam: 0, ..Default::default() }],
            ..Default::default()
        };
        ui::seed_review_cut(&window, &seeded);
        // A recording that does NOT open at 0:00 — the only case where `off` can be told from 0.
        ui::set_session_recordings(&[Recording { base: "mic".to_string(), start: 30.0, end: 90.0 }]);
        settle();

        ui::draw_selection(&window, Surface::WaveStrip, Some("mic"), 40.0, 46.0);
        let said = ui::press_copy(&window);
        assert!(said.starts_with("copied "), "\u{29c9} Copy took the lane band: {said}");
        // Inside a filmed run, so `lane_start` accepts the second.
        ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 44.0 });
        let status = ui::press_lane(&window);
        println!("f2_9 lane press: {status}");

        let lanes = ui::review_lanes(&window);
        let fresh = lanes.last().expect("the new row");
        assert_eq!(fresh.at, 44.0, "the row starts at the line");
        assert_eq!(fresh.dur, 6.0, "and runs the copied length");
        assert_eq!(fresh.off, 10.0, "the row windows the FILE from its tenth second, not from 0");
        assert!(fresh.src.contains("mic"), "and names its file: {:?}", fresh.src);

        // And it is on disk: re-read the written cut.json rather than trusting the in-memory mirror.
        let written = std::fs::read_to_string(
            naivepost::layout::Tree::new(naivepost::startup::session_dir(&root))
                .expect("a project")
                .cut_json(),
        )
        .expect("the lane press wrote cut.json");
        let back: Cut = serde_json::from_str(&written).expect("cut.json parses");
        let reopened = back.lanes.iter().find(|lane| lane.name == fresh.name).expect("the row is in the file");
        assert_eq!(reopened.off, 10.0, "`off` survived the write — a dropped `off` would reopen as 0");
        assert_eq!((reopened.at, reopened.dur), (44.0, 6.0), "and so did `at` and `dur`");
        println!("f2_9 lane after reopen: off {} at {} dur {} src {:?}", reopened.off, reopened.at, reopened.dur, reopened.src);
        RAN_LANE_OFF_PERSISTS.store(true, Ordering::SeqCst);
        out.borrow_mut().replace(app.clone());
        app.quit();
    });
    app.run_with_args::<String>(&[]);
    assert!(RAN_LANE_OFF_PERSISTS.load(Ordering::SeqCst), "the lane-offset check ran");
}

#[test]
fn f2_9_s1_s2_s3_the_scope_is_aimable_and_the_source_is_asked() {
    let _ = fixture_dir();
    s2_the_paste_asks_the_source_for_the_file_second_under_it();
    s3_a_lane_starts_where_the_session_actually_filmed();
    s2_the_strip_draws_the_pieces_the_paste_laid();
    s1_a_picture_row_is_footage_and_a_wave_strip_names_that_rows_recording();
    s3_the_lane_windows_the_file_and_reopens_with_it();
    let _ = cut::Cut::default();
}
