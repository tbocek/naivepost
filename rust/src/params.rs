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
use crate::cut;
use crate::cut_captions;
use crate::cut_cards;
use crate::cut_clamp;
use crate::cut_copy;
use crate::cut_effects_pass;
use crate::cut_insert;
use crate::cut_line;
use crate::cut_play_back;
use crate::cut_review;
use crate::cut_screen;
use crate::cut_select;
use crate::cut_speed;
use crate::cut_speed_pass;
use crate::cut_trim;
use crate::degraded;
use crate::diarize;
use crate::edges;
use crate::frames;
use crate::fx_aspect;
use crate::fx_band;
use crate::fx_lane;
use crate::fx_record;
use crate::fx_svg;
use crate::fx_text;
use crate::fx_volume;
use crate::fx_zoom;
use crate::hand_edit;
use crate::llm_budget;
use crate::llm_liveness;
use crate::llm_retry;
use crate::prepare;
use crate::prepare_decisions;
use crate::preview;
use crate::produce_render;
use crate::produce_runs;
use crate::produce_screen;
use crate::produce_subtitles;
use crate::produce_upload;
use crate::project;
use crate::render_fx;
use crate::requests;
use crate::roles;
use crate::runqueue;
use crate::separate;
use crate::suggest;
use crate::tools::{clips, cutpass, describe, retakes, textedit};
use crate::transcribe;
use crate::ui;
use crate::watchdog;
use crate::wave;
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

/// Which of §10's homes an id belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Set per project by the User Context.
    Policy,
    /// §3's fourth home: the project's own tab controls. §10 spells `P.` for one row
    /// only (`P.project.frameInterval`); the rest of that section's values carry a bare
    /// `project.` prefix and land in [`Family::Other`], as every unrowed id here does.
    Project,
    /// The Settings dialog / `llm.conf`.
    Machine,
    /// Fixed in code.
    Eng,
    /// Not §10's: an engineering value §10 has no row for yet.
    Other,
}

