//! §03-shell.md F0.1 Switch tab — the flow that decides which page is showing, whether a tab may
//! be entered, and what has to be redone on arrival.
//!
//! One test per numbered step (S1-S5) plus the two branches §1 adds around them: the bounce is only
//! ever for a click, and a page whose prerequisites vanish goes back in silence. Everything runs on
//! [`naivepost::shell`], so no window is built and nothing is clicked — the UI only renders what
//! these functions return.

use std::path::PathBuf;
use std::time::Duration;

use naivepost::cut::{self, Seg};
use naivepost::layout::Tree;
use naivepost::narration::{self, Entry};
use naivepost::project::{Project, Publish, Source};
use naivepost::publish;
use naivepost::shell::{self, Move, Outcome, Page, Pending, Refresh, Shell, NARRATION_AUTOSAVE};

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "naivepost-switch-tab-{}-{tag}",
        std::process::id()
    ))
}

/// A project folder plus the tree inside it, created so the two refreshes that read disk have
/// something to read. Same fixture shape as tests/layout.rs.
fn fixture(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).unwrap();
    tree.create_dirs().unwrap();
    (root, tree)
}

/// A project with one footage source — the state every page is unlocked in.
fn with_footage() -> Project {
    Project {
        sources: vec![Source { path: "cam.mp4".to_string(), footage: true, ..Default::default() }],
        ..Default::default()
    }
}

/// A project with a source that is not footage: Cut is locked on it, and so is an empty project.
fn without_footage() -> Project {
    Project {
        sources: vec![Source { path: "voice.wav".to_string(), ..Default::default() }],
        ..Default::default()
    }
}

/// The switch with nothing else in hand: no narration lines, no cut clips.
fn go(shell: &mut Shell, to: Page, why: Move, project: &Project, tree: Option<&Tree>) -> Outcome {
    shell.switch(to, why, project, tree, &[], &mut Vec::new())
}

#[test]
fn f0_1_s1_a_click_moves_to_the_page() {
    // S1: a tab click moves; so does a lucky run moving to the page it is about to run.
    let project = with_footage();
    for page in Page::all() {
        let mut shell = Shell::default();
        assert_eq!(shell.page, Page::Prepare, "the window opens on Prepare");
        let outcome = go(&mut shell, page, Move::Click, &project, None);
        // Which refresh ran is S5's business (and s5/s6/s7 pin each one); here it only has to be a
        // page that got shown.
        assert!(
            matches!(outcome, Outcome::Shown { page: shown, .. } if shown == page),
            "a click on {} must show it: {outcome:?}",
            page.label()
        );
        assert_eq!(shell.page, page);

        let mut shell = Shell::default();
        go(&mut shell, Page::Prepare, Move::Click, &project, None);
        let run = shell.switch(page, Move::Run, &project, None, &[], &mut Vec::new());
        assert!(matches!(run, Outcome::Shown { .. }), "a run must reach {}", page.label());
        assert_eq!(shell.page, page);
    }
    // The four tabs are the pipeline's own order and the same words as crate::PAGES.
    let labels: Vec<&str> = Page::all().map(|page| page.label()).to_vec();
    assert_eq!(labels, naivepost::PAGES.to_vec());
    let indexes: Vec<usize> = Page::all().map(|page| page.index()).to_vec();
    assert_eq!(indexes, vec![0, 1, 2, 3]);
}

#[test]
fn f0_1_s2_a_locked_tab_bounces_and_says_why() {
    // The spec's own sentence, em dash and all (§1: Cut locked until a source is marked footage).
    assert_eq!(
        shell::CUT_LOCK,
        "Add footage on the Prepare step first \u{2014} the cut is laid out from the recordings"
    );

    let empty = Project::default();
    assert_eq!(shell::lock(Page::Cut, &empty), Some(shell::CUT_LOCK));
    // §1: "Narrate and Produce never locked; their ▶ refuses without a cut." Prepare is where the
    // sources get added, so an empty project still has to open it.
    for page in [Page::Prepare, Page::Narrate, Page::Produce] {
        assert_eq!(shell::lock(page, &empty), None, "{} is never locked", page.label());
    }

    let mut shell = Shell::default();
    let outcome = go(&mut shell, Page::Cut, Move::Click, &empty, None);
    assert_eq!(
        outcome,
        Outcome::Bounced { reason: shell::CUT_LOCK },
        "a click on a locked tab bounces"
    );
    assert_eq!(shell.page, Page::Prepare, "the page did not move");
    assert_eq!(shell.status, shell::CUT_LOCK, "the status line carries the reason");
    // ⓘ still describes the page that is showing, not the one that was refused.
    assert_eq!(shell.help_page, Page::Prepare);

    // Mark a source footage and the same click goes through.
    let project = with_footage();
    assert_eq!(shell::lock(Page::Cut, &project), None);
    let outcome = go(&mut shell, Page::Cut, Move::Click, &project, None);
    assert!(matches!(outcome, Outcome::Shown { page: Page::Cut, .. }));
    assert_eq!(shell.page, Page::Cut);
    // A bounce leaves no debt behind: the status is overwritten by whatever happens next.
    let outcome = go(&mut shell, Page::Prepare, Move::Click, &project, None);
    assert!(matches!(outcome, Outcome::Shown { page: Page::Prepare, .. }));
}

