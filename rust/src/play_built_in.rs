use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;

/// How long a cue may wait for the pipeline to preroll before its seek: a seek is only honoured once the first
/// frame is decoded, and a local file prerolls in well under this.
const PREROLL_WAIT_S: u64 = 3;

/// How often a cue in flight looks at the pipeline again.
const CUE_POLL_MS: u64 = 20;

/// The second of the playing file from which its sound is silenced in the stream (f64 bits; NaN for none): what ▶✂
/// sets to the end of the kept stretch it plays. Read on GStreamer's streaming thread, hence an atomic.
static SILENCE_FROM: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0x7ff8_0000_0000_0000);

/// Silence the playing file's sound from `local` seconds on, in the stream itself (F2.2 S5): whatever the sound
/// card has already been handed past a clip's end is then silence, however far ahead the output runs, so ▶✂ plays a
/// cut's end where the render cuts it. `None` lets everything play. A cue clears it.
pub fn silence_from(local: Option<f64>) {
    let bits = local.unwrap_or(f64::NAN).to_bits();
    SILENCE_FROM.store(bits, Ordering::Relaxed);
    // A pipeline that came in as the spare reads its own cell, not the process-wide one.
    with_pipe(|pipe| {
        if let Some(own) = &pipe.silence {
            own.store(bits, Ordering::Relaxed);
        }
    });
}

/// The second the stream is silenced from, if any.
pub fn silenced_from() -> Option<f64> {
    let at = f64::from_bits(SILENCE_FROM.load(std::sync::atomic::Ordering::Relaxed));
    (!at.is_nan()).then_some(at)
}

/// The audio filter the playbin runs its sound through: floats, and a probe that zeroes every sample at or past
/// [`silenced_from`] on the file's own clock (stream time). First rewrite: the jump at a clip's end came after the
/// output had been handed the next tens of milliseconds, so the start of a dropped word was heard (09-account,
/// "Ethereum," | "wie").
pub fn silencer() -> Result<gst::Element, String> {
    silencer_reading(None)
}

/// [`silencer`] reading its silence second from `own` instead of the process-wide one: the spare pipeline's filter,
/// which must not take the playing pipeline's answer (another file, another clock) while it prerolls.
fn silencer_reading(own: Option<Arc<AtomicU64>>) -> Result<gst::Element, String> {
    let bin = gst::parse::bin_from_description(
        "audioconvert ! audio/x-raw,format=F32LE,layout=interleaved ! identity name=silencer",
        true,
    )
    .map_err(|err| format!("the sound filter: {err}"))?;
    let pad = bin
        .by_name("silencer")
        .and_then(|element| element.static_pad("src"))
        .ok_or("the sound filter has no output")?;
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let from = match &own {
            Some(own) => f64::from_bits(own.load(Ordering::Relaxed)),
            None => f64::from_bits(SILENCE_FROM.load(Ordering::Relaxed)),
        };
        if from.is_nan() {
            return gst::PadProbeReturn::Ok;
        }
        let (Some(caps), Some(segment)) = (
            pad.current_caps(),
            pad.sticky_event::<gst::event::Segment>(0),
        ) else {
            return gst::PadProbeReturn::Ok;
        };
        let Some(structure) = caps.structure(0) else {
            return gst::PadProbeReturn::Ok;
        };
        let (Ok(rate), Ok(channels)) = (
            structure.get::<i32>("rate"),
            structure.get::<i32>("channels"),
        ) else {
            return gst::PadProbeReturn::Ok;
        };
        let Ok(segment) = segment.segment().clone().downcast::<gst::ClockTime>() else {
            return gst::PadProbeReturn::Ok;
        };
        if let Some(gst::PadProbeData::Buffer(ref mut buffer)) = info.data {
            let Some(start) = buffer.pts().and_then(|pts| segment.to_stream_time(pts)) else {
                return gst::PadProbeReturn::Ok;
            };
            silence_past(buffer.make_mut(), start.nseconds() as f64 / 1e9, from, rate as usize, channels as usize);
        }
        gst::PadProbeReturn::Ok
    });
    Ok(bin.upcast())
}

