//! §04-prepare#6 — the strings Prepare writes, confirmed against the code.
//!
//! Every line here is a sentence a user or a log reader sees when something goes wrong or finishes, and
//! each one was written because a vague version of it sent somebody looking in the wrong place. So these
//! tests hold the spellings: not that a message exists, but that it names the model, the count and the
//! remedy the way §6 says it does. The behaviour behind them belongs to the flow tests; what is pinned
//! here is what the pass says out loud.

use std::path::PathBuf;

use naivepost::bench;
use naivepost::fix_transcripts as fix;
use naivepost::prepare;
use naivepost::prepare_data;
use naivepost::project::Project;
use naivepost::requests::Word;
use naivepost::settings::{self, Paths};
use naivepost::textfmt::{self, Line, SessionLine};
use naivepost::tools::describe;
use naivepost::tools::retakes;
use naivepost::tools::textedit::{self, Side};
use naivepost::transcribe as transcribe;

/// A settings folder of our own: the bench writes prompts through `Paths`, and a test that
/// reached for the real `~/.config/naivepost` would edit what the person running it typed.
struct PromptDirs {
    root: PathBuf,
    paths: Paths,
}

impl PromptDirs {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "np-p6-{label}-{pid}-{n}",
            pid = std::process::id(),
            n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        Self { root, paths }
    }

    /// `prompts/<key>.txt`, the one-file-per-job layout.
    fn prompt(&self, key: &str) -> PathBuf {
        self.paths.config_dir.join("prompts").join(format!("{key}.txt"))
    }

    /// The old layout's file for one job: `prompts/<key>/<name>`.
    fn legacy(&self, key: &str, name: &str) -> PathBuf {
        self.paths.config_dir.join("prompts").join(key).join(name)
    }
}

static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

const BASE: &str = "lecture";
const OTHER: &str = "crowd";

fn word(text: &str, start_sample: u64, end_sample: u64) -> Word {
    Word {
        word: text.to_string(),
        start_sample,
        end_sample,
    }
}

// --- S7: the two hard failures of segments -------------------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s7_words_without_usable_times_are_refused_by_count() {
    // A word whose end is not after its start cannot be placed, and every cut point is put on a word
    // edge — so this is refused rather than skipped quietly. The count is in the message because "some"
    // sends a reader back to the file to count them.
    let words = [
        word("good", 0, 16_000),
        word("zero", 32_000, 32_000),
        word("backwards", 48_000, 40_000),
    ];
    assert_eq!(transcribe::unusable_words(&words), 2);
    assert_eq!(
        transcribe::unusable_words_error(2),
        "2 words carry no usable start_sample/end_sample -- the answer changed shape"
    );

    // A whole transcript of usable times is not a failure at all.
    assert_eq!(transcribe::unusable_words(&[word("fine", 0, 8_000)]), 0);
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s7_no_word_times_names_the_remedy_and_the_variant_points_upwards() {
    // No aligner registered: the remedy is a setting, so the message names the task to register.
    assert_eq!(
        transcribe::no_word_times("whisper.cpp", false),
        "whisper.cpp transcribed this recording but timed no words, no aligner is registered to time them -- register one (task \"align\") -- or use an ASR that answers with word timings"
    );

    // An aligner was asked and left no times either: why is the warning directly above this line, so
    // repeating it here would let the two drift apart.
    assert_eq!(
        transcribe::no_word_times("qwen3-asr", true),
        "qwen3-asr transcribed this recording but timed no words, and the aligner left no times either, which the line above this one says why"
    );
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s7_transcript_srt_tags_the_speaker_unless_it_is_unknown() {
    // `transcript.srt` puts the speaker in brackets in front of each cue, and drops the tag for an
    // unknown one: "[?]" would be shown as somebody's name.
    let known = Line {
        start: 0.0,
        end: 2.0,
        speaker: "SPEAKER_00".into(),
        text: "hello".into(),
    };
    let srt = transcribe::srt_text(&[known.clone()]);
    assert!(srt.contains("[SPEAKER_00] hello"), "{srt}");

    let unknown = Line {
        speaker: transcribe::UNKNOWN.into(),
        ..known
    };
    let blind = transcribe::srt_text(&[unknown]);
    assert!(!blind.contains('['), "an unknown speaker gets no tag: {blind}");
}

// --- S8: meta.env's conditionals ------------------------------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s8_meta_env_holds_the_video_pair_only_for_footage() {
    // INTERVAL and SCALE are always there; VIDEO_FILE/BASE only when there is footage. §1's own list of
    // keys, in order, so a script reading the file finds what it expects where it expects it.
    assert_eq!(
        prepare_data::META_KEYS,
        ["INTERVAL", "SCALE", "VIDEO_FILE", "VIDEO_BASE", "AUDIO_FILE", "AUDIO_BASE"]
    );

    let mut project = naivepost::project::Project::default();
    project.interval = 0.25;
    let keys = |sources: Vec<naivepost::project::Source>| {
        let mut project = project.clone();
        project.sources = sources;
        prepare_data::meta(&project, std::path::Path::new("/s"), std::path::Path::new("/p"))
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>()
    };

    // A recording with footage puts the video pair in; a recording tagged narrator 1 puts the audio pair
    // in beside it. Neither is ever present-but-empty — a script reading this file asks for the key, and an
    // empty value would read as a path that exists.
    let with_footage = keys(vec![video_source(BASE, 1), audio_source(OTHER, 2)]);
    assert_eq!(
        with_footage,
        vec!["INTERVAL", "SCALE", "VIDEO_FILE", "VIDEO_BASE", "AUDIO_FILE", "AUDIO_BASE"],
        "footage plus a narrator's microphone means all six keys"
    );

    // Footage alone: the audio pair is absent because nobody is tagged as the narrator, not blank.
    let footage_only = keys(vec![video_source(BASE, 0), audio_source(OTHER, 0)]);
    assert_eq!(footage_only, vec!["INTERVAL", "SCALE", "VIDEO_FILE", "VIDEO_BASE"]);

    // No footage at all: the video pair goes, INTERVAL and SCALE stay.
    let audio_only = keys(vec![audio_source(BASE, 1)]);
    assert_eq!(audio_only, vec!["INTERVAL", "SCALE", "AUDIO_FILE", "AUDIO_BASE"]);
}

