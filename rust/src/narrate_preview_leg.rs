//! F4.5: the leg the preview runs ON, over the two real players (§A.3: video `n.player`, narration audio `n.voice`).
//! Decides nothing: every answer is a call into [`crate::narrate_preview`], which holds the rules and no process state.
//! The seam is the player PROGRAM, read per spawn from Settings (`llm.conf`) or `NAIVEPOST_*_PLAYER` (§02-services#1).
//! No sound hardware here (`/dev/snd` is absent), so tests assert the argv, the registered pid, the duck/rate/lanes
//! numbers and the synthesized file, not what is heard.

use std::process::Child;
use std::rc::Rc;

use crate::cut::{Cut, Fx, Seg};
use crate::narrate_preview::{self, Failed, Take};
use crate::narrate_preview_media;
use crate::narrate_tts;
use crate::narration::{self, Entry};
use crate::settings::{self, Conf};

/// `P.eng.picturePlayer`. Spec silent on the program (only §A.3 "Two players" and §F4.5 S6 "⏹ stops both"): the
/// GStreamer CLI, as its OWN process because S6 needs two pids to kill and a paintable sink in the window has none.
pub const PICTURE_PLAYER: &str = "gst-play-1.0";

/// `P.eng.voicePlayer`: spec silent, same default as [`PICTURE_PLAYER`].
pub const VOICE_PLAYER: &str = "gst-play-1.0";

pub const PICTURE_PLAYER_KEY: &str = "PICTURE_PLAYER";
pub const VOICE_PLAYER_KEY: &str = "VOICE_PLAYER";

/// `P.eng.voicePlayerFloor`: spec silent; keeps a `game_volume` of 0 from spawning an inaudible voice. 0.02 is −34 dB.
pub const VOICE_VOLUME_FLOOR: f64 = 0.02;

/// What spawns a player; injectable so a test can watch the argv while still getting a real pid (as `produce_exec::spawn_tool`).
pub type Spawn = dyn FnMut(&str, &[String]) -> Result<Child, String>;

/// [`crate::subprocess::spawn_piped`], which never reaps, so the pid stays killable.
pub fn spawn_player(program: &str, args: &[String]) -> Result<Child, String> {
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::subprocess::spawn_piped(program, &borrowed).map_err(|err| {
        format!(
            "{} failed: {err}",
            crate::subprocess::shown(program, &borrowed)
        )
    })
}

pub struct Preview {
    pub game_volume: f64,
    pub volume: f64,
    paused: bool,
    waiting: Option<usize>,
    speaking: Option<usize>,
    /// The second to resume at when the wait ends, per [`narrate_preview::resume_after_synthesis`].
    frozen: f64,
    failed: Failed,
    picture: Option<Picture>,
    voice: Option<Child>,
    /// The gain the live bed was last asked for; an equal answer spawns nothing (as [`crate::play_back::set_volume`]).
    bed_gain: Option<f64>,
}

pub enum Picture {
    Process(Child),
    /// `start` is the session second of the file's first second.
    BuiltIn {
        start: f64,
    },
}

impl Picture {
    fn pid(&self) -> Option<u32> {
        match self {
            Picture::Process(child) => Some(child.id()),
            Picture::BuiltIn { .. } => None,
        }
    }
}

