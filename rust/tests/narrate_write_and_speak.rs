//! F4.1 ▶ Write and speak (`spec/07-narrate.md` F4.1) — the run's decisions, tested without a window,
//! a model or an audio server. One test per spec step, each pinning the SENTENCES byte-for-byte (em dash
//! U+2014 where §F4.1 has it) and the ORDER of the checks, since both are what the user reads.

use std::path::PathBuf;

use naivepost::audit_gaps::silent_survives_entry_rewrite;
use naivepost::cut::NO_CUT_YET;
use naivepost::layout::Tree;
use naivepost::narrate_data::keep_previous;
use naivepost::narrate_off::refuse_run;
use naivepost::narrate_rewrite::{log_line, why};
use naivepost::narrate_run::{
    captions_only_done, failed_status, fold_written, plan_run, refuse, speak_line, speak_pass,
    spoken_log, stopped_status, tally, voice_is_captions_only, written_log, Outcome, Plan, Speak,
    Written, DONE_CAPTIONS, DONE_SPOKEN, JOBS, JOB_NARRATION, JOB_SPEAKING, NO_SESSION_TIMELINE,
    SPEAKING_STATUS, STAGE_THINKING,
};
use naivepost::narrate_screen::CAPTIONS;
use naivepost::narration::{Entry, Narration, Silent};
use naivepost::runqueue::{PROGRESS_PULSE, Queue};

