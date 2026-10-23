// F2.14 Suggest a cut — §2's word gate and §2's one list of word kinds.
//
// spec/05-cut.md F2.14 S1 refuses ▶ when the base cut's recordings have no usable words, and §2
// tells both cut jobs to look at the same four kinds of point "so one list spells the kinds for both
// jobs". The rules live in `src/cut_suggest_words.rs`; this file pins them, including the two §5
// causes that must stay distinguishable on disk: no `words.json` at all, and a `words.json` whose
// aligner never answered.
//
// Every case here works in a throwaway application root through the same `crate::requests` doors the
// program reads the word files with, so a rename of either file breaks this file rather than passing
// on a mock.

use naivepost::cut::{Cut, Seg};
use naivepost::cut_suggest_words::{self as words, WordState};
use naivepost::layout::Tree;
use naivepost::requests::{self, AlignedDoc, Word, WordsDoc};
use naivepost::timeline::Recording;

use crate::common;
use common::temp_dir;

/// A project folder under a throwaway root, gone when the test ends.
struct Dir {
    root: std::path::PathBuf,
    tree: Tree,
}

impl Dir {
    fn fresh(name: &str) -> Self {
        let root = temp_dir(name);
        let dir = root.join(format!("f2_14_{name}.naivepost"));
        std::fs::create_dir_all(&dir).expect("create the fixture project");
        let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
        Self { root, tree }
    }

