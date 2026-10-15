//! F4.5 — the leg the preview runs ON, over the two real players.
//!
//! [`crate::narrate_preview`] holds every rule of §F4.5 and settles with no widget, no process and no
//! network, which is what makes its tests cheap. This module is the other half: the 100 ms loop that
//! drives those rules, the two subprocess players §A.3 names (video `n.player` and narration audio
//! `n.voice`), and the pids ⏹ has to reach. It decides NOTHING: every where/what/sentence answer is a
//! call into `narrate_preview::*`, so a rule retuned there moves the running page with it.
//!
//! Why a second file rather than more functions in `narrate_preview.rs`: the same reason `cut_play` and
//! [`crate::cut_play_leg`] are two files — `narrate_preview` is pure arithmetic and must stay callable
//! with no process state, while everything below holds child processes, a glib timer and the shared set of
//! takes the server refused.
//!
//! The seam is the player PROGRAM, exactly as [`crate::speak_leg`]'s seam is the server address: the
//! program name comes from Settings (an `llm.conf` line) or `NAIVEPOST_*_PLAYER` in the environment,
//! re-read per spawn (§02-services#1), so a test points the picture and the voice at whatever binary it
//! likes and no program code knows the difference. No scripted reply lives here — the answers come from
//! the real speech server and the real child processes.
//!
//! What is NOT verifiable in this container: there is no sound hardware (`/dev/snd` is absent), so
//! "you can hear the take" cannot be asserted here. What IS, and what the tests assert: the real argv the
//! player was asked for, the real pid registered for ⏹, the duck/rate/lanes numbers handed to it, and
//! the real file the synthesis wrote before the voice was pointed at it.

use std::process::Child;
use std::rc::Rc;

use crate::cut::{Cut, Fx, Seg};
use crate::narrate_preview::{self, Failed, Take};
use crate::narrate_preview_media;
use crate::narrate_tts;
use crate::narration::{self, Entry};
use crate::settings::{self, Conf};

/// `P.eng.picturePlayer` — the program the picture runs in. **Spec silent on the program**: the
/// spec's only words on this are `spec/inventory/narrate.md` §A.3's "Two players: video (n.player)
/// and narration audio (n.voice)" and §F4.5 S6's "⏹ stops both players", and it names no binary for
/// either. So the program is this module's own choice, written once here, and like every other local
/// tool of this app it is read from Settings rather than fixed (§02-services states that rule for the
/// media tools the spec does name — each one "on PATH (or a configured path)" — and the picture player
/// follows the same three-step lookup as the server addresses do). The program chosen is the
/// GStreamer CLI: the same pipeline the prototype ran inside the toolkit (`gui/player.go`: playbin
/// into a GTK paintable sink), reached as its OWN process because §F4.5 S6 needs two pids to kill and
/// a paintable sink inside the window has none.
pub const PICTURE_PLAYER: &str = "gst-play-1.0";

/// `P.eng.voicePlayer` — the program the narration's wav plays in. Same spec note as
/// [`PICTURE_PLAYER`]: the spec names two players and no programs, so this is the app's own choice, and
/// the same default keeps them one program asked twice rather than a second dependency nobody declared.
pub const VOICE_PLAYER: &str = "gst-play-1.0";

/// The settings keys the two player programs are read from, named after the `P.eng.*` ids above in
/// the uppercase form `llm.conf` writes its keys in.
pub const PICTURE_PLAYER_KEY: &str = "PICTURE_PLAYER";
pub const VOICE_PLAYER_KEY: &str = "VOICE_PLAYER";

/// `P.eng.voicePlayerFloor` — the floor under the voice's level while it speaks. **Spec silent**: S3
/// ducks the GAME BED by the game volume and says nothing about the take's own level, so the floor
/// exists only to keep a `game_volume` typed as 0 from spawning a voice nobody can hear. 0.02 is
/// −34 dB: audible at any speaker setting, inaudible against real audio.
pub const VOICE_VOLUME_FLOOR: f64 = 0.02;

/// What spawns a player and hands back its still-running child. A function pointer rather than a
/// direct call so a test can watch the argv the leg built while still getting a real pid back from a
/// real `std::process::Command` — the same shape `produce_exec::spawn_tool` is for the render.
pub type Spawn = dyn FnMut(&str, &[String]) -> Result<Child, String>;

/// The program's own spawner: [`crate::subprocess::spawn_piped`], which never reaps, so the pid
/// stays killable. Kept behind [`Spawn`] so the loop below has one injection point and no branch.
pub fn spawn_player(program: &str, args: &[String]) -> Result<Child, String> {
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::subprocess::spawn_piped(program, &borrowed)
        .map_err(|err| format!("{} failed: {err}", crate::subprocess::shown(program, &borrowed)))
}

/// The whole running preview: two children, the picture's second, what is speaking, and the takes the
/// speech server has already refused (shared with the row's ▶ through [`failed_takes`]).
pub struct Preview {
    /// `game_volume` as read off the project (P.policy.gameVolume) at the press that started this.
    pub game_volume: f64,
    /// The shared preview volume (the `narrate-volume` slider), multiplied in the way S3 says.
    pub volume: f64,
    /// Seconds the picture has been held still: the pause of S2's "the picture pauses".
    paused: bool,
    /// The line whose take was missing when the picture stopped.
    waiting: Option<usize>,
    /// The line whose audio is running right now — `tick`'s `speaking`.
    speaking: Option<usize>,
    /// The second to resume at when the wait ends: success re-reads the line's own start, failure
    /// resumes where the picture froze, and which one comes from
    /// [`narrate_preview::resume_after_synthesis`].
    frozen: f64,
    /// The take this session refused. Kept live here so the row's own ▶ and a lap of the cut read
    /// ONE answer, per S2's "sticky per wav".
    failed: Failed,
    picture: Option<Child>,
    voice: Option<Child>,
    /// The gain the live bed process was last asked for. `None` until something is cued; a tick whose answer
    /// equals it spawns nothing at all (the stamp [`crate::play_back::set_volume`] keeps for the Cut page).
    bed_gain: Option<f64>,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            game_volume: 1.0,
            volume: 1.0,
            paused: false,
            waiting: None,
            speaking: None,
            frozen: 0.0,
            failed: Failed::default(),
            picture: None,
            voice: None,
            bed_gain: None,
        }
    }
}

