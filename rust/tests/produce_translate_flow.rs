//! F5.4 S3 (`translate each ticked language, in batches of P.machine.translateBatch`) — the walk in
//! `naivepost::produce_translate`, plus the wire that makes the Produce page's ▶ use it.
//!
//! The logic half drives the walk through an injected [`Ask`] (the same seam
//! `narrate_tts::speak_line` uses for both of its network legs), so every rule §F5.4 S3 states is
//! checked without a model standing: the batch count, the one re-ask naming its numbers as they were
//! originally given, the original-text fallback with its warning, the cache gate, the session's own
//! language riding along untranslated, and the placement tag travelling stripped and coming back
//! attached.
//!
//! The widget half fires the real `play-button` and reads the sidecar file off disk. It follows
//! `tests/produce_stamp_widgets.rs` exactly: one `adw::Application`, one `run_with_args` inside a
//! `static ONCE`, cwd pinned to a temp root before the run, and every check wrapped in
//! `catch_unwind` through [`record`] — an assertion unwinding straight into GTK's `extern "C"`
//! trampoline aborts the binary with `panic_cannot_unwind`, which reads as a crash instead of a failed
//! check. The checks are therefore recorded inside `connect_activate` and asserted after the run.
//!
//! What the press has to get past, and why the fixture is laid out as it is: the page answers the
//! up-to-date question first (no video and no stamp in this root, so it encodes), then S5 needs a tool
//! this container does not have (`command -v ffmpeg` is empty), so a scripted spawner writes each
//! command's own output file; and the cue sheet must be NON-empty or S9 writes no sidecars at all,
//! which is why the session word list is written with words inside the first segment.

use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Mutex;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Seg;
use naivepost::layout::Tree;
use naivepost::produce_subtitles as subs;
use naivepost::produce_translate as trans;
use naivepost::shell::Page;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, session_tree_of as session, settle};

// ---- logic helpers -------------------------------------------------------------------------------

/// A log sink for checks that do not care what the translation walk said: the wording itself is pinned by
/// `tests/produce_rules.rs`, these tests are about the track it returns.
fn ignore_log(_: &str) {}

/// A track of `n` cues, each text distinct, all at the bottom (`pos` empty means the player's default).
fn cues(n: usize) -> Vec<subs::Cue> {
    (0..n)
        .map(|i| subs::Cue {
            s: i as f64 * 3.0,
            e: i as f64 * 3.0 + 2.5,
            text: format!("original line number {i}"),
            pos: String::new(),
        })
        .collect()
}

/// Every recorded ask: the message it got and the numbers it was given.
#[derive(Default)]
struct Asks {
    calls: std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<usize>)>>>,
}

impl Asks {
    /// Answer every number with a distinct translated string.
    fn answer_all(&self) -> Box<trans::Ask> {
        let calls = self.calls.clone();
        Box::new(move |message: &str, numbers: &[usize]| {
            calls.lock().unwrap().push((message.to_string(), numbers.to_vec()));
            Ok(numbers
                .iter()
                .map(|n| (*n, format!("translated line {n}")))
                .collect::<BTreeMap<usize, String>>())
        })
    }

    /// Answer everything EXCEPT `skipped`, so the walk's re-ask path is what gets exercised.
    fn answer_skipping(&self, skipped: Vec<usize>) -> Box<trans::Ask> {
        let calls = self.calls.clone();
        Box::new(move |message: &str, numbers: &[usize]| {
            calls.lock().unwrap().push((message.to_string(), numbers.to_vec()));
            Ok(numbers
                .iter()
                .filter(|n| !skipped.contains(n))
                .map(|n| (*n, format!("translated line {n}")))
                .collect::<BTreeMap<usize, String>>())
        })
    }

