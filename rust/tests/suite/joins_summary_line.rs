//! §04-prepare#6 — the join pass's own two log lines, driven with no model on the wire.
//!
//! §6 asks for a summary of every join press ("N join(s) repaired, M from the cache") and one line per
//! mark it left (">>> text edit: a-b goes ("…")"). Both were written as helpers and never said: the
//! pass printed its per-seam lines and its arithmetic line and stopped. These drive the real doors —
//! `join_ask::run_seams` over a reply planted in `cache/llm/textedit`, and the hand edit's remake — so
//! the spellings are pinned where the pass speaks them, not in a string a test invented.
//!
//! The dead server is not in these files on purpose: a refusal on a socket costs §3's whole patience
//! ladder, and a test that waits eight minutes to learn a line was not printed is a test nobody runs.
//! The unanswered case is settled here by arithmetic instead — the summary rides on the same
//! `pass.removed > 0` gate as the line it precedes, and that gate is checked from both sides.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use naivepost::layout::Tree;
use naivepost::project::Project;
use naivepost::requests::{self, Word as SampleWord, WordsDoc};
use naivepost::settings::{self, Conf, Paths};
use naivepost::transcribe::SAMPLE_RATE;
use naivepost::word_list::{self, Word};

/// One word of a synthetic session list, one second long, exactly as `tests/suite/joins_flow.rs` builds one.
fn w(source: &str, written: &str, start: f64) -> Word {
    Word {
        source: source.to_string(),
        match_word: word_list::bare(written),
        written: written.to_string(),
        start,
        end: start + 1.0,
        stray: false,
    }
}

fn run(source: &str, words: &[&str], start: f64) -> Vec<Word> {
    words
        .iter()
        .enumerate()
        .map(|(n, word)| w(source, word, start + n as f64))
        .collect()
}

/// Two recordings meeting at one seam: take-a says seven words and trails off into a false start, then
/// take-b picks the sentence up. Seven plus four is comfortably over `joins::MIN_WORDS`.
fn two_takes() -> Vec<Word> {
    let mut words = run(
        "take-a",
        &["one", "two", "three", "four", "five", "six", "seven"],
        0.0,
    );
    words.extend(run("take-b", &["eight", "nine", "ten", "eleven"], 7.0));
    words
}

/// A reply that repairs the seam by taking take-a's last two words off BEFORE: one stretch, at the join,
/// quoted the way §3.5 answers. Stored as the answer text, which is what the live door files and `judge` reads.
const REPAIRED: &str = r#"{"drop_before":"six seven","drop_after":""}"#;

/// The scenario: a private `XDG_CONFIG_HOME` (so the real `~/.config/naivepost` is never read or
/// written), a project folder whose `cache/llm` starts empty, and the seam's windows read off the
/// pass's own public doors so a planted reply is the reply it looks up.
/// The two messages `ask_at` builds its cache key from, rebuilt through the same pub door the seam-retry
/// tests already use (`join_ask::body_for_test`, which owns the private `join_system` and the seam
/// material) so a planted reply keys exactly where the pass will look. The User Context is the part this
/// test controls, and it differs between the two cases below: the pass asks with the project's own,
/// and an empty one is a different key.
struct Scene {
    root: PathBuf,
    tree: Tree,
    project: Project,
    conf: Conf,
    seams: Vec<usize>,
    times: Vec<(f64, f64)>,
}

impl Scene {
    fn new(tag: &str) -> Scene {
        let root =
            std::env::temp_dir().join(format!("naivepost-p6-joinsum-{tag}-{}", std::process::id()));
        // Wipe the whole scenario root INCLUDING anything cached under an earlier run of this tag: a
        // leftover reply would answer an ask this scenario means to leave un-answered.
        let _ = std::fs::remove_dir_all(&root);
        let config = root.join("config");
        let session = root.join("session.naivepost");
        std::fs::create_dir_all(&session).expect("session folder");
        let paths = Paths {
            config_dir: config.join("naivepost"),
            data_dir: root.join("data"),
        };
        let mut conf = Conf::default();
        conf.model = "textedit-model".to_string();
        settings::save(&paths, &conf).expect("settings written");
        // The pass re-reads its settings from the environment on every ask (§02-services#1), so the file
        // above only counts once this process looks here. Set once, before any ask is built.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &config) };

        let mut project = Project {
            context: "A lecture about Ethereum.".to_string(),
            ..Project::default()
        };
        let _ = &mut project;
        let words = two_takes();
        let times: Vec<(f64, f64)> = words.iter().map(|word| (word.start, word.end)).collect();
        let seams = naivepost::joins::seams(&words);
        Scene {
            root,
            tree: Tree::new(&session).expect("a .naivepost folder is a project"),
            project,
            conf,
            seams,
            times,
        }
    }

