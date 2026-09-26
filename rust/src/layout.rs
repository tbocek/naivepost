//! The project's folder layout: spec/01-project-and-files.md §1.
//!
//! A project is a folder ending in `.naivepost`, and the folder **is** the output
//! folder — nothing stores where work goes, so nothing can disagree. This module
//! is the only place those paths are spelled; pure std, no UI (directive C).

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const PROJECT_SUFFIX: &str = ".naivepost";

const DIR_MODE: u32 = 0o755;
const FILE_MODE: u32 = 0o644;

/// A handle on one project folder.
#[derive(Debug, Clone)]
pub struct Tree {
    dir: PathBuf,
}

impl Tree {
    /// The folder's name ending in `.naivepost` is the whole test of "is this a
    /// project" — there is no marker file and no stored location.
    pub fn new(dir: impl Into<PathBuf>) -> Result<Tree, String> {
        let dir = dir.into();
        let is_project = dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.len() > PROJECT_SUFFIX.len() && n.ends_with(PROJECT_SUFFIX));
        if is_project {
            Ok(Tree { dir })
        } else {
            Err(format!(
                "{}: a project is a folder ending in {PROJECT_SUFFIX}",
                dir.display()
            ))
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `<name>` in `<name>.naivepost`.
    pub fn name(&self) -> Option<&str> {
        self.dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(PROJECT_SUFFIX))
    }

    // --- the project file and the folders beside it -------------------------

    pub fn project_file(&self) -> PathBuf {
        self.dir.join("naivepost.json")
    }

    /// Copies of added files, when "copy into project" is on.
    pub fn sources_dir(&self) -> PathBuf {
        self.dir.join("sources")
    }

    /// A copy in flight: `<name>.part` beside its destination.
    pub fn copy_part(&self, name: &str) -> PathBuf {
        self.sources_dir().join(format!("{name}.part"))
    }

    /// Voice-separation products.
    pub fn stems_dir(&self) -> PathBuf {
        self.dir.join("stems")
    }

    pub fn split_voice(&self, base: &str) -> PathBuf {
        self.stems_dir().join(format!("{base}.split-voice.wav"))
    }

    /// The no-voice half keeps the source's own container (`.wav` or `.mkv`).
    pub fn split_novoice(&self, base: &str, container: &str) -> PathBuf {
        self.stems_dir()
            .join(format!("{base}.split-novoice.{container}"))
    }

    // --- prepare ------------------------------------------------------------

    /// `KEY=VALUE` lines; its existence says the sources were read. §6 says the
    /// rewrite MAY drop it, so `create_dirs` does not expect it and nothing reads it.
    pub fn meta_env(&self) -> PathBuf {
        self.prepare_dir().join("inputs/meta.env")
    }

    /// This page's own folder: what Prepare writes, and what its Outputs count counts.
    pub fn prepare_dir(&self) -> PathBuf {
        self.dir.join("prepare")
    }

    pub fn input_dir(&self, source: &str) -> PathBuf {
        self.prepare_dir().join("inputs").join(source)
    }

    fn input_file(&self, source: &str, file: &str) -> PathBuf {
        self.input_dir(source).join(file)
    }

    pub fn voice16k_wav(&self, source: &str) -> PathBuf {
        self.input_file(source, "voice16k.wav")
    }

    pub fn transcript_txt(&self, source: &str) -> PathBuf {
        self.input_file(source, "transcript.txt")
    }

    pub fn transcript_tsv(&self, source: &str) -> PathBuf {
        self.input_file(source, "transcript.tsv")
    }

    pub fn transcript_srt(&self, source: &str) -> PathBuf {
        self.input_file(source, "transcript.srt")
    }

    pub fn words_json(&self, source: &str) -> PathBuf {
        self.input_file(source, "words.json")
    }

    pub fn asrchunks_json(&self, source: &str) -> PathBuf {
        self.input_file(source, "asrchunks.json")
    }

    pub fn words_aligned_json(&self, source: &str) -> PathBuf {
        self.input_file(source, "words.aligned.json")
    }

    pub fn turns_json(&self, source: &str) -> PathBuf {
        self.input_file(source, "turns.json")
    }

    /// ASR chunks (c00.wav…) — scratch while running. When it is removed is the
    /// Prepare flows' rule (F1.x), not a layout question.
    pub fn asr_scratch(&self, source: &str) -> PathBuf {
        self.input_dir(source).join("asr")
    }

    /// Forced alignment scratch: `w<ms>.wav` per window and its `.plan`. When it is removed is the
    /// Prepare flows' rule (F1.x), not a layout question.
    pub fn align_scratch(&self, source: &str) -> PathBuf {
        self.input_dir(source).join("align")
    }

