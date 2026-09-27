//! The cut on disk: `cut/cut.json` (spec/01-project-and-files.md §3).
//!
//! Pure data and file format — no UI here, so the record and its rules are testable
//! without a window (spec/00-principles.md §5, directive C).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout;

/// The cut, whole: one shape for reload, the editor's save and the render.
///
/// Everything but `segs` is omitted when empty, which is what keeps an ordinary
/// `cut.json` — a list of footage scenes — the small file it always was.
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
    // BTreeMap so a save is byte-stable: the autosave rule of §2 compares marshalled
    // bytes, and a HashMap would shuffle keys into a change that is not one.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub shift: BTreeMap<String, f64>,
    /// Hand corrections of which row a recording sits on.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub rows: BTreeMap<String, i32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<Lane>,
    /// A floor under the row count. Written only when set, like the prototype's
    /// `omitempty` on it — and `0` is not a floor, so 0 means "say nothing".
    #[serde(skip_serializing_if = "is_zero_i32", default)]
    pub nrows: i32,
    /// View only — ignored by the render (05 F2.11). Nothing that measures the cut
    /// reads them; they say what the page had folded when it was saved.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub folds: Vec<[f64; 2]>,
    /// Legacy whole-cut `sound`: one recording the cut was heard on, before each scene
    /// said for itself. Read by [`Cut::migrate_sound`], and public only so a test can
    /// hand it a file's contents — `save` never writes it (§3).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sound: Option<String>,
}

/// One scene of the cut, or one spliced insert.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seg {
    pub s: f64,
    pub e: f64,
    /// The asset, by the one path rule of §1; empty for footage. May carry
    /// `?key=value&…`, and `copy:<seconds>` is a pasted stretch of the session.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ins: String,
    /// How long a SPLICED insert runs; an overwriting one runs for exactly the
    /// footage it replaces, so it carries none.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub dur: f64,
    /// Playback rate, written only by the render planner (F5.2).
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub rate: f64,
    /// Start inside an inserted sound.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub ss: f64,
    /// Spliced: silent. Overwriting: the footage's own sound stays.
    #[serde(skip_serializing_if = "is_false")]
    pub mute: bool,
    /// The picture row it is shown from.
    #[serde(skip_serializing_if = "is_zero_i32", default)]
    pub cam: i32,
    /// Which recording an overlaid sound replaces; `""` = everything audible.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lane: String,
    /// The lanes this scene does not hear.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub quiet: Vec<String>,
    /// Starts at a Split border, so it is never merged automatically.
    #[serde(skip_serializing_if = "is_false")]
    pub split: bool,
}

/// A window of a file given a row of its own.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lane {
    pub name: String,
    pub src: String,
    /// The session second the row starts at. Always written — §3 names the four keys
    /// a lane always carries; only `off` may be dropped.
    pub at: f64,
    /// The second of the file that lands there.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub off: f64,
    /// And how long it runs. Always written, like `at`.
    pub dur: f64,
}