thread_local! {
    static BUILT_IN_SHOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn picture_built_in() -> bool {
    BUILT_IN_SHOWN.with(|cell| cell.get())
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
    /// S1: start the preview at `from`, with S3's rate from [`narrate_preview::seek_rate`]. A picture that will not
    /// spawn is reported, not swallowed: it is the "nothing to show" case for the person.
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
        let gain = narrate_preview_media::bed_gain(cut, from, false, self.game_volume, self.volume);
        let program = PICTURE_PROGRAM.with(|name| name.borrow().clone());
        let (picture, _cue) = player(&program, tree, cut, from, gain, false, spawn)?;
        self.picture = Some(picture);
        self.bed_gain = Some(gain);
        // S3: said out loud so a hush typed into a scene shows on the run.
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

    /// S2's wait: the picture is PAUSED at `t` (its child stays) while line `line` is synthesized; `t` feeds
    /// [`narrate_preview::resume_after_synthesis`] on failure.
    pub fn pause_for(&mut self, line: usize, t: f64) {
        self.paused = true;
        self.waiting = Some(line);
        self.frozen = t;
    }

    /// S2: the take arrived. Resume at the LINE's start and point the voice at the written file (S3: the take's own
    /// floor, never the duck, which is for the game bed). Returns the second the picture moved to.
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
        // Drop the old voice first so one preview never has two wavs over the same second; dropping a child does not end it.
        end_child(self.voice.take());
        if let Ok(child) = spawn(
            &VOICE_PROGRAM.with(|name| name.borrow().clone()),
            &voice_args(wav, 0.0, ducked_voice(self.game_volume)),
        ) {
            self.voice = Some(child);
        }
        at
    }

    pub fn resume_failed(&mut self, wav: &str) -> f64 {
        let at = narrate_preview::resume_after_synthesis(self.frozen, self.frozen, false);
        self.paused = false;
        self.waiting = None;
        self.failed.add(wav);
        at
    }

    /// S6: stop BOTH players, reporting through the rules module's struct, and hand back the pids killed
    /// ([`crate::stop_legs::signal`]: TERM then KILL) so F0.3's ⏹ drain cannot signal one twice.
    pub fn stop(&mut self) -> (narrate_preview::Stopped, Vec<u32>) {
        // Take both children out FIRST; an unreaped killed `Child` stays a zombie and reads as alive.
        let picture = match self.picture.take() {
            Some(Picture::Process(child)) => Some(child),
            Some(Picture::BuiltIn { .. }) => {
                crate::play_built_in::pause();
                BUILT_IN_SHOWN.with(|cell| cell.set(false));
                None
            }
            None => None,
        };
        let voice = self.voice.take();
        let pids: Vec<u32> = [picture.as_ref(), voice.as_ref()]
            .into_iter()
            .flatten()
            .map(Child::id)
            .collect();
        let actually = crate::stop_legs::signal(&pids);
        // Reaped on a thread: a child ignoring SIGTERM lives until the KILL, and the window thread never waits (§03-shell §1).
        let reaped: Vec<Child> = picture.into_iter().chain(voice).collect();
        std::thread::spawn(move || {
            for mut child in reaped {
                let _ = child.wait();
            }
        });
        self.paused = false;
        self.waiting = None;
        self.speaking = None;
        (narrate_preview::stop(), actually)
    }

    pub fn speaking(&self) -> Option<usize> {
        self.speaking
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn waiting_line(&self) -> Option<usize> {
        self.waiting
    }

    pub fn children(&self) -> Vec<u32> {
        self.picture
            .iter()
            .filter_map(Picture::pid)
            .chain(self.voice.iter().map(Child::id))
            .collect()
    }

    /// §6 (`Preview`): has the PICTURE player run to its end by itself? `try_wait` reaps it. Only the picture answers:
    /// the voice outliving the picture is the bug, and asking about it would stop the preview for a take that finished first.
    pub fn picture_finished(&mut self) -> bool {
        match self.picture.as_mut() {
            Some(Picture::Process(child)) => matches!(child.try_wait(), Ok(Some(_))),
            Some(Picture::BuiltIn { .. }) => crate::play_built_in::ended(),
            // No picture means no answer: the press that tried reports it.
            None => false,
        }
    }

    pub fn forget_picture(&mut self) {
        self.picture = None;
    }

    /// End the narration half only: the take still speaking when the picture stops. Ended outright, not with SIGTERM
    /// (this container's `sleep` carries on, [`crate::stop_legs::signal`] escalates, and per-tick escalation would stall the beat), then reaped.
    pub fn end_voice(&mut self) -> Option<u32> {
        let mut child = self.voice.take()?;
        let pid = child.id();
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
        let _ = child.wait();
        self.speaking = None;
        Some(pid)
    }

    /// Freeze both players (`SIGSTOP`/`SIGCONT`); they stay in the slot so ⏹ still owns them. A freeze also ends the take's wait (S2: a STILL, not a dial).
    pub fn set_paused(&mut self, hold: bool) -> bool {
        let was = std::mem::replace(&mut self.paused, hold);
        if hold {
            self.waiting = None;
        }
        narrate_preview_media::signal_pids(&self.children(), hold);
        if matches!(self.picture, Some(Picture::BuiltIn { .. })) {
            if hold {
                crate::play_built_in::pause();
            } else {
                crate::play_built_in::resume();
            }
        }
        hold || was
    }

    pub fn failed_mut(&mut self) -> &mut Failed {
        &mut self.failed
    }

    /// S3's stamp: whether `gain` is a NEW answer; re-cuing every 100 ms beat would seek mid-clip for an unmoved duck.
    pub fn bed_gain_ask(&mut self, gain: f64) -> bool {
        let changed = self.bed_gain != Some(gain);
        if changed {
            self.bed_gain = Some(gain);
        }
        changed
    }

    pub fn picture_second(&self) -> Option<f64> {
        match self.picture {
            Some(Picture::BuiltIn { start }) if !self.paused && crate::play_built_in::playing() => {
                crate::play_built_in::position().map(|local| start + local)
            }
            _ => None,
        }
    }

    pub fn follow(&mut self, at: f64, tree: &crate::layout::Tree, cut: &Cut) {
        let Some(Picture::BuiltIn { start }) = self.picture else {
            return;
        };
        let there = crate::play_built_in::position()
            .is_some_and(|local| (start + local - at).abs() < FOLLOW_TOLERANCE_S);
        let file = narrate_preview_media::cue_files(tree, cut, at)
            .into_iter()
            .next();
        if there && file.as_deref() == crate::play_built_in::file().as_deref() {
            return;
        }
        let gain = self.bed_gain.unwrap_or(1.0);
        if let Ok((picture, _)) = built_in(tree, cut, at, gain, self.paused) {
            self.picture = Some(picture);
        }
    }

    /// S3: the bed's level lives in its command line, so a new answer needs a new process. Kill and re-cue the bed at `gain`
    /// over `at`, keeping the hold; answers the argv before and after, `None` when there is no picture or it will not spawn.
    pub fn recue_bed(
        &mut self,
        gain: f64,
        at: f64,
        tree: &crate::layout::Tree,
        cut: &Cut,
        spawn: &mut Spawn,
    ) -> Option<(Vec<String>, Vec<String>)> {
        self.picture.as_ref()?;
        if matches!(self.picture, Some(Picture::BuiltIn { .. })) {
            self.bed_gain = Some(gain);
            crate::play_built_in::set_volume(gain.clamp(0.0, 1.0));
            return None;
        }
        // The player clamps `--volume=`; log what it was really handed.
        let before = narrate_preview_media::bed_argv(
            &narrate_preview_media::cue_files(tree, cut, at),
            narrate_preview_media::footage_second(cut, at),
            gain.clamp(0.0, 1.0),
        );
        // Kill first: two beds over one second is double sound, and §F4.5 S6 counts ONE picture pid.
        self.picture = None;
        self.bed_gain = Some(gain);
        let program = PICTURE_PROGRAM.with(|name| name.borrow().clone());
        let (picture, cue) = player(&program, tree, cut, at, gain, self.paused, spawn).ok()?;
        self.picture = Some(picture);
        Some((before, cue.args))
    }
}

thread_local! {
    static RUNNING: std::cell::RefCell<Option<Preview>> = const { std::cell::RefCell::new(None) };
    static PICTURE_PROGRAM: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static VOICE_PROGRAM: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// `P.eng.picturePlayer`: the `llm.conf` line, else `NAIVEPOST_PICTURE_PLAYER`, else [`PICTURE_PLAYER`].
pub fn picture_program() -> String {
    named_program(
        PICTURE_PLAYER_KEY,
        "NAIVEPOST_PICTURE_PLAYER",
        PICTURE_PLAYER,
    )
}

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

/// One machine value out of `llm.conf`. The player keys are not drawn by the dialog yet, so they ride in the unknown-key list.
fn setting_value(conf: &Conf, key: &str) -> Option<String> {
    unknown_value(conf, key)
}

/// A key the settings writer carried through untouched. `Conf::unknown` is private, so the value is taken from the
/// rendered file and unquoted (`settings::quote`'s inverse is not public).
fn unknown_value(conf: &Conf, key: &str) -> Option<String> {
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

fn unquoted(value: &str) -> String {
    let body = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
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

/// What one tick did, for the status line and the clock.
#[derive(Debug, Clone, PartialEq)]
pub enum Stepped {
    Same,
    Moved { at: f64, rate: f64 },
    Held,
    Speaking { line: usize, offset: f64, gain: f64 },
    Synthesizing { line: usize },
    Muted { line: usize, said: String },
    Paused,
}

/// Advance one tick. `now` is the second the picture would be at if nothing held it; `entries` is the narration
/// record and `tree` the project folder (a take's existence is a file check, which makes S2's branches real).
pub fn step(
    preview: &mut Preview,
    tree: &crate::layout::Tree,
    cut: &Cut,
    entries: &[Entry],
    now: f64,
    spawn: &mut Spawn,
) -> Stepped {
    if preview.picture_finished() {
        preview.forget_picture();
        if let Some(pid) = preview.end_voice() {
            crate::ui::window::log_line(&format!(
                "preview: the picture ended, so the narration stopped with it (killed pid {pid})"
            ));
        }
        // The CALLER (`tick_loop`) empties the slot: it already holds the borrow, and re-borrowing here would panic.
        return Stepped::Paused;
    }
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
                if let Ok(child) = spawn(
                    &voice_program(),
                    &voice_args(&wav.to_string_lossy(), offset, gain),
                ) {
                    if let Some(mut previous) = preview.voice.replace(child) {
                        let _ = previous.kill();
                        let _ = previous.wait();
                    }
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

/// S1/S3/S6: one tick of the thread-local preview: starts it if none runs (the picture click's S1), else advances it,
/// and returns the sentence to print. Keeps `ui/narrate_page.rs` to one call per tick.
#[allow(clippy::too_many_arguments)] // one beat of the preview reads all eight
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

/// What the tick loop needs from the page, so this module holds no GTK-slot knowledge (as [`crate::cut_play_leg::CutPreviewPage`]).
pub trait PreviewPage {
    fn running(&self) -> bool;
    fn head(&self) -> f64;
    fn set_head(&self, at: f64);
    fn cut(&self) -> Cut;
    fn entries(&self) -> Vec<Entry>;
    /// The project folder the takes live in; `None` with no session open, and the timer then does nothing.
    fn tree(&self) -> Option<crate::layout::Tree>;
    fn game_volume(&self) -> f64;
    fn volume(&self) -> f64;
    fn say(&self, said: &str);
    /// The preview ended on its own (walked off the last clip), so the page clears its flags (§5's `started` half).
    fn stopped(&self) {}
}

/// F4.5 S2: the Narrate preview's 100 ms tick (§A.3), started once per window from the page's `wire`, like
/// [`crate::cut_play_leg::start_cut_preview_tick`]. A missing take pauses the picture and dials the speech server on
/// a worker ([`ask_for_take`]). No-ops when the preview is not running.
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
            let from = RUNNING
                .with(|slot| slot.borrow().as_ref().and_then(Preview::picture_second))
                .unwrap_or_else(|| page.head());
            let game_volume = page.game_volume();
            let volume = page.volume();
            let speaking = RUNNING.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .and_then(|preview| preview.speaking())
                    .is_some()
            });
            let heard = heard_at(&cut, from, speaking, game_volume);
            // S3: the mix this second is owed must REACH the bed, whose level lives in `--volume=`, so a new answer is a new
            // process; `bed_gain_ask` says whether this beat changed anything.
            let bed_gain =
                narrate_preview_media::bed_gain(&cut, heard.at, speaking, game_volume, volume);
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
            // S2: the picture's second moves here, one tick at a time at the cut's rate; otherwise `tick` is asked the same second forever.
            let at =
                narrate_preview::advance(from, crate::preview::TICK_SECONDS, heard.rate, &cut.segs);
            if at != from {
                page.set_head(at);
            }
            // A pause for a missing take is a dial to a server, so it is handled here and `step` stays testable without a socket.
            let stepped = {
                let waiting = RUNNING.with(|slot| {
                    slot.borrow()
                        .as_ref()
                        .and_then(|preview| preview.waiting_line())
                });
                match waiting {
                    Some(line) => match entries.get(line) {
                        Some(entry) => {
                            ask_for_take(page.clone(), &tree, entry, line);
                            Stepped::Same
                        }
                        None => {
                            clear_waiting();
                            Stepped::Same
                        }
                    },
                    None => {
                        let stepped = tick_loop(
                            &tree,
                            &cut,
                            &entries,
                            at,
                            game_volume,
                            volume,
                            &mut spawn_player,
                        );
                        if let Stepped::Moved { at, rate: _ } = stepped {
                            page.set_head(at);
                        }
                        stepped
                    }
                }
            };
            // §6's ending is news only about a preview that was actually heard. `running()` only says a slot is held, and a slot can
            // hold exited children, so read life from THIS beat's children; otherwise "spoken — <take>" gets talked over.
            // The line may have jumped (a skip, a resume): the built-in picture goes with it.
            let head = page.head();
            RUNNING.with(|slot| {
                if let Some(preview) = slot.borrow_mut().as_mut() {
                    preview.follow(head, &tree, &cut);
                }
            });
            let ends_a_preview = matches!(stepped, Stepped::Paused)
                && was_running
                && (!running_children().is_empty() || picture_built_in());
            if let Some(said) = stepped_said(&stepped)
                .filter(|_| !matches!(stepped, Stepped::Paused) || ends_a_preview)
            {
                page.say(&said);
            }
            // S3: name the mix on the log only, so a hush typed into a scene shows on the run.
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
                // No ⏹ was pressed, so nothing else tells the page the preview is over (§5); only when one was actually going.
                page.stopped();
            }
            glib::ControlFlow::Continue
        },
    );
}