    /// Each call answers everything it was given EXCEPT the numbers of its own entry in `skipped_by_call`
    /// (one entry per call), so a test controls round one and round two separately.
    fn answer_sequence(&self, skipped_by_call: Vec<Vec<usize>>) -> Box<trans::Ask> {
        let calls = self.calls.clone();
        let skipped = std::sync::Arc::new(std::sync::Mutex::new(skipped_by_call));
        Box::new(move |message: &str, numbers: &[usize]| {
            calls.lock().unwrap().push((message.to_string(), numbers.to_vec()));
            let mut rounds = skipped.lock().unwrap();
            let skip = if rounds.is_empty() { Vec::new() } else { rounds.remove(0) };
            drop(rounds);
            Ok(numbers
                .iter()
                .filter(|n| !skip.contains(n))
                .map(|n| (*n, format!("translated line {n}")))
                .collect::<BTreeMap<usize, String>>())
        })
    }

    fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn numbers_at(&self, index: usize) -> Vec<usize> {
        self.calls
            .lock()
            .unwrap()
            .get(index)
            .map(|(_, numbers)| numbers.clone())
            .unwrap_or_default()
    }

    fn message_at(&self, index: usize) -> String {
        self.calls
            .lock()
            .unwrap()
            .get(index)
            .map(|(message, _)| message.clone())
            .unwrap_or_default()
    }
}

fn ticked(languages: &[&str]) -> Vec<String> {
    languages.iter().map(|l| l.to_string()).collect()
}

// ---- the logic tests ----------------------------------------------------------------------------

/// §F5.4 S3: a ticked language's cues carry the TRANSLATED text, not the session's, and a full answer
/// is a cacheable one.
#[test]
fn f5_4_s3_a_ticked_language_gets_translated_cues_not_the_original() {
    let original = cues(3);
    let asks = Asks::default();
    let ask = asks.answer_all();
    let tracked = trans::track(&original, "English", &ticked(&["German"]), &ask, ignore_log);

    assert_eq!(tracked.len(), 1, "one ticked language, one track");
    let german = &tracked[0];
    assert_eq!(german.language, "German");
    assert_eq!(tracked.len(), 1, "one ticked language, one track");
    assert_eq!(
        german.cues.iter().map(|c| c.text.clone()).collect::<Vec<String>>(),
        vec!["translated line 1", "translated line 2", "translated line 3"],
        "the track carries the answers, not the originals"
    );
    for (index, cue) in german.cues.iter().enumerate() {
        // Only the text travels: the spans stay where the original put them.
        assert_eq!(cue.s, original[index].s);
        assert_eq!(cue.e, original[index].e);
        assert_ne!(cue.text, original[index].text, "cue {} still the original", index + 1);
    }
    assert!(german.missing.is_empty(), "nothing missing: {german:?}");
    assert_eq!(german.cached, subs::cached(true), "a complete answer is cacheable");
}

/// §F5.4 S3 + P.machine.translateBatch (150): 150 lines ride in ONE request, 151 cost two.
#[test]
fn f5_4_s3_150_lines_is_one_request_and_151_is_two() {
    assert_eq!(subs::batches(150).len(), 1, "a 150-line track is one batch");
    assert_eq!(subs::batches(151).len(), 2, "the 151st line opens a second");
    // The batch boundary is the half-open pair the walk slices by.
    assert_eq!(subs::batches(151), vec![(0, 150), (150, 151)]);

    let asks = Asks::default();
    let ask = asks.answer_all();
    let tracked = trans::track(&cues(150), "English", &ticked(&["German"]), &ask, ignore_log);
    assert_eq!(asks.count(), 1, "a full 150-line track asked once");
    assert_eq!(tracked[0].missing.len(), 0);

    let asks = Asks::default();
    let ask = asks.answer_all();
    let tracked = trans::track(&cues(151), "English", &ticked(&["German"]), &ask, ignore_log);
    assert_eq!(asks.count(), 2, "151 lines asked twice, one per batch");
    // The second batch keeps the TRACK's numbers: line 151 is asked for as 151, never as 1.
    assert_eq!(asks.numbers_at(1), vec![151], "second batch asked for line 151 by its own number");
    assert!(tracked[0].missing.is_empty());
}

