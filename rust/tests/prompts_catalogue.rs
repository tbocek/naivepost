//! §10-parameters.md §4 Prompts — the thirteen prompt keys and their thinking flags.

use naivepost::{bench, prompts, roles, settings};

const ITEM: &str = "§10-parameters#4-prompts";

/// A config home that exists only for this test, so nothing here reads or writes `$HOME`.
struct Dirs {
    root: std::path::PathBuf,
    paths: settings::Paths,
}

impl Dirs {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("np-prompts-{name}"));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        Self {
            paths: settings::paths_from(
                Some(root.join("cfg").to_str().unwrap()),
                Some(root.to_str().unwrap()),
                None,
                None,
            )
            .expect("a config dir given explicitly always resolves"),
            root,
        }
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

// ---- S1: the thirteen keys, in the spec's order -------------------------------------------

#[test]
fn sec_10_parameters_4_prompts_s1_the_thirteen_keys_are_exactly_the_specs_list() {
    assert_eq!(ITEM, "§10-parameters#4-prompts");
    // §4 verbatim: "system, describe, fix, retake, textedit, cut, captions, speed, effects,
    // narrate, translate, youtube (prompts/); new: policy." The first twelve are the shipped
    // set; `policy` is the one this rewrite adds.
    assert_eq!(
        prompts::KEYS,
        [
            "system",
            "describe",
            "fix",
            "retake",
            "textedit",
            "cut",
            "captions",
            "speed",
            "effects",
            "narrate",
            "translate",
            "youtube",
            "policy",
        ]
    );
    // The new row is known, and the file that shares `prompts/` but is not a prompt is not.
    assert!(prompts::is_known("policy"), "F0.7's policy prompt must be known");
    assert!(!prompts::is_known("tools"), "prompts/tools.md is tool descriptions, not a prompt key");
    assert!(!prompts::is_known(""), "an empty key is the User Context, not a prompt");
}

// ---- S2: every key is a storable per-machine override -------------------------------------

#[test]
fn sec_10_parameters_4_prompts_s2_every_key_stores_as_a_per_machine_override() {
    assert_eq!(ITEM, "§10-parameters#4-prompts");
    let d = Dirs::new("store");
    for key in prompts::KEYS {
        // A key names a file under `prompts/`, so each of the thirteen is a legal name.
        settings::prompt_file_ok(key)
            .unwrap_or_else(|err| panic!("{key}: not a usable prompt key: {err}"));
        assert_eq!(
            d.paths.prompt_file(key),
            d.paths.prompts_dir().join(format!("{key}.txt")),
            "{key} stores under the one-file-per-prompt layout"
        );
    }

    // The new key round-trips through the same storage as the twelve before it.
    let shipped = "Read the context, set only the fields it speaks to.";
    assert_eq!(
        settings::prompt_text(&d.paths, "policy", shipped).unwrap(),
        shipped,
        "nothing stored yet: the shipped wording answers"
    );
    assert!(
        settings::write_prompt(&d.paths, "policy", "set markingPass from the context alone", shipped)
            .unwrap(),
        "an edit differs from shipped, so it is kept"
    );
    assert_eq!(
        settings::prompt_text(&d.paths, "policy", shipped).unwrap(),
        "set markingPass from the context alone",
        "this machine's wording wins over the shipped one"
    );
    // Typing the shipped text back is how an edit is undone: `write_prompt` reports it stored
    // nothing, and the file on disk keeps the last real edit until Reset deletes it. The undo
    // lands at the read path rather than the write path — `stored_prompt` skips a file that trims
    // to nothing, so only an empty override reads as "use the shipped wording".
    assert!(
        !settings::write_prompt(&d.paths, "policy", shipped, shipped).unwrap(),
        "the shipped wording written back stores nothing"
    );
    assert_eq!(
        std::fs::read_to_string(d.paths.prompt_file("policy")).unwrap(),
        "set markingPass from the context alone",
        "a no-op write leaves the previous override where it was"
    );
    assert!(settings::clear_prompt(&d.paths, "policy").unwrap());
    assert_eq!(settings::prompt_text(&d.paths, "policy", shipped).unwrap(), shipped);
}

// ---- S3: thinking on for exactly four ------------------------------------------------------

#[test]
fn sec_10_parameters_4_prompts_s3_thinking_is_on_for_exactly_the_four_named() {
    assert_eq!(ITEM, "§10-parameters#4-prompts");
    // §4: "Thinking on for textedit, cut, narrate, youtube; off elsewhere."
    assert_eq!(prompts::THINKING_ON, ["textedit", "cut", "narrate", "youtube"]);
    for key in prompts::THINKING_ON {
        assert!(prompts::thinking(key), "{key} is in §4's thinking set");
    }
    let off: Vec<&str> = prompts::KEYS
        .iter()
        .copied()
        .filter(|k| !prompts::thinking(k))
        .collect();
    assert_eq!(
        off,
        [
            "system",
            "describe",
            "fix",
            "retake",
            "captions",
            "speed",
            "effects",
            "translate",
            "policy"
        ],
        "nine stay off — a tenth would have to be added to THINKING_ON on purpose"
    );
    // F0.7 asks with thinking off, which is why `policy` is in the off list rather than missing.
    assert!(!prompts::thinking("policy"), "F0.7 S2: the policy call runs with thinking off");
}

