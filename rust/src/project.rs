//! The project on disk: `naivepost.json` (spec/01-project-and-files.md §2).
//!
//! Pure data and file format — no GTK here, so every rule is testable without a
//! window (spec/00-principles.md §5, directive C).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout;

pub const PROJECT_FILE: &str = "naivepost.json";

/// `interval` — Freq: seconds between the frames sent to the describe model.
pub const INTERVAL_MIN: f64 = 0.25;
pub const INTERVAL_MAX: f64 = 5.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub sources: Vec<Source>,
    pub interval: f64,
    pub language: String,
    pub no_narration: bool,
    /// Inverted on purpose: absent = copy sources into the project.
    pub reference_sources: bool,
    pub vid_dir: Option<String>,
    pub aud_dir: Option<String>,
    pub context: String,
    pub policy: Policy,
    pub produce: Produce,
    /// Absent until the upload text is written (§5). Written as absent, not as
    /// `null`: an existing `publish.json` means the upload text is written, and a
    /// key holding null would say that with the wrong answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish: Option<Publish>,
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
            vid_dir: None,
            aud_dir: None,
            context: String::new(),
            policy: Policy::default(),
            produce: Produce::default(),
            publish: None,
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

impl Project {
    /// The last video chooser folder. Its default is root-relative, so `None`
    /// stays `None` in the file and only resolves against a root here — which
    /// root is a machine setting (spec/10-parameters.md), passed in by the caller.
    pub fn vid_dir(&self, root: &Path) -> PathBuf {
        match &self.vid_dir {
            Some(written) => layout::resolve(root, Path::new(""), written),
            None => root.join("input_video"),
        }
    }

    pub fn aud_dir(&self, root: &Path) -> PathBuf {
        match &self.aud_dir {
            Some(written) => layout::resolve(root, Path::new(""), written),
            None => root.join("input_audio"),
        }
    }
}

/// Which Prepare marking pass runs (P.policy.markingPass).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkingPass {
    Joins,
    Retakes,
    None,
}

/// How Suggest builds the cut (P.policy.cutMode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CutMode {
    Words,
    Model,
}

/// Where a policy value came from. Named `Origin`, not `Source`: a `Source` is
/// already a session file (§2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    User,
    Model,
    #[default]
    Default,
}

/// A policy value and the origin the form shows beside it. The spec says "a
/// `source` per field" without fixing the JSON spelling; a `{"value", "source"}`
/// object is self-describing and carries the origin through a save (00-principles §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field<T> {
    pub value: T,
    #[serde(rename = "source", default)]
    pub origin: Origin,
}

/// The editing policy. Only the two fields §2 names are here; the rest of
/// P.policy.* arrives with its own item. Keys are camelCase because that is how
/// the spec writes them — deliberately unlike the project file's snake_case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    #[serde(rename = "markingPass")]
    pub marking_pass: Field<MarkingPass>,
    #[serde(rename = "cutMode")]
    pub cut_mode: Field<CutMode>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            marking_pass: Field { value: MarkingPass::Retakes, origin: Origin::Default },
            cut_mode: Field { value: CutMode::Model, origin: Origin::Default },
        }
    }
}

