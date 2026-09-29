//! F4.6 S5 — the sample's own legs: is a take already on disk, speak one that isn't, play it, stop it.
//!
//! The page keeps what only the page can hold (the refusal ladder over `NarrateState`, the entry widget,
//! the status bar); everything here takes its inputs as arguments and returns the sentence the press
//! should print, so the whole chain is testable without a window. Nothing in this module reads the
//! page's state or widgets.
//!
//! The two seams a test replaces are the ones the program itself uses: the TTS address `speak_leg`
//! re-reads from Settings per call (§02-services#1), and the player binary
//! [`narrate_preview_leg::voice_program`] resolves per spawn. Point both at a fake and the program
//! cannot tell the difference.

use crate::layout::Tree;
use crate::narrate_details;
use crate::narrate_tts;
use crate::narrate_preview_leg;
use crate::voice_ref;

/// S5's door, before any dial: is this voice and these words already on disk? Answering here rather
/// than inside the speak keeps the caller's busy flag honest — hearing a take that exists is instant,
/// so it is not a synthesis and must not make the page look busy.
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

/// S5's speech itself, shared by ▶ and ⟳ so neither copies the other's legs. `roll` is the re-roll
/// count, and its absence from the log line (`>>> sample:` against `>>> sample take 2:`) is the point:
/// a sample's first take is not worth naming.
///
/// Returns the status the caller should print. It does NOT write the status bar itself, because the
/// press that started it owns the bar: the synthesising sentence goes up first, and whatever comes
/// back replaces it, whether that is the take or the reason there is none.
pub fn speak(
    tree: &Tree,
    voice: &str,
    pitch: f64,
    text: &str,
    roll: usize,
) -> Result<String, String> {
    // §6: every sample logs WHO spoke WHAT before anything can go wrong, since a sample has no output
    // file to inspect afterwards if the dial never happens.
    crate::ui::window::log_line(&narrate_details::sample_log(
        voice,
        pitch,
        if roll == 0 { None } else { Some(roll) },
        text,
    ));
    // F4.4 S1 asks this before ANY dial: with no reference there is nothing to clone, and asking a
    // live server about its health would put two requests on the wire for a sample that cannot be
    // spoken.
    if let Some(problem) = narrate_tts::reference_problem(tree) {
        return Err(problem);
    }
    let key = voice_ref::sample_key(voice, text);
    let seed = crate::narration::tts_seed(&key);
    let language = crate::project::load(tree.dir())
        .map(|project| project.language)
        .unwrap_or_default();
    match crate::speak_leg::speak(tree, text, "", seed, &key, &language) {
        crate::narrate_tts::Outcome::Refused(why) => Err(why),
        crate::narrate_tts::Outcome::Take(take) => {
            let bytes = std::fs::read(&take).map_err(|err| format!("{}: {err}", take.display()))?;
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

/// Hand the sample to the player. The player program is the same seam the preview uses, so a test
/// points it at a stub and the program is unchanged. The child is held so ⏹ can reach it through
/// [`stop`]; `-autoexit` was passed, so a player nobody stops ends on its own.
fn play(file: &std::path::Path) -> Result<String, String> {
    let player = narrate_preview_leg::voice_program();
    let path = file.to_string_lossy().into_owned();
    let args: Vec<String> = ["-nodisp", "-autoexit", "-loglevel", "quiet", "-i", &path]
        .iter()
        .map(|a| a.to_string())
        .collect();
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

/// The sample players still alive. Held so ⏹ can reach them; each entry leaves when the player exits.
static PLAYERS: std::sync::LazyLock<std::sync::Mutex<Vec<std::process::Child>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

/// ⏹'s side of the sample: kill every sample player still running, and say how many. Kept separate
/// from the preview's `stop_running` because the sample spawns its own players and is not a
/// `Preview`; the two never share a child, so neither can stop the other's audio by accident.
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