/// A throwaway `.naivepost` folder for the file-level assertions.
fn tmp_tree(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("np-f41-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let dir = root.with_extension("naivepost");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).expect("a folder ending in .naivepost is a project");
    (dir, tree)
}

/// Three clips, one of them already narrated, so the toggle's effect is visible in the counts.
const CLIPS: [(f64, f64); 3] = [(0.0, 10.0), (10.0, 24.0), (24.0, 31.0)];

fn one_line() -> Narration {
    Narration {
        entries: vec![Entry {
            s: 10.0,
            e: 24.0,
            text: "the middle clip already has a line".to_string(),
            ..Default::default()
        }],
        silent: Vec::new(),
    }
}

// --- S1: the four refusals, and the order they are asked in -------------------------------------

#[test]
fn f4_1_s1_refusal_order() {
    // Busy beats everything: a second run would overwrite the first one's progress line.
    assert_eq!(
        refuse(true, false, true, false).as_deref(),
        Some(naivepost::run::PAUSING),
        "busy must be checked before no-cut, narration-off and the missing timeline"
    );
    // Not busy: no cut comes next.
    assert_eq!(
        refuse(false, false, true, false).as_deref(),
        Some(NO_CUT_YET),
        "byte-for-byte the Cut page's own sentence"
    );
    // A cut stands: narration off next, ahead of the timeline check, because a video with no
    // narration needs no transcript either.
    assert_eq!(
        refuse(false, true, true, false).as_deref(),
        refuse_run(true).as_deref(),
        "the same sentence the tick's refusal uses, not a paraphrase of it"
    );
    assert_eq!(
        refuse(false, true, true, false).as_deref(),
        Some("this video has no narration \u{2014} tick Narration at the top of this page to write one"),
        "U+2014 em dash, exactly as §F4.1 S1 spells it"
    );
    // Narration on, timeline missing: the last gate.
    assert_eq!(
        refuse(false, true, false, false).as_deref(),
        Some(NO_SESSION_TIMELINE)
    );
    // Every gate open: nothing to say, and the run proceeds.
    assert_eq!(refuse(false, true, false, true), None);
}

#[test]
fn f4_1_s1_no_session_timeline() {
    assert_eq!(
        NO_SESSION_TIMELINE,
        "run Transcript first \u{2014} no session timeline",
        "§F4.1 S1 / spec/inventory/narrate.md: the missing file is session.tsv, whose accessor is Tree::session_tsv"
    );
    // The em dash really is U+2014 and not a hyphen: count the bytes rather than trust the font.
    let dashes = NO_SESSION_TIMELINE
        .chars()
        .filter(|c| *c == '\u{2014}')
        .count();
    assert_eq!(dashes, 1);
    assert!(!NO_SESSION_TIMELINE.contains("--"));
}

// --- S2: the previous generation is kept before anything is overwritten ------------------------

#[test]
fn f4_1_s2_previous_file_kept() {
    let (_dir, tree) = tmp_tree("prev");

    // No record yet: nothing to keep, and that is not a problem.
    assert_eq!(
        keep_previous(&tree),
        Ok(false),
        "no narration.json means nothing to copy forward"
    );
    assert!(!tree.narration_prev_json().exists());

    // Write a record, note its bytes, then run the copy a real ▶ makes before overwriting.
    let record = one_line();
    naivepost::narration::save(&record, &tree).unwrap();
    let old_bytes = std::fs::read(tree.narration_json()).unwrap();
    assert_eq!(keep_previous(&tree), Ok(true));

    let prev_bytes = std::fs::read(tree.narration_prev_json()).unwrap();
    assert_eq!(
        prev_bytes, old_bytes,
        "narration.prev.json holds the OLD bytes byte-for-byte"
    );
    assert_eq!(
        std::fs::read(tree.narration_json()).unwrap(),
        old_bytes,
        "keep_previous never touches the current record"
    );
    // And the copy parses back into the same narration it was copied from.
    let back = naivepost::narrate_data::previous(&tree).expect("the previous generation parses");
    assert_eq!(back, record);
}

// --- S3: the two jobs, and how the bar reads while each runs ---------------------------------

#[test]
fn f4_1_s3_bar_states() {
    // Job names and halves: `narration 1/2`, then `speaking 2/2`.
    assert_eq!(JOB_NARRATION, "narration");
    assert_eq!(JOB_SPEAKING, "speaking");
    assert_eq!(JOBS, 2, "one ▶ fills both halves of the queue");

    let mut queue = Queue::new();
    queue.phase(0.0, 1.0);
    queue.job(0, JOB_NARRATION, 1, JOBS);
    assert_eq!(queue.track(0).line(), "narration 1/2");
    queue.job(1, JOB_SPEAKING, 2, JOBS);
    assert_eq!(queue.track(1).line(), "speaking 2/2");

    // The bar opens on the wait being named, so a slow first token does not read as hung.
    assert_eq!(STAGE_THINKING, "thinking about it");
    // ...and it pulses at the queue's own interval until the first clip closes.
    assert_eq!(
        PROGRESS_PULSE,
        std::time::Duration::from_millis(150),
        "§F4.1 S3: 150 ms; the run reuses runqueue::PROGRESS_PULSE rather than its own timer"
    );
    // After the first clip closes the label carries the count.
    assert_eq!(naivepost::narrate_run::writing(2, 5), "writing 2/5 clips");
    assert_eq!(naivepost::narrate_run::writing(0, 3), "writing 0/3 clips");
}

// --- S3: the log line, for every <why> the spec lists ---------------------------------------

#[test]
fn f4_1_s3_log_line() {
    // Each reason spelled by narrate_rewrite::why, framed by log_line, matches §F4.1 S3 exactly.
    let cases: [(&str, usize); 4] = [
        ("there is no narration yet", 4),
        (
            "clip 2 has no narration \u{2014} it is new, or the cut moved under it",
            1,
        ),
        ("the narration has lines for clips the cut no longer has", 2),
        ("rewriting every line", 3),
    ];
    for (reason, clips) in cases {
        assert_eq!(
            log_line(reason, clips),
            format!(
                ">>> narrate: {reason} \u{2014} writing {clips} clip(s), one LLM call, then speaking them"
            )
        );
    }

    // And the reasons themselves come out of `why` in its precedence order, not just the frame.
    assert_eq!(why(false, &[0], 3, 0, 0), "there is no narration yet");
    assert_eq!(
        why(false, &[1], 3, 1, 0),
        "clip 2 has no narration \u{2014} it is new, or the cut moved under it"
    );
    assert_eq!(
        why(false, &[0], 3, 2, 1),
        "the narration has lines for clips the cut no longer has"
    );
    assert_eq!(why(true, &[0], 3, 2, 1), "rewriting every line");
}

// --- S4: which clips the one call covers ----------------------------------------------------

#[test]
fn f4_1_s4_rewrite_toggle_off_writes_only_missing() {
    // P.policy.narrationRewrite off (the default): only the clips with no line.
    let narration = one_line();
    let plan = plan_run(false, &CLIPS, &narration, "own");
    assert_eq!(
        plan.clips,
        vec![(0.0, 10.0), (24.0, 31.0)],
        "the narrated middle clip is left exactly as it is, and play order is kept"
    );
    assert_eq!(plan.why, "clip 1 has no narration \u{2014} it is new, or the cut moved under it");
    assert!(plan.log.contains("writing 2 clip(s)"));
    assert!(!plan.captions_only);
    // The default the project ships agrees with the rule's default.
    assert_eq!(
        naivepost::narrate_rewrite::REWRITE_DEFAULT,
        naivepost::project::Policy::default().narration_rewrite.value,
        "the toggle's default must be one number in two places at most"
    );
}

#[test]
fn f4_1_s4_rewrite_on_writes_every_clip() {
    let narration = one_line();
    let plan = plan_run(true, &CLIPS, &narration, "own");
    assert_eq!(
        plan.clips,
        CLIPS.to_vec(),
        "on: every clip, in the order the caller passed them"
    );
    assert_eq!(plan.why, "rewriting every line", "the toggle wins over staleness in <why>");
    assert!(plan.log.contains("writing 3 clip(s)"));
}

#[test]
fn f4_1_s4_silent_list_survives() {
    let mut narration = one_line();
    narration.silent.push(Silent { s: 24.0, e: 31.0 });
    let before = narration.clone();

    let _plan = plan_run(true, &CLIPS, &narration, "own");
    // plan_run takes the record by reference and writes nothing: silence survives by construction.
    assert_eq!(narration, before, "plan_run must not mutate the record");
    assert!(narration.is_silent(24.0, 31.0));

    // And the rewrite path itself keeps the markers: entries change, the silent list does not.
    let new_entries = vec![Entry {
        s: 0.0,
        e: 10.0,
        text: "a freshly written line".to_string(),
        ..Default::default()
    }];
    let kept = silent_survives_entry_rewrite(&narration.silent, &narration.entries, &new_entries);
    assert_eq!(kept, narration.silent, "the user's own silence decisions are not the model's to drop");
}

#[test]
fn f4_1_s4_written_log() {
    assert_eq!(written_log(4), ">>> narration written for 4 clips");
    assert_eq!(
        SPEAKING_STATUS,
        "narration written \u{2014} speaking it",
        "the status between the two jobs, em dash included"
    );
}

// --- S5: the captions-only voice stops after the write --------------------------------------

#[test]
fn f4_1_s5_captions_only_stops_before_speaking() {
    assert_eq!(
        captions_only_done(3),
        "narrate: captions only \u{2014} 3 line(s) written, none spoken"
    );
    // The branch is driven by the picker's row id, not by its wording.
    assert!(voice_is_captions_only(CAPTIONS));
    assert!(!voice_is_captions_only("own"));
    assert!(!voice_is_captions_only("No audio"));

    let narration = one_line();
    let plan = plan_run(false, &CLIPS, &narration, CAPTIONS);
    assert!(plan.captions_only);

    // With that plan, the run ends at the write: synthesize is never called.
    let mut synth_calls = 0usize;
    let outcome = plan_with(&plan, |_clips| Ok(2), |_s, _e, _t| {
        synth_calls += 1;
        Ok(true)
    });
    assert_eq!(outcome, Ok(Outcome::CaptionsOnly { written: 2 }));
    assert_eq!(synth_calls, 0, "captions only means NOTHING is spoken");
    assert_eq!(captions_only_done(2), "narrate: captions only \u{2014} 2 line(s) written, none spoken");
}

// --- S6: the per-line checkpoint -----------------------------------------------------------

#[test]
fn f4_1_s6_speak_skips_blank_and_cached() {
    // Blank first: synthesizing "" would cache a mistake under a key that suppresses the line forever.
    assert_eq!(speak_line("", "own", false), Speak::SkippedBlank);
    assert_eq!(speak_line("   \n\t ", "own", false), Speak::SkippedBlank);
    assert_eq!(speak_line("", "own", true), Speak::SkippedBlank);
    // Not blank but already in the cache: spend nothing.
    assert_eq!(speak_line("said it already", "own", true), Speak::SkippedCached);
    // Otherwise hand it to F4.4.
    assert_eq!(speak_line("a new line", "own", false), Speak::Synthesize);

    // The summary's indent is part of the spec: FOUR spaces, never `>>> `.
    assert!(spoken_log(2, 3).starts_with("    narrate:"));
    assert!(!spoken_log(2, 3).contains(">>>"));
    assert_eq!(spoken_log(2, 3), "    narrate: 2 line(s) spoken, 3 already in the cache");

    // Blanks count as neither spoken nor cached.
    assert_eq!(
        tally(&[
            Speak::Synthesize,
            Speak::SkippedCached,
            Speak::SkippedBlank,
            Speak::Synthesize,
            Speak::SkippedCached,
            Speak::SkippedCached,
        ]),
        (2, 3)
    );
}

// --- S7: the final statuses live on the status line -----------------------------------------

#[test]
fn f4_1_s7_done_statuses() {
    assert_eq!(
        DONE_SPOKEN,
        "narration ready and spoken \u{2014} \u{25b6} the preview hears it in place"
    );
    assert_eq!(DONE_CAPTIONS, "narration ready \u{2014} captions only, nothing spoken");
    assert_eq!(stopped_status("speaking"), "speaking stopped");
    assert_eq!(failed_status("narration"), "narration failed \u{2014} see log");

    // They are RETURNED strings, so the door can put them where they are read: the prototype wrote
    // them to the progress bar's hidden text, where nobody saw them (§F4.1 S7's bug note).
    assert_eq!(
        Outcome::Spoken {
            written: 3,
            spoken: 2
        }
        .status(),
        DONE_SPOKEN
    );
    assert_eq!(Outcome::CaptionsOnly { written: 3 }.status(), DONE_CAPTIONS);
    // A call that answered with no line at all is not reported as a finished narration.
    let nothing = Outcome::NothingWritten.status();
    assert!(nothing.contains("no usable answer"));
    assert_ne!(nothing, DONE_SPOKEN);
}

#[test]
fn f4_1_s2_fold_replaces_only_covered_clips() {
    // A record with lines on clips 1 and 2; this run covered clip 1 only.
    let record = Narration {
        entries: vec![
            Entry {
                s: 0.0,
                e: 10.0,
                text: "the old first line".to_string(),
                ..Default::default()
            },
            Entry {
                s: 10.0,
                e: 24.0,
                at: 2.5,
                text: "untouched second line".to_string(),
                emotion: "calm".to_string(),
                ..Default::default()
            },
        ],
        silent: Vec::new(),
    };
    let before_second = record.entries[1].clone();

    let folded = fold_written(
        &record,
        &[Written {
            start: 0.0,
            end: 10.0,
            at: 1.25,
            text: "the new first line".to_string(),
            emotion: "happy".to_string(),
        }],
    );

    assert_eq!(folded.entries.len(), 2, "a replace is not an append");
    assert_eq!(folded.entries[0].text, "the new first line");
    assert_eq!(folded.entries[0].at, 1.25, "the clip-relative offset passes through");
    assert_eq!(folded.entries[0].emotion, "happy");
    assert_eq!(
        folded.entries[1], before_second,
        "the entry on the clip this run did not cover is identical"
    );
    // The input record was not touched -- that is what lets S2 keep the previous file honestly.
    assert_eq!(record.entries[0].text, "the old first line");
}

#[test]
fn f4_1_s4_fold_keeps_silent() {
    let mut record = one_line();
    record.silent.push(Silent { s: 24.0, e: 31.0 });

    let folded = fold_written(
        &record,
        &[
            Written {
                start: 10.0,
                end: 24.0,
                at: 0.0,
                text: "a rewrite of the narrated clip".to_string(),
                emotion: String::new(),
            },
            Written {
                start: 0.0,
                end: 10.0,
                at: 0.0,
                text: "and a new one".to_string(),
                emotion: String::new(),
            },
        ],
    );

    assert!(
        folded.is_silent(24.0, 31.0),
        "the user's own silence marker survives a rewrite of every other clip"
    );
    assert_eq!(folded.silent, record.silent);
    // And the rule the fold leans on says the same thing about the same swap.
    let kept = silent_survives_entry_rewrite(&record.silent, &record.entries, &folded.entries);
    assert_eq!(kept, record.silent);
}

#[test]
fn f4_1_s6_speak_pass_counts() {
    let record = Narration {
        entries: vec![
            Entry {
                s: 0.0,
                e: 10.0,
                // A deliberately silent clip: blank text, no take.
                ..Default::default()
            },
            Entry {
                s: 10.0,
                e: 24.0,
                text: "already synthesized".to_string(),
                ..Default::default()
            },
            Entry {
                s: 24.0,
                e: 31.0,
                text: "needs a take".to_string(),
                ..Default::default()
            },
        ],
        silent: Vec::new(),
    };
    // Only the middle clip's wav exists in the cache. `speak_pass` takes the lines the tick allows to
    // speak (F4.8: `narrate_off::lines_to_speak`), so it is handed the record's entries.
    let speaks = speak_pass("own", &record.entries, |entry| entry.s == 10.0);
    assert_eq!(
        speaks,
        vec![Speak::SkippedBlank, Speak::SkippedCached, Speak::Synthesize],
        "one decision per line, in the record's order"
    );
    assert_eq!(tally(&speaks), (1, 1), "one synthesized, one cached, the blank neither");
    assert_eq!(spoken_log(1, 1), "    narrate: 1 line(s) spoken, 1 already in the cache");
}

/// Run a plan through [`naivepost::narrate_run::run_with`] with stub seams, keeping the helper short
/// at the call sites above.
fn plan_with<FWrite, FSpeak>(
    plan: &Plan,
    write: FWrite,
    synthesize: FSpeak,
) -> Result<Outcome, String>
where
    FWrite: FnMut(&[(f64, f64)]) -> Result<usize, String>,
    FSpeak: FnMut(f64, f64, &str) -> Result<bool, String>,
{
    naivepost::narrate_run::run_with(plan, write, synthesize)
}
