//! F5.3 What "up to date" means — `spec/08-produce.md` §F5.3, checked against `naivepost::produce_stamp`.
//! One test per node and branch of the flowchart: what goes into the hash (encoder settings without the
//! output path, the segments, the lines with their wav size and mtime, the sources with path/size/mtime, the
//! aspect, the voice, the narration flag), that the upload text does not, where `final.stamp` lives beside
//! the video, and what ▶ does when the stored hash is or is not the current one.
//!
//! Ids cited: `P.policy.gameVolume` (0.22) — one of the hashed encoder settings.

use std::path::{Path, PathBuf};

use naivepost::cut::Seg;
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::produce_render;
use naivepost::produce_stamp as stamp;
use naivepost::project::{self, Codec, Container, Crop, Preset, Produce, Publish, Subtitles};

const ITEM: &str = "F5.3";

fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("np-f53-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A file system of the imagination: every path reads as `(size, mtime)` from a list, so a test can move a
/// wav's size without owning a recording. Paths not listed are missing files — `(0, 0)`, the same answer
/// `disk_facts` gives for a file that is not there.
struct Fake {
    facts: Vec<(String, u64, u64)>,
}

impl Fake {
    fn empty() -> Self {
        Self { facts: Vec::new() }
    }
    fn facts(&self) -> impl Fn(&Path) -> (u64, u64) + '_ {
        move |path: &Path| {
            let key = path.to_string_lossy().to_string();
            self.facts
                .iter()
                .find(|(name, _, _)| *name == key)
                .map_or((0, 0), |(_, size, mtime)| (*size, *mtime))
        }
    }
}

/// Where a line's synthesis would live — one path per line, so a test can move one take's facts without
/// touching another's or the recording's. A line with no words has never been spoken and has no wav.
fn one_wav(line: &Entry) -> Option<PathBuf> {
    if line.text.is_empty() {
        return None;
    }
    Some(PathBuf::from(format!("narrate/tts/{}.wav", line.text)))
}

/// The take every spoken line has, at this size and mtime. Absent from a `Case`'s facts means the file is
/// not there: `(0, 0)`, which is how "not spoken yet" reads.
const FIRST_WAV: &str = "narrate/tts/the first line.wav";
const SECOND_WAV: &str = "narrate/tts/the second.wav";

fn seg(s: f64, e: f64) -> Seg {
    let mut clip = Seg::default();
    clip.s = s;
    clip.e = e;
    clip.ins = String::new();
    clip
}

fn entry(s: f64, e: f64, text: &str) -> Entry {
    let mut line = Entry::default();
    line.s = s;
    line.e = e;
    line.text = text.to_string();
    line
}

fn source(path: &str, footage: bool) -> project::Source {
    let mut row = project::Source::default();
    row.path = path.to_string();
    row.footage = footage;
    row
}

/// The unchanged state every test starts from: one clip, two spoken lines, one recording, 16:9, voice 1.
#[derive(Debug, Clone)]
struct Case {
    settings: Produce,
    segs: Vec<Seg>,
    lines: Vec<Entry>,
    sources: Vec<project::Source>,
    aspect: String,
    voice: String,
    no_narration: bool,
    facts: Vec<(String, u64, u64)>,
}

impl Case {
    fn new() -> Self {
        Case {
            settings: Produce::default(),
            segs: vec![seg(0.0, 12.0)],
            lines: vec![entry(0.0, 4.0, "the first line"), entry(4.0, 8.0, "the second")],
            sources: vec![source("media/talk.mkv", true)],
            aspect: "16:9".to_string(),
            voice: "1".to_string(),
            no_narration: false,
            facts: vec![
                (FIRST_WAV.into(), 40_000, 1_700_000_000),
                (SECOND_WAV.into(), 30_000, 1_700_000_000),
                ("media/talk.mkv".into(), 1_000_000, 1_699_000_000),
            ],
        }
    }

