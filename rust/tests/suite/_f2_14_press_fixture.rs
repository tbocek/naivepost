#![allow(dead_code)]

//! F2.14's press scenarios, shared by the three one-scenario-per-binary test files
//! (`f2_14_press_words.rs`, `f2_14_press_model.rs`, `f2_14_press_dead.rs`).
//!
//! A binary includes this with `#[path = "f2_14_press_fixture.rs"] mod press_fixture;` — the
//! `captions_wire_harness.rs` arrangement. One scenario per BINARY because GTK allows exactly one
//! `adw::Application::run_with_args` per process and libadwaita is only initialised while that loop is
//! alive: a second scenario in the same binary builds its window after the loop has been reaped and
//! dies with "Gtk has to be initialized".
//!
//! The seam each scenario drives is the program's own: `cut_suggest_*::press` re-reads the settings per
//! request (`settings::from_environment()`) and `services::llm_endpoint` builds the URL from
//! `Conf::server`, the field the Settings box writes — so pointing the config folder at a loopback port
//! makes the program telephone the fake server instead of a canned reply standing in for it.

use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::project::Project;
use naivepost::requests::{self, AlignedDoc, Word, WordsDoc};
use naivepost::settings::{self, Conf};
use naivepost::textfmt::Retake;

use crate::common::{fixture_dir, temp_dir};

/// A session folder with everything §S1's guards and §S2's inputs read, under a throwaway root.
pub struct Fixture {
    pub root: std::path::PathBuf,
    pub tree: Tree,
    pub project: Project,
}

impl Fixture {
    pub fn fresh(tag: &str) -> Self {
        crate::common::pin_private_cwd();
        let root = temp_dir(tag);
        std::fs::create_dir_all(&root).expect("create the throwaway root");
        let dir = root.join("session.naivepost");
        std::fs::create_dir_all(&dir).expect("create the session folder");
        let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
        std::fs::create_dir_all(
            tree.session_txt()
                .parent()
                .expect("session.txt sits in a folder"),
        )
        .expect("the transcript folder");
        let project = naivepost::project::load(&fixture_dir()).expect("fixture loads");
        let json = serde_json::to_string_pretty(&project).expect("project serialises");
        std::fs::write(dir.join(naivepost::project::PROJECT_FILE), json)
            .expect("write naivepost.json");
        Self {
            root,
            tree,
            project,
        }
    }

    /// §S1's timeline marker, and the seconds the model copies its stamps off.
    pub fn with_timeline(self) -> Self {
        std::fs::write(
            self.tree.session_txt(),
            "0:03 hello there\n0:31 second half\n0:52 that is all\n",
        )
        .expect("write session.txt");
        self
    }

    /// Usable words for `cam0`: `words.json` AND `words.aligned.json`, both non-empty, at 16 kHz.
    pub fn with_words(self, spoken: &[(&str, f64)]) -> Self {
        let words: Vec<Word> = spoken
            .iter()
            .map(|(text, at)| Word {
                word: text.to_string(),
                start_sample: (*at * 16000.0) as u64,
                end_sample: ((at + 0.3) * 16000.0) as u64,
            })
            .collect();
        let doc = WordsDoc {
            text: spoken
                .iter()
                .map(|(w, _)| *w)
                .collect::<Vec<&str>>()
                .join(" "),
            words,
        };
        requests::write_words(&self.tree, "cam0", &doc).expect("write words.json");
        requests::write_aligned(&self.tree, "cam0", &AlignedDoc { words: doc.words })
            .expect("write words.aligned.json");
        self
    }

    pub fn with_marks(self, marks: &[Retake]) -> Self {
        textfmt_write_retakes(marks, &self.tree.retakes_tsv()).expect("write retakes.tsv");
        self
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn textfmt_write_retakes(marks: &[Retake], path: &std::path::Path) -> Result<(), String> {
    naivepost::textfmt::write_retakes(marks, path)
}

pub fn retake(s: f64, e: f64) -> Retake {
    Retake {
        s,
        e,
        again: 0.0,
        to: e,
        text: String::new(),
        whole: String::new(),
    }
}

/// The WORDS arm's starting point (§S2a): nothing marked, no `final.txt`, so `no_model` has real work
/// to do, and the page's own cut as its base — the fixture's is a multi-scene session, which gives the
/// walk something to trim.
pub fn words_cut() -> Cut {
    let text =
        std::fs::read_to_string(fixture_dir().join("cut/cut.json")).expect("fixture cut.json");
    serde_json::from_str(&text).expect("the fixture cut parses")
}

/// One scene plus room beyond it: what the model is asked about in §S2b and what a dead dial must not
/// change on screen.
pub fn one_scene_cut() -> Cut {
    Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 60.0,
            cam: 0,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// Point the settings folder at a throwaway config/data pair and answer the LLM address the fake
/// listens on — the same field a person edits in Settings, and the one the press re-reads per request.
///
/// The env is process-wide, which is fine here: one scenario per binary pins it once, before any window
/// is built, and then only rewrites `server` in the file.
pub fn point_llm_at(url: &str) {
    let root = temp_dir("press-env");
    std::fs::create_dir_all(&root).expect("scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = Conf::default();
    conf.model = "suggest-model".to_string();
    conf.server = url.to_string();
    settings::save(&paths, &conf).expect("settings written");
}

/// Where a write lands: `save_insert_cut` writes to `startup::session_dir(current_dir())`, i.e. the
/// `session.naivepost` beside the process's working directory (§1's working copy), NOT the folder the
/// session was opened from — so the tree a test built in a temp folder is not the file to read back.
pub fn run_tree() -> Tree {
    crate::common::pin_private_cwd();
    let root = std::env::current_dir().expect("the process has a working directory");
    Tree::new(naivepost::startup::session_dir(&root))
        .expect("the working copy is a .naivepost folder")
}

/// The model's answer, framed the way a real server frames it: `choices[0].message.tool_calls`, each
/// with `function.name` and `function.arguments`. The program parses this through
/// `llm_request::reply_from`, so nothing here is shaped for the test's convenience.
pub fn tool_turn(calls: &[(&str, serde_json::Value)]) -> String {
    let list: Vec<serde_json::Value> = calls
        .iter()
        .map(|(name, args)| {
            serde_json::json!({
                "function": { "name": name, "arguments": args.to_string() },
            })
        })
        .collect();
    serde_json::json!({
        "choices": [{ "message": { "content": "", "tool_calls": list } }]
    })
    .to_string()
}