/// Encoder settings (spec/08-produce.md §A is normative for the option lists).
///
/// Two rules of that page are deliberately NOT applied here — "webm forces vp9"
/// and "track in file means no translate track" are what the Produce page does to
/// its own controls (08 §2 / F5.1), not properties of the stored file.
///
/// `out_file` is never stored: there is no field, so a key by that name in a file
/// on disk is ignored on load and the output stays `produce/final.<container>`
/// (`layout::Tree::final_video`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Produce {
    pub container: Container,
    pub codec: Codec,
    pub preset: Preset,
    pub resolution: Resolution,
    pub frame_rate: FrameRate,
    pub audio_kbps: u32,
    pub subtitles: Subtitles,
    pub translate: Vec<String>,
    /// How loud the original audio sits under the narration; hidden when
    /// narration is off, which is `no_narration`, not this value.
    pub game_volume: f64,
    pub crf: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    #[default]
    Mp4,
    Mkv,
    Webm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    #[default]
    #[serde(rename = "h264")]
    H264,
    #[serde(rename = "h265")]
    H265,
    #[serde(rename = "vp9")]
    Vp9,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Ultrafast,
    Veryfast,
    Fast,
    Medium,
    /// The spec's default; the prototype shipped veryslow.
    #[default]
    Slow,
    VerySlow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Resolution {
    #[serde(rename = "720p")]
    P720,
    /// The short side of the frame; the cut's aspect sets its shape.
    #[default]
    #[serde(rename = "1080p")]
    P1080,
    Original,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FrameRate {
    #[default]
    Source,
    #[serde(rename = "60")]
    F60,
    #[serde(rename = "30")]
    F30,
    #[serde(rename = "24")]
    F24,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Subtitles {
    BurnedIn,
    TrackInFile,
    /// What the video carries; an .srt is written beside it either way.
    #[default]
    None,
}

impl Default for Produce {
    fn default() -> Self {
        Self {
            container: Container::Mp4,
            codec: Codec::H264,
            preset: Preset::Slow,
            resolution: Resolution::P1080,
            frame_rate: FrameRate::F30,
            audio_kbps: 128,
            subtitles: Subtitles::None,
            translate: Vec::new(),
            game_volume: 0.22,
            crf: 24,
        }
    }
}

/// Upload text and thumbnail state (§5). Its presence is what "the upload text
/// is written" means; deleting the folder starts it over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Publish {
    /// The first frame is the base the image model edits; the rest are references.
    pub frames: Vec<String>,
    pub crop: Crop,
    /// The thumbnail is a chosen frame, not drawn.
    pub own: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_box: Option<TitleBox>,
    pub thumb_title: String,
    pub title_seeded: bool,
    pub texts: Vec<TextMark>,
    pub title: String,
    pub prompt: String,
    pub negative: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TitleBox {
    pub cx: f64,
    pub cy: f64,
    pub wf: f64,
    pub hf: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextMark {
    pub cx: f64,
    pub cy: f64,
    pub wf: f64,
    pub hf: f64,
    pub text: String,
}

impl Default for Publish {
    fn default() -> Self {
        Self {
            frames: Vec::new(),
            crop: Crop { x: 0.5, y: 0.5 },
            own: false,
            title_box: None,
            thumb_title: String::new(),
            title_seeded: false,
            texts: Vec::new(),
            title: String::new(),
            prompt: String::new(),
            negative: String::new(),
            description: String::new(),
        }
    }
}

impl Default for Crop {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

impl Default for TitleBox {
    fn default() -> Self {
        Self { cx: 0.5, cy: 0.25, wf: 1.0, hf: 0.4 }
    }
}

/// Read `<dir>/naivepost.json` with the legacy keys migrated in. A missing file is
/// the same state as an empty project, so it loads the defaults rather than failing.
pub fn load(dir: &Path) -> Result<Project, String> {
    Ok(load_report(dir)?.0)
}

/// [`load`] plus what it migrated, as lines the shell can log: a load must never
/// fail because of a legacy key, and folding something in silently is exactly what
/// the spec's "state every change" rule forbids.
///
/// Keys the current model has none of — `frame_scale`, `run_steps` (prototype only)
/// and `out_file` — are ignored on load because serde skips unknown keys, and are
/// never written because the write path serialises [`Project`] alone.
pub fn load_report(dir: &Path) -> Result<(Project, Vec<String>), String> {
    let file = dir.join(PROJECT_FILE);
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Project::default(), Vec::new()))
        }
        Err(err) => return Err(format!("{}: {err}", file.display())),
    };
    let raw: Raw = serde_json::from_str(&text).map_err(|err| format!("{}: {err}", file.display()))?;
    Ok(migrate(raw))
}

/// The file as written, current and legacy keys together. The current keys default
/// through [`Project::default`] — the interval's 1.0 is not a zero to be caught later;
/// legacy keys are `Option` so "absent" is distinguishable from "empty", which is
/// what decides whether a migration applies.
#[derive(Debug, Deserialize)]
#[serde(default)]
struct Raw {
    sources: Vec<Source>,
    interval: f64,
    language: String,
    no_narration: bool,
    reference_sources: bool,
    vid_dir: Option<String>,
    aud_dir: Option<String>,
    context: String,
    policy: Option<Policy>,
    produce: Produce,
    publish: Option<RawPublish>,