fn video_source(name: &str, voice: u32) -> naivepost::project::Source {
    naivepost::project::Source {
        path: format!("project:{name}.mkv"),
        footage: true,
        narrator: voice,
        ..Default::default()
    }
}

fn audio_source(name: &str, voice: u32) -> naivepost::project::Source {
    naivepost::project::Source {
        path: format!("project:{name}.wav"),
        footage: false,
        narrator: voice,
        ..Default::default()
    }
}

/// One footage/recording pairing for `offsets.tsv`, which reads the clock rather than a project.
fn pair(base: &str, file_name: &str, start: f64, video: bool) -> fix::Source {
    fix::Source {
        base: base.into(),
        file_name: file_name.into(),
        start,
        duration: 60.0,
        is_video: video,
        is_commentary: !video,
    }
}

// --- S9: describe's speech block and its filing rules ---------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s9_the_speech_block_always_shows_all_three_headings() {
    // A model told about a section it cannot see answers better than one left to wonder, so an empty
    // section still gets its heading and a word for what is missing.
    assert_eq!(
        describe::SPEECH_HEADINGS,
        [
            "--- context before (do not describe) ---",
            "--- spoken during these frames ---",
            "--- context after (do not describe) ---",
        ]
    );

    let empty = describe::speech_block(&[], &[], &[]);
    assert_eq!(
        empty,
        "--- context before (do not describe) ---\n(none)\n\
         --- spoken during these frames ---\n(no speech during these frames)\n\
         --- context after (do not describe) ---\n(none)\n"
    );

    // Silence over the frames is an observation about the video, so it does not borrow `(none)` — that
    // one means "no data here", this one means "nobody spoke".
    assert_eq!(describe::NO_SPEECH, "(none)");
    assert_eq!(describe::NO_SPEECH_DURING, "(no speech during these frames)");

    let filled = describe::speech_block(
        &["[+2.0s] SPEAKER_00: before".into()],
        &["[0.5s] SPEAKER_00: during".into()],
        &[],
    );
    assert!(filled.contains("--- spoken during these frames ---\n[0.5s] SPEAKER_00: during\n"), "{filled}");
    assert!(filled.ends_with("(none)\n"), "the empty tail still says so: {filled}");
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s9_a_reply_with_no_label_is_filed_as_no_event_line() {
    // No `EVENT` anywhere: the words were the description, which is what they were every time this
    // happened — but events.tsv has to say so, or a reader takes the line for an event that lost its stamp.
    let note = describe::no_event_line("A slide of a chart appears on screen");
    assert_eq!(note, "(no event line: A slide of a chart appears on screen)");

    // Newlines folded, so one reply is one row of the file.
    assert_eq!(describe::no_event_line("a\n b  c"), "(no event line: a b c)");

    // Long replies are shortened to what identifies the call, with the tail marked off rather than
    // silently cut — a reader has to be able to tell a quote from an excerpt. The `…` closes the bracket,
    // which is why this checks the whole string's shape rather than its last character.
    let long = describe::no_event_line(&"word ".repeat(60));
    assert!(long.starts_with("(no event line: word word"), "{long}");
    assert!(long.ends_with("…)"), "the tail is marked off, not silently cut: {long}");

    // The parser reaches this outcome through UnlabeledDescription; the two agree on which replies it is.
    let parsed = describe::parse_event_line("just prose here", 1.0, &[10.0, 11.0]);
    assert!(matches!(parsed, describe::ParseOutcome::UnlabeledDescription(_)), "{parsed:?}");
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s9_an_old_shape_event_belongs_to_the_first_frame() {
    // One EVENT for the whole batch, in the prototype's shape, has no stamp to place it by. The batch's
    // first frame is the only frame it can be about: naming any later one would date a change to a second
    // the answer never mentioned.
    assert_eq!(describe::OLD_SHAPE_FRAME, 1);

    // And the line §3.2 says the prototype force-wrote for a batch's opening frame with nothing before it.
    assert_eq!(describe::Batch::SAME_WITHOUT_HISTORY, "Calm; same view.");
}

// --- S10: fix's two logs, offsets.tsv, session.txt ------------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s10_a_block_out_of_tries_keeps_its_original_lines() {
    // The originals standing is the outcome worth logging: a reader who sees only "failed" looks for a
    // crash, and there was none. Counted from one, because that is how a person counts blocks.
    assert_eq!(
        fix::block_failed_log("lecture", 0, 4),
        ">>> [lecture] block 1/4 failed validation, keeping original lines"
    );
    assert_eq!(
        fix::block_failed_log("lecture", 3, 4),
        ">>> [lecture] block 4/4 failed validation, keeping original lines"
    );

    // And the re-run that asked for nothing says how much it skipped.
    assert_eq!(
        fix::cached_blocks_log("lecture", 7),
        ">>> [lecture] 7 block(s) answered from the cache"
    );
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s10_offsets_tsv_pairs_footage_with_recording_and_says_so() {
    // One row per footage/recording pair, each logged as which recording starts how far into which.
    let rows = fix::offsets_rows(&[pair(BASE, "lecture.mkv", 0.0, true), pair(OTHER, "crowd.wav", -1.0, false)]);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let (video, audio, offset) = &rows[0];
    assert_eq!((video.as_str(), audio.as_str()), (BASE, OTHER));
    assert_eq!(
        fix::offsets_log(video, audio, *offset),
        ">>> offset: lecture starts 1 s into crowd"
    );

    // A fraction keeps only the digits it needs; a whole second loses the `.0` rather than reading as a
    // measurement finer than the clock that produced it.
    assert_eq!(fix::offsets_log(BASE, OTHER, -10.0), ">>> offset: lecture starts -10 s into crowd");
    assert_eq!(fix::offsets_log(BASE, OTHER, 0.25), ">>> offset: lecture starts 0.25 s into crowd");

    // An all-audio session has nothing to pair, so nothing is logged.
    assert!(fix::offsets_rows(&[pair(OTHER, "crowd.wav", 0.0, false)]).is_empty());
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s10_session_txt_carries_both_clocks_and_the_right_label() {
    // `[<start>s-<end>s | mm:ss] LABEL: text` — seconds for the model that reads this file and answers in
    // them, mm:ss for the person scanning it for a moment.
    assert_eq!(
        fix::session_line(0.0, 4.0, "EVENT", "a slide changes"),
        "[0s-4s | 00:00] EVENT: a slide changes"
    );
    assert_eq!(
        fix::session_line(65.5, 70.0, "SPEAKER_01", "hello there"),
        "[65.5s-70s | 01:05] SPEAKER_01: hello there"
    );

    // The narrator's own microphone is labelled by its role; the diariser has never heard anyone else in it.
    assert_eq!(fix::label("crowd", "SPEAKER_00", "crowd"), "NARRATOR");
    assert_eq!(fix::label("stage", "SPEAKER_02", "crowd"), "SPEAKER_02");
    assert_eq!(fix::label("lecture", "EVENT", ""), "EVENT");

    // Whole session: every row stamped, and a marked stretch folded to one line apiece.
    let rows = vec![
        event_row("lecture", 0.0, 4.0, "a slide changes"),
        speech_row("crowd", 5.0, 9.0, "SPEAKER_00", "welcome everyone"),
    ];
    let text = fix::session_text(&rows, "crowd", &[]);
    assert_eq!(
        text,
        "[0s-4s | 00:00] EVENT: a slide changes\n[5s-9s | 00:05] NARRATOR: welcome everyone\n"
    );
}

fn event_row(source: &str, start: f64, end: f64, text: &str) -> SessionLine {
    SessionLine {
        start,
        end,
        source: source.into(),
        who: "EVENT".into(),
        text: text.into(),
    }
}

fn speech_row(source: &str, start: f64, end: f64, who: &str, text: &str) -> SessionLine {
    SessionLine {
        start,
        end,
        source: source.into(),
        who: who.into(),
        text: text.into(),
    }
}

// --- S11: retakes' four lines ---------------------------------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s11_a_lost_run_is_set_aside_and_the_pass_goes_on() {
    // Three runs are pooled because they cost one request each and answer one question. Losing one is not
    // a reason to lose what the other two agreed on, so the pass says which went and how many remain —
    // the prototype threw all three away here.
    let logs = retakes::run_set_aside_log(1, "no such line 40", 2);
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0], "!!! retakes: run 2: no such line 40 -- its answer is set aside");
    assert_eq!(logs[1], ">>> retakes: going on with 2");

    // Counted from one, because that is how a reader counts runs in a log.
    assert_eq!(retakes::run_set_aside_log(0, "x", 2)[0], "!!! retakes: run 1: x -- its answer is set aside");
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s11_cache_and_silence_are_both_said_out_loud() {
    assert_eq!(
        retakes::cached_runs_log(2, 3),
        ">>> retakes: 2 of 3 run(s) answered from the cache"
    );

    // Nothing said twice is the model's answer, not an absence of one — and silence in the log reads as a
    // pass that never ran.
    assert_eq!(retakes::no_retakes_log(), ">>> retakes: none");
}

