//! F1.12 Hand-edit the text (spec/04-prepare.md) — final.txt edited by hand in any text editor; when it
//! is newer than retakes.tsv the marks are remade from it with no model asked, never-said words are
//! dropped with a warning and more than 40 % removed is refused.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use naivepost::hand_edit as edit;
use naivepost::layout::Tree;
use naivepost::textfmt::{self, Retake};
use naivepost::tools::retakes::RETAKE_CEIL;
use naivepost::wave::Wave;

const HZ: f64 = 200.0;

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-handedit-{tag}-{}-{}.naivepost",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tree(tag: &str) -> Tree {
    Tree::new(&temp(tag)).unwrap()
}

/// `final.txt` and `retakes.tsv` sit in the transcript folder, which is the app's to create — a test
/// writing one directly makes its folder first.
fn transcript(dir: &Path) -> PathBuf {
    let folder = dir.join("prepare/transcript");
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// Write `final.txt` and hand back the tree, which is what a user's editor effectively does.
fn write_final(tree: &Tree, text: &str) {
    edit::write_final(tree, text).unwrap();
}

/// Ten spoken words, one second each.
const WORDS: [&str; 10] = [
    "so", "we", "take", "the", "clip", "and", "cut", "it", "there", "twice",
];

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|word| (*word).to_string()).collect()
}

/// Word `n` runs from n to n + 0.9 seconds.
fn times(count: usize) -> Vec<(f64, f64)> {
    (0..count).map(|n| (n as f64, n as f64 + 0.9)).collect()
}

/// The file as Prepare writes it, then as any text editor hands it back after words were deleted: one
/// sentence per line (so the word list is whitespace-separated), and a trailing newline that is not a word.
fn text(list: &[&str]) -> String {
    format!("{}\n", list.join(" "))
}

// --- S1: the user edits prepare/transcript/final.txt in any text editor ----------------------------

#[test]
fn f1_12_s1_the_edited_file_is_read_back_as_words_and_joins() {
    let tree = tree("read");
    // Two words deleted, a `|cut 3|` join kept where three went, and the prototype's whole-take spelling,
    // which carries no words at all.
    let written = "so we take |cut 3| and cut |cut|whole take| there twice\n";
    write_final(&tree, written);

    let read = std::fs::read_to_string(tree.final_txt()).unwrap();
    let (kept, joins) = textfmt::read_final(&read);
    assert_eq!(
        kept,
        words(&["so", "we", "take", "and", "cut", "there", "twice"])
    );
    assert_eq!(joins, vec![3, 0], "both marks are joins; neither contributes a word");

    // Deleting the join marks too is the same edit: what goes is in `retakes.tsv`, not in the text.
    let (same, _) = textfmt::read_final("so we take and cut there twice\n");
    assert_eq!(same, kept);
}

#[test]
fn f1_12_s1_the_file_round_trips_through_the_writer() {
    let tree = tree("roundtrip");
    let surviving = words(&["so", "we", "take", "and", "cut", "there", "twice"]);
    // One join with nothing gone is the bare `|cut|` §6 spells out.
    write_final(&tree, &textfmt::write_final(&surviving, &[0, 0, 2, 0, 0, 0]));

    let read = std::fs::read_to_string(tree.final_txt()).unwrap();
    assert!(read.contains("|cut 2|"), "{read}");
    let (kept, joins) = textfmt::read_final(&read);
    assert_eq!(kept, surviving);
    assert_eq!(joins.len(), 6);
}

// --- S1: the next Cut ▶ reads that same file, markers and all -------------------------------------

/// The edited file with its join marks LEFT IN — the spelling `f1_12_s1_the_edited_file_is_read_back_as_words_and_joins`
/// writes. Kept as a constant so all three tests here read the very same bytes a text editor would hand back.
const MARKED: &str = "so we take |cut 3| and cut |cut|whole take| there twice\n";

