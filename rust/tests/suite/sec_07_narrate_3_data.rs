//! §07-narrate#3 Data (spec/07-narrate.md) — the files in `narrate/` that a page press is supposed to
//! leave behind. `narration.json` and its previous generation have their own rounds; this file pins the
//! two one-line settings and `takes.json`, each through the module that owns the format
//! ([`naivepost::narrate_data`]) rather than through a second writer.
//!
//! Headless: these are file rules, so no widget is built here. The wire test that presses the slider and
//! the take buttons lives in the same file under the item's name once the doors reach these writers.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::narrate_data::{self, Take};
use naivepost::narrate_screen;
use naivepost::narration::{self, Entry, Narration};
use naivepost::ui;

use crate::common;
use common::{settle, status_text, tree_in};

/// A minimal 16-bit PCM wav: the header `voice_ref::wav_header` reads, then `frames` of silence.
fn wav_bytes(rate: u32, channels: u16, frames: u32) -> Vec<u8> {
    let data = frames * channels as u32 * 2;
    let mut out: Vec<u8> = Vec::new();
    let mut push = |bytes: &[u8]| out.extend_from_slice(bytes);
    push(b"RIFF");
    push(&(36 + data).to_le_bytes());
    push(b"WAVEfmt ");
    push(&16u32.to_le_bytes());
    push(&1u16.to_le_bytes());
    push(&channels.to_le_bytes());
    push(&rate.to_le_bytes());
    push(&(rate * channels as u32 * 2).to_le_bytes());
    push(&(channels * 2).to_le_bytes());
    push(&16u16.to_le_bytes());
    push(b"data");
    push(&data.to_le_bytes());
    out.resize(44 + data as usize, 0);
    out
}

/// The band's live selection, put there the way a drag leaves it: this file is about what the ＋ button
/// WRITES, and the span itself is pinned in `tests/suite/voice_ref_wire_widgets.rs`, which drives the gesture.
fn put_selection(window: &adw::ApplicationWindow, span: (f64, f64)) {
    // The band maps a pixel offset to seconds through its own px/s, which comes from the recording's length;
    // a gesture here would be a second test of that mapping. The slope is what a drag's release stores, so
    // the same call a release makes is what stands in for one here.
    ui::narrate_band::set_selection_for_test(window, span);
}

/// One `adw::Application` and one main loop for the whole binary, as every widget test here has to be: a
/// second `adw::Application::run` in the same process aborts on the already-acquired default main context,
/// so all three scenarios ride this single activation (`narrate_off_tick_widgets.rs` has the same shape).
static RUN: std::sync::Once = std::sync::Once::new();
static DOORS_RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SEEDED_RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn shared_loop() {
    RUN.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_body);
        // Our own argv: handing cargo-test's flags to libgio aborts before activate runs.
        app.run_with_args(&["naivepost"]);
    });
}

fn run_body(app: &adw::Application) {
    // The two scenarios share the process's cwd, so they share one root; each writes the files it asserts on.
    let (tree, project) = session_project("data");
    doors_write_their_files(app, &tree, &project);
    stored_values_seed_the_page(app, &tree, &project);
    app.quit();
}