// --- S12: joins -----------------------------------------------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s12_a_stretch_beyond_seam_snap_says_which_way_it_missed() {
    // P.machine.seamSnapWords: within it a stretch is still at the join, past it the two mistakes are
    // named apart because the model corrects them in opposite directions.
    assert_eq!(textedit::seam_snap(Side::Before, 0), Ok(()));
    assert_eq!(textedit::seam_snap(Side::Before, textedit::SEAM_SNAP_WORDS), Ok(()));
    assert_eq!(
        textedit::seam_snap(Side::Before, textedit::SEAM_SNAP_WORDS + 2),
        Err("the stretch left out stops 5 words short of the join".into())
    );
    assert_eq!(
        textedit::seam_snap(Side::After, textedit::SEAM_SNAP_WORDS + 1),
        Err("the stretch left out starts 4 words past the join".into())
    );
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s12_the_join_pass_counts_joins_not_words() {
    // Joins are the unit a reader checks the video against, and the cache half matters: a re-run that
    // asked everything again looks the same as one that remembered nothing.
    assert_eq!(
        textedit::joins_repaired_log(3, 1),
        ">>> text edit: 3 join(s) repaired, 1 from the cache"
    );
    assert_eq!(
        textedit::joins_repaired_log(0, 0),
        ">>> text edit: 0 join(s) repaired, 0 from the cache"
    );
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s12_each_mark_gets_a_line_and_a_dedupe_note_names_the_earlier_copy() {
    // One line per mark, spelled by `hand_edit.rs`'s own format string — `">>> text edit: {}-{} goes
    // ({:?})"` with `mm_ss` at both ends and the words in quotes — because that line is what a person
    // reads back to find the splice again on the timeline.
    let mark_line = format!(
        ">>> text edit: {}-{} goes ({:?})",
        naivepost::tools::mm_ss(62.0),
        naivepost::tools::mm_ss(65.0),
        "so so"
    );
    assert_eq!(mark_line, ">>> text edit: 01:02-01:05 goes (\"so so\")");

    // The dedupe note quotes the later saying and says which copy goes — the earlier one, because what was
    // said last is what the speaker meant to keep.
    assert_eq!(
        textedit::dedupe_note(70.0, "so so then"),
        "70: \"so so then\" said again straight after the cut -- the earlier one goes"
    );
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s12_one_recording_still_writes_both_files() {
    // A single take has no retakes, and still writes both files: final.txt with every word in it and an
    // empty retakes.tsv. The empty file is what tells Cut the pass ran — leaving it out makes a clean
    // recording indistinguishable from one that was never marked.
    let root = std::env::temp_dir().join(format!("naivepost-details-strings-{}", std::process::id()));
    let dir = root.join("talk.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    let tree = naivepost::layout::Tree::new(&dir).expect("a .naivepost folder is a project");

    let words = vec!["welcome".to_string(), "everyone".to_string()];
    textfmt::write_final(&words, &[]);
    let final_text = textfmt::write_final(&words, &[]);
    assert_eq!(final_text, "welcome everyone", "every word, no cut marker: {final_text}");

    textfmt::write_retakes(&[], &tree.retakes_tsv()).unwrap();
    assert_eq!(textfmt::NO_MARKS, "");
    assert_eq!(std::fs::read_to_string(tree.retakes_tsv()).unwrap(), "");
    std::fs::remove_dir_all(&root).ok();
}

// --- S13-S15: the prompt bench's tooltips ---------------------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s13_the_edit_mark_names_the_folder_that_holds_the_wording()
{
    // "the mark's tooltip \"Your wording is kept in ~/.config/naivepost/prompts, so a newer
    // built-in prompt will not replace it. Reset puts it back.\"" — the sentence has to name the
    // folder, because "kept in your settings" is where an editor looks for a file and finds none.
    assert_eq!(
        bench::KEPT_TIP,
        "Your wording is kept in ~/.config/naivepost/prompts, so a newer built-in prompt will not replace it. Reset puts it back."
    );
    assert!(
        bench::KEPT_TIP.contains("~/.config/naivepost/prompts"),
        "the mark has to say where the wording lives: {}",
        bench::KEPT_TIP
    );
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s14_reset_says_what_it_does_under_each_state() {
    // "Reset live only while this machine holds an edit (\"Put the built-in wording back\" /
    // \"This is the built-in wording, unchanged\")".
    assert_eq!(bench::reset_tip(&bench::ROWS[1], true), "Put the built-in wording back");
    assert_eq!(
        bench::reset_tip(&bench::ROWS[1], false),
        "This is the built-in wording, unchanged"
    );
    // The User Context holds text and still has nothing to restore: it ships no wording, so a live
    // Reset there would offer to throw the editor's own notes away.
    assert_eq!(
        bench::reset_tip(&bench::ROWS[0], true),
        "This is the built-in wording, unchanged"
    );
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s15_every_row_says_what_the_job_gets_and_answers() {
    for (index, _) in bench::picker_rows() {
        let row = &bench::ROWS[index];
        let tip = bench::tooltip(row);
        assert!(!tip.is_empty(), "{} says nothing", row.title);
        assert!(tip.chars().count() > 40, "{} is a label, not an explanation: {tip}", row.title);
        for slipped in ["%d", "{}", "\\n"] {
            assert!(!tip.contains(slipped), "{} carries its template: {tip}", row.title);
        }
    }
    // The two rows whose wording is arithmetic name the numbers a request is really built to.
    let describe_tip = bench::tooltip(&bench::ROWS[2]);
    for number in ["4 ", "3 ", "2 "] {
        assert!(describe_tip.contains(number), "describe's arithmetic: {describe_tip}");
    }
    assert!(bench::tooltip(&bench::ROWS[3]).contains("25 "), "the fixer's block size");
}

// --- S16-S17: the prompt files this machine may have inherited ------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s16_an_edit_kept_under_the_old_layout_is_adopted_once()
{
    // "legacy `prompts/<key>/General.txt|Default.txt` are read once and rewritten as
    // `prompts/<key>.txt`" — the rewrite is what the bench's own store does, so adopting must not
    // look like an edit that arrived twice, and it must not eat the old file on the way.
    let d = PromptDirs::new("adopt");
    std::fs::create_dir_all(d.legacy("cut", "General.txt").parent().unwrap()).unwrap();
    std::fs::write(d.legacy("cut", "General.txt"), "what four videos tuned\n").unwrap();

    assert_eq!(
        settings::stored_prompt(&d.paths, "cut").unwrap(),
        Some("what four videos tuned".to_string()),
        "the trailing newline is the file's, not the wording's"
    );
    let cut = &bench::ROWS[6];
    assert_eq!(cut.key, "cut");
    assert!(bench::edited(&d.paths, cut), "an inherited edit still shows its mark");

    let mut project = Project::default();
    bench::store(&d.paths, &mut project, cut, "what four videos tuned").unwrap();
    assert_eq!(std::fs::read_to_string(d.prompt("cut")).unwrap(), "what four videos tuned");
    assert!(d.legacy("cut", "General.txt").is_file(), "the old copy is left where it was");

    // A second pass over the same text changes nothing: no double adoption, no vanished folder.
    bench::store(&d.paths, &mut project, cut, "what four videos tuned").unwrap();
    assert_eq!(
        settings::stored_prompt(&d.paths, "cut").unwrap(),
        Some("what four videos tuned".to_string())
    );
    assert!(bench::edited(&d.paths, cut));
    assert_eq!(
        std::fs::read_dir(d.legacy("cut", "General.txt").parent().unwrap())
            .unwrap()
            .flatten()
            .count(),
        1,
        "the old folder keeps exactly the file it had"
    );
    std::fs::remove_dir_all(&d.root).ok();
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s17_an_empty_prompt_file_is_no_prompt() {
    // "an empty prompt file is no prompt" — a wording that says nothing would send the model no
    // system prompt at all, so it reads as absent: no mark, and the shipped text in the box.
    let d = PromptDirs::new("empty");
    std::fs::create_dir_all(d.legacy("fix", "Default.txt").parent().unwrap()).unwrap();
    std::fs::write(d.prompt("cut"), "").unwrap();
    std::fs::write(d.legacy("fix", "Default.txt"), "   \n").unwrap();

    assert_eq!(settings::stored_prompt(&d.paths, "cut").unwrap(), None);
    assert_eq!(settings::stored_prompt(&d.paths, "fix").unwrap(), None);
    let project = Project::default();
    for index in [3, 6] {
        let row = &bench::ROWS[index];
        assert!(!bench::edited(&d.paths, row), "{} claims an edit it has not got", row.title);
        assert_eq!(bench::text(&d.paths, &project, row), "", "{}: no wording yet", row.title);
    }
    std::fs::remove_dir_all(&d.root).ok();
}

// --- S18-S19: the bottom bar's two readouts --------------------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s18_no_sources_says_where_to_add_them() {
    // "\"no input files \u{2014} add some above\" (tooltip \"(no sources)\")" — the label points at the
    // Add button, because the bottom bar has nothing to click.
    assert_eq!(prepare::NO_INPUTS, "no input files \u{2014} add some above");
    assert_eq!(prepare::NO_INPUTS_TIP, "(no sources)");
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s19_the_outputs_readout_counts_and_stays_quiet_about_an_unmeasured_age()
{
    // Outputs "\"nothing yet\" / \"N files, size\" with \"newest <ago>\"".
    assert_eq!(prepare::outputs_readout(0, 0, None), ("nothing yet".to_string(), String::new()));

    let (label, tip) = prepare::outputs_readout(3, 1_258_291, Some(5));
    assert_eq!(label, "3 files, 1.2 MB");
    assert_eq!(tip, "newest 5 min ago");

    let (label, tip) = prepare::outputs_readout(1, 12, Some(0));
    assert_eq!(label, "1 file, 12 B", "one file is not several files");
    assert_eq!(tip, "newest just now");

    // The ladder: minutes give way to hours at an hour, hours to days at a day.
    assert_eq!(prepare::outputs_readout(2, 2048, Some(300)).1, "newest 5 h ago");
    assert_eq!(prepare::outputs_readout(2, 2048, Some(30 * 60)).1, "newest yesterday");
    assert_eq!(prepare::outputs_readout(2, 2048, Some(3 * 24 * 60)).1, "newest 3 days ago");

    // Files with no measured age: the count is a fact, an invented age is not.
    assert_eq!(prepare::outputs_readout(4, 1024, None).1, "");
}
