//! P.policy.narrationRewrite — the toggle that decides what ▶ Narrate writes.
//! `spec/10-parameters.md` row `P.policy.narrationRewrite` (default **off**): off, only clips
//! without a line get one; on, every clip's line is rewritten. The prototype always rewrote every
//! line, which is why the default is off ([`naivepost::narrate_rewrite`], F4.1 S4).
//!
//! S1 the default is off · S2 off writes only the clip with no line · S3 on writes every clip ·
//! S4 the log says why · S5 the silent list survives both settings · S6 catalogue and project JSON.

use naivepost::narrate_rewrite as rw;
use naivepost::narration::{Entry, Narration, Silent};
use naivepost::params::{self, Family};
use naivepost::project;

const A: (f64, f64) = (10.0, 20.0);
const B: (f64, f64) = (20.0, 30.0);

fn entry(start: f64, end: f64, text: &str) -> Entry {
    Entry {
        s: start,
        e: end,
        at: 0.0,
        text: text.into(),
        ..Default::default()
    }
}

/// A cut of two clips, one of which already has a line, and B marked silent by hand.
fn narration() -> Narration {
    Narration {
        entries: vec![entry(A.0, A.1, "the opening line")],
        silent: vec![Silent { s: B.0, e: B.1 }],
    }
}

// --- S1: the default is off -------------------------------------------------------------------

#[test]
fn p_policy_narrationrewrite_s1_off_is_the_default() {
    // P.policy.narrationRewrite defaults to off, so an untouched project never rewrites a line the
    // user may have edited. Both spellings of the default are pinned against each other.
    assert!(!rw::REWRITE_DEFAULT);
    let policy = project::Policy::default();
    assert!(!policy.narration_rewrite.value);
    assert_eq!(policy.narration_rewrite.origin, project::Origin::Default);
    assert_eq!(params::family("P.policy.narrationRewrite"), Family::Policy);
}

// --- S2: off writes only the clip that has no line --------------------------------------------

#[test]
fn p_policy_narrationrewrite_s2_off_writes_only_clips_without_a_line() {
    let n = narration();
    let clips = [A, B];
    // A already has a line and is left exactly as it is; B, which has none, is what gets written.
    assert_eq!(rw::clips_to_write(false, &clips, &n), vec![B]);
    assert_eq!(rw::unwritten(&clips, &n), 1);
    // Nothing in the narration moved while deciding — this is a read, not a rewrite.
    assert_eq!(n.entries.len(), 1);
    assert!(n.has_line(A.0, A.1));
    // With every clip already lined there is nothing for an off run to do.
    let all_lined = Narration {
        entries: vec![entry(A.0, A.1, "a"), entry(B.0, B.1, "b")],
        silent: vec![],
    };
    assert_eq!(rw::clips_to_write(false, &clips, &all_lined), Vec::<(f64, f64)>::new());
    assert_eq!(rw::unwritten(&clips, &all_lined), 0);
}

// --- S3: on writes every clip ---------------------------------------------------------------

#[test]
fn p_policy_narrationrewrite_s3_on_rewrites_every_line() {
    let n = narration();
    let clips = [A, B];
    // The toggle buys the whole redo: A's existing line is included, because the user asked for the
    // narration to be done again rather than filled in.
    assert_eq!(rw::clips_to_write(true, &clips, &n), clips.to_vec());
    // And it does not care how complete the narration already was.
    let all_lined = Narration {
        entries: vec![entry(A.0, A.1, "a"), entry(B.0, B.1, "b")],
        silent: vec![],
    };
    assert_eq!(rw::clips_to_write(true, &clips, &all_lined), clips.to_vec());
}

// --- S4: the log says why this run is happening ----------------------------------------------

