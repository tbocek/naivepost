//! F2.1's built-in player: the recording played INSIDE the preview, the way the prototype plays it (`go/player.go`):
//! one `playbin` whose video sink is `gtk4paintablesink`, so its frames land on a `GdkPaintable` the preview's
//! picture shows (inventory §A: "preview (GtkPicture contain-fit, min height 160, click toggles play)"), and whose
//! sound is the box's own audio sink, its loudness the playbin's `volume` property.
//!
//! The first port played through `gst-play-1.0` as a subprocess ([`crate::play_back`]), which on a desktop opens a
//! window of its own and left the preview standing on one frame. That subprocess is still what plays when a player
//! program is NAMED (`NAIVEPOST_PLAYER`): a person, or a test with its fake script, asking for an outside player.
//! Otherwise this one plays whenever GStreamer starts and has both `playbin` and the GTK4 sink; without the sink the
//! preview would be a black square (`release.sh` refuses to build without it for the same reason).
//!
//! Everything here runs on the GTK thread, which owns the sink's paintable: one pipeline, held in a thread-local and
//! reused from cue to cue. That thread never waits (§03-shell §1): a cue sets the pipeline going and returns, and
//! a 20 ms poll on the same thread finishes it (preroll, seek, preroll again, play) as GStreamer gets there. Until
//! then the player answers as the cue asked: [`position`] is the cued second and [`playing`] what it will be. The
//! first rewrite waited up to 3 s twice inside every cue, on every seek a line move made.

use std::cell::RefCell;

use gstreamer as gst;
use gstreamer::prelude::*;

/// How long a cue may wait for the pipeline to preroll before its seek: a seek is only honoured once the first
/// frame is decoded, and a local file prerolls in well under this.
const PREROLL_WAIT_S: u64 = 3;

/// How often a cue in flight looks at the pipeline again.
const CUE_POLL_MS: u64 = 20;

/// A cue that has not landed yet: where it stands, whether it plays, and which step it is on.
#[derive(Clone, Copy)]
struct Pending {
    generation: u64,
    at_secs: f64,
    play: bool,
    seeked: bool,
    since: std::time::Instant,
}

/// The one pipeline, and what it was last given.
struct Pipe {
    playbin: gst::Element,
    sink: gst::Element,
    /// The file the playbin holds, as handed to [`cue`].
    file: String,
    /// The stream ran out (end of stream seen on the bus) since the last cue.
    ended: bool,
    /// The cue in flight, if one is: its poll finishes it.
    pending: Option<Pending>,
    /// Bumped by every cue, so a newer cue's poll is the only one that acts.
    generation: u64,
}

thread_local! {
    static PIPE: RefCell<Option<Pipe>> = const { RefCell::new(None) };
}

/// Is the built-in player the one ▶ plays through? Not while a player program is named (`NAIVEPOST_PLAYER`, read
/// per press like every other player setting); otherwise whenever GStreamer has `playbin` and the GTK4 sink.
pub fn in_use() -> bool {
    std::env::var("NAIVEPOST_PLAYER")
        .unwrap_or_default()
        .trim()
        .is_empty()
        && available()
}

/// GStreamer started, and has the two elements this player is made of. Asked once per process. Public for a page
/// whose player program is named by a setting of its own (the Narrate picture's `PICTURE_PLAYER`).
pub fn available() -> bool {
    static OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OK.get_or_init(|| {
        gst::init().is_ok()
            && gst::ElementFactory::find("playbin").is_some()
            && gst::ElementFactory::find("gtk4paintablesink").is_some()
    })
}

/// The pipeline: `playbin` with the GTK4 sink as its video sink.
fn build() -> Result<Pipe, String> {
    let sink = gst::ElementFactory::make("gtk4paintablesink")
        .build()
        .map_err(|err| format!("gtk4paintablesink: {err}"))?;
    let playbin = gst::ElementFactory::make("playbin")
        .property("video-sink", &sink)
        .build()
        .map_err(|err| format!("playbin: {err}"))?;
    Ok(Pipe {
        playbin,
        sink,
        file: String::new(),
        ended: false,
        pending: None,
        generation: 0,
    })
}