/// §F5.4 S3 (`re-asked once with the ORIGINAL numbers`): the repair call names line 2 as 2, and the
/// answer fills the gap.
#[test]
fn f5_4_s3_a_missing_number_is_asked_once_more_with_its_original_number() {
    let original = cues(3);
    let asks = Asks::default();
    let mut seq: Vec<Vec<usize>> = Vec::new();
    seq.push(vec![2]); // the first round keeps line 2 back
    seq.push(vec![]); // the re-ask answers everything it is named
    let _asks = Asks::default();
    let ask = asks.answer_sequence(seq);
    let tracked = trans::track(&original, "English", &ticked(&["German"]), &ask, ignore_log);

    assert_eq!(asks.count(), 2, "one batch, one refusal of line 2, so exactly one re-ask");
    assert_eq!(asks.numbers_at(1), vec![2], "the re-ask named line 2 by the number it was given");
    // The re-ask is `ask_again_text`, not the plain sentence: it says outright that the numbers do not
    // start at 1, which is what stops the model renumbering them.
    let second = asks.message_at(1);
    assert!(
        second.contains("2") && second.contains('\t'),
        "the re-ask carries the number and a tab: {second:?}"
    );
    assert!(tracked[0].missing.is_empty(), "the second round filled it: {:?}", tracked[0]);
    assert_eq!(tracked[0].cues[1].text, "translated line 2");
    assert_eq!(tracked[0].cached, subs::cached(true));
}

/// §F5.4 S3 (`still missing → the original text, with a warning`, `cached only when complete`).
#[test]
fn f5_4_s3_still_missing_ships_the_original_with_a_warning_and_caches_nothing() {
    let original = cues(3);
    // A closure that never answers line 2, in either round.
    let asks = Asks::default();
    let ask = asks.answer_skipping(vec![2, 2]);
    let tracked = trans::track(&original, "English", &ticked(&["German"]), &ask, ignore_log);
    // The second round only re-asked 2 and skipped it again.
    assert_eq!(asks.numbers_at(1), vec![2]);

    let german = &tracked[0];
    assert_eq!(
        german.cues[1].text,
        subs::sent_text(&original[1]),
        "the line that stayed behind ships as the original text"
    );
    assert_eq!(german.cues[1].s, original[1].s, "its span is untouched");
    let warning = german.warning.clone().expect("a warning was raised");
    assert!(warning.contains('2'), "the warning names line 2: {warning}");
    assert!(warning.contains("German"), "and the language it belongs to: {warning}");
    assert!(!german.cached, "a partial answer is never cached");
    assert_eq!(german.missing, vec![2]);
    // The other two lines still arrived translated: one bad line costs one line, not the track.
    assert_eq!(german.cues[0].text, "translated line 1");
    assert_eq!(german.cues[2].text, "translated line 3");
}

/// §F5.4 S3 (`the session's own language = track 0, never translated`): it is the bare-stem track that
/// is already there, so the walk is not even asked.
#[test]
fn f5_4_s3_the_session_language_is_track_zero_and_never_asked() {
    // The rule, straight from `subs`: the session's own language leaves the ticked list.
    assert_eq!(
        subs::track_languages("English", &ticked(&["English", "German"])),
        vec!["German".to_string()],
        "the session's own language is dropped whatever the tick says"
    );

    let asks = Asks::default();
    let ask = asks.answer_all();
    let tracked =
        trans::track(&cues(3), "English", &ticked(&["English", "German"]), &ask, ignore_log);
    assert_eq!(tracked.len(), 1, "only the translated language gets a track here");
    assert_eq!(tracked[0].language, "German");
    assert_eq!(asks.count(), 1, "exactly one ask for the whole run");
    assert!(
        asks.message_at(0).contains("German") && !asks.message_at(0).contains("English"),
        "the one ask was for German: {:?}",
        asks.message_at(0)
    );

    // Ticked with nothing but its own language: the walk does nothing at all and dials nobody.
    let asks = Asks::default();
    let ask = asks.answer_all();
    let tracked = trans::track(&cues(3), "English", &ticked(&["English"]), &ask, ignore_log);
    assert!(tracked.is_empty(), "the session's own language is not a translation target");
    assert_eq!(asks.count(), 0, "no ask was made for the session's own language");
}

