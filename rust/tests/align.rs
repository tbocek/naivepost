//! F1.5 Forced alignment — spec/04-prepare.md.
//!
//! Which aligner, what each request covers, and what `words.aligned.json` ends up holding all live in
//! [`naivepost::align`]; ▶ only hands it ffmpeg's cut and the audio box. So these checks assert which
//! models were asked, in what order, over which seconds and with what text — never that a server
//! answered (spec/00-principles.md §5).

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use naivepost::align::{self, Span, MIN_PIECE, PAD, WINDOW};
use naivepost::asr::{self, Silence};
use naivepost::layout::Tree;
use naivepost::requests::{self, Chunk, Word};
use naivepost::transcribe::seconds;
use naivepost::services::AudioModel;

const SOURCE: &str = "mic.wav";
const URL: &str = "http://box:8000/v1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/naivepost-align-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder with the one file this flow reads. Its contents do not matter — the caller hands
/// over the duration and the silences, as ▶ does after probing.
fn project(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("talk.naivepost");
    fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    fs::create_dir_all(tree.input_dir(SOURCE)).unwrap();
    (root, tree)
}

fn model(id: &str, task: &str) -> AudioModel {
    AudioModel {
        id: id.to_string(),
        family: String::new(),
        task: task.to_string(),
    }
}

/// One word of an answer, in the clip's own seconds.
fn word(text: &str, from: f64, to: f64) -> Value {
    json!({"word": text, "start": from, "end": to})
}

/// A recording with no silences at all: everything in it is talking.
fn loud(duration: f64) -> Vec<Silence> {
    let _ = duration;
    Vec::new()
}

/// The audio box, as a test answers it. Every ask is recorded as `(model, clip, text)` so the order,
/// the models and the seconds a request covered are all assertable, and `refuse` names the models that
/// answer with an error instead of words.
#[derive(Clone)]
struct Box_ {
    asked: Arc<Mutex<Vec<(String, String, String)>>>,
    /// Models that refuse this window; anything else answers with one word per second of clip.
    refuses: Vec<String>,
    /// Answers to hand back verbatim, in the order the windows are asked about.
    scripted: Vec<Value>,
    /// What every ask answers with when nothing is scripted for it.
    words: Value,
    /// S5: whether `words.aligned.json` existed at each ask — written whole at the end or not.
    mid_run: Arc<Mutex<Vec<bool>>>,
    aligned_path: PathBuf,
}

impl Box_ {
    fn new(aligned_path: PathBuf) -> Self {
        Self {
            asked: Arc::new(Mutex::new(Vec::new())),
            refuses: Vec::new(),
            scripted: Vec::new(),
            words: json!({"words": [word("hello", 0.5, 1.0)]}),
            mid_run: Arc::new(Mutex::new(Vec::new())),
            aligned_path,
        }
    }

    /// Refuse every ask for these models with this error.
    fn refusing(mut self, models: &[&str], err: &str) -> Self {
        let _ = err;
        self.refuses = models.iter().map(|m| m.to_string()).collect();
        self
    }

    /// Answer each window in order with exactly this body.
    fn scripted(mut self, answers: Vec<Value>) -> Self {
        self.scripted = answers;
        self
    }

    /// Answer every window with this body.
    fn always(mut self, answer: Value) -> Self {
        self.words = answer;
        self
    }

    fn calls(&self) -> Vec<(String, String, String)> {
        self.asked.lock().unwrap().clone()
    }

    /// The seconds each clip was cut at, read back out of its name (`w000650000.wav` → 650.0).
    fn clips(&self) -> Vec<f64> {
        self.calls()
            .iter()
            .map(|(_, clip, _)| clip[1..clip.len() - 4].parse::<f64>().unwrap() / 1000.0)
            .collect()
    }

    fn mid_run(&self) -> Vec<bool> {
        self.mid_run.lock().unwrap().clone()
    }

    fn ask(&mut self, model: &str, clip: &str, text: &str) -> Result<Value, String> {
        let index = {
            let mut asked = self.asked.lock().unwrap();
            asked.push((model.to_string(), clip.to_string(), text.to_string()));
            asked.len() - 1
        };
        self.mid_run
            .lock()
            .unwrap()
            .push(self.aligned_path.is_file());
        if self.refuses.iter().any(|refused| refused == model) {
            return Err("unsupported model family hint".into());
        }
        Ok(match self.scripted.get(index) {
            Some(answer) => answer.clone(),
            None if self.scripted.is_empty() => self.words.clone(),
            None => self.words.clone(),
        })
    }
}

