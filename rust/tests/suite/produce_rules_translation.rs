//! §08-produce#5-rules, bullet 2's translation words and bullet 4's webm note, on the walk's own log.
//!
//! §5 says gaps are "re-asked once, then left in the original", the "own language never translated", and
//! "webm carries no subtitle track". §6 spells the three sentences a run must say while that happens. Two of
//! them were spelled in `produce_subtitles` but nothing reached them: the re-ask line sat inside
//! `produce_translate`'s per-language walk with no way out, and the webm note had no caller at all, so a
//! person watching a webm render had no way to tell absent (wrong) from absent (impossible).
//!
//! The walk (`produce_exec::walk`) owns the order — note AFTER the mux, warnings after the translation — so
//! every check here drives the real walk over a real scratch folder with the program's own seams: the
//! translation leg, the spawner and the log.

use std::cell::RefCell;
use std::rc::Rc;

use naivepost::produce_exec as exec;
use naivepost::produce_flow as flow;
use naivepost::produce_render as render;
use naivepost::produce_subtitles as subs;
use naivepost::project::{Container, Produce, Publish};

const ITEM: &str = "§08-produce#5-rules";

/// Every check but the last drives a clip whose footage says these two words, which is what gives the run a cue.
const LOUD: &[&str] = &["the", "lecture"];

use crate::common;
use common::{clip as seg, produce_project as project};

fn no_fx(_: &naivepost::cut::Seg) -> Vec<naivepost::cut::Fx> {
    Vec::new()
}
fn exists_everything(_: &str) -> bool {
    true
}
fn no_wav(_: &naivepost::narration::Entry) -> Option<std::path::PathBuf> {
    None
}
fn any_source(_: &render::Clip) -> Option<String> {
    Some("project:media/in.mp4".to_string())
}
fn project_sources() -> Vec<naivepost::project::Source> {
    project().sources
}

/// The clip's own words. `subs::tidy` shortens every cue to its neighbour's start and folds one shorter than
/// `MIN_SECONDS` into the next, so a 0.4 s word can only ever be ONE caption whatever the spacing: TWO cues
/// come from TWO clips, which is what the two gap checks need (one hole out of two numbers is a re-ask; one
/// number in the world would leave nothing to re-ask). A single-cue check passes one text for one clip.
fn words(texts: &[&str], at: f64) -> Vec<subs::Word> {
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| subs::Word {
            text: t.to_string(),
            s: at + 1.0 + i as f64 * 10.0,
            e: at + 1.0 + i as f64 * 10.0 + 0.4,
        })
        .collect()
}

pub fn cut_of(segs: Vec<naivepost::cut::Seg>) -> naivepost::cut::Cut {
    naivepost::cut::Cut {
        segs,
        ..Default::default()
    }
}

/// The materials, with the translation leg scripted per check: `answer` sees the line numbers the batch
/// asked for and returns what the model answered, so a hole in the reply is a hole in the track.
/// The scripted answer outlives the walk: `Materials::translate` is a `&'static`-shaped `Ask`, which is what
/// forces the counter and the answer into `Rc`/`Arc` rather than a borrow.
struct Mats<'a> {
    settings: Produce,
    // §1's second door needs the session's own language to be knowable to the render, which is what
    // `Cues::session_language` carries and what the page fills from the live project.
    session_language: String,
    // The render's own `Ask` is `&'static`-shaped, so a scripted leg has to be `&'static`. Each `drive` leaks
    // its own answer for the length of the process (one `drive` at a time per test binary, as the wire tests
    // say of their env) — the same trade `produce_translate::set_spawn_for_test` makes for the spawner.
    ask: &'static naivepost::produce_translate::Ask,
    words: &'a dyn Fn(&render::Clip) -> Vec<subs::Word>,
}

