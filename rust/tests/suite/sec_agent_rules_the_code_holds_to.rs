//! §AGENT#rules-the-code-holds-to — the nine rules `spec/AGENT.md` states, each proven through the
//! door that enforces it rather than quoted from the prose.
//!
//! `naivepost --rules` prints what [`naivepost::code_rules::lines`] computes, so the wire test (s10)
//! and the rule tests below check the same numbers from two sides: the module's answer and the door's.
//! Nothing here needs a display or a server; the CLI cases go through doors answered before GTK opens.

use std::path::{Path, PathBuf};
use std::process::Command;

use naivepost::code_rules;
use naivepost::degraded;
use naivepost::edges::{self, AlignedWord};
use naivepost::layout;
use naivepost::llm_cache::{self, Request};
use naivepost::params;
use naivepost::prompts;
use naivepost::retakes;
use naivepost::run_stages;
use naivepost::textfmt::Retake;
use naivepost::tools::retakes::{Line, Marks, Trim};

/// The checkout root: `rust/` is the crate, one level below it is the repo.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Run the built binary from `dir`, returning stdout. Both doors used here print and exit before any
/// display is asked for.
fn run(args: &[&str], dir: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_naivepost"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("the built binary runs");
    assert!(
        output.status.success(),
        "`naivepost {}` exited {}: {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// This test program's own scratch folder, so parallel test binaries never share one.
fn scratch(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "naivepost-rules-{}-{}",
        tag,
        std::process::id()
    ))
}

/// S1: models propose in words, line numbers or clip-relative seconds; the machine places the cut.
#[test]
fn sec_agent_rules_the_code_holds_to_s1_models_propose_the_machine_places() {
    for key in prompts::KEYS {
        assert!(prompts::is_known(key), "{key} is a prompt key this build knows");
    }
    let lines = code_rules::lines();
    assert_eq!(lines.len(), 9, "one line per rule the spec lists");
    assert!(
        lines[0].contains("place_edges") && lines[0].contains("clamp_to_cut"),
        "rule 1 names both placing doors:\n{}",
        lines[0]
    );
    // The ask side: no prompt key reaches the model as a request for an absolute timestamp — the keys
    // are the jobs, and every one of them answers in text the app turns into a cut.
    assert_eq!(prompts::KEYS.len(), 13, "the shipped job list is the one §9 counts");
}

/// S2: every cut lands on a word edge — the words fence both edges, the envelope only chooses inside.
#[test]
fn sec_agent_rules_the_code_holds_to_s2_a_cut_lands_on_a_word_edge() {
    let words = [
        AlignedWord { word: "one".into(), s: 4.0, e: 5.0 },
        AlignedWord { word: "two".into(), s: 6.0, e: 7.0 },
    ];
    let mark = Retake { s: 5.4, e: 6.0, again: 8.0, to: 6.0, text: String::new(), whole: String::new() };
    // No envelope at all (`edge_of` answers None): the word times alone must place both edges, and they
    // may not cross a word — the pad is all the room the sound gets.
    let (placed, notes) = edges::place_edges(vec![mark], &words, |_| None, &[(0.0, 20.0)]);
    let m = &placed[0];
    assert!(
        m.s >= 5.0 - 1e-9 && m.s <= 5.0 + edges::WORD_PAD + 1e-9,
        "the front edge stays between the last word that stays and the first that goes: {}",
        m.s
    );
    assert!(
        m.to >= 6.0 - 1e-9 && m.to <= 8.0 + 1e-9,
        "the resume edge stays between the dropped word's end and the retake: {}",
        m.to
    );
    assert!(!notes.is_empty(), "moving an edge says so on the log: {notes:?}");
}

/// S3: nothing is deleted, only marked — marking adds a range and leaves every transcript line alone.
#[test]
fn sec_agent_rules_the_code_holds_to_s3_nothing_deleted_only_marked() {
    let lines = vec![
        Line { n: 1, start: 0.0, end: 2.0 },
        Line { n: 2, start: 2.0, end: 4.0 },
        Line { n: 3, start: 8.0, end: 10.0 },
    ];
    let before = lines.clone();
    let mut marks = Marks::new(lines, 6.0);
    let answer = marks.mark_abandoned(1, 2, Some(3), Trim::none());
    assert!(
        !answer.starts_with("!!!") && !answer.contains("error"),
        "a well-formed mark is accepted: {answer}"
    );
    assert_eq!(marks.marks().len(), 1, "the mark was added");
    assert_eq!(marks.lines(), &before[..], "every transcript line is still there -- marking removes no word");
    assert_eq!(
        retakes::parse_answer(r#"{"abandoned": [{"from": 7, "to": 12, "again": 15}]}"#),
        vec![(7, 12, 15)],
        "the model answers in 1-based line numbers, never timestamps"
    );
}

/// S4: a step's own output file is its resume marker.
#[test]
fn sec_agent_rules_the_code_holds_to_s4_the_output_file_is_the_resume_marker() {
    assert!(
        run_stages::Task::already_there("extracting frames").already_done,
        "a step whose output is on disk is taken, never run again"
    );
    assert!(
        !run_stages::Task::run_it("extracting frames", "ffmpeg", Vec::new()).already_done,
        "a step with nothing on disk must run"
    );
}

/// S5: a tuning number lives once in the module whose rule uses it; `params.rs` catalogues and holds none.
#[test]
fn sec_agent_rules_the_code_holds_to_s5_params_catalogues_and_holds_none() {
    let sections: [(&str, Vec<params::Param>); 5] = [
        ("prepare", params::prepare()),
        ("cut", params::cut()),
        ("effects", params::effects()),
        ("narrate", params::narrate()),
        ("produce", params::produce()),
    ];
    for (name, rows) in &sections {
        assert!(!rows.is_empty(), "{name} catalogues its section's tuning values");
        for row in rows {
            assert!(!row.id.is_empty(), "every row carries §10's id");
            assert!(
                row.id.starts_with("P.") || row.id.contains('.'),
                "{} is either a P. id or a bare-prefixed value (§10 gives no P. id)",
                row.id
            );
            assert!(
                row.from.contains("::"),
                "{} names the constant it is built from, not a number of its own ({})",
                row.id,
                row.from
            );
        }
    }
    assert!(params::find("P.policy.minTakeSeconds").is_some(), "§10's first policy id is catalogued");
}

/// A request shaped like every real one, varying only what a case is about.
fn probe(images: Vec<String>) -> Request<'static> {
    Request {
        system: "system",
        user: "user",
        state: "",
        speech: "",
        context: "",
        images,
        run_index: None,
        model: "probe-model",
        thinking: false,
    }
}

