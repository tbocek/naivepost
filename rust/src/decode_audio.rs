//! F1.3 S1 — the decode every other Prepare step reads: `voice16k.wav`, mono 16 kHz.
//!
//! Nothing else writes that file: the ASR, aligner, diarizer and both transcripts count samples of THIS file ([`crate::transcribe::SAMPLE_RATE`] is the rate its word times are divided by). It is the first leg of the pass, on the same
//! subprocess door as every ffmpeg call ([`crate::subprocess::run`], which logs the argv first, §8).
//!
//! The output is a real wav (`pcm_s16le`), not the headerless stream separation decodes to: ffprobe reads a duration from a wav header, which is how the pass knows the take's length first (S2's short-take test).

use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::separate_media;
use crate::transcribe;

/// S1's arguments: no picture, 16 kHz, one channel, signed 16-bit samples in a wav.
///
/// The rate is [`transcribe::SAMPLE_RATE`], not a literal: it turns sample numbers into seconds everywhere after, and a drifted rate would shift every word time by a percentage instead of breaking loudly.
pub fn decode_plan() -> Vec<String> {
    vec![
        "-vn".into(),
        // On the recording's own clock: a sound track that starts after the picture keeps that lead as silence, so
        // sample 0 is the file's second 0, the second Produce and ▶ seek to (first rewrite: OBS writes its sound 22 ms
        // after the picture, and every word time, cut edge and envelope sat 21 ms ahead of what the render cut).
        "-af".into(),
        CLOCK_FILTER.into(),
        "-ar".into(),
        transcribe::SAMPLE_RATE.to_string(),
        "-ac".into(),
        "1".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
    ]
}

/// The filter that keeps a late sound track's lead: silence up to its first sample.
pub const CLOCK_FILTER: &str = "aresample=async=1:first_pts=0";

/// The file beside `voice16k.wav` saying it was decoded on the recording's own clock ([`CLOCK_FILTER`]). A decode
/// without it is an older one, moved over once by [`ensure`].
pub fn clock_mark(tree: &Tree, lane: &str) -> PathBuf {
    tree.voice16k_wav(lane).with_file_name("voice16k.clock")
}

/// The longest lead an older decode is moved over by; past it the times are made again.
const MOST_LEAD_SAMPLES: usize = transcribe::SAMPLE_RATE as usize;

/// S1's resume rule: the decode happens unless the file is already there. Existence is the marker, not size (an empty recording decodes to a header-only wav).
pub fn already_decoded(tree: &Tree, lane: &str) -> bool {
    tree.voice16k_wav(lane).exists()
}

/// Decode `source` into `out`. Every failure names a path (§Failure-is-specific).
///
/// A source with no audio stream fails with ffmpeg's own sentence: the tool that reads it decides whether a file has sound.
pub fn decode(
    configured_ffmpeg: &str,
    source: &Path,
    out: &Path,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-i".to_string(),
        source.display().to_string(),
    ];
    args.extend(decode_plan());
    args.push(out.display().to_string());
    let owned: Vec<&str> = args.iter().map(String::as_str).collect();
    // Same rule as every other ffmpeg call site: the configured box when the settings name one.
    crate::subprocess::run(&separate_media::program(configured_ffmpeg), &owned, log)
        .map_err(|err| format!("{}: {err}", out.display()))?;
    if out.exists() {
        return Ok(());
    }
    Err(format!("{}: the decode produced nothing", out.display()))
}

/// The decoded audio this row needs, making it first if absent. Returns the path every later step reads ([`Tree::voice16k_wav`]).
///
/// `source` is the recording, resolved by the caller through the one path rule; `lane` is the row's own lane, not the file's stem as it sits on disk.
pub fn ensure(
    tree: &Tree,
    lane: &str,
    source: &Path,
    configured_ffmpeg: &str,
    log: &mut impl FnMut(&str),
) -> Result<PathBuf, String> {
    let out = tree.voice16k_wav(lane);
    let mark = clock_mark(tree, lane);
    if already_decoded(tree, lane) && mark.exists() {
        return Ok(out);
    }
    if !source.exists() {
        return Err(format!(
            "{}: the recording named by the row is not there",
            source.display()
        ));
    }
    if already_decoded(tree, lane) {
        // A move that cannot be made keeps the older decode and is tried again next time: Prepare carries on.
        if let Err(why) = move_to_the_recordings_clock(tree, lane, source, configured_ffmpeg, log) {
            let fresh = out.with_file_name("voice16k.clock.wav");
            let _ = std::fs::remove_file(&fresh).or_else(|_| std::fs::remove_dir_all(&fresh));
            log(&format!(
                "!!! [{lane}] could not decode again on the recording's own clock -- the older decode stays, its times \
                 21 ms or so ahead of the file: {why}"
            ));
            return Ok(out);
        }
    } else {
        decode(configured_ffmpeg, source, &out, log)?;
    }
    std::fs::write(&mark, "the recording's own clock\n").map_err(|err| format!("{}: {err}", mark.display()))?;
    Ok(out)
}

