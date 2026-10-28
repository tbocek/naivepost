//! §F4.6 / inventory §A.7 — what the take band is drawn across, and when it shows at all.
//!
//! §A.7 says the band is the **narrator's recording** waveform, "visible only for a narrator slot with a
//! recording". That is a question about the PROJECT — Prepare tagged a recording as that narrator, and the
//! file is there — not about whether a built reference happens to be sitting on disk this second. The
//! distinction is not academic: §F4.6 S4 removes `voice_ref_base.wav` whenever the takes, the pitch or the
//! voice change, so a visibility rule keyed to that file hides the band on the very press that made a take
//! (which is how `just snapshot 07-take` ended up printing "take band seeded = true" over a shot with no
//! band in it). Both questions are answered here from the project instead.
//!
//! Its own file because [`crate::ui::narrate_band`] and [`crate::ui::narrate_page`] are both past the size a
//! new block should grow them by; each calls into here.

use std::path::{Path, PathBuf};

use crate::layout;
use crate::narrate_screen;
use crate::project::Project;
use crate::voice_ref;

/// Is there a recording for this voice to be cloned from? §F4.6 S1's three branches, in order:
/// captions speak nothing so there is nothing to draw; a narrator slot needs the source Prepare tagged
/// with that slot AND its file; anything else is a voices-folder id, which has to name a file that exists
/// (S1's "voice X is no longer in DIR — pick another" is the same question asked at pick time).
pub fn recording_exists(tree: &layout::Tree, project: &Project, voice: &str) -> bool {
    recording_path(tree, project, voice).is_some_and(|path| path.is_file())
}

/// The seconds the band spans: its tape's length ([`tape`]), so the ruler, the wave, the takes, the red bar and ▶
/// all mean seconds of what the narrator says in the cut. With no recording at all, the built reference's own
/// length ([`crate::ui::narrate_band::recording_seconds`]), the one thing left to select from. `None` only when
/// there is neither.
pub fn band_seconds(tree: &layout::Tree, project: &Project, voice: &str) -> Option<f64> {
    // The RECORDING's own seconds first: the wave, the takes, the red bar and ▶ are all seconds of the
    // recording, and the first rewrite measured the ruler from the built reference instead, so a band over a
    // ten-minute recording mapped its bar onto the reference's few seconds and ▶ played from somewhere else.
    let length = tape(tree, project, voice).length();
    if length > 0.0 && length.is_finite() {
        return Some(length);
    }
    crate::ui::narrate_band::recording_seconds()
}

thread_local! {
    /// Each recording's length by (path, size, mtime): asked once, not on every paint (ffprobe is a process).
    static LENGTHS: std::cell::RefCell<std::collections::HashMap<(PathBuf, u64, i64), Option<f64>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn probe_cached(path: &Path) -> Option<f64> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |at| at.as_secs() as i64);
    let key = (path.to_path_buf(), meta.len(), mtime);
    if let Some(known) = LENGTHS.with(|lengths| lengths.borrow().get(&key).copied()) {
        return known;
    }
    let seconds = probe_seconds(path);
    LENGTHS.with(|lengths| lengths.borrow_mut().insert(key, seconds));
    seconds
}

/// Where this voice's recording is, without asking whether it exists: the door every answer above reads.
/// A `project:`-relative source path is resolved by §2's one path rule ([`layout::resolve`]), the same way
/// the Cut page and the render resolve stored paths — `tree.dir()` IS the project folder a `Tree` was made
/// from, so it stands as both roots here.
pub fn recording_path(tree: &layout::Tree, project: &Project, voice: &str) -> Option<PathBuf> {
    if voice == narrate_screen::CAPTIONS || voice.is_empty() {
        return None;
    }
    if let Some(slot) = voice_ref::narrator_slot(voice) {
        let source = project
            .sources
            .iter()
            .find(|source| source.narrator == slot as u32)?;
        return Some(layout::resolve(tree.dir(), tree.dir(), &source.path));
    }
    // A voices-folder id: the file the picker copied in. The ids come off `voices::list_wavs`, which lists
    // stems, so an id without an extension means that stem's .wav in the voices folder.
    voices_candidate(voice)
}

/// The voices folder's file for a picked id: the id as given if it already names a `.wav`, else the id with
/// `.wav` added — the picker stores ids without the extension (`voices::list_wavs`' stems).
fn voices_candidate(voice: &str) -> Option<PathBuf> {
    let folder = crate::ui::narrate_voices::voices_folder();
    let named = if voice.ends_with(".wav") {
        voice.to_string()
    } else {
        format!("{voice}.wav")
    };
    Some(folder.join(named))
}

/// The band's tape for this voice (§1 badge 10): a narrator slot's kept stretches across every recording tagged
/// with that slot ([`crate::narrate_tape::kept`]), laid out on the session recordings the page holds; a voice that
/// is one file (a voices-folder id, a narrator whose recording no kept clip shows, a session whose recordings are
/// not measured yet) is that file whole. Empty for captions, or when there is no recording at all.
pub fn tape(tree: &layout::Tree, project: &Project, voice: &str) -> crate::narrate_tape::Tape {
    if let Some(slot) = voice_ref::narrator_slot(voice) {
        let narrated: Vec<(String, PathBuf)> = project
            .sources
            .iter()
            .filter(|source| source.narrator == slot as u32)
            .map(|source| {
                let file = layout::resolve(tree.dir(), tree.dir(), &source.path);
                (crate::narrate_data::take_base(&file), file)
            })
            .collect();
        let cut = crate::cut::load(tree).unwrap_or_default();
        let kept = crate::narrate_tape::kept(&cut, &crate::ui::page_recordings(), &narrated);
        if !kept.is_empty() {
            return kept;
        }
    }
    // A file ffprobe cannot measure is still the voice's recording: one open-ended stretch, its seconds the
    // band's own, so takes and ▶ keep working and only the drawing waits for a length.
    match recording_path(tree, project, voice).filter(|path| path.is_file()) {
        Some(file) => {
            let seconds = probe_cached(&file).unwrap_or(f64::INFINITY);
            crate::narrate_tape::whole(file, seconds)
        }
        None => crate::narrate_tape::Tape::default(),
    }
}

/// ffprobe's answer for one file, through the configured ffmpeg's own ffprobe like every other probe in
/// the app (§1's "ffprobe from the same folder"). Silent on failure: the caller answers `None`, and the
/// page then draws no band rather than a ruler of guessed seconds.
fn probe_seconds(path: &Path) -> Option<f64> {
    // A wav answers from its own header, with no subprocess at all: §F4.6 S3 keeps the reference and the
    // voices folder's imports as 16-bit PCM, so the arithmetic `recording_seconds` uses already covers it,
    // and a shot or a first paint must not depend on ffprobe being installed.
    if let Some(seconds) = wav_seconds(path) {
        return Some(seconds);
    }
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    let seconds =
        crate::cut_lanes::probe_duration(&configured, &path.display().to_string(), &mut |line| {
            crate::ui::window::log_line(line)
        })
        .ok()?;
    (seconds > 0.0).then_some(seconds)
}

/// How long a 16-bit PCM wav holds, from its own header and size — the same reading
/// [`crate::ui::narrate_band::recording_seconds`] makes of the reference file, applied to any recording.
fn wav_seconds(path: &Path) -> Option<f64> {
    let (channels, rate) = voice_ref::wav_header(path)?;
    let bytes = std::fs::metadata(path).ok()?.len();
    let per_second = (channels.max(1) as u64) * (rate.max(1) as u64) * 2;
    Some(((bytes.saturating_sub(44)) as f64 / per_second as f64).max(0.1))
}
