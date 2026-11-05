//! F2.2 S5 — ▶✂ silences the sound past a kept stretch's end in the stream itself, so what the output already holds
//! past the end is silence however far ahead it runs. 09-account, "Ethereum," | "wie": the jump came after the output
//! had been handed the start of the dropped word. Real GStreamer: a playbin with the app's own sound filter, its sound
//! captured on the file's own clock after a seek.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;
use naivepost::play_built_in::{silence_from, silencer};

fn have_tools() -> bool {
    gst::init().is_ok()
        && gst::ElementFactory::find("playbin").is_some()
        && std::process::Command::new("ffmpeg").arg("-version").output().is_ok()
}

#[test]
fn f2_2_s5_the_sound_past_a_stretchs_end_is_silenced_in_the_stream() {
    if !have_tools() {
        eprintln!("no GStreamer playbin or ffmpeg here: not checked");
        return;
    }
    let dir = crate::common::temp_dir_created("silencer");
    let file = dir.join("tone.wav");
    let made = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=6"])
        .arg(&file)
        .status()
        .expect("ffmpeg");
    assert!(made.success());

    let playbin = gst::ElementFactory::make("playbin").build().expect("playbin");
    playbin.set_property("uri", gst::glib::filename_to_uri(&file, None).unwrap().as_str());
    playbin.set_property("audio-filter", &silencer().expect("the app's sound filter"));
    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("signal-handoffs", true)
        .build()
        .expect("fakesink");
    playbin.set_property("audio-sink", &sink);
    // (stream second, loudest sample) per buffer, as the output received them.
    let heard: Arc<Mutex<Vec<(f64, f32)>>> = Arc::default();
    let into = heard.clone();
    sink.connect("handoff", false, move |values| {
        let buffer = values[1].get::<gst::Buffer>().ok()?;
        let pad = values[2].get::<gst::Pad>().ok()?;
        let segment = pad.sticky_event::<gst::event::Segment>(0)?.segment().clone().downcast::<gst::ClockTime>().ok()?;
        let at = segment.to_stream_time(buffer.pts()?)?.nseconds() as f64 / 1e9;
        let map = buffer.map_readable().ok()?;
        let loudest = map
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs())
            .fold(0.0f32, f32::max);
        into.lock().unwrap().push((at, loudest));
        None
    });

    // As ▶✂ does: the stretch's end on the file's clock is in place before anything decodes, then the cue (seek).
    silence_from(Some(3.0));
    playbin.set_state(gst::State::Paused).expect("pauses");
    let _ = playbin.state(gst::ClockTime::from_seconds(5));
    playbin
        .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, gst::ClockTime::from_seconds(2))
        .expect("seeks");
    let _ = playbin.state(gst::ClockTime::from_seconds(5));
    heard.lock().unwrap().clear();
    playbin.set_state(gst::State::Playing).expect("plays");
    let bus = playbin.bus().unwrap();
    let _ = bus.timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Eos, gst::MessageType::Error]);
    playbin.set_state(gst::State::Null).unwrap();
    silence_from(None);

    let heard = heard.lock().unwrap().clone();
    assert!(!heard.is_empty(), "the sound reached the output");
    let before: Vec<&(f64, f32)> = heard.iter().filter(|(at, _)| *at >= 2.0 && *at < 2.9).collect();
    let after: Vec<&(f64, f32)> = heard.iter().filter(|(at, _)| *at >= 3.0).collect();
    assert!(!before.is_empty() && before.iter().all(|(_, loud)| *loud > 0.1), "the stretch itself plays: {before:?}");
    assert!(!after.is_empty() && after.iter().all(|(_, loud)| *loud == 0.0), "nothing past its end is heard: {:?}", &after[..after.len().min(5)]);
    // The buffer that straddles the end keeps its sound up to the end and no further.
    assert!(heard.iter().any(|(at, _)| *at >= 2.9 && *at < 3.0), "a buffer runs up to the end");
}
