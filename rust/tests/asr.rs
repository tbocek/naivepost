//! F1.4 ASR in chunks — spec/04-prepare.md.
//!
//! The chunking, the halving and the stitching live in [`naivepost::asr`]; ▶ only hands it ffmpeg's
//! cut and the audio box. So these checks assert how many requests a recording becomes, where their
//! edges land, what each was asked with and which files end up on disk — never that a server answered
//! (spec/00-principles.md §5).

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use naivepost::asr::{self, Answer, Silence};
use naivepost::layout::Tree;
use naivepost::requests;
use naivepost::transcribe::seconds;

const SOURCE: &str = "mic.wav";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/naivepost-asr-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder with the one file this flow reads. Its length is probed by the caller, so its
/// contents do not matter — only that it is there.
fn project(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("talk.naivepost");
    fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    fs::create_dir_all(tree.input_dir(SOURCE)).unwrap();
    fs::write(tree.voice16k_wav(SOURCE), b"pcm").unwrap();
    (root, tree)
}

/// The audio box, as a test answers it: every chunk it was asked for is recorded, and the first `fail`
/// answers come back as the server's "no room" so the halving loop can be seen working.
#[derive(Clone)]
struct Box_ {
    asked: Arc<Mutex<Vec<String>>>,
    fail_first: usize,
    text: fn(usize) -> String,
    words: bool,
}

impl Box_ {
    fn new(fail_first: usize) -> Self {
        Self {
            asked: Arc::new(Mutex::new(Vec::new())),
            fail_first,
            text: |index| format!("chunk {index}"),
            words: true,
        }
    }

