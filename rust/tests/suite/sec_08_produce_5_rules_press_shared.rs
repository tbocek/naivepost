//! §08-produce#5 Rules, bullet 2's cue half — driven by a press on the real ▶.
//!
//! `tests/suite/produce_rules.rs` and `tests/suite/produce_rules_translation.rs` pin the same rules over `plan`/`walk`
//! with a hand-built `Materials`. This file is the wire: every folder below is written by a click on
//! `play-button` through the page's own press path (`produce_page::press_produce_run` →
//! `produce_snapshot::render_half` → `produce_exec::run_render`), and every assertion is read off the DISK
//! that press wrote or off the program's own log lines. Nothing here hands the render a `Materials`.
//!
//! One press per PROCESS: `adw::Application::run` cannot be re-entered, and the spawner seam is thread-local.
//! So each check is its own test BINARY, four targets compiled from the four sibling files named
//! `sec_08_produce_5_rules_press_*.rs`, and the check each one runs is named by the `CASE` constant its own
//! file sets before including this one.
//!
//! The subprocess legs are the program's own seams, never a test-only door: the spawner goes in through
//! `produce_translate::set_spawn_for_test`, which `ui::produce_languages::spawn` consults for every command
//! (the same seam `sec_08_produce_3_data.rs` and `produce_tag_page.rs` use). The translation leg keeps its
//! real address, which points at no server, so a ticked language answers the way a down server answers.
//! No ffmpeg, no live server, no sleeping.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Which of §5's four press checks this binary runs: the including file sets it before `mod press;`.
use super::CASE;

use adw::prelude::*;
use naivepost::layout::Tree;
use naivepost::produce_details as details;
use naivepost::produce_render as render;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{
    fixture_dir, hold_last_window, release_last_window, session_tree_of as session, settle,
};

fn two_segs() -> Vec<naivepost::cut::Seg> {
    vec![
        naivepost::cut::Seg {
            s: 0.0,
            e: 9.5,
            cam: 0,
            ..Default::default()
        },
        naivepost::cut::Seg {
            s: 10.0,
            e: 30.0,
            cam: 0,
            ..Default::default()
        },
    ]
}

/// The cut the press finds on disk (§08 reads the Cut page's saved cut when no live review cut stands).
fn write_cut(tree: &Tree) {
    let cut = naivepost::cut::Cut {
        segs: two_segs(),
        aspect: "16:9".to_string(),
        ..Default::default()
    };
    std::fs::create_dir_all(tree.cut_json().parent().unwrap()).unwrap();
    std::fs::write(tree.cut_json(), serde_json::to_string_pretty(&cut).unwrap()).unwrap();
}

/// The session's own speech, one word per clip. A press with NOTHING to say is the same session with this
/// file absent: `word_list::load` answers `None`, which is what makes §5's refusal reachable.
fn write_words(tree: &Tree) {
    let one = |word: &str, start: f64| naivepost::word_list::Word {
        source: "/media/take.mp4".into(),
        match_word: word.into(),
        written: word.into(),
        start,
        end: start + 0.6,
        stray: false,
    };
    naivepost::word_list::save(tree, &[one("hello", 1.0), one("there", 11.0)])
        .expect("word list saved");
}

/// The spawner the walk is handed through the program's own seam: each command's output file appears on disk,
/// so the run's "did it land" checks pass as they do with a real encoder.
fn scripted_spawn() -> naivepost::produce_translate::SpawnSlot {
    std::sync::Arc::new(move |command: &naivepost::produce_exec::Command| {
        // `-o` names the output for the mux and the join; an encode's output is its last argument.
        let out = command
            .argv
            .iter()
            .position(|arg| arg == "-o")
            .and_then(|at| command.argv.get(at + 1))
            .or(command.argv.last())
            .cloned();
        if let Some(out) = out {
            let path = Path::new(&out);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, b"a rendered file of some length");
        }
        Ok(())
    })
}

thread_local! {
    static CURRENT_APP: RefCell<Option<adw::Application>> = const { RefCell::new(None) };
}