/// S1: a name in the box is the whole list when the server declares it for `align`.
#[test]
fn f1_5_s1_the_configured_id_is_the_whole_list_when_it_is_declared_for_align() {
    let models = [model("qwen3-aligner", "align"), model("whisper", "asr")];
    assert_eq!(
        align::aligners("qwen3-aligner", URL, &models).unwrap(),
        vec!["qwen3-aligner".to_string()]
    );
    // A model declared for another task is not an aligner, however good it is at that other thing.
    let models = [model("qwen3-aligner", "asr")];
    assert!(align::aligners("qwen3-aligner", URL, &models).is_err());
}

/// S1: a configured id that is not served for alignment is not replaced — the refusal names the
/// server and the id, says where cut points come from instead, and never mentions another model.
#[test]
fn f1_5_s1_a_configured_id_that_is_not_served_skips_alignment_for_the_run() {
    let models = [model("mms-aligner", "align"), model("whisper", "asr")];
    let err = align::aligners("qwen3-aligner", URL, &models).unwrap_err();
    assert_eq!(
        err,
        format!("!!! align: {URL} does not serve \"qwen3-aligner\" for alignment -- cut points come off the waveform")
    );
    assert!(!err.contains("mms-aligner"), "{err}");

    // Through the driver: nothing is asked, nothing is written, and the log is that one line.
    let (_root, tree) = project("s1-skipped");
    let mut agent = Box_::new(tree.words_aligned_json(SOURCE));
    // `aligners` came back Err: nothing is served for alignment, so the run skips it — one log line
    // from the caller holding that refusal, nothing asked, and no marker left behind.
    let outcome = align::run(
        &tree,
        SOURCE,
        60.0,
        "en",
        &loud(60.0),
        &[],
        &[],
        "one two three",
        &[],
        |model, clip, text| agent.ask(model, clip, text),
    )
    .expect("no aligner is a working setup");
    assert!(outcome.skipped);
    assert!(outcome.logs.is_empty(), "{:?}", outcome.logs);
    assert!(agent.calls().is_empty());
    assert!(!tree.words_aligned_json(SOURCE).exists());
}

/// S1: with an empty box every model declared for `align` is worth trying, the preferred one first —
// two aligners sorted plainly put the weaker one first because its name is shorter.
#[test]
fn f1_5_s1_an_empty_box_offers_every_align_model_preferencing_qwen3() {
    let models = [
        model("aaa-aligner", "align"),
        model("qwen3-aligner", "align"),
        model("mms-aligner", "align"),
        model("whisper", "asr"),
        model("bs-roformer", "sep"),
    ];
    assert_eq!(
        align::aligners("", URL, &models).unwrap(),
        vec![
            "qwen3-aligner".to_string(),
            "aaa-aligner".to_string(),
            "mms-aligner".to_string()
        ]
    );
    // Nothing declared for `align` is nothing to try.
    assert!(align::aligners("", URL, &[model("whisper", "asr")]).unwrap().is_empty());
}

/// S1: the first aligner to answer serves the rest of the run — a catalog entry is a claim rather than
/// a working model, so the one that answered is remembered and the others are never asked again.
#[test]
fn f1_5_s1_the_first_to_answer_serves_the_rest_of_the_run() {
    let (_root, tree) = project("s1-picker");
    let mut agent = Box_::new(tree.words_aligned_json(SOURCE))
        .refusing(&["mms-aligner"], "unsupported model family hint");
    let all = vec!["mms-aligner".to_string(), "qwen3-aligner".to_string()];
    // Two windows: 60 s apart, so neither is over the limit and nothing splits.
    let chunks = vec![
        Chunk { s: 0.0, e: 60.0, text: "one two three".into() },
        Chunk { s: 60.0, e: 120.0, text: "four five six".into() },
    ];
    let outcome = align::run(
        &tree,
        SOURCE,
        120.0,
        "en",
        &loud(120.0),
        &chunks,
        &[],
        "one two three four five six",
        &all,
        |model, clip, text| agent.ask(model, clip, text),
    )
    .expect("the second aligner answered");

    let calls = agent.calls();
    // Window one walked the list; every window after it asked only the aligner that answered. Four
    // asks for two 60 s pieces: F1.4's ceiling is 300 s, so its chunks are re-cut at this flow's.
    assert_eq!(calls.len(), 4, "{calls:?}");
    assert_eq!(calls[0].0, "mms-aligner");
    assert!(calls[1..]
        .iter()
        .all(|(model, _, _)| model == "qwen3-aligner"), "{calls:?}");
    let log = outcome
        .logs
        .iter()
        .find(|line| line.contains("answers where"))
        .expect("which aligner served the run is worth knowing");
    assert!(log.ends_with("-- using it for the rest of the run"), "{log}");
    assert!(log.contains("mms-aligner"), "{log}");
    assert_eq!(outcome.model, "qwen3-aligner");
    // The refusal of the model that could not do it is in the log too.
    assert!(outcome
        .logs
        .iter()
        .any(|line| line.contains("mms-aligner: unsupported model family hint")));
}