/// An older decode (made before [`CLOCK_FILTER`]) is decoded again, and everything timed on it moves later by the lead
/// the new decode put in front: the new file is the old one behind that much silence, sample for sample (measured on
/// 09-account: 336 samples, the rest identical but for one sample off by one). Words, aligned words, ASR pieces,
/// speaker turns and the cleaned lines move; the envelope and the session word list are made again from the new file.
/// A lead that is negative or over a second is no lead: those times are thrown away and Prepare makes them again.
fn move_to_the_recordings_clock(
    tree: &Tree,
    lane: &str,
    source: &Path,
    configured_ffmpeg: &str,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    let out = tree.voice16k_wav(lane);
    let fresh = out.with_file_name("voice16k.clock.wav");
    decode(configured_ffmpeg, source, &fresh, log)?;
    let old = crate::wave_build::read_wav_samples(&out)?.len();
    let new = crate::wave_build::read_wav_samples(&fresh)?.len();
    let lead = new as i64 - old as i64;
    if (0..=MOST_LEAD_SAMPLES as i64).contains(&lead) {
        if lead > 0 {
            shift_times(tree, lane, lead as u64)?;
        }
        log(&moved_log(lane, lead as u64));
    } else {
        for path in [tree.words_json(lane), tree.words_aligned_json(lane), tree.turns_json(lane), tree.asrchunks_json(lane)] {
            let _ = std::fs::remove_file(path);
        }
        log(&format!(
            "!!! [{lane}] decoded again on the recording's own clock, but it is {lead} samples off the old decode -- \
             its speech is transcribed again"
        ));
    }
    std::fs::rename(&fresh, &out).map_err(|err| format!("{}: {err}", out.display()))?;
    // Made again from the new file: the envelope by `wave_build::ensure` right after, the word list by the merge.
    let _ = std::fs::remove_file(tree.wave(lane));
    let _ = std::fs::remove_file(tree.session_word_list_json());
    Ok(())
}

pub fn moved_log(lane: &str, lead: u64) -> String {
    format!(
        ">>> [{lane}] decoded again on the recording's own clock: its sound starts {:.1} ms after the picture, so its word times move that much later",
        lead as f64 * 1000.0 / transcribe::SAMPLE_RATE as f64
    )
}

/// Every time the older decode gave this lane, `lead` samples later.
fn shift_times(tree: &Tree, lane: &str, lead: u64) -> Result<(), String> {
    use crate::requests;
    let later = |w: &mut requests::Word| {
        w.start_sample += lead;
        w.end_sample += lead;
    };
    if let Some(mut doc) = requests::read_words(tree, lane)? {
        doc.words.iter_mut().for_each(later);
        requests::write_words(tree, lane, &doc)?;
    }
    if let Some(mut doc) = requests::read_aligned(tree, lane)? {
        doc.words.iter_mut().for_each(later);
        requests::write_aligned(tree, lane, &doc)?;
    }
    if tree.turns_json(lane).exists() {
        let mut turns = requests::read_turns(tree, lane)?;
        for turn in &mut turns {
            turn.start_sample += lead;
            turn.end_sample += lead;
        }
        requests::write_turns(tree, lane, &turns)?;
    }
    let seconds = lead as f64 / transcribe::SAMPLE_RATE as f64;
    if tree.asrchunks_json(lane).exists() {
        let mut chunks = requests::read_chunks(tree, lane)?;
        for chunk in &mut chunks {
            // The first piece still starts at the file's first sample: the lead is silence, and it belongs to it.
            if chunk.s > 0.0 {
                chunk.s += seconds;
            }
            chunk.e += seconds;
        }
        requests::write_chunks(tree, lane, &chunks)?;
    }
    let fixed = tree.transcript_fixed_tsv(lane);
    if fixed.exists() {
        let mut lines = crate::textfmt::read_lines(&fixed)?;
        for line in &mut lines {
            line.start += seconds;
            line.end += seconds;
        }
        crate::textfmt::write_lines(&lines, &fixed)?;
    }
    Ok(())
}
