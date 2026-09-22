//! F4.8 Narration off (spec/07-narrate.md) — the tick greys the page, leaves the record
//! exactly as it is, hides Produce's game-volume slider without losing its value, and empties
//! what every step reads through. Ticking it back finds everything written.

use std::path::PathBuf;

use naivepost::layout::Tree;
use naivepost::narrate_off::{
    game_volume_shown, greyed, lines_to_speak, page_returns, refuse_run, set_off, skips,
    tick_checked, touches_the_record, TICK_LABEL,
};
use naivepost::narration::{self, Entry, Narration};
use naivepost::project::Project;
use naivepost::run::Step;

fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-narroff-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A project folder with a `narrate/` record and the two files a spoken video leaves beside it.
fn tree_with(tag: &str, record: &Narration) -> (PathBuf, Tree) {
    let root = temp_root(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    // The Tree owns the paths, so `save` is what writes the record — it makes `narrate/`
    // itself, which a hand-written file inside it would otherwise have to do first.
    let tree = Tree::new(&dir).unwrap();
    narration::save(record, &tree).unwrap();
    let take = tree.tts_wav("abc");
    std::fs::create_dir_all(take.parent().unwrap()).unwrap();
    std::fs::write(&take, b"RIFFfake").unwrap();
    std::fs::write(tree.voice_ref_wav(), b"RIFFref").unwrap();
    (root, tree)
}

fn read_record_bytes(tree: &Tree) -> Vec<u8> {
    std::fs::read(tree.narration_json()).unwrap()
}

/// What `project::save` wrote, read back as text so a test can see the key itself.
fn saved_project(tree: &Tree) -> String {
    let file = tree.dir().join("naivepost.json");
    std::fs::read_to_string(file).unwrap()
}

/// Two lines over two clips: one with words, one a deliberate silence.
fn entries() -> Vec<Entry> {
    vec![
        Entry {
            s: 0.0,
            e: 10.0,
            text: "we open on the desktop".into(),
            ..Default::default()
        },
        Entry {
            s: 10.0,
            e: 18.0,
            text: String::new(),
            ..Default::default()
        },
    ]
}

fn narrated() -> Project {
    Project {
        no_narration: false,
        ..Default::default()
    }
}

fn silent() -> Project {
    Project {
        no_narration: true,
        ..Default::default()
    }
}

#[test]
fn f4_8_s1_the_tick_greys_the_page_and_never_greys_itself() {
    // F4.8 node 1: "the lines, the preview and the voice picker" go grey together.
    let off = greyed(true);
    assert!(off.lines && off.preview && off.voice);
    // The tick is how the page comes back: greying it would lock the page shut.
    assert!(!off.tick, "the Narration tick must stay pressable — it is the only way back onto this page");

    let on = greyed(false);
    assert!(
        !on.lines && !on.preview && !on.voice && !on.tick,
        "a narrated project greys nothing"
    );

    // Inventory A.2: initially on unless the project says no narration.
    assert_eq!(TICK_LABEL, "Narration");
    assert!(tick_checked(narrated().no_narration));
    assert!(!tick_checked(silent().no_narration));
}

#[test]
fn f4_8_s2_the_tick_writes_the_flag_and_leaves_the_record_alone() {
    let record = Narration {
        entries: entries(),
        silent: vec![],
    };
    let (root, tree) = tree_with("s2", &record);
    let before = read_record_bytes(&tree);

    let mut project = narrated();
    set_off(&mut project, &tree, true).unwrap();
    assert!(project.no_narration);
    assert!(saved_project(&tree).contains("\"no_narration\": true"));

    // F4.8 node 2: narration.json left exactly as it is; takes and reference untouched.
    assert_eq!(read_record_bytes(&tree), before);
    assert!(tree.tts_wav("abc").exists());
    assert!(tree.voice_ref_wav().exists());
    assert_eq!(narration::load(&tree).unwrap().entries, entries());
    assert!(!touches_the_record(), "the tick must never open the record");

    set_off(&mut project, &tree, false).unwrap();
    assert!(!saved_project(&tree).contains("\"no_narration\": true"));
    assert_eq!(read_record_bytes(&tree), before);

    let _ = std::fs::remove_dir_all(&root);
}

// P.policy.gameVolume — the slider is Produce's control over a number crate::project owns
// (§10 line 93 spells the id `P.policy.*` while the field sits under `produce`, where
// `Project::default()` gives it §10's 0.22).
#[test]
fn f4_8_s3_produce_hides_the_slider_and_renders_without_lines() {
    assert!(!game_volume_shown(true));
    assert!(game_volume_shown(false));

    // The one seam: off answers empty, so neither the speaking pass nor the render sees a line.
    assert!(lines_to_speak(true, &entries()).is_empty());
    let spoken = lines_to_speak(false, &entries());
    assert_eq!(spoken.len(), 1, "a blank line is silence, not a take");
    assert_eq!(spoken[0].text, "we open on the desktop");

    // The slider goes; the number stays where it was left.
    let (root, tree) = tree_with("s3", &Narration::default());
    let mut project = narrated();
    let kept = project.produce.game_volume;
    assert_eq!(kept, 0.22);
    set_off(&mut project, &tree, true).unwrap();
    assert!(!game_volume_shown(project.no_narration));
    assert_eq!(project.produce.game_volume, kept);
    set_off(&mut project, &tree, false).unwrap();
    assert!(game_volume_shown(project.no_narration));
    assert_eq!(project.produce.game_volume, kept);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn f4_8_s4_a_lucky_run_skips_narrate() {
    // F0.4's `narration off?` branch, in the spec's own wording (em dash U+2014).
    assert_eq!(
        skips(Step::Narrate, true).as_deref(),
        Some(">>> run: Narrate skipped \u{2014} this video has no narration")
    );

    // Skipping Narrate does not shorten the pipeline.
    assert_eq!(skips(Step::Narrate, false), None);
    assert_eq!(skips(Step::Prepare, true), None);
    assert_eq!(skips(Step::Suggest, true), None);
    assert_eq!(skips(Step::Produce, true), None);
}

#[test]
fn f4_8_s5_turning_it_back_finds_everything_written() {
    // F4.1 S1's refusal, byte for byte.
    assert_eq!(
        refuse_run(true).as_deref(),
        Some("this video has no narration \u{2014} tick Narration at the top of this page to write one")
    );
    assert_eq!(refuse_run(false), None);

    let record = Narration {
        entries: entries(),
        silent: vec![],
    };
    let (root, tree) = tree_with("s5", &record);
    let before = read_record_bytes(&tree);

    let mut project = narrated();
    set_off(&mut project, &tree, true).unwrap();
    assert!(!page_returns(project.no_narration));
    set_off(&mut project, &tree, false).unwrap();

    // "☑ again: everything written is still there."
    assert!(page_returns(project.no_narration));
    assert_eq!(read_record_bytes(&tree), before);
    assert_eq!(narration::load(&tree).unwrap().entries, entries());
    assert!(tick_checked(project.no_narration));
    let on = greyed(false);
    assert!(!on.lines && !on.preview && !on.voice);
    assert!(game_volume_shown(project.no_narration));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn f4_8_s6_the_run_refuses_before_it_writes_anything() {
    // The refusal is asked first, at the point a run has nothing to write: no text box read,
    // nothing saved. With it answered, the seam still hands out no lines.
    let refused = refuse_run(true);
    assert!(refused.is_some());
    assert!(lines_to_speak(true, &entries()).is_empty());

    // And the sentence names the way back rather than leaving a dead page.
    let said = refused.unwrap();
    assert!(
        said.contains("tick Narration"),
        "the refusal must say how to turn narration back on: {said}"
    );
    assert_eq!(refuse_run(false), None);
    // Nothing about the record changed by merely asking.
    assert!(!touches_the_record());
}
