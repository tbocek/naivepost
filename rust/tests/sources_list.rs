//! §03-shell.md **§4 Sources list (lives on Prepare, specified here because the shell snapshots it)** —
//! the item's ledger id is
//! `sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it`,
//! which every test below carries.
//!
//! The reason the list is specified with the shell is that ▶ freezes these rows as the session a run
//! works through (F0.2 S3), so the checks care about what holds between two presses and across a load,
//! not only at load. `naivepost::sources` holds the rules; the page draws them (spec/00-principles.md §5).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::{Project, Source};
use naivepost::sources::{self, Control, TrackInfo};
use naivepost::ui;

/// The item's id, interpolated into every assertion message so a failure names the item.
const ITEM: &str = "sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it";

fn row(path: &str, footage: bool, narrator: u32) -> Source {
    Source { path: path.into(), footage, narrator, ..Default::default() }
}

/// A camera recording with two voices, a separate mic, and a screen capture: three rows covering video,
/// audio and an untagged row. Every check takes its own copy so one rule cannot leak into the next.
fn session() -> Project {
    Project {
        sources: vec![
            row("project:sources/cam_2026-08-08_19-55-15.mkv", true, 1),
            row("card/mic.wav", false, 0),
            row("/media/screen.mkv", false, 0),
        ],
        ..Default::default()
    }
}

/// §4's control set per row: 🎥 only for a file with frames, the track menu only once there are ≥ 2
/// audio streams to pick between, and always a narrator slot, a name and a way off the list.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_rows_show_footage_and_tracks_by_file_kind() {
    let cam = &session().sources[0];
    assert_eq!(
        sources::row_controls(cam, 3),
        vec![
            Control::Footage,
            Control::Narrator,
            Control::Name,
            Control::Tracks,
            Control::Separate,
            Control::Remove,
        ],
        "{ITEM}: a multi-stream video gets §4's whole row"
    );
    assert_eq!(
        sources::row_controls(&session().sources[1], 1),
        vec![Control::Narrator, Control::Name, Control::Warning, Control::Separate, Control::Remove],
        "{ITEM}: an audio file has no frames, so no footage control at all"
    );
    // A name with a stamp says where it starts, so there is nothing to warn about; the mic has no
    // stamp and carries §4's ⚠.
    let stamped = row("cam_2026-08-08_19-55-15.mkv", true, 1);
    assert!(!sources::row_controls(&stamped, 3).contains(&Control::Warning));
    // One stream is nothing to choose between, so the menu is absent rather than showing `1/1`.
    assert!(!sources::row_controls(cam, 1).contains(&Control::Tracks));
    assert_eq!(sources::tracks_face(cam, 1), None, "{ITEM}: a one-stream row shows no face");
}

/// §4 dims the track button "while any track is out"; a dimmed menu over audio that is not there yet is
/// no menu, so `controls` drops it rather than offering a dead click.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_a_track_button_is_off_while_audio_is_out() {
    let cam = &session().sources[0];
    assert!(sources::controls(cam, 3, false).contains(&Control::Tracks));
    assert!(!sources::controls(cam, 3, true).contains(&Control::Tracks), "{ITEM}: dimmed means not offered");
    assert!(sources::tracks_dimmed(true) && !sources::tracks_dimmed(false));
}

/// §4's badge **5** is the file name: what the row is called, without turning into a path.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_a_row_is_named_by_its_file() {
    // The name is the file, whole: a stamp is what §4 places the file by, and two files differing only
    // by extension are two files, so neither part is dropped.
    for (path, want) in [
        ("project:sources/cam_2026-08-08_19-55-15.mkv", "cam_2026-08-08_19-55-15.mkv"),
        ("card/mic.wav", "mic.wav"),
        ("/media/screen.mkv", "screen.mkv"),
        ("take.2", "take.2"),
        ("no_extension", "no_extension"),
    ] {
        assert_eq!(sources::row_name(path), want, "{ITEM}: row name of `{path}`");
    }
    // The last part of a stored path whichever of the one path rule's spellings it uses (01 §2).
    assert_eq!(sources::row_name("project:sources/lecture.mkv"), "lecture.mkv");
}

/// §4's 🎥 toggle refuses a file with no frames: only a video may be footage.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_footage_is_a_videos_to_give() {
    let mut project = session();
    assert!(sources::toggle_footage(&mut project, 2, true));
    assert!(project.sources[2].footage);
    // Setting the same value again still reports a row that may be footage — the rule is about the
    // file's kind, not about whether anything moved.
    assert!(sources::toggle_footage(&mut project, 2, true));
    assert!(!sources::toggle_footage(&mut project, 1, true), "{ITEM}: the mic has no frames");
    assert!(!project.sources[1].footage);
    assert!(!sources::toggle_footage(&mut project, 9, true), "{ITEM}: no such row");
}

