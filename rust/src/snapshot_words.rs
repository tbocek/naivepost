//! The `05-words` screen's one network-free press: §F2.14 S2a, the WORDS mode — no model asked, so the
//! whole job is reading a session and landing what the words say.

use crate::layout::Tree;
use crate::project::Project;
use crate::requests::{AlignedDoc, Word, WordsDoc};
use crate::textfmt::Retake;
use crate::wave::Wave;

/// Peaks per second of the envelope staged here. Whole hertz because `wave::encode` stores it as a u16.
const HZ: f64 = 5.0;
/// A quiet byte: `degraded::dbfs(0)` is minus infinity, under `cut_wave_edges::SILENCE_THRESHOLD_DB`.
const QUIET: u8 = 0;
/// A byte that is sound by a wide margin, so rounding cannot make it quiet.
const LOUD: u8 = 200;
/// Where the second scene starts on the session clock, and on the tape: no hand shift and no offset, so
/// `runs_of` moves its words by exactly `seg.s` (`timeline::session_at(local, seg.s, 0, 0)`).
pub const SCENE_GAP_START: f64 = 6.0;
pub const GAP_SECONDS: f64 = 1.0;
/// The last word of the first scene, in that tape's seconds; the scene stops at `SCENE_GAP_START - GAP_SECONDS`.
pub const FIRST_LAST: f64 = 4.5;
/// The first word of the second scene, in that tape's seconds: 1.2 s after the first trimmed run ends
/// (`FIRST_LAST + suggest::TRIM_REACH_SECONDS`), i.e. inside the seam the walk closes.
pub const SECOND_FIRST_LOCAL: f64 = 0.1;
/// The last word of the second scene, in that tape's seconds.
pub const SECOND_LAST_LOCAL: f64 = 3.0;
/// Seconds of the lane the envelope covers, matching the second scene's end.
pub const LANE_SECONDS: f64 = 30.0;
/// The recording's name on disk and the lane key its wave cache is filed under.
const RECORDING: &str = "lecture.mkv";
const LANE: &str = "lecture";

pub fn stage() -> Option<Tree> {
    let dir = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    let _ = std::fs::remove_dir_all(&dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    Tree::new(&dir).ok()
}

/// Write everything the words door reads into `tree` and return the project whose footage row names the
/// recording — `silences_of` reads the envelope only for a source the project lists.
pub fn stage_session(tree: &Tree) -> Option<Project> {
    write_words_files(tree).ok()?;
    write_marks(tree).ok()?;
    write_envelope(tree).ok()?;
    // The page opens on THIS cut: `snapshot::run` seeds every Cut-page shot from the project folder's
    // `cut/cut.json`, so the two-scene base has to be on disk before the window is built or the words arm
    // finds one run covering the lane and there is no hole for the walk-back to close.
    crate::cut::save(&base_cut(), tree).ok()?;
    let mut project = Project::default();
    project.sources = vec![crate::project::Source {
        path: format!("project:sources/{RECORDING}"),
        footage: true,
        ..Default::default()
    }];
    project.context = "The weekly blockchain lecture, recorded in one sitting.".to_string();
    crate::project::save(&project, tree.dir()).ok()?;
    Some(project)
}

/// Press ▶ in the WORDS mode through the window's own door and answer the sentence the status row shows.
pub fn press_through_page(window: &adw::ApplicationWindow) -> String {
    let said = crate::cut_suggest_words_run::press_words(window);
    crate::ui::window::set_status_line(window, &said);
    said
}

pub fn base_cut() -> crate::cut::Cut {
    crate::cut::Cut {
        segs: vec![
            crate::cut::Seg {
                s: 0.0,
                e: SCENE_GAP_START - GAP_SECONDS,
                cam: 0,
                ..Default::default()
            },
            crate::cut::Seg {
                s: SCENE_GAP_START,
                e: LANE_SECONDS,
                cam: 0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn write_words_files(tree: &Tree) -> Result<(), String> {
    let hz = crate::transcribe::SAMPLE_RATE as f64;
    let spoken = [
        ("hello", 1.0),
        ("there", FIRST_LAST),
        ("half", SECOND_FIRST_LOCAL),
        ("end", SECOND_LAST_LOCAL),
    ];
    let words: Vec<Word> = spoken
        .iter()
        .map(|(text, at)| Word {
            word: text.to_string(),
            start_sample: (*at * hz) as u64,
            end_sample: ((at + 0.3) * hz) as u64,
        })
        .collect();
    crate::requests::write_words(
        tree,
        "cam0",
        &WordsDoc {
            text: spoken
                .iter()
                .map(|(w, _)| *w)
                .collect::<Vec<&str>>()
                .join(" "),
            words: words.clone(),
        },
    )?;
    crate::requests::write_aligned(tree, "cam0", &AlignedDoc { words })
}

/// One retake mark, outside both trimmed spans, so the walk-back has a materialisation (§1's fourth row demands
/// one) without cutting the closed hole back open.
fn write_marks(tree: &Tree) -> Result<(), String> {
    crate::textfmt::write_retakes(
        &[Retake {
            s: 25.0,
            e: 26.0,
            again: 0.0,
            to: 26.0,
            text: String::new(),
            whole: String::new(),
        }],
        &tree.retakes_tsv(),
    )
}

/// The lane's envelope through `wave::write`, stamped against the recording it describes: talk, silence, talk.
fn write_envelope(tree: &Tree) -> Result<(), String> {
    std::fs::create_dir_all(tree.sources_dir()).map_err(|err| err.to_string())?;
    let path = tree.sources_dir().join(RECORDING);
    std::fs::write(&path, [7u8; 4_096]).map_err(|err| err.to_string())?;
    let meta = std::fs::metadata(&path).map_err(|err| err.to_string())?;
    let mtime = meta
        .modified()
        .map_err(|err| err.to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_secs() as i64;
    let mut peaks = vec![LOUD; ((SCENE_GAP_START - GAP_SECONDS) * HZ) as usize];
    peaks.extend(vec![QUIET; (GAP_SECONDS * HZ) as usize]);
    peaks.extend(vec![LOUD; ((LANE_SECONDS - SCENE_GAP_START) * HZ) as usize]);
    crate::wave::write(
        tree,
        LANE,
        &Wave {
            hz: HZ,
            chans: vec![peaks],
        },
        meta.len() as i64,
        mtime,
    )
}