fn argv_volume(argv: &[String]) -> String {
    argv.iter()
        .find(|arg| arg.starts_with("--volume="))
        .cloned()
        .unwrap_or_else(|| "--volume=?".to_string())
}

fn clear_waiting() {
    RUNNING.with(|slot| {
        if let Some(preview) = slot.borrow_mut().as_mut() {
            preview.waiting = None;
            preview.paused = false;
        }
    });
}

/// S2: which takes exist: a file check per line against [`narrate_tts::take_path`], the refused set winning over
/// an older file (or S2's sticky branch is unreachable). A blank line is a deliberate silence ([`crate::narration::lines_to_speak`]).
pub fn take_states(tree: &crate::layout::Tree, entries: &[Entry], failed: &Failed) -> Vec<Take> {
    let speakable = crate::narration::lines_to_speak(entries);
    entries
        .iter()
        .map(|entry| {
            if !speakable.iter().any(|spoken| spoken == entry) {
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

fn take_key(entry: &Entry) -> String {
    narration::key_for(entry, Some(&voice_of(entry)))
}

/// F4.5 S1/S2: where a line's take wav is, `None` when blank, unwritten, or refused this session; the same questions
/// `take_states` asks, in one public door.
pub fn line_take(
    tree: &crate::layout::Tree,
    entries: &[Entry],
    line: usize,
) -> Option<std::path::PathBuf> {
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

pub fn take_file(
    tree: &crate::layout::Tree,
    entries: &[Entry],
    line: usize,
) -> Option<std::path::PathBuf> {
    entries
        .get(line)
        .map(take_key)
        .map(|key| narrate_tts::take_path(tree, &key))
}

/// The voice this line's take was keyed to; the entry carries none, so this is the page's published voice.
fn voice_of(_entry: &Entry) -> String {
    VOICE_CHOICE.with(|voice| voice.borrow().clone())
}

thread_local! {
    /// The voice the session synthesizes with, part of the take key so the preview finds the file the row's ▶ wrote.
    static VOICE_CHOICE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

pub fn set_voice(voice: &str) {
    VOICE_CHOICE.with(|slot| *slot.borrow_mut() = voice.to_string());
}

pub fn voice() -> String {
    VOICE_CHOICE.with(|slot| slot.borrow().clone())
}

/// S3: how loud the voice plays while it speaks: `game_volume` clamped to [`VOICE_VOLUME_FLOOR`] (0 means the bed
/// is silent, not the narration). [`narrate_preview::duck`] is the BED's rule and is kept apart. Public because the
/// row's ▶ audition (`ui::narrate_page::speak_this_line`) must ask the level a running preview would give.
pub fn voice_gain(game_volume: f64) -> f64 {
    game_volume.clamp(VOICE_VOLUME_FLOOR, 1.0)
}

fn ducked_voice(game_volume: f64) -> f64 {
    voice_gain(game_volume)
}

pub fn voice_args(file: &str, offset: f64, gain: f64) -> Vec<String> {
    audio_argv(&voice_program(), file, offset, None, gain)
}

fn speaks_ffplay(program: &str) -> bool {
    std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("ffplay"))
}

/// ffplay takes a duration (`-t`); `gst-play-1.0` has none, so the caller's own clock ends the span.
pub fn voice_player_bounds_spans() -> bool {
    speaks_ffplay(&voice_program())
}

/// One sound file on a voice player, from `from` up to `to` where the player can be told an end, at `gain` (0..1),
/// with no window or keyboard, in the program's own flags: ffplay's `-nodisp -autoexit -ss -t` are refused by `gst-play-1.0`.
pub fn audio_argv(program: &str, file: &str, from: f64, to: Option<f64>, gain: f64) -> Vec<String> {
    let gain = gain.clamp(0.0, 1.0);
    if speaks_ffplay(program) {
        let mut args: Vec<String> = vec![
            "-nodisp".into(),
            "-autoexit".into(),
            "-loglevel".into(),
            "quiet".into(),
            "-volume".into(),
            format!("{}", (gain * 100.0).round() as i64),
            "-ss".into(),
            format!("{from:.3}"),
        ];
        if let Some(to) = to.filter(|to| to.is_finite()) {
            args.extend(["-t".into(), format!("{:.3}", (to - from).max(0.0))]);
        }
        args.extend(["-i".into(), file.to_string()]);
        return args;
    }
    vec![
        "--no-position".into(),
        "--no-interactive".into(),
        "--videosink=fakesink".into(),
        format!("--volume={gain:.3}"),
        "-s".into(),
        format!("{from:.3}"),
        file.to_string(),
    ]
}

/// Spawn the picture through the seam over the cut's OWN footage files at S3's level (argv from
/// [`narrate_preview_media::bed_argv`]). `gst-play-1.0` has no rate flag (as [`crate::cut_play_back::set_rate`]
/// documents), so the rate is expressed by `-s` and applied at seeks.
fn player(
    program: &str,
    tree: &crate::layout::Tree,
    cut: &Cut,
    offset: f64,
    gain: f64,
    hold: bool,
    spawn: &mut Spawn,
) -> Result<(Picture, narrate_preview_media::Cue), String> {
    if uses_built_in(program) {
        return built_in(tree, cut, offset, gain, hold);
    }
    // A running preview's cue is never the still-frame one; §6 reserves that for an audition over a clip with no footage.
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
    .map(|(child, cue)| (Picture::Process(child), cue))
}

const FOLLOW_TOLERANCE_S: f64 = 0.25;

fn uses_built_in(program: &str) -> bool {
    program == PICTURE_PLAYER && crate::play_built_in::available()
}

pub fn picture_is_built_in() -> bool {
    uses_built_in(&picture_program())
}

fn built_in(
    tree: &crate::layout::Tree,
    cut: &Cut,
    at: f64,
    gain: f64,
    hold: bool,
) -> Result<(Picture, narrate_preview_media::Cue), String> {
    let file = narrate_preview_media::cue_files(tree, cut, at)
        .into_iter()
        .next()
        .ok_or_else(|| narrate_preview::NO_RECORDING_AT_START.to_string())?;
    let local = narrate_preview_media::footage_second(cut, at);
    crate::play_built_in::cue(&file, local, !hold, &|line| {
        crate::ui::window::log_line(line)
    })?;
    crate::play_built_in::set_volume(gain.clamp(0.0, 1.0));
    BUILT_IN_SHOWN.with(|cell| cell.set(true));
    Ok((
        Picture::BuiltIn { start: at - local },
        narrate_preview_media::Cue {
            args: Vec::new(),
            pid: 0,
        },
    ))
}

/// Cue a STILL frame at `at` (§6: a line auditioned from its own ▶ over a clip with no footage); see [`narrate_preview_media::cue_picture`].
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

/// S1/S6: start the one running preview from `from` and hand back the sentence; a press and its ticks share one `Preview` and its pids.
pub fn start_running(
    tree: &crate::layout::Tree,
    cut: &Cut,
    entries: &[Entry],
    from: f64,
    game_volume: f64,
    volume: f64,
    spawn: &mut Spawn,
) -> Result<String, String> {
    // Stop the earlier pair first (§F4.5 S6's "both players" is ALL of them).
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

pub fn running() -> bool {
    RUNNING.with(|slot| slot.borrow().is_some())
}

/// S6: freeze or thaw BOTH players. `None` = no preview, so the press starts one; thawing `false` means nothing was held, so it re-cues.
pub fn hold_paused(hold: bool) -> Option<bool> {
    RUNNING.with(|slot| slot.borrow_mut().as_mut().map(|p| p.set_paused(hold)))
}

/// S6: stop the running preview and kill both children, reporting the rules module's struct. A never-started preview
/// stops nothing, reports both players paused, and has no pids.
pub fn stop_running() -> (narrate_preview::Stopped, Vec<u32>) {
    next_audition();
    let said = RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        match held.as_mut() {
            Some(preview) => preview.stop(),
            None => (narrate_preview::stop(), Vec::new()),
        }
    });
    // `hold_paused` reads the slot: a stopped preview left in it would make the next ⏸ a hold over nothing and ▶ thaw dead pids.
    RUNNING.with(|slot| *slot.borrow_mut() = None);
    said
}

thread_local! {
    static AUDITION: std::cell::RefCell<Option<Child>> = const { std::cell::RefCell::new(None) };
    static AUDITION_TURN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn end_child(child: Option<Child>) {
    if let Some(mut child) = child {
        let _ = child.kill();
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

pub fn next_audition() -> u64 {
    end_child(AUDITION.with(|slot| slot.borrow_mut().take()));
    AUDITION_TURN.with(|turn| {
        turn.set(turn.get() + 1);
        turn.get()
    })
}

pub fn audition_is_current(turn: u64) -> bool {
    AUDITION_TURN.with(std::cell::Cell::get) == turn
}

pub fn hold_audition(child: Child, line: Option<usize>) {
    end_child(AUDITION.with(|slot| slot.borrow_mut().replace(child)));
    RUNNING.with(|slot| {
        if let Some(preview) = slot.borrow_mut().as_mut() {
            preview.speaking = line;
        }
    });
}

pub fn running_children() -> Vec<u32> {
    RUNNING.with(|slot| held_children(&mut slot.borrow_mut()))
}

fn held_children(held: &mut Option<Preview>) -> Vec<u32> {
    held.as_ref().map(Preview::children).unwrap_or_default()
}

/// S4: a seek that lands on the cut. Restarts the picture player at that second and rate so the snap is the number the
/// player was asked for. Cues a running, held or unstarted (still) preview alike; `None` when no recording covers the cut.
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
        let Some(preview) = held.as_mut() else {
            if !uses_built_in(&picture_program()) {
                let gain = narrate_preview_media::bed_gain(cut, snapped, false, 1.0, 1.0);
                if let Err(why) = player(&picture_program(), tree, cut, snapped, gain, true, spawn)
                {
                    crate::ui::window::log_line(&format!(
                        "!!! the picture player could not start -- {why}"
                    ));
                }
            }
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
        if let Ok((picture, _cue)) = player(
            &picture_program(),
            tree,
            cut,
            snapped,
            gain,
            was_held,
            spawn,
        ) {
            preview.picture = Some(picture);
            // A seek is a fresh cue: stamp it, or the next tick reads the replaced duck as already applied.
            preview.bed_gain = Some(gain);
        }
    });
    Some((snapped, rate))
}

/// S5: where the preview goes when a row is selected (the line's lead-in); `None` with no such row.
pub fn lead_in_of(entries: &[Entry], index: usize) -> Option<f64> {
    entries
        .get(index)
        .map(|_| narrate_preview::lead_in(index, entries))
}

/// S2: free this take before the row ▶ dials the speech server, so a second failure reads as one ([`narrate_preview::retry_line`]
/// over the ONE set the preview reads). True = this press was a retry.
pub fn retry_wav(wav: &str) -> bool {
    RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        match held.as_mut() {
            Some(preview) => narrate_preview::retry_line(preview.failed_mut(), wav),
            None => false,
        }
    })
}

pub fn note_failed_wav(wav: &str) {
    RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        if let Some(preview) = held.as_mut() {
            preview.failed_mut().add(wav);
        }
    });
}

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

/// S3: the game volume this preview ducks by, loaded per press so an edit during a preview is heard; the project's default with none to read.
pub fn game_volume_for(tree: &crate::layout::Tree) -> f64 {
    crate::project::load(tree.dir())
        .map(|project| project.produce.game_volume)
        .unwrap_or_else(|_| crate::project::Produce::default().game_volume)
}

/// S3: the three numbers handed to the players for one second, read once per tick so they cannot drift apart.
pub struct Heard {
    pub duck: f64,
    pub rate: f64,
    pub hushed: Vec<String>,
    pub at: f64,
}

pub fn heard_at(cut: &Cut, at: f64, line_speaking: bool, game_volume: f64) -> Heard {
    Heard {
        duck: narrate_preview::duck(line_speaking, game_volume),
        rate: narrate_preview::seek_rate(&cut.fx, at),
        hushed: narrate_preview::lanes_heard(cut, at),
        at,
    }
}

thread_local! {
    /// The line a worker is dialling a take for, so the tick (which keeps beating while the picture waits) asks once.
    static ASKING: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// S2's pause for a missing take: dial the speech server for `line` on a worker (§03-shell §1: the window thread never
/// waits). When the take lands the voice picks the line up from its start; a refusal is made sticky and the picture plays on mute.
fn ask_for_take(page: Rc<dyn PreviewPage>, tree: &crate::layout::Tree, entry: &Entry, line: usize) {
    if ASKING.with(|asking| asking.get()) == Some(line) {
        return;
    }
    ASKING.with(|asking| asking.set(Some(line)));
    let key = take_key(entry);
    let seed = narration::tts_seed(&key);
    let starting = narrate_preview::synthesizing(line);
    page.say(&starting);
    let line_start = entry.s + entry.at;
    let (text, emotion) = (entry.text.clone(), entry.emotion.clone());
    let (worker_tree, worker_key) = (tree.clone(), key.clone());
    crate::ui::background::run(
        "preview take",
        move || {
            let language = crate::project::load(worker_tree.dir())
                .map(|project| project.policy.language.value)
                .unwrap_or_default();
            crate::speak_leg::speak(&worker_tree, &text, &emotion, seed, &worker_key, &language)
        },
        move |outcome| {
            ASKING.with(|asking| asking.set(None));
            if let Some((said, at)) = land_take(
                line,
                line_start,
                &key,
                outcome,
                &starting,
                &mut spawn_player,
            ) {
                page.set_head(at);
                page.say(&said);
            }
        },
    );
}

/// A take landed: the preview still waiting on that line resumes with it (or, refused, plays mute, sticky). `None`
/// when the preview was stopped or moved on; the take is just filed.
fn land_take(
    line: usize,
    line_start: f64,
    key: &str,
    outcome: narrate_tts::Outcome,
    starting: &str,
    spawn: &mut Spawn,
) -> Option<(String, f64)> {
    RUNNING.with(|slot| {
        let mut held = slot.borrow_mut();
        let preview = held.as_mut()?;
        if preview.waiting_line() != Some(line) {
            return None;
        }
        Some(match outcome {
            narrate_tts::Outcome::Take(wav) => {
                let at = preview.resume_spoken(line, line_start, &wav.to_string_lossy(), spawn);
                (
                    format!(
                        "{} \u{2014} {}",
                        starting,
                        narrate_preview::line_ready(line)
                    ),
                    at,
                )
            }
            narrate_tts::Outcome::Refused(why) => {
                let at = preview.resume_failed(key);
                let said = format!(
                    "{} \u{b7} {}",
                    narrate_preview::failed_playing_on(line),
                    why
                );
                (said, at)
            }
        })
    })
}

/// The cut this leg needs from segments and effects (S3's rate and hush read the effects).
pub fn cut_of(segs: Vec<Seg>, fx: Vec<Fx>) -> Cut {
    Cut {
        segs,
        fx,
        ..Default::default()
    }
}
