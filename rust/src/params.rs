//! §04-prepare#4-parameters-used — the one place that names every parameter Prepare may be tuned by and
//! says where its value comes from.
//!
//! §4 lists what a reader has to know to judge a run: the retake and seam family, describe's context
//! sizes, the fix block, `P.audio`'s chunk/silence/diarization/merge numbers, and the engineering choices
//! around frames. It is a list of *ids*, not of numbers — so this module holds no numbers either. Every
//! entry is built from the constant the code already uses, which makes a stale catalogue impossible: move
//! a rule's bound in its own module and the entry here moves with it.

use crate::align;
use crate::asr;
use crate::cut_hear;
use crate::cut_select;
use crate::degraded;
use crate::frames;
use crate::hand_edit;
use crate::prepare;
use crate::requests;
use crate::roles;
use crate::separate;
use crate::tools::{describe, retakes, textedit};
use crate::transcribe;
use crate::word_list;

/// One row of §10's table as Prepare uses it.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// §10's full id, `P.machine.retakePauseSeconds`.
    pub id: &'static str,
    /// §10's own spelling of the value — `90, 45, 25`, `2/3`, `-35`, `on` — so a reader comparing this
    /// list against the spec is comparing strings that already match.
    pub spelled: String,
    /// The constant the value is built from, `module::CONST`.
    pub from: &'static str,
}

/// Which of §10's three homes an id belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Set per project by the User Context.
    Policy,
    /// The Settings dialog / `llm.conf`.
    Machine,
    /// Fixed in code.
    Eng,
    /// Not §10's: an engineering value §10 has no row for yet.
    Other,
}

/// A number the way §10 writes it: whole numbers lose the `.0`, fractions keep as few digits as they need.
fn num(value: f64) -> String {
    if value == value.trunc() {
        return format!("{}", value as i64);
    }
    let mut text = format!("{value}");
    while text.ends_with('0') && !text.ends_with(".0") {
        text.pop();
    }
    text.trim_end_matches('.').to_string()
}

/// A list of seconds, §10-style: `90, 45, 25`.
fn secs(values: &[f64]) -> String {
    values.iter().copied().map(num).collect::<Vec<_>>().join(", ")
}

fn param(id: &'static str, spelled: String, from: &'static str) -> Param {
    Param { id, spelled, from }
}

/// Every parameter Prepare reads, in §4's order: the eighteen it names by hand, then `P.audio`'s chunk,
/// silence, diarization and merge numbers, then the engineering ones around frames.
pub fn prepare() -> Vec<Param> {
    let mut params = vec![
        // --- §4's own list -----------------------------------------------------------------------
        param(
            "P.policy.minTakeSeconds",
            num(requests::MIN_TAKE_SECONDS),
            "requests::MIN_TAKE_SECONDS",
        ),
        param(
            "P.machine.retakePauseSeconds",
            num(retakes::RETAKE_PAUSE_SECONDS),
            "tools::retakes::RETAKE_PAUSE_SECONDS",
        ),
        param(
            "P.machine.retakeRuns",
            roles::RETAKE_RUNS_POOLED.to_string(),
            "roles::RETAKE_RUNS_POOLED",
        ),
        param(
            "P.machine.retakeReachSeconds",
            num(retakes::RETAKE_REACH_SECONDS),
            "tools::retakes::RETAKE_REACH_SECONDS",
        ),
        param(
            "P.machine.retakeMinSeconds",
            num(retakes::RETAKE_MIN_SECONDS),
            "tools::retakes::RETAKE_MIN_SECONDS",
        ),
        param(
            "P.machine.retakeCeil",
            num(retakes::RETAKE_CEIL),
            "tools::retakes::RETAKE_CEIL",
        ),
        param(
            "P.machine.retakeFragmentSeconds",
            num(retakes::RETAKE_FRAGMENT_SECONDS),
            "tools::retakes::RETAKE_FRAGMENT_SECONDS",
        ),
        param(
            "P.machine.againReachSeconds",
            num(retakes::AGAIN_REACH_SECONDS),
            "tools::retakes::AGAIN_REACH_SECONDS",
        ),
        param(
            "P.machine.seamReachWords",
            textedit::SEAM_REACH_WORDS.to_string(),
            "tools::textedit::SEAM_REACH_WORDS",
        ),
        param(
            "P.machine.seamMaxWords",
            textedit::SEAM_MAX_WORDS.to_string(),
            "tools::textedit::SEAM_MAX_WORDS",
        ),
        param(
            "P.machine.seamCeil",
            num(textedit::SEAM_CEIL),
            "tools::textedit::SEAM_CEIL",
        ),
        param(
            "P.machine.seamSnapWords",
            textedit::SEAM_SNAP_WORDS.to_string(),
            "tools::textedit::SEAM_SNAP_WORDS",
        ),
        param(
            "P.machine.seamNoiseWords",
            textedit::SEAM_NOISE_WORDS.to_string(),
            "tools::textedit::SEAM_NOISE_WORDS",
        ),
        param(
            "P.machine.describeFramesPerReq",
            roles::VISION_FRAMES_PER_CALL.to_string(),
            "roles::VISION_FRAMES_PER_CALL",
        ),
        param(
            "P.machine.describeRecentEvents",
            describe::RECENT_EVENTS.to_string(),
            "tools::describe::RECENT_EVENTS",
        ),
        param(
            "P.machine.describeCtxSegs",
            describe::CTX_SEGS.to_string(),
            "tools::describe::CTX_SEGS",
        ),
        param(
            "P.machine.describeCtxWindowSeconds",
            num(describe::CTX_WINDOW_SECONDS),
            "tools::describe::CTX_WINDOW_SECONDS",
        ),
        param(
            "P.machine.fixBlockLines",
            prepare::FIX_BLOCK_LINES.to_string(),
            "prepare::FIX_BLOCK_LINES",
        ),
    ];
    params.extend(audio());
    params.extend(engineering());
    params
}