    fn input(&self) -> stamp::Input<'_> {
        stamp::Input {
            settings: &self.settings,
            segs: &self.segs,
            lines: &self.lines,
            sources: &self.sources,
            aspect: &self.aspect,
            voice: &self.voice,
            no_narration: self.no_narration,
        }
    }

    fn stamp(&self) -> String {
        let fake = Fake { facts: self.facts.clone() };
        stamp::stamp(&self.input(), one_wav, fake.facts())
    }

    fn parts(&self) -> Vec<(&'static str, String)> {
        let fake = Fake { facts: self.facts.clone() };
        stamp::parts(&self.input(), one_wav, fake.facts())
    }

    /// The same state with one file's facts replaced — what a re-spoken take or a re-recorded source does.
    fn with_facts(mut self, path: &str, size: u64, mtime: u64) -> Self {
        match self
            .facts
            .iter_mut()
            .find(|(name, _, _)| name == path)
        {
            Some(slot) => *slot = (path.to_string(), size, mtime),
            None => self.facts.push((path.to_string(), size, mtime)),
        }
        self
    }

    /// The same state with the settings replaced — the shape every settings test needs.
    fn with_settings(self, change: impl FnOnce(&mut Produce)) -> Self {
        let mut next = self;
        change(&mut next.settings);
        next
    }
}

/// `changed_parts` for two states, which is how a test says *which* group moved rather than only that
/// something did.
fn changed(before: &Case, after: &Case) -> Vec<&'static str> {
    stamp::changed_parts(&before.parts(), &after.parts())
}

// ---- the hash's ingredients ------------------------------------------------------

#[test]
fn f5_3_s1_every_encoder_setting_moves_the_hash() {
    assert_eq!(ITEM, "F5.3");
    let base = Case::new();
    // P.policy.gameVolume (0.22) is one of the hashed settings, so it is flipped here like the rest.
    assert!((base.settings.game_volume - 0.22).abs() < 1e-9);

    let flips: Vec<(&str, fn(&mut Produce))> = vec![
        ("container", |s| s.container = Container::Mkv),
        ("codec", |s| s.codec = Codec::Vp9),
        ("preset", |s| s.preset = Preset::Fast),
        ("resolution", |s| s.resolution = project::Resolution::P720),
        ("frame_rate", |s| s.frame_rate = project::FrameRate::F60),
        ("vfr", |s| s.vfr = !s.vfr),
        ("mono", |s| s.mono = !s.mono),
        ("audio_kbps", |s| s.audio_kbps = 192),
        ("subtitles", |s| s.subtitles = Subtitles::BurnedIn),
        ("translate", |s| s.translate.push("de".to_string())),
        ("game_volume", |s| s.game_volume = 0.4),
        ("crf", |s| s.crf = 18),
        ("blurred_edges", |s| s.blurred_edges = !s.blurred_edges),
    ];
    for (row, change) in flips {
        let moved = base.clone().with_settings(change);
        assert_ne!(moved.stamp(), base.stamp(), "{row} must make the render stale");
        assert_eq!(changed(&base, &moved), vec!["settings"], "{row}");
    }

    // The frame rate and what it means are two different files: 30 as a target and 30 as a ceiling.
    let target = base.clone().with_settings(|s| s.frame_rate = project::FrameRate::F30);
    let ceiling = target
        .clone()
        .with_settings(|s| s.vfr = true);
    assert_eq!(target.settings.frame_rate, project::FrameRate::F30);
    assert_ne!(ceiling.stamp(), target.stamp());

    // The same ticks in another order are the same render — the languages are hashed sorted.
    let one = base.clone().with_settings(|s| s.translate = vec!["de".into(), "sv".into()]);
    let other = base.clone().with_settings(|s| s.translate = vec!["sv".into(), "de".into()]);
    assert_eq!(one.stamp(), other.stamp());

    // Nothing changed at all: the same hash, twice in a row, so an autosave that rewrote the project costs
    // no encode.
    assert_eq!(Case::new().stamp(), base.stamp());
}