/// Zero the samples of `buffer` (interleaved f32 starting at `start` seconds) that sit at or past `from`.
fn silence_past(buffer: &mut gst::BufferRef, start: f64, from: f64, rate: usize, channels: usize) {
    let Ok(mut map) = buffer.map_writable() else {
        return;
    };
    let frame = 4 * channels.max(1);
    let frames = map.len() / frame;
    let first = ((from - start) * rate as f64).ceil().max(0.0) as usize;
    if first >= frames {
        return;
    }
    map[first * frame..].fill(0);
}

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
    /// The silence second this pipeline's filter reads, when it has one of its own (a pipeline built as the spare);
    /// `None` reads the process-wide [`silenced_from`].
    silence: Option<Arc<AtomicU64>>,
}

/// The spare: a second pipeline prerolled at the next jump's second and held paused, so the join is a swap of
/// pipelines and not a reload (F2.2 S5). `at` is the second of `pipe.file` it was asked for.
struct Spare {
    pipe: Pipe,
    at: f64,
}

/// How close the second a swap is asked for must be to the one the spare prerolled: float slack between two
/// numbers the same arithmetic produced, not a search radius.
const SPARE_MATCH_S: f64 = 0.05;

thread_local! {
    static PIPE: RefCell<Option<Pipe>> = const { RefCell::new(None) };
    static SPARE: RefCell<Option<Spare>> = const { RefCell::new(None) };
    /// Every spare pipeline gets a generation of its own, so a poll left over from a dropped spare never acts on the next.
    static SPARE_GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// How many joins were crossed by a swap since the process started; a test's evidence that the join did not reload.
    static SWAPPED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
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
    build_reading(None)
}

/// [`build`] with the sound filter reading `silence` (the spare's own cell), or the process-wide second for `None`.
fn build_reading(silence: Option<Arc<AtomicU64>>) -> Result<Pipe, String> {
    let sink = gst::ElementFactory::make("gtk4paintablesink")
        .build()
        .map_err(|err| format!("gtk4paintablesink: {err}"))?;
    let playbin = gst::ElementFactory::make("playbin")
        .property("video-sink", &sink)
        .build()
        .map_err(|err| format!("playbin: {err}"))?;
    // The sound passes the silencer, so a cut's end is kept to the sample however far ahead the output runs.
    if let Ok(filter) = silencer_reading(silence.clone()) {
        playbin.set_property("audio-filter", &filter);
    }
    Ok(Pipe {
        playbin,
        sink,
        file: String::new(),
        ended: false,
        pending: None,
        generation: 0,
        silence,
    })
}

pub fn cue(file: &str, at_secs: f64, play: bool) -> Result<(), String> {
    cue_silenced(file, at_secs, play, None)
}