/// S2: the ASR's own pieces are kept when their word counts agree — same seconds, same text, no
/// estimate anywhere. Off by one and the whole split has to be re-derived.
#[test]
fn f1_5_s2_pieces_are_the_asrs_own_chunks_when_the_word_counts_agree() {
    let chunks = vec![
        Chunk { s: 0.0, e: 60.0, text: "one two three".into() },
        Chunk { s: 60.0, e: 120.0, text: "four five".into() },
    ];
    let timed = |secs: &[f64]| -> Vec<Word> {
        secs.iter()
            .map(|at| Word {
                word: "word".into(),
                start_sample: (*at * 16_000.0) as u64,
                end_sample: (*at * 16_000.0) as u64 + 8_000,
            })
            .collect()
    };
    assert!(align::counts_agree(&chunks, &timed(&[1.0, 20.0, 40.0, 70.0, 100.0])));
    // One word's time landing in the wrong chunk is enough: the pieces would misplace text.
    assert!(!align::counts_agree(&chunks, &timed(&[1.0, 20.0, 40.0, 59.0, 100.0])));
    // So is a total that does not add up — an ASR with no word times at all.
    assert!(!align::counts_agree(&chunks, &timed(&[])));
    assert!(!align::counts_agree(&[], &timed(&[1.0])));

    let agreed = timed(&[1.0, 20.0, 40.0, 70.0, 100.0]);
    assert_eq!(
        align::pieces(&chunks, &agreed, 120.0, &[], WINDOW),
        vec![(0.0, 60.0), (60.0, 120.0)]
    );
}