#[test]
fn f1_12_s1_join_marks_left_in_the_text_are_not_read_as_words() {
    // Derivation of the expected count. `read_final` strips every marker and contributes NO word for
    // it (proven at line 79: `joins == [3, 0]`), so the words this text leaves are
    //   so, we, take, and, cut, there, twice  = 7 of the 10 spoken.
    // What goes is therefore 3 -- exactly what `|cut 3|` names; the whole-take field carries no number
    // of its own and adds nothing to the count. P.machine.retakeCeil = 0.4 keeps 3/10 an edit.
    let marked = edit::remake(MARKED, &words(&WORDS), &times(10), |_| None);
    assert_eq!(marked.dropped, 3, "the three the |cut 3| marker stands for");
    assert_eq!(marked.extra, 0, "a marker is a mark, never a never-said word");
    assert!(!marked.refused, "3 of 10 is under the ceiling");
    assert!(
        marked.marks.iter().all(|mark| !mark.text.contains('|')),
        "no marker survives into a mark's text: {:?}",
        marked.marks
    );
}

#[test]
fn f1_12_s1_join_marks_removed_leaves_the_same_marks_as_keeping_them() {
    // The spec's clause "join marks may stay or go": whether the user left them in cannot change what
    // the cut reads, because the markers name the deletion rather than add to it.
    let marked = edit::remake(MARKED, &words(&WORDS), &times(10), |_| None);
    let plain = edit::remake(
        &text(&["so", "we", "take", "and", "cut", "there", "twice"]),
        &words(&WORDS),
        &times(10),
        |_| None,
    );
    assert_eq!(marked.dropped, plain.dropped);
    assert_eq!(marked.extra, plain.extra);
    assert_eq!(marked.marks.len(), plain.marks.len(), "same number of stretches");
    for (kept, gone) in marked.marks.iter().zip(plain.marks.iter()) {
        assert_eq!((kept.s, kept.e, kept.to), (gone.s, gone.e, gone.to));
    }
}

#[test]
fn f1_12_s1_before_cut_reads_a_marked_final_txt() {
    let tree = tree("disk-marked");
    transcript(tree.dir());
    write_words(&tree, &WORDS);
    seed_marks(&tree);
    write_final(&tree, MARKED);
    set_stamp(&tree.retakes_tsv(), 60);
    set_stamp(&tree.final_txt(), 0);
    let written = std::fs::read(tree.final_txt()).unwrap();

    let outcome = edit::before_cut(&tree, &[SRC.to_string()]).expect("the edited text is newer");
    assert_eq!(outcome.logs[0], edit::EDITED_NOTE);
    assert_eq!(outcome.dropped, 3, "the markers were read as the deletion they name");

    let on_disk = textfmt::read_retakes(&tree.retakes_tsv()).unwrap();
    assert_eq!(on_disk.len(), outcome.marks.len(), "what was written is what was decided");
    for (row, mark) in on_disk.iter().zip(outcome.marks.iter()) {
        assert_eq!((row.s, row.e, row.to), (mark.s, mark.e, mark.to));
        assert!(!row.text.contains('|'), "{row:?}");
    }
    // The remake writes `retakes.tsv` only: the user's edited file is left byte-for-byte alone,
    // markers still in it, so re-opening it in an editor shows what they typed.
    assert_eq!(std::fs::read(tree.final_txt()).unwrap(), written);
}

// --- S2: the next Cut ▶ notices, and remakes the marks from the text ------------------------------

fn stamp(seconds_ago: u64) -> SystemTime {
    SystemTime::now() - Duration::from_secs(seconds_ago)
}

#[test]
fn f1_12_s2_only_a_newer_text_is_an_edit() {
    let (new, old) = (stamp(0), stamp(60));
    assert!(edit::edited(Some(new), Some(old)), "the text was edited after Prepare");
    assert!(!edit::edited(Some(old), Some(new)), "the model's marks stand");
    let same = stamp(30);
    assert!(!edit::edited(Some(same), Some(same)), "equal times are not newer");
    assert!(edit::edited(Some(new), None), "nothing marked yet: the edit is waiting");
    assert!(!edit::edited(None, Some(old)), "no text, nothing to read");
    assert!(!edit::edited(None, None));
}