impl Preview {
    /// S1: start the preview at `from`. Spawns the picture (S3's rate comes straight from
    /// [`narrate_preview::seek_rate`], so the picture is asked for the same rate the render will use)
    /// and answers what the status line should say.
    ///
    /// A picture that will not spawn is reported rather than swallowed: S1's refusal set is about there
    /// being nothing to show, and a player program that is not on the machine is the same case for the
    /// person — the page prints what comes back here instead of claiming a preview that has no process.
    fn start(
        &mut self,
        tree: &crate::layout::Tree,
        cut: &Cut,
        entries: &[Entry],
        from: f64,
        spawn: &mut Spawn,
    ) -> Result<String, String> {
        let _ = entries;
        self.paused = false;
        self.waiting = None;
        self.speaking = None;
        self.frozen = from;
        // S3's bed level for this second: ducked while a line speaks, hushed when the scene says so.
        let gain = narrate_preview_media::bed_gain(cut, from, false, self.game_volume, self.volume);
        let program = PICTURE_PROGRAM.with(|name| name.borrow().clone());
        let (picture, _cue) = player(&program, tree, cut, from, gain, false, spawn)?;
        self.picture = Some(picture);
        self.bed_gain = Some(gain);
        // S3: the lanes this second hears, said out loud so a hush that was typed into a scene shows up
        // on the run rather than silently changing the mix.
        let hushed = narrate_preview::lanes_heard(cut, from);
        if hushed.is_empty() {
            Ok(format!(
                "playing the cut from {}",
                crate::narrate_screen::time_field(from)
            ))
        } else {
            Ok(format!(
                "playing the cut from {} \u{b7} {} hushed",
                crate::narrate_screen::time_field(from),
                hushed.join(", ")
            ))
        }
    }

    /// S2's wait: the picture is stopped at `t` while the line at `line` is synthesized. The picture's
    /// child keeps its place (it is PAUSED, not gone): the second the pause began is what
    /// [`narrate_preview::resume_after_synthesis`] is given on the failure side.
    pub fn pause_for(&mut self, line: usize, t: f64) {
        self.paused = true;
        self.waiting = Some(line);
        self.frozen = t;
    }

    /// S2: the take arrived. Resume at the LINE's start, which is the ok side of
    /// [`narrate_preview::resume_after_synthesis`], and point the voice at the file that was written
    /// (S3: the take's own floor, never the duck — the duck is what the game bed gets, not the voice).
    ///
    /// Returns the second the picture moved to, so the caller's clock and the picture agree.
    pub fn resume_spoken(
        &mut self,
        line: usize,
        line_start: f64,
        wav: &str,
        spawn: &mut Spawn,
    ) -> f64 {
        let at = narrate_preview::resume_after_synthesis(self.frozen, line_start, true);
        self.paused = false;
        self.waiting = None;
        self.speaking = Some(line);
        // The old voice belongs to the line that just ended; drop it before the new one starts so one
        // preview never has two wavs playing over the same second.
        self.voice = None;
        if let Ok(child) = spawn(&VOICE_PROGRAM.with(|name| name.borrow().clone()), &voice_args(wav, 0.0, ducked_voice(self.game_volume))) {
            self.voice = Some(child);
        }
        at
    }

    /// S2: the take failed while the picture waited. Resume where the picture FROZE (the not-ok side of
    /// [`narrate_preview::resume_after_synthesis`]) and remember the wav, so the next lap runs mute and
    /// says so again.
    pub fn resume_failed(&mut self, wav: &str) -> f64 {
        let at = narrate_preview::resume_after_synthesis(self.frozen, self.frozen, false);
        self.paused = false;
        self.waiting = None;
        self.failed.add(wav);
        at
    }

    /// S6: stop BOTH players and report it through the rules module's own struct, so the ownership
    /// sentence (`hand_play_back`) is answered by the same value the page shows. Kills the real pids
    /// through [`crate::stop_legs::signal`] (TERM, then KILL) and hands them back so the caller can
    /// log them and so F0.3's ⏹ drain cannot signal the same child twice.
    pub fn stop(&mut self) -> (narrate_preview::Stopped, Vec<u32>) {
        // Take both children out FIRST so neither can be signalled twice: a killed `Child` dropped
        // without being reaped stays a zombie, and "is this pid dead?" would read a zombie as alive.
        let picture = self.picture.take();
        let voice = self.voice.take();
        let pids: Vec<u32> = [picture.as_ref(), voice.as_ref()]
            .into_iter()
            .flatten()
            .map(Child::id)
            .collect();
        let actually = crate::stop_legs::signal(&pids);
        for mut child in picture.into_iter().chain(voice) {
            let _ = child.wait();
        }
        self.paused = false;
        self.waiting = None;
        self.speaking = None;
        (narrate_preview::stop(), actually)
    }

    /// S2: the line whose audio is running (the `speaking` argument of the rules' `tick`).
    pub fn speaking(&self) -> Option<usize> {
        self.speaking
    }

    /// Whether the picture is held (waiting on a take) rather than running.
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// S2: the line the picture is waiting on, if any. The timer reads this to decide whether the beat
    /// is a dial to the speech server or a plain step over the cut.
    pub fn waiting_line(&self) -> Option<usize> {
        self.waiting
    }

    /// The pids ⏹ still has to reach. Empty once both have been killed.
    pub fn children(&self) -> Vec<u32> {
        [&self.picture, &self.voice]
            .into_iter()
            .flatten()
            .map(Child::id)
            .collect()
    }