/// §3.10's two must-fixes on the return path: the tag goes out stripped and comes back attached by the
/// ORIGINAL cue, and the model's own row breaks survive instead of being re-wrapped.
#[test]
fn f5_4_s3_the_placement_tag_goes_stripped_and_comes_back_attached() {
    let top = subs::Cue {
        s: 0.0,
        e: 2.0,
        text: r"{\an8}the caption that sits at the top".to_string(),
        pos: "top".to_string(),
    };
    // Out: no tag anywhere near the model.
    assert!(
        !subs::sent_text(&top).contains(r"\an8"),
        "sent_text strips the tag: {:?}",
        subs::sent_text(&top)
    );
    let asks = Asks::default();
    let ask = asks.answer_all();
    let tracked = trans::track(std::slice::from_ref(&top), "English", &ticked(&["German"]), &ask, ignore_log);
    assert!(
        !asks.message_at(0).contains(r"\an8"),
        "the message carried no tag: {:?}",
        asks.message_at(0)
    );
    // Back: the ORIGINAL cue puts its placement on whatever came back.
    assert!(
        tracked[0].cues[0].text.starts_with(r"{\an8}"),
        "the placement came back attached: {:?}",
        tracked[0].cues[0].text
    );
    assert_eq!(tracked[0].cues[0].pos, "top", "and its pos field still says so");

    // A model answer that wraps the line itself is KEPT, not re-wrapped at
    // P.policy.subtitleRowChars (42): the break was chosen in the other language, on purpose.
    let long_line = "a translated line long enough that the app would certainly break it at forty-two";
    // The model answers with its own break in it; the return path must keep it (S3/§3.10 second fix).
    let wanted = long_line.replace("that", "that\n");
    let answers: BTreeMap<usize, String> = BTreeMap::from([(1usize, wanted.clone())]);
    let asks = Asks::default();
    let calls = asks.calls.clone();
    let scripted: std::rc::Rc<dyn Fn(&str, &[usize]) -> Result<BTreeMap<usize, String>, String>> =
        Rc::new(move |message: &str, numbers: &[usize]| {
        calls.lock().unwrap().push((message.to_string(), numbers.to_vec()));
        Ok(numbers
            .iter()
            .filter_map(|number| answers.get(number).cloned().map(|text| (*number, text)))
            .collect::<BTreeMap<usize, String>>())
    });
    let tracked = trans::track(&cues(1), "English", &ticked(&["German"]), &*scripted, ignore_log);
    assert_eq!(
        tracked[0].cues[0].text, wanted,
        "the model's own break survived — a re-wrap at 42 would have moved it"
    );
    assert!(tracked[0].cues[0].text.contains('\n'));
    assert!(tracked[0].missing.is_empty(), "the answer covered line 1: {:?}", tracked[0]);
}

// ---- the widget wire -----------------------------------------------------------------------------

/// One slot per check, holding that block's failure text if it failed. Written from inside the activate
/// callback and read after `run_with_args` returns, so a failing assert never unwinds into GTK.
static SIDECAR_FAIL: Mutex<Option<String>> = Mutex::new(None);
/// The §S4 press check's own slot, so a failure here is not blamed on the sidecar check above.
static SHEET_FAIL: Mutex<Option<String>> = Mutex::new(None);