/// Stand on `file` at `at_secs` into the file, playing when `play` says so and held on that frame otherwise. Same
/// file: a seek; another file: the playbin is handed it. Returns at once: the preroll (bounded by
/// [`PREROLL_WAIT_S`]), the seek and the play are finished by a poll ([`finish_cue`]), and a pipeline that will
/// not start says why in the log, from the bus's own error when it has one. What fails before anything is
/// waited for (no such file, a pipeline that will not pause) still comes back here.
pub fn cue(file: &str, at_secs: f64, play: bool, log: &dyn Fn(&str)) -> Result<(), String> {
    let _ = log;
    let generation = PIPE.with(|slot| -> Result<u64, String> {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(build()?);
        }
        let pipe = slot.as_mut().expect("built just above");
        if pipe.file != file {
            let _ = pipe.playbin.set_state(gst::State::Null);
            let uri =
                gst::glib::filename_to_uri(file, None).map_err(|err| format!("{file}: {err}"))?;
            pipe.playbin.set_property("uri", uri.as_str());
            pipe.file = file.to_string();
        }
        pipe.ended = false;
        if pipe.playbin.set_state(gst::State::Paused).is_err() {
            return Err(failed(pipe, "the pipeline would not pause"));
        }
        pipe.generation += 1;
        pipe.pending = Some(Pending {
            generation: pipe.generation,
            at_secs,
            play,
            seeked: false,
            since: std::time::Instant::now(),
        });
        Ok(pipe.generation)
    })?;
    // A pipeline already prerolled on this file finishes here and now; one that is not goes on in the poll.
    if finish_cue(generation) {
        glib::timeout_add_local(std::time::Duration::from_millis(CUE_POLL_MS), move || {
            if finish_cue(generation) {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }
    Ok(())
}

/// The pipeline's own reason for failing, else `what`; the pipeline goes down.
fn failed(pipe: &Pipe, what: &str) -> String {
    let said = pipe
        .playbin
        .bus()
        .and_then(|bus| bus.pop_filtered(&[gst::MessageType::Error]))
        .and_then(|message| match message.view() {
            gst::MessageView::Error(err) => Some(err.error().to_string()),
            _ => None,
        })
        .unwrap_or_else(|| what.to_string());
    let _ = pipe.playbin.set_state(gst::State::Null);
    format!("{}: {said}", pipe.file)
}

/// One step of cue `generation`, never waiting: answers whether it still has a step to come. A newer cue ends it.
fn finish_cue(generation: u64) -> bool {
    let failure = PIPE.with(|slot| -> Option<String> {
        let mut slot = slot.borrow_mut();
        let pipe = slot.as_mut()?;
        let pending = pipe
            .pending
            .filter(|pending| pending.generation == generation)?;
        let (state, _, _) = pipe.playbin.state(gst::ClockTime::ZERO);
        match state {
            Err(_) => {
                pipe.pending = None;
                return Some(failed(pipe, "the pipeline did not preroll"));
            }
            Ok(gst::StateChangeSuccess::Async) => {
                if pending.since.elapsed() > std::time::Duration::from_secs(PREROLL_WAIT_S) {
                    pipe.pending = None;
                    return Some(failed(pipe, "the pipeline did not preroll"));
                }
                return None;
            }
            Ok(_) => {}
        }
        if !pending.seeked {
            let at = gst::ClockTime::from_nseconds((pending.at_secs.max(0.0) * 1e9) as u64);
            if pipe
                .playbin
                .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, at)
                .is_err()
            {
                pipe.pending = None;
                return Some(failed(pipe, "the seek was refused"));
            }
            // A flushing seek prerolls again; the next steps wait for it, so the position reads the new second
            // before anything follows the player (until then `position` answers the cued second).
            pipe.pending = Some(Pending {
                seeked: true,
                since: std::time::Instant::now(),
                ..pending
            });
            return None;
        }
        pipe.pending = None;
        if pending.play {
            if pipe.playbin.set_state(gst::State::Playing).is_err() {
                return Some(failed(pipe, "the pipeline would not play"));
            }
            crate::ui::window::log_line(&format!(
                "    built-in player: {} from {}",
                pipe.file,
                crate::tools::mm_ss(pending.at_secs)
            ));
        }
        None
    });
    if let Some(why) = failure {
        crate::ui::window::log_line(&crate::preview::playback_failure(
            "the built-in player",
            &why,
        ));
        return false;
    }
    PIPE.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|pipe| pipe.pending)
            .is_some_and(|pending| pending.generation == generation)
    })
}

