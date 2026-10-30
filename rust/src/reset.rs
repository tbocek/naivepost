//! ↺ on the run bar (spec/03-shell.md F0.15): what each workflow generated, so it can be done again from nothing.
//!
//! This file only decides and removes; the button, the question and the refresh are
//! [`crate::ui::window_reset`]'s. Each page owns its outputs and the model answers they were made from: the
//! answers go too, or the redo would replay them and come back the same. Sources, the project file, a later
//! page's outputs and the exchange log are nobody's output here and stay.

use std::path::PathBuf;

use crate::layout::Tree;
use crate::shell::Page;

/// The model answers each page's steps cached, by the step's own folder under `cache/llm/`.
fn cached_steps(page: Page) -> &'static [&'static str] {
    match page {
        Page::Prepare => &["policy", "textgate", "describe", "fix", "retakes", "textedit"],
        Page::Cut => &["cut", "captions", "speed", "effects"],
        Page::Narrate => &["narrate", "speak"],
        Page::Produce => &["translate", "youtube", "thumbnail"],
    }
}

/// What a reset of `page` removes that is on disk now. Empty when there is nothing to remove.
pub fn plan(tree: &Tree, page: Page) -> Vec<PathBuf> {
    let dir = tree.dir();
    let mut paths: Vec<PathBuf> = match page {
        Page::Prepare => vec![
            tree.prepare_dir(),
            tree.stems_dir(),
            dir.join("cache/waves"),
            dir.join("cache/edges"),
        ],
        // The red line (`cut/line.json`) is where the page stands, not a result; the page writes it again at once.
        Page::Cut => vec![tree.cut_json(), dir.join("cut/cut.log.json")],
        // The voice settings (voice.txt, pitch.txt, the reference recording) are choices, not outputs.
        Page::Narrate => vec![
            tree.narration_json(),
            tree.narration_prev_json(),
            tree.takes_json(),
            dir.join("narrate/tts"),
        ],
        // `publish/` beside `produce/` is where a project from before the move still keeps its upload text.
        Page::Produce => vec![tree.produce_dir(), dir.join("publish")],
    };
    paths.extend(
        cached_steps(page)
            .iter()
            .map(|step| dir.join("cache/llm").join(step)),
    );
    paths.retain(|path| path.exists());
    paths
}

/// What the question says goes, and what stays, so nobody has to know the folder layout to answer it.
pub fn what_goes(page: Page) -> &'static str {
    match page {
        Page::Prepare => {
            "Frames, transcripts, descriptions, the word list, the marks, final.txt with any edit made to it, \
             separated voices, and the model's answers for these steps. Everything is made again on the next \
             ▶, which can take a long time and asks the model again.\n\nThe sources, the project and the cut stay."
        }
        Page::Cut => {
            "The cut, every hand edit in it, and the model's answers for the cut.\n\nThe sources, the project \
             and what Prepare made stay."
        }
        Page::Narrate => {
            "The narration lines, their spoken audio, and the model's answers for them.\n\nThe voice settings, \
             the sources, the project and the cut stay."
        }
        Page::Produce => {
            "The finished video, its clips and subtitles, the upload text and thumbnail, and the model's answers \
             for them.\n\nThe sources, the project, the cut and the narration stay."
        }
    }
}

/// What a removal came to: the paths gone, relative to the project, and those that would not go with their error.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Removed {
    pub gone: Vec<String>,
    pub failed: Vec<(String, String)>,
}

/// Remove what [`plan`] named. A path that will not go is said and the rest still go.
pub fn remove(tree: &Tree, paths: &[PathBuf]) -> Removed {
    let mut removed = Removed::default();
    for path in paths {
        let shown = path
            .strip_prefix(tree.dir())
            .unwrap_or(path)
            .display()
            .to_string();
        let result = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        match result {
            Ok(()) => removed.gone.push(shown),
            Err(error) => removed.failed.push((shown, error.to_string())),
        }
    }
    removed
}