/// The project folder the page resolves as its session. The page reads
/// `startup::session_dir(current_dir())`, so the cwd is pinned here and the project written BEFORE any
/// window is built; a narrator source goes in the same file because `takes.json` is keyed by that recording.
/// Both scenarios share ONE root and one cwd on purpose: `set_current_dir` is process-wide, so the second
/// call would move the folder the first one's assertions still read. Everything the first round wrote is
/// cleared here so the second starts as clean as it claims to.
fn session_project(tag: &str) -> (naivepost::layout::Tree, naivepost::project::Project) {
    let root = std::env::temp_dir().join(format!("np-data-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned");
    // The page resolves its session as `startup::session_dir(cwd)`, so the tree the assertions read is THIS
    // folder rather than one named by hand: a mismatch here means the page writes somewhere and the test
    // reads an empty path and calls that a missing write.
    let dir = std::env::current_dir()
        .expect("cwd just pinned")
        .join(naivepost::startup::SESSION_NAME);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("session folder");
    let tree = naivepost::layout::Tree::new(&dir).expect("the folder is a project");
    let project = naivepost::project::Project {
        sources: vec![naivepost::project::Source {
            path: "project:sources/lecture.wav".into(),
            narrator: 1,
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json");
    // The narrator's recording is a real file: the take band shows a narrator's recordings, and a take is filed
    // under the recording it lies in.
    std::fs::create_dir_all(tree.dir().join("sources")).unwrap();
    naivepost::snapshot::write_silence_wav(
        &tree.dir().join("sources/lecture.wav"),
        8_000,
        8_000 * 45,
    )
    .expect("the recording");
    (tree, project)
}

/// The same pair from a folder `tree_in` already made: a narrator-tagged recording, so `takes.json` has a key
/// of its own and `stored_takes` can find it. Split from `session_project` because the headless cases need a
/// tree but no window, and `session_project` moves the process's cwd.
fn narrated_tree(tag: &str) -> naivepost::layout::Tree {
    let (_root, tree) = tree_in(tag);
    // A real 12 s recording: the band shows a narrator's recordings, so a file that is not there has no seconds
    // for a take to sit on.
    std::fs::create_dir_all(tree.dir().join("sources")).unwrap();
    naivepost::snapshot::write_silence_wav(
        &tree.dir().join("sources/lecture.wav"),
        16_000,
        16_000 * 12,
    )
    .expect("the recording");
    naivepost::project::save(
        &naivepost::project::Project {
            sources: vec![naivepost::project::Source {
                path: "project:sources/lecture.wav".into(),
                narrator: 1,
                footage: true,
                ..Default::default()
            }],
            ..Default::default()
        },
        tree.dir(),
    )
    .expect("naivepost.json with a narrator");
    tree
}

/// What a page build reads from `narrate/takes.json`: the narrator recording's own picks, and nobody else's.
/// The seed rule this file is about, pinned headless so it does not depend on a window's state surviving the
/// per-tab page rebuild.
#[test]
fn sec_07_narrate_3_data_stored_takes_come_from_the_recording_key() {
    let tree = narrated_tree("data-stored");
    assert!(
        ui::narrate_seed::stored_takes(&tree, naivepost::narrate_data::OWN_VOICE).is_empty(),
        "nothing picked yet, so the band opens bare"
    );
    narrate_data::save_takes(&tree, "lecture", &[Take { s: 4.0, e: 9.0 }]).expect("a pick stored");
    narrate_data::save_takes(&tree, "other_cam", &[Take { s: 0.0, e: 2.0 }])
        .expect("another recording's pick");
    assert_eq!(
        ui::narrate_seed::stored_takes(&tree, naivepost::narrate_data::OWN_VOICE),
        vec![(4.0, 9.0)],
        "the seed is this recording's list only"
    );
}

/// Open the shell on the Narrate tab, the way a person does: the tab's own clicked emission.
fn narrate_tab(
    app: &adw::Application,
    project: &naivepost::project::Project,
) -> adw::ApplicationWindow {
    let window = ui::build_window(app, project, "Prepare");
    window.present();
    ui::tab_button(&window, naivepost::shell::Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    window
}

fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if let Some(found) = walk(&current, name) {
                    return Some(found);
                }
                cursor = current.next_sibling();
            }
        }
        None
    }
    walk(&window.content().expect("the window has content"), name)
}

/// (a) the slider's own path writes `narrate/pitch.txt`; (b) ＋ over a live span writes `narrate/takes.json`
/// under the recording's key. Asserted through the FILES, which is what §3-data is about.
fn doors_write_their_files(
    app: &adw::Application,
    tree: &naivepost::layout::Tree,
    project: &naivepost::project::Project,
) {
    let window = narrate_tab(app, project);
    // The door a keyboard step reaches — the same one `narrate_surface_widgets.rs` block (8) drives. Not the
    // slider's own `value-changed`: with one page box built PER TAB, that wire belongs to whichever copy was
    // wired last and is never painted, and its PITCH_APPLY_MS debounce cannot be waited out from a test. The
    // clamp is still the rule being checked, so the file must hold P's maximum, not the number typed here.
    assert!(
        ui::read_state().pitch.abs() < 1e-9,
        "the round starts unshifted, or the pitch move below is not what changes the file"
    );
    let said = ui::press_pitch(&window, narrate_screen::PITCH_MAX_SEMITONES + 4.0);
    settle();
    assert!(
        !said.is_empty(),
        "the pitch door answers with the voice line, got {said:?}"
    );
    assert!(
        (narrate_data::read_pitch(tree) - narrate_screen::PITCH_MAX_SEMITONES).abs() < 1e-9,
        "moving the pitch must leave the clamped shift in narrate/pitch.txt, read {}; tree={}; cwd={}",
        narrate_data::read_pitch(tree),
        tree.pitch_txt().display(),
        std::env::current_dir().unwrap_or_default().display()
    );
    assert!(
        tree.pitch_txt().is_file(),
        "and the file the spec names is the one that appeared; the log says {:?}",
        naivepost::ui::window::logs_for_test()
    );

    // (b) The band's gesture is pinned elsewhere; here the span is put where a drag would leave it and ＋ is
    // pressed, which is the door §3-data says writes the file.
    put_selection(&window, (4.0, 9.0));
    // The band shows only over a recording it can measure (§A.7), and `take-add`'s own sentence is about the
    // span, so a readable wav header on the reference path is what makes the button exist at all. 44-byte
    // header + 1 s of 16-bit mono at 8 kHz: enough for `recording_seconds` to answer.
    let base = tree.voice_ref_base_wav();
    std::fs::create_dir_all(base.parent().expect("narrate/ has a parent")).expect("narrate/");
    std::fs::write(&base, wav_bytes(8000, 1, 8000)).expect("a reference wav the band can read");
    ui::narrate_band::show_only_with_recording(&window);
    ui::refresh(&window);
    settle();
    widget_in(&window, "take-add")
        .expect("the Narrate page carries take-add")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    // `note_takes` files the picks under the project's narrator recording, and says nothing when no narrator
    // is tagged — so the same press is silent in a project without one, which is the other half of the rule.
    let why = status_text(&window);
    assert!(
        tree.takes_json().is_file(),
        "＋ over a span must leave narrate/takes.json on disk; the page said: {why}"
    );
    let all = narrate_data::load_takes(tree).expect("takes.json reads");
    assert_eq!(
        all.get("lecture").map(|takes| takes.as_slice()),
        Some(&[Take { s: 4.0, e: 9.0 }][..]),
        "keyed by the narrator recording's own stem: {all:?}"
    );
    DOORS_RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    window.close();
    settle();
}

/// The seed: a voice id on disk must be what the freshly built page shows, in the state AND on the picker.
fn stored_values_seed_the_page(
    app: &adw::Application,
    tree: &naivepost::layout::Tree,
    project: &naivepost::project::Project,
) {
    // Written before the window is built, which is the whole point: the page reads these at build time.
    narrate_data::write_voice(tree, "narrator1").expect("a voice id on disk");
    narrate_data::write_pitch(tree, -2.5).expect("a shift on disk");
    let window = narrate_tab(app, project);
    let state = ui::read_state();
    assert_eq!(
        state.voice, "narrator1",
        "the page opens on the voice the project saved, not on the default row"
    );
    assert!(
        (state.pitch - (-2.5)).abs() < 1e-9,
        "and on the shift narrate/pitch.txt holds, read {}",
        state.pitch
    );
    // The picker can only SHOW a voice once a narrator is published: its rows are
    // `voice_options(state.narrators, &state.voice_files)`, and a state that names no narrator has the
    // captions-only row alone. Publishing the count this project really has is the flow's job, not the page
    // build's — so the face check has to follow a publish, while the assertions above (taken straight after
    // `build`) are the seed this item was sent back for.
    let mut st = ui::read_state();
    st.narrators = 1;
    ui::set_state(st);
    ui::refresh(&window);
    settle();
    let picker = widget_in(&window, "voice-picker")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        .expect("the Narrate page carries voice-picker");
    // Row order is `voice_options(1, [])`'s, spelled by the rules module rather than counted here.
    let rows = naivepost::narrate_screen::voice_options(1, &[]);
    let want = rows
        .iter()
        .position(|option| option.id == "own")
        .expect("narrator 1 (`own`) is a row of the picker");
    assert_eq!(
        picker.selected() as usize,
        want,
        "the picker shows the saved voice as its face"
    );
    let scale = widget_in(&window, "pitch-slider")
        .and_then(|w| w.downcast::<gtk::Scale>().ok())
        .expect("the Narrate page carries pitch-slider");
    assert!(
        (scale.value() - (-2.5)).abs() < 1e-9,
        "and the slider stands at the stored shift, read {}",
        scale.value()
    );
    // (c) The same door the slider uses, on a project whose narrator is tagged: the take lands under that
    // recording's stem, which is the key §3-data says survives a re-tag. The band needs its reference here too.
    // NOTE the span starts at 1.0 but `add_takes` MERGES it with the 4–9 pick block (b) left in the state —
    // overlapping spans are one take, so the stored end is 9.0 and the file holds ONE entry, not two.
    let base = tree.voice_ref_base_wav();
    std::fs::write(&base, wav_bytes(8000, 1, 8000)).expect("a reference wav the band can read");
    ui::narrate_band::show_only_with_recording(&window);
    put_selection(&window, (1.0, 8.0));
    widget_in(&window, "take-add")
        .expect("the Narrate page carries take-add")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let kept = narrate_data::load_takes(tree).expect("takes.json reads");
    assert_eq!(
        kept.get("lecture").map(|takes| takes.as_slice()),
        Some(&[Take { s: 1.0, e: 9.0 }][..]),
        "keyed by the narrator recording's own stem, merged with the earlier overlapping pick: {kept:?}"
    );
    SEEDED_RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    window.close();
    settle();
}

/// A line of a narration record: the clip's bounds and what is said over it.
fn entry(s: f64, e: f64, text: &str) -> Entry {
    Entry {
        s,
        e,
        at: 0.0,
        text: text.into(),
        emotion: String::new(),
        ..Default::default()
    }
}

/// sec_07_narrate_3_data_pitch_survives_a_reload — `narrate/pitch.txt` holds one decimal, and reading it
/// back is how the slider shows the shift a previous session left. A project that never touched the slider
/// has no file at all and reads 0.0, which is "no shift", not an error.
#[test]
fn sec_07_narrate_3_data_pitch_survives_a_reload() {
    let (_root, tree) = tree_in("data-pitch");
    assert!(
        !tree.pitch_txt().exists(),
        "a fresh session has no pitch.txt to read"
    );
    assert_eq!(
        narrate_data::read_pitch(&tree),
        0.0,
        "and the missing file answers the neutral shift"
    );

    narrate_data::write_pitch(&tree, -2.5).expect("the shift is written");
    assert!(tree.pitch_txt().is_file(), "narrate/pitch.txt is there");
    assert_eq!(
        std::fs::read_to_string(tree.pitch_txt()).expect("it reads"),
        "-2.5",
        "one decimal, the slider's own step and all a rubberband shift answers to"
    );
    assert_eq!(
        narrate_data::read_pitch(&tree),
        -2.5,
        "and the value comes back through the reader, which is what the next window seeds from"
    );

    // A rewrite replaces rather than appends: the file is one line, whatever the last value was.
    narrate_data::write_pitch(&tree, 1.0).expect("the new shift replaces the old");
    assert_eq!(narrate_data::read_pitch(&tree), 1.0);
}

/// sec_07_narrate_3_data_takes_are_keyed_by_the_recording — `narrate/takes.json` is keyed by the RECORDING's
/// stem, not by the narrator slot pointing at it, so re-tagging a source never hands one person's picks to
/// another. Writing one recording's list leaves every other key exactly as it was, and an empty list deletes
/// the key instead of storing a take of no seconds.
#[test]
fn sec_07_narrate_3_data_takes_are_keyed_by_the_recording() {
    let (_root, tree) = tree_in("data-takes");
    let file = tree.takes_json();
    assert!(!file.exists(), "a fresh session has no takes.json");
    assert!(
        narrate_data::load_takes(&tree)
            .expect("a missing file is nobody having picked anything")
            .is_empty(),
        "and it reads as an empty map"
    );

    // Two recordings, so "untouched" can be shown rather than assumed.
    narrate_data::save_takes(
        &tree,
        "lecture",
        &[Take { s: 4.0, e: 9.0 }, Take { s: 20.0, e: 26.0 }],
    )
    .expect("the first recording's picks are stored");
    narrate_data::save_takes(&tree, "stage_cam", &[Take { s: 1.0, e: 6.0 }])
        .expect("the second recording's picks are stored beside them");
    assert!(file.is_file(), "narrate/takes.json is there");

    let all = narrate_data::load_takes(&tree).expect("it reads back");
    assert_eq!(
        all.get("lecture").map(|takes| takes.as_slice()),
        Some(&[Take { s: 4.0, e: 9.0 }, Take { s: 20.0, e: 26.0 }][..]),
        "keyed by the recording's own stem: {all:?}"
    );
    assert_eq!(
        all.get("stage_cam").map(|takes| takes.len()),
        Some(1),
        "the other recording keeps its own single pick"
    );
    // The stem is what `take_base` answers for a path, which is the key the door uses.
    assert_eq!(
        narrate_data::take_base(std::path::Path::new("/media/lecture.mkv")),
        "lecture",
        "so a re-tagged slot still finds the same list"
    );

    // Removing everything drops the key: "no takes" and "never had any" are the same file.
    narrate_data::save_takes(&tree, "lecture", &[]).expect("an empty list deletes the key");
    let after = narrate_data::load_takes(&tree).expect("still readable");
    assert!(
        !after.contains_key("lecture"),
        "the emptied recording is gone from the file: {after:?}"
    );
    assert_eq!(
        after.get("stage_cam").map(|takes| takes.len()),
        Some(1),
        "and the untouched recording is still there"
    );
}

#[test]
fn sec_07_narrate_3_data_widget_doors_write_their_files() {
    shared_loop();
    assert!(
        DOORS_RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the slider-and-take block never ran"
    );
}

/// The fourth gap this item was sent back for: `narrate/voice.txt` was written by `press_voice` but never
/// read back, so a project that had chosen a voice opened on the default row. Same loop, second scenario.
#[test]
fn sec_07_narrate_3_data_stored_values_seed_the_page() {
    shared_loop();
    assert!(
        SEEDED_RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the seeding block never ran"
    );
}

/// sec_07_narrate_3_data_previous_kept_only_after_a_valid_reply — `narration.prev.json` is the PREVIOUS
/// generation, so it may only be copied once a valid reply is in hand. A call that fails (or answers nothing)
/// must leave both files exactly as they were: throwing away the good record on the way to producing nothing
/// is the one thing a one-deep backup cannot do.
#[test]
fn sec_07_narrate_3_data_previous_kept_only_after_a_valid_reply() {
    let (_root, tree) = tree_in("data-prev");
    // What was there before this run: one line, which is what the copy has to preserve.
    let before = Narration {
        entries: vec![entry(0.0, 10.0, "the OLD first line")],
        silent: vec![],
    };
    narration::save(&before, &tree).expect("a record exists before the run");
    assert!(
        narrate_data::keep_previous(&tree).expect("copying an existing record works"),
        "and the copy reports it made one"
    );

    // The rule the run follows, stated where the code can be pointed at it: the copy happens between the
    // valid reply and the save (`src/ui/window_narrate.rs`, S4), never on the way in.
    assert_eq!(
        narrate_data::previous(&tree).map(|kept| kept.entries.len()),
        Some(1),
        "the kept generation still holds the OLD record"
    );
    assert_eq!(
        narration::load(&tree).expect("the current file reads"),
        before,
        "and the current file is untouched until a reply arrives"
    );
    assert!(
        keep_sits_between_reply_and_save(),
        "the copy must happen after a valid reply and before the save, not on the way in"
    );
}

/// The order the run writes its two narration files in, read out of the source rather than guessed at:
/// `keep_previous` must sit AFTER the reply's success checks and BEFORE `narration::save`. Copying on the way
/// in is what this item was sent back for, and a comment saying so proves nothing — the line order does.
fn keep_sits_between_reply_and_save() -> bool {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/window_narrate.rs"),
    )
    .expect("the run's own source is readable from a test");
    let at = |needle: &str| src.find(needle).expect("the run still contains: {needle}");
    at("Ok(lines) => lines") < at("narrate_data::keep_previous(tree)")
        && at("narrate_data::keep_previous(tree)") < at("narration::save(&record, tree)")
}