impl<'a> Mats<'a> {
    /// The materials for one walk: the scripted translation leg and the clip's own words, both held by this
    /// struct so the borrows the render's `Materials` wants live as long as the walk does.
    fn materials(&'a self) -> exec::Materials<'a> {
        exec::Materials {
            exists: &exists_everything,
            fx_for: &no_fx,
            lanes: &[] as &[naivepost::cut::Lane],
            src_shape: (1920, 1080),
            wav_of: &no_wav,
            source_file: &any_source,
            on_recording: true,
            footage_left: &render::unknown_footage,
            filmed_runs: naivepost::produce_plan_logs::NO_TAPE,
            base_of: &|_seg| String::new(),
            wav_seconds: &|_entry| None,
            cues: exec::Cues {
                languages: self.settings.translate.clone(),
                session_language: self.session_language.clone(),
                ..Default::default()
            },
            words: self.words,
            translate: self.ask,
            sources: &project_sources,
            probe: &|_file| None,
        }
    }
}

/// Everything one walk needs: the folder, the plan, and every line the run said, in order.
struct Driven {
    root: std::path::PathBuf,
    tree: naivepost::layout::Tree,
    ran: exec::Ran,
    logs: Vec<String>,
}

/// The scripted translation leg's shape: line numbers in, the answers back.
type ScriptedAnswer = Box<dyn Fn(&[usize]) -> std::collections::BTreeMap<usize, String>>;

fn drive(
    tag: &str,
    settings: Produce,
    segs: Vec<naivepost::cut::Seg>,
    answer: ScriptedAnswer,
    spoken: Vec<subs::Word>,
    session_language: &str,
) -> Driven {
    let (root, tree) = common::tree_in(tag);
    let run = flow::snapshot(
        Some(cut_of(segs)),
        &tree,
        Vec::new(),
        settings.clone(),
        &project(),
        Some(Publish::default()),
        "1".to_string(),
    );
    // The walk's cue source and its translation leg, both scripted. Each word becomes a cue of its own (see
    // `words`), so a hole in a reply is visible as exactly one line number.
    let source = spoken;
    let answer: &'static dyn Fn(&[usize]) -> std::collections::BTreeMap<usize, String> =
        Box::leak(answer);
    let ask: &'static naivepost::produce_translate::Ask =
        Box::leak(Box::new(move |message: &str, numbers: &[usize]| {
            // A re-ask says "again" in §6's own wording; the answers it expects keep the original numbering.
            let _ = message;
            Ok(answer(numbers))
        }));
    let mats = Mats {
        settings,
        session_language: session_language.to_string(),
        ask,
        // The plan and the walk must see the SAME cue sheet: a caller that hands no words (the "nothing to put
        // in a subtitle" case) gets an empty track from both doors, not one from each.
        words: &|_c| source.clone(),
    };
    let logs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = logs.clone();
    let plan = exec::plan(&run, &tree, &mats.materials());
    // One test needs a clip with no words at all (§5's "nothing to put in a subtitle"), so `drive` hands the
    // walk its materials through this hook rather than reusing the plan's.
    let ran = exec::walk(
        &plan,
        &mats.materials(),
        move |line| sink.borrow_mut().push(line.to_string()),
        |_line| Ok(()),
        |_at, _what| true,
        |command| {
            // The scripted spawner stands in for ffmpeg: every command's `-o` output appears on disk so the
            // walk's own "did it land" checks pass as they do with a real encoder.
            if let Some(pos) = command.argv.iter().position(|a| a == "-o") {
                if let Some(out) = command.argv.get(pos + 1) {
                    let path = std::path::Path::new(out);
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(path, b"encoded");
                }
            }
            Ok(())
        },
    );
    let shown = logs.borrow().clone();
    Driven {
        root,
        tree,
        ran,
        logs: shown,
    }
}

/// Every line answered: the walk's completion path (no re-ask, no warning, a cache place).
fn all_translated(numbers: &[usize]) -> std::collections::BTreeMap<usize, String> {
    numbers
        .iter()
        .map(|n| (*n, format!("uebersetzt {n}")))
        .collect()
}

/// §6's line naming the numbers that never came back, spelled by the program's own function.
fn left_line(numbers: &[usize]) -> String {
    subs::left_in_the_original("de", numbers)
}

/// §6's re-ask line, spelled by the program's own function.
fn reask_line(numbers: &[usize]) -> String {
    subs::came_back_missing("de", numbers)
}

#[test]
fn sec_08_produce_5_rules_s2_a_gap_is_re_asked_once_and_then_named_on_the_log() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // §5: "translations read back by number; gaps re-asked once, then left in the original". The walk asks
    // every number of the batch twice — the first round and the single re-ask — and the model here answers
    // only the FIRST round's first line and then everything on the re-ask. What must be on the log is §6's
    // re-ask sentence naming the hole, then the cache line for the finished track, and NO
    // left-in-the-original warning because the re-ask filled it.
    let asks = Rc::new(RefCell::new(0usize));
    let seen = asks.clone();
    let answer = move |numbers: &[usize]| -> std::collections::BTreeMap<usize, String> {
        // The counter is keyed on the NUMBERS asked, not on a call count: `track` answers from its cache
        // within one process (each check runs in its own binary), and a cached batch never calls the leg at
        // all. Round 1 of this scenario is the ask that carries both numbers.
        if numbers.len() > 1 {
            *seen.borrow_mut() += 1;
            // Only the first line comes back: the rest "came back missing".
            return numbers[..1]
                .iter()
                .map(|n| (*n, format!("eins {n}")))
                .collect();
        }
        *seen.borrow_mut() += 1;
        all_translated(numbers)
    };
    let got = drive(
        "s5-rules-reask",
        Produce {
            translate: vec!["de".to_string()],
            ..Default::default()
        },
        vec![seg(0.0, 10.0), seg(10.0, 20.0)],
        Box::new(answer),
        words(LOUD, 0.0),
        "en",
    );
    assert_eq!(got.ran.failed, None, "{:?}", got.ran.failed);
    assert_eq!(
        asks.borrow().clone(),
        2,
        "one round plus exactly ONE re-ask"
    );
    let missing = reask_line(&[2]);
    assert!(
        got.logs.contains(&missing),
        "the re-ask is said before it happens: {:?}",
        got.logs
    );
    assert!(
        got.logs
            .iter()
            .any(|l| l.starts_with(">>> subtitles: de came from the cache")),
        "the finished track says where an answer like this can come from: {:?}",
        got.logs
    );
    assert!(
        !got.logs.iter().any(|l| l.contains("left in the original")),
        "nothing stayed behind, so no warning: {:?}",
        got.logs
    );
    let sheet = std::fs::read_to_string(got.tree.final_srt(Some("de"))).expect("the German sheet");
    assert!(
        sheet.contains("uebersetzt 2"),
        "the re-ask's answer is in the file: {sheet}"
    );
    let _ = std::fs::remove_dir_all(&got.root);
}

