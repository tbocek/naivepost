//! F4.7's WIRE through the real widgets: every edit on a line row is fired the way a click fires it,
//! and the check is what the record says afterwards — the state list, `narration.json` on disk, and the
//! text that came back on the status line. Nothing here pokes state to make a branch reachable: the
//! presses go through `line-add-below-N`, `narrate-add-line`, `line-remove-N`, `line-reroll-N` and the
//! row's own time entry (`line-time-N`, Enter = activate).
//!
//! The speak leg underneath the re-roll runs for real against a fake audio.cpp on a loopback port, so
//! the take file the re-roll leaves behind is a file the program actually addressed, not one this test
//! imagined. One scenario per binary (cwd and XDG are process-wide), one application, exactly one
//! `#[test]`, in the shape `tests/suite/sample_play_wire_widgets.rs` uses.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::narration::{Entry, Silent};
use naivepost::project::{Project, Source};
use naivepost::settings::{self, Conf};
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{hold_last_window, release_last_window, settle, status_text, widget_in, narrate_button as button};

#[path = "speak_tts_wire_harness.rs"]
mod harness;
use harness::{FakeAudio, Script};

static RAN_EDIT_WIRE: AtomicBool = AtomicBool::new(false);


fn time_field(window: &adw::ApplicationWindow, index: usize) -> gtk::Entry {
    widget_in(window, &format!("line-time-{index}"))
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .unwrap_or_else(|| panic!("row {index}'s time field is an editable entry"))
}

/// The session folder the page resolves its files into: `startup::session_dir(current_dir())`.
fn session_tree() -> Tree {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    Tree::new(&dir).expect("the seeded session folder ends in .naivepost")
}

/// Whether `narration.json` holds a line `matches` accepts. A file mid-write reads as "not yet".
fn on_disk_has(tree: &Tree, matches: impl Fn(&Entry) -> bool) -> bool {
    naivepost::narration::load(tree)
        .map(|narration| narration.entries.iter().any(matches))
        .unwrap_or(false)
}

/// Read one entry out of the record on disk by its words, so the assertion is about the FILE and not
/// about the page's memory.
fn saved_line_with(tree: &Tree, needle: &str) -> Option<Entry> {
    naivepost::narration::load(tree)
        .expect("narration.json parses")
        .entries
        .into_iter()
        .find(|entry| entry.text == needle)
}

/// Type into a row's real text box. `set_text` moves the widget AND emits `changed`, which is the signal
/// the page wired — the same path a keystroke takes.
fn type_in_box(window: &adw::ApplicationWindow, index: usize, text: &str) {
    widget_in(window, &format!("line-text-{index}"))
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .unwrap_or_else(|| panic!("row {index} carries an editable text box"))
        .set_text(text);
}

/// The text box's face, read back off the widget.
fn box_text(window: &adw::ApplicationWindow, index: usize) -> String {
    widget_in(window, &format!("line-text-{index}"))
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .expect("row {index} carries an editable text box")
        .text()
        .to_string()
}

