//! Starting a project of one's own (spec/03-shell.md F0.8). ＋ New is refused while a run works and asks before throwing anything away; those decisions live here and the UI draws them (spec/00-principles.md §5).

use std::path::{Path, PathBuf};

use crate::{project, startup};

/// S1: a run reads this session's sources, so emptying it mid-run would pull them out. The em dash is the spec's.
pub const RUN_REFUSAL: &str =
    "stop the run first \u{2014} a new project would pull its inputs out from under it";
pub const QUESTION: &str = "Start a new project?";
pub const DETAIL: &str =
    "The sources, the session context and every prompt edit go back to empty. \
Files already written to the output folder are left alone.";
pub const BUTTON: &str = "Start new\u{2026}";
pub const TITLE: &str = "New project";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Refused { reason: &'static str },
    Confirm { detail: String },
    Name,
}

/// S1 → S2 → S3 in that precedence: a run outranks everything, and an empty session has nothing to lose so is not asked (S2).
/// Nothing here writes; Cancel just does not press on.
pub fn press(running: bool, empty: bool, open: &Path) -> Gate {
    if running {
        return Gate::Refused {
            reason: RUN_REFUSAL,
        };
    }
    if empty {
        return Gate::Name;
    }
    Gate::Confirm {
        detail: confirm_detail(open),
    }
}

/// S3's body: what goes back to empty, plus what happens to the open project (named or working copy).
pub fn confirm_detail(open: &Path) -> String {
    let named = open
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name != startup::SESSION_NAME);
    if named {
        let base = open
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        format!(
            "{DETAIL}\n\n{base} stays on disk as it is, with everything it has written \
-- this session simply stops being it."
        )
    } else {
        format!(
            "{DETAIL}\n\nThis session has never been saved under a name of its own, \
so there is nothing to come back to."
        )
    }
}

/// S4: `.naivepost` appended unless already there, compared case-insensitively.
pub fn with_suffix(name: &str) -> String {
    if Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("naivepost"))
    {
        return name.to_string();
    }
    format!("{name}{}", crate::layout::PROJECT_SUFFIX)
}

/// S4's default name: today's date, with `-2`, `-3` … while the folder is taken.
pub fn free_name(dir: &Path, today: &str) -> String {
    let mut name = with_suffix(today);
    for n in 2.. {
        if !dir.join(&name).exists() {
            return name;
        }
        name = with_suffix(&format!("{today}-{n}"));
    }
    unreachable!("a free name is always found: the numbers do not run out")
}

pub fn already_there(base: &str) -> String {
    format!("{base} is a project already \u{2014} open it, or pick another name")
}

/// What S6 did: the blank project to show, the status sentence, the log line, and where the source choosers open next. Only `PartialEq` because the project holds floats.
#[derive(Debug, PartialEq)]
pub struct Created {
    pub path: PathBuf,
    pub base: String,
    pub project: project::Project,
    pub status: String,
    pub log: String,
    pub chooser: Option<PathBuf>,
}

#[derive(Debug, PartialEq)]
pub enum Answer {
    Dismissed,
    Created(Box<Created>),
    Refused(String),
}

/// S4's answer as a rule: the path typed or picked (01 §1: a project IS a folder), or `None` when the sheet was closed.
/// The `.naivepost` suffix is added inside [`create`], so a typed name and a browsed folder share one rule and S5 fires on a typed name.
pub fn answer(root: &Path, chosen: Option<&Path>) -> Answer {
    let Some(path) = chosen else {
        return Answer::Dismissed;
    };
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let named = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    match create(root, &dir, &named) {
        Ok(created) => Answer::Created(Box::new(created)),
        Err(reason) => Answer::Refused(reason),
    }
}

/// S5 + S6: refuse a name that is a project already, else write the blank session there.
/// Only `naivepost.json` is created; the previously open project keeps its files (S3). Applying the blank project is F0.9's apply list; nothing here touches `llm.conf`.
pub fn create(root: &Path, dir: &Path, named: &str) -> Result<Created, String> {
    let path = dir.join(with_suffix(named));
    let base = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();

    // `project::load` defaults for a folder with no file, so "is a project already" must be the file itself.
    if path.join(project::PROJECT_FILE).is_file() {
        return Err(already_there(&base));
    }

    let blank = project::Project::default();
    project::save(&blank, &path)?;

    // Add opens where the project was put (where its footage likely is), except under the root.
    let parent = path.parent().map(|p| p.to_path_buf());
    let chooser = parent.filter(|parent| parent != root);

    Ok(Created {
        status: format!("new project \u{2014} {base}"),
        log: format!(
            ">>> new project {} -- the session is empty; outputs on disk are untouched",
            path.display()
        ),
        chooser,
        path,
        base,
        project: blank,
    })
}