#[test]
fn sec_08_produce_5_rules_s2_a_gap_the_re_ask_cannot_fill_is_left_and_said() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // The other ending: the hole survives the re-ask. The line is NOT dropped — the original is kept and the
    // run says so in §6's words, naming the number. That sentence used to be spelled and never spoken.
    let answer = |numbers: &[usize]| -> std::collections::BTreeMap<usize, String> {
        numbers
            .iter()
            .filter(|n| **n != 2)
            .map(|n| (*n, format!("uebersetzt {n}")))
            .collect()
    };
    let got = drive(
        "s5-rules-left",
        Produce {
            translate: vec!["de".to_string()],
            ..Default::default()
        },
        vec![seg(0.0, 10.0), seg(10.0, 20.0)],
        Box::new(answer),
        words(LOUD, 0.0),
        "en",
    );
    assert_eq!(
        got.ran.failed, None,
        "a hole costs a warning, not the video: {:?}",
        got.ran.failed
    );
    let warn = left_line(&[2]);
    assert_eq!(
        warn,
        "!!! subtitles: de: 1 line(s) left in the original (2) -- the rest of the track is good"
    );
    assert!(
        got.logs.contains(&warn),
        "§6's sentence, on the run's own log: {:?}",
        got.logs
    );
    // And the file agrees with the log: line 2 is the English that was already there.
    let sheet = std::fs::read_to_string(got.tree.final_srt(Some("de"))).expect("the German sheet");
    assert!(
        sheet.contains("lecture"),
        "the original line survives: {sheet}"
    );
    assert!(!sheet.contains("uebersetzt 2"), "{sheet}");
    let _ = std::fs::remove_dir_all(&got.root);
}