/// S2: otherwise the recording is cut into windows at silences and the text shared out by how much of
/// each window has sound in it.
#[test]
fn f1_5_s2_otherwise_windows_are_cut_at_silences_and_text_shared_by_voiced_seconds() {
    let duration = 640.0;
    let silence = Silence { start: 200.0, end: 210.0 };
    let silences = [silence];
    // No word times means no agreement, so the windows are derived here.
    let pieces = align::pieces(&[], &[], duration, &silences, WINDOW);
    assert_eq!(pieces[0].0, 0.0);
    assert_eq!(pieces.last().unwrap().1, duration);
    // The windows use the ASR's ceiling (so a request carries the words its audio holds) but the
    // aligner's OWN reach -- `P.eng.alignCutSeekSeconds` (4 s, prototype `alignCutSeek`), not the
    // chunker's 20 s. Compared like with like: the same cutter, the same limit, the align reach.
    let asr_edges = asr::cut_points(duration, &silences, asr::CHUNK_QWEN, align::CUT_SEEK);
    for pair in asr_edges.windows(2) {
        assert!(pieces.contains(&(pair[0], pair[1])), "{pieces:?} vs {asr_edges:?}");
    }
    for (from, to) in &pieces {
        assert!(to > from, "no empty piece: {pieces:?}");
        assert!(to - from <= WINDOW + 1e-9, "over the window: {pieces:?}");
    }
    // With the new reach the step is 640 / ceil(640/(60-8)) = 640/13 = 49.2307.., so the nominal
    // wants are 49.23, 98.46, 147.69, 196.92... The fixture's silence [200,210] has midpoint 205.0,
    // which is 8.08 past the nearest want -- outside the 4 s reach (`cut_points` compares strictly
    // `<`), so nothing slid and every edge sits on its nominal multiple. Under the borrowed 20 s reach
    // (trimmed to 10) this silence WOULD have been taken; that is exactly what §10's 4 s replaces.
    let step = duration / 13.0;
    for piece in 1..13 {
        let want = step * piece as f64;
        assert!(
            pieces.iter().any(|(from, _)| (*from - want).abs() < 1e-9),
            "edge {piece} stayed at its nominal {want}: {pieces:?}"
        );
    }
    assert!(
        !pieces.iter().any(|(from, _)| (*from - 205.0).abs() < 1e-9),
        "the silence midpoint 205.0 is out of the align reach and was not slid onto: {pieces:?}"
    );

    // The share each window was asked to align: proportional to the sound in it, and together the
    // whole transcript with nothing lost or duplicated. Both windows hold speech; what differs is how
    // much, and a stretch of silence holds no words however long it is.
    let text = "a b c d e f g h i j k l";
    // Both windows are 60 s of wall clock; what differs is how much of that has speech in it, since a
    // stretch of silence holds no words however long it is. Neither window overlaps the one silence,
    // so the shares are proportional to their lengths and every word has somewhere to go.
    let even = [(0.0, 60.0), (210.0, 270.0)];
    let skewed = [(0.0, 90.0), (210.0, 240.0)];
    let shared = align::share_text(&even, text, &[]);
    let uneven = align::share_text(&skewed, text, &[]);
    assert_eq!(shared[0].split_whitespace().count(), 6, "{shared:?}");
    assert!(
        uneven[0].split_whitespace().count() > shared[0].split_whitespace().count(),
        "the window with more speech in it gets more text: {shared:?} vs {uneven:?}"
    );
    for shares in [&shared, &uneven] {
        assert_eq!(shares.join(" "), text, "{shares:?}");
    }

    // A window with no sound in it at all is not handed words it cannot place: the estimate puts them
    // where there is speech to align. Where nothing has sound, every word still goes somewhere rather
    // than being lost — a silent recording gets an empty transcript anyway.
    let over_silence = [(0.0, 60.0), (200.0, 210.0)];
    let placed = align::share_text(&over_silence, text, &silences);
    assert_eq!(placed[1], "", "{placed:?}");
    // The trailing space is this flow's own joiner between one share and the next; the words are all
    // there, none of them duplicated.
    assert_eq!(placed.concat(), text, "{placed:?}");
    let all_quiet = align::share_text(&[(0.0, 30.0), (200.0, 210.0)], text, &silences);
    // A share of "" contributes nothing, so concatenating is the joiner that shows nothing was lost:
    // joining with a space here would leave one hanging off the end.
    assert_eq!(all_quiet.concat(), text, "{all_quiet:?}");
}

/// S3: how much of a stretch has sound in it, and the part of a window that is worth sending.
#[test]
fn f1_5_s3_a_window_is_trimmed_to_its_sound_plus_a_quarter_second() {
    let silences = [Silence { start: 10.0, end: 20.0 }];
    assert_eq!(align::voiced(&silences, 0.0, 30.0), 20.0);
    assert_eq!(align::voiced(&silences, 12.0, 15.0), 0.0);
    // A silence only partly inside the stretch counts for the part that is.
    assert_eq!(align::voiced(&silences, 15.0, 30.0), 10.0);

    let window = align::trim((0.0, 30.0), &silences).expect("something has sound");
    // Sound runs 0..10 and 20..30: the pad reaches back from where the sound starts, and there is no
    // quiet before 0 to reach into.
    assert_eq!(window, (0.0, 30.0));
    let inside = align::trim((9.5, 20.5), &silences).unwrap();
    // The window holds one tenth of a second of sound at each end and nothing but silence between: the
    // pad reaches into the quiet on both sides and is clipped by the window's own edges.
    assert_eq!(inside, (9.5, 20.5));
    // The pad is exactly PAD on each side of the sound.
    assert!((PAD - 0.25).abs() < 1e-12);
    assert_eq!(align::trim((10.0, 20.0), &silences), None, "all quiet asks nothing");
}