// ---- S4: the two tables agree ------------------------------------------------------------

#[test]
fn sec_10_parameters_4_prompts_s4_thinking_agrees_with_the_job_table_where_both_apply() {
    assert_eq!(ITEM, "§10-parameters#4-prompts");
    // Wherever a prompt has a job, its flag must be that job's flag: one rule, read two ways.
    for key in prompts::KEYS {
        if let Some(job) = prompts::job(key) {
            assert_eq!(
                prompts::thinking(key),
                roles::thinking(job),
                "{key} / {job:?}: the prompt's thinking flag and the job's disagree"
            );
        }
    }
    // And every job that thinks has a prompt key that says so, so a newly-thinking job cannot be
    // catalogued at one level only.
    let thinking_jobs: Vec<roles::Job> = roles::Job::all()
        .into_iter()
        .filter(|job| roles::thinking(*job))
        .collect();
    assert_eq!(thinking_jobs.len(), 4, "§2 gives four jobs thinking");
    for job in thinking_jobs {
        let keyed = prompts::KEYS
            .iter()
            .filter(|k| prompts::job(*k) == Some(job))
            .count();
        assert!(keyed >= 1, "{job:?} thinks but no prompt key names it");
        assert!(
            prompts::KEYS
                .iter()
                .any(|k| prompts::job(*k) == Some(job) && prompts::thinking(k)),
            "{job:?} thinks but its prompt says off"
        );
    }
    // The two keys with no job are the two §4 lists can only reach at this level.
    assert_eq!(prompts::job("system"), None, "the preamble is never a call of its own");
    assert_eq!(prompts::job("policy"), None, "the policy derivation has no §2 job row");
    // Three prompts share the clip-rules job: captions, speed and effects are one shape asked thrice.
    for key in ["captions", "speed", "effects"] {
        assert_eq!(prompts::job(key), Some(roles::Job::ClipRules), "{key} is a clip pass");
    }
}

// ---- S5: the shipped wording exists for all thirteen --------------------------------------

/// The tree this build ships its wording from, when `spec/` is beside the crate. An installed
/// tree without it skips rather than invents (same rule as `tests/web_tools_f6_1.rs`).
fn shipped_spec_dir() -> Option<std::path::PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .join("spec")
        .join("prompts");
    dir.is_dir().then_some(dir)
}

#[test]
fn sec_10_parameters_4_prompts_s5_each_key_has_shipped_wording_in_the_prompts_folder() {
    assert_eq!(ITEM, "§10-parameters#4-prompts");
    let Some(dir) = shipped_spec_dir() else {
        eprintln!("no spec/prompts beside this checkout — skipping the shipped-wording check");
        return;
    };
    for key in prompts::KEYS {
        let file = prompts::shipped_file(key).expect("every known key names a shipped file");
        assert_eq!(file, format!("prompts/{key}.md"), "{key}'s shipped path");
        let text = std::fs::read_to_string(dir.join(format!("{key}.md")))
            .unwrap_or_else(|err| panic!("{key}: no shipped wording at {file}: {err}"));
        assert!(
            !text.trim().is_empty(),
            "{key} ships an empty file — a prompt that says nothing is not a prompt"
        );
    }
    // tools.md sits in the same folder and is not a key: the web tools' descriptions travel with
    // the tool schemas (`web_tools` copies them), not with a system prompt.
    assert!(dir.join("tools.md").is_file(), "tools.md is in prompts/ …");
    assert!(!prompts::is_known("tools"), "… and is not a prompt key");
}

// ---- S6: the bench carries the twelve, and not the thirteenth ------------------------------

#[test]
fn sec_10_parameters_4_prompts_s6_the_bench_rows_are_the_twelve_in_order_and_policy_is_absent() {
    assert_eq!(ITEM, "§10-parameters#4-prompts");
    // Row 0 is the User Context; rows 1..=12 are §4's first twelve keys, in order, so the page
    // a person edits and the catalogue a request is built from cannot drift apart.
    assert!(bench::ROWS[0].key.is_empty(), "row 0 is the User Context");
    let bench_keys: Vec<&str> = bench::ROWS[1..].iter().map(|row| row.key).collect();
    assert_eq!(bench_keys, prompts::KEYS[..12], "the bench lists §4's twelve in order");
    // `policy` is deliberately NOT a bench row: §04-prepare#1 keeps "Editing policy" as a
    // REVIEW note, and F0.7 opens the policy *form* (value, source and reason per field) rather
    // than a box of wording. This assertion is what makes adding it later a deliberate change
    // that also has to settle where the form goes.
    assert!(
        !bench::ROWS.iter().any(|row| row.key == "policy"),
        "policy has no bench row while §04 leaves it a REVIEW note"
    );
    assert_eq!(bench::ROWS.len(), 13, "User Context + twelve prompts");
}