#[test]
fn sec_08_produce_5_rules_s2_a_tick_named_twice_costs_no_ask() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // "own language never translated": §1 makes that a drawing rule first — `translate_options` never offers
    // the session's own language, so no tick can be clicked for it — and `track_languages` is the second door,
    // for a stored setting that already carries it. A tick that got through both would answer its own numbers
    // and write a second sheet saying the original text in a language-suffixed name.
    assert_eq!(
        naivepost::produce_screen::translate_options("English"),
        ["German", "French"],
        "the page does not even offer the language the viewer hears"
    );
    assert_eq!(
        subs::track_languages("English", &["English".to_string(), "German".to_string()]),
        ["German".to_string()],
        "and a setting that carries it anyway asks for nothing"
    );
    // So the walk over a tick list carrying only the session's own language costs no ask and writes no
    // language-suffixed sheet; the bare stem still holds the original.
    let asks = Rc::new(RefCell::new(0usize));
    let seen = asks.clone();
    let answer = move |numbers: &[usize]| {
        *seen.borrow_mut() = 1;
        all_translated(numbers)
    };
    // A tick naming the language the cues are already in is what `track_languages` catches; a blank names no
    // track at all and goes through the same filter. Neither may cost an ask or write a suffixed sheet.
    // §1's second door, on the walk: a tick naming the language the cues are already in must cost no ask and
    // write no suffixed sheet. The page cannot produce such a tick (`translate_options` hides it); a
    // `naivepost.json` written by an older build can, and this is what catches it.
    let got = drive(
        "s5-rules-own-language",
        Produce {
            // The fixture project's language is `en`. The list also carries a real translation, which must
            // still happen, and a blank, which names no track at all.
            translate: vec!["en".to_string(), "German".to_string(), String::new()],
            ..Default::default()
        },
        vec![seg(0.0, 10.0)],
        Box::new(answer),
        words(LOUD, 0.0),
        "en",
    );
    assert_eq!(got.ran.failed, None, "{:?}", got.ran.failed);
    assert_eq!(
        asks.borrow().clone(),
        1,
        "the one real translation was asked for"
    );
    assert!(
        got.tree.final_srt(Some("German")).is_file(),
        "the language that IS a translation gets its sheet"
    );
    assert!(
        !got.tree.final_srt(Some("en")).is_file(),
        "the session's own language gets none: it is the bare stem already"
    );
    // The only LANGUAGE sheet announced beside the video is German's; `en` and the blank tick name nothing.
    // (The bare stem is announced too — that is the track the viewer hears, §S9.)
    let said: Vec<String> = got
        .logs
        .iter()
        .filter(|l| l.starts_with(">>> subtitles: final."))
        .cloned()
        .collect();
    assert_eq!(
        said,
        [
            render::sidecar_log("final", None),
            render::sidecar_log("final", Some("German"))
        ],
        "{:?}",
        got.logs
    );
    let bare = std::fs::read_to_string(got.tree.final_srt(None)).expect("the bare-stem sheet");
    assert!(
        bare.contains("the") && bare.contains("lecture"),
        "the bare track is the original: {bare}"
    );
    // And on disk the same story: every `.srt` beside the video is either the bare stem or German's.
    let produced_dir = got.tree.final_video("mp4").parent().unwrap().to_path_buf();
    let strays: Vec<String> = std::fs::read_dir(&produced_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| {
                    n.ends_with(".srt")
                        && *n != "final.srt"
                        && *n != render::sidecar_names("final", Some("German"))[0]
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(strays.is_empty(), "no self-translation file: {strays:?}");
    let _ = std::fs::remove_dir_all(&got.root);
}

#[test]
fn sec_08_produce_5_rules_s2_a_webm_says_where_its_subtitles_are_after_the_mux() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // §5's "webm carries no subtitle track" and §6's "after a webm mux: …". The tick survives the container
    // rule only as an absent track, so the log has to say the subtitles are the files BESIDE the video, or
    // an absent track reads as a broken render. It goes after the mux — the video is finished by then.
    let webm = drive(
        "s5-rules-webm",
        Produce {
            container: Container::Webm,
            ..Default::default()
        },
        vec![seg(0.0, 10.0)],
        Box::new(all_translated),
        words(LOUD, 0.0),
        "en",
    );
    assert_eq!(webm.ran.failed, None, "{:?}", webm.ran.failed);
    assert!(webm.ran.muxed, "the note belongs to a finished video");
    let at = webm
        .logs
        .iter()
        .rposition(|l| l.contains("final.webm") || l.contains("mux"))
        .unwrap_or_else(|| panic!("the mux says what it did: {:?}", webm.logs));
    let note = webm
        .logs
        .iter()
        .position(|l| *l == naivepost::produce_details::WEBM_SRT_LOG)
        .expect("webm cannot carry an srt track — the subtitles are the files beside the video");
    assert!(note > at, "the note comes after the mux: {:?}", webm.logs);
    // The cue sheet still exists beside the video, which is what the note points at.
    assert!(
        webm.tree
            .clips_dir()
            .join(render::final_srt_name())
            .is_file(),
        "the files the note names are real"
    );

    // mp4 carries its track, so it says nothing: the same check on an mp4 run must find no note at all.
    let mp4 = drive(
        "s5-rules-mp4",
        Produce::default(),
        vec![seg(0.0, 10.0)],
        Box::new(all_translated),
        words(LOUD, 0.0),
        "en",
    );
    assert_eq!(mp4.ran.failed, None, "{:?}", mp4.ran.failed);
    assert!(
        !mp4.logs.iter().any(|l| l.contains("webm")),
        "a container that carries its track gets no lecture: {:?}",
        mp4.logs
    );
    let _ = std::fs::remove_dir_all(&webm.root);
    let _ = std::fs::remove_dir_all(&mp4.root);
}

