//! The project on disk: `naivepost.json` (spec/01-project-and-files.md §2).
//!
//! Pure data and file format — no GTK here, so every rule is testable without a
//! window (spec/00-principles.md §5, directive C).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const PROJECT_FILE: &str = "naivepost.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub sources: Vec<Source>,
    pub interval: f64,
    pub language: String,
    pub no_narration: bool,
    pub reference_sources: bool,
    pub context: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Source {
    pub path: String,
    pub footage: bool,
    pub narrator: u32,
    pub sepvoice: bool,
    pub tracks: Vec<u32>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            interval: 1.0,
            language: "en".to_string(),
            no_narration: false,
            reference_sources: false,
            context: String::new(),
        }
    }
}

impl Default for Source {
    fn default() -> Self {
        Self {
            path: String::new(),
            footage: false,
            narrator: 0,
            sepvoice: false,
            tracks: Vec::new(),
        }
    }
}

/// Read `<dir>/naivepost.json`. A missing file is the same state as an empty
/// project, so it loads the defaults rather than failing.
pub fn load(dir: &Path) -> Result<Project, String> {
    let file = dir.join(PROJECT_FILE);
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Project::default()),
        Err(err) => return Err(format!("{}: {err}", file.display())),
    };
    serde_json::from_str(&text).map_err(|err| format!("{}: {err}", file.display()))
}

/// Write `<dir>/naivepost.json`, creating the directory if needed.
pub fn save(project: &Project, dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let text = serde_json::to_string_pretty(project).map_err(|err| err.to_string())?;
    let file = dir.join(PROJECT_FILE);
    fs::write(&file, text).map_err(|err| format!("{}: {err}", file.display()))
}