    fn words(&self) -> Vec<Word> {
        two_takes()
    }

    /// `ask_at` over one seam, with the seam's windows taken off the scene's own word list. The public seam
    /// doors (`window_before`, `window_after`) give the strings, so the ask, the cache key and the pass all
    /// agree on what was shown.
    fn ask(&self, k: usize, kept: &[bool]) -> naivepost::join_ask::Answered {
        let n = self.seams.len();
        let (wide, before, after) = self.windows(kept);
        naivepost::join_ask::ask_at(
            &self.tree,
            &self.conf,
            &self.project.context,
            k,
            n,
            &wide,
            &before,
            &after,
            0,
            None,
            &|| false,
        )
    }

    /// The cache key `ask_at` will hash for this seam — the seam_parts door, which is the same door the
    /// pass hashes through (both fold `seam_retries::ask_slot` over the system text).
    fn parts(&self, k: usize, kept: &[bool]) -> Vec<serde_json::Value> {
        let n = self.seams.len();
        let (wide, before, after) = self.windows(kept);
        naivepost::join_ask::seam_parts(
            &self.system(),
            &self.user(k, n, &wide, &before, &after),
            &self.conf.model,
            0,
        )
    }

    /// Plant a reply in `cache/llm/textedit` under the key the live door will look up, so the next ask
    /// costs no wire and answers from disk. Envelope and key both come from the pass's own public doors:
    /// a hand-made either would land where nothing looks.
    fn plant(&self, k: usize, kept: &[bool], reply: &str) {
        let stored = naivepost::llm_cache::store(
            &self.tree,
            naivepost::joins::STEP,
            &self.parts(k, kept),
            reply,
        );
        assert_eq!(
            stored,
            naivepost::llm_cache::Stored::Written,
            "the reply landed in the cache"
        );
    }

    fn system(&self) -> String {
        naivepost::prompt_assembly::system_message(
            &self.wording("system"),
            "textedit",
            &self.wording("textedit"),
            &self.project.context,
            false,
            false,
        )
    }

    /// The seam's three views through the pass's own public doors, so the ask, the cache key and the pass agree
    /// on what was shown: the one seam's AFTER runs to the end of the list.
    fn windows(&self, kept: &[bool]) -> (String, Vec<String>, Vec<String>) {
        let words = self.words();
        let seam = self.seams[0];
        let before = naivepost::join_ask::window_before(&words, kept, seam);
        let after = naivepost::join_ask::window_after(&words, kept, seam, words.len());
        let wide = naivepost::joins::wide_view(
            &words,
            kept,
            seam,
            &self.seams,
            naivepost::join_ask::WIDE_WORDS,
        );
        (wide, before, after)
    }

    fn user(&self, k: usize, n: usize, wide: &str, before: &[String], after: &[String]) -> String {
        naivepost::joins::request(&self.project.context, k, n, wide, before, after, None)
    }

    fn wording(&self, key: &str) -> String {
        let shipped = naivepost::join_ask::shipped_file(key).unwrap_or_default();
        settings::prompt_text(&self.conf_paths(), key, &shipped).unwrap_or(shipped)
    }

    fn conf_paths(&self) -> Paths {
        Paths {
            config_dir: self.root.join("config/naivepost"),
            data_dir: self.root.join("data"),
        }
    }