/// Press ▶ and hand back the program's own log. `container` and `translate` are the two rows this item cares
/// about; every other row comes from `produce_screen::defaults`, so the press hashes real settings.
fn press(container: &str, translate: &[&str]) -> Vec<String> {
    naivepost::produce_translate::set_spawn_for_test(scripted_spawn());
    let app = CURRENT_APP
        .with(|held| held.borrow().clone())
        .expect("the round published the application");
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    release_last_window();
    let window = ui::build_window(&app, &model, Page::Produce.label());
    hold_last_window(window.clone());
    window.present();
    settle();

    let row = naivepost::produce_screen::defaults();
    let mut page = ui::produce_page::read_state();
    page.container = container.to_string();
    page.codec = row[1].1.to_string();
    page.preset = row[2].1.to_string();
    page.resolution = row[3].1.to_string();
    page.frame_rate = row[4].1.to_string();
    page.audio = row[5].1.to_string();
    page.subtitles = row[6].1.to_string();
    page.translate = translate.iter().map(|code| code.to_string()).collect();
    page.game_volume = row[8].1.parse().expect("game audio parses");
    page.crf = row[9].1.parse().expect("crf parses");
    page.voice = "1".to_string();
    ui::produce_page::set_state(page);
    ui::seed_review_cut(
        &window,
        &naivepost::cut::Cut {
            segs: two_segs(),
            aspect: "16:9".to_string(),
            ..Default::default()
        },
    );

    let button =
        ui::line_step_button(&window, "play-button").expect("the run bar drew a play-button");
    button.emit_by_name::<()>("clicked", &[]);
    // The press walks on a worker (§03-shell §1): its answer lands before anything is read.
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
        "the press's worker answered"
    );
    settle();
    let logs = ui::window::window_logs();
    window.close();
    settle();
    // A click that never reached the run would leave the log looking like some earlier press's.
    assert!(
        logs.iter()
            .any(|l| l.starts_with(">>> producing ") || l.starts_with(">>> transcoding ")),
        "the press never reached the render: {:?}",
        logs
    );
    logs
}

/// The cue bodies of a real .srt as the press wrote it — one entry per block, joined across its rows.
fn cue_texts(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).expect("the sheet the press wrote");
    text.split("\n\n")
        .filter_map(|block| {
            let mut lines = block.lines();
            let _ = lines.next()?;
            let _ = lines.next()?;
            let body: Vec<&str> = lines.collect();
            (!body.is_empty()).then(|| body.join(" "))
        })
        .collect()
}

// ---- the checks -------------------------------------------------------------------------------

/// Cues built in the default subtitle mode ("none in the video"), both files for every ticked language, and
/// no empty cue in what the press wrote. The ticked language answers nothing — its server is down — so its
/// pair carries the original text, which is bullet 2's other clause and what keeps the pair non-empty.
fn check_sidecars_for_every_language(tree: &Tree) {
    write_cut(tree);
    write_words(tree);
    let logs = press("mp4", &["German"]);
    let produced = tree.final_video("mp4").parent().unwrap().to_path_buf();
    for name in render::sidecar_names("final", None) {
        let path = produced.join(&name);
        assert!(
            path.is_file(),
            "the session's own track {name} is not beside the video"
        );
        assert!(
            !std::fs::read_to_string(&path).unwrap().trim().is_empty(),
            "{name} is empty"
        );
    }
    for name in render::sidecar_names("final", Some("de")) {
        assert!(
            produced.join(&name).is_file(),
            "the ticked language wrote no {name}"
        );
    }
    assert!(
        logs.iter()
            .any(|l| *l == render::sidecar_log("final", Some("de"))),
        "the press never said the ticked language's sidecar: {:?}",
        logs
    );
    // "never an empty cue", read off the file the press wrote rather than off a cue list.
    let bodies = cue_texts(&tree.final_srt(None));
    assert!(
        !bodies.is_empty(),
        "the sheet holds no cue at all: {bodies:?}"
    );
    for body in bodies.iter() {
        assert!(!body.trim().is_empty(), "one cue said nothing: {bodies:?}");
    }
    assert!(
        bodies
            .iter()
            .any(|b| b.contains("hello") || b.contains("there")),
        "the cues are not the session's own speech: {bodies:?}"
    );
}

/// Nothing to say anywhere → §5's refusal verbatim, no sheet, and the stale sweep still ran by exact name.
fn check_nothing_to_say(tree: &Tree) {
    write_cut(tree);
    let produced = tree.final_video("mp4").parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&produced).unwrap();
    // The page stores a tick under the name the tick itself carries (§A's Translate row: English, German,
    // French); its sidecar is named after the language's CODE (`final.de.srt`, the prototype's spelling). The ticked language's own pair is stale
    // (this run rewrites it), the bare stem is left over from an older render with nothing to rewrite it now,
    // and a language nobody ticked is not this run's to touch.
    let stale_de = produced.join("final.de.srt");
    let stale_de_vtt = produced.join("final.de.vtt");
    let bare_sheet = tree.final_srt(None);
    let bare_vtt = tree.final_vtt(None);
    let foreign = produced.join("final.nl.srt");
    std::fs::write(&stale_de, "1\n00:00:00,000 --> 00:00:01,000\nalt\n\n").unwrap();
    std::fs::write(&stale_de_vtt, "WEBVTT\n\n").unwrap();
    std::fs::write(&bare_sheet, "1\n00:00:00,000 --> 00:00:01,000\nweg\n\n").unwrap();
    std::fs::write(&bare_vtt, "WEBVTT\n\n").unwrap();
    std::fs::write(&foreign, "1\n00:00:00,000 --> 00:00:01,000\nnerlands\n\n").unwrap();

    let logs = press("mp4", &["German"]);
    assert_eq!(
        render::no_subtitle_cue_log(),
        "!!! nothing to put in a subtitle: no narration, and no speech in the clips"
    );
    assert!(
        logs.iter().any(|l| *l == render::no_subtitle_cue_log()),
        "the refusal is not on the log: {:?}",
        logs
    );
    // Nothing was written: the bare sheet is still the OLD text, not this run's, because this run had nothing
    // to put in it.
    assert!(
        std::fs::read_to_string(&bare_sheet)
            .unwrap()
            .contains("weg"),
        "a run with nothing to say wrote its own sheet anyway"
    );
    assert!(
        !stale_de.is_file() && !stale_de_vtt.is_file(),
        "the ticked language's stale pair outlived a run that had nothing to say"
    );
    assert!(
        foreign.is_file(),
        "a language nobody ticked is not this run's to delete"
    );
    let _ = bare_vtt;
}

