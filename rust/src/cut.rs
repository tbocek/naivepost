//! The cut on disk: `cut/cut.json` (spec/01-project-and-files.md §3).
//!
//! Pure data and file format — no UI here, so the record and its rules are testable
//! without a window (spec/00-principles.md §5, directive C).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cut {
    // Always written: an empty cut writes `"segs": []`, and that is what unlocks
    // nothing (§3).
    pub segs: Vec<Seg>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub aspect: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fx: Vec<Fx>,
    // BTreeMap so a save is byte-stable: the autosave rule of §2 compares marshalled bytes, and a
    // HashMap would shuffle keys into a change that is not one.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub shift: BTreeMap<String, f64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub rows: BTreeMap<String, i32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<Lane>,
    #[serde(skip_serializing_if = "is_zero_i32", default)]
    pub nrows: i32,
    /// View only — ignored by the render (05 F2.11).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub folds: Vec<[f64; 2]>,
    /// Legacy whole-cut `sound`: one recording the cut was heard on, before each scene said for
    /// itself.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sound: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seg {
    pub s: f64,
    pub e: f64,
    /// The asset, by the one path rule of §1; empty for footage.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ins: String,
    /// How long a SPLICED insert runs; an overwriting one runs for exactly the footage it replaces,
    /// so it carries none.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub dur: f64,
    /// Playback rate, written only by the render planner (F5.2).
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub rate: f64,
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub ss: f64,
    #[serde(skip_serializing_if = "is_false")]
    pub mute: bool,
    #[serde(skip_serializing_if = "is_zero_i32", default)]
    pub cam: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lane: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub quiet: Vec<String>,
    /// Starts at a Split border, so it is never merged automatically.
    #[serde(skip_serializing_if = "is_false")]
    pub split: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lane {
    pub name: String,
    pub src: String,
    pub at: f64,
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub off: f64,
    pub dur: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fx {
    pub kind: String,
    pub t: f64,
    /// Total length including the fades — the width of the bar (§1).
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub dur: f64,
    /// Seconds faded in at the start; a glide for a zoom, a ramp for a speed, a fade for a text, a
    /// drawing or a volume. 0 either side is a hard cut.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub trans: f64,
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub tout: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ease: String,
    /// zoom: hold the region until the next zoom (true) rather than pull back.
    #[serde(skip_serializing_if = "is_false")]
    pub stay: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cx: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cy: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wf: Option<f64>,
    /// speed: 1 is its own clock, 0 is a stop.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub rate: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub snd: String,
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub gain: f64,
    /// One field serves both a text and a label — the words in one case, the name in the other
    /// (§1's `text / src` row).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// svg: the drawing's file. Its own key, because a path that is sometimes words is a bug
    /// waiting for the first caption that looks like a filename.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub src: String,
    #[serde(skip_serializing_if = "is_zero_i32", default)]
    pub cam: i32,
    /// volume (new): `""` is the whole bed, a lane's name that lane only.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lane: String,
    #[serde(skip_serializing_if = "is_false", default)]
    pub mute: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    Zoom,
    Speed,
    Text,
    Svg,
    Volume,
    Label,
}

impl EffectKind {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "zoom" => Some(Self::Zoom),
            // A stop is a speed whose rate is 0 (§1's column reads "speed/stop"), so it is the same
            // record and not a seventh kind.
            "speed" | "stop" => Some(Self::Speed),
            "text" => Some(Self::Text),
            "svg" => Some(Self::Svg),
            "volume" => Some(Self::Volume),
            "label" => Some(Self::Label),
            _ => None,
        }
    }
}

/// The lower third a caption with no box of its own goes into: 0.8 of the width, 0.16 of the
/// height, centred on x and low on y (06-effects.md §F3.7's default box).
const LOWER_THIRD: (f64, f64) = (0.5, 0.78);

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

impl Cut {
    pub fn row_count(&self, recording_rows: usize) -> usize {
        // A lane is a row of its own while it exists (§F2.9 `⇲ Lane`), so the rows the page draws
        // are the footage rows plus the lanes.
        recording_rows
            .max(self.lanes.len())
            .max(self.nrows.max(0) as usize)
    }