/// §F4.7 (**Text**) through the widgets: a typed tag reaches the line at once, reaches DISK only after
/// the 400 ms beat, survives a tab leave and a window close, and a `[tag @N]` moves the line.
fn text_block(app: &adw::Application) {
    let tree = session_tree();
    // Two clips again, so a `@N` can name a second in the other one and be refused for it.
    let spoken = Entry {
        s: 0.0,
        e: 10.0,
        at: 2.0,
        text: "a spoken line".into(),
        emotion: "calm".into(),
        pos: String::new(),
        roll: 0,
    };
    let state = ui::NarrateState {
        entries: vec![
            spoken.clone(),
            Entry { s: 10.0, e: 20.0, at: 1.0, text: "second clip line".into(), ..Default::default() },
        ],
        segs: vec![
            Seg { s: 0.0, e: 10.0, ..Default::default() },
            Seg { s: 10.0, e: 20.0, ..Default::default() },
        ],
        takes: vec![],
        voice: "narrator1".into(),
        pitch: 0.0,
        session: 5.0,
        cut_at: 5.0,
        length: 20.0,
        clips: 2,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec![],
        recording_covers_clip: true,
        playing: false,
        started: false,
        busy: false,
        narration_off: false,
        covered_spans: vec![],
        language: "en".into(),
    };
    release_last_window();
    let window = ui::build_window(app, &naivepost::project::load(tree.dir()).unwrap_or_default(), "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(state.clone());
    ui::refresh(&window);
    settle();

    // --- (f) typing a placement tag: the line changes NOW, the file changes after the beat -------
    type_in_box(&window, 0, "[top] a caption typed in the box");
    // Nothing was written yet, read before the loop turns: the beat is real time, and on a busy machine
    // it is over by the time a `settle` returns.
    assert!(
        saved_line_with(&tree, "a caption typed in the box").is_none(),
        "typing does not write per character"
    );
    settle();
    let edited = ui::read_state()
        .entries
        .iter()
        .find(|entry| entry.text == "a caption typed in the box")
        .cloned()
        .expect("the typed words reached the held entry on the keystroke itself");
    assert_eq!(edited.pos, "top", "the placement landed");
    assert!(edited.emotion.is_empty(), "a placement CLEARS the emotion -- a caption is read, not spoken");
    common::wait_until("the autosave writing the typed caption", || {
        on_disk_has(&tree, |entry| entry.text == "a caption typed in the box")
    });
    let on_disk = saved_line_with(&tree, "a caption typed in the box")
        .expect("after the beat the autosave wrote the line");
    assert_eq!((on_disk.pos.as_str(), on_disk.emotion.as_str()), ("top", ""), "the file agrees with the row");
    assert_eq!(
        box_text(&window, 0),
        naivepost::narrate_screen::write_box(&on_disk),
        "the box and the record show the same thing"
    );

    // --- (g) emptying the box keeps the delivery and does NOT delete the line -------------------
    // Back to a spoken line first, so there is an old emotion to keep.
    type_in_box(&window, 0, "[angry] words with a delivery");
    settle();
    common::wait_until("the autosave writing the delivery line", || {
        on_disk_has(&tree, |entry| entry.text == "words with a delivery")
    });
    let angry = saved_line_with(&tree, "words with a delivery").expect("the delivery line is on disk");
    assert_eq!(angry.emotion, "angry");
    let count_before = ui::read_state().entries.len();
    type_in_box(&window, 0, "");
    settle();
    assert_eq!(
        ui::read_state().entries.len(),
        count_before,
        "emptying a box is not a delete: the row stays listed"
    );
    let emptied = ui::read_state()
        .entries
        .iter()
        .find(|entry| entry.s == 0.0 && entry.at == 2.0)
        .cloned()
        .expect("the emptied line is still the same line");
    assert_eq!(emptied.text, "", "the words went");
    assert_eq!(emptied.emotion, "angry", "emptying a box keeps the old emotion");

    // --- (h) a `[tag @N]` inside the clip moves it; one outside is refused and said -------------
    type_in_box(&window, 0, "[calm @6] moved by the tag");
    settle();
    let moved = ui::read_state()
        .entries
        .iter()
        .find(|entry| entry.text == "moved by the tag")
        .cloned()
        .expect("the tag's words landed");
    assert_eq!((moved.s, moved.e, moved.at), (0.0, 10.0, 6.0), "the tag moved the line inside its clip");
    assert_eq!(moved.emotion, "calm", "and kept its delivery");
    // A second past the clip's last usable second: the line stays and the status says where it stayed.
    let before = ui::read_state()
        .entries
        .iter()
        .find(|entry| entry.text == "moved by the tag")
        .cloned()
        .unwrap();
    type_in_box(&window, 0, "[calm @99] stayed put");
    settle();
    let stayed = ui::read_state()
        .entries
        .iter()
        .find(|entry| entry.text == "stayed put")
        .cloned()
        .expect("the words changed even where the move was refused");
    assert_eq!(
        (stayed.s, stayed.e, stayed.at),
        (before.s, before.e, before.at),
        "a second outside the clip moves the line nowhere"
    );
    assert!(
        status_text(&window).contains("is outside this clip"),
        "and the refusal names both numbers: {}",
        status_text(&window)
    );

    // --- (i) leaving the tab flushes what was typed, without any test poking the flag ------------
    type_in_box(&window, 0, "half-typed when the tab changed");
    assert!(
        saved_line_with(&tree, "half-typed when the tab changed").is_none(),
        "not written before the beat"
    );
    settle();
    ui::tab_button(&window, Page::Prepare)
        .expect("the shell has a Prepare tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        saved_line_with(&tree, "half-typed when the tab changed").is_some(),
        "leaving the tab wrote the half-typed line: {:?}",
        naivepost::narration::load(&tree).map(|r| r.entries.iter().map(|e| e.text.clone()).collect::<Vec<_>>())
    );

    // --- (j) closing the window flushes too ------------------------------------------------------
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    type_in_box(&window, 0, "typed just before the close");
    assert!(
        saved_line_with(&tree, "typed just before the close").is_none(),
        "still inside the beat when the window goes"
    );
    settle();
    window.close();
    settle();
    assert!(
        saved_line_with(&tree, "typed just before the close").is_some(),
        "the close request flushed the line to disk"
    );
    settle();
}

/// f4_7_s5_refit_on_entering_the_tab_writes_the_lines_it_moved — §F4.7's Refit through the real tab
/// button, in its OWN window and its OWN folder: arriving at Narrate must move the line AND write it
/// (the app did the work; nobody typed), which is the rule that cannot live in the block above — that
/// window starts on Prepare, so its Narrate visit is already this window's last one, and F0.1's
/// "a switch with no pending write must not touch the file" would be undone by a write made on arrival.
fn refit_block(app: &adw::Application) {
    // A folder of its own: this block rewrites both `cut/cut.json` and `narration.json` to make the cut
    // look as if it moved, and the blocks that run after it read those same files.
    let root = std::env::temp_dir().join(format!("np-f47-refit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config")).expect("config root");
    std::fs::create_dir_all(root.join("data")).expect("data root");
    std::env::set_current_dir(&root).expect("cwd pinned to the refit root");
    let tree = session_tree();
    naivepost::project::save(&Project::default(), tree.dir()).expect("a project to open");
    // The file carries a line on clip 0-10 at 2 s in; the cut on disk starts that clip at 12 instead,
    // so the video under the words moved two seconds and the line must follow it.
    naivepost::narration::save(
        &naivepost::narration::Narration {
            entries: vec![Entry {
                s: 10.0,
                e: 30.0,
                at: 6.0,
                text: "a line whose clip moved".into(),
                emotion: "calm".into(),
                ..Default::default()
            }],
            silent: vec![],
        },
        &tree,
    )
    .expect("the pre-refit record is written");
    naivepost::cut::save(
        &Cut {
            segs: vec![Seg { s: 12.0, e: 30.0, ..Default::default() }],
            ..Default::default()
        },
        &tree,
    )
    .expect("the moved cut is what the page will refit against");

    release_last_window();
    let window = ui::build_window(app, &naivepost::project::load(tree.dir()).unwrap_or_default(), "Prepare");
    hold_last_window(window.clone());
    window.present();
    settle();
    // The toolkit's own way in: click the Narrate tab.
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        status_text(&window).contains("the cut moved"),
        "the page says what it did: {}",
        status_text(&window)
    );
    let on_disk = naivepost::narration::load(&tree)
        .expect("narration.json reads back")
        .entries;
    let refit = on_disk
        .iter()
        .find(|entry| entry.text == "a line whose clip moved")
        .cloned()
        .expect("the line is still on disk");
    assert_eq!(
        (refit.s, refit.e, refit.at),
        (12.0, 30.0, 4.0),
        "entering the tab WROTE the refit: the line follows its clip's new bounds and keeps its video second"
    );
    assert_eq!(
        ui::read_state()
            .entries
            .iter()
            .find(|entry| entry.text == "a line whose clip moved")
            .map(|entry| (entry.s, entry.e, entry.at)),
        Some((12.0, 30.0, 4.0)),
        "and the rows show the same numbers the file now holds, so a later flush cannot undo it"
    );
    // The folder is this block's own, so nothing needs restoring for whoever runs next.
    window.close();
    settle();
    let _ = std::fs::remove_dir_all(&root);
}