    /// Diarization scratch: s.wav, a00.wav…, anchor.list, anchor.wav, seg.wav,
    /// cc.list, win.wav.
    pub fn diar_scratch(&self, source: &str) -> PathBuf {
        self.input_dir(source).join("diar")
    }

    /// `<YYYY-MM-DD_HH-MM-SS.mmm>.jpg` on the 250 ms grid restarted at each scene
    /// change, plus scenes.tsv and the `.frames` marker.
    pub fn frames_dir(&self, source: &str) -> PathBuf {
        self.prepare_dir().join("inputs/frames").join(source)
    }

    /// `<grid>|<scene threshold>` — §6. Frames are always the video's own size, so
    /// nothing else is stored; the prototype's `.interval` held `<interval>|<scale name>`
    /// and had no scenes at all.
    pub fn frames_marker(&self, source: &str) -> PathBuf {
        self.frames_dir(source).join(".frames")
    }

    /// `time\tscore`, one row per scene change (§6).
    pub fn scenes_tsv(&self, source: &str) -> PathBuf {
        self.frames_dir(source).join("scenes.tsv")
    }

    pub fn describe_dir(&self, source: &str) -> PathBuf {
        self.prepare_dir().join("describe").join(source)
    }

    /// One row per frame.
    pub fn events_tsv(&self, source: &str) -> PathBuf {
        self.describe_dir(source).join("events.tsv")
    }

    pub fn describe_state(&self, source: &str) -> PathBuf {
        self.describe_dir(source).join("state.txt")
    }

    /// The frames actually sent, scaled to P.machine.describeFrameWidth — the
    /// scaling itself belongs to Describe (F1.7).
    pub fn llm_frames_dir(&self, source: &str) -> PathBuf {
        self.describe_dir(source).join(".llmframes")
    }

    /// The folder holding this run's model exchange pages (spec/03-shell.md §7).
    pub fn llm_dir(&self) -> PathBuf {
        self.dir().join(crate::exchanges::LLM_DIR)
    }

    pub fn transcript_src_dir(&self, source: &str) -> PathBuf {
        self.prepare_dir().join("transcript").join(source)
    }

    fn transcript_dir(&self) -> PathBuf {
        self.prepare_dir().join("transcript")
    }

    /// The merged timeline, machine copy.
    pub fn session_tsv(&self) -> PathBuf {
        self.transcript_dir().join("session.tsv")
    }

    /// The timeline as sent to the cut model.
    pub fn session_txt(&self) -> PathBuf {
        self.transcript_dir().join("session.txt")
    }

    /// video, audio, offset seconds.
    pub fn offsets_tsv(&self) -> PathBuf {
        self.transcript_dir().join("offsets.tsv")
    }

    /// One source's own folder under `transcript/`, where its fixed transcript lives (§1).
    pub fn transcript_source_dir(&self, source: &str) -> PathBuf {
        self.transcript_dir().join(source)
    }

    /// The fixer's answer for one source (F1.8).
    pub fn transcript_fixed_tsv(&self, source: &str) -> PathBuf {
        self.transcript_source_dir(source).join("transcript.fixed.tsv")
    }

    /// …or the recorder's, which is commentary rather than somebody's speech (§1).
    pub fn commentary_fixed_tsv(&self, source: &str) -> PathBuf {
        self.transcript_source_dir(source).join("commentary.fixed.tsv")
    }

    /// Subtitles, for a video source only — an audio file has nothing to caption over.
    pub fn subtitles_srt(&self, source: &str) -> PathBuf {
        self.transcript_source_dir(source).join("subtitles.srt")
    }

    /// The marks.
    pub fn retakes_tsv(&self) -> PathBuf {
        self.transcript_dir().join("retakes.tsv")
    }

    /// F1.13's one session word list: the glued, re-timed, dressed and re-dressed words every later
    /// pass reads (retakes, joins, `final.txt`, subtitles). Written once after the fix pass, so its
    /// presence is that step's resume marker; see `word_list::save`/`load`.
    pub fn session_word_list_json(&self) -> PathBuf {
        self.prepare_dir().join("word_list.json")
    }

    /// The words of the finished video (markingPass joins).
    pub fn final_txt(&self) -> PathBuf {
        self.transcript_dir().join("final.txt")
    }

    // --- caches -------------------------------------------------------------

    /// Cached model replies, keyed by the exact request.
    pub fn cache_llm(&self, step: &str, sha256: &str) -> PathBuf {
        self.dir.join("cache/llm").join(step).join(sha256)
    }