/// §4's 🎤 button cycles free slots 1..N then none, and nobody is moved to make room.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_the_narrator_button_walks_the_free_slots() {
    let mut project = session();
    let mut seen = vec![];
    for _ in 0..5 {
        seen.push(sources::cycle_narrator(&mut project, 1));
    }
    // Slot 1 is re-offered on the second press because a cycle that vacates it hands it over again —
    // see the next check for who gets it.
    assert_eq!(seen, vec![2, 0, 2, 0, 2], "{ITEM}: free slots then none, never somebody else's slot");
    assert_eq!(project.sources[0].narrator, 1, "{ITEM}: the camera keeps slot 1");

    // With every slot taken there is nowhere to go, so the press lands on none.
    let mut held = Project {
        sources: (1..=4).map(|slot| row(&format!("v{slot}.wav"), false, slot)).collect::<Vec<_>>(),
        ..Default::default()
    };
    assert_eq!(sources::cycle_narrator(&mut held, 0), 0);
}

/// Slot 1 is the voice the narration is spoken in (F2.14), so §4 hands it to the first untagged row
/// whenever nothing holds it — recordings before footage.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_slot_one_is_never_left_unheld() {
    let mut project = session();
    assert_eq!(sources::slot_holder(&project, 1), Some(0));
    // The camera's press vacates slot 1; the mic is next and it is a recording.
    sources::cycle_narrator(&mut project, 0);
    assert_eq!(sources::slot_holder(&project, 1), Some(1), "{ITEM}: the mic took the vacated slot");
    assert_eq!(sources::narrator_of(&project, 1), Some("card/mic.wav"));

    // A row that already has a voice is never demoted to fill it, and the vacating press leaves slot 1
    // with whoever else can hold it — here, the row that still does.
    let mut held = Project {
        sources: vec![row("cam.mkv", true, 1), row("mic.wav", false, 2)],
        ..Default::default()
    };
    sources::cycle_narrator(&mut held, 0);
    assert_eq!(sources::slot_holder(&held, 1), Some(0), "{ITEM}: slot 1 went back to the camera");
    assert_eq!(held.sources[1].narrator, 2, "{ITEM}: the mic keeps slot 2");
}

/// §4's 🗑 takes the row off the list; rows below move up, an index off the end takes nothing, and the
/// file itself is left alone — which is why no assertion here needs a filesystem.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_removing_a_row_moves_the_others_up() {
    let mut project = session();
    assert_eq!(sources::remove(&mut project, 0).map(|gone| gone.path), Some("project:sources/cam_2026-08-08_19-55-15.mkv".into()));
    assert_eq!(project.sources[0].path, "card/mic.wav", "{ITEM}: the rows below moved up");
    assert_eq!(sources::slot_holder(&project, 1), Some(0), "{ITEM}: its holder went, so slot 1 moved on");
    assert!(sources::remove(&mut project, 9).is_none(), "{ITEM}: no such row");

    assert_eq!(sources::removal_status("project:sources/lecture.mkv", 0), "removed lecture.mkv -- 0 source(s) left");
}

/// §4's clash rule: two rows of one base name would both be written into `inputs/<base>`, so ▶ refuses
/// until one is renamed. The status line is short, the log names both paths.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_two_rows_of_one_name_refuse_the_run() {
    let clashing = Project {
        sources: vec![row("project:sources/clip.mkv", true, 0), row("/media/clip.mkv", false, 0)],
        ..Default::default()
    };
    assert_eq!(
        sources::clash(&clashing),
        Some(("project:sources/clip.mkv".into(), "/media/clip.mkv".into())),
        "{ITEM}: the folder is not part of the name"
    );
    assert_eq!(
        sources::clash_log("project:sources/clip.mkv", "/media/clip.mkv"),
        "!!! project:sources/clip.mkv and /media/clip.mkv are both inputs/clip -- rename one"
    );
    assert_eq!(
        sources::clash_status("project:sources/clip.mkv", "/media/clip.mkv"),
        "clip.mkv and clip.mkv have the same name \u{2014} rename one"
    );

    let distinct = Project {
        sources: vec![
            row("cam_2026-08-08_19-55-15.mkv", true, 0),
            row("cam_2026-08-09_08-00-00.mkv", true, 0),
        ],
        ..Default::default()
    };
    assert_eq!(sources::clash(&distinct), None, "{ITEM}: two stamps are two files");
}