    /// §6 (`Preview`): "when the picture stops, narration stops too" — has the PICTURE player run to its
    /// end by itself? `try_wait` reaps it if so, which is also what keeps a finished player from sitting in
    /// the slot as a zombie that `children()` would still report as killable.
    ///
    /// Only the picture answers this: the voice child outliving the picture is the bug, not a condition, so
    /// asking about it here would stop the preview for a take that simply finished first.
    pub fn picture_finished(&mut self) -> bool {
        match self.picture.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(Some(_))),
            // No picture at all is no answer: a preview whose picture never spawned is reported by the press
            // that tried, not ended by a later beat.
            None => false,
        }
    }

    /// Drop the picture child once it has answered "exited": it is already reaped by `picture_finished`, and
    /// leaving it in `children()` would have ⏹ report a dead pid as one it killed.
    pub fn forget_picture(&mut self) {
        self.picture = None;
    }

    /// End the narration half only: the take that is still speaking when the picture stops. Reports the pid
    /// it killed so the log can name it, and clears `speaking` so the next tick does not hold a boundary for
    /// a line nobody is hearing any more.
    ///
    /// A plain `SIGTERM` will not do here: this container's `sleep` accepts the signal and carries on (the
    /// reason [`crate::stop_legs::signal`] escalates), and escalating on every tick would make the beat wait
    /// for the grace period. The voice child is OURS and already dead-ended, so it is ended outright and then
    /// reaped — a killed `Child` left unreaped would still show in `children()` as a pid to kill.
    pub fn end_voice(&mut self) -> Option<u32> {
        let mut child = self.voice.take()?;
        let pid = child.id();
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
        let _ = child.wait();
        self.speaking = None;
        Some(pid)
    }

    /// Freeze both players where they stand, or thaw them (`SIGSTOP`/`SIGCONT`). They stay in the slot, so ⏹
    /// still owns them and their position survives. A freeze also ends the take's wait: S2's pause is a
    /// STILL, not a dial to the speech server, so nothing queues behind a line being synthesized.
    pub fn set_paused(&mut self, hold: bool) -> bool {
        let was = std::mem::replace(&mut self.paused, hold);
        if hold {
            self.waiting = None;
        }
        narrate_preview_media::signal_pids(&self.children(), hold);
        hold || was
    }

    /// The set of refused takes this preview is reading. Public so the page hands a row's retry into the
    /// SAME set rather than keeping a second one that would disagree about what is sticky.
    pub fn failed_mut(&mut self) -> &mut Failed {
        &mut self.failed
    }

    /// S3's stamp: ask the live bed to run at `gain`, answering whether that is a NEW answer. A tick whose
    /// answer equals the last one changes nothing and spawns nothing — re-cuing on every 100 ms beat would put
    /// a seek in the middle of every clip for a duck that has not moved.
    pub fn bed_gain_ask(&mut self, gain: f64) -> bool {
        let changed = self.bed_gain != Some(gain);
        if changed {
            self.bed_gain = Some(gain);
        }
        changed
    }

    /// S3: the bed's level lives in its command line (`--volume=`), so a new answer needs a new process.
    /// Kill this preview's own bed and cue it again at `gain` over `at`, keeping whatever hold the
    /// preview is under. Answers the argv before and after so the log can name the number that changed;
    /// `None` when there is no picture to re-cue or the new one would not spawn.
    pub fn recue_bed(
        &mut self,
        gain: f64,
        at: f64,
        tree: &crate::layout::Tree,
        cut: &Cut,
        spawn: &mut Spawn,
    ) -> Option<(Vec<String>, Vec<String>)> {
        self.picture.as_ref()?;
        // The player clamps its own `--volume=`, so the log has to show what it was really handed rather
        // than the unclamped product of duck × mix.
        let before = narrate_preview_media::bed_argv(
            &narrate_preview_media::cue_files(tree, cut, at),
            narrate_preview_media::footage_second(cut, at),
            gain.clamp(0.0, 1.0),
        );
        // The dying child goes first: two beds over one second is double sound, and §F4.5 S6 counts ONE
        // picture pid for ⏹.
        self.picture = None;
        self.bed_gain = Some(gain);
        let program = PICTURE_PROGRAM.with(|name| name.borrow().clone());
        let (child, cue) = player(&program, tree, cut, at, gain, self.paused, spawn).ok()?;
        self.picture = Some(child);
        Some((before, cue.args))
    }
}

thread_local! {
    /// The one running preview per process, as `cut_play_leg::SPARE` keeps one armed spare pipeline.
    static RUNNING: std::cell::RefCell<Option<Preview>> = const { std::cell::RefCell::new(None) };
    /// The picture player's program name, resolved from Settings at the press that started this
    /// preview. [`picture_program`] re-reads it per call; this slot only exists so the spawn inside
    /// [`Preview::start`] can reach it without threading a fifth argument through the loop.
    static PICTURE_PROGRAM: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static VOICE_PROGRAM: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// `P.eng.picturePlayer` as it stands on disk: the `llm.conf` line, else
/// `NAIVEPOST_PICTURE_PLAYER`, else [`PICTURE_PLAYER`]. The same three-step precedence every leg in
/// this app uses for a machine value (`speak_leg`'s audio address, the configured encoder path), so the
/// seam is the setting and not a test hook.
pub fn picture_program() -> String {
    named_program(PICTURE_PLAYER_KEY, "NAIVEPOST_PICTURE_PLAYER", PICTURE_PLAYER)
}

/// `P.eng.voicePlayer` as it stands on disk: see [`picture_program`].
pub fn voice_program() -> String {
    named_program(VOICE_PLAYER_KEY, "NAIVEPOST_VOICE_PLAYER", VOICE_PLAYER)
}

fn named_program(conf_key: &str, env_key: &str, fallback: &str) -> String {
    if let Some(paths) = settings::from_environment() {
        if let Ok(conf) = settings::read(&paths) {
            if let Some(value) = setting_value(&conf, conf_key) {
                if !value.is_empty() {
                    return value.to_string();
                }
            }
        }
    }
    match std::env::var(env_key) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => fallback.to_string(),
    }
}

/// One machine value out of `llm.conf`. The known fifteen have fields; `preview.picturePlayer` and
/// `preview.voicePlayer` are machine settings the dialog does not draw yet, so they ride in the
/// unknown-key list, where a newer build's setting is kept verbatim — which is exactly where a
/// hand-typed player line belongs.
fn setting_value(conf: &Conf, key: &str) -> Option<String> {
    unknown_value(conf, key)
}

// The fifteen dialog keys' values are read through `Conf::KEYS` by the settings module itself; a
// player key is not one of them, so nothing here needs a per-key match and none is written.

/// A key the settings writer carried through untouched. `Conf::unknown` is private (it is written
/// back verbatim, and being read-only from outside is what keeps that safe), so the value is taken from
/// the rendered file rather than by poking at the field. `settings::quote` is public and its unquote is
/// not, so the quoting is undone here by the same two rules that matter for a program path: strip the
/// surrounding double quotes, then unescape the six characters the writer escapes.
fn unknown_value(conf: &Conf, key: &str) -> Option<String> {
    // Rendered, then scanned: the one path that keeps `unknown`'s privacy and still answers a lookup.
    let rendered = settings::render(conf);
    for line in rendered.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() == key {
            return Some(unquoted(value.trim()));
        }
    }
    None
}

