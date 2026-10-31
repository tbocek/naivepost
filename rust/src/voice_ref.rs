//! F4.6 Choose the voice and build the reference — `spec/07-narrate.md` F4.6, steps S1–S5.

use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::narrate_data;
use crate::narrate_screen::CAPTIONS;

// --- the numbers §10 lists for this flow -----------------------------------------------------

/// P.eng.refMinTakeSeconds: the shortest stretch the automatic pick will use. Below this the model
/// clones from too little audio and the voice wavers between lines.
pub const REF_MIN_TAKE_SECONDS: f64 = 5.0;

pub const REF_WANT_SECONDS: f64 = crate::narrate_details::REFERENCE_WANTED_SECONDS;

/// P.machine.refTakeMax: the most pieces the automatic pick may stitch together. More than three and
/// the reference is a patchwork whose seams the clone inherits.
pub const REF_TAKE_MAX: usize = 3;

/// P.eng.refMinWordsPerSecond: how dense a turn must be to be worth cloning. A sparse turn is the
/// diarizer guessing over music or a door slam.
pub const REF_MIN_WORDS_PER_SECOND: f64 = 1.5;

/// P.eng.refPadSeconds: how far another speaker has to stay clear of a candidate take. It keeps the clone
/// from hearing the other person's last word inside the reference.
pub const REF_PAD_SECONDS: f64 = 2.0;

/// The rate ceiling a picked wav must respect to be readable by the speech server (§F4.6 S3).
const READABLE_MAX_RATE: u32 = 48_000;

/// `WAVE_FORMAT_EXTENSIBLE` — the header tag ffmpeg writes above 48 kHz or for non-standard channel
/// masks, which the speech server refuses outright.
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

// --- S1: picking the voice -------------------------------------------------------------------

/// What a voice choice resolved to. `Chosen` carries the sentence S1 says out loud; `Refused` carries
/// the spec's refusal verbatim, which the page prints unchanged rather than paraphrasing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceChoice {
    /// Captions only: nothing is spoken, and S1 wants that said rather than left inferred from a grey
    /// control.
    Silent(String),
    /// The voice is usable; `said` is the switch report.
    Chosen { id: String, said: String },
    /// The voice cannot be used; `why` is the sentence to print.
    Refused(String),
}

/// S1: resolve a picked voice against what Prepare actually tagged and what the voices folder holds.
pub fn pick_voice(id: &str, narrators: usize, voice_files: &[String], dir: &str) -> VoiceChoice {
    if id == CAPTIONS {
        return VoiceChoice::Silent(SILENT_SWITCH.to_string());
    }
    if let Some(slot) = crate::narrate_rules::narrator_slot(id) {
        if slot < 1 || slot > narrators {
            return VoiceChoice::Refused(untagged_narrator(slot));
        }
        return VoiceChoice::Chosen {
            id: id.to_string(),
            said: narrator_switch(slot),
        };
    }
    if voice_files.iter().any(|name| name == id) {
        return VoiceChoice::Chosen {
            id: id.to_string(),
            said: file_switch(id),
        };
    }
    VoiceChoice::Refused(vanished_voice(id, dir))
}

/// S1's refusal when a slot has no recording tagged on the Prepare step.
pub fn untagged_narrator(slot: usize) -> String {
    format!(
        "narrator {slot} is not tagged on the Prepare step \u{2014} tag a recording, or pick another voice"
    )
}

/// S1's refusal when a voices-folder file has gone missing since it was chosen.
pub fn vanished_voice(name: &str, dir: &str) -> String {
    format!("voice {name} is no longer in {dir} \u{2014} pick another")
}

/// S4: which setting changed, so the invalidation knows whether the base survives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Changed {
    /// A different voice: the base was cut from something else, so both files go.
    Voice,
    /// The pitch slider: only the shifted copy is wrong; the base re-shifts.
    Pitch,
    /// The take set: the base was cut from those seconds, so both files go.
    Takes,
}

pub fn invalidate_for_change(tree: &Tree, what: Changed) -> usize {
    let keep_base = matches!(what, Changed::Pitch);
    narrate_data::drop_reference(tree, keep_base)
}