/// One effect. The field table is [06-effects.md §1](spec/06-effects.md), which §3
/// points at for it: the same record serves all six kinds, and a field means what its
/// kind says it means — `cx` is a fraction of the SOURCE frame on a zoom and of the
/// OUTPUT frame on a text or a drawing.
///
/// `kind` stays a `String` rather than an enum on purpose: one unknown kind in an old
/// file must not fail the load of the whole cut, and `#[serde(other)]` would only
/// cover a string it can match anyway. [`Fx::effect_kind`] is the typed view, and it
/// answers `None` for a kind this build does not know instead of discarding the effect.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fx {
    pub kind: String,
    /// The session second it happens at. Always written, like `kind`: an effect with
    /// no time is not an effect.
    pub t: f64,
    /// Total length including the fades — the width of the bar (§1).
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub dur: f64,
    /// Seconds faded in at the start; a glide for a zoom, a ramp for a speed, a fade
    /// for a text, a drawing or a volume. 0 either side is a hard cut.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub trans: f64,
    /// …and out at the end.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub tout: f64,
    /// The shape of those two fades. `""` IS linear, and stays `""` on a save so an
    /// old file comes back byte-identical (§1); an unknown name survives untouched,
    /// since a curve added in a later build must not be erased by this one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ease: String,
    /// zoom: hold the region until the next zoom (true) rather than pull back.
    #[serde(skip_serializing_if = "is_false")]
    pub stay: bool,
    // The box, four fractions. `Option` because a stored box must be told apart from
    // no box at all: §1 gives each kind its own default, and a 0.0 would read as "the
    // top-left corner, no height" instead of "ask my kind". Only text and svg consult
    // them (§1 marks the other kinds `–`); see [`Fx::centre`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cx: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cy: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf: Option<f64>,
    /// Box width as a fraction of the output frame — text and svg only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wf: Option<f64>,
    /// speed: 1 is its own clock, 0 is a stop.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub rate: f64,
    /// speed: what the sound does — `""` / pitch / own / scene / mute.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub snd: String,
    /// volume: linear gain, 0 is silence.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub gain: f64,
    /// One field serves both a text and a label — the words in one case, the name in
    /// the other (§1's `text / src` row). The prototype does the same, and the kind is
    /// what says which of the two it is; keeping them apart would mean a second key
    /// that only ever one kind may fill.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// svg: the drawing's file. Its own key, because a path that is sometimes words is
    /// a bug waiting for the first caption that looks like a filename.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub src: String,
    /// zoom (new): the camera row it was framed on.
    #[serde(skip_serializing_if = "is_zero_i32", default)]
    pub cam: i32,
    /// volume (new): `""` is the whole bed, a lane's name that lane only.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lane: String,
    /// Legacy speed silence: read and migrated to `snd: "mute"` by [`Fx::migrate`], and
    /// public only so a test can hand it a file's contents — `save` never writes it (§1).
    #[serde(skip_serializing_if = "is_false", default)]
    pub mute: bool,
}

/// The kinds this build knows. A file may name another — an effect from a newer build,
/// or the legacy `view` before [`load`] migrates it — and that must cost the cut
/// nothing but the typed view of that one effect.
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
            // A stop is a speed whose rate is 0 (§1's column reads "speed/stop"), so
            // it is the same record and not a seventh kind.
            "speed" | "stop" => Some(Self::Speed),
            "text" => Some(Self::Text),
            "svg" => Some(Self::Svg),
            "volume" => Some(Self::Volume),
            "label" => Some(Self::Label),
            _ => None,
        }
    }
}