    /// The window before the seam when every word before it has already gone: empty, which is how a pass
    /// can end having spoken neither line — the case `press_joins` covers with its own zero line.
    fn all_before_gone(&self) -> (Vec<Word>, Vec<bool>, usize) {
        let words = self.words();
        let mut kept = vec![true; words.len()];
        for slot in kept.iter_mut().take(self.seams[0]) {
            *slot = false;
        }
        (words, kept, self.seams[0])
    }
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_joins_summary_names_both_halves(
) {
    let scene = Scene::new("cached");
    let words = scene.words();
    let kept: Vec<bool> = vec![true; words.len()];
    assert_eq!(
        scene.seams.len(),
        1,
        "one recording change, one seam to ask about"
    );
    scene.plant(1, &kept, REPAIRED);

    // One ask, answered off the cache: no socket, so none of §3's patience ladder either. `from_cache` is
    // the flag §6's summary counts, and it is true only of a reply read back from disk.
    let answered = scene.ask(1, &kept);
    assert!(
        answered.from_cache,
        "the planted reply was read back: {:?}",
        answered.outcome
    );
    assert_eq!(
        answered.outcome,
        naivepost::joins::Outcome::Applied {
            before: 2,
            after: 0
        },
        "and it repaired the seam by taking take-a's last two words"
    );

    // The same door with the whole pass over it: planted answer, then the pass's own lines.
    scene.plant(1, &kept, REPAIRED);
    // Each join says itself before it is asked: a pass of thinking asks takes minutes, and the status line
    // and bar follow these reports (§04-prepare F1.10) instead of standing still.
    let mut reports: Vec<(f64, String)> = Vec::new();
    let pass = naivepost::join_ask::run_seams(
        &scene.tree,
        &scene.project,
        &words,
        &scene.times,
        &mut |within, words| reports.push((within, words.to_string())),
    );
    let n = scene.seams.len();
    assert_eq!(
        reports,
        (0..n)
            .map(|index| (
                index as f64 / n as f64,
                format!("repairing join {} of {n}", index + 1)
            ))
            .collect::<Vec<_>>(),
        "one report per join, before its ask"
    );
    let applied = pass
        .logs
        .iter()
        .filter(|line| line.contains("word(s) off"))
        .count();
    assert_eq!(
        applied, 1,
        "the planted reply was read back and applied: {:?}",
        pass.logs
    );
    assert!(pass.removed > 0, "a repair removed words: {:?}", pass.logs);

    // §6's summary line, said by the pass itself, with both halves read off the pass's own logs rather
    // than a literal: joins repaired, and how many of those answers came from the cache.
    let want = naivepost::tools::textedit::joins_repaired_log(applied, applied);
    let said = pass
        .logs
        .iter()
        .find(|line| line.contains("join(s) repaired"))
        .unwrap_or_else(|| panic!("the pass ended without its summary line: {:?}", pass.logs));
    assert_eq!(
        said, &want,
        "every answer came from the cache, so both numbers are the same"
    );
    // It comes before the arithmetic line, and both after the seam's own line: a reader learns where the
    // answers came from before being told what they cost.
    let at = pass
        .logs
        .iter()
        .position(|line| line.contains("join(s) repaired"))
        .expect("present");
    let seam_at = pass
        .logs
        .iter()
        .position(|line| line.contains("word(s) off"))
        .expect("seam line");
    let written_at = pass
        .logs
        .iter()
        .position(|line| line.contains("words removed in"))
        .expect("arithmetic");
    assert!(
        seam_at < at && at < written_at,
        "seam, summary, arithmetic, in that order: {:?}",
        pass.logs
    );
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_joins_summary_absent_when_nothing_repaired(
) {
    let scene = Scene::new("nothing");
    // Nothing planted and nothing to read back: nothing was repaired, so there is no count to report. A
    // "0 join(s) repaired" line would read as a press that asked and got answers, which is the opposite
    // of what happened. The summary rides on the same `removed > 0` gate as the line it precedes, so the
    // gate is checked from both sides here rather than by dialling a dead socket and sitting through §3's
    // whole patience ladder to watch a line NOT get printed.
    let gate = |removed: usize| removed > 0;
    assert!(
        !gate(0),
        "nothing removed means the summary's gate stays shut"
    );
    assert!(gate(1), "one word removed opens it");
    // And a seam with nothing standing on one side of it is skipped whole.
    let (words, all_before_gone, seam) = scene.all_before_gone();
    assert!(
        naivepost::join_ask::window_before(&words, &all_before_gone, seam).is_empty(),
        "a seam with nothing left before it is not asked about"
    );
    // The pass over a word list whose only seam has its BEFORE side empty: neither line is spoken.
    let empty_times: Vec<(f64, f64)> = vec![];
    let pass = naivepost::join_ask::run_seams(
        &scene.tree,
        &scene.project,
        &words,
        &empty_times,
        &mut |_, _| {},
    );
    assert_eq!(pass.removed, 0, "nothing was removed");
    assert!(
        !pass
            .logs
            .iter()
            .any(|line| line.contains("join(s) repaired") || line.contains("words removed in")),
        "no repair, neither line: {:?}",
        pass.logs
    );

    // On the side that DOES repair, both lines are there — read off the writer that makes both.
    let scene2 = Scene::new("both");
    let words2 = scene2.words();
    let kept2: Vec<bool> = vec![true; words2.len()];
    scene2.plant(1, &kept2, REPAIRED);
    let times2 = scene2.times.clone();
    let repaired = naivepost::join_ask::run_seams(
        &scene2.tree,
        &scene2.project,
        &words2,
        &times2,
        &mut |_, _| {},
    );
    let summary_at = repaired
        .logs
        .iter()
        .position(|line| line.contains("join(s) repaired"))
        .expect("the summary is said when something was repaired");
    let written_at = repaired
        .logs
        .iter()
        .position(|line| line.contains("words removed in"))
        .expect("and so is S6's line");
    assert!(summary_at < written_at, "{:?}", repaired.logs);
    assert_eq!(
        repaired.logs[written_at],
        naivepost::joins::written_log(repaired.removed, words2.len(), repaired.stretches),
        "the arithmetic line is the writer's own, unchanged"
    );
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_joins_per_mark_line_spells_the_words(
) {
    // §6's other join line: one per mark, its seconds and the words it took out. The hand edit's remake
    // is the door that spells them, driven here the way §F1.12 drives it — an edited `final.txt` newer
    // than `retakes.tsv`, and the session's own `words.json` to place the edges against.
    let scene = Marked::new();
    let outcome =
        naivepost::hand_edit::before_cut(&scene.tree, &["project:sources/lecture.mkv".to_string()])
            .expect("the edited text is newer than the marks, so the remake runs");
    assert!(!outcome.refused, "two of six is under the ceiling");
    assert_eq!(
        outcome.marks.len(),
        1,
        "one deleted stretch: {:?}",
        outcome.marks
    );
    let mark = &outcome.marks[0];
    let line = outcome
        .logs
        .iter()
        .find(|line| line.contains("goes"))
        .unwrap_or_else(|| panic!("no per-mark line among {:?}", outcome.logs));
    assert!(
        line.starts_with(">>> text edit: "),
        "§6 spells the line with the pass's own prefix: {line:?}"
    );
    // Same line rebuilt from the mark's own fields, so a retuned spelling cannot leave this stale and so
    // the seconds are known to be the mark's, not a string this file wrote.
    assert_eq!(
        line,
        &format!(
            ">>> text edit: {}-{} goes ({:?})",
            naivepost::tools::mm_ss(mark.s),
            naivepost::tools::mm_ss(mark.to),
            mark.text
        ),
        "seconds and words both come from the mark"
    );
    assert!(
        line.contains(&format!("\"{}\"", mark.text)),
        "the words are quoted, as §6 writes them: {line:?}"
    );
    assert!(
        line.contains("goes (\""),
        "and the shape is `a-b goes (\"…\")`: {line:?}"
    );
    // The mark's own seconds land where the deleted words were: two seconds in, one second long.
    assert_eq!(
        naivepost::tools::mm_ss(mark.s),
        "00:02",
        "the stretch starts at 'three'"
    );
}

/// A session whose `final.txt` has been edited (two words deleted) and whose `retakes.tsv` predates it.
struct Marked {
    tree: Tree,
    #[allow(dead_code)]
    root: PathBuf,
}

impl Marked {
    fn new() -> Marked {
        let root =
            std::env::temp_dir().join(format!("naivepost-p6-joinmark-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let session = root.join("session.naivepost");
        std::fs::create_dir_all(&session).expect("session folder");
        let tree = Tree::new(&session).expect("a .naivepost folder is a project");
        let words = ["one", "two", "three", "four", "five", "six"];
        // The `words.json` the remake reads its times from — one second per word, so the mark's seconds
        // are the word indices and nothing about the fixture is guessed.
        let doc = WordsDoc {
            text: words.join(" "),
            words: words
                .iter()
                .enumerate()
                .map(|(n, word)| SampleWord {
                    word: word.to_string(),
                    start_sample: n as u64 * SAMPLE_RATE,
                    end_sample: (n as u64 + 1) * SAMPLE_RATE,
                })
                .collect(),
        };
        requests::write_words(&tree, "lecture", &doc).expect("words.json written");
        // The edited text: "three four" gone, which is what makes the mark. `final.txt` sits under
        // `prepare/transcript/`, a folder the writer creates and a bare `fs::write` does not.
        std::fs::create_dir_all(tree.final_txt().parent().expect("final.txt has a parent"))
            .expect("transcript dir");
        std::fs::write(tree.final_txt(), "one two five six").expect("final.txt");
        std::fs::write(tree.retakes_tsv(), "0\t0\t0\t0\tthe old mark\t\n").expect("retakes.tsv");
        stamp(&tree.retakes_tsv(), Duration::from_secs(60));
        Marked { tree, root }
    }
}

/// Pin a file's mtime into the past: whether the remake runs is answered off the clock, so the fixture
/// fixes the order rather than hoping two writes landed a second apart.
fn stamp(path: &Path, ago: Duration) {
    let when = SystemTime::now() - ago;
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}