    /// Waveform envelopes, one file per lane.
    pub fn wave(&self, lane: &str) -> PathBuf {
        self.dir.join("cache/waves").join(format!("{lane}.wave"))
    }

    /// Mono envelopes for edge placement.
    pub fn edges_dir(&self) -> PathBuf {
        self.dir.join("cache/edges")
    }

    // --- cut, narrate -------------------------------------------------------

    pub fn cut_json(&self) -> PathBuf {
        self.dir.join("cut/cut.json")
    }

    /// `{"t": <session second>}` — the red line.
    pub fn cut_line_json(&self) -> PathBuf {
        self.dir.join("cut/line.json")
    }

    pub fn narration_json(&self) -> PathBuf {
        self.narrate_dir().join("narration.json")
    }

    /// One generation back.
    pub fn narration_prev_json(&self) -> PathBuf {
        self.narrate_dir().join("narration.prev.json")
    }

    fn narrate_dir(&self) -> PathBuf {
        self.dir.join("narrate")
    }

    pub fn voice_txt(&self) -> PathBuf {
        self.narrate_dir().join("voice.txt")
    }

    pub fn pitch_txt(&self) -> PathBuf {
        self.narrate_dir().join("pitch.txt")
    }

    pub fn takes_json(&self) -> PathBuf {
        self.narrate_dir().join("takes.json")
    }

    pub fn voice_ref_base_wav(&self) -> PathBuf {
        self.narrate_dir().join("voice_ref_base.wav")
    }

    pub fn voice_ref_wav(&self) -> PathBuf {
        self.narrate_dir().join("voice_ref.wav")
    }

    /// Spoken lines, named by the first 16 hex digits of what was spoken.
    pub fn tts_wav(&self, hex16: &str) -> PathBuf {
        self.narrate_dir().join("tts").join(format!("{hex16}.wav"))
    }

    /// Reference samples per voice.
    pub fn sample_wav(&self, voice: &str, hex12: &str) -> PathBuf {
        self.narrate_dir()
            .join("samples")
            .join(format!("{voice}_{hex12}.wav"))
    }

    // --- produce ------------------------------------------------------------

    /// Per-clip encodes, burn cues, filled cards, baked animations, final.srt.
    pub fn clips_dir(&self) -> PathBuf {
        self.produce_dir().join("clips")
    }

    fn produce_dir(&self) -> PathBuf {
        self.dir.join("produce")
    }

    pub fn final_video(&self, container: &str) -> PathBuf {
        self.produce_dir().join(format!("final.{container}"))
    }

    pub fn final_stamp(&self) -> PathBuf {
        self.produce_dir().join("final.stamp")
    }

    /// `final.srt`, or `final.<code>.srt` for a translated subtitle.
    pub fn final_srt(&self, code: Option<&str>) -> PathBuf {
        self.produce_dir().join(match code {
            Some(code) => format!("final.{code}.srt"),
            None => "final.srt".to_string(),
        })
    }

    pub fn final_vtt(&self, code: Option<&str>) -> PathBuf {
        self.produce_dir().join(match code {
            Some(code) => format!("final.{code}.vtt"),
            None => "final.vtt".to_string(),
        })
    }

    pub fn final_jpg(&self) -> PathBuf {
        self.produce_dir().join("final.jpg")
    }

    pub fn final_html(&self) -> PathBuf {
        self.produce_dir().join("final.html")
    }

    /// The upload text and thumbnail state. A project written before the move
    /// keeps them in `<name>.naivepost/publish/`, is read there for ever, and is
    /// never migrated — so reads go through [`Tree::publish_dir`] rather than here.
    fn publish_new_dir(&self) -> PathBuf {
        self.produce_dir().join("publish")
    }

    fn publish_legacy_dir(&self) -> PathBuf {
        self.dir.join("publish")
    }

    /// Where this project's upload text lives: the new place once it exists, else
    /// the legacy one. Nothing is moved or copied — the spec forbids migrating.
    pub fn publish_dir(&self) -> PathBuf {
        let now = self.publish_new_dir();
        if now.exists() {
            now
        } else {
            self.publish_legacy_dir()
        }
    }

    pub fn publish_json(&self) -> PathBuf {
        self.publish_dir().join("publish.json")
    }

    pub fn thumbnail_png(&self) -> PathBuf {
        self.publish_dir().join("thumbnail.png")
    }

    pub fn thumbnail_plain_png(&self) -> PathBuf {
        self.publish_dir().join("thumbnail-plain.png")
    }