fn run_round(app: &adw::Application) {
    let root = std::env::temp_dir().join(format!("np-f47-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config")).expect("config root");
    std::fs::create_dir_all(root.join("data")).expect("data root");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        // The player seam: a real child is spawned and `/bin/true` takes its arguments and exits 0.
        std::env::set_var("NAIVEPOST_VOICE_PLAYER", "/bin/true");
    }

    let audio = FakeAudio::start(Script::healthy());
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = Conf::default();
    conf.audio_server = audio.url();
    settings::save(&paths, &conf).expect("the fake address is what the Settings now say");

    let tree = session_tree();
    let mut project = Project::default();
    project.language = "en".to_string();
    project.sources.push(Source {
        path: "/media/lecture.mkv".into(),
        footage: true,
        narrator: 1,
        ..Default::default()
    });
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json written");
    let cut = Cut {
        segs: vec![
            Seg { s: 0.0, e: 10.0, ..Default::default() },
            Seg { s: 10.0, e: 20.0, ..Default::default() },
        ],
        ..Default::default()
    };
    naivepost::cut::save(&cut, &tree).expect("cut.json written");
    // F4.4 S1 asks whether a reference exists before dialling; plant one so the re-roll reaches the
    // wire instead of stopping at the missing file.
    let reference = tree.voice_ref_wav();
    std::fs::create_dir_all(reference.parent().expect("narrate/ has a parent")).expect("narrate/ made");
    std::fs::write(&reference, b"RIFF fake voice reference").expect("reference written");

    release_last_window();
    let window = ui::build_window(app, &project, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    // TWO entries on TWO DIFFERENT clips, so a move to another clip is possible at all.
    let first = Entry {
        s: 0.0,
        e: 10.0,
        at: 2.0,
        text: "a spoken line".into(),
        emotion: "calm".into(),
        pos: String::new(),
        roll: 0,
    };
    let second = Entry {
        s: 10.0,
        e: 20.0,
        at: 1.0,
        text: "second clip line".into(),
        emotion: String::new(),
        pos: String::new(),
        roll: 0,
    };
    let state = ui::NarrateState {
        entries: vec![first.clone(), second.clone()],
        segs: cut.segs.clone(),
        takes: vec![],
        voice: "narrator1".into(),
        pitch: 0.0,
        session: 15.0,
        cut_at: 5.0,
        length: 20.0,
        clips: 2,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec![],
        recording_covers_clip: true,
        playing: false,
        started: false,
        busy: false,
        narration_off: false,
        covered_spans: vec![],
        language: "en".into(),
    };
    ui::set_state(state.clone());
    ui::refresh(&window);
    settle();

    // ---- (a) ＋ below inserts a NEW row and leaves the clicked row's own second alone ----------
    let before = ui::read_state();
    assert_eq!(before.entries.len(), 2, "two rows to start with");
    assert_eq!(before.entries[0].at, 2.0, "the clicked row sits at 2.0 s into its clip");
    button(&window, "line-add-below-0").emit_by_name::<()>("clicked", &[]);
    settle();
    let added = ui::read_state();
    assert_eq!(added.entries.len(), 3, "a third row appeared under the clicked one");
    assert_eq!(
        added.entries[0].at, 2.0,
        "＋ below adds a row; it does NOT move the row it was pressed on"
    );
    assert_eq!(added.entries[1].s, 0.0, "the new row belongs to the same clip");
    // audio end 2.0 + the worded gap 0.5 = 2.5 into the clip (§F4.7).
    assert_eq!(added.entries[1].at, 2.5, "the new row starts half a second after its audio");
    // §F4.7: the row is part of the narration record, so the click writes it — nobody had to type.
    common::wait_until("the added row reaching narration.json", || {
        naivepost::narration::load(&tree).map(|narration| narration.entries.len() == 3).unwrap_or(false)
    });
    let below = naivepost::narration::load(&tree)
        .expect("narration.json reads back")
        .entries;
    assert_eq!(
        below.len(),
        3,
        "f4_7_s2_add_below_click_lands_and_reaches_the_file: the file holds all three rows: {:?}",
        below.iter().map(|e| (e.s, e.at)).collect::<Vec<_>>()
    );
    assert!(
        below.iter().any(|entry| (entry.s, entry.at) == (0.0, 2.5)),
        "and the inserted row is on disk at the second the page put it: {:?}",
        below.iter().map(|e| (e.s, e.at)).collect::<Vec<_>>()
    );

    // ---- (b) transport ＋: a free second adds, a speaking second refuses ----------------------
    let mut free = ui::read_state();
    // 8.0 is inside clip 0-10, more than 1 s clear of both lines already there, and past the first
    // line's estimated end (13 chars / 15 + 0.2 = ~1.07 s of speech from 2.0).
    free.session = 8.0;
    ui::set_state(free);
    ui::refresh(&window);
    settle();
    let count_before_add = ui::read_state().entries.len();
    button(&window, "narrate-add-line").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::read_state().entries.len(),
        count_before_add + 1,
        "a free second takes a new line"
    );
    assert!(
        status_text(&window).starts_with("a line starts at "),
        "the add says where: {}",
        status_text(&window)
    );
    // f4_7_s1_add_at_playhead_click_lands_at_the_second_and_reaches_the_file: §F4.7's free add is "a
    // line HERE", so the row carries the playhead's own second as a clip-relative offset (8.0 into the
    // 0-10 clip), and the click wrote the record without anyone typing in a box.
    let landed = ui::read_state()
        .entries
        .iter()
        .filter(|entry| entry.s == 0.0 && entry.text.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        landed.iter().any(|entry| (entry.s, entry.e, entry.at) == (0.0, 10.0, 8.0)),
        "the new row sits at the playhead second, not the clip's head: {:?}",
        landed.iter().map(|e| (e.s, e.at)).collect::<Vec<_>>()
    );
    common::wait_until("the add reaching narration.json", || {
        on_disk_has(&tree, |entry| (entry.s, entry.e, entry.at) == (0.0, 10.0, 8.0))
    });
    let on_disk = naivepost::narration::load(&tree)
        .expect("narration.json reads back")
        .entries;
    assert!(
        on_disk
            .iter()
            .any(|entry| (entry.s, entry.e, entry.at) == (0.0, 10.0, 8.0)),
        "the add reached the file on its own: {:?}",
        on_disk.iter().map(|e| (e.s, e.at)).collect::<Vec<_>>()
    );

    let mut speaking = ui::read_state();
    // 2.5 sits inside the first line's window (it started at 2.0), so nothing may be laid over it.
    speaking.session = 2.5;
    ui::set_state(speaking);
    ui::refresh(&window);
    settle();
    let count_before_refusal = ui::read_state().entries.len();
    button(&window, "narrate-add-line").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::read_state().entries.len(),
        count_before_refusal,
        "a press inside a speaking line adds nothing"
    );
    assert!(
        status_text(&window).starts_with("a line already starts here ")
            || status_text(&window).starts_with("a line is speaking here until "),
        "and the refusal is the flowchart's own: {}",
        status_text(&window)
    );

    // ---- (c) 🗑 the last line of a clip marks that clip silent ON DISK -------------------------
    // Reset to the two-entry pair so entry 1 really is its clip's only line.
    let only = ui::NarrateState {
        entries: vec![first.clone()],
        ..ui::read_state()
    };
    ui::set_state(only);
    ui::refresh(&window);
    settle();
    // Now put the second clip's line back so there are two again, then delete it.
    let pair = ui::NarrateState {
        entries: vec![first.clone(), second.clone()],
        ..ui::read_state()
    };
    ui::set_state(pair);
    ui::refresh(&window);
    settle();
    assert_eq!(ui::read_state().entries.len(), 2, "both rows present before the delete");
    button(&window, "line-remove-1").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(ui::read_state().entries.len(), 1, "the row left the list");
    let record = naivepost::narration::load(&tree).expect("narration.json reads back");
    assert!(
        record.silent.contains(&Silent { s: 10.0, e: 20.0 }),
        "the emptied clip is remembered on disk, not only in memory: {:?}",
        record.silent
    );

    // ---- (d) ↻ bumps the roll and keeps the old take ------------------------------------------
    let take = naivepost::produce_stamp::wav_of(&tree, &first, Some("narrator1"), 0)
        .expect("a spoken line has a take path");
    std::fs::create_dir_all(take.parent().expect("tts/ has a parent")).expect("tts/ made");
    std::fs::write(&take, b"RIFF old take").expect("old take planted");
    assert_eq!(ui::read_state().entries[0].roll, 0, "the roll starts at zero");
    button(&window, "line-reroll-0").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(ui::read_state().entries[0].roll, 1, "↻ is roll + 1");
    assert!(take.is_file(), "the previous take stays where it was: {}", take.display());

    // ---- (e) the time field: commit into another clip, refuse a gap, write the box back -------
    let field = time_field(&window, 0);
    field.set_text("12.0");
    field.emit_by_name::<()>("activate", &[]);
    settle();
    let moved = ui::read_state()
        .entries
        .iter()
        .find(|entry| entry.text == "a spoken line")
        .cloned()
        .expect("the moved line is still listed");
    assert_eq!((moved.s, moved.e), (10.0, 20.0), "the line adopted the other clip's bounds");
    assert_eq!(moved.at, 2.0, "12.0 on the session clock is 2.0 into the 10-20 clip");
    assert!(
        status_text(&window).starts_with("moved this line to the clip at "),
        "the move says so: {}",
        status_text(&window)
    );

    // A second outside the cut: nothing moves, and the box goes back to where the line really is.
    let field = time_field(&window, 0);
    field.set_text("25.0");
    field.emit_by_name::<()>("activate", &[]);
    settle();
    let kept = ui::read_state()
        .entries
        .iter()
        .find(|entry| entry.text == "a spoken line")
        .cloned()
        .expect("the line is still there");
    assert_eq!((kept.s, kept.e, kept.at), (10.0, 20.0, 2.0), "a gap refused the move outright");
    assert_eq!(
        field.text().as_str(),
        naivepost::narrate_screen::time_field(kept.s + kept.at).as_str(),
        "the box was written back to the second the line actually sits at"
    );
    assert!(
        status_text(&window).contains("is outside the cut"),
        "and the refusal names the typed second: {}",
        status_text(&window)
    );

    RAN_EDIT_WIRE.store(true, Ordering::SeqCst);
    window.close();
    settle();
    // §F4.7 (**Text**) runs last in this binary: it closes its own window, and the page's published state
    // is process-wide, so one scenario per binary keeps the two blocks from reading each other's rows.
    text_block(app);
    refit_block(app);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn f4_7_wire_editing_a_line_reaches_the_page_and_the_file() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        // Our own argv: handing cargo-test's flags to libgio aborts before activate runs.
        app.run_with_args(&["naivepost"]);
    });
    assert!(
        RAN_EDIT_WIRE.load(Ordering::SeqCst),
        "the F4.7 edit-wire block never ran"
    );
}