/// S1's report for captions only.
pub const SILENT_SWITCH: &str =
    "no audio \u{2014} the narration is written and timed, never spoken";

/// S1's report for a narrator slot.
pub fn narrator_switch(slot: usize) -> String {
    format!(
        "voice: narrator {slot}'s \u{2014} it is re-cut from the recording on the next line spoken"
    )
}

/// S1's report for a voices-folder file.
pub fn file_switch(name: &str) -> String {
    format!("voice: {name} \u{2014} \u{25b6} beside the sample plays it")
}

/// S1's report when installing a picked file failed; the reason goes to the log.
pub const INSTALL_FAILED: &str = "could not install that voice \u{2014} see log";

// --- S2: choosing the seconds the reference is cut from --------------------------------------

/// One candidate piece for the automatic reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub start: f64,
    pub end: f64,
}

impl Piece {
    pub fn seconds(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
}

/// What the automatic pick decided.
#[derive(Debug, Clone, PartialEq)]
pub enum ReferencePlan {
    /// Hand-picked takes win outright: no cap, no diarization needed (S2).
    HandPicked(Vec<Piece>),
    /// Pieces chosen from the diarized turns, within the wanted length and the piece cap.
    Automatic(Vec<Piece>),
    /// No usable reference; `why` is the spec's sentence.
    Unavailable(String),
}

/// S2: decide what the reference is cut from, against the session's own Prepare output.
pub fn plan_reference(
    takes: &[(f64, f64)],
    turns: &[(f64, f64, u32)],
    // `&impl Fn` rather than `impl Fn`: after the plan is made, `recut_for_voice` asks this same counter for
    // the pieces it kept, to put §6's real word count in the build log. Taking it by value would consume it.
    words_in: &impl Fn(f64, f64) -> usize,
    // §6 (`Automatic ranking`): "narrowed to their words" — the transcript's own span inside a window, so
    // the diarizer's turn can be cut down to what was actually said. `None` = no words there at all.
    words_span: &impl Fn(f64, f64) -> Option<(f64, f64)>,
    narrator_speaker: u32,
) -> ReferencePlan {
    if !takes.is_empty() {
        return ReferencePlan::HandPicked(
            takes
                .iter()
                .map(|&(start, end)| Piece { start, end })
                .collect(),
        );
    }
    if turns.is_empty() {
        return ReferencePlan::Unavailable(NO_DIARIZATION.to_string());
    }

    let mut candidates: Vec<(Piece, usize)> = Vec::new();
    let mut sparse: Vec<(Piece, usize)> = Vec::new();
    for &(start, end, speaker) in turns {
        if speaker != narrator_speaker {
            continue;
        }
        let clear = |other: &(f64, f64, u32)| {
            other.2 != narrator_speaker
                && other.1 > start - REF_PAD_SECONDS
                && other.0 < end + REF_PAD_SECONDS
        };
        if turns.iter().any(clear) {
            continue;
        }
        let Some((from, until)) = words_span(start, end) else {
            continue;
        };
        if until - from < REF_MIN_TAKE_SECONDS {
            continue;
        }
        let piece = Piece {
            start: from,
            end: until,
        };
        let words = words_in(from, until);
        if (words as f64) < (until - from) * REF_MIN_WORDS_PER_SECOND {
            sparse.push((piece, words));
            continue;
        }
        candidates.push((piece, words));
    }
    if candidates.is_empty() {
        if sparse.is_empty() {
            return ReferencePlan::Unavailable(NO_CLEAN_STRETCH.to_string());
        }
        // §6's exception: every solo stretch is thin, so the thin ones ARE the reference.
        candidates = sparse;
    }

    // §F4.6's ranking: by the WORDS said in each stretch, its length only breaking a tie. Ranking by length
    // alone would take a slow rambling take over a dense one carrying twice the speech, and the clone learns
    // the voice, not the pacing, so density is what matters.
    candidates.sort_by(|(a, a_words), (b, b_words)| {
        b_words.cmp(a_words).then(
            b.seconds()
                .partial_cmp(&a.seconds())
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    let mut chosen: Vec<Piece> = Vec::new();
    let mut total = 0.0;
    for (piece, _) in candidates {
        if chosen.len() >= REF_TAKE_MAX || total >= REF_WANT_SECONDS {
            break;
        }
        total += piece.seconds();
        chosen.push(piece);
    }
    chosen.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ReferencePlan::Automatic(chosen)
}

/// The turn's own seconds: the span a speaker holds in the diarization.
pub fn turn_seconds(turns: &[(f64, f64, u32)], speaker: u32) -> f64 {
    turns
        .iter()
        .filter(|(_, _, who)| *who == speaker)
        .map(|(start, end, _)| (end - start).max(0.0))
        .sum()
}

pub fn ranked_speaker(turns: &[(f64, f64, u32)]) -> Option<u32> {
    let mut best: Option<(f64, u32)> = None;
    for speaker in turns
        .iter()
        .map(|(_, _, who)| *who)
        .collect::<std::collections::BTreeSet<u32>>()
    {
        let held = turn_seconds(turns, speaker);
        if best.is_none_or(|(seconds, _)| held > seconds) {
            best = Some((held, speaker));
        }
    }
    best.map(|(_, speaker)| speaker)
}

/// S2's refusal when there is no diarization to choose from at all.
pub const NO_DIARIZATION: &str =
    "no diarization for X -- run Prepare, or pick the seconds by hand under the video";

/// S2's refusal when diarization exists but nothing clean and long enough turned up.
pub const NO_CLEAN_STRETCH: &str = "no clean solo stretch found for the voice reference";

/// S2's refusal naming the slot when nothing is tagged for it.
pub fn nothing_tagged(slot: usize) -> String {
    format!("nothing is tagged as narrator {slot} on the Prepare step")
}

// --- S3: the real level-and-shift leg -------------------------------------------------------

/// Where the two reference files landed.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    pub base: PathBuf,
    pub served: PathBuf,
    /// The sentences worth printing while building (a re-cut notice, a rename).
    pub said: Vec<String>,
    /// §6 (`Reference build`): how much audio the build cut, and whether a hand or the ranking chose it.
    pub seconds: f64,
    /// Words in the chosen pieces; 0 when a hand picked them (§6 names takes instead of words then).
    pub words: usize,
    /// How many pieces came from the take band; 0 means the automatic wording.
    pub hand_picked: usize,
}

pub fn level_args(input: &str, out: &str) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        narrate_data::REF_SAMPLE_RATE.to_string(),
        "-af".into(),
        narrate_data::REF_LOUDNESS.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        out.to_string(),
    ]
}

/// The ffmpeg argument vector that shifts the pitch with formants kept (`rubberband`'s `formant=y`).
pub fn shift_args(input: &str, out: &str, semitones: f64) -> Vec<String> {
    let ratio = 2f64.powf(semitones / 12.0);
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string(),
        "-af".into(),
        format!("rubberband=pitch={ratio:.6}:formant=preserved"),
        "-c:a".into(),
        "pcm_s16le".into(),
        out.to_string(),
    ]
}

