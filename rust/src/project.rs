//! The project on disk: `naivepost.json` (spec/01-project-and-files.md §2).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout;

pub const PROJECT_FILE: &str = "naivepost.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Project {
    pub sources: Vec<Source>,
    /// Inverted on purpose: absent = copy sources into the project.
    pub reference_sources: bool,
    pub vid_dir: Option<String>,
    pub aud_dir: Option<String>,
    pub context: String,
    pub policy: Policy,
    pub produce: Produce,
    /// Absent until the upload text is written (§5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish: Option<Publish>,
    /// F0.14: the profile this project's `context` is made from, with this project's own values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProfileUse>,
}

/// A project's use of a profile (§01 §3 `profile`): the profile's name and the values filled in here.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileUse {
    pub name: String,
    pub values: std::collections::BTreeMap<String, String>,
    /// The profile's `## produce` rows as this project last took them (`Row: value` per line).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub produce_rows: String,
    /// The profile's whole text as this project last used it (§03-shell §5b "Carried in the project"): a
    /// project folder copied to another machine brings its profile with it. Written only when there is one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub template: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Source {
    pub path: String,
    pub footage: bool,
    pub narrator: u32,
    pub sepvoice: bool,
    pub tracks: Vec<u32>,
    /// §4's sound button, off: this video's own sound only lines the file up, and ▶ Prepare gives its
    /// copy in `sources/` the sound of the separate recording that ran at the same time (04 F1.15).
    /// Written only when set, so every older project file reads and writes as it did.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub sound_off: bool,
}

/// Every recording is a voice, 1..N: there is no "no voice", so a row starts at voice 1, the one the
/// narration is spoken in (§4).
impl Default for Source {
    fn default() -> Self {
        Self {
            path: String::new(),
            footage: false,
            narrator: 1,
            sepvoice: false,
            tracks: Vec::new(),
            sound_off: false,
        }
    }
}

impl Project {
    /// The last video chooser folder.
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

/// Where a policy value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    User,
    Model,
    #[default]
    Default,
}

/// A policy value and the origin the log says beside it.
fn no_target_field() -> Field<f64> {
    Field {
        value: crate::tools::cutpass::NO_TARGET,
        origin: Origin::Default,
        because: None,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field<T> {
    pub value: T,
    #[serde(rename = "source", default)]
    pub origin: Origin,
    /// F0.7 S3: the reason the model gave when it set this (`tool:set_policy`'s `because`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub because: Option<String>,
}

/// The editing policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    #[serde(rename = "markingPass")]
    pub marking_pass: Field<MarkingPass>,
    #[serde(rename = "cutMode")]
    pub cut_mode: Field<CutMode>,
    /// `P.policy.narrationRewrite`: ▶ on Narrate rewrites the lines that already exist too.
    #[serde(rename = "narrationRewrite")]
    pub narration_rewrite: Field<bool>,
    /// `P.policy.keepSwearing`: the captions pass keeps the speaker's swearing (default).
    #[serde(rename = "keepSwearing")]
    pub keep_swearing: Field<bool>,
    /// `P.policy.captionsPass`: whether F3.9's caption pass is asked at all.
    #[serde(rename = "captionsPass")]
    pub captions_pass: Field<bool>,
    /// `P.policy.speedPass`: whether F3.10's speed pass is asked at all.
    #[serde(rename = "speedPass")]
    pub speed_pass: Field<bool>,
    /// `P.policy.decorationsPass`: whether F3.11's decoration pass is asked at all.
    #[serde(rename = "decorationsPass")]
    pub decorations_pass: Field<bool>,
    /// `P.policy.targetLengthSeconds`: the length the cut aims at, seconds, 0 = none (§10's default).
    #[serde(rename = "targetLengthSeconds", default = "no_target_field")]
    pub target_length_seconds: Field<f64>,
    /// `P.policy.language`: the language the recordings are spoken in, as its code ("en", "de", …): what
    /// the speech recogniser is told and what the voice speaks.
    #[serde(rename = "language", default = "english_field")]
    pub language: Field<String>,
}

/// The language a session is in when nothing says otherwise: English, §10's default.
pub const DEFAULT_LANGUAGE: &str = "en";

/// A `language` field for a project file written before the field moved into the policy, or without it.
fn english_field() -> Field<String> {
    Field {
        value: DEFAULT_LANGUAGE.to_string(),
        origin: Origin::Default,
        because: None,
    }
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            marking_pass: Field {
                value: MarkingPass::Retakes,
                origin: Origin::Default,
                because: None,
            },
            cut_mode: Field {
                value: CutMode::Model,
                origin: Origin::Default,
                because: None,
            },
            narration_rewrite: Field {
                value: false,
                origin: Origin::Default,
                because: None,
            },
            keep_swearing: Field {
                value: true,
                origin: Origin::Default,
                because: None,
            },
            captions_pass: Field {
                value: true,
                origin: Origin::Default,
                because: None,
            },
            speed_pass: Field {
                value: true,
                origin: Origin::Default,
                because: None,
            },
            target_length_seconds: Field {
                value: crate::tools::cutpass::NO_TARGET,
                origin: Origin::Default,
                because: None,
            },
            language: english_field(),
            decorations_pass: Field {
                value: true,
                origin: Origin::Default,
                because: None,
            },
        }
    }
}