    /// The legacy whole-cut `sound` spread scene by scene (§3), returning the note to show or
    /// `None` when there was nothing to migrate. Taking the key out means a cut that went through
    /// here never writes it again.
    pub fn migrate_sound(&mut self, recordings: &[String]) -> Option<String> {
        let heard = self.sound.take()?;
        if heard.trim().is_empty() || self.segs.is_empty() {
            return None;
        }
        for seg in &mut self.segs {
            if !seg.quiet.is_empty() {
                continue;
            }
            seg.quiet = recordings
                .iter()
                .filter(|lane| *lane != &heard)
                .cloned()
                .collect();
        }
        Some(format!(
            "this cut was heard on {heard} from end to end; that is now said scene by scene, \
             and every scene has been set to silence the other lanes"
        ))
    }

    /// The legacy spellings of `fx` resolved: `view` becomes a zoom, a per-effect
    /// `mute` becomes `snd: "mute"` (§1). Read once on load — none of it is ever written
    /// back. Public because anything that parses segments elsewhere needs the same one
    /// migration; [`load`] runs it.
    pub fn migrate_fx(&mut self) {
        for fx in &mut self.fx {
            fx.migrate();
        }
    }
}

impl Seg {
    pub fn is_insert(&self) -> bool {
        self.s == self.e && self.dur > 0.0
    }

    /// An insert that replaces footage: it runs for exactly the footage it covers, so it carries no
    /// `dur` of its own.
    pub fn is_overwrite_insert(&self) -> bool {
        !self.ins.is_empty() && self.s != self.e
    }

    pub fn hears(&self, lane: &str) -> bool {
        !self.quiet.iter().any(|q| q == lane)
    }

    pub fn lane(&self) -> Option<&str> {
        (!self.lane.is_empty()).then_some(self.lane.as_str())
    }

    /// `copy:<seconds>` is a pasted stretch of the session, not a file.
    pub fn copy_seconds(&self) -> Option<f64> {
        self.ins
            .strip_prefix("copy:")?
            .split('?')
            .next()?
            .parse()
            .ok()
    }

    /// The asset `ins` names, resolved by the one path rule of §1: `project:…` under
    /// the project, a root-relative card under `<root>/assets`, anything else
    /// absolute. `None` for a `copy:` stretch and for footage.
    pub fn insert_asset(&self, root: &Path, project: &Path) -> Option<PathBuf> {
        if self.ins.is_empty() || self.ins.starts_with("copy:") {
            return None;
        }
        let path = self.ins.split('?').next().unwrap_or_default();
        Some(layout::resolve(root, project, path))
    }

    pub fn insert_query(&self) -> Option<Vec<(String, String)>> {
        let (_, query) = self.ins.split_once('?')?;
        Some(
            query
                .split('&')
                .filter(|pair| !pair.is_empty())
                .map(|pair| {
                    let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                    (key.to_string(), value.to_string())
                })
                .collect(),
        )
    }
}

impl Fx {
    pub fn effect_kind(&self) -> Option<EffectKind> {
        EffectKind::parse(&self.kind)
    }

    /// The kind behind a stored kind id — the public door onto the private `parse`, for callers
    /// that hold the id rather than an `Fx` (the effects dropdown's rows carry it as their `tip`).
    pub fn parse_kind(id: &str) -> Option<EffectKind> {
        EffectKind::parse(id)
    }

    /// The window it covers on the timeline: `dur` already includes the fades, so the
    /// bar runs exactly this wide (06-effects.md §1).
    pub fn spans(&self) -> (f64, f64) {
        (self.t, self.t + self.dur)
    }

    /// The recording a volume effect rides: `None` is the whole bed. Same shape as
    /// [`Seg::lane`], because it answers the same question.
    pub fn lane(&self) -> Option<&str> {
        (!self.lane.is_empty()).then_some(self.lane.as_str())
    }

    pub fn has_box(&self) -> bool {
        self.cx.is_some() || self.cy.is_some() || self.wf.is_some() || self.hf.is_some()
    }