/// Undo [`settings::quote`] for the six characters it escapes.
fn unquoted(value: &str) -> String {
    let body = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value);
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('$') => out.push('$'),
            Some('`') => out.push('`'),
            Some('\n') => out.push('\n'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// S2/S3/S4/S5/S6: ONE tick's answer, computed by the rules and acted on here. Public so a test can
/// drive the same function the timer does, without waiting on a wall clock; the timer itself calls
/// nothing else.
///
/// Returns what to tell the person (and where the picture went), which is exactly what the loop writes
/// to the status line and the clock.
#[derive(Debug, Clone, PartialEq)]
pub enum Stepped {
    /// Nothing changed this tick.
    Same,
    /// The picture moved to `at` (a skip forward, or a plain step), at `rate`.
    Moved { at: f64, rate: f64 },
    /// The boundary is held while a line still speaks (S2).
    Held,
    /// Entering a line whose take exists: its wav starts at `offset` (S2), at the ducked volume.
    Speaking { line: usize, offset: f64, gain: f64 },
    /// Entering a line with no take: the picture pauses, `synthesizing line N` (S2).
    Synthesizing { line: usize },
    /// Entering a line whose take already failed: the clip runs mute, `sticky_failed` (S2).
    Muted { line: usize, said: String },
    /// Past the last clip: both players stop (S2).
    Paused,
}

/// Advance the preview one tick. `now` is the second the picture would be at if nothing held it, and
/// every decision comes from the rules module — this function only turns the answer into a player
/// action and a sentence.
///
/// `cut` is the session's cut, `entries` the narration record, and `tree` the project folder the
/// takes live in (a take's existence is a file check, which is what makes S2's three branches real).
pub fn step(
    preview: &mut Preview,
    tree: &crate::layout::Tree,
    cut: &Cut,
    entries: &[Entry],
    now: f64,
    no_narration: bool,
    spawn: &mut Spawn,
) -> Stepped {
    // §6 (`Preview`) before anything else: a picture that ran to its own end ENDS the narration too. Left
    // unchecked, the take's player keeps speaking over a frame that stopped moving seconds ago, and the tick
    // goes on asking about seconds the picture is no longer playing. Answering `Paused` is what makes this
    // ending identical to walking off the last clip: the caller stops both players and tells the page its
    // preview is over (§5's `started` half), whether the cut ended or its player simply did.
    if preview.picture_finished() {
        preview.forget_picture();
        if let Some(pid) = preview.end_voice() {
            crate::ui::window::log_line(&format!(
                "preview: the picture ended, so the narration stopped with it (killed pid {pid})"
            ));
        }
        // The slot is emptied by the CALLER (`tick_loop` clears RUNNING after this returns): `step` is also
        // called from `tick_loop`, which already holds the slot borrowed, and re-borrowing it here would
        // panic. A slot left holding two dead players is what the caller's `Stepped::Paused` branch ends.
        return Stepped::Paused;
    }
    let takes = take_states(tree, entries, &preview.failed, no_narration);
    match narrate_preview::tick(now, &cut.segs, entries, &takes, preview.speaking) {
        narrate_preview::Tick::SkipTo(at) => {
            let rate = narrate_preview::seek_rate(&cut.fx, at);
            preview.frozen = at;
            Stepped::Moved { at, rate }
        }
        narrate_preview::Tick::Hold => Stepped::Held,
        narrate_preview::Tick::Pause => Stepped::Paused,
        narrate_preview::Tick::Idle => Stepped::Same,
        narrate_preview::Tick::Speak { line, offset } => {
            let gain = ducked_voice(preview.game_volume);
            preview.speaking = Some(line);
            preview.frozen = now;
            if let Some(wav) = take_file(tree, entries, line) {
                // S2/S3: the take plays from the offset the picture is at — a mid-line entry does not
                // restart the wav from its first word.
                if let Ok(child) = spawn(
                    &voice_program(),
                    &voice_args(&wav.to_string_lossy(), offset, gain),
                ) {
                    preview.voice = Some(child);
                }
            }
            Stepped::Speaking { line, offset, gain }
        }
        narrate_preview::Tick::Synthesize { line } => {
            preview.pause_for(line, now);
            Stepped::Synthesizing { line }
        }
        narrate_preview::Tick::Mute { line } => {
            preview.speaking = None;
            preview.frozen = now;
            Stepped::Muted {
                line,
                said: narrate_preview::sticky_failed(line),
            }
        }
    }
}

/// S1/S3/S6: the running preview's whole loop body, one call per tick, over the thread-local preview.
/// Starts the preview if none is running (the picture click's S1 answer), otherwise advances it, and
/// gives the page the sentence to print. Present so `ui/narrate_page.rs` holds one call per tick and
/// no rule of its own.
// The tick's whole argument list is what one beat needs: the page's facts, the mix for this second and
// the seam that spawns. Splitting it would move a rule into the caller.
#[allow(clippy::too_many_arguments)] // one beat of the preview reads all eight
pub fn tick_loop(
    tree: &crate::layout::Tree,
    cut: &Cut,
    entries: &[Entry],
    at: f64,
    game_volume: f64,
    volume: f64,
    no_narration: bool,
    spawn: &mut Spawn,
) -> Stepped {
    RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        if held.is_none() {
            *held = Some(Preview {
                game_volume,
                volume,
                ..Default::default()
            });
        }
        let preview = held.as_mut().expect("just filled");
        step(preview, tree, cut, entries, at, no_narration, spawn)
    })
}

/// What the tick loop needs from the page, so this module holds no GTK-slot knowledge of its own —
/// the same seam shape as [`crate::cut_play_leg::CutPreviewPage`]. The page implements it over its
/// published state; the leg asks four questions and gets one string back.
pub trait PreviewPage {
    /// Is this window's Narrate preview going? False once ⏹ stopped it or it was never started, which
    /// is what makes the timer cost one boolean read between presses.
    fn running(&self) -> bool;
    /// Where the line sits now, in session seconds.
    fn head(&self) -> f64;
    /// Move the line (a skip forward, a hold released, a resume after a synthesis).
    fn set_head(&self, at: f64);
    /// This window's cut and narration record.
    fn cut(&self) -> Cut;
    fn entries(&self) -> Vec<Entry>;
    /// The project folder the takes live in, or `None` with no session open (nothing to check, nothing
    /// to spawn — the timer then does nothing rather than inventing a path).
    fn tree(&self) -> Option<crate::layout::Tree>;
    /// P.policy.gameVolume as the project holds it now.
    fn game_volume(&self) -> f64;
    /// The shared preview volume (the `narrate-volume` slider).
    fn volume(&self) -> f64;
    /// Whether this project's narration is switched off (§F4.8). The tick asks it of the page rather
    /// than re-reading `naivepost.json` every beat; the page already keeps the tick's own state.
    fn no_narration(&self) -> bool {
        false
    }
    /// Print one sentence on the status line.
    fn say(&self, said: &str);
    /// The preview ended on its own (the tick walked off the last clip) rather than through a press:
    /// the page clears its own flags so §5's `started` half does not outlive the players.
    fn stopped(&self) {}
}