#[test]
fn f1_12_s2_the_files_own_times_are_what_count() {
    let dir = temp("mtimes");
    let (final_txt, retakes) = (transcript(&dir).join("final.txt"), transcript(&dir).join("retakes.tsv"));
    std::fs::write(&final_txt, textfmt::write_final(&words(&WORDS), &[])).unwrap();
    textfmt::write_retakes(&[], &retakes).unwrap();

    // The marks are as fresh as the text: Cut keeps what the model said.
    let mtime = |path: &Path| std::fs::metadata(path).unwrap().modified().ok();
    assert!(!edit::edited(mtime(&final_txt), mtime(&retakes)));

    // The user saves the file from their editor and it is newer than both.
    let later = SystemTime::now() + Duration::from_secs(2);
    std::fs::File::open(&final_txt).unwrap().set_modified(later).unwrap();
    assert!(edit::edited(mtime(&final_txt), mtime(&retakes)));
}

#[test]
fn f1_12_s2_two_words_deleted_become_one_stretch_with_no_model() {
    let outcome = edit::remake(
        // "cut" and "it" deleted; neither word appears anywhere else in the sentence, so the backwards
        // match cannot borrow one for the other and both drop out where they were said.
        &text(&["so", "we", "take", "the", "clip", "and", "there", "twice"]),
        &words(&WORDS),
        &times(10),
        |_| None,
    );

    assert!(!outcome.refused);
    assert_eq!(outcome.dropped, 2);
    assert_eq!(outcome.extra, 0);
    assert_eq!(outcome.marks.len(), 1);
    let mark = &outcome.marks[0];
    assert_eq!((mark.s, mark.e), (6.0, 7.9));
    // Hand-editing has no second take to read instead: the words are all that goes.
    assert_eq!(mark.again, 0.0);
    assert_eq!(mark.to, mark.e);
    assert_eq!(mark.text, "cut it");

    assert_eq!(outcome.logs[0], edit::EDITED_NOTE);
    assert_eq!(outcome.logs.last().unwrap(), ">>> text edit: 2 of 10 words removed in 1 stretch(es)");
    assert!(
        outcome.logs.contains(&">>> text edit: 00:06-00:07 goes (\"cut it\")".to_string()),
        "{:?}",
        outcome.logs
    );
}

#[test]
fn f1_12_s2_two_separate_deletions_are_two_stretches() {
    let outcome = edit::remake(
        // "we" and "there" deleted: two runs, two marks.
        &text(&["so", "take", "the", "clip", "and", "cut", "it", "twice"]),
        &words(&WORDS),
        &times(10),
        |_| None,
    );
    assert_eq!(outcome.dropped, 2);
    assert_eq!(outcome.marks.len(), 2);
    assert_eq!((outcome.marks[0].s, outcome.marks[0].to), (1.0, 1.9));
    assert_eq!((outcome.marks[1].s, outcome.marks[1].to), (8.0, 8.9), "there went out alone");
}

#[test]
fn f1_12_s2_a_word_that_was_never_said_is_left_out_with_a_warning() {
    let outcome = edit::remake(
        // A word nobody spoke, spelled like nothing else in the sentence: the match runs backwards and
        // cannot borrow an earlier word for it, so it is left out rather than shifting anything.
        &text(&["so", "we", "take", "the", "clip", "and", "cut", "it", "typo", "there", "twice"]),
        &words(&WORDS),
        &times(10),
        |_| None,
    );

    assert_eq!(outcome.extra, 1);
    assert_eq!(outcome.dropped, 0, "nothing spoken was lost by the typing");
    assert!(outcome.marks.is_empty());
    assert!(outcome.logs.contains(&"!!! text edit: 1 word(s) in the text were never said -- left out, there is no sound for them".to_string()), "{:?}", outcome.logs);
}