#[test]
fn sec_08_produce_5_rules_s2_a_run_with_nothing_to_say_explains_nothing() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // The webm note is about a track the person expected. With no cues anywhere there is nothing beside the
    // video either, and "!!! nothing to put in a subtitle" is the whole of what should be said.
    // The emptiness is a run whose clips have nothing to say: no narration line and no words of their own.
    let silent = drive(
        "s5-rules-webm-silent",
        Produce {
            container: Container::Webm,
            ..Default::default()
        },
        vec![seg(0.0, 10.0)],
        // …which this check gets by answering nothing: no cue means no batch, so the leg is never asked.
        Box::new(|numbers: &[usize]| {
            let _ = numbers;
            std::collections::BTreeMap::new()
        }),
        Vec::new(),
        "en",
    );
    assert_eq!(silent.ran.failed, None, "{:?}", silent.ran.failed);
    assert!(
        silent
            .logs
            .iter()
            .any(|l| *l == render::no_subtitle_cue_log()),
        "the emptiness is said: {:?}",
        silent.logs
    );
    assert!(
        !silent
            .logs
            .iter()
            .any(|l| *l == naivepost::produce_details::WEBM_SRT_LOG),
        "and nothing else: {:?}",
        silent.logs
    );
    assert!(
        !silent
            .logs
            .iter()
            .any(|l| l.starts_with(">>> subtitles: final.srt")),
        "with no cues there is no sidecar to announce either: {:?}",
        silent.logs
    );
    let _ = std::fs::remove_dir_all(&silent.root);
}