/// F4.5 S2: run the Narrate preview's own 100 ms tick (§A.3 "100 ms tick follows playback"),
/// started once per window from the page's `wire`, mirroring
/// [`crate::cut_play_leg::start_cut_preview_tick`].
///
/// Every beat it asks [`narrate_preview::tick`] through [`step`] and acts on the answer: the line
/// moves, a take that arrived is put on the voice player, a missing one pauses the picture and dials
/// the real speech server ([`synthesize_line`]), and a stop ends both players. It no-ops whenever the
/// preview is not running, so the timer costs one boolean read while the person is writing lines
/// rather than watching a cut.
pub fn start_tick(page: Rc<dyn PreviewPage>) {
    glib::timeout_add_local(
        std::time::Duration::from_millis(crate::narrate_screen::TICK_MS),
        move || {
            if !page.running() {
                return glib::ControlFlow::Continue;
            }
            let was_running = running();
            let Some(tree) = page.tree() else {
                return glib::ControlFlow::Continue;
            };
            let cut = page.cut();
            let entries = page.entries();
            let from = page.head();
            let game_volume = page.game_volume();
            let volume = page.volume();
            // S3: read the three numbers for THIS second once, through the leg's own reader, so the
            // duck, the seek rate and the hushed lanes cannot drift apart mid-tick.
            let speaking = RUNNING.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .and_then(|preview| preview.speaking())
                    .is_some()
            });
            let heard = heard_at(&cut, from, speaking, game_volume);
            // S3 (`sound equals the render's`): the mix this second is owed has to REACH the bed. The
            // level lives in the player's `--volume=`, so a new answer is a new process; `bed_gain_ask`
            // is the stamp that says whether this beat changed anything, and nothing is re-cued when it
            // did not — otherwise every 100 ms beat would seek the middle of every clip.
            let bed_gain = narrate_preview_media::bed_gain(&cut, heard.at, speaking, game_volume, volume);
            let recued = RUNNING.with(|slot| {
                slot.borrow_mut().as_mut().and_then(|preview| {
                    if !preview.bed_gain_ask(bed_gain) {
                        return None;
                    }
                    preview.recue_bed(bed_gain, from, &tree, &cut, &mut spawn_player)
                })
            });
            if let Some((before, after)) = recued {
                crate::ui::window::log_line(&format!(
                    "s3 bed at {}: {} -> {}",
                    crate::narrate_screen::time_field(heard.at),
                    argv_volume(&before),
                    argv_volume(&after)
                ));
            }
            // S2: the picture's own second moves here, one tick at a time and at the cut's rate — without it
            // `tick` is asked the same second forever and only a gap skip ever moves the head.
            let at = narrate_preview::advance(from, crate::preview::TICK_SECONDS, heard.rate, &cut.segs);
            if at != from {
                page.set_head(at);
            }
            // A pause for a missing take is handled here rather than inside `step`: the wait is a dial
            // to a server, and `step` stays the pure decide-and-cue half so it can be driven by a test
            // without a socket.
            let stepped = {
                let waiting = RUNNING.with(|slot| {
                    slot.borrow()
                        .as_ref()
                        .and_then(|preview| preview.waiting_line())
                });
                match waiting {
                    Some(line) => match entries.get(line) {
                        Some(entry) => {
                            let (said, at) =
                                synthesize_line(&tree, entry, line, volume, &mut spawn_player);
                            page.set_head(at);
                            page.say(&said);
                            Stepped::Same
                        }
                        None => {
                            // The line went away while its take was being asked for: drop the wait so
                            // the timer does not sit paused on a row that no longer exists.
                            clear_waiting();
                            Stepped::Same
                        }
                    },
                    None => {
                        let stepped =
                            tick_loop(
                                &tree,
                                &cut,
                                &entries,
                                at,
                                game_volume,
                                volume,
                                page.no_narration(),
                                &mut spawn_player,
                            );
                        if let Stepped::Moved { at, rate: _ } = stepped {
                            page.set_head(at);
                        }
                        stepped
                    }
                }
            };
            // §6's ending is news only about a preview that was actually being heard. `running()` only says a
            // slot is held, and a slot can hold children that already exited — a player that spawns and leaves
            // looks exactly like one that ran to its end — so life is read from THIS beat's own children. Over
            // no live players the tick merely walked off the last clip of a cut nobody was listening to, and
            // saying so would speak over whatever press just happened (an audition's "spoken \u{2014} <take>",
            // for one); a page whose `playing` was left up from an earlier flow has nothing for ⏹ to have
            // stopped either.
            let ends_a_preview =
                matches!(stepped, Stepped::Paused) && was_running && !running_children().is_empty();
            let ends_a_preview =
                matches!(stepped, Stepped::Paused) && was_running && !running_children().is_empty();
            if let Some(said) = stepped_said(&stepped)
                .filter(|_| !matches!(stepped, Stepped::Paused) || ends_a_preview)
            {
                page.say(&said);
            }
            // S3: name the mix this tick applied, on the log only (not the status line), so a hush
            // typed into a scene shows up on the run rather than silently changing the mix.
            if !heard.hushed.is_empty() || heard.duck < 1.0 {
                crate::ui::window::log_line(&format!(
                    "s3 at {}: duck {:.2} rate {:.2} hushed [{}]",
                    crate::narrate_screen::time_field(heard.at),
                    heard.duck,
                    heard.rate,
                    heard.hushed.join(", ")
                ));
            }
            if ends_a_preview {
                let _ = stop_running();
                // No ⏹ was pressed, so nothing else would tell the page its preview is over (§5).
                // Only do it when a preview was actually going: a page whose `playing` was a staged or
                // stale value has no players here to end, and answering `Paused` for it would
                // contradict the page it just told the leg to trust.
                page.stopped();
            }
            glib::ControlFlow::Continue
        },
    );
}