/// §4's track checks: the last ticked track cannot be unticked (a row hearing nothing would leave Cut
/// with nothing to align), ticking is sorted into the file's own stream order, and each ticked track
/// becomes a lane of its own.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_tracks_are_ticked_and_united_into_lanes() {
    let mut project = Project {
        sources: vec![{
            let mut cam = row("cam.mkv", true, 1);
            cam.tracks = vec![1];
            cam
        }],
        ..Default::default()
    };

    assert!(!sources::tick_track(&mut project, 0, 1, false), "{ITEM}: the only tick stays");
    assert!(sources::tick_track(&mut project, 0, 3, true));
    assert!(sources::tick_track(&mut project, 0, 2, true));
    assert_eq!(project.sources[0].tracks, vec![1, 2, 3], "{ITEM}: ticks follow the file's stream order");
    assert!(sources::tick_track(&mut project, 0, 2, false), "{ITEM}: one of three can go");
    assert_eq!(project.sources[0].tracks, vec![1, 3]);

    assert_eq!(sources::tracks_face(&project.sources[0], 3).as_deref(), Some("2/3"));
    // Two ticks split the file; one tick is used as it stands, so it contributes no lanes.
    assert_eq!(sources::split_lanes(&project), ["cam-t1", "cam-t3"]);
    project.sources[0].tracks = vec![2];
    assert!(sources::split_lanes(&project).is_empty(), "{ITEM}: an unsplit file is one source");

    let info = TrackInfo { index: 2, title: "Room mic".into(), channels: 2 };
    assert_eq!(info.label(), "Track 2 \u{2014} Room mic (stereo)");
    assert_eq!(TrackInfo { index: 1, title: String::new(), channels: 1 }.label(), "Track 1 \u{2014}  (mono)");
}

/// §4's ⚠ is a file whose name carries no timestamp: such a file starts where the session does.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_a_name_without_a_timestamp_is_warned_about() {
    assert_eq!(sources::warning(&row("mic.wav", false, 0)), Some(sources::WARN_TIP));
    assert_eq!(sources::warning(&row("cam_2026-08-08_19-55-15.mkv", true, 1)), None);
}

/// §4's scissors are dead on a split product — that voice has already been taken off.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_a_split_product_has_no_voice_left_to_split() {
    for path in ["mic.split-voice.wav", "mic.split-novoice.mkv", "old.voice.wav", "old.novoice.wav"] {
        assert!(!sources::separable(&row(path, false, 0)), "{ITEM}: `{path}` is already split");
    }
    // A name merely containing a suffix is not the product of one.
    for path in ["mic.wav", "split-voice.mkv"] {
        assert!(sources::separable(&row(path, false, 0)), "{ITEM}: `{path}` still has its voice");
    }
}

/// §4's cleaning is not a refusal: a hand-edited project loads anyway and says what was changed. It
/// matters because ▶ snapshots whatever the list holds, so a contradiction has to be gone before a run
/// reads it. A list that loads clean keeps whatever the user left in slot 1.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_a_loaded_list_is_cleaned_and_says_what_it_changed() {
    let mut project = Project {
        sources: vec![
            row("a.wav", false, 2),
            row("b.wav", false, 2),
            row("mic.wav", true, 0),
            row("cam.mkv", true, 1),
        ],
        ..Default::default()
    };
    let mut report = Vec::new();
    sources::clean_on_load(&mut project, &mut report);

    assert_eq!(
        report,
        [
            "b.wav: slot 2 is taken -- untagged",
            "mic.wav: an audio file has no frames -- not footage"
        ],
        "{ITEM}: each clearing is reported"
    );
    assert_eq!(project.sources[1].narrator, 0);
    assert!(!project.sources[2].footage);
    // The rows that meant something are untouched.
    assert_eq!(project.sources[0].narrator, 2);
    assert_eq!(project.sources[3].narrator, 1);

    // A slot outside the range the narrator button can reach is stripped, and stripping one is what
    // lets §4's auto-fill seat a narrator (§4: "a load that stripped a bad tag").
    let mut wild = Project { sources: vec![row("z.wav", false, 9)], ..Default::default() };
    let mut lines = Vec::new();
    sources::clean_on_load(&mut wild, &mut lines);
    assert_eq!(lines, ["z.wav: slot 9 is taken -- untagged"]);
    assert_eq!(sources::slot_holder(&wild, 1), Some(0));

    // A clean list is not talked to, and its empty slot 1 stays empty.
    let mut quiet = session();
    quiet.sources[0].narrator = 0;
    let mut silence = Vec::new();
    sources::clean_on_load(&mut quiet, &mut silence);
    assert!(silence.is_empty(), "{ITEM}: a good list gets no report: {silence:?}");
    assert_eq!(sources::slot_holder(&quiet, 1), None, "{ITEM}: the user emptied slot 1");
}