#[test]
fn f1_12_s2_more_than_the_ceiling_is_refused_and_nothing_is_marked() {
    // P.machine.retakeCeil = 0.4: more than 40 % removed is refused, 40 % is an edit.
    assert_eq!(RETAKE_CEIL, 0.4);

    let five = text(&["we", "the", "and", "it", "there"]);
    let outcome = edit::remake(&five, &words(&WORDS), &times(10), |_| None);
    assert!(outcome.refused);
    assert_eq!(outcome.dropped, 5);
    assert!(outcome.marks.is_empty(), "nothing is marked");
    assert_eq!(
        outcome.logs.last().unwrap(),
        "!!! text edit: 5 of 10 words removed -- refused, that is not an edit, nothing is marked"
    );

    // Exactly 40 % is a hand edit the user meant.
    let four = text(&["so", "we", "clip", "and", "it", "there"]);
    let outcome = edit::remake(&four, &words(&WORDS), &times(10), |_| None);
    assert!(!outcome.refused);
    assert_eq!(outcome.dropped, 4, "exactly the ceiling is an edit");

    // And with nothing spoken at all there is neither an edit nor a refusal.
    let outcome = edit::remake(&text(&[]), &words(&WORDS), &[], |_| None);
    assert!(!outcome.refused);
    assert!(outcome.marks.is_empty());
}

#[test]
fn f1_12_s2_a_word_only_matching_far_back_was_not_said() {
    // KEEP_REACH: 260 words, and the text's tail word exists only at the very start — past the reach, so
    // it is a typing rather than that word.
    assert_eq!(edit::KEEP_REACH, 200);
    let spoken: Vec<String> = (0..260).map(|n| format!("w{n}")).collect();
    // The text is the whole recording minus its last word, with that word's earlier twin glued on the end.
    let typed: Vec<String> = spoken[..259]
        .iter()
        .cloned()
        .chain(std::iter::once("w0".to_string()))
        .collect();
    let (kept, extra) = edit::keep_mask(&spoken, &typed);
    assert_eq!(extra, 1, "no match across the whole recording");
    // The text's tail is matched first and finds no copy within the reach, so exactly one word goes
    // unmatched and nothing else is shifted out of place.
    assert_eq!(kept.iter().filter(|keep| !**keep).count(), 1);
    assert!(!kept[259], "the last word is the one nothing matched");

    // Within the reach it is the same word, and nothing is extra.
    // The same text one step shorter: now every word has its own copy within the reach, and nothing is
    // extra — which is what makes the reach a reach rather than a refusal to match at all.
    let near: Vec<String> = spoken[..258].iter().cloned().chain(std::iter::once("w0".to_string())).collect();
    let (kept, extra) = edit::keep_mask(&spoken, &near);
    assert_eq!(extra, 1, "the tail copy is still out of reach");
    assert_eq!(kept.iter().filter(|keep| !**keep).count(), 2);
}

#[test]
fn f1_12_s2_writing_the_marks_once_stops_the_remake() {
    let tree = tree("writeonce");
    write_final(
        &tree,
        &text(&["so", "we", "take", "the", "clip", "and", "there", "twice"]),
    );

    let outcome = edit::remake(
        &std::fs::read_to_string(tree.final_txt()).unwrap(),
        &words(&WORDS),
        &times(10),
        |_| None,
    );
    edit::write_marks(&tree, &outcome.marks).unwrap();

    let marks = textfmt::read_retakes(&tree.retakes_tsv()).unwrap();
    assert_eq!(marks.len(), 1);
    assert_eq!((marks[0].s, marks[0].to), (6.0, 7.9));

    // The fresh file is newer than the text it was made from, so the next Cut ▶ reads the marks and does
    // not ask the model or remake the edit a second time.
    let mtime = |path: &PathBuf| std::fs::metadata(path).unwrap().modified().ok();
    assert!(!edit::edited(mtime(&tree.final_txt()), mtime(&tree.retakes_tsv())));

    // An empty mark list is still written: "asked, none" is a file, not an absence.
    edit::write_marks(&tree, &[]).unwrap();
    assert_eq!(textfmt::read_retakes(&tree.retakes_tsv()).unwrap(), Vec::<Retake>::new());
}