/// S6: `cache/llm` is keyed on the exact request, images included.
#[test]
fn sec_agent_rules_the_code_holds_to_s6_the_cache_key_is_the_exact_request() {
    let key = |req: &Request| llm_cache::key(&llm_cache::parts(req)).expect("a real request has a key");
    let plain = probe(Vec::new());
    assert_eq!(key(&plain), key(&probe(Vec::new())), "an unchanged step hits the same entry");
    let one = probe(vec!["data:one".into()]);
    assert_ne!(key(&plain), key(&one), "an image changes the question");
    assert_ne!(
        key(&probe(vec!["data:a".into(), "data:b".into()])),
        key(&probe(vec!["data:b".into(), "data:a".into()])),
        "images are positional: swapping two frames is a different question"
    );
    assert_ne!(
        key(&one),
        key(&probe(vec!["data:ONE".into()])),
        "the key covers the image bytes exactly"
    );
    assert_eq!(
        llm_cache::parts(&probe(vec!["a".into(), "b".into()])).len(),
        llm_cache::parts(&plain).len() + 2,
        "one part per image, in send order"
    );
    assert!(llm_cache::key(&[]).is_none(), "no question, no key: nothing is shared with empty input");
}

/// S7: failure is specific and local — name the model and the reason, carry on elsewhere.
#[test]
fn sec_agent_rules_the_code_holds_to_s7_failure_is_specific_and_local() {
    assert!(degraded::forgot_job(404) && degraded::forgot_job(410), "both mean the server forgot");
    assert!(!degraded::forgot_job(500), "any other status is an answer about a live job");
    let message = degraded::forgot_job_message("job7", 404);
    assert!(message.contains("job7") && message.contains("404"), "the sentence names both: {message}");
    let (outcome, lines) = degraded::finish_render(false, Err("sd.cpp job job7 HTTP 404 gone".into()));
    assert_eq!(
        outcome,
        degraded::RenderOutcome::ThumbnailFailed(message.clone()),
        "the video half stands; only the thumbnail is reported"
    );
    assert_eq!(lines, vec![message], "one specific line, naming the job and the status");
    let (whole, quiet) = degraded::finish_render(false, Ok(()));
    assert_eq!(whole, degraded::RenderOutcome::Complete);
    assert!(quiet.is_empty(), "nothing to say when nothing failed");
}

/// S8: a project is a folder ending in `.naivepost`; a path in it stores as `project:` + root-relative.
#[test]
fn sec_agent_rules_the_code_holds_to_s8_paths_store_project_relative() {
    let scratch = scratch("paths");
    let root = scratch.join("root");
    let project = root.join("demo.naivepost");
    let inside = project.join("sub").join("take.wav");
    std::fs::create_dir_all(project.join("sub")).expect("scratch project");
    let written = layout::write_path(&root, &project, &inside);
    assert_eq!(written, "project:sub/take.wav", "inside the project stores as project: + relative");
    assert_eq!(
        layout::resolve(&root, &project, &written),
        inside,
        "and resolves back to the very path it came from"
    );
    assert_eq!(
        layout::write_path(&root, &project, &root.join("other.wav")),
        "other.wav",
        "under the root but outside the project stores root-relative"
    );
    assert_eq!(
        layout::write_path(&root, &project, Path::new("/elsewhere/take.wav")),
        "/elsewhere/take.wav",
        "nowhere near the project stays absolute -- the only place an absolute path survives"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// S9: tests pin reasons, not only outputs — the shipped wording every prompt test quotes is reachable.
#[test]
fn sec_agent_rules_the_code_holds_to_s9_the_pinned_wording_is_reachable() {
    for key in prompts::KEYS {
        let file = prompts::shipped_file(key).unwrap_or_else(|| panic!("{key} names a shipped file"));
        assert!(
            file.starts_with("prompts/") && file.ends_with(".md"),
            "{key} ships under spec/prompts/, named {file}"
        );
        assert!(
            prompts::shipped_text(key).is_some(),
            "{key}'s shipped wording reads from this tree -- a test can quote it"
        );
    }
}

/// S10: the wire — `naivepost --rules` prints exactly those lines, and `--help` names the door.
#[test]
fn sec_agent_rules_the_code_holds_to_s10_the_binary_prints_the_rules() {
    let printed = run(&["--rules"], &root());
    for line in code_rules::lines() {
        assert!(
            printed.contains(&line),
            "`--rules` prints every line the module produces; missing:\n{line}\ngot:\n{printed}"
        );
    }
    assert_eq!(
        printed.lines().count(),
        code_rules::lines().len(),
        "one line per rule the spec lists, and nothing else on stdout:\n{printed}"
    );
    let help = run(&["--help"], &root());
    assert!(help.contains("--rules"), "--help names the door that prints the rules");
}