/// [`cue`], with the second of `file` its sound is silenced from set BEFORE the pipeline starts decoding, so not one
/// buffer of the new place passes the filter without it (the decoder runs ahead of the sound you hear).
pub fn cue_silenced(file: &str, at_secs: f64, play: bool, silence: Option<f64>) -> Result<(), String> {
    silence_from(silence);
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

/// One step of cue `generation` on `pipe`, never waiting: `Ok(true)` while it still has a step to come, `Ok(false)`
/// once it has landed (or when a newer cue has taken over), `Err` with the pipeline's reason when it failed.
fn step_cue(pipe: &mut Pipe, generation: u64) -> Result<bool, String> {
    let Some(pending) = pipe
        .pending
        .filter(|pending| pending.generation == generation)
    else {
        return Ok(false);
    };
    let (state, _, _) = pipe.playbin.state(gst::ClockTime::ZERO);
    match state {
        Err(_) => {
            pipe.pending = None;
            return Err(failed(pipe, "the pipeline did not preroll"));
        }
        Ok(gst::StateChangeSuccess::Async) => {
            if pending.since.elapsed() > std::time::Duration::from_secs(PREROLL_WAIT_S) {
                pipe.pending = None;
                return Err(failed(pipe, "the pipeline did not preroll"));
            }
            return Ok(true);
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
            return Err(failed(pipe, "the seek was refused"));
        }
        // A flushing seek prerolls again; the next steps wait for it, so the position reads the new second
        // before anything follows the player (until then `position` answers the cued second).
        pipe.pending = Some(Pending {
            seeked: true,
            since: std::time::Instant::now(),
            ..pending
        });
        return Ok(true);
    }
    pipe.pending = None;
    if pending.play {
        if pipe.playbin.set_state(gst::State::Playing).is_err() {
            return Err(failed(pipe, "the pipeline would not play"));
        }
        crate::ui::window::log_line(&format!(
            "    built-in player: {} from {}",
            pipe.file,
            crate::tools::mm_ss(pending.at_secs)
        ));
    }
    Ok(false)
}

/// One step of cue `generation` on the playing pipeline: answers whether it still has a step to come. A newer cue ends it.
fn finish_cue(generation: u64) -> bool {
    match PIPE.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .map(|pipe| step_cue(pipe, generation))
    }) {
        Some(Ok(more)) => more,
        Some(Err(why)) => {
            crate::ui::window::log_line(&crate::preview::playback_failure(
                "the built-in player",
                &why,
            ));
            false
        }
        None => false,
    }
}

/// One step of the spare's preroll `generation`: answers whether it still has a step to come. A spare that fails
/// is dropped, with its reason in the log; the join it was for is then a reload, as every join was before it.
fn finish_spare(generation: u64) -> bool {
    let stepped = SPARE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let spare = slot.as_mut()?;
        if spare.pipe.generation != generation {
            return None;
        }
        Some(step_cue(&mut spare.pipe, generation))
    });
    match stepped {
        Some(Ok(more)) => more,
        Some(Err(why)) => {
            crate::ui::window::log_line(&format!(
                "    built-in player: the spare could not preroll ({why}) \u{2014} the join is a reload"
            ));
            SPARE.with(|slot| *slot.borrow_mut() = None);
            false
        }
        None => false,
    }
}