    fn tree(&self) -> &Tree {
        &self.tree
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// One word of a `words.json` / `words.aligned.json`, at the aligner's 16 kHz sample rate.
fn word(text: &str, start_s: f64) -> Word {
    Word {
        word: text.to_string(),
        start_sample: (start_s * 16000.0) as u64,
        end_sample: ((start_s + 0.3) * 16000.0) as u64,
    }
}

/// Write a recording's `words.json`.
fn write_words(tree: &Tree, source: &str, spoken: &[(&str, f64)]) {
    let doc = WordsDoc {
        text: spoken
            .iter()
            .map(|(w, _)| *w)
            .collect::<Vec<&str>>()
            .join(" "),
        words: spoken.iter().map(|(w, t)| word(w, *t)).collect(),
    };
    requests::write_words(tree, source, &doc).expect("write words.json");
}

/// Write a recording's `words.aligned.json`; `false` stands for "an aligner that did not answer"
/// (§04 F1.9 files the aligner's words, and only its words).
fn write_aligned(tree: &Tree, source: &str, aligned: bool) {
    let doc = AlignedDoc {
        words: if aligned {
            vec![word("hello", 1.0)]
        } else {
            Vec::new()
        },
    };
    requests::write_aligned(tree, source, &doc).expect("write words.aligned.json");
}

/// Leave a recording with a `words.json` that is not JSON: the §5 "words.json unreadable" cause.
fn write_unreadable_words(tree: &Tree, source: &str) {
    std::fs::create_dir_all(tree.input_dir(source)).expect("the input folder");
    std::fs::write(tree.words_json(source), "not json at all").expect("write an unreadable file");
}

/// A cut showing one camera's footage.
fn cam_cut(cam: i32) -> Cut {
    Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 60.0,
            cam,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// The session's recordings: one row per camera slot.
fn recordings(cams: &[i32]) -> Vec<Recording> {
    cams.iter()
        .map(|cam| Recording {
            base: format!("cam{cam}"),
            start: 0.0,
            end: 120.0,
        })
        .collect()
}

#[test]
fn f2_14_s2_the_four_kinds_are_one_list_spelled_once() {
    // §2: "one list spells the kinds for both jobs" — the four kinds exist ONCE, as `Candidate`, and
    // the weights §S3 quotes are that enum's own. A second list of words for the same four places is
    // what would drift, so this pins the enum and its weights rather than a sentence.
    use naivepost::cut_effect_decisions::{
        Candidate, WEIGHT_LINE_EDGE, WEIGHT_SILENCE, WEIGHT_VISUAL_CUT, WEIGHT_WORD_EDGE,
    };
    let kinds = [
        Candidate::SilenceMidpoint,
        Candidate::WordEdge,
        Candidate::LineEdge,
        Candidate::VisualCut,
    ];
    assert_eq!(kinds.len(), 4);
    // §05 F2.14 S3's numbers, in the order the spec lists them: silence 0.8, word edge 0.9, line
    // edge 0.95, visual cut where nobody talks 1.0 — each one read off the kind itself, so a weight
    // can only be stated once.
    assert_eq!(
        Candidate::weight(Candidate::SilenceMidpoint),
        WEIGHT_SILENCE
    );
    assert_eq!(Candidate::weight(Candidate::WordEdge), WEIGHT_WORD_EDGE);
    assert_eq!(Candidate::weight(Candidate::LineEdge), WEIGHT_LINE_EDGE);
    assert_eq!(Candidate::weight(Candidate::VisualCut), WEIGHT_VISUAL_CUT);
    assert_eq!(
        kinds.map(|kind| kind.weight()),
        [0.8_f64, 0.9, 0.95, 1.0],
        "§S3's weights, in §S3's order"
    );
    // And the contest really uses them: at equal distance and direction, the better KIND wins, which
    // is the whole reason the four kinds are one ordered list rather than four ad-hoc checks.
    let score = |kind: Candidate| {
        naivepost::cut_effect_decisions::edge_score(kind, 10.0, 10.5, 5.0, false)
            .expect("inside the tolerance")
    };
    assert!(score(Candidate::VisualCut) > score(Candidate::LineEdge));
    assert!(score(Candidate::LineEdge) > score(Candidate::WordEdge));
    assert!(score(Candidate::WordEdge) > score(Candidate::SilenceMidpoint));
}

#[test]
fn f2_14_s1_no_words_file_refuses_and_names_nothing_more() {
    // §1: no `words.json` anywhere → "Words needed – add them with ✐ Captions first" and nothing
    // runs. §5 calls this the plain cause, so the refusal carries no extra clause. Two cameras, both
    // with no file, so the sentence stays the short one even though two rows are affected.
    let dir = Dir::fresh("missing");
    let tree = dir.tree();
    let cut = Cut {
        segs: vec![
            Seg {
                s: 0.0,
                e: 10.0,
                cam: 0,
                ..Default::default()
            },
            Seg {
                s: 10.0,
                e: 20.0,
                cam: 1,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let recs = recordings(&[0, 1]);

    let states = words::read_word_states(tree, &cut, &recs)
        .expect("reading an empty project is not a failure");
    assert_eq!(states, vec![WordState::Missing, WordState::Missing]);
    assert_eq!(words::gate(&states), words::Words::Needed);
    assert_eq!(words::reason_for(&states), None);
    assert_eq!(
        words::press_answer(&Ok(states)),
        (false, words::WORDS_NEEDED.to_string())
    );
}

#[test]
fn f2_14_s5_words_present_but_never_aligned_refuses_naming_the_cause() {
    // §5: an aligner that did not answer is a DIFFERENT cause from no file — the sentence must say
    // which, so a person knows whether to run ✐ Captions or wait for the aligner. Written by hand so
    // the state is pinned, not inferred from which helper wrote which file.
    let states = vec![WordState::NotAligned];
    assert_eq!(
        words::reason_for(&states),
        Some("1 of 1 recording(s) have words we cannot use: not aligned".to_string())
    );
    let (may_run, said) = words::press_answer(&Ok(states));
    assert!(!may_run);
    assert!(said.starts_with(words::WORDS_NEEDED), "{said}");
    assert!(said.contains("not aligned"), "{said}");
    // ...and it is NOT the no-file sentence: the two causes stay told apart.
    assert!(!said.contains("no words.json"), "{said}");
}

#[test]
fn f2_14_s5_an_unanswered_aligner_does_not_pass_the_gate() {
    // A `words.aligned.json` whose word list is empty is an aligner that answered nothing, which is
    // not "usable" however full the ASR's own file is — so the rule is stated once, over states, and
    // read from both sides: the gate refuses it and the reason names it.
    let states = vec![WordState::NotAligned];
    assert_eq!(words::gate(&states), words::Words::Needed);
    assert!(words::reason_for(&states).is_some_and(|said| said.contains("not aligned")));
    // The file-level spelling of the same case, through the reading door: aligned words present is
    // what Usable means on disk.
    let dir = Dir::fresh("alignedok");
    write_words(dir.tree(), "cam0", &[("hello", 1.0)]);
    write_aligned(dir.tree(), "cam0", true);
    let read = words::read_word_states(dir.tree(), &cam_cut(0), &recordings(&[0])).expect("read");
    assert_eq!(read, vec![WordState::Usable]);
}

#[test]
fn f2_14_s1_aligned_words_let_the_press_run() {
    let dir = fresh_aligned_project("usable");
    let tree = dir.tree();
    let states = words::read_word_states(tree, &cam_cut(0), &recordings(&[0])).expect("read");
    assert_eq!(states, vec![WordState::Usable]);
    assert_eq!(words::gate(&states), words::Words::Present);
    assert_eq!(words::press_answer(&Ok(states)), (true, String::new()));
}

#[test]
fn f2_14_s1_only_the_cameras_the_cut_shows_are_asked_for_words() {
    // cam1 is in the session but the cut is shown entirely from cam0, so cam1's missing words must
    // not hold the press: the gate reads the recordings the cut depends on.
    let dir = Dir::fresh("onecam");
    let tree = dir.tree();
    write_words(tree, "cam0", &[("hello", 1.0)]);
    write_aligned(tree, "cam0", true);

    let recs = recordings(&[0, 1]);
    assert_eq!(
        words::depended_recordings(&cam_cut(0), &recs),
        vec!["cam0".to_string()]
    );
    let states = words::read_word_states(tree, &cam_cut(0), &recs).expect("read");
    assert_eq!(states, vec![WordState::Usable]);
}

#[test]
fn f2_14_s5_two_bad_causes_are_both_said() {
    // A cut showing two cameras, one with no file and one unaligned: the reason names both counts,
    // because §5 wants the cause to be nameable and both are true at once.
    let dir = Dir::fresh("twocauses");
    let tree = dir.tree();
    write_aligned(tree, "cam1", true);
    write_unreadable_words(tree, "cam1");

    let cut = Cut {
        segs: vec![
            Seg {
                s: 0.0,
                e: 10.0,
                cam: 0,
                ..Default::default()
            },
            Seg {
                s: 10.0,
                e: 20.0,
                cam: 1,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let states = words::read_word_states(tree, &cut, &recordings(&[0, 1])).expect("read");
    // cam1's `words.json` is there and unreadable, cam0 has no word file at all — two causes, both
    // said, because §5 wants each one nameable and a plain "Words needed" only covers the case where
    // nothing at all was ever transcribed.
    assert_eq!(states, vec![WordState::Missing, WordState::Unreadable]);
    let (may_run, said) = words::press_answer(&Ok(states));
    assert!(!may_run);
    assert!(
        said.contains("no words.json") && said.contains("words.json unreadable"),
        "{said}"
    );
    assert!(said.contains("2 of 2"), "{said}");
}

#[test]
fn f2_14_s1_a_cut_with_nothing_in_it_needs_no_words() {
    // A page opened before anything was laid out has no segments, so no recording is depended on and
    // the word gate has nothing to refuse for — §1's other guards (hand edits, no timeline) are the
    // ones that answer that press.
    assert_eq!(words::press_answer(&Ok(Vec::new())), (true, String::new()));
    assert_eq!(
        words::depended_cam_slots(&Cut::default()),
        Vec::<i32>::new()
    );
}

#[test]
fn f2_14_s1_a_lane_segment_counts_as_its_own_camera() {
    // A cut lane is a recording too (§01 §B), so the words of the camera a lane shows are depended
    // on exactly like a row's.
    let cut = Cut {
        segs: vec![Seg {
            s: 5.0,
            e: 9.0,
            cam: 2,
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(words::depended_cam_slots(&cut), vec![2]);
}

/// A project whose cam0 has both word files and an aligner that answered.
fn fresh_aligned_project(name: &str) -> Dir {
    let dir = Dir::fresh(name);
    let tree = dir.tree();
    write_words(tree, "cam0", &[("hello", 1.0)]);
    write_aligned(tree, "cam0", true);
    dir
}