#[test]
fn f0_1_s3_a_switch_flushes_the_narration_autosave() {
    // §7-narrate.md:215 — autosaved 400 ms after typing, flushed on tab leave.
    assert_eq!(NARRATION_AUTOSAVE, Duration::from_millis(400));

    let mut pending = Pending::default();
    assert!(!pending.owe(), "nothing typed, nothing owed");
    assert!(!pending.writable(NARRATION_AUTOSAVE * 2));
    pending.touched(Duration::ZERO);
    assert_eq!(pending.due_at(), Some(NARRATION_AUTOSAVE));
    assert!(!pending.writable(Duration::from_millis(399)));
    assert!(pending.writable(Duration::from_millis(400)));

    // A second keystroke pushes the deadline back: one write per pause, not per letter.
    pending.touched(Duration::from_millis(200));
    assert_eq!(pending.due_at(), Some(Duration::from_millis(600)));
    assert!(!pending.writable(Duration::from_millis(400)));
    assert!(pending.writable(Duration::from_millis(600)));
    // Leaving the tab writes it even a beat early, and only once.
    assert!(pending.flush());
    assert!(!pending.owe());
    assert!(!pending.flush(), "a flush after a flush owes nothing");

    // S3 inside the switch: flushed is true exactly when typing owed a write.
    let project = with_footage();
    let mut shell = Shell::default();
    shell.narration_pending.touched(Duration::ZERO);
    let outcome = go(&mut shell, Page::Cut, Move::Click, &project, None);
    assert!(matches!(outcome, Outcome::Shown { flushed: true, .. }), "{outcome:?}");

    // Nothing typed since: the next switch reports no flush.
    let outcome = go(&mut shell, Page::Narrate, Move::Click, &project, None);
    assert!(matches!(outcome, Outcome::Shown { flushed: false, .. }), "{outcome:?}");

    // And a bounce is S2 stopping the flow before S3 — nothing was written because nothing moved.
    let empty = Project::default();
    let mut shell = Shell::default();
    shell.narration_pending.touched(Duration::ZERO);
    assert!(matches!(
        go(&mut shell, Page::Cut, Move::Click, &empty, None),
        Outcome::Bounced { .. }
    ));
    assert!(
        shell.narration_pending.owe(),
        "a refused switch must leave the write owed"
    );
}