/// Encoder settings (spec/08-produce.md §A is normative for the option lists).
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
    /// How loud the original audio sits under the narration (drawn on the Narrate page).
    pub game_volume: f64,
    pub crf: u32,
    /// §A "Channels", the tick `mono`: mix the finished audio down to a single channel.
    pub mono: bool,
    /// §A "Frame edges", the tick `blurred`: fill the empty edges of the frame with a blown-up, blurred
    /// copy of the picture itself instead of black.
    pub blurred_edges: bool,
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
    #[default]
    Slow,
    VerySlow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Resolution {
    #[serde(rename = "720p")]
    P720,
    #[default]
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "original", alias = "Original")]
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
            mono: false,
            blurred_edges: true,
        }
    }
}

/// Upload text and thumbnail state (§5).
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
        Self {
            cx: 0.5,
            cy: 0.25,
            wf: 1.0,
            hf: 0.4,
        }
    }
}

/// Read `<dir>/naivepost.json` with the legacy keys migrated in.
pub fn load(dir: &Path) -> Result<Project, String> {
    Ok(load_report(dir)?.0)
}

/// [`load`] plus what it migrated, as lines the shell can log: a load must never
/// fail because of a legacy key, and folding something in silently is exactly what
/// the spec's "state every change" rule forbids.
pub fn load_report(dir: &Path) -> Result<(Project, Vec<String>), String> {
    let file = dir.join(PROJECT_FILE);
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Project::default(), Vec::new()))
        }
        Err(err) => return Err(format!("{}: {err}", file.display())),
    };
    // §6's two stored-word fallbacks first: they are what keeps a load working when one row's word is stale,
    // so they must be in hand before anything can fail on that word.
    let stored = crate::project_produce::read(produce_block(&text).as_ref());
    // Two readings of one block, each owning what it can.
    let parsed = serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|err| format!("{}: {err}", file.display()))
        .and_then(|value| {
            serde_json::from_value::<Raw>(alias_produce(value))
                .map_err(|err| format!("{}: {err}", file.display()))
        });
    match parsed {
        Ok(raw) => Ok(migrate(raw, dir, stored)),
        // Only a `produce`-shaped failure may be carried this way: anything else in the file is still the
        // person's problem to hear about, spelled exactly as it was before this fallback existed.
        Err(why) if why.contains("variant") || why.contains("invalid value") => {
            let mut report = stored.report.clone();
            report.push(format!(
                "produce: read row by row ({why}) \u{2014} a row this app does not offer keeps its default"
            ));
            Ok((
                Project {
                    produce: stored.settings,
                    ..Project::default()
                },
                report,
            ))
        }
        Err(why) => Err(why),
    }
}

/// The file's `produce` object, re-read from the text.
pub(crate) fn produce_block(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()?
        .as_object()?
        .get("produce")
        .cloned()
}

/// The alias key [`Raw::produce_legacy`] reads the `produce` object under.
pub(crate) const PRODUCE_ALIAS: &str = "$produce_alias";