/// The lower third a caption with no box of its own goes into: 0.8 of the width,
/// 0.16 of the height, centred on x and low on y (06-effects.md §F3.7's default box).
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
    /// `nrows` is a floor, not a count: the recordings decide how many rows there are
    /// unless the cut insists on more.
    pub fn row_count(&self, recording_rows: usize) -> usize {
        recording_rows.max(self.nrows.max(0) as usize)
    }

    /// The legacy whole-cut `sound` spread scene by scene (§3), returning the note to
    /// show or `None` when there was nothing to migrate. Taking the key out means a cut
    /// that went through here never writes it again.
    ///
    /// A scene that already names its `quiet` is left alone; every other scene silences
    /// every recording but the one the whole cut was heard on. A `sound` naming no known
    /// recording silences them all — nothing was heard.
    ///
    /// The prototype refines this for a scene shown from another camera than the one
    /// whose sound ran under it (gui/cut_hear.go:562-613 keeps such a scene hearing its
    /// own picture and counts them in the note). That needs the row model of 05 F2.10 —
    /// which recording sits under which `cam` at which second — and is not this file's
    /// rule, so this function takes lane names alone.
    pub fn migrate_sound(&mut self, recordings: &[String]) -> Option<String> {
        let heard = self.sound.take()?;
        if heard.trim().is_empty() || self.segs.is_empty() {
            return None;
        }
        for seg in &mut self.segs {
            if !seg.quiet.is_empty() {
                continue;
            }
            seg.quiet = recordings.iter().filter(|lane| *lane != &heard).cloned().collect();
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
    /// `s == e` with `dur > 0`: material spliced in beside the footage. `s == e` with
    /// no length is not one — it is an empty scene, not a card.
    pub fn is_insert(&self) -> bool {
        self.s == self.e && self.dur > 0.0
    }

    /// An insert that replaces footage: it runs for exactly the footage it covers, so
    /// it carries no `dur` of its own.
    pub fn is_overwrite_insert(&self) -> bool {
        !self.ins.is_empty() && self.s != self.e
    }

    /// Does this scene hear that lane? `quiet` names what it does not; the `lane`
    /// field says what an overlaid sound *replaces*, which is a different question.
    pub fn hears(&self, lane: &str) -> bool {
        !self.quiet.iter().any(|q| q == lane)
    }

    /// The recording an overlaid sound replaces. `None` (absent) and `Some("")` are
    /// the same answer today — everything audible — but they are different bytes, and
    /// a save must not turn one into the other.
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

    /// The `?key=value&…` a card carries — its declared inputs. Values are taken
    /// verbatim: the spec's own example is `?S=Dust II`, where the space belongs to
    /// the value, so decoding percent escapes here would be inventing an encoding the
    /// file format does not have.
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
    /// The typed kind, or `None` for a name this build does not know.
    pub fn effect_kind(&self) -> Option<EffectKind> {
        EffectKind::parse(&self.kind)
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

    /// Does this effect carry a box of its own? Only text and svg ask (§1 marks every
    /// other kind `–`), so that is what "carries" means here.
    pub fn has_box(&self) -> bool {
        self.cx.is_some() || self.cy.is_some() || self.wf.is_some() || self.hf.is_some()
    }

    /// The centre to draw at: the stored one, else this kind's default — a caption with
    /// no box goes in the lower third (§1, and 06-effects.md §F3.7), anything else sits
    /// mid-frame (§1: "svg: middle").
    ///
    /// Deliberately only the centre. §1 names svg's default *position* and no width or
    /// height, and the prototype agrees (`fxSvgDefault` is 0.6×0.6 against text's
    /// 0.8×0.16), so inventing a band here would be this file deciding something it was
    /// not asked to: the size belongs to the svg effect's own item (06 §F3.8). A zoom
    /// has no output-frame default at all — it is framed on the source, where `cx`/`cy`
    /// are fractions of a different frame — so it reports 0.5/0.5 and `has_box()` tells
    /// you whether that is stored or merely nothing to say.
    pub fn centre(&self) -> (f64, f64) {
        let default = match self.effect_kind() {
            Some(EffectKind::Text) => LOWER_THIRD,
            _ => (0.5, 0.5),
        };
        (self.cx.unwrap_or(default.0), self.cy.unwrap_or(default.1))
    }

    /// `view` was a region the camera kept: same record, under a name this build calls
    /// zoom (§1). Any preset name it carried is gone with the name — the region itself
    /// lives in `cx`/`cy`/`hf`, which migrate untouched. A per-effect `mute` becomes
    /// `snd: "mute"`. [`load`] runs this on every effect; it is public because anything
    /// that parses segments elsewhere needs the same one migration.
    pub fn migrate(&mut self) {
        if self.kind == "view" {
            self.kind = "zoom".to_string();
        }
        // The stop's old silence tick: a bool, because a stop was the only rate with a
        // question about its sound. `snd` answers for every rate now (§1).
        if self.mute {
            self.mute = false;
            if self.snd.is_empty() {
                self.snd = "mute".to_string();
            }
        }
    }
}

/// The segments that unlock the Narrate and Produce ▶: the live editor's if it has any,
/// else those parsed from `cut.json`. A file holding `{"segs": []}` unlocks nothing; an
/// unsaved editor holding segments does. The tabs always open — this answers whether ▶
/// runs, and the refusal is the caller's log line (§3).
pub fn cut_segments<'a>(editor: Option<&'a [Seg]>, file: &'a Cut) -> Option<&'a [Seg]> {
    match editor {
        Some(segs) if !segs.is_empty() => Some(segs),
        _ => (!file.segs.is_empty()).then_some(file.segs.as_slice()),
    }
}

/// §3's refusal, byte for byte.
pub const NO_CUT_YET: &str = "no cut yet — build one on the Cut step first";

/// Read `cut/cut.json`. A project before its first cut has no file, which is the same
/// state as an empty cut — nothing was cut yet.
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
            // Legacy spellings are resolved here, once, so nothing downstream has to
            // know `view` or a per-effect `mute` ever existed (§1).
            cut.migrate_fx();
            cut
        })
}

/// The one function that writes `cut.json` (§3): every change to the cut lands here,
/// so there is no second writer whose shape could disagree with this one.
pub fn save(cut: &Cut, tree: &layout::Tree) -> Result<(), String> {
    let text = serde_json::to_string_pretty(cut).map_err(|err| err.to_string())?;
    // The path comes from layout, so there is one spelling of it; only its prefix
    // inside the project is taken back off, which is what lets write_file create the
    // directory and set §1's 0644.
    let file = tree.cut_json();
    let rel = file.strip_prefix(tree.dir()).unwrap_or(Path::new("cut/cut.json"));
    tree.write_file(rel, text.as_bytes())
}