/// Run one check so a panic becomes a recorded message rather than an abort through the C trampoline.
fn record(slot: &Mutex<Option<String>>, label: &str, body: impl FnOnce() -> Result<(), String> + std::panic::UnwindSafe) {
    let outcome = std::panic::catch_unwind(body);
    let failure = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(why)) => Some(why),
        Err(payload) => Some(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string()),
        ),
    };
    if let Some(why) = failure {
        *slot.lock().unwrap() = Some(format!("{label}: {why}"));
    }
}

thread_local! {
    static CURRENT_APP: std::cell::RefCell<Option<adw::Application>> = const { std::cell::RefCell::new(None) };
}

fn app_in_round() -> adw::Application {
    CURRENT_APP
        .with(|held| held.borrow().clone())
        .expect("run_round published the application")
}

/// Two segments, so the cut is non-empty and the press reaches the render instead of being refused.
fn two_segs() -> Vec<Seg> {
    let mut first = Seg::default();
    first.s = 0.0;
    first.e = 9.5;
    let mut second = Seg::default();
    second.s = 10.0;
    second.e = 30.0;
    vec![first, second]
}

/// A window sitting on the Produce tab.
fn produce_window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Produce.label());
    hold_last_window(window.clone());
    window.present();
    settle();
    window
}

/// Seed the page's own row (as `tests/produce_stamp_widgets.rs` does), then tick a language and name
/// the session's own — the two fields the S3 walk reads out of `ProduceState`.
fn seed_page(_window: &adw::ApplicationWindow) {
    let row = naivepost::produce_screen::defaults();
    let mut page = ui::produce_page::read_state();
    page.container = row[0].1.to_string();
    page.codec = row[1].1.to_string();
    page.preset = row[2].1.to_string();
    page.resolution = row[3].1.to_string();
    page.frame_rate = row[4].1.to_string();
    page.audio = row[5].1.to_string();
    page.subtitles = row[6].1.to_string();
    page.game_volume = row[8].1.parse().expect("game audio parses");
    page.crf = row[9].1.parse().expect("crf parses");
    page.vfr = row[10].1 == "on";
    page.mono = row[11].1 == "on";
    page.blurred_edges = row[12].1 == "on";
    page.voice = "1".to_string();
    page.session_language = "English".to_string();
    ui::produce_page::set_state(page);
}

/// The scripted spawner: no ffmpeg exists in this container, so each command's own output file is
/// "written" here, the way `tests/produce_render_exec.rs` fakes it.
fn scripted_spawn() -> trans::SpawnSlot {
    Rc::new(|command: &naivepost::produce_exec::Command| {
        if let Some(out) = command.argv.last() {
            let path = Path::new(out);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, b"a rendered file of some length");
        }
        Ok(())
    })
}

/// Words for the session's own list, inside the FIRST segment (0.0–9.5) and on a source the fixture's
/// `narrator` filter cannot mistake for the mic: that is what gives the run a non-empty cue sheet, and
/// no sheet means no sidecars at all.
fn write_words(tree: &Tree) {
    let words = vec![
        naivepost::word_list::Word {
            source: "/media/room.wav".to_string(),
            match_word: "alpha".to_string(),
            written: "alpha".to_string(),
            start: 1.0,
            end: 1.4,
            stray: false,
        },
        naivepost::word_list::Word {
            source: "/media/room.wav".to_string(),
            match_word: "bravo".to_string(),
            written: "bravo".to_string(),
            start: 1.6,
            end: 2.0,
            stray: false,
        },
        naivepost::word_list::Word {
            source: "/media/room.wav".to_string(),
            match_word: "charlie".to_string(),
            written: "charlie".to_string(),
            start: 2.2,
            end: 2.8,
            stray: false,
        },
    ];
    naivepost::word_list::save(tree, &words).expect("word list written");
}