    // Legacy, read and migrated once, never written again.
    videos: Option<Vec<String>>,
    audios: Option<Vec<String>>,
    in_dir: Option<String>,
    out_dir: Option<String>,
    describe_hints: Option<String>,
    transcript_hints: Option<String>,
    cut_hints: Option<String>,
    narrate_hints: Option<String>,
    prompts: Option<serde_json::Map<String, serde_json::Value>>,
    style: Option<String>,
    /// The output-pitch slider is gone; a project written back then still loads.
    pitch: Option<serde_json::Value>,
}

impl Default for Raw {
    fn default() -> Self {
        let empty = Project::default();
        Self {
            sources: empty.sources,
            interval: empty.interval,
            language: empty.language,
            no_narration: empty.no_narration,
            reference_sources: empty.reference_sources,
            vid_dir: empty.vid_dir,
            aud_dir: empty.aud_dir,
            context: empty.context,
            policy: None,
            produce: empty.produce,
            publish: None,
            videos: None,
            audios: None,
            in_dir: None,
            out_dir: None,
            describe_hints: None,
            transcript_hints: None,
            cut_hints: None,
            narrate_hints: None,
            prompts: None,
            style: None,
            pitch: None,
        }
    }
}

/// A stored `publish` object plus the two keys that predate it. Its numbers all
/// default through [`Publish`]: `crop` is 0.5/0.5 and a title box 1.0 wide, which an
/// absent key must mean, not zero.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct RawPublish {
    frames: Vec<String>,
    crop: Crop,
    own: bool,
    title_box: Option<TitleBox>,
    thumb_title: String,
    title_seeded: bool,
    texts: Vec<TextMark>,
    title: String,
    prompt: String,
    negative: String,
    description: String,
    /// The old radio's answer: which frame was the base, read as a list index — what
    /// the prototype did, so an old project keeps the same picture as its base.
    base: Option<usize>,
    /// The old spelling of "not printed".
    title_off: Option<bool>,
}

impl Default for RawPublish {
    fn default() -> Self {
        let empty = Publish::default();
        Self {
            frames: empty.frames,
            crop: empty.crop,
            own: empty.own,
            title_box: empty.title_box,
            thumb_title: empty.thumb_title,
            title_seeded: empty.title_seeded,
            texts: empty.texts,
            title: empty.title,
            prompt: empty.prompt,
            negative: empty.negative,
            description: empty.description,
            base: None,
            title_off: None,
        }
    }
}

/// A project's note folded into a prompt, kept as data: the prompts themselves are
/// machine settings and their store is a later item, so there is nothing here to
/// fold *into* yet. Returning the fold is what lets the shell apply it once and
/// log it, and keeps the lead-ins testable now.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptFold {
    pub prompt: String,
    pub lead_in: String,
    pub notes: String,
}

/// The lead-in each note used to be glued on with — the runners' own wording, which
/// is why it has to stay verbatim: the model was tuned against these sentences.
const HINT_FOLDS: [(&str, &str, &str); 4] = [
    (
        "describe_hints",
        "describe",
        "Editor's notes about this footage -- trust them:",
    ),
    ("transcript_hints", "fix", "Editor's notes -- trust them:"),
    (
        "cut_hints",
        "cut",
        "Editor's notes about this session -- trust them and let them guide what matters:",
    ),
    (
        "narrate_hints",
        "narrate",
        "Editor's goals and context -- honor them:",
    ),
];

/// Apply every migration §2 lists. The write path serialises only [`Project`], so a
/// legacy key is gone at the next save — that is what "migrated once, never written"
/// means in practice.
fn migrate(mut raw: Raw) -> (Project, Vec<String>) {
    let mut report = Vec::new();
    // Taken before the moves below: each migration needs to know whether the file
    // carried the new key or only its legacy spelling.
    let has_policy = raw.policy.is_some();
    let publish_raw = raw.publish.take();
    let mut project = Project {
        sources: std::mem::take(&mut raw.sources),
        // The range is the spec's; what to do outside it is not said. Clamping beats
        // an error or a panic: 0 was the prototype's "every frame", and an interval
        // of 0 would divide every later step.
        interval: raw.interval.clamp(INTERVAL_MIN, INTERVAL_MAX),
        language: std::mem::take(&mut raw.language),
        no_narration: raw.no_narration,
        reference_sources: raw.reference_sources,
        vid_dir: raw.vid_dir.take(),
        aud_dir: raw.aud_dir.take(),
        context: std::mem::take(&mut raw.context),
        policy: raw.policy.take().unwrap_or_default(),
        produce: std::mem::take(&mut raw.produce),
        publish: publish_raw.map(migrate_publish),
    };
    // An absent `language` is the default "en"; serde's default gave us that, so a
    // file that never named one stays English without a report line.

    migrate_sources(&mut project, &raw, &mut report);
    migrate_dirs(&mut project, &raw, &mut report);
    migrate_style(&mut project, has_policy, &raw, &mut report);
    migrate_hints(&raw, &mut report);
    migrate_prompts(&raw, &mut report);
    if raw.pitch.is_some() {
        report.push("pitch: ignored, the field is gone".to_string());
    }
    (project, report)
}