#[test]
fn f1_12_s2_the_sound_moves_an_edge_when_there_is_one_to_ask() {
    let spoken = words(&WORDS);
    let when = times(10);
    let edited = text(&["so", "we", "take", "the", "clip", "and", "there", "twice"]);

    // No envelope: the edges stay on the word times.
    let plain = edit::remake(&edited, &spoken, &when, |_| None);
    assert_eq!((plain.marks[0].s, plain.marks[0].to), (6.0, 7.9));
    assert!(plain.logs.iter().all(|line| !line.contains("the sound before it stops")));

    // An envelope with quiet before the dropped run ends the cut there instead, and says so. A bucket is
    // 5 ms at this rate; the word levels are what F1.11's tests use.
    let mut peaks = vec![8u8; (20.0 * HZ) as usize];
    // Sound up to 5.9 s, quiet from there to past the deleted run: the cut ends where the sound stops and
    // nothing offers a resume edge, since hand-editing has no second take to reach back to.
    for bucket in (5.0 * HZ) as usize..(5.9 * HZ) as usize {
        peaks[bucket] = 120;
    }
    let envelope = naivepost::edges::Edges::new(Wave { hz: HZ, chans: vec![peaks] }, 0.0);
    let placed = edit::remake(&edited, &spoken, &when, |_| Some(&envelope));

    // The last sound bucket ends at 5.9 s, and the pad leaves 0.05 s of it: 5.9 + 0.05.
    assert_eq!((placed.marks[0].s, placed.marks[0].to), (5.95, 7.9));
    assert!(
        placed.logs.iter().any(|line| line.contains("the sound before it stops")),
        "{:?}",
        placed.logs
    );
    // Both runs report the same edit; only the placement differs.
    assert_eq!(placed.dropped, plain.dropped);
    assert_eq!(placed.logs.last(), plain.logs.last());
}

// --- S2 through the disk: what the next Cut ▶ finds when it asks -------------------------------

/// A source's `words.json` with one word per second, as ASR writes it at 16 kHz.
fn write_words(tree: &Tree, list: &[&str]) {
    // Keyed as Prepare keys it (`prepare_data::base`): `lecture`, not `lecture.mkv`.
    let source = "lecture";
    use naivepost::requests::{Word, WordsDoc};
    const ASR_HZ: u64 = 16_000;
    let words = list
        .iter()
        .enumerate()
        .map(|(n, word)| Word {
            word: (*word).to_string(),
            start_sample: (n as u64) * ASR_HZ,
            end_sample: (n as u64 + 1) * ASR_HZ - (ASR_HZ / 10),
        })
        .collect();
    naivepost::requests::write_words(
        tree,
        source,
        &WordsDoc {
            text: list.join(" "),
            words,
        },
    )
    .unwrap();
}

/// The marks that stand before the edit: one old stretch nobody is about to keep.
fn seed_marks(tree: &Tree) -> String {
    std::fs::write(
        tree.retakes_tsv(),
        "1.0\t2.0\t0.0\t2.0\tthe old mark\n",
    )
    .unwrap();
    std::fs::read_to_string(tree.retakes_tsv()).unwrap()
}

/// Set a file's mtime by hand so the two files' order is fixed rather than raced.
fn set_stamp(path: &Path, offset: u64) {
    let when = SystemTime::now() - Duration::from_secs(offset);
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(when).unwrap();
}

const SRC: &str = "lecture.mkv";