    pub fn centre(&self) -> (f64, f64) {
        let default = match self.effect_kind() {
            Some(EffectKind::Text) => LOWER_THIRD,
            _ => (0.5, 0.5),
        };
        (self.cx.unwrap_or(default.0), self.cy.unwrap_or(default.1))
    }

    /// `view` was a region the camera kept: same record, under a name this build calls zoom (§1).
    /// [`load`] runs this on every effect.
    pub fn migrate(&mut self) {
        if self.kind == "view" {
            self.kind = "zoom".to_string();
        }
        // The stop's old silence tick: a bool, because a stop was the only rate with a question
        // about its sound.
        if self.mute {
            self.mute = false;
            if self.snd.is_empty() {
                self.snd = "mute".to_string();
            }
        }
    }
}

/// §3's refusal, byte for byte.
pub const NO_CUT_YET: &str = "no cut yet — build one on the Cut step first";

pub fn load(tree: &layout::Tree) -> Result<Cut, String> {
    let file = tree.cut_json();
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Cut::default()),
        Err(err) => return Err(format!("{}: {err}", file.display())),
    };
    serde_json::from_str::<Cut>(&text)
        .map_err(|err| format!("{}: {err}", file.display()))
        .map(|mut cut| {
            // Legacy spellings are resolved here, once, so nothing downstream has to know `view` or
            // a per-effect `mute` ever existed (§1).
            cut.migrate_fx();
            cut
        })
}

/// The lane names this cut itself knows, in the order it lists them: its own `lanes`, and nothing
/// else.
pub fn lane_names(cut: &Cut) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for lane in &cut.lanes {
        add_lane_name(&mut names, &lane.name);
    }
    names
}

fn add_lane_name(names: &mut Vec<String>, name: &str) {
    if !name.is_empty() && !names.iter().any(|have| have == name) {
        names.push(name.to_string());
    }
}

/// The one function that writes `cut.json` (§3): every change to the cut lands here, so there is no
/// second writer whose shape could disagree with this one.
pub fn save(cut: &Cut, tree: &layout::Tree) -> Result<(), String> {
    let mut migrated = cut.clone();
    let note = migrated.migrate_sound(&lane_names(&migrated));
    let text = serde_json::to_string_pretty(&migrated).map_err(|err| err.to_string())?;
    let file = tree.cut_json();
    let rel = file
        .strip_prefix(tree.dir())
        .unwrap_or(Path::new("cut/cut.json"));
    tree.write_file(rel, text.as_bytes())?;
    if let Some(said) = note {
        crate::ui::window::log_line(&said);
    }
    Ok(())
}

/// §7 (`its existence unlocks Narrate and Produce`): the gate is the file being there, not its
/// contents being good.
pub fn exists(tree: &layout::Tree) -> bool {
    tree.cut_json().is_file()
}

/// §7 (`Suggest replaces the footage half and the effects, keeps inserts`, and an insert is never
/// `dropped by a re-suggest`): the model's answer for the footage, plus every card of the cut it
/// replaces, in time order.
pub fn keep_inserts(suggested: &[Seg], current: &[Seg]) -> Vec<Seg> {
    let inserts: Vec<&Seg> = current.iter().filter(|seg| !seg.ins.is_empty()).collect();
    // An overwriting insert stands in the list in place of those seconds, a sound as well as a
    // picture.
    let mut kept: Vec<Seg> = suggested.to_vec();
    for over in inserts.iter().filter(|seg| seg.is_overwrite_insert()) {
        kept = kept
            .into_iter()
            .flat_map(|seg| outside(seg, over.s, over.e))
            .collect();
    }
    kept.extend(inserts.into_iter().cloned());
    spliced(kept)
}

fn outside(seg: Seg, from: f64, to: f64) -> Vec<Seg> {
    if !seg.ins.is_empty() || seg.e <= from || seg.s >= to {
        return vec![seg];
    }
    let mut parts = Vec::new();
    if seg.s < from {
        parts.push(Seg {
            e: from,
            ..seg.clone()
        });
    }
    if seg.e > to {
        parts.push(Seg { s: to, ..seg });
    }
    parts
}