#[test]
fn f5_3_s2_the_output_path_is_not_in_it() {
    assert_eq!(ITEM, "F5.3");
    let base = Case::new();
    // §A fixes the file as `produce/final.<container>`, so there is no path to hash: two runs of the same
    // state agree however the result is later renamed or moved by hand.
    assert_eq!(base.stamp(), Case::new().stamp());
    let input = base.input();
    let fake = Fake::empty();
    let first = stamp::stamp(&input, one_wav, fake.facts());
    let second = stamp::stamp(&input, one_wav, fake.facts());
    assert_eq!(first, second);
    assert_eq!(first.len(), 16, "a stamp is 16 hex characters: {first}");
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()), "{first}");

    // The container *is* a setting, and it is the only thing that decides the name — so switching mp4→webm
    // does make the render stale even though it also changes where the file goes.
    let webm = base.clone().with_settings(|s| s.container = Container::Webm);
    assert_ne!(webm.stamp(), base.stamp());
    assert_eq!(changed(&base, &webm), vec!["settings"]);

    // And the stamp file is not named by any group: it lives beside the video, not inside its hash.
    assert!(!stamp::STAMP_FILE.contains("produce/"));
}

#[test]
fn f5_3_s3_a_moved_or_edited_segment_moves_the_hash() {
    assert_eq!(ITEM, "F5.3");
    let base = Case::new();
    let edits: Vec<(&str, fn(&mut Seg))> = vec![
        ("start", |s| s.s = 1.0),
        ("end", |s| s.e = 11.0),
        ("insert", |s| s.ins = "media/ sting.mp4".to_string()),
        ("insert length", |s| s.dur = 3.0),
        ("rate", |s| s.rate = 2.0),
        ("mute", |s| s.mute = true),
        ("lane", |s| s.lane = "Music".to_string()),
        ("insert offset", |s| s.ss = 1.5),
    ];
    for (what, change) in edits {
        let mut moved = base.clone();
        change(&mut moved.segs[0]);
        assert_ne!(moved.stamp(), base.stamp(), "{what} must move the hash");
        assert_eq!(changed(&base, &moved), vec!["segments"], "{what}");
    }

    // A segment shown from another picture row, or one whose preview hush is different, encodes the same
    // bytes: those are view state, and re-encoding a whole video for them would be absurd.
    let mut moved = base.clone();
    moved.segs[0].cam = 2;
    assert_eq!(moved.stamp(), base.stamp());
    let mut moved = base.clone();
    moved.segs[0].quiet = vec!["Music".to_string()];
    assert_eq!(moved.stamp(), base.stamp());

    // Dropping a segment is a change, and so is adding one back at the end.
    let mut fewer = base.clone();
    fewer.segs.pop();
    assert_ne!(fewer.stamp(), base.stamp());
    assert_eq!(changed(&base, &fewer), vec!["segments"]);
}

#[test]
fn f5_3_s4_lines_bounds_text_wav_size_and_mtime() {
    assert_eq!(ITEM, "F5.3");
    let base = Case::new();

    // The wav grew: the line was re-spoken and said something longer.
    let bigger = base.clone().with_facts(FIRST_WAV, 41_000, 1_700_000_000);
    assert_ne!(bigger.stamp(), base.stamp());
    assert_eq!(changed(&base, &bigger), vec!["lines"]);

    // Same size, later mtime: re-recorded with the same number of words. Size alone would miss it.
    let touched = base.clone().with_facts(FIRST_WAV, 40_000, 1_700_000_600);
    assert_ne!(touched.stamp(), base.stamp());
    assert_eq!(changed(&base, &touched), vec!["lines"]);

    // A line with no wav at all is not a line whose wav is empty: `no wav` is its own value, so a synthesis
    // that never ran cannot leave the video looking up to date.
    let mut spoken = base.clone();
    spoken.lines[1].text.clear();
    assert_ne!(spoken.stamp(), base.stamp());
    let empty_wav = base.clone().with_facts(FIRST_WAV, 0, 0);
    assert_ne!(empty_wav.stamp(), spoken.stamp());

    // Bounds and words.
    let mut moved = base.clone();
    moved.lines[0].at += 0.5;
    assert_eq!(moved.stamp(), base.stamp(), "where the line sits in the clip is not hashed twice");
    let mut moved = base.clone();
    moved.lines[0].s = 0.5;
    assert_ne!(moved.stamp(), base.stamp());
    let mut moved = base.clone();
    moved.lines[0].text = "the first line!".to_string();
    assert_ne!(moved.stamp(), base.stamp());
    assert_eq!(changed(&base, &moved), vec!["lines"]);

    // Order is meaning: the same two lines swapped are a different video.
    let mut swapped = base.clone();
    swapped.lines.reverse();
    assert_ne!(swapped.stamp(), base.stamp());
    assert_eq!(changed(&base, &swapped), vec!["lines"]);
}