pub fn wav_header(path: &Path) -> Option<(u16, u32)> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 28 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let fmt_at = (12..bytes.len().saturating_sub(4)).find(|&i| &bytes[i..i + 4] == b"fmt ")?;
    let body = fmt_at + 8;
    if bytes.len() < body + 16 {
        return None;
    }
    let tag = u16::from_le_bytes([bytes[body], bytes[body + 1]]);
    let rate = u32::from_le_bytes([
        bytes[body + 4],
        bytes[body + 5],
        bytes[body + 6],
        bytes[body + 7],
    ]);
    Some((tag, rate))
}

const WAV_HEAD_BYTES: u64 = 64 * 1024;

pub fn wav_seconds(path: &Path) -> Option<f64> {
    use std::io::Read;
    let size = std::fs::metadata(path).ok()?.len();
    let mut head = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(WAV_HEAD_BYTES)
        .read_to_end(&mut head)
        .ok()?;
    let (_, rate) = wav_header(path)?;
    let fmt = (12..head.len().saturating_sub(4)).find(|&i| &head[i..i + 4] == b"fmt ")? + 8;
    let channels = u16::from_le_bytes([*head.get(fmt + 2)?, *head.get(fmt + 3)?]).max(1) as u64;
    let bits = match u16::from_le_bytes([*head.get(fmt + 14)?, *head.get(fmt + 15)?]) {
        0 => 16,
        bits => bits as u64,
    };
    // Whatever chunks come before it, the sound starts after `data`. A streamed file's size field is unset, so
    // the sound is whatever the file holds past that point.
    let start = (12..head.len().saturating_sub(4))
        .find(|&i| &head[i..i + 4] == b"data")
        .map_or(44, |at| at as u64 + 8);
    let bytes_per_second = channels * (rate.max(1) as u64) * bits / 8;
    Some(size.saturating_sub(start) as f64 / bytes_per_second as f64)
}