#[test]
fn p_policy_narrationrewrite_s4_the_reason_names_the_cause() {
    let n = narration();
    let clips = [A, B];
    let missing = rw::indexes_without_line(&clips, &n);
    assert_eq!(missing, vec![1]);

    // Toggle on wins over staleness: the cause is the user's request, not a gap.
    assert_eq!(rw::why(true, &missing, 2, 1, 0), "rewriting every line");
    // Nothing written anywhere: the head of the flow, not a missing clip.
    assert_eq!(rw::why(false, &[0, 1], 2, 0, 0), "there is no narration yet");
    // One clip with no line, named where a person can act on it.
    assert_eq!(
        rw::why(false, &missing, 2, 1, 0),
        "clip 2 has no narration \u{2014} it is new, or the cut moved under it"
    );
    // Lines sitting on video the cut dropped: said first among the stale cases, because the run has
    // work to undo before the gap matters.
    assert_eq!(
        rw::why(false, &missing, 2, 3, 1),
        "the narration has lines for clips the cut no longer has"
    );
    // Every clip lined and nothing off the cut: nothing forces this run, which is what the toggle is for.
    assert_eq!(rw::why(false, &[], 2, 2, 0), "every clip already has a line");

    // The frame around it, in F4.1 S3's shape.
    assert_eq!(
        rw::log_line("rewriting every line", 2),
        ">>> narrate: rewriting every line \u{2014} writing 2 clip(s), one LLM call, then speaking them"
    );
    assert!(rw::log_line("there is no narration yet", 1).starts_with(">>> narrate: "));
}

// --- S5: the silent list survives either setting ---------------------------------------------

#[test]
fn p_policy_narrationrewrite_s5_the_silent_list_is_untouched_either_way() {
    let mut n = narration();
    n.sort();
    assert!(rw::silent_kept(&n, B.0, B.1), "B is marked silent by hand");
    assert!(!rw::silent_kept(&n, A.0, A.1));

    let before = n.clone();
    // Deciding what to write reads the narration and changes none of it — off ...
    let off = rw::clips_to_write(false, &[A, B], &n);
    assert_eq!(n, before, "an off pass left everything alone");
    // ... and on alike. The silent marker is the user's decision and outlives a full rewrite of the
    // entries; the prototype rebuilt the list from what the model wrote, which is how a hand-made
    // "this clip plays its own audio" disappeared (audit_gaps' Defect::SilentListWipe, decided in
    // F4.1). Here it cannot happen: nothing in this module writes to a Narration.
    let on = rw::clips_to_write(true, &[A, B], &n);
    assert_eq!(n, before, "even a rewrite-everything pass leaves the markers alone");
    assert!(rw::silent_kept(&n, B.0, B.1));
    // A silent clip is still written for when it has no line: it needs a caption even with no voice on
    // it, so silence is not a filter on this list.
    assert!(off.contains(&B));
    assert!(on.contains(&B));
    assert_eq!(
        naivepost::audit_gaps::silent_survives_entry_rewrite(&n.silent, &[], &[]),
        n.silent
    );
}

// --- S6: catalogue row and the project file ---------------------------------------------------

#[test]
fn p_policy_narrationrewrite_s6_rowed_as_off_and_absent_reads_as_off() {
    // `params::find` answers Prepare's rows only, so the Narrate section is searched directly — as
    // every other narrate/produce row's test does.
    let row = params::narrate()
        .into_iter()
        .find(|param| param.id == "P.policy.narrationRewrite")
        .expect("§10's row exists");
    // Spelled the way §10 spells it, read from the project's own default rather than retyped.
    assert_eq!(row.spelled, "off");
    assert!(row.from.contains("narration_rewrite"), "{}", row.from);

    // Written out it carries the camelCase key the spec uses.
    let mut policy = project::Policy::default();
    policy.narration_rewrite = project::Field {
        value: true,
        origin: project::Origin::User,
    };
    let json = serde_json::to_string(&policy).unwrap();
    assert!(json.contains("\"narrationRewrite\""), "{json}");

    // A project file written before this parameter existed has no key, and reads as off — never on,
    // which would rewrite lines nobody agreed to.
    let old: project::Policy = serde_json::from_str("{}").unwrap();
    assert!(!old.narration_rewrite.value);
    assert_eq!(old.narration_rewrite.origin, project::Origin::Default);
}