/// Tick a language on the page's OWN widget, so the wire the test exercises is the toggle handler and
/// not a state poke. The tick exists because `session_language` is not that language
/// (`screen::translate_options` drops the session's own language from the row).
fn tick_language(window: &adw::ApplicationWindow, language: &str) {
    let name = format!("translate-tick-{language}");
    let tick = ui::produce_page::widget_in(window, &name)
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        .unwrap_or_else(|| panic!("the page drew no {name}"));
    // The real signal path, not `set_active` (which is swallowed by the page's repaint guard): the
    // handler that a click runs is `toggled`, so emit it and report the state afterwards.
    tick.set_active(true);
    // GTK declares `toggled` with no return value, so emit it as `()`.
    tick.emit_by_name::<()>("toggled", &[]);
    settle();
    let now = ui::produce_page::read_state().translate.clone();
    assert!(now.iter().any(|t| t == language), "emitting {name} did not reach `state.translate` (state is {now:?})");
}

/// The check: press the real ▶ and read the ticked language's `.srt` back off disk.
/// A name nothing listens on, in the shape the settings box holds it. The leg dials this and gets a
/// refused connection, which is §F5.4 S3's unanswered batch: the track ships as the original with the
/// warning. `just test` runs one binary per process, so reading it here touches no other test's env.
fn dead_server_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// Rewrite the settings' LLM address — the seam the real leg reads per batch.
fn point_at(url: &str) {
    let paths = naivepost::settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = naivepost::settings::read(&paths).expect("settings read back");
    conf.server = url.to_string();
    naivepost::settings::save(&paths, &conf).expect("new address written");
}

fn check_press_writes_the_translated_sidecar(root: &Path) {
    let tree = session(root);
    // The page reads `cut/cut.json` as its second answer for "what is the cut".
    let cut = naivepost::cut::Cut {
        segs: two_segs(),
        aspect: "16:9".to_string(),
        ..Default::default()
    };
    let file = tree.cut_json();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, serde_json::to_string_pretty(&cut).unwrap()).unwrap();
    write_words(&tree);

    // The settings address is the seam (§F5.4 S3, round rule 5): the leg reads `settings::read` per
    // batch and dials whatever the file names, so pointing it at an unused port is what the program does
    // when the translation server is simply not there. S3's own rule answers that way — every line ships
    // as the ORIGINAL with the merge's warning, and the render carries on rather than failing over a model.
    point_at(&dead_server_url());
    trans::set_spawn_for_test(scripted_spawn());

    let window = produce_window(&app_in_round());
    ui::seed_review_cut(
        &window,
        &naivepost::cut::Cut {
            segs: two_segs(),
            aspect: "16:9".to_string(),
            ..Default::default()
        },
    );
    seed_page(&window);
    tick_language(&window, "German");

    let button = ui::line_step_button(&window, "play-button").expect("the run bar drew a play-button");
    button.emit_by_name::<()>("clicked", &[]);
    settle();

    let said = ui::find_status(window.upcast_ref())
        .expect("the window has a status line")
        .text()
        .to_string();
    assert_ne!(
        said,
        naivepost::cut::NO_CUT_YET,
        "the press thought there was no cut: {said:?}"
    );
    assert_ne!(
        said,
        naivepost::produce_flow::SKIP_LOG,
        "the press skipped the encode instead of rendering: {said:?}"
    );

    let logs = ui::window::window_logs();
    let sidecar_logged = logs
        .iter()
        .any(|l| l.contains("final.") && l.contains("German") && l.contains(".vtt"));
    assert!(
        sidecar_logged,
        "the run logged the ticked language's sidecars: {:?}",
        logs.iter().filter(|l| l.contains("subtitles")).collect::<Vec<&String>>()
    );

    let srt = tree.final_srt(Some("German"));
    let bare = tree.final_srt(None);
    assert!(bare.exists(), "the session's own .srt is beside it at {}", bare.display());
    assert!(srt.exists(), "the ticked language's .srt was written at {}", srt.display());
    let text = std::fs::read_to_string(&srt).expect("the sidecar reads back");
    // S3's unanswered-batch rule, reached through the real leg: no server answers, so this track is the
    // original text and the run says so out loud rather than dropping the language.
    assert!(
        text.contains("alpha bravo charlie"),
        "with no server the sidecar ships the original text: {text}"
    );
    let said_logs = ui::window::window_logs();
    assert!(
        said_logs.iter().any(|l| l.contains("did not answer") || l.contains("original")),
        "the unanswered translation is said, not swallowed: {:?}",
        said_logs
            .iter()
            .filter(|l| l.contains("subtitles") || l.contains("translate"))
            .collect::<Vec<&String>>()
    );
    assert!(
        !text.contains("uebersetzt"),
        "nothing invented can appear in a track the model never answered: {text}"
    );
    // The originals are still where a player looks first: the bare stem, untranslated.
    let bare = tree.final_srt(None);
    if bare.exists() {
        let bare_text = std::fs::read_to_string(&bare).expect("the bare stem reads back");
        assert!(
            !bare_text.contains("uebersetzt"),
            "the session's own track stays in its own language: {bare_text}"
        );
    }
    window.close();
    settle();
}