/// F2.2 S5: preroll the spare pipeline at `at_secs` of `file`, held paused with its first frame decoded and its sound
/// silenced from `silence` (that file's second) on. Asked every tick while a jump is within the lead; a spare already
/// sitting on this file and second (or on its way there) is left alone. A new ask drops the previous spare.
pub fn preroll(file: &str, at_secs: f64, silence: Option<f64>) -> Result<(), String> {
    let same = SPARE.with(|slot| {
        slot.borrow().as_ref().is_some_and(|spare| {
            spare.pipe.file == file && (spare.at - at_secs).abs() < SPARE_MATCH_S
        })
    });
    if same {
        return Ok(());
    }
    drop_spare();
    let own = Arc::new(AtomicU64::new(silence.unwrap_or(f64::NAN).to_bits()));
    let mut pipe = build_reading(Some(own))?;
    let uri = gst::glib::filename_to_uri(file, None).map_err(|err| format!("{file}: {err}"))?;
    pipe.playbin.set_property("uri", uri.as_str());
    pipe.file = file.to_string();
    if pipe.playbin.set_state(gst::State::Paused).is_err() {
        return Err(failed(&pipe, "the spare would not pause"));
    }
    let generation = SPARE_GENERATION.with(|cell| {
        let next = cell.get() + 1;
        cell.set(next);
        next
    });
    pipe.generation = generation;
    pipe.pending = Some(Pending {
        generation,
        at_secs,
        play: false,
        seeked: false,
        since: std::time::Instant::now(),
    });
    SPARE.with(|slot| *slot.borrow_mut() = Some(Spare { pipe, at: at_secs }));
    crate::ui::window::log_line(&format!(
        "    built-in player: the spare prerolls {file} from {}",
        crate::tools::mm_ss(at_secs)
    ));
    if finish_spare(generation) {
        glib::timeout_add_local(std::time::Duration::from_millis(CUE_POLL_MS), move || {
            if finish_spare(generation) {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }
    Ok(())
}

/// Is a spare sitting prerolled at `at_secs` of `file`, ready to be swapped in?
pub fn spare_ready(file: &str, at_secs: f64) -> bool {
    SPARE.with(|slot| {
        slot.borrow().as_ref().is_some_and(|spare| {
            spare.pipe.pending.is_none()
                && spare.pipe.file == file
                && (spare.at - at_secs).abs() < SPARE_MATCH_S
        })
    })
}

/// The join: the spare prerolled at `at_secs` of `file` becomes the playing pipeline, silenced from `silence` on and
/// set playing, and the one that was playing goes down. The picture is not re-pointed here (`cut_picture::show_video`
/// does that, as after any cue). Answers whether there was a spare to swap; `false` means cue as usual.
pub fn swap_in(file: &str, at_secs: f64, silence: Option<f64>) -> bool {
    if !spare_ready(file, at_secs) {
        // Say why the join reloads: the one line that tells a held spare apart from none at all.
        let held = SPARE.with(|slot| {
            slot.borrow().as_ref().map(|spare| {
                format!(
                    "{} from {}{}",
                    spare.pipe.file,
                    crate::tools::mm_ss(spare.at),
                    if spare.pipe.pending.is_some() {
                        ", still prerolling"
                    } else {
                        ""
                    }
                )
            })
        });
        crate::ui::window::log_line(&format!(
            "    built-in player: the jump to {file} from {} finds {} \u{2014} reloading",
            crate::tools::mm_ss(at_secs),
            held.map(|h| format!("the spare holding {h}"))
                .unwrap_or_else(|| "no spare".to_string())
        ));
        return false;
    }
    let Some(spare) = SPARE.with(|slot| slot.borrow_mut().take()) else {
        return false;
    };
    let outgoing = PIPE.with(|slot| slot.borrow_mut().replace(spare.pipe));
    if let Some(old) = outgoing {
        let _ = old.playbin.set_state(gst::State::Null);
    }
    // The silence before the first sample plays: this pipeline's own cell and the process-wide second both.
    silence_from(silence);
    let played = with_pipe(|pipe| {
        pipe.ended = false;
        pipe.playbin.set_state(gst::State::Playing).is_ok()
    })
    .unwrap_or(false);
    if !played {
        let why = with_pipe(|pipe| failed(pipe, "the spare would not play")).unwrap_or_default();
        crate::ui::window::log_line(&crate::preview::playback_failure(
            "the built-in player",
            &why,
        ));
        return false;
    }
    SWAPPED.with(|cell| cell.set(cell.get() + 1));
    crate::ui::window::log_line(&format!(
        "    built-in player: {file} from {} \u{2014} the spare pipeline was already sitting there, swapped in",
        crate::tools::mm_ss(at_secs)
    ));
    true
}

/// Drop the spare, if one is held: paused, playing on, or the cut changed under it.
pub fn drop_spare() -> bool {
    match SPARE.with(|slot| slot.borrow_mut().take()) {
        Some(spare) => {
            let _ = spare.pipe.playbin.set_state(gst::State::Null);
            true
        }
        None => false,
    }
}

/// How many joins a swap crossed since the process started. Test-only evidence that the join did not reload.
pub fn swapped_for_test() -> u64 {
    SWAPPED.with(|cell| cell.get())
}

/// Pause, keeping the frame on the picture. Answers whether it was playing.
pub fn pause() -> bool {
    with_pipe(|pipe| {
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
    drop_spare();
}

/// Is it playing right now?
pub fn playing() -> bool {
    with_pipe(|pipe| match pipe.pending {
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