/// `P.audio`: chunk sizes, the silence threshold, diarization windows and anchor sizes, merge gap/length.
fn audio() -> Vec<Param> {
    vec![
        param(
            "P.machine.asrChunkQwenSeconds",
            num(asr::CHUNK_QWEN),
            "asr::CHUNK_QWEN",
        ),
        param(
            "P.machine.asrChunkMaxSeconds",
            num(asr::CHUNK_MAX),
            "asr::CHUNK_MAX",
        ),
        param(
            "P.machine.asrChunkMinSeconds",
            num(asr::CHUNK_MIN),
            "asr::CHUNK_MIN",
        ),
        param(
            "P.machine.alignChunkMaxSeconds",
            num(align::WINDOW),
            "align::WINDOW",
        ),
        param(
            "P.machine.alignChunkMinSeconds",
            num(align::MIN_PIECE),
            "align::MIN_PIECE",
        ),
        param(
            "P.machine.sepChunkMaxSeconds",
            num(separate::CHUNK_MAX_SECONDS),
            "separate::CHUNK_MAX_SECONDS",
        ),
        // The silencedetect pair, spelled twice over: the same two numbers live in asr's own names for
        // the ffmpeg filter and in degraded's, which is where silence is measured off an envelope.
        param("P.machine.silenceThresholdDB", num(asr::QUIET_DB), "asr::QUIET_DB"),
        param("P.machine.silenceMinSeconds", num(asr::QUIET_MIN), "asr::QUIET_MIN"),
        param(
            "P.machine.diarWindowsSeconds",
            secs(&roles::DIAR_WINDOWS_SECONDS),
            "roles::DIAR_WINDOWS_SECONDS",
        ),
        param(
            "P.machine.diarHopShare",
            format!("{}/{}", roles::DIAR_HOP_SHARE.0, roles::DIAR_HOP_SHARE.1),
            "roles::DIAR_HOP_SHARE",
        ),
        param(
            "P.machine.anchorPerSeconds",
            num(roles::ANCHOR_PER_SECONDS),
            "roles::ANCHOR_PER_SECONDS",
        ),
        param("P.eng.mergeGapSeconds", num(transcribe::MERGE_GAP), "transcribe::MERGE_GAP"),
        param(
            "P.eng.mergeMaxSeconds",
            num(transcribe::MERGE_MAX_LEN),
            "transcribe::MERGE_MAX_LEN",
        ),
        param(
            "P.eng.mergeMaxWordSeconds",
            num(transcribe::MERGE_MAX_WORD),
            "transcribe::MERGE_MAX_WORD",
        ),
        param(
            "P.eng.mergeNearSeconds",
            num(transcribe::MERGE_NEAR),
            "transcribe::MERGE_NEAR",
        ),
        param(
            "P.eng.asrSampleRate",
            transcribe::SAMPLE_RATE.to_string(),
            "transcribe::SAMPLE_RATE",
        ),
        param("P.eng.asrCutSeekSeconds", num(asr::SEEK_MAX), "asr::SEEK_MAX"),
    ]
}