/// The list in play order, with footage cut open wherever a spliced card lands inside it: the
/// list's order is what the render walks, so a card behind a scene it was placed inside would play
/// after that scene.
pub fn spliced(segs: Vec<Seg>) -> Vec<Seg> {
    let cards: Vec<f64> = segs
        .iter()
        .filter(|seg| seg.is_insert())
        .map(|seg| seg.s)
        .collect();
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len() + cards.len());
    for seg in segs {
        let mut rest = seg;
        for at in &cards {
            if rest.ins.is_empty() && rest.s < *at && *at < rest.e {
                out.push(Seg {
                    e: *at,
                    ..rest.clone()
                });
                rest.s = *at;
            }
        }
        out.push(rest);
    }
    // `total_cmp` so a fold's negative zero and a plain 0.0 cannot order two ways in one save.
    out.sort_by(|a, b| a.s.total_cmp(&b.s).then(a.e.total_cmp(&b.e)));
    out
}

// --- The edit history (§2's undo snapshots and base) ---------------------------------------------

pub const UNDO_DEPTH: usize = 50;

/// One state the cut has been in: the seven editable things of §3, and nothing else.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub segs: Vec<Seg>,
    pub fx: Vec<Fx>,
    pub aspect: String,
    pub shift: BTreeMap<String, f64>,
    pub rows: BTreeMap<String, i32>,
    pub lanes: Vec<Lane>,
    pub nrows: i32,
}

impl Snapshot {
    pub fn of(cut: &Cut) -> Snapshot {
        Snapshot {
            segs: cut.segs.clone(),
            fx: cut.fx.clone(),
            aspect: cut.aspect.clone(),
            shift: cut.shift.clone(),
            rows: cut.rows.clone(),
            lanes: cut.lanes.clone(),
            nrows: cut.nrows,
        }
    }