/// S3: over the limit and longer than [`MIN_PIECE`] a window is cut at the quietest moment nearest its
/// middle — kept away from both edges so a half can always be made smaller.
#[test]
fn f1_5_s3_an_over_long_window_is_cut_at_the_quietest_moment_nearest_the_middle() {
    let span = Span {
        from: 0.0,
        to: 120.0,
        text: "a b c d e f g h".into(),
    };
    // Two silences: one near the middle and one near an edge that is not a candidate at all.
    let silences = [
        Silence { start: 58.0, end: 62.0 },
        Silence { start: 2.0, end: 40.0 },
    ];
    let (left, right) = align::split_at(&span, &silences);
    let cut = left.to;
    assert!(cut > 30.0 && cut < 90.0, "inside the middle three quarters: {cut}");
    assert!((cut - 60.0).abs() < 1e-9, "the quietest moment nearest the middle: {cut}");
    assert!(left.to - left.from < span.to - span.from);
    assert!(right.to - right.from < span.to - span.from);
    // The words follow the sound: both halves keep some and none are lost.
    assert_eq!(format!("{} {}", left.text, right.text), span.text);

    // With no silence anywhere the middle is still a legal cut, so this never returns nothing.
    let (left, right) = align::split_at(&span, &[]);
    assert!((left.to - 60.0).abs() < 1e-9 && (right.from - 60.0).abs() < 1e-9);

    // Through the driver: one span over the limit becomes two asks whose clips tile it. 100 s at this
    // flow's ceiling is three pieces — F1.4 cut the same recording at these edges — and the middle
    // one, which sits over a silence, is trimmed to its sound before it goes up.
    let (_root, tree) = project("s3-split");
    let mut agent = Box_::new(tree.words_aligned_json(SOURCE));
    align::run(
        &tree,
        SOURCE,
        100.0,
        "en",
        &[Silence { start: 48.0, end: 52.0 }],
        &[],
        &[],
        "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty",
        &["qwen3-aligner".to_string()],
        |model, clip, text| agent.ask(model, clip, text),
    )
    .expect("three windows");
    let clips = agent.clips();
    // With the aligner's own reach (P.eng.alignCutSeekSeconds = 4) the count is
    // ceil(100/(60-2*4)) = ceil(1.923) = 2 pieces of 50.0 each. The one interior edge lands on the
    // nominal 50.0, which is also the silence's midpoint, so the cut is in the quiet either way.
    // The clip does NOT start at that edge: the window is first trimmed to its sound plus PAD
    // (`trim`, S3), and this window [50,100] has its first sounding stretch starting at 52 (the end
    // of the 48-52 silence), padded back by 0.25 -> 51.75. Hence [0.0, 51.75].
    assert_eq!(clips, vec![0.0, 51.75], "{clips:?}");
    assert_eq!(clips[0], 0.0);
    // The pieces are the recording's own edges, and a clip starts where its sound does — an edge
    // reached back into the quiet by exactly PAD, except at the recording's own start.
    let silence = Silence { start: 48.0, end: 52.0 };
    // The reference edges are the ones `pieces` actually built with: the aligner's own reach, not the
    // chunker's. Same cutter, same limit, same seek -- so this stays a like-for-like check that every
    // clip starts on a real edge of the recording.
    let edges = asr::cut_points(100.0, &[silence], WINDOW, align::CUT_SEEK);
    assert_eq!(edges.len(), 3, "{edges:?}");
    // The clip name carries the start in milliseconds, so it reads back to three decimals — which is
    // how ffmpeg is told where the window begins.
    let as_ms = |edge: f64| (edge * 1000.0).round() / 1000.0;
    // The count still matches: one clip per piece. But a clip starts where its window's SOUND starts,
    // padded back into the quiet by PAD (`trim`, S3) -- not at the piece boundary. Piece one [0,50]
    // holds sound from 0 up to the silence at 48, whose end 52 lies outside it, so nothing is trimmed
    // and the clip starts at 0.0. Piece two [50,100] has no sound before the silence ends at 52, so
    // its first stretch starts there and pads back to 51.75. Under the borrowed wider reach there was
    // a middle piece sitting over the silence, which is why this used to read as clips == edges.
    assert_eq!(clips.len(), edges.len() - 1, "{clips:?} vs {edges:?}");
    assert_eq!(clips[0], as_ms(edges[0]), "the first clip starts at the recording's start");
    assert!(
        clips[1] > edges[1],
        "the second clip starts past its piece edge, where the sound resumes: {clips:?} vs {edges:?}"
    );

    // At or under the floor a window is sent as it stands: no split, so no recursion that never ends.
    let halved = align::halve(SOURCE, MIN_PIECE);
    assert_eq!(halved.window, MIN_PIECE);
    assert_eq!(halved.log, None, "the retry loop ends here");
    let halved = align::halve(SOURCE, 30.0);
    assert_eq!(halved.window, 15.0);
    assert!(halved.log.expect("said out loud").contains("15 s at a time"));
}