/// Engineering: frame workers, quality, scratch handling. The scratch folders are §1's layout rather than
/// a number, so they appear as paths (`Tree::asr_scratch` and friends) instead of rows here.
fn engineering() -> Vec<Param> {
    vec![
        param(
            "P.eng.frameGridSeconds",
            num(prepare::EXTRACTION_GRID),
            "prepare::EXTRACTION_GRID",
        ),
        param(
            "P.eng.frameWorkersMinFrames",
            frames::WORKER_MIN_FRAMES.to_string(),
            "frames::WORKER_MIN_FRAMES",
        ),
        param("P.eng.frameWorkersMin", frames::WORKERS_MIN.to_string(), "frames::WORKERS_MIN"),
        param("P.eng.frameWorkersMax", frames::WORKERS_MAX.to_string(), "frames::WORKERS_MAX"),
        param(
            "P.machine.sceneThreshold",
            num(frames::SCENE_THRESHOLD),
            "frames::SCENE_THRESHOLD",
        ),
        param(
            "P.machine.sceneMinGapSeconds",
            num(frames::SCENE_MIN_GAP),
            "frames::SCENE_MIN_GAP",
        ),
        param(
            "P.machine.describeFrameWidth",
            frames::LLM_FRAME_WIDTH.to_string(),
            "frames::LLM_FRAME_WIDTH",
        ),
        // §10 gives the JPEG quality of the frames a project keeps no id of its own (its frame-width row
        // covers the copies sent), so it is spelled after that row rather than as an invented `P.` id.
        param(
            "machine.jpegQuality",
            frames::STORED_FRAME_JPEG_QUALITY.to_string(),
            "frames::STORED_FRAME_JPEG_QUALITY",
        ),
        param("P.eng.keepReachWords", hand_edit::KEEP_REACH.to_string(), "hand_edit::KEEP_REACH"),
        // F1.13's word list: how a word is caught being on no sound, and how far the two dressings look.
        param(
            "P.machine.strayWordRatio",
            word_list::STRAY_RATIO.to_string(),
            "word_list::STRAY_RATIO",
        ),
        param(
            "P.machine.strayWordGapSeconds",
            num(word_list::STRAY_GAP_SECONDS),
            "word_list::STRAY_GAP_SECONDS",
        ),
        param(
            "P.eng.dressReachWords",
            word_list::DRESS_REACH_WORDS.to_string(),
            "word_list::DRESS_REACH_WORDS",
        ),
        param(
            "P.eng.respellRunReachWords",
            word_list::RESPPELL_REACH_WORDS.to_string(),
            "word_list::RESPPELL_REACH_WORDS",
        ),
        // F2.5's mix: the ceiling every gain is held to, an effect's own and the slider's alike.
        param("P.eng.maxGain", num(cut_hear::MAX_GAIN), "cut_hear::MAX_GAIN"),
        // F2.6's two floors, each held by the rule that reads it: a resize may not close the band, and
        // ＋ Add / ⧉ Copy will not take a region too short to be a scene.
        param("P.eng.minPieceSeconds", num(cut_select::MIN_SECONDS), "cut_select::MIN_SECONDS"),
        param(
            "P.policy.minSceneSeconds",
            num(cut_select::MIN_SCENE_SECONDS),
            "cut_select::MIN_SCENE_SECONDS",
        ),
    ]
}

/// One parameter by its §10 id.
pub fn find(id: &str) -> Option<Param> {
    prepare().into_iter().find(|param| param.id == id)
}

/// Which home an id's prefix names — including for ids Prepare does not use, since the question is about
/// §10's table rather than this module's rows.
pub fn family(id: &str) -> Family {
    if id.starts_with("P.policy.") {
        Family::Policy
    } else if id.starts_with("P.machine.") {
        Family::Machine
    } else if id.starts_with("P.eng.") {
        Family::Eng
    } else {
        Family::Other
    }
}

/// The silence pair as §10 spells it, from the module that measures an envelope: both spellings of the
/// same two numbers are here so a caller never has to know which one a given module reached for.
pub fn silence() -> (f64, f64) {
    (degraded::SILENCE_THRESHOLD_DB, degraded::SILENCE_MIN_SECONDS)
}