/// The `--volume=` an argv carries, for the log line that names what the bed was really handed.
fn argv_volume(argv: &[String]) -> String {
    argv.iter()
        .find(|arg| arg.starts_with("--volume="))
        .cloned()
        .unwrap_or_else(|| "--volume=?".to_string())
}

/// Drop the picture's wait on a take without moving the picture: the row disappeared mid-synthesis.
fn clear_waiting() {
    RUNNING.with(|slot| {
        if let Some(preview) = slot.borrow_mut().as_mut() {
            preview.waiting = None;
            preview.paused = false;
        }
    });
}

/// S2: which takes exist. A file check per line against the take path F4.4 writes to
/// ([`narrate_tts::take_path`]), with the refused set winning over a file that exists: a take the
/// server refused and then some older run left on disk must still read as refused for this session, or
/// S2's sticky branch is unreachable.
///
/// `no_narration` is the project's flag, and the question "may this line be spoken" goes through
/// [`crate::narrate_off::lines_to_speak`] rather than a local test of its own: §5's captions-only clause
/// and F4.8 name that seam as the ONE answer, so a preview over a narration that is switched off offers
/// no line at all instead of pausing to synthesize wavs nobody will hear.
pub fn take_states(
    tree: &crate::layout::Tree,
    entries: &[Entry],
    failed: &Failed,
    no_narration: bool,
) -> Vec<Take> {
    let speakable = crate::narrate_off::lines_to_speak(no_narration, entries);
    entries
        .iter()
        .map(|entry| {
            if !speakable.iter().any(|spoken| spoken == entry) {
                // Nothing to look for: `line_at` never offers a line the seam excluded, and a file check
                // here would be a disk read for a line that is never spoken.
                return Take::Missing;
            }
            let key = take_key(entry);
            if failed.holds(&key) {
                return Take::Failed;
            }
            if narrate_tts::take_path(tree, &key).is_file() {
                Take::Exists
            } else {
                Take::Missing
            }
        })
        .collect()
}

/// The take's key, spelled the way F4.4 spells it (`narration::key_for`), so the file the preview
/// looks for and the file the speech leg wrote are named by one rule.
fn take_key(entry: &Entry) -> String {
    narration::key_for(entry, Some(&voice_of(entry)))
}

/// F4.5 S1/S2: where a line's take wav is, if there is audio to hear. `None` when the entry is blank
/// (nothing was spoken), when no file has been written for its key, or when the speech server refused
/// this session's copy of it — the same three questions `take_states` asks the running preview, in one
/// public door so a caller outside the tick loop cannot disagree with it about what is speakable.
pub fn line_take(tree: &crate::layout::Tree, entries: &[Entry], line: usize) -> Option<std::path::PathBuf> {
    let entry = entries.get(line)?;
    if entry.text.trim().is_empty() {
        return None;
    }
    let path = take_file(tree, entries, line)?;
    let refused = RUNNING.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|preview| preview.failed.holds(&take_key(entry)))
            .unwrap_or(false)
    });
    (!refused && path.exists()).then_some(path)
}

/// S2: the wav path of one line, for the voice to play and for a failure to name.
pub fn take_file(tree: &crate::layout::Tree, entries: &[Entry], line: usize) -> Option<std::path::PathBuf> {
    entries
        .get(line)
        .map(take_key)
        .map(|key| narrate_tts::take_path(tree, &key))
}

/// The voice this line's take was keyed to. The page's chosen voice is what F4.4 keys on, and the
/// entry itself does not carry one: `None` here means the leg is being driven without a voice
/// published (a fresh preview), which is the same key the row's ▶ used before a voice was chosen.
fn voice_of(_entry: &Entry) -> String {
    VOICE_CHOICE.with(|voice| voice.borrow().clone())
}