/// "Stale ones deleted first", seen as content: the ticked language's old file is gone and the new one holds
/// THIS run's cues.
fn check_stale_sidecar_replaced(tree: &Tree) {
    write_cut(tree);
    write_words(tree);
    let produced = tree.final_video("mp4").parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&produced).unwrap();
    let stale = produced.join("final.de.srt");
    std::fs::write(
        &stale,
        "1\n00:00:00,000 --> 00:00:01,000\nvon einem alteren render\n\n",
    )
    .unwrap();

    press("mp4", &["German"]);
    let now = std::fs::read_to_string(&stale).expect("the .de sheet after the press");
    assert!(
        !now.contains("alteren"),
        "the old render's file still stands: {now}"
    );
    assert!(
        now.contains("hello") || now.contains("there"),
        "the replacement is not this run's cues: {now}"
    );
}

/// webm carries no subtitle track, so the run says where its subtitles are — after the mux, and the files it
/// points at exist beside the video.
fn check_webm_names_its_files(tree: &Tree) {
    write_cut(tree);
    write_words(tree);
    let logs = press("webm", &[]);
    let mux_at = logs
        .iter()
        .position(|l| l.contains(&details::joined("webm")) || l.contains("final.webm"))
        .expect("the run never logged its own mux");
    let note = logs
        .iter()
        .position(|l| *l == details::WEBM_SRT_LOG)
        .expect("the webm note is missing from the log");
    assert!(note > mux_at, "the note comes before the mux: {:?}", logs);
    let produced = tree.final_video("webm").parent().unwrap().to_path_buf();
    for name in render::sidecar_names("final", None) {
        assert!(
            produced.join(&name).is_file(),
            "the note points at {name}, which is not there"
        );
    }
}

/// A check that threw is reported as a message, not an abort through the C trampoline.
static FAILURE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// One check, parked so `connect_activate` (an `Fn`) can consume it exactly once.
type OnceCheck = Rc<RefCell<Option<Box<dyn FnOnce(&Tree)>>>>;

/// One check, one process: pin cwd, build the app, run the check inside its activate round. The check sits in
/// a [`OnceCheck`] because `connect_activate` wants an `Fn` and this consumes it once.
fn run(tag: &str, body: impl FnOnce(&Tree) + std::panic::RefUnwindSafe + 'static) {
    let root = std::env::temp_dir().join(format!("np-sec085-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    // No LLM answers at that address on purpose: a down translation server is a normal state and the run has
    // to stay green through it (§00: failure is specific and local).
    unsafe {
        std::env::set_var("NAIVEPOST_FAKE_LLM", "http://127.0.0.1:1");
        std::env::set_var("XDG_CONFIG_HOME", root.join("config").to_str().unwrap());
        std::env::set_var("XDG_DATA_HOME", root.join("data").to_str().unwrap());
    }
    let tree = session(&root);
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    CURRENT_APP.with(|held| *held.borrow_mut() = Some(app.clone()));
    let once: OnceCheck = Rc::new(RefCell::new(Some(Box::new(body))));
    app.connect_activate(move |app| {
        let app = app.clone();
        let tree = tree.clone();
        let once = once.clone();
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(check) = once.borrow_mut().take() {
                check(&tree);
            }
        }))
        .is_err()
        {
            let mut held = FAILURE.lock().unwrap();
            if held.is_none() {
                *held = Some(format!(
                    "the press for `{}` left the folder §5 does not describe",
                    CASE
                ));
            }
        }
        glib::idle_add_local(move || {
            app.quit();
            glib::ControlFlow::Break
        });
    });
    app.run_with_args(&["naivepost"]);
    let message = FAILURE.lock().unwrap().take();
    std::fs::remove_dir_all(&root).ok();
    assert_eq!(message, None, "{tag}");
}

/// The check this binary was built for, plus the entry point: `CASE` and the `#[test]` name come from the
/// sibling file that includes this one, so `cargo test` (and `just test`) runs the press with no arguments.
/// `sec_08_produce_5_rules_press_sidecars.rs` and its three siblings each set `CASE` and name their test
/// `sec_08_produce_5_rules_s2_…`.
pub fn run_case() {
    match CASE {
        "sidecars" => run("sidecars", check_sidecars_for_every_language),
        "silent" => run("silent", check_nothing_to_say),
        "replaced" => run("replaced", check_stale_sidecar_replaced),
        "webm" => run("webm", check_webm_names_its_files),
        other => panic!("`{other}` is not one of §5's four press checks"),
    }
}