/// S3: an out-of-memory answer halves the window and asks again; at the floor there is nothing left to
/// do but let the refusal through.
#[test]
fn f1_5_s3_out_of_memory_halves_the_window_and_never_goes_under_fifteen_seconds() {
    OOM.lock().unwrap().clear();
    let (_root, tree) = project("s3-oom");
    // The first ask says there is no room; the second answers. The clip is 20 s — under this flow's
    // 60 s ceiling, so it was sent without being split and halving is the only thing left to try.
    let outcome = align::run(
        &tree,
        SOURCE,
        20.0,
        "en",
        &loud(20.0),
        &[],
        &[],
        "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen",
        &["qwen3-aligner".to_string()],
        |model, clip, _text| {
            let mut agent = OOM.lock().unwrap();
            agent.push((model.to_string(), clip.to_string()));
            if agent.len() == 1 {
                return Err("CUDA out of memory".into());
            }
            Ok(json!({"words": [word("hello", 0.5, 1.0)]}))
        },
    )
    .expect("the second try had room");

    let log = outcome
        .logs
        .iter()
        .find(|line| line.contains("no room"))
        .expect("that the machine is out of room is said out loud");
    assert!(log.contains(SOURCE), "{log}");
    assert!(log.contains("30 s at a time"), "{log}");
    // Both asks were for the same window: halving changes what a piece may be, not where it starts.
    let asks = OOM.lock().unwrap().clone();
    assert_eq!(asks.len(), 2, "{asks:?}");
    assert_eq!(asks[0].1, asks[1].1, "{asks:?}");

    // And at the floor the refusal is a refusal: 30 s splits to two 15 s halves, and 15 cannot go
    // smaller, so the second half's "no room" reaches the caller.
    let (_root, tree) = project("s3-oom-floor");
    let mut tries = 0usize;
    let err = align::run(
        &tree,
        SOURCE,
        30.0,
        "en",
        &loud(30.0),
        &[],
        &[],
        "one two three",
        &["qwen3-aligner".to_string()],
        |_, _, _| {
            tries += 1;
            Err("CUDA out of memory".into())
        },
    )
    .expect_err("nothing left to halve");
    assert_eq!(err, "CUDA out of memory");
    assert!(tries >= 2, "{tries} asks: the floor is reached, not skipped");
}

/// S5: `words.aligned.json` is written whole at the end, never per window — a half-written file would
/// be F1.3's "the times are the aligner's" marker for work that was never done.
#[test]
fn f1_5_s5_the_aligned_words_are_written_whole_at_the_end() {
    let (_root, tree) = project("s5-once");
    // One window per piece, each answering with a word of its own so their order is checkable. The
    // silent-free fixture gives 2 pieces under the aligner's own reach (see the count below), so two
    // answers -- one more than pieces would have been dropped as "nothing heard here".
    let answers = vec![
        json!({"words": [word("one", 0.5, 1.0)]}),
        json!({"words": [word("two", 40.5, 41.0)]}),
    ];
    let mut agent = Box_::new(tree.words_aligned_json(SOURCE)).scripted(answers);
    let outcome = align::run(
        &tree,
        SOURCE,
        100.0,
        "en",
        &loud(100.0),
        &[],
        &[],
        "one two three",
        &["qwen3-aligner".to_string()],
        |model, clip, text| agent.ask(model, clip, text),
    )
    .expect("three windows");

    // Every window was asked before the file existed: nothing on disk until the pass was over.
    // The fixture is silent-free (`loud()` returns no silences), so with the aligner's own 4 s reach
    // there is nothing to slide onto: ceil(100/(60-8)) = 2 pieces of 50.0, hence two asks. Under the
    // borrowed 20 s reach (trimmed to 10) the count was 3 -- only the piece count moved, not the
    // property under test, which is that none of these asks saw the marker.
    let mid_run = agent.mid_run();
    assert_eq!(mid_run.len(), 2, "{mid_run:?}");
    assert!(
        mid_run.iter().all(|existed| !*existed),
        "the marker appeared mid-run: {mid_run:?}"
    );

    // And it reads back as one document, every word in order across the piece boundaries, with times
    // in the recording's own seconds rather than each clip's.
    let saved = requests::read_aligned(&tree, SOURCE)
        .unwrap()
        .expect("written whole");
    let placed: Vec<&str> = saved.words.iter().map(|word| word.word.as_str()).collect();
    assert_eq!(placed, vec!["one", "two"], "every word of every piece, in piece order");
    let starts: Vec<f64> = saved
        .words
        .iter()
        .map(|word| seconds(word.start_sample))
        .collect();
    assert_eq!(starts.len(), 2);
    for pair in starts.windows(2) {
        assert!(pair[1] > pair[0], "in order across the pieces: {starts:?}");
    }
    assert!((starts[0] - 0.5).abs() < 1e-9, "{starts:?}");
    // The outcome carries the same words, dressed from the transcript.
    assert_eq!(outcome.words.len(), 2);
}