#[test]
fn f1_12_s2_before_cut_remakes_and_writes_the_marks() {
    let tree = tree("disk-remake");
    transcript(tree.dir());
    // P.machine.retakeCeil = 0.4: two of ten words removed is an edit, not a refusal.
    write_words(&tree, &WORDS);
    let before = seed_marks(&tree);
    write_final(&tree, &text(&["so", "we", "take", "the", "clip", "and", "there", "twice"]));
    set_stamp(&tree.retakes_tsv(), 60);
    set_stamp(&tree.final_txt(), 0);

    let outcome = edit::before_cut(&tree, &[SRC.to_string()]).expect("the edited text is newer");
    assert!(!outcome.refused, "two of ten removed is under the ceiling");
    assert_eq!(outcome.logs[0], edit::EDITED_NOTE);
    assert_eq!(outcome.marks.len(), 1, "one deleted stretch");
    assert_eq!(outcome.dropped, 2);

    let on_disk = textfmt::read_retakes(&tree.retakes_tsv()).unwrap();
    assert_eq!(on_disk.len(), 1, "the remade mark replaced the old one");
    assert_ne!(
        on_disk[0].text, "the old mark",
        "what is on disk now came from the edited text"
    );
    // The marks file now answers S2's question with "no": stamped ahead of the text so the tie a
    // coarse filesystem clock could leave cannot stand in for the rule being proved.
    set_stamp(&tree.retakes_tsv(), 0);
    assert!(
        !edit::edited(
            std::fs::metadata(tree.final_txt())
                .unwrap()
                .modified()
                .ok(),
            std::fs::metadata(tree.retakes_tsv())
                .unwrap()
                .modified()
                .ok()
        ),
        "S2: the fresh marks stop the next press remaking the same edit"
    );
    assert_ne!(
        std::fs::read_to_string(tree.retakes_tsv()).unwrap(),
        before,
        "the seeded marks are gone"
    );
    // Asking again changes nothing: no second remake, no second note.
    assert!(edit::before_cut(&tree, &[SRC.to_string()]).is_none());
}

#[test]
fn f1_12_s2_before_cut_refuses_and_leaves_the_marks_alone() {
    let tree = tree("disk-refuse");
    transcript(tree.dir());
    write_words(&tree, &WORDS);
    let before = seed_marks(&tree);
    // Six of ten gone is over P.machine.retakeCeil (0.4), so nothing is marked.
    write_final(&tree, &text(&["so", "we", "take", "the"]));
    set_stamp(&tree.retakes_tsv(), 60);
    set_stamp(&tree.final_txt(), 0);

    let outcome = edit::before_cut(&tree, &[SRC.to_string()]).expect("the text is newer, so it is asked");
    assert!(outcome.refused, "more than 40 % removed is refused");
    assert!(
        outcome.logs.iter().any(|line| line.contains("refused")),
        "the refusal says so: {:?}",
        outcome.logs
    );
    assert_eq!(
        std::fs::read_to_string(tree.retakes_tsv()).unwrap(),
        before,
        "a refusal writes nothing -- the marks that were there still are"
    );
}

#[test]
fn f1_12_s2_before_cut_says_the_marks_stand_when_nothing_was_edited() {
    let tree = tree("disk-stand");
    transcript(tree.dir());
    write_words(&tree, &WORDS);
    let before = seed_marks(&tree);
    write_final(&tree, &text(&WORDS));
    // final.txt older than retakes.tsv: the N --no branch, the model's marks stand.
    set_stamp(&tree.final_txt(), 120);
    set_stamp(&tree.retakes_tsv(), 0);

    assert!(
        edit::before_cut(&tree, &[SRC.to_string()]).is_none(),
        "S2: nothing newer than the marks is nothing to do"
    );
    assert_eq!(std::fs::read_to_string(tree.retakes_tsv()).unwrap(), before);
}