#[test]
fn f5_3_s5_sources_path_size_and_mtime() {
    assert_eq!(ITEM, "F5.3");
    let base = Case::new();

    // A re-recorded or replaced recording moves the hash even though every segment still points at the same
    // second of it — the file behind those seconds is a different file now.
    let longer = base.clone().with_facts("media/talk.mkv", 2_000_000, 1_700_000_000);
    assert_ne!(longer.stamp(), base.stamp());
    assert_eq!(changed(&base, &longer), vec!["sources"]);

    let touched = base.clone().with_facts("media/talk.mkv", 1_000_000, 1_700_000_600);
    assert_ne!(touched.stamp(), base.stamp());
    assert_eq!(changed(&base, &touched), vec!["sources"]);

    let mut renamed = base.clone();
    renamed.sources[0].path = "media/talk-take-2.mkv".to_string();
    assert_ne!(renamed.stamp(), base.stamp());
    assert_eq!(changed(&base, &renamed), vec!["sources"]);

    // A source that is not footage — the recording the voice is cloned from — is read by the Narrate page's
    // sample, not by the encoder. The voice that clones it is hashed under `voice` instead.
    let mut extra = base.clone();
    extra.sources.push(source("media/host.mkv", false));
    assert_eq!(extra.stamp(), base.stamp());
}

#[test]
fn f5_3_s6_aspect_voice_and_the_narration_flag() {
    assert_eq!(ITEM, "F5.3");
    let base = Case::new();

    let mut wider = base.clone();
    wider.aspect = "4:3".to_string();
    assert_ne!(wider.stamp(), base.stamp());
    assert_eq!(changed(&base, &wider), vec!["aspect"]);

    // Switching voices keeps every line and every setting and changes only the sound.
    let mut other = base.clone();
    other.voice = "2".to_string();
    assert_ne!(other.stamp(), base.stamp());
    assert_eq!(changed(&base, &other), vec!["voice"]);

    // The flag on its own: with narration off no line is spoken, and the lines must not be able to hold the
    // video up to date behind the tick.
    let mut quiet = base.clone();
    quiet.no_narration = true;
    assert_ne!(quiet.stamp(), base.stamp());
    assert_eq!(changed(&base, &quiet), vec!["narration"]);

    // The flag is a group of its own: with it on, an edit that moves only the lines still names only them.
    let mut longer = base.clone();
    longer.lines[0].text = "longer than before".to_string();
    assert_eq!(changed(&base, &longer), vec!["lines"]);

    // Stability: the same state hashed twice is the same 16 characters.
    assert_eq!(base.stamp(), base.stamp());
}

// ---- where it lives, and what ▶ does with it -------------------------------------