/// S4: what an aligner is asked with, and how its answer is read however it spells it.
#[test]
fn f1_5_s4_the_request_carries_audio_text_and_language_and_times_are_read_four_ways() {
    let body = align::body("w000033333.wav", "one two", "de");
    assert_eq!(body["audio"], "w000033333.wav");
    assert_eq!(body["text"], "one two");
    assert_eq!(body["language"], "de");

    // `-ss` before the caller's own `-i`, as in F1.4: on the pcm this stage wrote that seek is exact.
    let plan = align::clip_plan(33.333, 60.0);
    assert_eq!(plan[0], "-ss");
    assert!(!plan.contains(&"-i".to_string()), "the caller appends its own input: {plan:?}");

    // Each of the five places a server puts its words is read.
    for (key, value) in [
        ("words", json!({"word": "hello", "start": 1.0, "end": 2.0})),
        ("alignment", json!({"word": "hello", "start": 1.0, "end": 2.0})),
        ("segments", json!({"word": "hello", "start": 1.0, "end": 2.0})),
    ] {
        let found = align::parse_words(&json!({key: [value]})).expect(key);
        assert_eq!(found.len(), 1, "{key}");
    }
    let nested = json!({"result": {"words": [word("hello", 1.0, 2.0)]}});
    assert_eq!(align::parse_words(&nested).unwrap().len(), 1);
    let nested_alignment = json!({"result": {"alignment": [word("hello", 1.0, 2.0)]}});
    assert_eq!(align::parse_words(&nested_alignment).unwrap().len(), 1);

    // The four ways a time is spelled, all landing on the same second.
    let read = |body: Value| align::parse_words(&body).expect("one word")[0].clone();
    let samples = json!({"words": [{"word": "a", "start_sample": 16_000, "end_sample": 32_000}]});
    assert_eq!(seconds(read(samples).start_sample), 1.0);
    let millis = json!({"words": [{"word": "a", "start_ms": 1000.0, "end_ms": 2000.0}]});
    assert_eq!(seconds(read(millis).start_sample), 1.0);
    let times = json!({"words": [{"word": "a", "start_time": 1.0, "end_time": 2.0}]});
    assert_eq!(seconds(read(times).start_sample), 1.0);
    let seconds_given = json!({"words": [{"word": "a", "start": 1.5, "end": 2.5}]});
    assert_eq!(seconds(read(seconds_given).start_sample), 1.5);
    // A bare number past a clip's length was samples that forgot to say so.
    let bare_samples = json!({"words": [{"word": "a", "start": 16_000.0, "end": 32_000.0}]});
    assert_eq!(seconds(read(bare_samples).start_sample), 1.0);

    // Words are stored lower-cased, and the transcript's case and punctuation come back without any
    // time moving. The punctuation rides on the word the aligner heard, so it is stripped to compare.
    let dressed = json!({"words": [{"word": "Hello", "start": 1.0, "end": 2.0}]});
    let heard = align::parse_words(&dressed).unwrap();
    assert_eq!(heard[0].word, "hello");
    let restored = align::restore(&heard, "Hello, world");
    assert_eq!(restored[0].word, "Hello,");
    assert_eq!(restored[0].start_sample, heard[0].start_sample);
    assert_eq!(restored[0].end_sample, heard[0].end_sample);

    // An answer with no words is an error, and it is logged with the shape of what did come back.
    let empty = json!({"text": "not what this expected"});
    let err = align::parse_words(&empty).unwrap_err();
    assert_eq!(err, "no words in the answer");
    let log = align::unreadable_log(&err, &empty.to_string());
    assert!(log.starts_with("!!! align: no words in the answer -- the answer began: "), "{log}");
    assert!(log.contains("not what this expected"), "{log}");

    // Through the driver: that log line is what the run leaves behind when nothing can be placed.
    let (_root, tree) = project("s4-unreadable");
    let mut agent = Box_::new(tree.words_aligned_json(SOURCE)).always(empty);
    let outcome = align::run(
        &tree,
        SOURCE,
        20.0,
        "en",
        &loud(20.0),
        &[],
        &[],
        "one two",
        &["qwen3-aligner".to_string()],
        |model, clip, text| agent.ask(model, clip, text),
    )
    .expect_err("an answer with no words is a refusal");
    assert!(outcome.contains("no words in the answer"), "{outcome}");
}