/// §S4's sentence, built through the rule so this cannot drift from the wording `produce_exec` prints.
fn cue_count_sentence(counts: naivepost::produce_subtitles_run::CueCounts, cues: usize) -> String {
    naivepost::produce_subtitles_run::cue_count_log(cues, &counts)
}

/// The press that letters nothing: voice "captions only" with Subtitles "none in the video", where the words
/// exist ONLY in the file beside the video and F5.2 S2 says so out loud. The cue sheet is still written —
/// refusing to burn or to mux a track is not refusing to caption — so this check pins both halves: the
/// warning line appears, and `clips/final.srt` carries the spoken words.
fn check_press_writes_the_sheet_and_says_what_it_holds(root: &Path) {
    let tree = session(root);
    let cut = naivepost::cut::Cut {
        segs: two_segs(),
        aspect: "16:9".to_string(),
        ..Default::default()
    };
    let file = tree.cut_json();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, serde_json::to_string_pretty(&cut).unwrap()).unwrap();
    write_words(&tree);
    trans::set_spawn_for_test(scripted_spawn());

    let window = produce_window(&app_in_round());
    ui::seed_review_cut(&window, &cut);
    seed_page(&window);
    // Voice "captions only" (§1's dropdown id) plus Subtitles "none in the video": the one combination where
    // the words are in the sidecar alone, which is exactly what S2 promises to warn about. Both are rows the
    // page already draws; this writes them through `set_state` and repaints, the same door every flow uses.
    let mut page = ui::produce_page::read_state();
    page.voice = naivepost::narrate_screen::CAPTIONS.to_string();
    page.subtitles = "none in the video".to_string();
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(&window);
    settle();
    let button = ui::line_step_button(&window, "play-button").expect("the run bar drew a play-button");
    button.emit_by_name::<()>("clicked", &[]);
    settle();

    let said = ui::find_status(window.upcast_ref())
        .expect("the window has a status line")
        .text()
        .to_string();
    assert_ne!(said, naivepost::cut::NO_CUT_YET, "the press thought there was no cut: {said:?}");

    // §S4's file, on disk, holding the clip's own speech: the render handed its cue sheet to the encoder's
    // folder rather than an empty list to nothing.
    let sheet_path = tree.clips_dir().join(naivepost::produce_render::final_srt_name());
    let sheet = std::fs::read_to_string(&sheet_path)
        .unwrap_or_else(|why| panic!("{} was not written: {why}", sheet_path.display()));
    assert!(
        sheet.contains("alpha bravo charlie"),
        "the sheet holds the clip's speech: {sheet}"
    );

    let logs = ui::window::window_logs();
    assert!(
        logs.iter().any(|l| l.starts_with(">>> subtitles:") && l.contains("cues from")),
        "S4 says what its sheet holds: {:?}",
        logs.iter().filter(|l| l.contains("subtitles")).collect::<Vec<&String>>()
    );
    assert!(
        logs.contains(
            &naivepost::produce_render::captions_only_warning(naivepost::project::Subtitles::None)
                .expect("that pairing has a warning")
                .to_string()
        ),
        "F5.2 S2's captions-only warning is logged, not only computed: {logs:?}"
    );
    assert!(
        !logs.iter().any(|l| l.starts_with("!!! nothing to put in a subtitle")),
        "there WAS something to caption, so the refusal must not print: {logs:?}"
    );
    // The sentence's own shape, pinned against the rule it came from: at least one clip spoke for itself.
    let counts = naivepost::produce_subtitles_run::CueCounts { from_lines: 0, from_speech: 1 };
    assert_eq!(
        cue_count_sentence(counts, 1),
        ">>> subtitles: 1 cues from 0 spoken line(s) and 1 clip(s) of their own speech."
    );
    window.close();
    settle();
}