/// sec_07_narrate_3_data_previous_reads_back_the_generation — what [`narrate_data::previous`] is for: a
/// damaged `narration.json` does not have to mean starting from no lines. `load_or_previous` answers the kept
/// generation and says it did; a MISSING current file is the opposite case and reads as an empty record.
/// [`narrate_data::previous`] is what reads that generation back, and `load_or_previous` is the door the run
/// goes through when the current file cannot be read at all.
#[test]
fn sec_07_narrate_3_data_previous_reads_back_the_generation() {
    let (_root, tree) = tree_in("data-prev-back");
    // Fresh session: no file at all is not damage, so no fallback and no sentence about one — `load` itself
    // answers an empty record for a missing file, which is what keeps this case apart from a broken one.
    assert!(
        narration::load(&tree).is_ok(),
        "an absent narration.json is a fresh session, not a broken one"
    );
    let (record, fell_back) = narrate_data::load_or_previous(&tree).expect("a fresh session reads");
    assert!(
        record.entries.is_empty() && !fell_back,
        "and it reads as no lines, without falling back"
    );

    // One good generation, then the current file is damaged by hand — the state a half-written save leaves.
    let kept = Narration {
        entries: vec![entry(0.0, 10.0, "the KEPT line")],
        silent: vec![],
    };
    narration::save(&kept, &tree).expect("a record exists");
    narrate_data::keep_previous(&tree).expect("it becomes the kept generation");
    std::fs::write(tree.narration_json(), b"{ this is not json")
        .expect("the current file is now garbage");

    assert!(
        narration::load(&tree).is_err(),
        "the current file genuinely fails to load -- that is the case the fallback is for"
    );
    let (record, fell_back) =
        narrate_data::load_or_previous(&tree).expect("the kept generation is readable");
    assert!(
        fell_back,
        "and the answer came from the fallback, which is what gets said out loud"
    );
    assert_eq!(
        record
            .entries
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        vec!["the KEPT line"],
        "so the run starts from the generation one back, not from nothing"
    );
    assert_eq!(
        narrate_data::previous(&tree).expect("and previous() itself reads the same file"),
        record,
        "one file, one reading of it"
    );
}