/// Copy the stored `produce` object to [`PRODUCE_ALIAS`] so two fields can read it: serde gives one key to one
/// field, and the legacy half of this block would otherwise be dropped as an unknown key while `Produce` took the
/// rest.
fn alias_produce(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(map) = value.as_object_mut() {
        if let Some(block) = map.get("produce").cloned() {
            map.insert(PRODUCE_ALIAS.to_string(), block);
        }
    }
    value
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Raw {
    sources: Vec<Source>,
    /// The project file's top-level `language`, from before it moved into the policy: read once, never
    /// written ([`migrate_language`]).
    language: Option<String>,
    reference_sources: bool,
    vid_dir: Option<String>,
    aud_dir: Option<String>,
    context: String,
    policy: Option<Policy>,
    produce: Produce,
    // The stored `produce` block read a SECOND time, under its own alias key (see [`alias_produce`], which
    // copies the object there before `Raw` is deserialized).
    #[serde(
        rename = "$produce_alias",
        default,
        deserialize_with = "crate::project_produce::from_object"
    )]
    produce_legacy: crate::project_produce::RawProduce,
    publish: Option<RawPublish>,
    profile: Option<ProfileUse>,

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
    pitch: Option<serde_json::Value>,
}

impl Default for Raw {
    fn default() -> Self {
        let empty = Project::default();
        Self {
            sources: empty.sources,
            language: None,
            reference_sources: empty.reference_sources,
            vid_dir: empty.vid_dir,
            aud_dir: empty.aud_dir,
            context: empty.context,
            policy: None,
            produce: empty.produce,
            produce_legacy: crate::project_produce::RawProduce::default(),
            publish: None,
            profile: None,
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

/// A stored `publish` object plus the two keys that predate it.
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
    base: Option<usize>,
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
/// fold *into* yet.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptFold {
    pub prompt: String,
    pub lead_in: String,
    pub notes: String,
}

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

/// Apply every migration §2 lists.
fn migrate(
    mut raw: Raw,
    dir: &Path,
    mut stored: crate::project_produce::Stored,
) -> (Project, Vec<String>) {
    let mut report = Vec::new();
    // Taken before the moves below: each migration needs to know whether the file
    // carried the new key or only its legacy spelling.
    let has_policy = raw.policy.is_some();
    let publish_raw = raw.publish.take();
    let mut project = Project {
        sources: std::mem::take(&mut raw.sources),
        reference_sources: raw.reference_sources,
        vid_dir: raw.vid_dir.take(),
        aud_dir: raw.aud_dir.take(),
        context: std::mem::take(&mut raw.context),
        policy: raw.policy.take().unwrap_or_default(),
        produce: std::mem::take(&mut stored.settings),
        publish: publish_raw.map(migrate_publish),
        profile: raw.profile.take(),
    };
    migrate_language(&mut project, &raw, &mut report);
    migrate_sources(&mut project, &raw, &mut report);
    migrate_dirs(&mut project, &raw, dir, &mut report);
    migrate_style(&mut project, has_policy, &raw, &mut report);
    migrate_hints(&raw, &mut report);
    // §08 §6's legacy `produce` keys, folded into the settings the current model holds and reported there.
    report.extend(migrate_produce(&mut project.produce, &raw.produce_legacy));
    // And §6's two stored-word fallbacks, said in the same voice as every other fold.
    report.extend(stored.report);
    migrate_prompts(&raw, &mut report);
    if raw.pitch.is_some() {
        report.push("pitch: ignored, the field is gone".to_string());
    }
    (project, report)
}

/// The stored `produce` block's legacy keys, read through a raw view of the same object `Produce` was
/// deserialized from and folded by [`crate::project_produce::fold_legacy`].
fn migrate_produce(
    settings: &mut Produce,
    legacy: &crate::project_produce::RawProduce,
) -> Vec<String> {
    if legacy.is_empty() {
        return Vec::new();
    }
    crate::project_produce::fold_legacy(settings, legacy)
}

fn migrate_sources(project: &mut Project, raw: &Raw, report: &mut Vec<String>) {
    let legacy = raw
        .videos
        .as_deref()
        .unwrap_or(&[])
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
    for path in raw.audios.as_deref().unwrap_or(&[]) {
        project.sources.push(Source {
            path: path.clone(),
            ..Source::default()
        });
    }
    report.push(format!(
        "videos/audios: {} sources, every recording is voice 1",
        project.sources.len()
    ));
}

pub fn out_dir_note(from: &str, to: &str) -> String {
    format!("!!! this project used to write into {from} and now writes into {to} -- the old folder is untouched")
}

/// Whether a stored `out_dir` means a folder other than the project itself.
fn writes_elsewhere(stored: &str, dir: &Path) -> bool {
    let stored = stored.trim_end_matches('/');
    if stored.is_empty() || stored == "." {
        return false;
    }
    let name = dir.file_name().and_then(|name| name.to_str()).unwrap_or("");
    !stored.is_empty() && stored != name && Path::new(stored) != dir
}

fn migrate_dirs(project: &mut Project, raw: &Raw, dir: &Path, report: &mut Vec<String>) {
    if project.vid_dir.is_none() {
        if let Some(dir) = &raw.in_dir {
            project.vid_dir = Some(dir.clone());
            report.push(format!("in_dir: now vid_dir ({dir})"));
        }
    }
    if project.aud_dir.is_none() {
        if let Some(stored) = &raw.out_dir {
            project.aud_dir = Some(stored.clone());
            report.push(format!("out_dir: now aud_dir ({stored})"));
            if writes_elsewhere(stored, dir) {
                report.push(out_dir_note(stored, &dir.display().to_string()));
            }
        }
    }
}

fn migrate_language(project: &mut Project, raw: &Raw, report: &mut Vec<String>) {
    let Some(code) = raw
        .language
        .as_deref()
        .map(str::trim)
        .filter(|code| !code.is_empty())
    else {
        return;
    };
    if project.policy.language.origin != Origin::Default {
        return;
    }
    if code != project.policy.language.value {
        report.push(format!(
            ">>> language {code} moved from the project file into the editing policy -- the User Context sets it from now on"
        ));
    }
    project.policy.language = Field {
        value: code.to_string(),
        origin: Origin::User,
        because: Some("the Language box before it moved into the User Context".to_string()),
    };
}

fn migrate_style(project: &mut Project, has_policy: bool, raw: &Raw, report: &mut Vec<String>) {
    let Some(style) = raw.style.as_deref() else {
        return;
    };
    if has_policy {
        report.push(format!(
            "style {style:?}: ignored, the file carries a policy"
        ));
        return;
    }
    // P.policy.markingPass and P.policy.cutMode, exactly as §2 spells the mapping.
    let (marking_pass, cut_mode) = match style {
        "read" => (MarkingPass::Joins, CutMode::Words),
        "" => (MarkingPass::Retakes, CutMode::Model),
        other => {
            report.push(format!(
                "style {other:?}: unknown, the policy defaults stand"
            ));
            return;
        }
    };
    project.policy.marking_pass = Field {
        value: marking_pass,
        origin: Origin::Default,
        because: None,
    };
    project.policy.cut_mode = Field {
        value: cut_mode,
        origin: Origin::Default,
        because: None,
    };
    report.push(format!(
        "style {style:?}: markingPass {}, cutMode {}",
        serde_json::to_value(marking_pass).unwrap_or_default(),
        serde_json::to_value(cut_mode).unwrap_or_default()
    ));
}

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

fn migrate_prompts(raw: &Raw, report: &mut Vec<String>) {
    let Some(prompts) = raw.prompts.as_ref() else {
        return;
    };
    if prompts.is_empty() {
        return;
    }
    let keys = adopted_prompt_keys(raw);
    report.push(format!(
        "prompts: adopt where the machine has none: {}",
        keys.join(", ")
    ));
}

fn adopted_prompt_keys(raw: &Raw) -> Vec<String> {
    raw.prompts
        .as_ref()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn migrate_publish(mut raw: RawPublish) -> Publish {
    // A project from when the picture printed the entry's words.
    if raw.title_off.unwrap_or(false) {
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

/// How often the project is marshalled to disk (§2).
pub const AUTOSAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Writes the project only when its bytes differ from the last write.
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
        Ok(Self {
            last_written: Some(marshal(project)?),
        })
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