/// S6: how much of the talking has no word over it, and when that is worth saying out loud.
#[test]
fn f1_5_s6_bare_voiced_seconds_are_warned_about_once() {
    // Both bounds are needed: ten bare seconds of a four-hour session is rounding, and ten bare
    // seconds of a twenty-second clip is the whole clip.
    assert!(!align::bare_warns(9.0, 10.0), "under ten seconds is rounding");
    assert!(!align::bare_warns(9.9, 9.9), "and under ten seconds even at every second spoken");
    assert!(!align::bare_warns(10.0, 200.0), "ten of two hundred is five percent");
    assert!(align::bare_warns(16.0, 200.0), "eight percent and sixteen seconds");
    assert!(align::bare_warns(10.0, 100.0));

    assert_eq!(
        align::bare_warning(12.0, 100.0),
        "!!! align: 12 s of the 100 s spoken has no word over it -- the times are wrong, and the cut will drop that footage"
    );

    // Through the driver: an answer that leaves twelve seconds of speech un-covered says so, and one
    // that covers the recording does not.
    let duration = 60.0;
    let gap = json!({"words": [
        word("one", 0.5, 2.0),
        word("two", 14.0, 20.0),
        word("three", 20.0, 26.0),
        word("four", 26.0, 32.0),
        word("five", 32.0, 38.0),
        word("six", 38.0, 44.0),
        word("seven", 44.0, 50.0),
        word("eight", 50.0, 56.0),
        word("nine", 56.0, 59.5)
    ]});
    let (_root, tree) = project("s6-bare");
    let mut agent = Box_::new(tree.words_aligned_json(SOURCE)).always(gap.clone());
    let outcome = align::run(
        &tree,
        SOURCE,
        duration,
        "en",
        &loud(duration),
        &[],
        &[],
        "one two three four five six seven eight nine",
        &["qwen3-aligner".to_string()],
        |model, clip, text| agent.ask(model, clip, text),
    )
    .expect("the words came back");
    let warning = outcome
        .logs
        .iter()
        .find(|line| line.contains("has no word over it"))
        .expect("twelve bare seconds of speech is the cut deleting them");
    assert!(warning.contains("the times are wrong"), "{warning}");

    let (_root, tree) = project("s6-covered");
    let mut agent = Box_::new(tree.words_aligned_json(SOURCE)).always(json!({"words": [
        word("one", 0.0, duration)
    ]}));
    let outcome = align::run(
        &tree,
        SOURCE,
        duration,
        "en",
        &loud(duration),
        &[],
        &[],
        "one",
        &["qwen3-aligner".to_string()],
        |model, clip, text| agent.ask(model, clip, text),
    )
    .expect("the word came back");
    assert!(
        !outcome
            .logs
            .iter()
            .any(|line| line.contains("has no word over it")),
        "{:?}",
        outcome.logs
    );
}

/// The asks of the out-of-memory test, recorded across the closure's captures.
static OOM: std::sync::LazyLock<Mutex<Vec<(String, String)>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));