#[test]
fn f0_1_s4_the_page_its_readouts_and_the_info_button_move_together() {
    let (_root, tree) = fixture("readouts");
    let project = with_footage();
    let cut = cut::Cut {
        segs: vec![Seg { s: 0.0, e: 30.0, ..Default::default() }],
        ..Default::default()
    };
    let narration = narration::Narration {
        entries: vec![Entry { s: 0.0, e: 30.0, at: 1.0, text: "one".to_string(), ..Default::default() }],
        ..Default::default()
    };

    let mut shell = Shell::default();
    let mut seen_inputs = Vec::new();
    for page in Page::all() {
        go(&mut shell, page, Move::Click, &project, Some(&tree));
        // S4: the page, its readouts and ⓘ are one change.
        assert_eq!(shell.page, page);
        assert_eq!(shell.help_page, page, "ⓘ syncs with the page shown");
        let inputs = shell.inputs(Some(&tree), &project, &cut, &narration);
        assert!(!inputs.is_empty(), "{} shows no Inputs", page.label());
        assert_ne!(inputs, "0", "{}'s Inputs row says something", page.label());
        seen_inputs.push(inputs);
        // §1: Outputs is a count of what the page wrote. A fresh project has written nothing, and
        // an empty folder is 0 file(s) — never blank, which would read as a broken row.
        let outputs = shell.outputs(Some(&tree));
        assert!(outputs.ends_with("file(s)"), "{}: {outputs}", page.label());
        assert_eq!(shell.info_tip(), shell::info_tip(page));
        assert!(
            shell.info_tip().starts_with(page.label()),
            "ⓘ leads with the tab's word: {}",
            shell.info_tip()
        );
    }
    // The rows belong to their page: Prepare counts frames and lines (spec/04-prepare.md §1),
    // Cut clips, Narrate lines. Nothing has run here, so Prepare's two pairs are zero.
    assert_eq!(
        seen_inputs[0],
        "0 frames \u{2192} 0 vision \u{b7} 0 lines \u{2192} 0 fixer"
    );
    assert_eq!(seen_inputs[1], "1 clip(s)");
    assert_eq!(seen_inputs[2], "1 line(s) over 1 clip(s)");
    // Produce's row is spec/08-produce.md §1: "N clip(s) · mm:ss[ · no narration][ · no upload text]".
    let produce = &seen_inputs[3];
    assert!(produce.starts_with("1 clip(s) · "), "{produce}");
    assert!(produce.contains("00:30"), "mm:ss of the cut's length: {produce}");
    assert!(produce.ends_with("· no upload text"), "no publish.json yet: {produce}");
    assert!(!produce.contains("no narration"), "there is a line to speak: {produce}");

    // Narration off is what says so, and it is the row that changes.
    let captions = Project { no_narration: true, ..with_footage() };
    go(&mut shell, Page::Narrate, Move::Click, &captions, Some(&tree));
    assert_eq!(shell.inputs(Some(&tree), &captions, &cut, &narration), "no narration — captions only");
    go(&mut shell, Page::Produce, Move::Click, &captions, Some(&tree));
    assert!(
        shell.inputs(Some(&tree), &captions, &cut, &narration).contains("no narration"),
        "the tail appears with nothing to speak"
    );

    // With no project folder yet there is nothing to count, and the row says nothing rather than
    // lying about a run that has not happened.
    go(&mut shell, Page::Prepare, Move::Click, &project, None);
    assert_eq!(shell.outputs(None), "");
}

#[test]
fn f0_1_s5_cut_rebuilds_only_when_prepare_moved() {
    let (_root, tree) = fixture("rebuild");
    let project = with_footage();
    // The fixture has to be the state S5 reasons about: one footage source whose transcript exists.
    assert!(project.sources.iter().any(|source| source.footage));
    std::fs::create_dir_all(tree.input_dir("cam.mp4")).unwrap();
    let transcript = tree.transcript_txt("cam.mp4");
    std::fs::write(&transcript, "first").unwrap();

    let stamp = shell::prepare_stamp(&tree, &project);
    assert!(
        stamp.sources.iter().any(|(name, _)| name.ends_with("cam.mp4/transcript.txt")),
        "the stamp names Prepare's own output: {:?}",
        stamp.sources
    );
    assert_eq!(stamp, shell::prepare_stamp(&tree, &project), "an unchanged project stamps the same");

    let mut build = shell::CutBuild::default();
    // Never built, so it has to be.
    assert!(build.needs_rebuild(&stamp));
    build.record(stamp.clone());
    assert!(!build.needs_rebuild(&stamp), "nothing moved since the build");

    // One file of Prepare's output changed — a re-transcribe — and the build is stale.
    std::fs::write(&transcript, "second, longer").unwrap();
    let moved = shell::prepare_stamp(&tree, &project);
    assert_ne!(moved, stamp, "the stamp notices the new transcript");
    assert!(build.needs_rebuild(&moved));

    // Through the switch: stale on arrival the first time, recorded so the next visit is not.
    let mut shell = Shell::default();
    let outcome = go(&mut shell, Page::Cut, Move::Click, &project, Some(&tree));
    assert_eq!(
        outcome,
        Outcome::Shown { page: Page::Cut, flushed: false, refresh: Refresh::CutRebuild { stale: true } }
    );
    let outcome = go(&mut shell, Page::Prepare, Move::Click, &project, Some(&tree));
    assert_eq!(outcome, Outcome::Shown { page: Page::Prepare, flushed: false, refresh: Refresh::None });
    let outcome = go(&mut shell, Page::Cut, Move::Click, &project, Some(&tree));
    assert_eq!(
        outcome,
        Outcome::Shown { page: Page::Cut, flushed: false, refresh: Refresh::CutRebuild { stale: false } },
        "nothing moved between the two visits"
    );

    // A project with no folder cannot be stamped, and so claims nothing was rebuilt.
    let mut shell = Shell::default();
    assert_eq!(
        go(&mut shell, Page::Cut, Move::Click, &project, None),
        Outcome::Shown { page: Page::Cut, flushed: false, refresh: Refresh::CutRebuild { stale: false } }
    );
}

