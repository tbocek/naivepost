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