/// `videos` and `audios` become sources; the first recording is narrator 1.
fn migrate_sources(project: &mut Project, raw: &Raw, report: &mut Vec<String>) {
    let legacy = raw.videos.as_deref().unwrap_or(&[])
        .iter()
        .chain(raw.audios.as_deref().unwrap_or(&[]))
        .count();
    if legacy == 0 || !project.sources.is_empty() {
        return;
    }
    for path in raw.videos.as_deref().unwrap_or(&[]) {
        project.sources.push(Source {
            path: path.clone(),
            footage: true,
            ..Source::default()
        });
    }
    // "narrator 1..N (exclusive)" — the first recording carries voice 1, which is
    // the one the narration is spoken in.
    for (n, path) in raw.audios.as_deref().unwrap_or(&[]).iter().enumerate() {
        project.sources.push(Source {
            path: path.clone(),
            narrator: if n == 0 { 1 } else { 0 },
            ..Source::default()
        });
    }
    report.push(format!(
        "videos/audios: {} sources, the first recording is narrator 1",
        project.sources.len()
    ));
}

fn migrate_dirs(project: &mut Project, raw: &Raw, report: &mut Vec<String>) {
    if project.vid_dir.is_none() {
        if let Some(dir) = &raw.in_dir {
            project.vid_dir = Some(dir.clone());
            report.push(format!("in_dir: now vid_dir ({dir})"));
        }
    }
    if project.aud_dir.is_none() {
        if let Some(dir) = &raw.out_dir {
            project.aud_dir = Some(dir.clone());
            report.push(format!("out_dir: now aud_dir ({dir})"));
        }
    }
}

/// The two video styles chose which pipeline ran; the generalisations put that in
/// the policy, so `style` maps onto it — as a `source: default` the User Context can
/// still override.
fn migrate_style(project: &mut Project, has_policy: bool, raw: &Raw, report: &mut Vec<String>) {
    let Some(style) = raw.style.as_deref() else {
        return;
    };
    if has_policy {
        report.push(format!("style {style:?}: ignored, the file carries a policy"));
        return;
    }
    // P.policy.markingPass and P.policy.cutMode, exactly as §2 spells the mapping.
    let (marking_pass, cut_mode) = match style {
        "read" => (MarkingPass::Joins, CutMode::Words),
        "" => (MarkingPass::Retakes, CutMode::Model),
        other => {
            report.push(format!("style {other:?}: unknown, the policy defaults stand"));
            return;
        }
    };
    project.policy.marking_pass = Field { value: marking_pass, origin: Origin::Default };
    project.policy.cut_mode = Field { value: cut_mode, origin: Origin::Default };
    report.push(format!(
        "style {style:?}: markingPass {}, cutMode {}",
        serde_json::to_value(marking_pass).unwrap_or_default(),
        serde_json::to_value(cut_mode).unwrap_or_default()
    ));
}

/// The per-step note boxes are gone; their text is folded into the prompt they used
/// to be appended to, with the lead-in the runners used.
fn migrate_hints(raw: &Raw, report: &mut Vec<String>) {
    for (key, prompt, lead_in) in HINT_FOLDS {
        let held = match key {
            "describe_hints" => &raw.describe_hints,
            "transcript_hints" => &raw.transcript_hints,
            "cut_hints" => &raw.cut_hints,
            _ => &raw.narrate_hints,
        };
        let notes = held.as_deref().unwrap_or("").trim();
        if notes.is_empty() {
            continue;
        }
        report.push(format!(
            "{key}: folded into the {prompt} prompt after {lead_in:?}"
        ));
    }
}