thread_local! {
    /// The voice the current session synthesizes with, published by the page. Part of the take key, so
    /// the preview looks for the file the row's ▶ actually wrote and not for one from another voice.
    static VOICE_CHOICE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Publish the voice the preview keys its takes on. Called by the page wherever the voice changes, so
/// a lap of the cut after picking a different voice looks for THAT voice's files.
pub fn set_voice(voice: &str) {
    VOICE_CHOICE.with(|slot| *slot.borrow_mut() = voice.to_string());
}

/// The voice the preview is keyed to right now.
pub fn voice() -> String {
    VOICE_CHOICE.with(|slot| slot.borrow().clone())
}

/// S3: how loud the voice plays while it speaks. The clip is ducked by the game volume (S3's
/// "whole clip ducked"), and the voice itself sits at that same level over the bed rather than on top
/// of it — with a floor, because a `game_volume` of 0 means "the bed is silent", not "the narration
/// must be inaudible too".
///
/// [`narrate_preview::duck`] is the rule for the BED; this is the voice side of the same number, and
/// the two are kept apart deliberately: handing the duck straight to the voice would mean a
/// `game_volume` of 1.0 left nothing lowered anywhere, and the S3 test that pins `duck(true, 0.22) ==
/// 0.22` is about the bed, not about the take.
/// The take's own level while it speaks: `game_volume` clamped to [`VOICE_VOLUME_FLOOR`]. Public because
/// the row's ▶ audition (`ui::narrate_page::speak_this_line`) plays ONE take with no bed under it and must
/// ask for exactly the level a running preview would give that line — two rules for one number would let a
/// solo audition and a lap of the cut sound different for the same wav.
pub fn voice_gain(game_volume: f64) -> f64 {
    game_volume.clamp(VOICE_VOLUME_FLOOR, 1.0)
}

fn ducked_voice(game_volume: f64) -> f64 {
    voice_gain(game_volume)
}

/// The argv one player is asked for: the file, the offset to start at, and the gain to play it at.
/// Shared by both players so the picture's seek and the voice's start are the same three numbers in
/// the same order, and one test can read the shape once.
pub fn player_args(file: &str, offset: f64, gain: f64) -> Vec<String> {
    vec![
        file.to_string(),
        format!("start={offset:.3}"),
        format!("gain={gain:.3}"),
    ]
}

/// The voice's own argv — the same three arguments as the picture's, named for the voice so a log
/// line reads as the file it belongs to.
pub fn voice_args(file: &str, offset: f64, gain: f64) -> Vec<String> {
    player_args(file, offset, gain)
}

/// Spawn the picture through the seam, over the cut's OWN footage files at the level S3 says.
///
/// The argv is [`narrate_preview_media::bed_argv`], so three things happen at once: the player is handed real
/// files (not a placeholder name), the duck/hush reaches it as `--volume`, and the rate that second plays at
/// is expressed by `-s` — `gst-play-1.0` has no rate flag (measured on its own `--help-all`), which is the
/// same limitation [`crate::cut_play_back::set_rate`] documents for the Cut page's player. The app's clock
/// still applies the rate ([`narrate_preview::seek_rate`]), so "speed effects apply at seeks" holds at the
/// seek itself rather than only in arithmetic.
fn player(
    program: &str,
    tree: &crate::layout::Tree,
    cut: &Cut,
    offset: f64,
    gain: f64,
    hold: bool,
    spawn: &mut Spawn,
) -> Result<(Child, narrate_preview_media::Cue), String> {
    // A running preview's cue is never the still-frame one: §6 reserves that for an audition over a clip with
    // no footage, which has no footage to cue at all.
    narrate_preview_media::cue_picture(
        program,
        tree,
        cut,
        offset,
        gain,
        if hold {
            narrate_preview_media::Freeze::Paused
        } else {
            narrate_preview_media::Freeze::Play
        },
        spawn,
    )
}

/// Cue a STILL frame at `at` — §6's "a line auditioned from its own ▶ over a clip with no footage … speaks
/// against a still frame". Public because the row's ▶ owns this leg; see [`narrate_preview_media::cue_picture`].
pub fn still_frame(
    tree: &crate::layout::Tree,
    cut: &Cut,
    offset: f64,
    gain: f64,
    spawn: &mut Spawn,
) -> Result<(Child, narrate_preview_media::Cue), String> {
    let program = picture_program();
    narrate_preview_media::cue_picture(
        &program,
        tree,
        cut,
        offset,
        gain,
        narrate_preview_media::Freeze::StillFrame,
        spawn,
    )
}

/// S1/S6: start the one running preview from `from` (the press's answer) and hand back the sentence.
/// Fills the thread-local the tick loop drives, so a press and the ticks that follow share one
/// `Preview` and one set of pids.
pub fn start_running(
    tree: &crate::layout::Tree,
    cut: &Cut,
    entries: &[Entry],
    from: f64,
    game_volume: f64,
    volume: f64,
    spawn: &mut Spawn,
) -> Result<String, String> {
    // Stop whatever was running before starting again, so a re-cue never leaves the previous pair of
    // players alive behind the new ones (§F4.5 S6's "both players" is about ALL of them).
    let _ = stop_running();
    PICTURE_PROGRAM.with(|slot| *slot.borrow_mut() = picture_program());
    VOICE_PROGRAM.with(|slot| *slot.borrow_mut() = voice_program());
    let mut preview = Preview {
        game_volume,
        volume,
        ..Default::default()
    };
    let said = preview.start(tree, cut, entries, from, spawn)?;
    RUNNING.with(|slot| *slot.borrow_mut() = Some(preview));
    Ok(said)
}

/// Whether the one running preview is going right now — neither stopped nor waiting on a take.
/// Cheap enough for the timer to ask every tick, which is what keeps the timer free when nothing runs.
pub fn running() -> bool {
    RUNNING.with(|slot| slot.borrow().is_some())
}

/// S6's pause half and its release: freeze or thaw BOTH players of the running preview. `None` = no preview
/// at all, so that press starts one instead; a `false` when thawing means nothing was held, so it re-cues.
pub fn hold_paused(hold: bool) -> Option<bool> {
    RUNNING.with(|slot| slot.borrow_mut().as_mut().map(|p| p.set_paused(hold)))
}

/// S6: stop the running preview, kill both children, and report the rules module's struct so
/// `hand_play_back` answers from the same value. A preview that was never started stops nothing and
/// says so with the same struct (both players come back `picture_paused`/`voice_paused`, since
/// nothing is playing), with no pids to report.
pub fn stop_running() -> (narrate_preview::Stopped, Vec<u32>) {
    let said = RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        match held.as_mut() {
            Some(preview) => preview.stop(),
            None => (narrate_preview::stop(), Vec::new()),
        }
    });
    // `Preview::stop` clears its own flag, but the slot is what `hold_paused` reads: leaving a stopped
    // preview in place would make the next ⏸ answer a hold over nothing and the next ▶ thaw dead pids.
    RUNNING.with(|slot| *slot.borrow_mut() = None);
    said
}

/// The one running preview's pids, for the page to register with `run::Subprocesses` so ⏹ reaches
/// them through F0.3's existing drain.
pub fn running_children() -> Vec<u32> {
    RUNNING.with(|slot| {
        held_children(&mut slot.borrow_mut())
    })
}

fn held_children(held: &mut Option<Preview>) -> Vec<u32> {
    held.as_ref().map(Preview::children).unwrap_or_default()
}

/// S4: a seek that lands on the cut. The rules answer where; this moves the picture's head by
/// restarting the picture player at that second with the rate that second plays at, so the snap is
/// not just arithmetic the page shows but the number the player was asked for.
///
/// Cues however much of a preview exists — running, held, or not started at all (a still) — because "drag to
/// seek" asks to move the picture, not to move it only when the answer is interesting. Answers `None` only
/// when there is nothing to cue at all (no recording covers the cut), so the caller can say why.
pub fn seek(
    tree: &crate::layout::Tree,
    cut: &Cut,
    target: f64,
    forward: bool,
    spawn: &mut Spawn,
) -> Option<(f64, f64)> {
    if narrate_preview_media::picture_files(tree, cut).is_empty() {
        return None;
    }
    let snapped = narrate_preview::snap_seek(target, &cut.segs, forward);
    let rate = narrate_preview::seek_rate(&cut.fx, snapped);
    RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        // No preview yet: this seek makes the still, so the slot stays empty and nothing is left running.
        let Some(preview) = held.as_mut() else {
            let gain = narrate_preview_media::bed_gain(cut, snapped, false, 1.0, 1.0);
            let _ = player(&picture_program(), tree, cut, snapped, gain, true, spawn);
            return;
        };
        let was_held = preview.paused;
        preview.frozen = snapped;
        let gain = narrate_preview_media::bed_gain(
            cut,
            snapped,
            preview.speaking.is_some(),
            preview.game_volume,
            preview.volume,
        );
        if let Ok((child, _cue)) = player(&picture_program(), tree, cut, snapped, gain, was_held, spawn) {
            preview.picture = Some(child);
            // A seek is a fresh cue: the stamp follows it, or the next tick would read the duck it just
            // replaced as "already applied".
            preview.bed_gain = Some(gain);
        }
    });
    Some((snapped, rate))
}