/// A number the way §10 writes it: whole numbers lose the `.0`, fractions keep as few digits as they need.
pub(crate) fn num(value: f64) -> String {
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
pub(crate) fn secs(values: &[f64]) -> String {
    values
        .iter()
        .copied()
        .map(num)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn param(id: &'static str, spelled: String, from: &'static str) -> Param {
    Param { id, spelled, from }
}

/// One of §6's colours, spelled as three components (`0.2 0.8 0.3`) so a reader can compare it with the tuple in
/// `spec/inventory/cut.md` §D without decoding a hex string.
pub(crate) fn colour(
    id: &'static str,
    rgb: crate::ui::cut_colours::Rgb,
    from: &'static str,
) -> Param {
    param(id, crate::ui::cut_colours::spell(rgb), from)
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
        // F3.9 S1: how many clips one caption request carries, read from the pass that builds the batches.
        param(
            "P.machine.captionBatch",
            cut_captions::BATCH.to_string(),
            "cut_captions::BATCH",
        ),
        // F0.5's two clocks. §10 lists them only inside its prose line ("progress pulse 150 ms;
        // checkpoint poll 200 ms") and gives neither a `P.` id, so they take the bare `machine.`
        // prefix — the same choice `machine.jpegQuality` made over inventing ids §10 does not carry.
        param(
            "machine.checkpointPollMs",
            format!("{} ms", runqueue::CHECKPOINT_POLL.as_millis()),
            "runqueue::CHECKPOINT_POLL",
        ),
        param(
            "machine.progressPulseMs",
            format!("{} ms", runqueue::PROGRESS_PULSE.as_millis()),
            "runqueue::PROGRESS_PULSE",
        ),
        // §1's hang watchdog, the third pair of numbers in that same prose row ("hang watchdog 200 ms /
        // 3 s"): the GUI thread's beat and how much of its silence counts as a hang.
        param(
            "machine.hangWatchdogBeatMs",
            format!("{} ms", watchdog::HEARTBEAT.as_millis()),
            "watchdog::HEARTBEAT",
        ),
        param(
            "machine.hangWatchdogSilenceSeconds",
            format!("{} s", watchdog::SILENCE_LIMIT.as_secs()),
            "watchdog::SILENCE_LIMIT",
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
        param(
            "P.machine.silenceThresholdDB",
            num(asr::QUIET_DB),
            "asr::QUIET_DB",
        ),
        param(
            "P.machine.silenceMinSeconds",
            num(asr::QUIET_MIN),
            "asr::QUIET_MIN",
        ),
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
        // §10's row for how far a slot's voice must reach into an anchor block to claim it: one
        // constant answers both this row and `diarize::match_slots`' two bound checks, so the
        // catalogue cannot drift from the rule that reads it.
        param(
            "P.eng.minAnchorOverlap",
            num(diarize::MIN_ANCHOR_OVERLAP),
            "diarize::MIN_ANCHOR_OVERLAP",
        ),
        param(
            "P.eng.mergeGapSeconds",
            num(transcribe::MERGE_GAP),
            "transcribe::MERGE_GAP",
        ),
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
        // §10's row for the gap that ends a turn: one constant answers both this row and
        // `transcribe::glue_turns`' comparison, so the catalogue cannot drift from the rule that
        // reads it.
        param(
            "P.eng.turnGapSeconds",
            num(transcribe::TURN_GLUE),
            "transcribe::TURN_GLUE",
        ),
        param(
            "P.eng.asrSampleRate",
            transcribe::SAMPLE_RATE.to_string(),
            "transcribe::SAMPLE_RATE",
        ),
        param(
            "P.eng.asrCutSeekSeconds",
            num(asr::SEEK_MAX),
            "asr::SEEK_MAX",
        ),
        // §10's row for the alignment cut's own reach: one constant answers both this row and
        // `align::pieces`' call into the shared cutter, so the catalogue cannot drift from the rule.
        param(
            "P.eng.alignCutSeekSeconds",
            num(align::CUT_SEEK),
            "align::CUT_SEEK",
        ),
        // §10's row for the sound kept on each side of an alignment piece: one constant answers both
        // this row and `trim()`/`bare_voiced()`, so the catalogue cannot drift from the rule.
        param("P.eng.alignSoundPadSeconds", num(align::PAD), "align::PAD"),
        // §10's row for the sanity ceiling on a returned "second": one constant answers both this row
        // and `time()`'s bare-key branch, so the catalogue cannot drift from the rule. A bare number
        // past it was a sample count that did not say so.
        param(
            "P.eng.alignClipMaxSeconds",
            num(align::PLAIN_AS_SAMPLES),
            "align::PLAIN_AS_SAMPLES",
        ),
        // §10's row for the absolute floor under the bare-audio warning: one constant answers both this
        // row and `bare_warns()`, so the catalogue cannot drift from the rule.
        param(
            "P.eng.alignBareWarnSeconds",
            num(align::BARE_WARN),
            "align::BARE_WARN",
        ),
        // §10's row for the relative half of the same warning: `bare_warns()` reads this alongside
        // `align::BARE_WARN`, so one constant each keeps the catalogue from drifting from the rule.
        param(
            "P.eng.alignBareShare",
            num(align::BARE_SHARE),
            "align::BARE_SHARE",
        ),
        // §10's row for the lop-sided split note: one constant answers both this row and
        // `lopsided()`, so the catalogue cannot drift from the rule. The note is what lets the user
        // drop the empty half by hand; the app never discards a stem on its own.
        param(
            "P.eng.sepLopsidedDB",
            num(separate::LOPSIDED_DB),
            "separate::LOPSIDED_DB",
        ),
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
        param(
            "P.eng.frameWorkersMin",
            frames::WORKERS_MIN.to_string(),
            "frames::WORKERS_MIN",
        ),
        param(
            "P.eng.frameWorkersMax",
            frames::WORKERS_MAX.to_string(),
            "frames::WORKERS_MAX",
        ),
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
        param(
            "P.eng.keepReachWords",
            hand_edit::KEEP_REACH.to_string(),
            "hand_edit::KEEP_REACH",
        ),
        // §10's row for how many kept words either side of a join are compared for a doubled saying.
        // One constant answers both this row and `textedit::dedupe_joins`' cap, so the catalogue
        // cannot drift from the rule that reads it.
        param(
            "P.eng.joinReachWords",
            textedit::JOIN_REACH_WORDS.to_string(),
            "tools::textedit::JOIN_REACH_WORDS",
        ),
        // §10's row for the fuzzy repeat matcher's step-over: how many words of the later take may be skipped
        // while looking for the next tail word. One constant answers both this row and `run_from`'s loop, so the
        // catalogue cannot drift from the rule that reads it.
        param(
            "P.eng.repeatSkip",
            prepare_decisions::TAIL_SKIP_MAX.to_string(),
            "prepare_decisions::TAIL_SKIP_MAX",
        ),
        // §10's row for the share of the marked tail that has to turn up again. One constant answers both this
        // row and `suffix_match`'s share test, so the catalogue cannot drift from the rule that reads it.
        param(
            "P.eng.repeatShare",
            num(prepare_decisions::TAIL_MATCH_MIN),
            "prepare_decisions::TAIL_MATCH_MIN",
        ),
        // §10's row for the room a cut leaves a word. One constant answers both this row and every pad in
        // `edges` — `place_edges`, `end_after`/`start_before` and the word a stamp names — so the catalogue
        // cannot drift from the rule that reads it.
        param(
            "P.eng.wordPadSeconds",
            num(edges::WORD_PAD),
            "edges::WORD_PAD",
        ),
        // §10's row for the reach of the envelope-only edge walk: how far off its stamp an edge may be
        // moved when there are no word times to fence it. One constant answers both this row and
        // `Edges::end_before`/`start_at`, so the catalogue cannot drift from the rule that reads it.
        param(
            "P.eng.edgeReachSeconds",
            num(edges::EDGE_REACH),
            "edges::EDGE_REACH",
        ),
        // §10's row for how late a stamp may be behind its sound: one constant answers both this row
        // and `Edges::start_at`, so the catalogue cannot drift from the rule that reads it.
        param(
            "P.eng.lateStampSeconds",
            num(edges::LATE_STAMP),
            "edges::LATE_STAMP",
        ),
        // §10's row for the window the room is measured over: one constant answers both this row and
        // `Edges::floor`, so the catalogue cannot drift from the rule that reads it.
        param(
            "P.eng.envelopeWindowSeconds",
            num(edges::ENVELOPE_WINDOW),
            "edges::ENVELOPE_WINDOW",
        ),
        // §10's row for how far below a word's own peak its tail still counts as the word: one
        // constant answers both this row and `Edges::tail_level`, so the catalogue cannot drift from
        // the rule that reads it.
        param(
            "P.eng.edgeTailDB",
            num(edges::EDGE_TAIL_DB),
            "edges::EDGE_TAIL_DB",
        ),
        // §10's row for how long a word's tail is followed at most: one constant answers both this row
        // and `Edges::end_after`/`start_before`, so the catalogue cannot drift from the rule that
        // reads it.
        param(
            "P.eng.edgeTailMaxSeconds",
            num(edges::EDGE_TAIL_MAX),
            "edges::EDGE_TAIL_MAX",
        ),
        // §10's row for how far past the word's tail the quietest moment is looked for: one constant
        // answers both this row and `Edges::end_after`/`start_before`, so the catalogue cannot drift
        // from the rule that reads it.
        param(
            "P.eng.troughReachSeconds",
            num(edges::TROUGH_REACH),
            "edges::TROUGH_REACH",
        ),
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
        param(
            "P.eng.maxGain",
            num(fx_record::gain_ceiling()),
            "fx_record::gain_ceiling",
        ),
        // §10's own row for the rejoin dip, catalogued from the module that fades it: the lane draws it and the render
        // plans its read head around the same number.
        param(
            "P.eng.soundDipSeconds",
            num(fx_lane::SOUND_DIP_SECONDS),
            "fx_lane::SOUND_DIP_SECONDS",
        ),
        // F3.12: how much of an effect band has to survive the final cut for it to be worth keeping. Read by the clamp
        // pass, which runs after snapping, dead-air and mark removal, and coalescing.
        param(
            "P.eng.effectMinSurvivingSeconds",
            cut_clamp::MIN_SURVIVING_SECONDS.to_string(),
            "cut_clamp::MIN_SURVIVING_SECONDS",
        ),
        // F2.6's two floors, each held by the rule that reads it: a resize may not close the band, and
        // ＋ Add / ⧉ Copy will not take a region too short to be a scene.
        param(
            "P.eng.minPieceSeconds",
            num(cut_select::MIN_SECONDS),
            "cut_select::MIN_SECONDS",
        ),
        param(
            "P.policy.minSceneSeconds",
            num(cut_select::MIN_SCENE_SECONDS),
            "cut_select::MIN_SCENE_SECONDS",
        ),
        // F2.12's fallback: how long a still or card runs when nothing in the file says.
        param(
            "P.policy.insertDefaultSeconds",
            num(cut_insert::DEFAULT_SECONDS),
            "cut_insert::DEFAULT_SECONDS",
        ),
        // §09 §3's two patiences. §10 lists them only in its by-area line ("backoff 5 s–4 min"), with no `P.`
        // row of their own, so they carry the bare `llm.` prefix. The ladder is one string because it is one
        // decision — five waits that together outlast a container restart and a weight load.
        param(
            "llm.backoffSeconds",
            llm_retry::BACKOFF_SECONDS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            "llm_retry::BACKOFF_SECONDS",
        ),
        param(
            "llm.retrySeconds",
            llm_retry::RETRY_SECONDS.to_string(),
            "llm_retry::RETRY_SECONDS",
        ),
        // §09 §4's liveness bounds. These two DO have rows of their own in §10, so they carry real `P.` ids
        // (unlike the two above).
        param(
            "P.eng.llmStallMinutes",
            llm_liveness::STALL_MINUTES.to_string(),
            "llm_liveness::STALL_MINUTES",
        ),
        param(
            "P.eng.llmWholeMinutes",
            llm_liveness::WHOLE_MINUTES.to_string(),
            "llm_liveness::WHOLE_MINUTES",
        ),
        // §09 §5: the most one request may carry. Spelled plain (no thousands separator), the way
        // P.eng.thumbnailJPEGMax spells its 2 MiB cap.
        param(
            "P.machine.promptMaxChars",
            llm_budget::PROMPT_MAX_CHARS.to_string(),
            "llm_budget::PROMPT_MAX_CHARS",
        ),
    ]
}

/// §05-cut#6-parameters-used — every parameter the Cut page may be tuned by, in `spec/05-cut.md` §6's own
/// order, built from the constant each rule already reads.
///
/// §6 is a list of ids, not of numbers, so this holds none either: move a bound in its module and this row
/// moves with it. The rows split as §6 splits them — what bounds an edit, what the suggest pass and the target
/// arithmetic compute on, what the preview does, then the engineering choices around pixels and formats.
///
/// Deliberately absent, because no rule in this tree reads them yet: nothing does now — `P.eng.talkPadSeconds`,
/// §D's `rateSeekGap` 250 ms and thumbnail batch 6 all gained a reader and a row below.
/// The dead-air pair (`deadAirMaxSeconds`,
/// `deadAirKeepSeconds`) and `P.eng.seamMaxSeconds` are no longer among them: F2.14 S2a and S3 read all
/// three from `src/suggest.rs`, so each has a row below. (`P.eng.preloadLeadSeconds` is no longer one of them:
/// `preview::PRELOAD_LEAD_SECONDS` reads it, so its row is in `cut()`.)
/// A number nothing reads has no module to live in, which is the rule this catalogue exists to keep — each of
/// those appears here the round its rule is written. §6's colours are catalogued at the foot of this list from
/// `ui::cut_colours`, the one module that spells them. §6 also groups the preview numbers under a heading, and
/// §10 has no such family (they are listed in `spec/inventory/cut.md` §D), so those rows take a `preview.`
/// prefix rather than inventing ids §10 does not carry — the same choice `machine.jpegQuality` made.
pub fn cut() -> Vec<Param> {
    vec![
        // --- what bounds an edit -----------------------------------------------------------------
        // Also catalogued for Prepare, which keeps a scene by the same bound: one constant, two sections
        // naming it, so neither list can drift from the rule in cut_select.
        param(
            "P.policy.minSceneSeconds",
            num(cut_select::MIN_SCENE_SECONDS),
            "cut_select::MIN_SCENE_SECONDS",
        ),
        param("P.eng.minPieceSeconds", num(cut_select::MIN_SECONDS), "cut_select::MIN_SECONDS"),
        param(
            "P.policy.snapToleranceSeconds",
            num(cutpass::SNAP_TOLERANCE_SECONDS),
            "tools::cutpass::SNAP_TOLERANCE_SECONDS",
        ),
        // ▶✂✂ plays this much before and after each join; §6's review bound, held by the module that walks it.
        param(
            "P.policy.reviewPadSeconds",
            num(cut_review::REVIEW_PAD_SECONDS),
            "cut_review::REVIEW_PAD_SECONDS",
        ),
        // --- what the suggest pass and the target arithmetic compute on ---------------------------
        param(
            "P.policy.targetLengthSeconds",
            format!("{} (none)", num(crate::project::Policy::default().target_length_seconds.value)),
            "project::Policy::target_length_seconds",
        ),
        // F2.14 S2a: the dead-air pair, both read by the words path's walk-back in `src/suggest.rs`.
        param(
            "P.policy.deadAirMaxSeconds",
            num(suggest::DEAD_AIR_MAX_SECONDS),
            "suggest::DEAD_AIR_MAX_SECONDS",
        ),
        param(
            "P.policy.deadAirKeepSeconds",
            num(suggest::DEAD_AIR_KEEP_SECONDS),
            "suggest::DEAD_AIR_KEEP_SECONDS",
        ),
        // F2.14 S3: how wide a hole the walk-back may close where somebody talked.
        param(
            "P.eng.seamMaxSeconds",
            num(suggest::SEAM_MAX_SECONDS),
            "suggest::SEAM_MAX_SECONDS",
        ),
        param(
            "P.policy.shortTargetSeconds",
            num(cutpass::SHORT_TARGET_SECONDS),
            "cutpass::SHORT_TARGET_SECONDS",
        ),
        param("P.policy.maxSpeedRate", num(cutpass::MAX_RATE), "cutpass::MAX_RATE"),
        // F3.9 S5: the floor a proposed caption has to clear before it is worth placing, read from §3.7's tool —
        // the one place 0.3 is written for this rule, so the pass and the tool cannot disagree about what is short.
        param(
            "P.policy.captionMinSeconds",
            num(clips::CAPTION_MIN_SECONDS),
            "tools::clips::CAPTION_MIN_SECONDS",
        ),
        // F3.10 S4: how near two runs at one rate may come before they are one run. Read from the speed pass, which is
        // the only rule that folds stretches — the by-hand page (§F3.3) never merges what a person placed.
        param(
            "P.policy.speedGapSeconds",
            num(cut_speed_pass::GAP_SECONDS),
            "cut_speed_pass::GAP_SECONDS",
        ),
        // F3.10 S3/S4: the two numbers that decide whether a proposed rate says anything and what its ramps cost. §10
        // gives them no `P.` row, so both take the bare prefix of the rule that reads them — as F3.9's and F3.6's rows
        // above do.
        param(
            "effects.proposedRampSeconds",
            num(cut_speed_pass::PROPOSED_RAMP_SECONDS),
            "cut_speed_pass::PROPOSED_RAMP_SECONDS",
        ),
        param(
            "effects.neutralRateTolerance",
            num(cut_speed_pass::NEUTRAL_TOLERANCE),
            "cut_speed_pass::NEUTRAL_TOLERANCE",
        ),
        // F3.11 S4: the two numbers §F3.11 gives a proposed decoration that §10 leaves unrowed — how tall a model's zoom
        // is, and how soft a model's stop lands. Both read from this pass; the glide and the volume ramp are already
        // catalogued above as P.policy.effectDefaultFades' zoom value and F3.10's proposed-ramp row.
        param(
            "effects.proposedZoomHeight",
            num(cut_effects_pass::ZOOM_HEIGHT),
            "cut_effects_pass::ZOOM_HEIGHT",
        ),
        param(
            "effects.proposedStopFadeSeconds",
            num(cut_effects_pass::STOP_FADE_CAP_SECONDS),
            "cut_effects_pass::STOP_FADE_CAP_SECONDS",
        ),
        // Three formula rows: §10 gives these no number to spell, only the arithmetic, so what is catalogued
        // is §10's own text and the function that computes it.
        param(
            "P.machine.suggestMinSegments",
            "min(1 + \u{230a}target/30\u{230b}, 4)".to_string(),
            "tools::cutpass::min_segments",
        ),
        param(
            "P.machine.suggestMaxSegments",
            "max(\u{230a}target/5\u{230b}, 40)".to_string(),
            "tools::cutpass::max_segments",
        ),
        param(
            "P.machine.footageWindow",
            "target \u{00d7} [0.6, 1.2] (\u{2264} shortTargetSeconds) else [0.6, 1.5]; ceiling \u{00d7} maxSpeedRate"
                .to_string(),
            "tools::cutpass::footage_window",
        ),
        param(
            "P.policy.insertDefaultSeconds",
            num(cut_insert::DEFAULT_SECONDS),
            "cut_insert::DEFAULT_SECONDS",
        ),
        // F2.9 S2's floor on the sound-over path: a piece of laid-over sound shorter than this is not worth a
        // segment of its own, so `cut_copy::sound_piece_overlap` lays none and `lay_over` splits nothing. §10
        // rows it as an engineering constant; the rule that reads it lives in cut_copy.
        param(
            "P.eng.soundMinPieceSeconds",
            num(cut_copy::MIN_SOUND_PIECE_SECONDS),
            "cut_copy::MIN_SOUND_PIECE_SECONDS",
        ),
        // F3.2's framing zoom, under the bare name §F gave it; §10 rows the same value as
        // `P.eng.aspectStaySeconds` right below, both reading this one constant.
        param(
            "effects.aspectHoldSeconds",
            num(fx_aspect::HOLD_SECONDS),
            "fx_aspect::HOLD_SECONDS",
        ),
        // §10's own row for that same second: "length of the staying zoom an aspect change places". One
        // constant answers both ids — `fx_aspect::choose` reads HOLD_SECONDS once when it builds the zoom a
        // shape brings — so the two spellings cannot be tuned apart, which is why this row points at the same
        // place rather than holding a second copy of 1.0. (Same shape as `P.policy.suggestedZoomHeight` /
        // `effects.proposedZoomHeight` and `P.eng.effectMinSelectionSeconds` / `effects.markedBandMinSeconds`.)
        param(
            "P.eng.aspectStaySeconds",
            num(fx_aspect::HOLD_SECONDS),
            "fx_aspect::HOLD_SECONDS",
        ),
        // F3.3's default rate: §06 §6 names it ("default rate 0.5") and §10 gives it no row, so the same bare prefix.
        param(
            "effects.defaultRate",
            num(cut_speed::DEFAULT_RATE),
            "cut_speed::DEFAULT_RATE",
        ),
        // F3.5's preview raster: §10 lists 512 as an implicit constant (`svgPreviewPx`) with no `P.` row, so the id
        // takes the bare prefix of the rule that reads it — the same treatment as the two rows above.
        param(
            "effects.previewRasterPx",
            num(fx_svg::PREVIEW_RASTER_PX),
            "fx_svg::PREVIEW_RASTER_PX",
        ),
        // --- F3.6's own defaults: §F's "gain 2" and its `fxMinDur 0.1` for volume have no `P.` row, so both take the
        // bare prefix of the rule that reads them — the same treatment as the three rows above.
        param(
            "effects.defaultGain",
            num(fx_volume::DEFAULT_GAIN),
            "fx_volume::DEFAULT_GAIN",
        ),
        param(
            "effects.volumeMinSeconds",
            num(fx_volume::MIN_SECONDS),
            "fx_volume::MIN_SECONDS",
        ),
        // §06-effects#4's three render numbers. §10 names them in prose ("Engineering: ... bake fps") but gives no `P.`
        // id, so each takes the bare prefix of the rule that reads it, like the two rows above.
        param(
            "effects.renderZoomFps",
            num(render_fx::ZOOM_GRID_FPS),
            "render_fx::ZOOM_GRID_FPS",
        ),
        param(
            "effects.renderZoomDepthCap",
            num(render_fx::ZOOM_DEPTH_CAP),
            "render_fx::ZOOM_DEPTH_CAP",
        ),
        param(
            "effects.renderTierShortSide",
            render_fx::TIER_SHORT_SIDE.to_string(),
            "render_fx::TIER_SHORT_SIDE",
        ),
        // §06-effects#5's card numbers. §10 writes them in prose ("card canvas 1920×1080 and timings, bake 25 fps")
        // without `P.` ids, so they take the bare `card.` prefix of the rules that read them.
        param(
            "card.stillnessSeconds",
            num(cut_cards::STILLNESS_SECONDS),
            "cut_cards::STILLNESS_SECONDS",
        ),
        param("card.bakeFps", num(cut_cards::BAKE_FPS), "cut_cards::BAKE_FPS"),
        param(
            "card.canvas",
            format!(
                "{}\u{00d7}{}",
                cut_cards::CANVAS.0,
                cut_cards::CANVAS.1
            ),
            "cut_cards::CANVAS",
        ),
        // --- what the preview does (§6's preview group, §D's numbers) --------------------------------
        param("preview.playTickMs", format!("{} ms", preview::TICK_MS), "preview::TICK_MS"),
        // F2.1's preview: how far ahead the next clip is opened, so a known jump is a swap not a reload.
        param(
            "P.eng.preloadLeadSeconds",
            num(preview::PRELOAD_LEAD_SECONDS),
            "preview::PRELOAD_LEAD_SECONDS",
        ),
        param("preview.cardFps", num(cut_insert::PREVIEW_FPS), "cut_insert::PREVIEW_FPS"),
        // §6's preview group in full: the two numbers §D names that had no reader when this list was first
        // written. The rate-seek floor is read by `cut_play_back::set_rate` before it replaces a player;
        // the thumbnail batch bounds how far one band pass looks into a row's frame list.
        param(
            "P.eng.talkPadSeconds",
            num(suggest::TALK_PAD_SECONDS),
            "suggest::TALK_PAD_SECONDS",
        ),
        param(
            "preview.rateSeekGapMs",
            format!("{} ms", cut_play_back::RATE_SEEK_GAP_MS),
            "cut_play_back::RATE_SEEK_GAP_MS",
        ),
        param("preview.thumbBatch", ui::cut_tracks::THUMB_BATCH.to_string(), "ui::cut_tracks::THUMB_BATCH"),
        // §1's two Cut-page dividers (inventory §A's `paned 660/380`). §10 gives them no `P.` id, so each
        // takes a bare prefix the way `preview.thumbBatch` above does; both are what the page opens at,
        // not what it must stay at — either divider is draggable.
        param(
            "cut.panedTopPx",
            ui::cut_page_layout::SELF_SPLIT_TOP_PX.to_string(),
            "ui::cut_page_layout::SELF_SPLIT_TOP_PX",
        ),
        param(
            "cut.panedPreviewPx",
            ui::cut_page_layout::SELF_SPLIT_PREVIEW_PX.to_string(),
            "ui::cut_page_layout::SELF_SPLIT_PREVIEW_PX",
        ),
        // --- engineering: pixel reaches, band heights, zoom, undo, the wave cache ------------------
        // §6 lists these as a family and §10 gives them no `P.` ids of their own (§I's implicit constants),
        // so each is spelled after §D's name for it — as `machine.jpegQuality` does for the JPEG quality.
        param("layout.edgeGrabPx", num(cut_trim::EDGE_GRAB_PX), "cut_trim::EDGE_GRAB_PX"),
        // §6's third reach: grabbing a selection END on a picture row reaches further than in the band
        // (`// layout.picGripPx` at `cut_select::PIC_GRIP_PX`), read by `cut_select::grip_px` for every press.
        param("layout.picGripPx", num(cut_select::PIC_GRIP_PX), "cut_select::PIC_GRIP_PX"),
        param("layout.snapPx", num(cut_select::SNAP_PX), "cut_select::SNAP_PX"),
        param("layout.lineReachPx", num(cut_line::EDGE_REACH_PX), "cut_line::EDGE_REACH_PX"),
        param("layout.rulerPx", num(cut_screen::RULER_PX), "cut_screen::RULER_PX"),
        param("layout.selBandPx", num(cut_screen::SELECTION_BAND_PX), "cut_screen::SELECTION_BAND_PX"),
        param("layout.effectRowPx", num(fx_lane::lane_height_px(1)), "fx_lane::lane_height_px"),
        param("layout.waveLanePx", num(cut_screen::WAVE_LANE_PX), "cut_screen::WAVE_LANE_PX"),
        param("layout.laneGapPx", num(cut_screen::ROW_GAP_PX), "cut_screen::ROW_GAP_PX"),
        param("layout.zoomAtOpen", num(cut_screen::ZOOM_AT_OPEN), "cut_screen::ZOOM_AT_OPEN"),
        param("layout.zoomMaxPps", num(cut_screen::ZOOM_MAX), "cut_screen::ZOOM_MAX"),
        param("layout.zoomStep", num(cut_screen::ZOOM_STEP), "cut_screen::ZOOM_STEP"),
        param("layout.undoDepth", cut::UNDO_DEPTH.to_string(), "cut::UNDO_DEPTH"),
        // The waveform cache's format is its magic: a file that does not start with these four bytes is read
        // again rather than trusted, which is the whole reason it was renamed AWV2 → AWV3 → AWV4.
        param("layout.waveCacheMagic", String::from_utf8_lossy(wave::MAGIC).to_string(), "wave::MAGIC"),
        // --- §6's "colours" ------------------------------------------------------------------------
        // §10 counts the Cut page's tints among its engineering values (line 133's "timeline: … colours") but
        // gives none a `P.` id, so each takes the bare `cut.colour.` prefix the way `machine.` and `preview.`
        // rows do. Each is read by the painter that puts it on screen — `ui::cut_colours` is the only place one
        // is spelled, so moving a row here moves the ink.
        colour("cut.colour.keptTint", ui::cut_colours::KEPT_TINT, "ui::cut_colours::KEPT_TINT"),
        colour(
            "cut.colour.keptBorder",
            ui::cut_colours::KEPT_BORDER,
            "ui::cut_colours::KEPT_BORDER",
        ),
        colour(
            "cut.colour.selectionBlue",
            ui::cut_colours::SELECTION_BLUE,
            "ui::cut_colours::SELECTION_BLUE",
        ),
        colour(
            "cut.colour.insertViolet",
            ui::cut_colours::INSERT_VIOLET,
            "ui::cut_colours::INSERT_VIOLET",
        ),
        colour("cut.colour.scrim", ui::cut_colours::SCRIM, "ui::cut_colours::SCRIM"),
        colour(
            "cut.colour.speedTint",
            ui::cut_colours::SPEED_TINT,
            "ui::cut_colours::SPEED_TINT",
        ),
        colour(
            "cut.colour.markInGreen",
            ui::cut_colours::MARK_IN_GREEN,
            "ui::cut_colours::MARK_IN_GREEN",
        ),
        colour(
            "cut.colour.markOutRed",
            ui::cut_colours::MARK_OUT_RED,
            "ui::cut_colours::MARK_OUT_RED",
        ),
        colour(
            "cut.colour.edgeAmber",
            ui::cut_colours::EDGE_AMBER,
            "ui::cut_colours::EDGE_AMBER",
        ),
        colour("cut.colour.ground", ui::cut_colours::GROUND, "ui::cut_colours::GROUND"),
        colour(
            "cut.colour.laneGround",
            ui::cut_colours::LANE_GROUND,
            "ui::cut_colours::LANE_GROUND",
        ),
        colour(
            "cut.colour.gutterGround",
            ui::cut_colours::GUTTER_GROUND,
            "ui::cut_colours::GUTTER_GROUND",
        ),
        colour(
            "cut.colour.gutterEdge",
            ui::cut_colours::GUTTER_EDGE,
            "ui::cut_colours::GUTTER_EDGE",
        ),
        colour(
            "cut.colour.bandGround",
            ui::cut_colours::BAND_GROUND,
            "ui::cut_colours::BAND_GROUND",
        ),
        colour(
            "cut.colour.keptEdge",
            ui::cut_colours::KEPT_EDGE,
            "ui::cut_colours::KEPT_EDGE",
        ),
        colour(
            "cut.colour.filmGround",
            ui::cut_colours::FILM_GROUND,
            "ui::cut_colours::FILM_GROUND",
        ),
        colour("cut.colour.slate", ui::cut_colours::SLATE, "ui::cut_colours::SLATE"),
        colour(
            "cut.colour.joinWash",
            ui::cut_colours::JOIN_WASH,
            "ui::cut_colours::JOIN_WASH",
        ),
        colour(
            "cut.colour.joinFrame",
            ui::cut_colours::JOIN_FRAME,
            "ui::cut_colours::JOIN_FRAME",
        ),
        // Last on purpose: §6's own sentence ends with the colours and `cut_parameters_used` pins
        // `cut.colour.playhead` as the list's final row, so a new tint goes above it.
        colour(
            "cut.colour.playhead",
            ui::cut_colours::PLAYHEAD_RGB,
            "ui::cut_colours::PLAYHEAD_RGB",
        ),
    ]
}

/// §06-effects#6-parameters-used — every parameter an effect may be tuned by, in `spec/06-effects.md` §6's own
/// order, built from the constant each rule already reads.
///
/// A list of ids, not of numbers, so this holds none: move a bound in its module and the row moves with it. The
/// ids §05 §6 names as well — `P.eng.maxGain`, `P.policy.speedGapSeconds`, `P.machine.captionBatch`,
/// `P.policy.captionMinSeconds`, `P.eng.effectMinSurvivingSeconds`, and this flow's own defaults
/// (`effects.defaultGain`, `effects.defaultRate`, `effects.volumeMinSeconds`, `effects.proposedZoomHeight`) and
/// drawing numbers (`layout.effectRowPx`, `layout.snapPx`, `effects.previewRasterPx`, the `card.*` rows) — keep
/// their single row in [`cut`]: one row, one home, so this list is the rest of §06's rather than a copy of it.
///
/// Two of §06's items have no row and are absent on purpose. The label form's typed floor (0.4) belongs to §F3.7,
/// whose form has no constant in this tree yet — a number nothing reads has no module to live in. §I's `killMin` 32,
/// `fxMinBand` 30 and `fxGrab` 9 are absent for the same reason — no constant here holds them. Note that §I's
/// `fxMinBand` (30) is the PIXEL width from which a bar's ends become handles at all, and stays unowned;
/// `P.eng.effectMinSeconds`, the SECONDS floor a drag may shrink a band down to, is owned by [`fx_lane::MIN_BAND_SECONDS`] and has its row
/// below. The prototype's two similarly-named constants (`fxMinBand`, `fxMinDur`) are different rules, not one —
/// and its third, `fxMinSel` (0.2), is likewise a SECONDS floor rather than a pixel width: it is
/// `P.eng.effectMinSelectionSeconds`, held by [`cut_speed::MIN_MARKED_SECONDS`] and rowed twice on purpose
/// under that id and §06's own `effects.markedBandMinSeconds`.
pub fn effects() -> Vec<Param> {
    vec![
        // --- what a speed or volume effect may be asked to do --------------------------------------
        param("P.eng.minRate", num(cut_speed::MIN_RATE), "cut_speed::MIN_RATE"),
        param("P.eng.maxRate", num(cut_speed::MAX_RATE), "cut_speed::MAX_RATE"),
        // §10's own cell says this is "also the speed clamp floor", and `cut_speed::apply` reads it as exactly
        // that: the 0.5 a stop's Length must reach is the 0.5 the render will not go under, so one row serves both
        // of §6's mentions and the two cannot be tuned apart.
        param("P.eng.minClipSeconds", num(cutpass::MIN_CLIP_SECONDS), "tools::cutpass::MIN_CLIP_SECONDS"),
        param("P.eng.rampStepSeconds", num(cut_speed::RAMP_STEP_SECONDS), "cut_speed::RAMP_STEP_SECONDS"),
        // --- the lengths and fades an effect placed by hand starts with ------------------------------
        // §10 gives one row to a family of kinds, so the row spells the whole cell and names every constant behind
        // it: the only way a reader can see that the "2" is three rules' default rather than one.
        param(
            "P.policy.effectDefaultSeconds",
            format!(
                "zoom/text/svg {}; stop/speed/volume/label {}",
                num(fx_zoom::DEFAULT_SECONDS),
                num(fx_volume::LINE_SECONDS)
            ),
            "fx_zoom::DEFAULT_SECONDS + fx_volume::LINE_SECONDS",
        ),
        param(
            "P.policy.effectDefaultFades",
            format!(
                "zoom {}; text/svg {}; volume {}; stop {}",
                num(fx_zoom::GLIDE_SECONDS),
                num(fx_text::FADE_SECONDS),
                num(fx_volume::FADE_SECONDS),
                num(cut_speed::STOP_FADE_SECONDS)
            ),
            "fx_zoom::GLIDE_SECONDS + fx_text::FADE_SECONDS + fx_volume::FADE_SECONDS + cut_speed::STOP_FADE_SECONDS",
        ),
        // The height a proposed zoom is given, spelled twice on purpose: §10 has this row and §F3.11 names the same
        // number under `effects.proposedZoomHeight`, which [`cut`] already carries. Both rows read one constant, so
        // the two spellings cannot disagree about what 0.6 is.
        param(
            "P.policy.suggestedZoomHeight",
            num(cut_effects_pass::ZOOM_HEIGHT),
            "cut_effects_pass::ZOOM_HEIGHT",
        ),
        // How many decorations an edit is expected to carry. §10 gives this a row ("three or four per five
        // minutes", home `fxRules`), so it is catalogued -- and its value is WORDING, the clause the shipped
        // effects prompt states, not a number anything is multiplied by. The guide's own constants are
        // `DENSITY_GUIDE` / `DENSITY_PER_FIVE_MINUTES`; what refuses a runaway reply is
        // `P.machine.maxProposedEffects` below, never this.
        param(
            "P.policy.decorationDensity",
            "\"three or four per five minutes\"".to_string(),
            "cut_effects_pass::DENSITY_GUIDE",
        ),
        // The cap on effects per cut reply. §10 spells its own qualification into the row — "effectively
        // none: the prompt decides" — and that is what the constant says too: this bounds a runaway
        // answer, it does not curate the edit. Counted through `counts_against_effect_cap`, which
        // exempts a whole-segment rate per 05-cut#8.
        param(
            "P.machine.maxProposedEffects",
            cut_effects_pass::MAX_PROPOSED_EFFECTS.to_string(),
            "cut_effects_pass::MAX_PROPOSED_EFFECTS",
        ),
        // --- the floors each form holds to when Apply is pressed --------------------------------------
        param("effects.zoomFloorSeconds", num(fx_zoom::MIN_SECONDS), "fx_zoom::MIN_SECONDS"),
        param("effects.textMinSeconds", num(fx_text::MIN_SECONDS), "fx_text::MIN_SECONDS"),
        // Not a form floor but the one §6 names outright ("the 0.2 s floor under which a band is not a marked
        // stretch"): below it a drag is a slipped click, and ⏩ Speed falls through to the line.
        param("effects.markedBandMinSeconds", num(cut_speed::MIN_MARKED_SECONDS), "cut_speed::MIN_MARKED_SECONDS"),
        // --- F3.8: the lane's own thresholds (§A.9) -----------------------------------------------
        // §10 gives these no `P.` rows, so each takes the bare prefix of the rule that reads it
        // (`fx_band`), exactly as `effects.labelMinSeconds` does for F3.7. The numbers live in `fx_band`;
        // these rows only catalogue them.
        param(
            "effects.gripMinPx",
            num(fx_band::GRIP_MIN_PX),
            "fx_band::GRIP_MIN_PX",
        ),
        param(
            "effects.killMinPx",
            num(fx_band::KILL_MIN_PX),
            "fx_band::KILL_MIN_PX",
        ),
        param(
            "effects.undoPushPx",
            num(fx_band::UNDO_PUSH_PX),
            "fx_band::UNDO_PUSH_PX",
        ),
        param(
            "effects.formDebounceMs",
            num(fx_band::DEBOUNCE_MS),
            "fx_band::DEBOUNCE_MS",
        ),
        param(
            "machine.holdReleaseSeconds",
            num(fx_band::HOLD_RELEASE_SECONDS),
            "fx_band::HOLD_RELEASE_SECONDS",
        ),
        // §10's own row for that same 0.2 s, spelled with its `P.` id: "timeline needed under the band before ⏩
        // Speed treats it as a chosen stretch". One constant answers both ids — `cut_speed::press` reads
        // MIN_MARKED_SECONDS once — so the two spellings cannot be tuned apart, which is why this row points at
        // the same place rather than holding a second copy of 0.2. (Same shape as the
        // `P.policy.suggestedZoomHeight` / `effects.proposedZoomHeight` pair above.)
        param(
            "P.eng.effectMinSelectionSeconds",
            num(cut_speed::MIN_MARKED_SECONDS),
            "cut_speed::MIN_MARKED_SECONDS",
        ),
        // The lane's own drag floor: however hard the hand shrinks a band from either end, `fx_lane::drag_end` will
        // not return less than this. §10 files it as an engineering constant; the rule that reads it is here.
        param("P.eng.effectMinSeconds", num(fx_lane::MIN_BAND_SECONDS), "fx_lane::MIN_BAND_SECONDS"),
        // --- what the lane and the picture are drawn with ---------------------------------------------
        // §6's "grip/kill widths" is one pair here and not two: only the grip has a constant in this tree, and the
        // kill width is part of §I's implicit list that nothing reads. The "snap 8/10 px" pair is split across the
        // two lists for the same reason as its name — 8 px pulls a lane band onto a word edge ([`cut`]'s
        // `layout.snapPx`) and 10 px drags a text box over a picture.
        param("layout.gripPx", num(cut_select::GRIP_PX), "cut_select::GRIP_PX"),
        param("effects.snapPx", num(fx_text::SNAP_PX), "fx_text::SNAP_PX"),
        // The fitting rule's own arithmetic: a caption is measured in em, so these three are what decide whether
        // the words fit the box the hand drew.
        param("effects.textAdvance", num(fx_text::CHAR_ADVANCE_EM), "fx_text::CHAR_ADVANCE_EM"),
        param("effects.textLineHeight", num(fx_text::LINE_HEIGHT_EM), "fx_text::LINE_HEIGHT_EM"),
        param("effects.textAscent", num(fx_text::ASCENT_EM), "fx_text::ASCENT_EM"),
        param("effects.textMinPoints", num(fx_text::MIN_POINTS), "fx_text::MIN_POINTS"),
        param("effects.textMaxLines", fx_text::MAX_LINES.to_string(), "fx_text::MAX_LINES"),
        // §6's "edge dilation" is the radius; how dark and in how many directions it is drawn are the same rule's
        // shape rather than a tuning value, so they stay uncatalogued.
        param("effects.edgeRadius", num(fx_text::EDGE_RADIUS_EM), "fx_text::EDGE_RADIUS_EM"),
        // F3.9's cleaning rule as the captions prompt states it ("the swearing kept"): a policy the
        // model reads as wording, not a number the app applies. Read from the project's own default.
        param(
            "P.policy.keepSwearing",
            project::Policy::default().keep_swearing.value.to_string(),
            "project::Policy::default — keep_swearing",
        ),
        // The language the recordings are spoken in: the speech recogniser's and the voice's. F0.7 reads it from
        // the User Context; English when the context does not say.
        param(
            "P.policy.language",
            project::Policy::default().language.value,
            "project::Policy::default — language",
        ),
    ]
}

/// Every number the Narrate page draws or waits with, in §1's order. The rows live in
/// [`crate::params_narrate`] — this file is at its size budget, and moving the section changed neither
/// its ids, its spellings, nor their order.
pub fn narrate() -> Vec<Param> {
    crate::params_narrate::rows()
}

/// §08 Produce's own rows (§10): the five numbers that build a cue and the size of one translation request.
/// They live in `produce_subtitles`, which is the module whose rules read them, so this only catalogues them
/// — the same split as every other section here.
pub fn produce() -> Vec<Param> {
    vec![
        // --- cue building (§10's four `sub*` rows) ------------------------------------------------------
        param(
            "P.policy.subtitleBreakSeconds",
            num(produce_subtitles::SUBBREAK_SECONDS),
            "produce_subtitles::SUBBREAK_SECONDS",
        ),
        param(
            "P.policy.subtitleRowChars",
            produce_subtitles::ROW_CHARS.to_string(),
            "produce_subtitles::ROW_CHARS",
        ),
        param(
            "P.policy.subtitleMaxSeconds",
            num(produce_subtitles::MAX_SECONDS),
            "produce_subtitles::MAX_SECONDS",
        ),
        // §10 lists the hold and the floor beside the other three; both are S2's tidy, not S1's grouping.
        param(
            "P.policy.subtitleHoldSeconds",
            num(produce_subtitles::HOLD_SECONDS),
            "produce_subtitles::HOLD_SECONDS",
        ),
        param(
            "P.policy.subtitleMinSeconds",
            num(produce_subtitles::MIN_SECONDS),
            "produce_subtitles::MIN_SECONDS",
        ),
        // --- one translation request's size (§10: new; the prototype sent the whole track) ---------------
        param(
            "P.machine.translateBatch",
            produce_subtitles::BATCH.to_string(),
            "produce_subtitles::BATCH",
        ),
        // --- §F5.7's page runs: the cap an upload puts on a picture, and the snap grid its words box stops on.
        param(
            "P.eng.thumbnailJPEGMax",
            produce_runs::JPEG_MAX_BYTES.to_string(),
            "produce_runs::JPEG_MAX_BYTES",
        ),
        param(
            "P.policy.publishWordsSnapPx",
            produce_runs::words_snap_px().to_string(),
            "produce_runs::words_snap_px",
        ),
        // --- §08 §4's mix: the bed under the narration, and the two targets the final audio is set to ------
        param(
            "P.policy.gameVolume",
            num(project::Produce::default().game_volume),
            "project::Produce::default — game_volume",
        ),
        param(
            "P.eng.loudness",
            loudnorm_spelled(produce_render::LOUDNORM),
            "produce_render::LOUDNORM",
        ),
        param(
            "P.eng.clipLimiter",
            limiter_spelled(),
            "produce_render::LIMITER",
        ),
        // §10 spells this row as a whole rule, `0.02·height, min 4`, so both constants `blur_sigma`
        // reads go into the spelling; a change to either shows up in the catalogue without a hand edit.
        param(
            "P.eng.blurSigma",
            blur_sigma_spelled(),
            "produce_render::blur_sigma",
        ),
        // --- §08 §4's thumbnail: its size, its band, and how many frames the row holds ---------------------
        // (`P.eng.narrationMaxTempo` used to sit here: §07's §4 names it too, so it now has one home in
        // `narrate()`, still pointing at `produce_render::MAX_TEMPO`.)
        param(
            "P.machine.thumbnailLongSide",
            produce_screen::THUMB_LONG_SIDE.to_string(),
            "produce_screen::THUMB_LONG_SIDE",
        ),
        param(
            "P.eng.titleBand",
            title_band_spelled(),
            "project::TitleBox::default",
        ),
        param(
            "P.policy.publishFrames",
            produce_screen::FIRST_IMAGES.to_string(),
            "produce_screen::FIRST_IMAGES",
        ),
        param(
            "P.eng.publishMaxFrames",
            produce_screen::MAX_IMAGES.to_string(),
            "produce_screen::MAX_IMAGES",
        ),
        param(
            "P.machine.briefMaxChars",
            produce_upload::BRIEF_MAX_CHARS.to_string(),
            "produce_upload::BRIEF_MAX_CHARS",
        ),
        // --- §4's engineering sentence: values §10 keeps in prose, so each takes the bare `produce.` prefix
        // and spells itself out of the constant or function its rule reads (`params_produce` builds the spelling).
        param(
            "produce.exportLadder",
            crate::params_produce::export_ladder(),
            "produce_runs::JPEG_QUALITIES + JPEG_RETRY_LIMIT",
        ),
        param(
            "produce.posterJPEGQuality",
            crate::params_produce::poster_quality(),
            "produce_embed::POSTER_JPEG_QUALITY + POSTER_QSCALE",
        ),
        param(
            "produce.sdWaitSeconds",
            crate::params_produce::sd_timeouts(),
            "bodies::sd_waits",
        ),
        param(
            "produce.seekBeforeInput",
            crate::params_produce::seek_rule(),
            "produce_render::SEEK_FLAG + INPUT_FLAG",
        ),
        param(
            "produce.mp4StreamFlags",
            crate::params_produce::mp4_flags(),
            "produce_render::MP4_STREAM_FLAGS + MP4_START_FLAGS",
        ),
    ]
}

/// §10 spells `P.eng.loudness` as `I -14, TP -1.5, LRA 11`, which is the filter string read backwards: the
/// three numbers are taken out of [`produce_render::LOUDNORM`] so the row cannot disagree with what ffmpeg
/// is actually sent, and only their spelling lives here.
/// §10's spelling of a `loudnorm` target: the three numbers taken out of the filter string the
/// renderer is actually sent, so a row cannot disagree with the command line.
pub(crate) fn loudnorm_spelled(filter: &str) -> String {
    let tail = filter.split("loudnorm=").nth(1).unwrap_or_default();
    // `I=-14:TP=-1.5:LRA=11` → `I -14, TP -1.5, LRA 11`: pairs apart on commas, each value off its key.
    tail.split(':')
        .map(|pair| pair.replace('=', " "))
        .collect::<Vec<_>>()
        .join(", ")
}

/// §10 spells `P.eng.clipLimiter` as `−1 dBFS (0.891)`: the decibels are what a person reads, the amplitude
/// is what `alimiter` takes. Both come out of [`produce_render::LIMITER`] — −1 dBFS is the amplitude 0.891,
/// so spelling one from the other keeps them one fact.
fn limiter_spelled() -> String {
    let amplitude = produce_render::LIMITER
        .split("limit=")
        .nth(1)
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_string();
    // 20·log10(0.891) = −1 dBFS, rounded to the whole decibel §10 writes.
    let db = (20.0 * amplitude.parse::<f64>().unwrap_or(1.0).log10()).round();
    format!("{db} dBFS ({amplitude})")
}

/// §10's `P.eng.titleBand` spelling, `{0.5, 0.25, 1, 0.4}` — the same four numbers [`project::TitleBox`]
/// holds, printed without trailing zeros so the row reads as the spec writes it.
fn title_band_spelled() -> String {
    let band = project::TitleBox::default();
    format!(
        "{{{}, {}, {}, {}}}",
        num(band.cx),
        num(band.cy),
        num(band.wf),
        num(band.hf)
    )
}

/// §10 spells `P.eng.blurSigma` as `0.02·height, min 4`: a rule with two halves, so both constants
/// [`produce_render::blur_sigma`] reads land in the spelling rather than only the fraction. The floor is
/// what keeps a small frame blurred at all, and a row that hid it would show a number the render does not
/// obey. `·` is U+00B7, the character §10 writes.
fn blur_sigma_spelled() -> String {
    format!(
        "{}\u{b7}height, min {}",
        num(produce_render::BLUR_SIGMA_FRACTION),
        num(produce_render::BLUR_SIGMA_MIN)
    )
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
    } else if id.starts_with("P.project.") {
        Family::Project
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
    (
        degraded::SILENCE_THRESHOLD_DB,
        degraded::SILENCE_MIN_SECONDS,
    )
}