/// §7 (`its existence unlocks Narrate and Produce`): the gate is the file being there, not its contents being
/// good. A project whose first cut has not happened yet has no `cut/cut.json`, and [`load`] reads that as an
/// empty cut — the same state, with nothing said about it. `shell::lock` deliberately keeps Narrate and Produce
/// unlocked (§1: their ▶ refuses instead), so this is what those steps ask before running: a lock would hide the
/// voice that explains itself, which is [`NO_CUT_YET`].
pub fn exists(tree: &layout::Tree) -> bool {
    tree.cut_json().is_file()
}

/// §7 (`Suggest replaces the footage half and the effects, keeps inserts`, and an insert is never
/// `dropped by a re-suggest`): the model's answer for the footage, plus every card of the cut it replaces, in
/// time order. A card is not footage — nobody filmed it, so nothing in a new suggestion can be its second
/// version, and losing the title card of a talk to a re-run would be an edit nobody made.
///
/// Effects go with the footage half: they are placed on scenes, and scenes the model did not choose have
/// nothing for them to sit on. The caller moves the base as well ([`History::suggested`]), since what Revert
/// returns to after a suggestion is the model's answer and not whatever was on disk an hour ago.
pub fn keep_inserts(suggested: &[Seg], current: &[Seg]) -> Vec<Seg> {
    let mut kept: Vec<Seg> = suggested.to_vec();
    kept.extend(current.iter().filter(|seg| !seg.ins.is_empty()).cloned());
    // `total_cmp` so a fold's negative zero and a plain 0.0 cannot order two ways in one save.
    kept.sort_by(|a, b| a.s.total_cmp(&b.s).then(a.e.total_cmp(&b.e)));
    kept
}

// --- The edit history (§2's undo snapshots and base) ---------------------------------------------

/// How many snapshots the history holds. spec/10-parameters.md:133's "undo depth 50"; a snapshot is a
/// whole cut — every segment, effect and hand correction of it — so without a bound one long session
/// of Add and Remove would quietly hold fifty megabytes of superseded timelines.
pub const UNDO_DEPTH: usize = 50;

/// One state the cut has been in: the seven editable things of §3, and nothing else.
///
/// `folds` is deliberately absent — spec/inventory/cut.md §B calls them "a view, not an edit", so
/// undoing an Add must not unfold the stretch the person was watching when they made it.
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
    /// What the cut holds right now, copied out so a later edit cannot reach back into it.
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

    /// Put the seven back. `folds` is left as the target had them: a fold is where the person was
    /// looking, not something they did (§B), and an undo that unfolded it would move the page under
    /// them for a reason no status line could name.
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

/// The undo/redo stack and the state Revert returns to.
///
/// `at` is the index of what is on screen, so undo and redo move an index instead of throwing copies
/// away — which is what lets redo survive as long as the branch it belongs to does.
#[derive(Debug, Clone, PartialEq)]
pub struct History {
    snapshots: Vec<Snapshot>,
    at: usize,
    base: Snapshot,
}

impl History {
    /// Open the page on a cut: what it was loaded with is both the first state and the base, since §2
    /// makes the base "the last suggestion or what the page opened with".
    pub fn open(cut: &Cut) -> History {
        let now = Snapshot::of(cut);
        History { snapshots: vec![now.clone()], at: 0, base: now }
    }

    /// F2.13 S4 (`only a real change reaches the stack`): record the cut if it actually differs from the
    /// state on screen, and say whether it did.
    ///
    /// Every mutating door funnels through here rather than calling [`History::push`] directly, because a
    /// door that writes the page also writes things that are not edits: a fold toggle (F2.11 saves folds
    /// but `Snapshot` deliberately omits them, so Undo must never unfold the page under someone) and the
    /// watched row (not a cut field at all). Testing the difference on the snapshot itself — rather than
    /// trusting the caller to know whether its write mattered — is what keeps those writes off the stack:
    /// an identical snapshot means "nothing happened", so no step is recorded and the buttons stay honest.
    /// Returns whether a snapshot was pushed, which is the same answer the greying needs.
    pub fn note_change(&mut self, cut: &Cut) -> bool {
        let now = Snapshot::of(cut);
        if self.snapshots.get(self.at) == Some(&now) {
            return false;
        }
        self.push_snapshot(now);
        true
    }