/// The check, on the one thread the application runs on, recorded rather than aborted.
fn run_round(app: &adw::Application) {
    CURRENT_APP.with(|held| *held.borrow_mut() = Some(app.clone()));

    let base = std::env::current_dir().expect("cwd pinned before the run");
    let root = base.join("translate");
    std::fs::create_dir_all(&root).expect("check root created");
    std::env::set_current_dir(&root).expect("cwd into the check root");
    record(
        &SIDECAR_FAIL,
        "translated-sidecar press",
        std::panic::AssertUnwindSafe(move || {
            check_press_writes_the_translated_sidecar(&root);
            Ok(())
        }),
    );

    // The §S4 check runs in the SAME round: this binary may call `Application::run` exactly once, so a
    // second window would never be activated. Its own temp folder keeps its files apart from the check
    // above, and cwd is moved back out afterwards for the same reason.
    let sheet_root = base.join("sheet");
    std::fs::create_dir_all(&sheet_root).expect("sheet check root created");
    std::env::set_current_dir(&sheet_root).expect("cwd into the sheet check root");
    record(
        &SHEET_FAIL,
        "cue-sheet press",
        std::panic::AssertUnwindSafe(move || {
            check_press_writes_the_sheet_and_says_what_it_holds(&sheet_root);
            Ok(())
        }),
    );
    std::env::set_current_dir(&base).expect("cwd back out of the check roots");

    let app = app.clone();
    glib::idle_add_local(move || {
        app.quit();
        glib::ControlFlow::Break
    });
}

#[test]
fn f5_4_s4_press_on_the_produce_page_writes_the_cue_sheet() {
    // The wire, not the rule: a real ▶ click on the run bar reaches `produce_exec`'s §S4 step and the cue
    // sheet lands on disk. The rules themselves are pinned in `tests/produce_subtitles_flow.rs`; what this
    // pins is that the button's own handler gets there, through the page's snapshot of the cut, the words
    // and the settings — and that F5.2 S2's captions-only warning is said by the same press.
    // Its block runs inside the one application round below (this binary may call `Application::run`
    // exactly once), so its failure text is read out of `SHEET_FAIL` alongside the sidecar check's.
    let failed = SHEET_FAIL.lock().unwrap().clone();
    assert!(
        failed.is_none(),
        "the cue-sheet press check ran in-round: {}",
        failed.unwrap_or_default()
    );
}

#[test]
fn f5_4_s3_pressing_produce_with_a_language_ticked_writes_the_translated_sidecar() {
    // Pin cwd BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`.
    let root = std::env::temp_dir().join(format!("np-f54s3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        // Our own argv: `run()` would hand the harness's flags to libgio, which treats an option it
        // does not know as fatal and aborts before the activate callback runs.
        app.run_with_args(&["naivepost"]);
    });

    let failed = SIDECAR_FAIL.lock().unwrap().take();
    let sheet_fail = SHEET_FAIL.lock().unwrap().take();
    std::fs::remove_dir_all(&root).ok();
    assert!(failed.is_none(), "{}", failed.unwrap_or_default());
    assert!(sheet_fail.is_none(), "{}", sheet_fail.unwrap_or_default());
}