/// §4's strings in the words the spec gives them (spec/03-shell.md §4).
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_the_strings_are_the_specs() {
    assert_eq!(
        sources::LIST_TIP,
        "Every file here is transcribed, and placed on the session clock by the timestamp in its name"
    );
    assert_eq!(sources::ADD_TIP, "Add recordings or footage \u{2014} several at once");
    assert_eq!(
        sources::COPY_TIP,
        "Ticked, an added file is copied into the project's sources/ folder, so the project holds \
         everything it needs. Unticked, the file is referenced where it is: nothing is duplicated, and \
         the session breaks if it moves."
    );
    assert_eq!(sources::TRACKS_TITLE, "Audio tracks in this file");
    assert_eq!(sources::REMOVE_TIP, "Remove from this session \u{2014} the file itself is left alone");
    // §4's key to a row's four symbols, and the rule about who carries it: the symbol controls end
    // their tooltip with it, while ⚠ — which explains one file, not the row — does not.
    assert_eq!(
        sources::ROW_KEY,
        "\n\nThe symbols on every row, in order:\n\
         \u{1f3a5} footage \u{2014} frames come out of it, and the cut is made of it\n\
         \u{1f3a4} narrator \u{2014} which of the voices this is; 1 speaks the narration\n\
         \u{2702} split the voice off \u{2014} \u{25b6} separates it into the voice and the rest\n\
         \u{1f5d1} remove \u{2014} off this list; the file itself is left alone"
    );
    assert!(!sources::WARN_TIP.contains("The symbols on every row"));

    let slot = |n| sources::slot_tip(n);
    assert_eq!(slot(0), "no narrator \u{2014} click to make this voice 1");
    assert!(slot(1).starts_with("the narration is spoken by this voice (1)"));
    assert_eq!(slot(3), "narrator slot 3 \u{2014} click back to no narrator");
}

/// One running GTK application for this binary: GTK's main loop may only be started by the thread that
/// runs it, so both widget checks happen in one `connect_activate`, each recording that it got there.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_the_row(app);
            RAN_ROW.store(true, Ordering::SeqCst);
            check_a_press(app);
            RAN_PRESS.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN_ROW: AtomicBool = AtomicBool::new(false);
static RAN_PRESS: AtomicBool = AtomicBool::new(false);

/// The widget the page named `name`. `naivepost::ui` exports one accessor for §4's list, in the same
/// shape as every other widget on the window (`rescan_button`, `play_button`, …) — see
/// [`ui::find_source_widget`].
fn widget<T: IsA<gtk::Widget>>(window: &adw::ApplicationWindow, name: &str) -> Option<T> {
    ui::find_source_widget(window, name)?.downcast().ok()
}

fn fixture() -> Project {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost");
    naivepost::project::load(&dir).expect("the fixture loads")
}

/// §4's list on the page: one row for the fixture's one source, named for its file, with the footage
/// control that file's kind earns — and nothing said until something is pressed.
fn check_the_row(app: &adw::Application) {
    let window = ui::build_window(app, &fixture(), "Prepare");
    window.present();

    let list: gtk::ListBox = widget(&window, "sources-list").expect("Prepare has a sources list");
    assert!(list.row_at_index(0).is_some(), "{ITEM}: the fixture's one source is one row");
    assert!(widget::<gtk::ListBoxRow>(&window, "source-row-1").is_none(), "{ITEM}: no row for a file nobody added");

    let name: gtk::Label = widget(&window, "source-name-0").expect("the row shows its name");
    assert_eq!(name.label().as_str(), "lecture.mkv", "{ITEM}: the row is named for its file, not its path");
    assert!(widget::<gtk::ToggleButton>(&window, "footage-0").is_some(), "{ITEM}: a video row has its footage toggle");
    assert_eq!(ui::state(&window).status, "", "{ITEM}: drawing the list says nothing");
}

/// A press on a row goes through §4's rules and reports there — the widget decides nothing.
fn check_a_press(app: &adw::Application) {
    let window = ui::build_window(app, &fixture(), "Prepare");
    window.present();

    let remove: gtk::Button = widget(&window, "remove-0").expect("every row can leave the list");
    remove.emit_clicked();
    // A removed row leaves its widgets behind — the page draws the list once and F0.9's live project
    // state is what will redraw it — so this asserts only what §4 promises at a press: which file left
    // the session, and that nothing is left in it.
    assert_eq!(
        ui::state(&window).status,
        sources::removal_status("project:sources/lecture.mkv", 0),
        "{ITEM}: the removal is reported"
    );
}

#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_the_page_draws_one_row_per_source() {
    window_round();
    assert!(RAN_ROW.load(Ordering::SeqCst), "{ITEM}: the row check never ran");
}

#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_a_press_on_a_row_is_reported() {
    window_round();
    assert!(RAN_PRESS.load(Ordering::SeqCst), "{ITEM}: the press check never ran");
}