    pub fn thumbnail_stamp(&self) -> PathBuf {
        self.publish_dir().join("thumbnail.stamp")
    }

    pub fn description_txt(&self) -> PathBuf {
        self.publish_dir().join("description.txt")
    }

    // --- the record of runs -------------------------------------------------

    /// One readable page per run, named for the run's `<MMDD-HHMMSS>` stamp.
    pub fn llm_page(&self, run: &str, step: &str) -> PathBuf {
        self.dir.join("llm").join(format!("{run}-{step}.html"))
    }

    /// Every request sent outside, timed; appended to, never rewritten.
    pub fn requests_tsv(&self) -> PathBuf {
        self.dir.join("requests.tsv")
    }

    // --- creating and writing ----------------------------------------------

    /// The project's directory spine, every directory at 0755.
    ///
    /// Created one component at a time: `DirBuilder::recursive(true)` applies the
    /// mode only to the last component, which would leave every parent at the
    /// umask default. The mode is then set outright, because a create's mode is
    /// masked by the umask. Files are not created here — `meta.env` is dropped (§6
    /// says the rewrite may) and everything else appears when its flow writes it.
    pub fn create_dirs(&self) -> Result<(), String> {
        for dir in self.dir_spine() {
            // Absolute so the first component is the root ("/") and not a
            // relative path that `push` would then have to replace.
            let mut built = self.dir.join(dir);
            let mut parents = vec![built.clone()];
            while let Some(parent) = built.parent() {
                if parent == self.dir || parent.as_os_str().is_empty() {
                    break;
                }
                built = parent.to_path_buf();
                parents.push(built.clone());
            }
            // Deepest first is fine: each component that is missing gets created,
            // and `parents` runs from the leaf up to just under the project dir.
            for path in parents.iter().rev() {
                if path.is_dir() {
                    continue;
                }
                fs::DirBuilder::new()
                    .mode(DIR_MODE)
                    .create(path)
                    .or_else(|err| {
                        if err.kind() == std::io::ErrorKind::AlreadyExists {
                            Ok(())
                        } else {
                            Err(err)
                        }
                    })
                    .map_err(|err| format!("{}: {err}", path.display()))?;
                // The creation mode is masked by the umask, so 0755 has to be set
                // outright — under a strict umask `create` alone yields 0700.
                fs::set_permissions(path, fs::Permissions::from_mode(DIR_MODE))
                    .map_err(|err| format!("{}: {err}", path.display()))?;
            }
        }
        Ok(())
    }

    /// Every directory §1 lists inside the project, as paths relative to it.
    fn dir_spine(&self) -> Vec<PathBuf> {
        [
            "sources",
            "stems",
            "prepare/inputs",
            "prepare/inputs/frames",
            "prepare/describe",
            "prepare/transcript",
            // F1.13's word list sits directly under prepare/, beside the transcript folder it dresses from.
            "prepare",
            "cache/llm",
            "cache/waves",
            "cache/edges",
            "cut",
            "narrate/tts",
            "narrate/samples",
            "produce/clips",
            "produce/publish",
            "llm",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect()
    }

    /// Write a project file at 0644. The mode is set after the write because a
    /// new file's creation mode is masked by the process umask.
    pub fn write_file(&self, rel: &Path, bytes: &[u8]) -> Result<(), String> {
        let path = self.dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
        }
        fs::write(&path, bytes).map_err(|err| format!("{}: {err}", path.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(FILE_MODE))
            .map_err(|err| format!("{}: {err}", path.display()))
    }
}

/// SVG cards and other inserts, shared by every project under the root. Root
/// level on purpose, so it is not a method of [`Tree`].
pub fn assets_dir(root: &Path) -> PathBuf {
    root.join("assets")
}

/// The one path rule, read side. Where the application root comes from is a
/// machine setting (spec/10-parameters.md), so both roots are passed in.
pub fn resolve(root: &Path, project: &Path, written: &str) -> PathBuf {
    if let Some(rel) = written.strip_prefix("project:") {
        project.join(rel)
    } else if written.starts_with('/') {
        PathBuf::from(written)
    } else {
        root.join(written)
    }
}

/// The one path rule, write side: inside the project → `project:<slash-relative>`;
/// under the application root → root-relative; else absolute.
pub fn write_path(root: &Path, project: &Path, path: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(project) {
        format!("project:{}", slashes(rel))
    } else if let Ok(rel) = path.strip_prefix(root) {
        slashes(rel)
    } else {
        path.to_string_lossy().into_owned()
    }
}

fn slashes(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}