/// Whether a picked wav needs re-cutting before the server will read it: no readable header, the
/// extensible format tag, or a rate above 48 kHz.
pub fn needs_re_cut(path: &Path) -> bool {
    match wav_header(path) {
        None => true,
        Some((WAVE_FORMAT_EXTENSIBLE, _)) => true,
        Some((_, rate)) => rate > READABLE_MAX_RATE,
    }
}

/// S3's notice that a picked file is being cut again, naming the file.
pub fn recut_notice(file: &str) -> String {
    format!("voice reference {file} is not a wav the server reads -- cutting it again")
}

pub fn recut_for_voice(
    tree: &Tree,
    ffmpeg: &str,
    source: &str,
    turns: &[(f64, f64, u32)],
    words_in: impl Fn(f64, f64) -> usize,
    // §6's narrowing, asked of the same transcript the counter reads: see `plan_reference`.
    words_span: impl Fn(f64, f64) -> Option<(f64, f64)>,
) -> Result<Built, String> {
    // S3's repair runs BEFORE anything is cut: an older project holds `voice_ref.wav` with no base,
    // and that lone file is the reference's history. Cutting over it would lose it.
    let mut said = Vec::new();
    if let Some(renamed) = adopt_existing_reference(tree) {
        said.push(format!(
            "voice reference had no base; renamed {} to {}",
            tree.voice_ref_wav().display(),
            renamed.display()
        ));
    }

    let base = tree.voice_ref_base_wav();
    let served = tree.voice_ref_wav();
    // S2's first question is not about diarization at all but about the project: the spec's "nothing
    // is tagged as narrator N on the Prepare step" is answered of the recording this build was asked
    // to cut from, and it comes BEFORE the rule because the rule cannot see the project file.
    let slot = voice_slot(tree);
    if let Some(slot) = slot.filter(|_| nothing_is_tagged(tree, source)) {
        return Err(nothing_tagged(slot as usize));
    }
    let picked = picked_takes(tree, source);
    let takes: Vec<(f64, f64)> = picked.iter().map(|(_, s, e)| (*s, *e)).collect();
    let plan = plan_reference(
        &takes,
        turns,
        &words_in,
        &words_span,
        ranked_speaker(turns).unwrap_or(slot.unwrap_or(1)),
    );
    let (pieces, hand_picked) = match &plan {
        ReferencePlan::HandPicked(pieces) => (pieces.clone(), pieces.len()),
        ReferencePlan::Automatic(pieces) => (pieces.clone(), 0usize),
        ReferencePlan::Unavailable(why) => return Err(why.clone()),
    };
    let files: Vec<String> = if hand_picked > 0 {
        picked.iter().map(|(file, _, _)| file.clone()).collect()
    } else {
        vec![source.to_string(); pieces.len()]
    };
    let words: usize = if hand_picked == 0 {
        pieces
            .iter()
            .map(|piece| words_in(piece.start, piece.end))
            .sum()
    } else {
        0
    };
    let seconds: f64 = pieces.iter().map(|piece| piece.seconds()).sum();

    if pieces.len() == 1 {
        let piece = &pieces[0];
        let args = level_piece_args(
            &files[0],
            piece.start,
            piece.end,
            &base.display().to_string(),
        );
        run_ffmpeg(ffmpeg, &args)?;
    } else {
        // Each piece levelled on its own, so the loudness target is met per stretch rather than across
        // silences, then one concat of the levelled parts. The list file's paths are quoted because
        // a recording named `take, 2.wav` would otherwise split on its own comma.
        let mut parts: Vec<PathBuf> = Vec::new();
        for (index, piece) in pieces.iter().enumerate() {
            let part = narrate_data::reference_part(tree, index);
            let args = level_piece_args(
                &files[index],
                piece.start,
                piece.end,
                &part.display().to_string(),
            );
            run_ffmpeg(ffmpeg, &args)?;
            parts.push(part);
        }
        let list = narrate_data::reference_list(tree);
        let body: String = parts
            .iter()
            .map(|part| {
                format!(
                    "file '{}'\n",
                    part.display().to_string().replace('\'', "'\\''")
                )
            })
            .collect();
        std::fs::write(&list, body).map_err(|err| format!("{}: {err}", list.display()))?;
        let args = concat_args(&list.display().to_string(), &base.display().to_string());
        run_ffmpeg(ffmpeg, &args)?;
    }

    let semitones = narrate_data::read_pitch(tree);
    if semitones.abs() < f64::EPSILON {
        copy_file(&base, &served)?;
    } else {
        let shift = shift_args(
            &base.display().to_string(),
            &served.display().to_string(),
            semitones,
        );
        run_ffmpeg(ffmpeg, &shift)?;
    }
    Ok(Built {
        base,
        served,
        said,
        seconds,
        words,
        hand_picked,
    })
}