    pub fn restore(&self, cut: &mut Cut) {
        cut.segs = self.segs.clone();
        cut.fx = self.fx.clone();
        cut.aspect = self.aspect.clone();
        cut.shift = self.shift.clone();
        cut.rows = self.rows.clone();
        cut.lanes = self.lanes.clone();
        cut.nrows = self.nrows;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct History {
    snapshots: Vec<Snapshot>,
    at: usize,
    base: Snapshot,
}

impl History {
    /// Open the page on a cut: what it was loaded with is both the first state and the base, since
    /// §2 makes the base "the last suggestion or what the page opened with".
    pub fn open(cut: &Cut) -> History {
        let now = Snapshot::of(cut);
        History {
            snapshots: vec![now.clone()],
            at: 0,
            base: now,
        }
    }

    /// F2.13 S4 (`only a real change reaches the stack`): record the cut if it actually differs
    /// from the state on screen, and say whether it did.
    pub fn note_change(&mut self, cut: &Cut) -> bool {
        let now = Snapshot::of(cut);
        if self.snapshots.get(self.at) == Some(&now) {
            return false;
        }
        self.push_snapshot(now);
        true
    }

    /// The present step becomes `cut`, without a new step: a drag spends ONE step on its first move
    /// (§7's "one Undo per drag") and every later move of the same hold lands here, so Undo goes
    /// back to before the drag and Redo comes back to where the hand let go.
    pub fn amend(&mut self, cut: &Cut) {
        let now = Snapshot::of(cut);
        match self.snapshots.get_mut(self.at) {
            Some(present) => *present = now,
            None => self.push_snapshot(now),
        }
    }

    /// The body of [`History::push`], working from an already-taken snapshot so `note_change` can
    /// compare the one copy it made instead of hashing the cut twice.
    pub fn push(&mut self, cut: &Cut) {
        self.push_snapshot(Snapshot::of(cut));
    }

    /// [`History::push`]'s body on an already-taken snapshot, so `note_change` compares the one
    /// copy it made instead of snapshotting the cut twice.
    fn push_snapshot(&mut self, now: Snapshot) {
        self.snapshots.truncate(self.at + 1);
        self.snapshots.push(now);
        if self.snapshots.len() > UNDO_DEPTH + 1 {
            self.snapshots.remove(0);
        }
        self.at = self.snapshots.len() - 1;
    }

    /// The state before this one, or `None` at the bottom — F2.13 words that refusal, this is the
    /// fact it is a refusal about.
    pub fn undo(&mut self) -> Option<Snapshot> {
        if self.at == 0 {
            return None;
        }
        self.at -= 1;
        Some(self.snapshots[self.at].clone())
    }

    /// The state after this one, or `None` at the top.
    pub fn redo(&mut self) -> Option<Snapshot> {
        if self.at + 1 >= self.snapshots.len() {
            return None;
        }
        self.at += 1;
        Some(self.snapshots[self.at].clone())
    }

    pub fn can_undo(&self) -> bool {
        self.at > 0
    }

    pub fn can_redo(&self) -> bool {
        self.at + 1 < self.snapshots.len()
    }

    /// A suggestion is the new base: what Revert goes back to is now the model's answer, not
    /// whatever was on disk an hour ago (§2).
    pub fn suggested(&mut self, cut: &Cut) {
        self.base = Snapshot::of(cut);
    }

    pub fn revert(&mut self) -> Snapshot {
        self.push_snapshot(self.base.clone());
        self.base.clone()
    }

    /// Whether Revert has anything to do (§A's "nothing to revert — the cut is as it was"; the
    /// sentence itself is F2.13's).
    pub fn base_is_the_screen(&self, cut: &Cut) -> bool {
        self.base == Snapshot::of(cut)
    }

    pub fn depth(&self) -> usize {
        self.snapshots.len()
    }

    pub fn current(&self) -> Snapshot {
        self.snapshots[self.at].clone()
    }
}

// --- F2.13: the four sentences (spec/05-cut.md F2.13) -------------------------------------------------
//
// They live here, beside the rules they describe, so the widget layer prints what a rule returned instead
// of composing a sentence of its own (spec/00-principles.md §5). Each one is the whole line the status bar
// shows for that answer; nothing in `ui` adds to them.

/// F2.13 S1 (`walk the snapshots, depth 50 · "undone — N segment(s) left"`): the line both ↶ Undo and
/// ↷ Redo print when they moved. One sentence for both directions because the spec gives one: what a person
/// needs to know after either press is how much of the cut is on screen now, not which way the pointer went.
/// `left` is the segments of the state arrived at. // P.layout.undoDepth 50 is how far back either walk goes.
pub fn undone(left: usize) -> String {
    format!("undone \u{2014} {} segment(s) left", left)
}

/// F2.13 S1: the refusal at the bottom of the stack.
pub const NOTHING_TO_UNDO: &str =
    "nothing to undo \u{2014} you are at the state this page opened with";

/// F2.13 S2 (`greyed · "nothing to revert — the cut is as it was"`): Revert with nothing changed since
/// the base. Same string greys the button and answers a press that got through anyway.
pub const NOTHING_TO_REVERT: &str = "nothing to revert \u{2014} the cut is as it was";

/// F2.13 S2: the two readings of a completed Revert, split on whether the base holds anything.
pub fn reverted(base_segs: usize, hand_made: usize) -> String {
    if base_segs == 0 {
        format!(
            "reverted \u{2014} {} hand-made segment(s) gone, the cut is empty",
            hand_made
        )
    } else {
        format!(
            "reverted to the {} segment(s) of the last suggestion (\u{21b6} Undo brings your edits back)",
            base_segs
        )
    }
}

/// F2.13 S3 (`"nothing to clear — the timeline holds no cut yet"`): Clear before any cut exists. The
/// sentence names the timeline rather than the button's target, because at this point there is no target.
pub const NOTHING_TO_CLEAR: &str = "nothing to clear \u{2014} the timeline holds no cut yet";

/// F2.13 S3 (`"cleared N scene(s) and M effect(s)"`): what one Clear took off. Recordings, rows,
/// shifts and lanes are NOT mentioned because they stay (see [`cleared`]).
pub fn cleared_message(scenes: usize, effects: usize) -> String {
    format!("cleared {} scene(s) and {} effect(s)", scenes, effects)
}

/// The cut with every kept stretch and every effect taken off it, leaving the recordings as they
/// were loaded (§A's Clear).
pub fn cleared(cut: &Cut) -> Cut {
    Cut {
        segs: Vec::new(),
        fx: Vec::new(),
        ..cut.clone()
    }
}