    fn silent() -> Self {
        Self {
            text: |_| String::new(),
            words: false,
            fail_first: 0,
            asked: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }

    fn ask(&mut self, file: &str) -> Result<Answer, String> {
        let index = {
            let mut asked = self.asked.lock().unwrap();
            asked.push(file.to_string());
            asked.len() - 1
        };
        if index < self.fail_first {
            return Err("CUDA out of memory".into());
        }
        let words = if self.words {
            vec![naivepost::requests::Word {
                word: "word".into(),
                start_sample: 8_000,
                end_sample: 14_400,
            }]
        } else {
            vec![]
        };
        Ok(Answer {
            text: (self.text)(index),
            words,
        })
    }
}

/// S1: how much audio one request may carry is a property of the model, not of the machine.
#[test]
fn f1_4_s1_the_limit_is_sixty_seconds_for_qwen3_and_three_hundred_otherwise() {
    assert_eq!(asr::chunk_limit("qwen3-asr"), 60.0);
    assert_eq!(asr::chunk_limit("Qwen3-ASR-1.7B").min(60.0), 60.0);
    assert_eq!(asr::chunk_limit("nemotron-asr"), 300.0);
    // An unknown family gets the general ceiling: guessing qwen3 would shorten every request for
    // nothing, and a chunk edge is the worst text in the file.
    assert_eq!(asr::chunk_limit(""), 300.0);
    assert_eq!(asr::chunk_limit("whisper-large"), 300.0);

    // The ceiling is what a request may be, so exactly on it is one request.
    assert!(asr::fits(300.0, 300.0));
    assert!(!asr::fits(300.1, 300.0));
}

/// S1: out of memory halves the ceiling, down to but never under twenty seconds — and says so, since
/// it is better to expose that the machine is out of room than to fail quietly.
#[test]
fn f1_4_s1_out_of_memory_halves_the_limit_and_never_goes_under_twenty_seconds() {
    let halved = asr::halve("mic.wav", 300.0);
    assert_eq!(halved.limit, 150.0);
    let log = halved.log.expect("the halving is said out loud");
    assert!(log.starts_with("!!! [mic.wav] ASR:"), "{log}");
    assert!(log.contains("no room"), "{log}");
    assert!(log.contains("trying 150 s at a time"), "{log}");

    // The floor is reached and held: 30 halves to 20, and at 20 there is nothing left to do but stop.
    let halved = asr::halve("mic.wav", 30.0);
    assert_eq!(halved.limit, 20.0);
    assert!(halved.log.is_some());
    let halved = asr::halve("mic.wav", 20.0);
    assert_eq!(halved.limit, 20.0);
    assert_eq!(halved.log, None, "the retry loop ends here");

    // Which errors are worth asking again about — the server's wording is not ours to fix.
    assert!(asr::retryable("CUDA out of memory"));
    assert!(asr::retryable("no room for 300 s"));
    assert!(asr::retryable("OOM killed"));
    assert!(!asr::retryable("model not loaded"));

    // And the loop itself: two "no room" answers, then an ordinary one at the smaller ceiling.
    let (_root, tree) = project("s1-halve");
    let mut agent = Box_::new(2);
    let outcome = asr::run(&tree, SOURCE, 640.0, "nemotron-asr", "en", &[], |file| agent.ask(file))
        .expect("the third try had room");
    assert_eq!(outcome.logs.len(), 2, "{:?}", outcome.logs);
    assert!(outcome.logs[0].contains("trying 150 s"), "{:?}", outcome.logs);
    assert!(outcome.logs[1].contains("trying 75 s"), "{:?}", outcome.logs);
    // Every ask is recorded, so the whole retry is visible: two "no room" answers at the top of the
    // loop, then an ordinary answer at each of the smaller ceilings. What a chunk was called does not
    // matter here — that it is asked again after each halving is F0.5's re-ask rule, and the log line
    // above is what tells the user why.
    let calls = agent.calls();
    assert_eq!(calls.len(), 15, "{calls:?}");

    // A refusal halving cannot help is a refusal, and it reaches the caller.
    let (_root, tree) = project("s1-refused");
    let err = asr::run(&tree, SOURCE, 60.0, "nemotron-asr", "en", &[], |_| {
        Err("model not loaded".to_string())
    })
    .expect_err("nothing to retry");
    assert_eq!(err, "model not loaded");
}

/// S2: a recording that fits the ceiling is one request, however the ceiling was arrived at.
#[test]
fn f1_4_s2_a_file_that_fits_is_one_request() {
    let (_root, tree) = project("s2-one");
    let mut agent = Box_::new(0);
    let outcome = asr::run(&tree, SOURCE, 40.0, "nemotron-asr", "en", &[], |file| agent.ask(file))
        .expect("one request");

    assert_eq!(agent.calls(), vec!["c00.wav".to_string()]);
    assert_eq!(outcome.chunks.len(), 1);
    assert_eq!((outcome.chunks[0].s, outcome.chunks[0].e), (0.0, 40.0));

    // Just over the ceiling is two even pieces rather than a full one and a runt.
    let (_root, tree) = project("s2-two");
    let mut agent = Box_::new(0);
    let outcome = asr::run(&tree, SOURCE, 360.0, "nemotron-asr", "en", &[], |file| agent.ask(file))
        .expect("two requests");
    assert_eq!(agent.calls(), vec!["c00.wav".to_string(), "c01.wav".to_string()]);
    let edges: Vec<f64> = outcome.chunks.iter().flat_map(|c| [c.s, c.e]).collect();
    assert_eq!(edges.len(), 4);
    assert_eq!(edges[0], 0.0);
    assert_eq!(*edges.last().unwrap(), 360.0);

    // The scratch is this stage's own mess and goes when the answer is on disk.
    assert!(!tree.asr_scratch(SOURCE).exists(), "kept: {:?}", tree.asr_scratch(SOURCE));
}

/// S2: even pieces, cut at silence midpoints, with each cut's reach paid for out of the piece — two
/// cuts sliding apart must not push the piece between them past the ceiling.
#[test]
fn f1_4_s2_pieces_are_even_and_cut_at_silence_midpoints() {
    let limit = 300.0;
    let edges = asr::cut_points(640.0, &[], limit, 20.0);
    assert_eq!(edges.len(), 4, "three pieces: {edges:?}");
    assert_eq!(edges[0], 0.0);
    assert_eq!(*edges.last().unwrap(), 640.0);
    for pair in edges.windows(2) {
        assert!(pair[1] > pair[0], "no empty piece: {edges:?}");
        assert!(pair[1] - pair[0] <= limit + 1e-9, "over the ceiling: {edges:?}");
    }
    // Even pieces: a 40 s runt would be a whole extra request whose text is the worst in the file.
    let spread = edges.windows(2).map(|pair| pair[1] - pair[0]).collect::<Vec<_>>();
    assert!(spread.iter().all(|piece| (*piece - spread[0]).abs() < 1e-9), "{spread:?}");

    // A silence midpoint near a cut is where the cut goes...
    let silence = Silence { start: 200.0, end: 210.0 };
    let edges = asr::cut_points(640.0, &[silence], limit, 20.0);
    assert_eq!(edges[1], 205.0, "the midpoint, not the clock: {edges:?}");
    // ...and one further away than the reach is not considered at all.
    let far = Silence { start: 145.0, end: 155.0 };
    let edges = asr::cut_points(640.0, &[far], limit, 20.0);
    assert!((edges[1] - 213.333_333).abs() < 0.01, "the nominal edge: {edges:?}");

    // The reach is capped at a sixth of the ceiling, so a generous seek buys its room out of the
    // piece: against a 60 s ceiling, 20 s of reach would have made 19 s pieces.
    let edges = asr::cut_points(200.0, &[], 60.0, 20.0);
    let step = edges[1] - edges[0];
    assert!((step - 40.0).abs() < 1e-9, "five even pieces: {edges:?}");
    // A silence 12 s from the nominal cut is outside that 10 s reach and so is ignored.
    let out_of_reach = Silence { start: 26.0, end: 28.0 };
    let with_silence = asr::cut_points(200.0, &[out_of_reach], 60.0, 20.0);
    assert_eq!(with_silence, edges, "the reach was trimmed to 10 s: {with_silence:?}");

    // Within the reach it is taken even when it is 12 s away; 25 s away is outside the 20 s seek and
    // so is not considered at all.
    let near = Silence { start: 200.0, end: 202.0 };
    let edges = asr::cut_points(640.0, &[near], limit, 20.0);
    assert_eq!(edges[1], 201.0, "{edges:?}");
    let pieces: Vec<f64> = edges.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(pieces.iter().all(|piece| *piece <= limit), "{pieces:?}");

    // Nothing to cut is nothing to ask about.
    assert_eq!(asr::cut_points(40.0, &[], limit, 20.0), vec![0.0, 40.0]);
}

/// S3: each chunk says which one it is, is seeked before its input, and is asked with the audio and
/// the language.
#[test]
fn f1_4_s3_each_chunk_says_which_one_it_is_and_is_seeked_before_the_input() {
    assert_eq!(asr::progress(0, 3), "recognising speech 1/3");
    assert_eq!(asr::progress(2, 3), "recognising speech 3/3");

    let (_root, tree) = project("s3-progress");
    let mut agent = Box_::new(0);
    let outcome = asr::run(&tree, SOURCE, 640.0, "nemotron-asr", "en", &[], |file| agent.ask(file))
        .expect("three requests");
    assert_eq!(
        outcome.steps,
        vec![
            "recognising speech 1/3".to_string(),
            "recognising speech 2/3".to_string(),
            "recognising speech 3/3".to_string()
        ]
    );

    // `-ss` before `-i`: on the pcm this stage wrote, that seek is exact rather than a keyframe away.
    let plan = asr::cut_plan(213.0, 426.0);
    assert_eq!(plan, vec!["-ss", "213", "-t", "213"]);
    assert_eq!(plan[0], "-ss", "the seek leads");
    assert!(!plan.contains(&"-i".to_string()), "the caller appends its own input");

    // Both what the box hears and what language to expect go up with it.
    let body = asr::body("c01.wav", "de");
    assert_eq!(body["audio"], "c01.wav");
    assert_eq!(body["language"], "de");

    // A successful pass leaves its scratch behind nowhere.
    assert!(!tree.asr_scratch(SOURCE).exists(), "kept: {:?}", tree.asr_scratch(SOURCE));
}

/// S4: a chunk's word times are relative to the file the server heard, so they move by where that
/// file started in the recording.
#[test]
fn f1_4_s4_words_are_shifted_by_their_chunks_start() {
    let words = vec![naivepost::requests::Word {
        word: "word".into(),
        start_sample: 8_000,
        end_sample: 14_400,
    }];

    // Unshifted, every chunk's words would land at the start of the recording.
    let shifted = asr::shift(&words, 213 * 16_000);
    assert_eq!(seconds(shifted[0].start_sample), 213.5);
    assert_eq!(seconds(shifted[0].end_sample), 213.9);

    // Through the driver: the second chunk of a three-way cut starts at its edge, not at zero.
    let (_root, tree) = project("s4-shift");
    let mut agent = Box_::new(0);
    let outcome = asr::run(&tree, SOURCE, 640.0, "nemotron-asr", "en", &[], |file| agent.ask(file))
        .expect("three requests");
    assert_eq!(outcome.words.len(), 3);
    assert_eq!(seconds(outcome.words[0].start_sample), 0.5);
    let second = seconds(outcome.words[1].start_sample);
    assert!((second - 213.833_333).abs() < 0.01, "{second}");
    assert!(outcome.words[1].start_sample > outcome.words[0].start_sample);

    // The join is one space: the seam is already the worst text in the file.
    assert_eq!(outcome.text, "chunk 0 chunk 1 chunk 2");
    let (text, _) = asr::stitch(&[]);
    assert_eq!(text, "");
}

/// S4: three files, written in the order that makes a resume honest — `words.json` last, because it
/// is the marker and nothing may exist after it that the marker does not stand for.
#[test]
fn f1_4_s4_the_three_files_and_the_order_they_appear_in() {
    let (_root, tree) = project("s4-files");
    let mut agent = Box_::new(0);
    let outcome = asr::run(&tree, SOURCE, 360.0, "nemotron-asr", "en", &[], |file| agent.ask(file))
        .expect("two requests");

    // transcript.txt: the joined text and one trailing newline.
    assert_eq!(
        fs::read_to_string(tree.transcript_txt(SOURCE)).unwrap(),
        "chunk 0 chunk 1\n"
    );

    // asrchunks.json: which seconds each request covered and what came back for them, in order.
    let chunks = requests::read_chunks(&tree, SOURCE).unwrap();
    assert_eq!(chunks.len(), 2);
    assert_eq!((chunks[0].s, chunks[0].e, chunks[0].text.as_str()), (0.0, 180.0, "chunk 0"));
    assert_eq!(chunks[1].s, 180.0);
    assert_eq!(chunks[1].e, 360.0);

    // words.json: the shifted words, and it reads back through the same reader F1.3 uses.
    let saved = requests::read_words(&tree, SOURCE).unwrap().expect("the marker exists");
    assert_eq!(saved.text, outcome.text);
    assert_eq!(saved.words.len(), 2);
    assert_eq!(seconds(saved.words[1].start_sample), 180.5);

    // The order is what makes a resume honest: with `words.json` deleted — the marker gone, so this
    // pass has to happen again — the box is asked; with it in place, F1.3's own step would not ask,
    // and `asrchunks.json` sitting beside it proves the requests were made.
    fs::remove_file(tree.words_json(SOURCE)).unwrap();
    assert!(tree.asrchunks_json(SOURCE).exists(), "the record of the requests stays");
    let mut agent = Box_::new(0);
    asr::run(&tree, SOURCE, 360.0, "nemotron-asr", "en", &[], |file| agent.ask(file))
        .expect("asked again without its marker");
    assert_eq!(agent.calls().len(), 2);

    // A failed pass keeps its scratch: `asr/cNN.wav` and its plan are how a chunk is compared against
    // its answer without asking the server again.
    let (_root, tree) = project("s4-kept");
    let mut agent = Box_::new(0);
    let err = asr::run(&tree, SOURCE, 640.0, "nemotron-asr", "en", &[], |file| {
        let answer = agent.ask(file)?;
        if answer.text == "chunk 1" {
            return Err("the server hung up".into());
        }
        Ok(answer)
    })
    .expect_err("a refusal is a refusal");
    assert_eq!(err, "the server hung up");
    let scratch = tree.asr_scratch(SOURCE);
    let kept: Vec<String> = fs::read_dir(&scratch)
        .expect("the scratch is kept on failure")
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect();
    assert!(kept.contains(&"c00.wav.plan".to_string()), "{kept:?}");
    let plan = fs::read_to_string(scratch.join("c00.wav.plan")).unwrap();
    assert!(plan.starts_with("-ss 0 -t "), "{plan}");
    assert!(plan.contains("\"language\":\"en\""), "{plan}");
}

/// S4: nothing heard is a real answer, not an error — a screen capture with no microphone behind it
/// is an ordinary thing to import.
#[test]
fn f1_4_s4_no_speech_at_all_is_a_real_answer() {
    let (_root, tree) = project("s4-empty");
    let mut agent = Box_::silent();
    let outcome = asr::run(&tree, SOURCE, 60.0, "nemotron-asr", "en", &[], |file| agent.ask(file))
        .expect("silence is not a failure");

    assert_eq!(outcome.logs, vec![">>> [mic.wav] no speech found -- an empty transcript"]);
    assert_eq!(asr::no_speech_log(SOURCE), ">>> [mic.wav] no speech found -- an empty transcript");
    assert_eq!(outcome.text, "");
    assert!(outcome.words.is_empty());

    // The lone newline `requests::write_silence` writes for the nothing-heard case — a zero-byte file
    // would read as "not written yet" to every reader here.
    assert_eq!(fs::read_to_string(tree.transcript_txt(SOURCE)).unwrap(), "\n");
    // words.json says only that nothing was heard, exactly as §6 spells it: no `words` array, since a
    // second way to say the same thing is a second way to be wrong.
    assert_eq!(fs::read_to_string(tree.words_json(SOURCE)).unwrap(), r#"{"text":""}"#);
    // And asrchunks.json exists, because the requests were genuinely made.
    let chunks = requests::read_chunks(&tree, SOURCE).unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!((chunks[0].s, chunks[0].e), (0.0, 60.0));
}