/// §6 (`Reference build`): the line a finished build leaves. The flow that ran ffmpeg prints it, because
/// only that flow knows which recording was cut from — `base_name` is that file's name, not a path.
pub fn built_log(built: &Built, base_name: &str) -> String {
    crate::narrate_details::reference_build_log(
        built.seconds,
        built.words,
        built.hand_picked,
        base_name,
    )
}

fn nothing_is_tagged(tree: &Tree, source: &str) -> bool {
    let Ok(project) = crate::project::load(tree.dir()) else {
        return false;
    };
    !project.sources.iter().any(|project_source| {
        project_source.narrator != 0 && same_file(&project_source.path, source)
    })
}

/// The narrator slot the project's voice names, `None` for a voices-folder file.
pub fn voice_slot(tree: &Tree) -> Option<u32> {
    crate::narrate_rules::narrator_slot(&narrate_data::read_voice(tree)).map(|slot| slot as u32)
}

/// Whether a person picked seconds for this voice, which beats everything automatic.
pub fn has_picked_takes(tree: &Tree, source: &str) -> bool {
    !picked_takes(tree, source).is_empty()
}

fn same_file(a: &str, b: &str) -> bool {
    let name = |p: &str| {
        Path::new(p.strip_prefix("project:").unwrap_or(p))
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    !a.is_empty() && !b.is_empty() && name(a) == name(b)
}

fn picked_takes(tree: &Tree, source: &str) -> Vec<(String, f64, f64)> {
    let all = narrate_data::load_takes(tree).unwrap_or_default();
    let slot = voice_slot(tree);
    let root = std::env::current_dir().unwrap_or_default();
    let mut files: Vec<String> = crate::project::load(tree.dir())
        .map(|project| {
            project
                .sources
                .iter()
                .filter(|tagged| Some(tagged.narrator) == slot)
                .map(|tagged| {
                    crate::layout::resolve(&root, tree.dir(), &tagged.path)
                        .display()
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default();
    if files.is_empty() {
        files.push(source.to_string());
    }
    files.sort_by_key(|file| narrate_data::take_base(Path::new(file)));
    files
        .iter()
        .flat_map(|file| {
            all.get(&narrate_data::take_base(Path::new(file)))
                .into_iter()
                .flatten()
                .map(move |take| (file.clone(), take.s, take.e))
        })
        .collect()
}

pub fn level_piece_args(input: &str, start: f64, end: f64, out: &str) -> Vec<String> {
    let mut args = level_args(input, out);
    let at = args.iter().position(|a| a == "-i").unwrap_or(0);
    args.splice(
        at..at,
        [
            "-ss".to_string(),
            format!("{start:.3}"),
            "-to".to_string(),
            format!("{end:.3}"),
        ],
    );
    args
}

/// S3's stitching pass: ffmpeg's own concat demuxer over the list file, streams copied (the parts are
/// already mono 48 kHz pcm from the level pass, so there is nothing left to re-encode).
pub fn concat_args(list: &str, out: &str) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list.to_string(),
        "-c".into(),
        "copy".into(),
        out.to_string(),
    ]
}

/// S3: build the reference from `source` through the configured ffmpeg binary.
pub fn build(tree: &Tree, source: &str, ffmpeg: &str, semitones: f64) -> Result<Built, String> {
    let base = tree.voice_ref_base_wav();
    let served = tree.voice_ref_wav();
    let mut said = Vec::new();

    let stage_source = source.to_string();
    if needs_re_cut(Path::new(source)) {
        said.push(recut_notice(source));
    }

    let level = level_args(&stage_source, &base.display().to_string());
    run_ffmpeg(ffmpeg, &level)?;

    if semitones.abs() < f64::EPSILON {
        copy_file(&base, &served)?;
    } else {
        let shift = shift_args(
            &base.display().to_string(),
            &served.display().to_string(),
            semitones,
        );
        run_ffmpeg(ffmpeg, &shift)?;
    }
    Ok(Built {
        // `build` levels a whole recording rather than chosen stretches, so the build's own length is what
        // it cut and no hand picked anything: §6's wording takes the seconds from the file that was made.
        seconds: wav_seconds(&base).unwrap_or(0.0),
        words: 0,
        hand_picked: 0,
        base,
        served,
        said,
    })
}

/// S3's last sentence: a project holding `voice_ref.wav` with no base renames the former to the base.
pub fn adopt_existing_reference(tree: &Tree) -> Option<PathBuf> {
    let served = tree.voice_ref_wav();
    let base = tree.voice_ref_base_wav();
    if served.exists() && !base.exists() {
        std::fs::rename(&served, &base).ok()?;
        return Some(base);
    }
    None
}

/// S5's key for one sample: the voice and the words, and nothing else.
pub fn sample_key(voice: &str, text: &str) -> String {
    let carrier = crate::narration::Entry {
        text: text.to_string(),
        ..Default::default()
    };
    crate::narration::key_for(&carrier, Some(voice))
}

/// Where one sample lives: `narrate/samples/<voice>_<hash>.wav`, named by who speaks and what they
/// read, so hearing a voice again is instant and a different voice never overwrites another's take.
pub fn sample_file(tree: &Tree, voice: &str, text: &str) -> PathBuf {
    narrate_data::sample_path(tree, voice, &sample_key(voice, text))
}

/// S5: the same sample bytes, if this voice and text were spoken before. A zero-length file is no
/// sample at all — an interrupted write must not be replayed as silence.
pub fn sample_cached(tree: &Tree, voice: &str, text: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(sample_file(tree, voice, text)).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(bytes)
}

/// S5's last step: file the take under the sample's own name. Returns nothing — the caller keeps the
/// bytes it read; this only says whether they could be written where they belong.
pub fn store_sample(tree: &Tree, voice: &str, text: &str, bytes: &[u8]) -> Result<(), String> {
    let file = sample_file(tree, voice, text);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    std::fs::write(&file, bytes).map_err(|err| format!("{}: {err}", file.display()))?;
    Ok(())
}

fn run_ffmpeg(ffmpeg: &str, args: &[String]) -> Result<(), String> {
    if let Some(out) = args.last() {
        if let Some(parent) = Path::new(out).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::subprocess::run(ffmpeg, &borrowed, |_| {})
        .map_err(|why| format!("{}: {why}", crate::subprocess::shown(ffmpeg, &borrowed)))?;
    Ok(())
}

fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    let bytes = std::fs::read(from).map_err(|err| format!("{}: {err}", from.display()))?;
    if let Some(parent) = to.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(to, bytes).map_err(|err| format!("{}: {err}", to.display()))
}