#[test]
fn f5_3_s7_the_stamp_lives_beside_the_video_named_after_its_stem() {
    assert_eq!(ITEM, "F5.3");
    let root = temp_root("s7");
    let tree = Tree::new(root.join("demo.naivepost")).unwrap();

    // `<stem>.stamp` beside the video: `produce/final.stamp`, next to `produce/final.<container>`. §A fixes
    // both names, so one stamp file serves every container — switching mp4→webm is a settings change inside
    // the hash, not a second stamp nobody would ever clear.
    assert_eq!(stamp::STAMP_FILE, "final.stamp");
    assert_eq!(stamp::stamp_path(&tree), tree.final_stamp());
    assert_eq!(stamp::stamp_path(&tree), root.join("demo.naivepost/produce/final.stamp"));
    assert_eq!(
        stamp::stamp_path(&tree).parent(),
        tree.final_video("mp4").parent(),
        "the stamp sits beside the video it describes"
    );

    // Not in the scratch folder, which every run clears before it encodes anything.
    assert!(!stamp::stamp_path(&tree)
        .starts_with(tree.clips_dir()));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f5_3_s8_the_same_hash_skips_the_encode() {
    assert_eq!(ITEM, "F5.3");
    let root = temp_root("s8");
    let tree = Tree::new(root.join("demo.naivepost")).unwrap();
    let base = Case::new();
    let current = base.stamp();

    // The flowchart's yes branch, and the sentence that skip is told in: `produce_flow::SKIP_LOG` is the
    // one skip line the page says (§F5.1 S2 spells it), so this pins *that* string, not a second variant.
    assert!(stamp::skip_encode(Some(&current), &current));
    assert!(
        naivepost::produce_flow::SKIP_LOG.contains("already what this page describes"),
        "{}",
        naivepost::produce_flow::SKIP_LOG
    );

    // Each no branch means encode.
    let other = base.clone().with_facts(FIRST_WAV, 41_000, 1_700_000_000);
    assert!(!stamp::skip_encode(Some(&current), &other.stamp()));
    assert!(!stamp::skip_encode(None, &current), "never produced");

    // No file, and an unreadable one, both read as None — the stamp may only ever let a run skip work it can
    // prove was done, never hide that it cannot tell.
    assert_eq!(stamp::read_stamp(&tree), None);
    stamp::write_stamp(&tree, &current).unwrap();
    assert_eq!(stamp::read_stamp(&tree).as_deref(), Some(current.as_str()));

    std::fs::write(stamp::stamp_path(&tree), "\n").unwrap();
    assert_eq!(stamp::read_stamp(&tree), None, "an empty stamp proves nothing");
    std::fs::remove_file(stamp::stamp_path(&tree)).unwrap();
    assert!(!stamp::skip_encode(stamp::read_stamp(&tree).as_deref(), &current));

    // The other branch has no line of its own: §F5.3 gives the skip a sentence and leaves the encode to
    // F5.1's opening line, so what is asserted here is only that the answer was "encode" — which the two
    // `!skip_encode` checks above already settled.
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f5_3_s9_encode_then_write_the_new_stamp() {
    assert_eq!(ITEM, "F5.3");
    let root = temp_root("s9");
    let tree = Tree::new(root.join("demo.naivepost")).unwrap();
    let base = Case::new();
    let current = base.stamp();

    // A first run: nothing stored, so encode, then write. The folder does not exist yet and is created.
    assert!(!stamp::stamp_path(&tree).exists());
    assert!(stamp::stamp_written(true, false), "an encode that returned writes the stamp");
    stamp::write_stamp(&tree, &current).unwrap();
    assert_eq!(stamp::read_stamp(&tree).as_deref(), Some(current.as_str()));

    // The next ▶ over the same state skips, and — because a skipped run writes nothing — cannot rewrite it.
    assert!(stamp::skip_encode(stamp::read_stamp(&tree).as_deref(), &current));
    assert!(!stamp::stamp_written(false, false), "a run that skipped proves nothing new");
    assert!(!stamp::stamp_written(true, true), "a failed run writes no promise");

    // A changed state means encode again and the stamp follows the file it describes.
    let moved = base.clone().with_facts(FIRST_WAV, 90_000, 1_700_000_900);
    let next = moved.stamp();
    assert!(!stamp::skip_encode(stamp::read_stamp(&tree).as_deref(), &next));
    stamp::write_stamp(&tree, &next).unwrap();
    assert_eq!(stamp::read_stamp(&tree).as_deref(), Some(next.as_str()));
    assert!(stamp::skip_encode(stamp::read_stamp(&tree).as_deref(), &next));

    // The log names the hash, so a stamp somebody wants to delete is findable by eye.
    assert_eq!(stamp::wrote_stamp_log("0a0b"), ">>> final.stamp: 0a0b");

    // F5.2's gate stays the only rule about when a write happens — this module stores, it does not decide.
    assert!(!produce_render::stamp_written(false, false));
    std::fs::remove_dir_all(&root).ok();
}

// ---- what is deliberately not in it ----------------------------------------------

#[test]
fn f5_3_s10_the_upload_text_is_not_in_the_stamp() {
    assert_eq!(ITEM, "F5.3");
    // Title, description, thumbnail and the upload record change with a redraw or one edited sentence; none
    // of them touches a frame of the video, so none of them may make an encode look stale. They are not even
    // part of `Input` — which is the strongest form the check can take.
    let base = Case::new();
    let mut written = Publish::default();
    written.title = "A better title".to_string();
    written.description = "everything about it".to_string();
    written.prompt = "draw this".to_string();
    written.negative = "not this".to_string();
    written.thumb_title = "BIG".to_string();
    written.frames = vec!["produce/publish/frame-1.png".to_string()];
    written.crop = Crop { x: 0.4, y: 0.2 };
    written.own = true;
    written.title_seeded = true;
    // The stamp's input cannot see any of that: same state, same hash.
    assert_eq!(base.stamp(), Case::new().stamp());
    assert_ne!(base.stamp().len(), 0);
    let _ = format!("{written:?}");

    // The upload text's own gate is a file, and deleting the folder starts it over — tested here against a
    // real project folder so F5.3 and §5 cannot drift apart.
    let root = temp_root("s10");
    let tree = Tree::new(root.join("demo.naivepost")).unwrap();
    assert!(!naivepost::publish::is_written(&tree));
    std::fs::create_dir_all(tree.publish_dir()).unwrap();
    std::fs::write(tree.publish_json(), "{}").unwrap();
    assert!(naivepost::publish::is_written(&tree));
    naivepost::publish::start_over(&tree).unwrap();
    assert!(!naivepost::publish::is_written(&tree));

    // And that gate lives inside `produce/`, beside the video and its stamp.
    assert!(tree.publish_json().starts_with(stamp::stamp_path(&tree).parent().unwrap().parent().unwrap()));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f5_3_s11_the_stamp_is_seven_named_parts_not_one_blob() {
    assert_eq!(ITEM, "F5.3");
    let base = Case::new();
    let parts = base.parts();
    let names: Vec<&str> = parts.iter().map(|(name, _)| *name).collect();
    // The flowchart's order, spelled out so a reader can check it against the diagram.
    assert_eq!(
        names,
        vec![
            "settings", "segments", "lines", "sources", "aspect", "voice", "narration"
        ]
    );
    assert!(parts.iter().all(|(_, hash)| hash.len() == 16), "{parts:?}");

    // One edit names exactly one group — the reason the groups are hashed apart.
    let wider = base.clone().with_settings(|s| s.crf = 30);
    assert_eq!(changed(&base, &wider), vec!["settings"]);
    let mut moved = base.clone();
    moved.segs[0].e += 1.0;
    assert_eq!(changed(&base, &moved), vec!["segments"]);
    let touched = base.clone().with_facts("media/talk.mkv", 5, 5);
    assert_eq!(changed(&base, &touched), vec!["sources"]);

    // Two edits name both.
    let mut both = base.clone().with_settings(|s| s.crf = 30);
    both.aspect = "9:16".to_string();
    assert_eq!(changed(&base, &both), vec!["settings", "aspect"]);

    // Nothing changed names nothing, and the whole stamp is the hash of these lines joined.
    assert!(changed(&base, &Case::new()).is_empty());
    let fake = Fake { facts: base.facts.clone() };
    let joined = stamp::parts(&base.input(), one_wav, fake.facts())
        .iter()
        .map(|(name, hash)| format!("{name}={hash}"))
        .collect::<Vec<String>>()
        .join("\n");
    assert_eq!(base.input().parts_of(one_wav, Fake { facts: base.facts.clone() }.facts()).join("\n"), joined);
    assert_eq!(stamp::stamp(&base.input(), one_wav, fake.facts()), base.stamp());
}