#[test]
fn f0_1_s6_narrate_refits_its_lines_to_the_cut() {
    // S5's Narrate branch: the words are kept and the times follow, with no model asked.
    let segs = [Seg { s: 10.0, e: 20.0, ..Default::default() }];
    // A line on the clip where it was written; the clip has moved to 10..20 and the words have to
    // stay over the same moment of the recording — `at` is re-based (gui/narrate.go:2432).
    let mut entries = vec![Entry { s: 5.0, e: 15.0, at: 2.0, ..Default::default() }];
    assert_eq!(shell::refit(&segs, &mut entries), (1, 0));
    assert_eq!((entries[0].s, entries[0].e), (10.0, 20.0));
    // 5 + 2 - 10 = -3 clamps to the clip's start: the line was written before the new clip began.
    assert_eq!(entries[0].at, 0.0, "{:?}", entries[0]);

    // A line inside a clip that only moved by hundredths is not "the cut moved".
    let mut settled = vec![Entry { s: 10.04, e: 20.0, at: 1.0, ..Default::default() }];
    assert_eq!(shell::refit(&segs, &mut settled), (0, 0));
    assert_eq!(settled[0].at, 1.0, "an unmoved line keeps its placement");

    // A line with no clip over it at all is an orphan and is left exactly where it was.
    let mut orphan = vec![Entry { s: 100.0, e: 110.0, at: 3.0, ..Default::default() }];
    assert_eq!(shell::refit(&segs, &mut orphan), (0, 1));
    assert_eq!((orphan[0].s, orphan[0].e, orphan[0].at), (100.0, 110.0, 3.0));

    // A line on a card keeps the zero-span segment sitting at the same moment.
    let cards = [Seg { s: 40.0, e: 40.0, ..Default::default() }];
    let mut card = vec![Entry { s: 40.0, e: 40.0, at: 0.0, ..Default::default() }];
    assert_eq!(shell::refit(&cards, &mut card), (0, 0));
    let mut stranded = vec![Entry { s: 60.0, e: 60.0, at: 0.0, ..Default::default() }];
    assert_eq!(shell::refit(&cards, &mut stranded), (0, 1));

    // Nothing written yet refits to nothing, and says nothing.
    let mut empty: Vec<Entry> = Vec::new();
    assert_eq!(shell::refit(&segs, &mut empty), (0, 0));

    // The status line, in the app's own words.
    assert_eq!(
        shell::refit_sentence(1, 0),
        "the cut moved \u{2014} 1 line(s) followed their clips"
    );
    assert_eq!(
        shell::refit_sentence(2, 3),
        "the cut moved \u{2014} 2 line(s) followed their clips, 3 sit on video the cut no longer has"
    );

    // Through the switch: arriving on Narrate refits against the caller's clips and reports it.
    let (_root, tree) = fixture("refit");
    let project = with_footage();
    let mut shell = Shell::default();
    let mut entries = vec![Entry { s: 5.0, e: 15.0, at: 2.0, ..Default::default() }];
    let outcome = shell.switch(Page::Narrate, Move::Click, &project, Some(&tree), &segs, &mut entries);
    assert_eq!(
        outcome,
        Outcome::Shown { page: Page::Narrate, flushed: false, refresh: Refresh::NarrateRefit { moved: 1, orphan: 0 } }
    );
    assert_eq!(shell.status, "the cut moved \u{2014} 1 line(s) followed their clips");

    // A narration with no lines refits to zero and leaves the status alone.
    let mut shell = Shell::default();
    shell.status = "unchanged".to_string();
    let outcome = shell.switch(Page::Narrate, Move::Click, &project, Some(&tree), &segs, &mut Vec::new());
    assert_eq!(outcome, Outcome::Shown { page: Page::Narrate, flushed: false, refresh: Refresh::NarrateRefit { moved: 0, orphan: 0 } });
    assert_eq!(shell.status, "unchanged", "nothing moved, so nothing is claimed");
}