    /// The body of [`History::push`], working from an already-taken snapshot so `note_change` can compare
    /// the one copy it made instead of hashing the cut twice.
    ///
    /// Everything after `at` goes first: redoing into a branch the person has already left behind
    /// would silently re-apply something they undid, and §A's "pushUndo clears redo" is that sentence
    /// written as a rule. Then the oldest snapshot is dropped once the stack passes [`UNDO_DEPTH`], so
    /// the bound costs the earliest edits rather than the whole history's usefulness.
    pub fn push(&mut self, cut: &Cut) {
        self.push_snapshot(Snapshot::of(cut));
    }

    /// [`History::push`]'s body on an already-taken snapshot, so `note_change` compares the one copy it
    /// made instead of snapshotting the cut twice.
    fn push_snapshot(&mut self, now: Snapshot) {
        self.snapshots.truncate(self.at + 1);
        self.snapshots.push(now);
        if self.snapshots.len() > UNDO_DEPTH + 1 {
            self.snapshots.remove(0);
        }
        self.at = self.snapshots.len() - 1;
    }

    /// The state before this one, or `None` at the bottom — F2.13 words that refusal, this is the fact
    /// it is a refusal about.
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

    /// A suggestion is the new base: what Revert goes back to is now the model's answer, not whatever
    /// was on disk an hour ago (§2). The undo stack stays where it is, because ↶ after a suggestion
    /// that dropped a run is still wanted — losing one's own work to a suggestion is the failure this
    /// keeps.
    pub fn suggested(&mut self, cut: &Cut) {
        self.base = Snapshot::of(cut);
    }

    /// Back to the base, and the stack rewinds with it so undo cannot walk into the edits Revert just
    /// threw away — they are gone, and a history that still reached them would be a lie about it.
    pub fn revert(&mut self) -> Snapshot {
        self.snapshots = vec![self.base.clone()];
        self.at = 0;
        self.base.clone()
    }

    /// Whether Revert has anything to do (§A's "nothing to revert — the cut is as it was"; the sentence
    /// itself is F2.13's).
    pub fn base_is_the_screen(&self, cut: &Cut) -> bool {
        self.base == Snapshot::of(cut)
    }

    /// How many snapshots are held, which is what [`UNDO_DEPTH`] bounds.
    pub fn depth(&self) -> usize {
        self.snapshots.len()
    }

    /// The state the pointer stands on — what the page is showing, as the stack sees it. `note_change`
    /// compares against this, and a test reads it to check that an identical write left no step behind.
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

/// F2.13 S1: the refusal at the bottom of the stack. The spec words the other three refusals and leaves
/// this one to the flow's shape; the wording kept is the page's own, because "the state this page opened
/// with" says where the floor is rather than only that there is one.
pub const NOTHING_TO_UNDO: &str = "nothing to undo \u{2014} you are at the state this page opened with";

/// F2.13 S2 (`greyed · "nothing to revert — the cut is as it was"`): Revert with nothing changed since
/// the base. Same string greys the button and answers a press that got through anyway.
pub const NOTHING_TO_REVERT: &str = "nothing to revert \u{2014} the cut is as it was";

/// F2.13 S2: the two readings of a completed Revert, split on whether the base holds anything.
///
/// A base with segments in it came from a suggestion (§2: the base is "the last suggestion or what the
/// page opened with"), so the sentence names whose seconds these are and promises the undo path back.
/// An empty base means nothing was ever suggested and the page opened blank: everything that disappears
/// was hand-made, and saying "the cut is empty" is more honest than quoting zero suggestions.
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

/// F2.13 S3 (`"cleared N scene(s) and M effect(s)"`): what one Clear took off. Scenes and effects are
/// counted apart because they are lost apart — a scene is a kept stretch of the recording, an effect is
/// something laid over it — and the numbers are what tells someone whether ✗ Clear was the button they
/// meant to press. Recordings, rows, shifts and lanes are NOT mentioned because they stay (see
/// [`cleared`]).
pub fn cleared_message(scenes: usize, effects: usize) -> String {
    format!("cleared {} scene(s) and {} effect(s)", scenes, effects)
}

/// The cut with every kept stretch and every effect taken off it, leaving the recordings as they were
/// loaded (§A's Clear). Empty segment and effect lists are the whole of it: `shift` (the hand offsets
/// per recording) and `rows` (which row a recording was put on) stay, because Clear takes work off the
/// timeline rather than undoing where a person parked a file — and the rows are what the page is drawn
/// from, so emptying them would blank the tracks rather than clear them.
pub fn cleared(cut: &Cut) -> Cut {
    Cut { segs: Vec::new(), fx: Vec::new(), ..cut.clone() }
}