/// Pause, keeping the frame on the picture. Answers whether it was playing.
pub fn pause() -> bool {
    with_pipe(|pipe| {
        // A cue still on its way to playing is told to hold instead.
        let mut was = false;
        if let Some(pending) = pipe.pending.as_mut() {
            was = pending.play;
            pending.play = false;
        }
        let was = was || pipe.playbin.current_state() == gst::State::Playing;
        if was {
            let _ = pipe.playbin.set_state(gst::State::Paused);
        }
        was
    })
    .unwrap_or(false)
}

/// Play on from where it is held (a stop's still released, F2.2 S5). Answers whether a pipeline was there.
pub fn resume() -> bool {
    with_pipe(|pipe| !pipe.file.is_empty() && pipe.playbin.set_state(gst::State::Playing).is_ok())
        .unwrap_or(false)
}

/// Play at `rate` from where it stands (F2.2 S5 "speed effects at their flat rate"): a flushing seek that carries
/// the rate, the prototype's `SetRateNow`. Answers whether the pipeline took it.
pub fn set_rate(rate: f64) -> bool {
    with_pipe(|pipe| {
        let Some(at) = pipe.playbin.query_position::<gst::ClockTime>() else {
            return false;
        };
        pipe.playbin
            .seek(
                rate,
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::SeekType::Set,
                at,
                gst::SeekType::None,
                gst::ClockTime::NONE,
            )
            .is_ok()
    })
    .unwrap_or(false)
}

/// Let the file go: the pipeline down to nothing, so the next cue starts clean and nothing holds the file.
pub fn stop() {
    with_pipe(|pipe| {
        let _ = pipe.playbin.set_state(gst::State::Null);
        pipe.file.clear();
        pipe.ended = false;
        pipe.pending = None;
    });
}

/// Is it playing right now?
pub fn playing() -> bool {
    with_pipe(|pipe| match pipe.pending {
        // On its way: what the cue asked for.
        Some(pending) => pending.play,
        None => pipe.playbin.current_state() == gst::State::Playing,
    })
    .unwrap_or(false)
}

/// The file it holds, if any.
pub fn file() -> Option<String> {
    with_pipe(|pipe| (!pipe.file.is_empty()).then(|| pipe.file.clone())).flatten()
}

/// Where it stands in its file, in seconds; also reads the bus, so an end of stream is noticed here.
pub fn position() -> Option<f64> {
    with_pipe(|pipe| {
        drain(pipe);
        // A cue on its way stands where it was cued: the pipeline's own position is the old second until its
        // seek has settled, and a tick reading that would walk the line back.
        if let Some(pending) = pipe.pending {
            return Some(pending.at_secs);
        }
        pipe.playbin
            .query_position::<gst::ClockTime>()
            .map(|at| at.nseconds() as f64 / 1e9)
    })
    .flatten()
}

/// Did the file run out since the last cue?
pub fn ended() -> bool {
    with_pipe(|pipe| {
        drain(pipe);
        pipe.ended && pipe.pending.is_none()
    })
    .unwrap_or(false)
}

/// The playbin's own loudness (F2.5 S6: muted and levelled by property, never by stopping). Answers whether a
/// pipeline was there to ask.
pub fn set_volume(gain: f64) -> bool {
    with_pipe(|pipe| {
        if pipe.file.is_empty() {
            return false;
        }
        pipe.playbin
            .set_property("volume", gain.clamp(0.0, crate::cut_hear::MAX_GAIN));
        true
    })
    .unwrap_or(false)
}

/// The sink's paintable, for the preview's picture to show. `None` before the first cue.
pub fn paintable() -> Option<gst::glib::Object> {
    with_pipe(|pipe| pipe.sink.property::<gst::glib::Object>("paintable"))
}

fn with_pipe<T>(act: impl FnOnce(&mut Pipe) -> T) -> Option<T> {
    PIPE.with(|slot| slot.borrow_mut().as_mut().map(act))
}

/// Read what the bus holds: an end of stream marks the file run out; nothing else is kept, so the bus never grows.
fn drain(pipe: &mut Pipe) {
    let Some(bus) = pipe.playbin.bus() else {
        return;
    };
    while let Some(message) = bus.pop() {
        if let gst::MessageView::Eos(_) = message.view() {
            pipe.ended = true;
        }
    }
}