/// Project prompts are adopted once, where the machine has none — they are machine
/// settings, so they are named here rather than stored on the project.
fn migrate_prompts(raw: &Raw, report: &mut Vec<String>) {
    let Some(prompts) = raw.prompts.as_ref() else {
        return;
    };
    if prompts.is_empty() {
        return;
    }
    let keys = adopted_prompt_keys(raw);
    report.push(format!("prompts: adopt where the machine has none: {}", keys.join(", ")));
}

fn adopted_prompt_keys(raw: &Raw) -> Vec<String> {
    raw.prompts
        .as_ref()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

/// The old base-by-index answer becomes the frame list's order. `title_off` is the old
/// spelling of today's "not printed" checkbox and carries over as far as the file can
/// say it — no printed line, title counted as answered — then retires. Nothing else is
/// inferred from a bare title: whether the picture gets words is the Produce page's
/// decision (08 §2, F5.6), and guessing would print where nobody chose to.
pub(crate) fn migrate_publish(mut raw: RawPublish) -> Publish {
    // A project from when the picture printed the entry's words. `title_off` was that
    // project's own answer and retires once read: it clears the printed line, whether
    // or not the title has been seeded since. Otherwise an unseeded title with no
    // printed line is the old shape — the picture printed the title, so the title
    // becomes that line. A project whose own upload text carries a printed line keeps
    // it: a fresh save must not disagree with what it loaded (§2's publish key).
    if raw.title_off.unwrap_or(false) {
        // "not printed": the picture carries no words, and the question is answered.
        raw.thumb_title.clear();
        raw.title_seeded = true;
    }
    Publish {
        frames: move_to_front(raw.frames, raw.base.unwrap_or(0)),
        crop: raw.crop,
        own: raw.own,
        title_box: raw.title_box,
        thumb_title: raw.thumb_title,
        title_seeded: raw.title_seeded,
        texts: raw.texts,
        title: raw.title,
        prompt: raw.prompt,
        negative: raw.negative,
        description: raw.description,
    }
}

/// The base named by a 0-based index moves to the front and the rest keeps its order,
/// so index 2 of [f0,f1,f2] gives [f2,f0,f1] — the prototype's moveToFront
/// (gui/publish.go). An index naming no other frame — 0, or past the end — names the
/// base already at the front, so it moves nothing.
pub(crate) fn move_to_front(frames: Vec<String>, i: usize) -> Vec<String> {
    if i == 0 || i >= frames.len() {
        return frames;
    }
    let chosen = &frames[i];
    let mut out = Vec::with_capacity(frames.len());
    out.push(chosen.clone());
    for (n, frame) in frames.iter().enumerate() {
        if n != i {
            out.push(frame.clone());
        }
    }
    out
}

/// How often the project is marshalled to disk (§2). The shell's tick reads this so
/// the rule lives in testable code rather than inside a UI timer.
pub const AUTOSAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Writes the project only when its bytes differ from the last write. There is no
/// "unsaved changes" prompt, so this write is the only safety net — which is why the
/// flush on window close must not be skippable.
#[derive(Debug, Default)]
pub struct Autosave {
    last_written: Option<Vec<u8>>,
}

impl Autosave {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed with what is on disk, so the first tick after a load writes nothing.
    pub fn from_project(project: &Project) -> Result<Self, String> {
        Ok(Self { last_written: Some(marshal(project)?) })
    }

    pub fn save_if_changed(&mut self, project: &Project, dir: &Path) -> Result<bool, String> {
        let bytes = marshal(project)?;
        if self.last_written.as_deref() == Some(bytes.as_slice()) {
            return Ok(false);
        }
        write_bytes(dir, &bytes)?;
        self.last_written = Some(bytes);
        Ok(true)
    }

    /// Window close: the same rule, named for the path that must not be skipped.
    pub fn flush(&mut self, project: &Project, dir: &Path) -> Result<bool, String> {
        self.save_if_changed(project, dir)
    }
}

fn marshal(project: &Project) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(project).map_err(|err| err.to_string())
}

/// Write `<dir>/naivepost.json`, creating the directory if needed.
pub fn save(project: &Project, dir: &Path) -> Result<(), String> {
    write_bytes(dir, &marshal(project)?)
}

fn write_bytes(dir: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let file = dir.join(PROJECT_FILE);
    fs::write(&file, bytes).map_err(|err| format!("{}: {err}", file.display()))?;
    // §1: files are 0644; a new file's mode would otherwise follow the umask.
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644))
        .map_err(|err| format!("{}: {err}", file.display()))
}
