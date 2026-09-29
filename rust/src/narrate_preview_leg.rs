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
    pub fn start(
        &mut self,
        tree: &crate::layout::Tree,
        cut: &Cut,
        entries: &[Entry],
        from: f64,
        spawn: &mut Spawn,
    ) -> Result<String, String> {
        let _ = (tree, entries);
        self.paused = false;
        self.waiting = None;
        self.speaking = None;
        self.frozen = from;
        let rate = narrate_preview::seek_rate(&cut.fx, from);
        let program = PICTURE_PROGRAM.with(|name| name.borrow().clone());
        let picture = player(&program, from, rate, spawn)?;
        self.picture = Some(picture);
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

    /// The set of refused takes this preview is reading. Public so the page hands a row's retry into the
    /// SAME set rather than keeping a second one that would disagree about what is sticky.
    pub fn failed_mut(&mut self) -> &mut Failed {
        &mut self.failed
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
    spawn: &mut Spawn,
) -> Stepped {
    let takes = take_states(tree, entries, &preview.failed);
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
pub fn tick_loop(
    tree: &crate::layout::Tree,
    cut: &Cut,
    entries: &[Entry],
    at: f64,
    game_volume: f64,
    volume: f64,
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
        step(preview, tree, cut, entries, at, spawn)
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
    /// Print one sentence on the status line.
    fn say(&self, said: &str);
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
        std::time::Duration::from_millis(crate::preview::TICK_MS),
        move || {
            if !page.running() {
                return glib::ControlFlow::Continue;
            }
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
                            tick_loop(&tree, &cut, &entries, from, game_volume, volume, &mut spawn_player);
                        if let Stepped::Moved { at, rate: _ } = stepped {
                            page.set_head(at);
                        }
                        stepped
                    }
                }
            };
            if let Some(said) = stepped_said(&stepped) {
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
            if matches!(stepped, Stepped::Paused) {
                let _ = stop_running();
            }
            glib::ControlFlow::Continue
        },
    );
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
pub fn take_states(tree: &crate::layout::Tree, entries: &[Entry], failed: &Failed) -> Vec<Take> {
    entries
        .iter()
        .map(|entry| {
            if entry.text.trim().is_empty() {
                // A silent line has no take to look for: `line_at` never offers it, and a file check
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

/// The take's key, spelled the way F4.4 spells it (`narration::tts_key`), so the file the preview
/// looks for and the file the speech leg wrote are named by one rule.
fn take_key(entry: &Entry) -> String {
    narration::tts_key(entry, Some(&voice_of(entry)), None)
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
fn ducked_voice(game_volume: f64) -> f64 {
    game_volume.clamp(VOICE_VOLUME_FLOOR, 1.0)
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

/// Spawn one player through the seam.
fn player(
    program: &str,
    offset: f64,
    rate: f64,
    spawn: &mut Spawn,
) -> Result<Child, String> {
    // The picture takes the rate rather than a gain: S3 puts speed at the seek, and the picture has
    // no duck of its own to apply.
    spawn(program, &player_args("cut", offset, rate))
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

/// S6: stop the running preview, kill both children, and report the rules module's struct so
/// `hand_play_back` answers from the same value. A preview that was never started stops nothing and
/// says so with the same struct (both players come back `picture_paused`/`voice_paused`, since
/// nothing is playing), with no pids to report.
pub fn stop_running() -> (narrate_preview::Stopped, Vec<u32>) {
    RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        match held.as_mut() {
            Some(preview) => preview.stop(),
            None => (narrate_preview::stop(), Vec::new()),
        }
    })
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
/// Returns the snapped second and the rate sought at, or `None` when the target was already on the
/// cut and the player needed no restart.
pub fn seek(
    tree: &crate::layout::Tree,
    cut: &Cut,
    target: f64,
    forward: bool,
    spawn: &mut Spawn,
) -> Option<(f64, f64)> {
    let snapped = narrate_preview::snap_seek(target, &cut.segs, forward);
    if (snapped - target).abs() < f64::EPSILON {
        return None;
    }
    let rate = narrate_preview::seek_rate(&cut.fx, snapped);
    RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        if let Some(preview) = held.as_mut() {
            preview.frozen = snapped;
            if let Ok(child) = spawn(&picture_program(), &player_args("cut", snapped, rate)) {
                preview.picture = Some(child);
            }
        }
    });
    let _ = tree;
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