#[test]
fn f0_1_s7_produce_refreshes_readouts_and_the_publish_panel() {
    let (_root, tree) = fixture("publish-panel");
    let project = with_footage();

    // No publish/publish.json: the text is not written and there is no base frame (§5 of
    // 08-produce.md — the file's existence IS "the text is written").
    assert!(!publish::is_written(&tree));
    let panel = shell::PublishPanel::refresh(&tree).expect("no file is a state, not an error");
    assert_eq!(panel.text_written, false);
    assert_eq!(panel.base, None);
    assert_eq!(panel.references, 0);

    // Write it and refresh again: the panel reads the record back, base first, references after.
    let record = Publish {
        frames: vec!["frame-1.png".to_string(), "frame-2.png".to_string(), "frame-3.png".to_string()],
        ..Publish::default()
    };
    publish::save(&record, &tree).expect("a publish file");
    let panel = shell::PublishPanel::refresh(&tree).expect("the file this wrote parses");
    assert!(panel.text_written);
    assert_eq!(panel.base.as_deref(), Some("frame-1.png"));
    assert_eq!(panel.references, 2);

    // S5's Produce branch: readouts and the publish panel, whatever their contents.
    let mut shell = Shell::default();
    let outcome = go(&mut shell, Page::Produce, Move::Click, &project, Some(&tree));
    assert_eq!(
        outcome,
        Outcome::Shown { page: Page::Produce, flushed: false, refresh: Refresh::ProducePanels }
    );
    // With the text written, the Inputs row stops saying otherwise.
    let project = Project { publish: Some(record), ..project };
    assert!(
        !shell.inputs(Some(&tree), &project, &cut::Cut::default(), &narration::Narration::default())
            .contains("no upload text")
    );

    // A publish file that does not parse is reported and still opens the page.
    std::fs::write(tree.publish_json(), "{ not json").unwrap();
    let mut shell = Shell::default();
    assert!(shell::PublishPanel::refresh(&tree).is_err());
    let outcome = go(&mut shell, Page::Produce, Move::Click, &project, Some(&tree));
    assert!(matches!(outcome, Outcome::Shown { refresh: Refresh::ProducePanels, .. }));
    assert!(!shell.status.is_empty(), "the reason reaches the status line: {}", shell.status);
}

#[test]
fn f0_1_s8_a_page_that_loses_its_prerequisites_goes_back_silently() {
    // spec/03-shell.md §1, the bullet before the closing one: "A page whose prerequisites vanish
    // while open switches silently to Prepare (F0.1 S2's bounce is only for a click)."
    let project = with_footage();
    let mut shell = Shell::default();
    go(&mut shell, Page::Cut, Move::Click, &project, None);
    assert_eq!(shell.page, Page::Cut);

    // The footage mark goes away while Cut is open.
    let lost = without_footage();
    assert_eq!(shell::lock(Page::Cut, &lost), Some(shell::CUT_LOCK));
    shell.status = "something said earlier".to_string();
    let outcome = go(&mut shell, Page::Cut, Move::Vanished, &lost, None);
    assert_eq!(
        outcome,
        Outcome::Shown { page: Page::Prepare, flushed: false, refresh: Refresh::None },
        "vanished prerequisites send the page back to Prepare"
    );
    assert_eq!(shell.page, Page::Prepare);
    assert_eq!(shell.status, "", "silently: no bounce, no lock reason");
    assert_eq!(shell.help_page, Page::Prepare, "ⓘ went with it");

    // The contrast the spec asks for: the same state reached by a click DOES bounce.
    let mut clicked = Shell::default();
    go(&mut clicked, Page::Cut, Move::Click, &lost, None);
    assert_eq!(clicked.page, Page::Prepare, "and did not move");
    assert_eq!(clicked.status, shell::CUT_LOCK);

    // A run is not a click either: it moves where the pipeline says, and its own ▶ says why it
    // cannot proceed.
    let mut running = Shell::default();
    let outcome = go(&mut running, Page::Cut, Move::Run, &lost, None);
    assert!(matches!(outcome, Outcome::Shown { page: Page::Cut, .. }), "{outcome:?}");
    assert_eq!(running.page, Page::Cut);
    assert_eq!(running.status, "", "a run that merely moved has nothing to report");

    // Vanished while unlocked is no move at all — the page keeps its place.
    let mut staying = Shell::default();
    go(&mut staying, Page::Narrate, Move::Click, &project, None);
    assert!(matches!(
        go(&mut staying, Page::Narrate, Move::Vanished, &project, None),
        Outcome::Shown { page: Page::Narrate, .. }
    ));
    assert_eq!(staying.page, Page::Narrate);
}