#[test]
fn f1_12_s2_no_model_is_asked_and_nothing_is_exchanged() {
    let tree = tree("no-model");
    transcript(tree.dir());
    // P.machine.retakeCeil = 0.4: two of ten removed, so this is an edit that gets marked rather
    // than refused -- and the marking happens without a single request leaving the machine.
    write_words(&tree, &WORDS);
    seed_marks(&tree);
    write_final(&tree, &text(&["so", "we", "take", "the", "clip", "and", "there", "twice"]));
    set_stamp(&tree.retakes_tsv(), 60);
    set_stamp(&tree.final_txt(), 0);

    // The two artefacts a model would leave behind, snapshotted BEFORE the remake: `llm/` holds the
    // exchange pages (layout::llm_dir -> exchanges::LLM_DIR) and requests.tsv logs every call made
    // outside. Neither may move.
    fn listing(path: &std::path::Path) -> Vec<String> {
        match std::fs::read_dir(path) {
            Ok(entries) => {
                let mut names: Vec<String> = entries
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .collect();
                names.sort();
                names
            }
            // Absent is its own state, kept distinct from "present but empty" so a directory created
            // by the remake would show up as a change.
            Err(_) => vec!["<absent>".to_string()],
        }
    }
    let llm_before = listing(&tree.llm_dir());
    let requests_before = listing(&tree.requests_tsv());

    let outcome = edit::before_cut(&tree, &[SRC.to_string()]).expect("the text is newer");
    assert_eq!(outcome.dropped, 2, "the edit was recognised");
    assert_eq!(
        outcome.logs[0],
        edit::EDITED_NOTE,
        "the line printed is the one that says no model was asked"
    );
    assert!(
        listing(&tree.llm_dir()) == llm_before,
        "no exchange page was written: {:?} -> {:?}",
        llm_before,
        listing(&tree.llm_dir())
    );
    assert!(
        listing(&tree.requests_tsv()) == requests_before,
        "nothing was logged as sent outside: {:?} -> {:?}",
        requests_before,
        listing(&tree.requests_tsv())
    );
    // And it did do its job locally: the marks are on disk.
    assert_eq!(textfmt::read_retakes(&tree.retakes_tsv()).unwrap().len(), 1);
}

#[test]
fn f1_12_s2_a_never_said_word_survives_the_disk_path() {
    let tree = tree("disk-typo");
    transcript(tree.dir());
    write_words(&tree, &WORDS);
    seed_marks(&tree);
    // One invented word among the survivors. Through before_cut, not remake: the NS -- yes branch has
    // only ever been proven against the pure function, never through the door the ▶ uses.
    write_final(
        &tree,
        &text(&["so", "we", "take", "the", "clip", "and", "typo", "there", "twice"]),
    );
    set_stamp(&tree.retakes_tsv(), 60);
    set_stamp(&tree.final_txt(), 0);

    let outcome = edit::before_cut(&tree, &[SRC.to_string()]).expect("the edited text is newer");
    assert_eq!(outcome.extra, 1, "one word nobody spoke");
    // P.machine.retakeCeil = 0.4: with the typo left out, one of ten went, nowhere near the ceiling.
    assert!(
        !outcome.refused,
        "an invented word is dropped, not a reason to refuse: {:?}",
        outcome.logs
    );
    assert!(
        outcome.logs.iter().any(|line| line.contains("were never said")),
        "the warning reached the log: {:?}",
        outcome.logs
    );
    assert!(
        !outcome.logs.iter().any(|line| line.contains("refused")),
        "no refusal line: {:?}",
        outcome.logs
    );
    // The remaining deletion was still marked, and the typo is in no mark's text.
    let on_disk = textfmt::read_retakes(&tree.retakes_tsv()).unwrap();
    assert_eq!(on_disk.len(), 1, "the real deletion is still marked");
    assert!(
        on_disk.iter().all(|row| !row.text.contains("typo")),
        "{on_disk:?}"
    );
    assert_eq!(outcome.dropped, 2, "'it' and 'cut' went -- the typo is extra, not a substitute");
}

// --- F1.11 through this door: does the Cut ▶ actually consult `cache/waves`? ---------------------

