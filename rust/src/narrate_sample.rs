//! F4.6 S5: the sample's own legs: is a take on disk, speak one that isn't, play it, stop it.
//! Takes inputs as arguments and returns the sentence to print, so a test needs no window. Test seams: the TTS address (`speak_leg` re-reads Settings per call, §02-services#1) and the player binary [`narrate_preview_leg::voice_program`].

use crate::layout::Tree;
use crate::narrate_details;
use crate::narrate_preview_leg;
use crate::voice_ref;

/// S5's door before any dial: is this voice with these words already on disk? Hearing an existing take is instant, so it must not make the page look busy.
pub fn already_heard(tree: &Tree, voice: &str, text: &str) -> Option<String> {
    let file = voice_ref::sample_file(tree, voice, text);
    let cached = voice_ref::sample_cached(tree, voice, text)?;
    crate::ui::window::log_line(&narrate_details::sample_file_log(
        &file_name(&file),
        cached.len() as u64,
        None,
    ));
    play(&file).ok()
}

/// S5's speech, shared by ▶ and ⟳. `roll` is absent from the log line for a first take (`>>> sample:` vs `>>> sample take 2:`).
/// Returns the status to print; the press owns the bar, so this does not write it.
pub fn speak(
    tree: &Tree,
    voice: &str,
    pitch: f64,
    text: &str,
    roll: usize,
) -> Result<String, String> {
    // §6: log WHO spoke WHAT first; a sample has no output file to inspect.
    crate::ui::window::log_line(&narrate_details::sample_log(
        voice,
        pitch,
        if roll == 0 { None } else { Some(roll) },
        text,
    ));
    // A roll is a new take, so a new seed.
    let key = crate::narration::key_for(
        &crate::narration::Entry {
            text: text.to_string(),
            roll: roll as i32,
            ..Default::default()
        },
        Some(voice),
    );
    let seed = crate::narration::tts_seed(&key);
    let language = crate::project::load(tree.dir())
        .map(|project| project.policy.language.value)
        .unwrap_or_default();
    match crate::speak_leg::speak(tree, text, "", seed, &key, &language) {
        crate::narrate_tts::Outcome::Refused(why) => Err(why),
        crate::narrate_tts::Outcome::Take(take) => {
            let bytes = std::fs::read(&take).map_err(|err| format!("{}: {err}", take.display()))?;
            // §6 (`an under-sized answer`): name the file and byte count and store NOTHING, else a header-only reply is cached as the take and every later press succeeds in silence.
            if (bytes.len() as u64) < MIN_REPLY_BYTES {
                let file = voice_ref::sample_file(tree, voice, text);
                let said =
                    narrate_details::sample_too_small_log(&file_name(&file), bytes.len() as u64);
                crate::ui::window::log_line(&said);
                return Err(said);
            }
            voice_ref::store_sample(tree, voice, text, &bytes)?;
            let file = voice_ref::sample_file(tree, voice, text);
            crate::ui::window::log_line(&narrate_details::sample_file_log(
                &file_name(&file),
                bytes.len() as u64,
                Some("just now"),
            ));
            play(&file)
        }
    }
}

/// A reply this small carries no audio (a wav header is 44 bytes plus one 16-bit frame); `speak` refuses it and logs [`crate::narrate_details::sample_too_small_log`].
pub const MIN_REPLY_BYTES: u64 = 46;

/// Hand the sample to the player (same seam as the preview). The child is held so ⏹ can reach it via [`stop`]; `-autoexit` ends an unstopped one.
fn play(file: &std::path::Path) -> Result<String, String> {
    play_file(file)
}

/// Hear one file on the voice player; public because the take band's ▶ plays the same reference (§1).
pub fn play_file(file: &std::path::Path) -> Result<String, String> {
    let player = narrate_preview_leg::voice_program();
    let path = file.to_string_lossy().into_owned();
    let args = narrate_preview_leg::audio_argv(&player, &path, 0.0, None, 1.0);
    let child = narrate_preview_leg::spawn_player(&player, &args).map_err(|_| {
        format!(
            "{player} could not play the sample \u{2014} it is at {}",
            file.display()
        )
    })?;
    PLAYERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(child);
    Ok(narrate_details::sample_playing_status().to_string())
}

/// §1 **8** ▶: play seconds `from`..`to` of a recording (sound only), held with the sample players so ⏹ and a second ▶ reach it.
pub fn play_span(file: &std::path::Path, from: f64, to: f64) -> Result<(), String> {
    let player = narrate_preview_leg::voice_program();
    // An endless `to` plays to the file's end; a player with no end is ended by `narrate_page_take::walk_on`.
    let args =
        narrate_preview_leg::audio_argv(&player, &file.to_string_lossy(), from, Some(to), 1.0);
    let child = narrate_preview_leg::spawn_player(&player, &args)
        .map_err(|_| format!("{player} could not play {}", file.display()))?;
    PLAYERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(child);
    Ok(())
}

static PLAYERS: std::sync::LazyLock<std::sync::Mutex<Vec<std::process::Child>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

/// Freeze or thaw every sample player (§6 "sample paused"), like [`crate::narrate_preview_leg::hold_paused`]; `None` when none is playing, so the page starts a sample instead.
pub fn hold_paused(hold: bool) -> Option<bool> {
    let mut held = false;
    PLAYERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain_mut(|child| {
            if child.try_wait().ok().flatten().is_some() {
                return false; // finished on its own (`-autoexit`): nothing to signal
            }
            let pid = child.id();
            if hold {
                crate::narrate_preview_media::pause_pids(&[pid]);
            } else {
                crate::narrate_preview_media::resume_pids(&[pid]);
            }
            held = true;
            true
        });
    held.then_some(hold)
}

/// Players not yet reaped, each slot holding its exit status once gone, to tell "takes played" from "stopped playing the takes".
fn alive() -> Vec<Option<std::process::ExitStatus>> {
    PLAYERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter_mut()
        .map(|child| child.try_wait().ok().flatten())
        .collect()
}

pub fn is_playing() -> bool {
    self::alive().iter().any(|status| status.is_none())
}

/// Whether every spawned player ran to the end ("takes played") against ⏹ ("stopped playing the takes"). Empty is `false`.
pub fn all_finished() -> bool {
    let statuses = self::alive();
    !statuses.is_empty() && statuses.iter().all(|status| status.is_some())
}

/// ⏹ for the sample: kill every running sample player and say how many. Separate from the preview's `stop_running`.
pub fn stop() -> usize {
    let mut killed = 0usize;
    PLAYERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain_mut(|child| match child.try_wait() {
            Ok(Some(_)) => false,
            Ok(None) => {
                let _ = child.kill();
                killed += 1;
                false
            }
            Err(_) => false,
        });
    killed
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
