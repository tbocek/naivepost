//! Starting a project of one's own (spec/03-shell.md F0.8).
//!
//! ＋ New empties the session into a folder the user names. Two things make that worth a module of
//! its own rather than a click handler: it is refused while a run works, and it asks before it
//! throws anything away — so which question applies, and what happens after the answer, are decided
//! here and drawn by the UI (spec/00-principles.md §5).

use std::path::{Path, PathBuf};

use crate::{project, startup};

/// S1: a run reads this session's sources, so emptying it mid-run would take them out from under
/// it. The em dash is the spec's.
pub const RUN_REFUSAL: &str =
    "stop the run first \u{2014} a new project would pull its inputs out from under it";
/// S3's question, and the promise that the press is not the last word: what follows names the new
/// project and puts it somewhere.
pub const QUESTION: &str = "Start a new project?";
/// S3's first paragraph — the second depends on what is open now ([`confirm_detail`]).
pub const DETAIL: &str = "The sources, the session context and every prompt edit go back to empty. \
Files already written to the output folder are left alone.";
/// The ellipsis again: Cancel keeps everything, Start new… still has to name a folder.
pub const BUTTON: &str = "Start new\u{2026}";
/// S4's title, on the dialog that asks where the new project goes.
pub const TITLE: &str = "New project";

/// What ＋ New does next: refuse, ask first, or go straight to naming the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Refused { reason: &'static str },
    Confirm { detail: String },
    Name,
}

/// S1 → S2 → S3, in that precedence. A run outranks everything else, and an empty session has
/// nothing to lose so it is not asked (S2).
///
/// Nothing here writes: Cancel is simply not pressing on, so the question has no effect to undo —
/// which is the only reason a button one click from Load and Save is safe.
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

/// S3's body: what goes back to empty, plus what happens to the project that is open now. Named or
/// working copy — one of the two sentences always applies, because "there is nothing to come back
/// to" is only true of a session nobody has saved under a name yet.
pub fn confirm_detail(open: &Path) -> String {
    let named = open
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name != startup::SESSION_NAME);
    if named {
        let base = open.file_name().and_then(|n| n.to_str()).unwrap_or_default();
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

/// S4: `.naivepost` on the end unless it is already there — compared case-insensitively, because a
/// file manager that spelled it `.Naivepost` would otherwise get `x.Naivepost.naivepost`.
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

/// S4's default name: today's date, with `-2`, `-3` … appended while the folder is taken. A suffix
/// rather than a refusal because the date *is* the name and two sessions on one day are ordinary.
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

/// S5's refusal, naming what is already there so it can be opened instead.
pub fn already_there(base: &str) -> String {
    format!("{base} is a project already \u{2014} open it, or pick another name")
}

/// What S6 did, in the form the window draws: the blank project to show, the status line's sentence,
/// the log's line, and where the source choosers should open next. `Eq` is not derivable while the
/// project holds floats (the frame interval), so `PartialEq` is as far as this goes.
#[derive(Debug, PartialEq)]
pub struct Created {
    pub path: PathBuf,
    pub base: String,
    pub project: project::Project,
    pub status: String,
    pub log: String,
    /// The folder Add should open at, `None` when the project went under the root — where the
    /// defaults already point.
    pub chooser: Option<PathBuf>,
}

/// S5 + S6: refuse a name that is a project already, else write the blank session there.
///
/// Only `naivepost.json` is created; each step makes its own folder when it first writes, and the
/// project that was open keeps every byte it wrote — that is what S3's "files already written to
/// the output folder are left alone" promises, and going back to it is Open, not undo.
///
/// Applying the blank project across the pages is F0.9's apply list, and recording this launch's
/// project for the root is F0.9's `rememberProject`, so nothing here touches `llm.conf`.
pub fn create(root: &Path, dir: &Path, named: &str) -> Result<Created, String> {
    let path = dir.join(with_suffix(named));
    let base = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();

    // `project::load` answers with defaults for a folder that has no file, so "is a project already"
    // has to be the file itself.
    if path.join(project::PROJECT_FILE).is_file() {
        return Err(already_there(&base));
    }

    let blank = project::Project::default();
    project::save(&blank, &path)?;

    // Where the project was put is where its footage almost certainly is, so that is where Add
    // opens — except for a session under the root, whose folders nobody chose.
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