/// The source file the project names, written for real so `stat_recording` has something to stat.
/// Returns the path and the stored form to hand `before_cut`.
///
/// The base name carries NO timestamp on purpose: an unstamped take sits at the session's start, offset
/// 0.0, which is where this test's word times already run from. A stamp in the file name would place
/// the lane on a different second than the words are counted from, and the envelope would be asked
/// about audio that is not under the mark.
fn make_source(tree: &Tree) -> (PathBuf, String) {
    let dir = tree.dir().join("sources");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lecture.mkv");
    std::fs::write(&path, b"a recording, standing in for the real one").unwrap();
    (path, "project:sources/lecture.mkv".to_string())
}

/// The recording's own `(size, mtime)` read back the way the app reads them — whole seconds since the
/// epoch — so a cache written with these matches the file instead of matching by luck.
fn stat_like_the_app(path: &Path) -> (i64, i64) {
    let meta = std::fs::metadata(path).unwrap();
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    (meta.len() as i64, mtime)
}

/// Two words deleted from the middle of ten, so the dropped run starts at second 5.
const CUT_TWO: [&str; 8] = [
    "so", "we", "take", "the", "clip", "and", "there", "twice",
];

#[test]
fn f1_11_s6_before_cut_asks_the_wave_cache_and_the_sound_moves_the_edge() {
    let tree = tree("f111-cache-hit");
    transcript(tree.dir());
    write_words(&tree, &WORDS);
    seed_marks(&tree);
    write_final(&tree, &text(&CUT_TWO));
    set_stamp(&tree.retakes_tsv(), 60);
    set_stamp(&tree.final_txt(), 0);

    // The recording on disk, then a cache written against THAT file's own size/mtime.
    let (src, stored) = make_source(&tree);
    let (size, mtime) = stat_like_the_app(&src);
    let mut peaks = vec![8u8; (20.0 * HZ) as usize];
    // Sound up to 5.9 s, quiet after: the cut ends where the sound stops instead of at the word time.
    for bucket in (5.0 * HZ) as usize..(5.9 * HZ) as usize {
        peaks[bucket] = 120;
    }
    naivepost::wave::write(&tree, "lecture", &Wave { hz: HZ, chans: vec![peaks] }, size, mtime)
        .unwrap();

    let outcome = edit::before_cut(&tree, &[stored])
        .expect("the text is newer, so the marks are remade");
    assert_eq!(outcome.marks.len(), 1);
    // WITHOUT the cache this edge would sit on the word time 6.0; the envelope put it at 5.95. That
    // difference is the proof the live path read `cache/waves/lecture.wave`.
    assert_eq!(outcome.marks[0].s, 5.95, "the cached envelope placed the edge, not the word time");
    assert_ne!(outcome.marks[0].s, 6.0);
    assert!(
        outcome.logs.iter().any(|line| line.contains("the sound before it stops")),
        "{:?}",
        outcome.logs
    );
}

#[test]
fn f1_11_s6_before_cut_without_a_cache_still_places_on_the_word_times() {
    let tree = tree("f111-no-cache");
    transcript(tree.dir());
    write_words(&tree, &WORDS);
    seed_marks(&tree);
    write_final(&tree, &text(&CUT_TWO));
    set_stamp(&tree.retakes_tsv(), 60);
    set_stamp(&tree.final_txt(), 0);

    // The recording exists, but NOTHING was ever cached for it — the state every session is in today,
    // since no code in src/ writes the wave cache yet. The run must carry on, not fail.
    let (_src, _stored) = make_source(&tree);
    assert!(!tree.wave("lecture").exists());

    let outcome = edit::before_cut(&tree, &[_stored])
        .expect("a missing cache is not a reason to refuse the remake");
    assert_eq!(outcome.marks.len(), 1);
    // With no envelope to ask, the edge stays on the aligner's own time.
    assert_eq!(outcome.marks[0].s, 6.0, "no envelope, so the word time stands");
    assert!(
        outcome.logs.iter().all(|line| !line.contains("the sound before it stops")),
        "nothing was asked, so nothing may claim the sound moved it: {:?}",
        outcome.logs
    );
}