/// S5: where the preview goes when a row is selected — the line's lead-in, from the rules module.
/// `None` when there is no such row, so the caller leaves the picture alone rather than jumping to 0.
pub fn lead_in_of(entries: &[Entry], index: usize) -> Option<f64> {
    entries.get(index).map(|_| narrate_preview::lead_in(index, entries))
}

/// S2: the door F4.4's row ▶ goes through before it dials the speech server: free this take so a
/// second failure reads as a second failure. Delegates to [`narrate_preview::retry_line`] over the
/// ONE set the preview is reading, so the row and a lap of the cut cannot disagree about stickiness.
///
/// Answers whether there was a failure to clear (true = this press was a retry).
pub fn retry_wav(wav: &str) -> bool {
    RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        match held.as_mut() {
            Some(preview) => narrate_preview::retry_line(preview.failed_mut(), wav),
            // No preview running: nothing is sticky in this session, so the press is a first attempt.
            None => false,
        }
    })
}

/// S2: remember that a take failed, from the row's own ▶, so a lap of the cut that reaches it says
/// `sticky_failed` rather than synthesizing it again.
pub fn note_failed_wav(wav: &str) {
    RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        if let Some(preview) = held.as_mut() {
            preview.failed_mut().add(wav);
        }
    });
}

/// The status-line sentence one `Stepped` answer is worth. Kept with the enum so the timer and a test
/// read the same words for the same branch, and neither can invent its own phrasing.
pub fn stepped_said(step: &Stepped) -> Option<String> {
    match step {
        Stepped::Same | Stepped::Moved { .. } | Stepped::Held => None,
        Stepped::Speaking { line, offset, gain } => Some(format!(
            "{} \u{b7} from {} at {gain:.2}",
            narrate_preview::line_ready(*line),
            crate::narrate_screen::time_field(*offset)
        )),
        Stepped::Synthesizing { line } => Some(narrate_preview::synthesizing(*line)),
        Stepped::Muted { said, .. } => Some(said.clone()),
        Stepped::Paused => Some(narrate_preview::HANDED_BACK.to_string()),
    }
}

/// S3: the game volume this preview ducks by, read off the project that owns the session. Loaded per
/// press rather than carried: a project edited while a preview runs would otherwise keep ducking by the
/// old number.
///
/// With no project to read (a session folder with no `.naivepost` yet) this answers the project's own
/// default, which is what `Project::default()` spells for P.policy.gameVolume — not a literal here.
pub fn game_volume_for(tree: &crate::layout::Tree) -> f64 {
    crate::project::load(tree.dir())
        .map(|project| project.produce.game_volume)
        .unwrap_or_else(|_| crate::project::Produce::default().game_volume)
}

/// S3: the three numbers the preview hands its players for one second — the bed's duck, the rate to
/// seek at, and the lanes the scene under the line does not hear. One read per tick so the three
/// cannot drift apart, and each one is the same call the render makes.
pub struct Heard {
    /// The bed's level while this line speaks ([`narrate_preview::duck`]).
    pub duck: f64,
    /// The rate at the seek ([`narrate_preview::seek_rate`]).
    pub rate: f64,
    /// The lanes hushed by the scene under the line ([`narrate_preview::lanes_heard`]).
    pub hushed: Vec<String>,
    /// The second these were read at.
    pub at: f64,
}

/// S3: read the three for one second.
pub fn heard_at(cut: &Cut, at: f64, line_speaking: bool, game_volume: f64) -> Heard {
    Heard {
        duck: narrate_preview::duck(line_speaking, game_volume),
        rate: narrate_preview::seek_rate(&cut.fx, at),
        hushed: narrate_preview::lanes_heard(cut, at),
        at,
    }
}

/// S2's wait, run for real: synthesize the line over the same wire the row's ▶ uses
/// ([`crate::speak_leg::speak`], which re-reads the Settings per call) and report where the picture
/// goes. Returns the sentence for the status line and the second to resume at.
///
/// This is what makes S2's "synthesizing line N" / "line N ready" real: the string is said before the
/// dial and the resume second comes from the rules module, so a page that prints one without the other
/// cannot happen from here.
pub fn synthesize_line(
    tree: &crate::layout::Tree,
    entry: &Entry,
    line: usize,
    volume: f64,
    spawn: &mut Spawn,
) -> (String, f64) {
    let key = take_key(entry);
    let seed = narration::tts_seed(&key);
    let language = crate::project::load(tree.dir())
        .map(|project| project.language)
        .unwrap_or_default();
    let starting = narrate_preview::synthesizing(line);
    let outcome = crate::speak_leg::speak(tree, &entry.text, &entry.emotion, seed, &key, &language);
    let line_start = entry.s + entry.at;
    match outcome {
        narrate_tts::Outcome::Take(wav) => {
            RUNNING.with(|slot| {
                let mut held = slot.borrow_mut();
                if let Some(preview) = held.as_mut() {
                    let at = preview.resume_spoken(line, line_start, &wav.to_string_lossy(), spawn);
                    return (format!("{} \u{2014} {}", starting, narrate_preview::line_ready(line)), at);
                }
                (narrate_preview::line_ready(line), line_start)
            })
        }
        narrate_tts::Outcome::Refused(why) => {
            let at = RUNNING.with(|slot| {
                let mut held = slot.borrow_mut();
                match held.as_mut() {
                    Some(preview) => preview.resume_failed(&key),
                    None => narrate_preview::resume_after_synthesis(line_start, line_start, false),
                }
            });
            let said = format!("{} \u{b7} {}", narrate_preview::failed_playing_on(line), why);
            let _ = volume;
            (said, at)
        }
    }
}

/// The cut this leg needs: the page holds segments rather than a whole `Cut`, so one place builds it.
/// The effects are what S3's rate and the hush read, and they travel with the cut rather than as a
/// second argument to every call.
pub fn cut_of(segs: Vec<Seg>, fx: Vec<Fx>) -> Cut {
    Cut {
        segs,
        fx,
        ..Default::default()
    }
}
