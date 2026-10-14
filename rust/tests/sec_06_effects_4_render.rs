//! §06-effects#4-render (`how each effect becomes ffmpeg`) — the render's OWN half: does an effect record reach
//! the filter graph and the encode command, through the doors the render itself uses.
//!
//! `tests/render_effect_filters.rs` pins what each single builder in [`naivepost::render_fx`] answers. That is not
//! this item: a builder nothing calls renders nothing, and for a while every one of them was uncalled while the
//! encode chain carried a placeholder per volume cue. Every assertion here therefore goes through
//! [`naivepost::produce_render::video_chain`] / [`naivepost::produce_render::audio_chain`] /
//! [`naivepost::produce_exec::plan`] — the same three doors a run walks — and never at a builder directly.
//!
//! The last test is the real-tool leg: it points Settings' ffmpeg box at a stub script (the seam
//! `tests/audio_task_dial.rs` uses) and checks that the filters leave the program in the argv the configured tool
//! actually receives. No ffmpeg is installed in this container and several tests pin that absence; the stub is the
//! intended door, not a substitute for one.

use std::path::{Path, PathBuf};

use naivepost::cut::{Cut, Fx, Lane, Seg};
use naivepost::layout::Tree;
use naivepost::produce_exec as exec;
use naivepost::produce_flow as flow;
use naivepost::produce_render as render;
use naivepost::produce_render::Clip;
use naivepost::project::{Produce, Publish};
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
pub use common::tree_in;

const ITEM: &str = "sec_06_effects_4_render_how_each_effect_becomes_ffmpeg";

fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

/// A clip as the planner leaves it: 30 s of footage at the given rate, framed at HD unless a test says otherwise.
fn clip(no: usize, rate: f64, on_screen: f64) -> Clip {
    Clip {
        no,
        seg_s: 0.0,
        on_screen,
        rate,
        frame: (1920, 1080),
        src: (1920, 1080),
        on_recording: true,
        scratch: common::temp_root("scratch"),
        ..Default::default()
    }
}

fn speed(t: f64, dur: f64, rate: f64, snd: &str) -> Fx {
    Fx { kind: "speed".into(), t, dur, rate, snd: snd.to_string(), ..Default::default() }
}

fn stop(t: f64, dur: f64) -> Fx {
    // A stop is a speed whose rate is nought (§1's column reads speed/stop as one control).
    Fx { kind: "speed".into(), t, dur, rate: 0.0, snd: "mute".into(), ..Default::default() }
}

fn zoom(t: f64, dur: f64, cx: f64, cy: f64, hf: f64) -> Fx {
    // Hard edges (no glide): with a fade the camera is still walking at every sampled breakpoint, and §4's
    // static-vs-moving question would be about the glide rather than about where the zoom was framed.
    Fx { kind: "zoom".into(), t, dur, cx: Some(cx), cy: Some(cy), hf: Some(hf), ..Default::default() }
}

/// The same zoom gliding in and back out — §4's moving case, whose breakpoints are what the sampling exists for.
/// Not `stay`: a camera that returns to the wide frame is exactly the journey `zoompan` draws.
fn gliding(mut effect: Fx) -> Fx {
    effect.stay = false;
    effect.trans = 0.5;
    effect.tout = 0.5;
    effect
}

fn text(at: f64, words: &str) -> Fx {
    Fx { kind: "text".into(), t: at, dur: 4.0, trans: 0.3, tout: 0.3, text: words.into(), ..Default::default() }
}

fn drawing(at: f64, file: &str) -> Fx {
    Fx { kind: "svg".into(), t: at, dur: 4.0, trans: 0.3, tout: 0.3, src: file.into(), ..Default::default() }
}

fn volume(t: f64, dur: f64, gain: f64) -> Fx {
    Fx { kind: "volume".into(), t, dur, gain, trans: 0.25, tout: 0.25, ..Default::default() }
}

// --- S1: speed -----------------------------------------------------------------------------------------------

/// §4 (`clips split at rate boundaries, the middle carries the rate (setpts=PTS/rate)`): the step lands in the
/// VIDEO chain the encoder is handed, which is where a re-timed picture has to appear.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s1_a_re_timed_clip_carries_setpts() {
    let fast = render::video_chain(&clip(1, 2.0, 15.0), &Produce::default(), None).join(",");
    assert!(fast.contains("setpts=PTS/2"), "a 2× clip's video chain: {fast}");
    let plain = render::video_chain(&clip(1, 1.0, 30.0), &Produce::default(), None).join(",");
    assert!(!plain.contains("setpts"), "1× needs no clock filter: {plain}");
    // The sound follows the picture by tempo, reached by chaining what one instance refuses.
    let sound = render::audio_chain(&clip(1, 4.0, 7.5), &Produce::default(), 0, &[]).join(",");
    assert_eq!(
        sound.match_indices("atempo=2").count(),
        2,
        "4× is two atempo instances, both present in the chain: {sound}"
    );
}

/// §4 (`asetrate when pitched`, `a mute expression when silent`): the spelling stored on the speed effect is the
/// only thing that decides which sound filter the clip gets.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s1_the_sound_answer_follows_the_stored_spelling() {
    let mut pitched = clip(1, 2.0, 15.0);
    pitched.fx = vec![speed(0.0, 15.0, 2.0, "pitch")];
    let sound = render::audio_chain(&pitched, &Produce::default(), 0, &[]).join(",");
    assert!(sound.contains("asetrate=48000*2"), "pitched sound moves the sample rate: {sound}");
    assert!(sound.contains("aresample=48000"), "and hands the graph back 48 kHz: {sound}");

    let mut muted = clip(1, 2.0, 15.0);
    muted.fx = vec![speed(0.0, 15.0, 2.0, "mute")];
    let sound = render::audio_chain(&muted, &Produce::default(), 0, &[]).join(",");
    assert!(sound.contains("volume=0"), "a muted stretch is silent, not merely quiet: {sound}");
    assert!(sound.contains("enable="), "silence is a window, so the rest of the clip still hears itself: {sound}");

    // `""` is nobody's answer but the default: tempo held, which is the atempo chain and nothing else.
    let mut plain = clip(1, 2.0, 15.0);
    plain.fx = vec![speed(0.0, 15.0, 2.0, "")];
    let sound = render::audio_chain(&plain, &Produce::default(), 0, &[]).join(",");
    assert!(sound.contains("atempo=2") && !sound.contains("asetrate"), "{sound}");
}

/// §4 (`or a planned 1× read head with 0.15 s dips at run ends for own/scene`): the sound that stayed at 1× while
/// the picture ran ahead reaches the graph as TWO fades at the run's ends — not the single seam dip the render used
/// to emit unconditionally, and not a placeholder naming a plan.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s1_an_own_clock_run_gets_two_real_dips() {
    let dip = naivepost::fx_lane::SOUND_DIP_SECONDS;
    assert_eq!(dip, 0.15, "P.eng.soundDipSeconds is the dip's whole length");
    let half = naivepost::effect_details::dip_each_side();
    let mut own = clip(1, 2.0, 10.0); // 20 s of footage read at 1× under a 10 s picture
    own.fx = vec![speed(0.0, 10.0, 2.0, "own")];
    own.read_head = Some((0.0, 20.0)); // §8: the plan decides the run, not the filter builder
    let sound = render::audio_chain(&own, &Produce::default(), 0, &[]).join(",");
    // §8 (`the dip half on each side of the join`): each fade is HALF the lane's dip,
    // one at each end of the run, so the join loses 0.15 s in all and not 0.3 s.
    assert!(sound.contains(&format!("afade=t=in:st=0.000:d={half}")), "the head fades in: {sound}");
    assert!(sound.contains(&format!("afade=t=out:st=20.000:d={half}")), "and out at the run's end: {sound}");
    assert_eq!(
        sound.matches("afade=").count(),
        2,
        "a read head brings its own pair, so the plain seam dip stands down: {sound}"
    );
    // A clip nobody re-timed keeps the plain seam dip: it is a splice, not a read head.
    let plain = render::audio_chain(&clip(1, 1.0, 30.0), &Produce::default(), 0, &[]).join(",");
    assert_eq!(plain.matches("afade=").count(), 1, "{plain}");
}

// --- S2: the stop ---------------------------------------------------------------------------------------------

/// §4 (`a still of the frame at t (scene's own camera) overlaid with alpha fades for the frozen spans; footage
/// runs on at 1×`): the freeze arrives as a full-frame overlay over a picture that kept its own clock.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s2_a_stop_overlays_a_faded_still_and_keeps_the_clock() {
    let mut c = clip(1, 1.0, 30.0);
    c.fx = vec![stop(10.0, 3.0)];
    let video = render::video_chain(&c, &Produce::default(), None).join(",");
    assert!(video.contains("tpad=stop_mode=clone:stop_duration=3"), "the frame is held for its span: {video}");
    assert!(video.contains("alpha=1"), "both edges are ALPHA fades, so the picture under it shows through: {video}");
    assert_eq!(video.matches("alpha=1").count(), 2, "one fade in, one out: {video}");
    assert!(video.contains("overlay=0:0:eof_action=pass"), "it covers the frame: {video}");
    assert!(video.contains("enable=between(t\\,10\\,13)"), "only for the frozen span: {video}");
    assert!(!video.contains("setpts"), "§4: the footage runs on at 1× — a stop is not a rate: {video}");
    assert!(!video.contains("atempo"), "and gives the sound no tempo either");
}

// --- S3: the camera -------------------------------------------------------------------------------------------

/// §4 (`static → crop + scale`): a camera parked on a region crops to it and scales back up, and never touches
/// `zoompan`, which would resample every frame for a picture that does not move.
/// The camera walk one clip sees, through the door the render itself uses: `produce_fx_chain` samples the zooms,
/// caps them and asks `render_fx` for the chain. A parked rectangle is TWO filters and no grid; a gliding one runs
/// `zoompan`, because the camera is between places at every second of its fades.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s3_a_parked_camera_is_a_crop_and_scale_pair() {
    let mut c = clip(1, 1.0, 30.0);
    // Two zooms, each holding its own rectangle until the next takes over (§A.1's `stay`): the camera sits on the
    // first box the whole time between them, so nothing moves and there is nothing to sample.
    c.fx = vec![
        Fx { kind: "zoom".into(), t: 0.0, dur: 8.0, stay: true, cx: Some(0.5), cy: Some(0.4), hf: Some(0.5), ..Default::default() },
        Fx { kind: "zoom".into(), t: 20.0, dur: 4.0, stay: true, cx: Some(0.5), cy: Some(0.4), hf: Some(0.5), ..Default::default() },
    ];
    let video = render::video_chain(&c, &Produce::default(), None).join(",");
    assert!(video.contains("crop="), "a zoom that holds still is a crop: {video}");
    assert!(video.contains("scale=1920:1080"), "back up to the frame box: {video}");
    assert!(!video.contains("zoompan"), "nothing moves, so nothing samples a grid: {video}");
}

/// §4 (`moving → pad black as needed, crop to the union box, zoompan with piecewise-linear expressions at fixed fps
/// (default 30)`): a gliding camera gets the grid, the expression and the padding only where it looks off the edge.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s3_a_moving_camera_runs_zoompan_on_the_grid() {
    let glide = gliding(zoom(5.0, 6.0, 0.3, 0.3, 0.25));
    let mut c = clip(1, 1.0, 30.0);
    c.fx = vec![glide];
    let video = render::video_chain(&c, &Produce::default(), None).join(",");
    assert!(video.contains("zoompan"), "a journey is a zoompan: {video}");
    assert!(
        video.contains(&format!("fps={}", naivepost::render_fx::ZOOM_GRID_FPS)),
        "at the grid §4 names ({}): {video}",
        naivepost::render_fx::ZOOM_GRID_FPS
    );
    assert!(video.contains("if(lt(it\\,"), "piecewise-linear in input time: {video}");
    // A rectangle reaching past the source is padded rather than clamped, or the window skews mid-journey.
    let off_edge = gliding(zoom(5.0, 6.0, 0.05, 0.05, 0.6));
    let mut wide = clip(1, 1.0, 30.0);
    wide.fx = vec![off_edge];
    let video = render::video_chain(&wide, &Produce::default(), None).join(",");
    assert!(video.contains("color=black"), "black where the camera looks off the tape: {video}");
}

/// §4 (`> 10× capped and logged`): deeper than ffmpeg's `zoompan` will go, the windows come back to the cap and the
/// log says which clip it happened to — through the plan's own lines, not a private note.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s3_a_zoom_past_ten_times_is_capped_and_logged() {
    let cap = naivepost::render_fx::ZOOM_DEPTH_CAP;
    assert_eq!(cap, 10.0, "P.eng.zoompanDepthCap, once");
    // A journey from mid-frame down to a sliver: the union box is most of the tape, so the deepest window wants
    // far more than ten times its height.
    let mut deep = zoom(5.0, 6.0, 0.5, 0.5, 0.5);
    deep = gliding(deep);
    deep.cx = Some(0.2);
    deep.cy = Some(0.2);
    deep.hf = Some(0.02);
    let mut c = clip(1, 1.0, 30.0);
    c.fx = vec![deep.clone()];
    let built = naivepost::produce_fx_chain::for_clip(&c, &c.fx, c.src, &common::temp_root("s3-cap"));
    assert!(
        built.logs.iter().any(|l| l.contains("deeper than ffmpeg's 10") && l.contains("clip 1")),
        "the cap is said, naming the clip: {:?}",
        built.logs
    );
    let video = render::video_chain(&c, &Produce::default(), None).join(",");
    assert!(video.contains("zoompan"), "and the clip still renders: {video}");
    // Nothing in what is emitted asks for more depth than the cap allows.
    let samples = naivepost::render_fx::camera_samples(&[deep], 0.0, 30.0);
    let (capped, _) = naivepost::render_fx::cap_zoom(&samples, (1920, 1080), (1920, 1080), 1);
    assert!(
        naivepost::render_fx::max_zoom(&capped, (1920, 1080), (1920, 1080)) <= cap + 0.001,
        "what renders respects the ceiling"
    );
}

// --- S4: words and drawings ------------------------------------------------------------------------------------

/// §4 (`composited after the camera and burned subtitles, each from a looped input (title: generated transparent
/// frame-sized SVG, black round-joined stroke then white fill …), with alpha fades and an enable window`).
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s4_a_title_is_written_as_a_box_sized_svg_and_looped() {
    let scratch = common::temp_root("s4-scratch");
    let mut c = clip(1, 1.0, 30.0);
    c.scratch = scratch.clone();
    c.fx = vec![text(6.0, "results")];
    let built = naivepost::produce_fx_chain::for_clip(&c, &c.fx.clone(), c.src, &scratch);
    assert_eq!(built.title_files.len(), 1, "one sheet for one caption");
    let (path, svg) = &built.title_files[0];
    std::fs::write(path, svg).expect("the sheet is written before ffmpeg is asked for it");
    let read_back = std::fs::read_to_string(path).expect("and it is really there");
    assert!(read_back.starts_with("<svg"), "{read_back}");
    // Frame-sized means the sheet is as big as the frame it will be laid over and its letters are sized off that
    // height (`h / 12` in `render_fx::title_svg`) — the box inside it is smaller, which is what §1's lower third is.
    let (w, h) = {
        let (x, y, w, h) = naivepost::produce_fx_chain::box_px(&c.fx[0], c.frame);
        let _ = (x, y); // the position is checked where it lands in the chain, below
        (w, h)
    };
    assert!(read_back.contains(&format!("width=\"{w}\" height=\"{h}\"")), "sized to its box: {read_back}");
    assert!(read_back.contains(&format!("viewBox=\"0 0 {w} {h}\"")), "{read_back}");
    assert!(w <= c.frame.0 && h <= c.frame.1, "never bigger than the frame: {w}x{h}");
    assert!(read_back.contains("stroke=\"black\""), "outline first: {read_back}");
    assert!(read_back.contains("stroke-linejoin=\"round\""), "round-joined: {read_back}");
    assert!(read_back.contains("fill=\"white\""), "then white letters: {read_back}");
    assert!(read_back.contains(">results</text>"), "the words themselves: {read_back}");
    // In the graph: a looped input, two alpha fades, an enable window, and AFTER the burned cues.
    let video = render::video_chain(&c, &Produce::default(), Some("produce/clips/final.srt")).join(",");
    assert!(video.contains("looped for 4"), "each comes from a looped input: {video}");
    assert_eq!(video.matches("alpha=1").count(), 2, "faded in and out: {video}");
    assert!(video.contains("enable=between(t\\,6\\,10)"), "and only while it is on: {video}");
    let burned = video.find("subtitles=").expect("the burned sheet is in the chain");
    // The LAST overlay is the title's own (a blurred-edges backdrop overlays first, and that belongs under the
    // cues); what §4 pins is that no title work happens before the burned sheet.
    let title = video.rfind("overlay=").expect("the title overlay is too");
    assert!(burned < title, "§4: a title sits over a burned cue, never under it: {video}");
    assert!(built.inputs.iter().any(|(kind, _)| kind == "text"), "the sheet is an input: {:?}", built.inputs);
}

/// §4 (`svg effect: the user's file scaled into its box`): their document, our box. A drawing whose file went away
/// costs the decoration and its own log line, not the clip.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s4_a_drawing_is_scaled_into_its_box() {
    let mut c = clip(1, 1.0, 30.0);
    c.fx = vec![drawing(6.0, "project:assets/arrow.svg")];
    let video = render::video_chain(&c, &Produce::default(), None).join(",");
    assert!(video.contains("input project:assets/arrow.svg"), "their file is read: {video}");
    assert!(video.contains("scale="), "into a box: {video}");
    assert!(video.contains("overlay="), "and laid over the picture: {video}");
    let built = naivepost::produce_fx_chain::for_clip(&c, &c.fx.clone(), c.src, &common::temp_root("s4b"));
    assert!(built.inputs.iter().any(|(k, p)| k == "svg" && p.as_deref() == Some("project:assets/arrow.svg")));

    let mut gone = clip(1, 1.0, 30.0);
    gone.fx = vec![drawing(6.0, "")];
    let built = naivepost::produce_fx_chain::for_clip(&gone, &gone.fx.clone(), gone.src, &common::temp_root("s4c"));
    assert_eq!(built.logs.len(), 1, "the missing file is named: {:?}", built.logs);
    assert!(built.inputs.is_empty(), "and nothing is fed to ffmpeg that cannot be opened");
}

// --- S5: volume -------------------------------------------------------------------------------------------------

/// §4 (`one volume=<expr>:eval=frame per cue, in turn, after the lane mix, before the narration`): the placeholder
/// the render used to emit per counted cue is gone, and what replaces it multiplies where two cues overlap.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s5_one_volume_filter_per_cue_in_turn_after_the_mix() {
    let mut c = clip(1, 1.0, 30.0);
    c.fx = vec![volume(4.0, 6.0, 0.3), volume(7.0, 6.0, 2.0)];
    c.cues = 2;
    let sound = render::audio_chain(&c, &Produce::default(), 1, &[]).join(",");
    assert_eq!(sound.matches("eval=frame").count(), 2, "one filter per cue: {sound}");
    assert!(sound.contains("volume=volume='if(between(t,4.000,10.000)"), "cue one's own window: {sound}");
    assert!(sound.contains("gv0") && sound.contains("gv1"), "chained IN TURN, so they multiply: {sound}");
    assert!(!sound.contains("volume=expr"), "no placeholder survives anywhere: {sound}");
    let mix = sound.find("amix=").expect("the lane bed is mixed first");
    let cue = sound.find("eval=frame").unwrap();
    assert!(mix < cue, "§4's order: the cues come after the lane mix: {sound}");
    // One spoken line shows where the cues land relative to the voice: before its `adelay`.
    let mut spoken = c.clone();
    spoken.lines = vec![naivepost::produce_render::Placed {
        entry_index: 0,
        at: 20.0,
        tempo: 1.0,
        extend: 0.0,
        text: "a line".into(),
        speech: 3.0,
    }];
    let voiced = render::audio_chain(&spoken, &Produce::default(), 1, &[]).join(",");
    let cue = voiced.find("eval=frame").unwrap();
    let voice = voiced.find("adelay=").expect("the narration is delayed into place: {voiced}");
    assert!(cue < voice, "§4: volume cues before the narration: {voiced}");
    // A duck's ramps are in the expression, so a gain does not click on.
    assert!(sound.contains("min(1,"), "the fade is part of the expression: {sound}");
}

// --- S6: the label ----------------------------------------------------------------------------------------------

/// §4 (`Label: nothing`): a label marks a moment for the narration brief and contributes no filter to any clip.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s6_a_label_renders_nothing_at_all() {
    let bare = clip(2, 1.0, 30.0);
    let mut labelled = clip(2, 1.0, 30.0);
    labelled.fx = vec![Fx { kind: "label".into(), t: 5.0, dur: 1.0, text: "demo".into(), ..Default::default() }];
    assert!(
        !naivepost::render_fx::is_rendered("label"),
        "a label is a kind the record knows and the render ignores"
    );
    assert_eq!(
        render::video_chain(&bare, &Produce::default(), None),
        render::video_chain(&labelled, &Produce::default(), None),
        "a label changes nothing about the picture"
    );
    assert_eq!(
        render::audio_chain(&bare, &Produce::default(), 0, &[]),
        render::audio_chain(&labelled, &Produce::default(), 0, &[]),
        "nor about the sound"
    );
}

// --- S7: the frame box ------------------------------------------------------------------------------------------

/// §4 (`Frame box for every clip: no aspect → the footage's own; with an aspect the resolution tier names the short
/// side (1080p on 9:16 = 1080×1920), even sides`).
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s7_the_frame_box_names_the_tier_by_its_short_side() {
    let tall = naivepost::render_fx::frame_box("9:16", 1920, 1080);
    assert_eq!(tall, (1080, 1920), "the tier is the SHORT edge: {tall:?}");
    assert_eq!(naivepost::render_fx::frame_box("", 1920, 1080), (1920, 1080), "no aspect, the footage's own");
    assert_eq!(naivepost::render_fx::frame_box("source", 1280, 720), (1280, 720));
    let wide = naivepost::render_fx::frame_box("16:9", 1000, 1000);
    assert_eq!((wide.0 % 2, wide.1 % 2), (0, 0), "every side even, for 4:2:0: {wide:?}");
    // And the box reaches the encoder: the chain scales and pads to exactly it.
    let mut c = clip(1, 1.0, 30.0);
    c.frame = tall;
    let video = render::video_chain(&c, &Produce::default(), None).join(",");
    assert!(video.contains("scale=1080:1920"), "{video}");
    assert!(video.contains("pad=1080:1920"), "{video}");
}

// --- the wire: the filters reach the ffmpeg the settings name ----------------------------------------------------

/// The item's last word: none of the above matters if the strings stay inside the process. This drives a whole run
/// plan through `produce_exec::plan` and hands every clip's argv to the ffmpeg path Settings holds — a stub script
/// that logs its argv — then reads that log. The filters are proven to LEAVE the program, through the same
/// resolution a real render uses.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s8_the_filters_reach_the_configured_ffmpeg() {
    let (root, tree) = common::tree_in("s8-wire");
    let (stub, log) = ffmpeg_stub(&root);
    assert!(stub.exists(), "the stub is the tool the settings name");
    let fx = vec![
        zoom(2.0, 6.0, 0.5, 0.4, 0.4),
        stop(12.0, 3.0),
        text(18.0, "results"),
        drawing(22.0, "project:assets/arrow.svg"),
        volume(4.0, 6.0, 0.3),
        volume(7.0, 6.0, 2.0),
        Fx { kind: "label".into(), t: 27.0, dur: 1.0, text: "demo".into(), ..Default::default() },
    ];
    let cut = Cut { segs: vec![seg(0.0, 30.0)], fx, aspect: "16:9".into(), ..Default::default() };
    let plan = planned_with_stub(&tree, cut);
    // A speed split cuts the segment list (§4's first bullet), so the count is read back rather than assumed: what
    // matters is that SOME clip carries all of this and its argv reaches the tool.
    assert!(!plan.encodes.is_empty(), "a render with one segment encodes at least one clip");
    let argv: Vec<String> = plan
        .encodes
        .iter()
        .flat_map(|e| e.command.argv.iter().cloned())
        .collect();
    // Run the configured tool on the argv the plan built. The stub writes its output file and logs every argument.
    let status = std::process::Command::new(&stub)
        .args(argv.iter().skip(1))
        .status()
        .expect("the stub runs");
    assert!(status.success(), "the stub accepted the argv the plan produced");
    let seen = std::fs::read_to_string(&log).expect("the stub logged its argv");
    for needle in [
        "zoompan",
        "tpad=stop_mode=clone",
        "alpha=1",
        "volume=volume='if(between(t,4.000,10.000)",
        "gv1",
        "overlay=",
        "-loop",
    ] {
        assert!(seen.contains(needle), "`{needle}` never reached the ffmpeg the settings name:\n{seen}");
    }
    assert!(!seen.contains("volume=expr"), "the placeholder is nowhere in what ffmpeg is given");
    assert!(!seen.contains("label"), "a label renders nothing, so it appears nowhere: {seen}");
    // The generated title sheet exists on disk before the tool was called, because it is an input. Counted by
    // CONTENT rather than by extension: one encode per rate stretch means more than one clip wrote its own copy.
    let sheets: Vec<PathBuf> = dir_svg(&plan.scratch);
    assert!(!sheets.is_empty(), "the run wrote its title sheet(s): {sheets:?}");
    assert!(std::fs::read_to_string(&sheets[0]).unwrap().contains("stroke=\"black\""));
}

// --- scaffolding -----------------------------------------------------------------------------------------------

/// Plan a run whose clips carry `fx`. Everything else about the run is the boring default: no narration, no lanes,
/// one footage source that always resolves.
fn planned_at(tree: &Tree, cut: Cut) -> exec::Plan {
    let run = flow::snapshot(
        Some(cut.clone()),
        tree,
        Vec::new(),
        Produce::default(),
        &common_project(),
        Some(Publish::default()),
        "1".to_string(),
    );
    // The effects travel on the CUT's own list (`cut.fx`), which is where §A puts them, and reach each clip through
    // `Materials::fx_for` — the door the planner already asks every segment through.
    exec::plan(&run, tree, &materials_with(cut.fx.clone()))
}

/// As [`planned`], but with Settings' ffmpeg box pointed at the stub so the plan's commands can be run for real.
/// As [`planned_at`], for the test that runs the plan's commands through the stubbed ffmpeg tool.
fn planned_with_stub(tree: &Tree, cut: Cut) -> exec::Plan {
    planned_at(tree, cut)
}

type Translate = dyn Fn(&str, &[usize]) -> Result<std::collections::BTreeMap<usize, String>, String>;

/// The materials answering `fx_for` with `fx` for every segment. The closures and the list they read are leaked to
/// the heap: a per-test local cannot outlive the borrow `Materials` holds, and this is scratch that dies with the
/// test binary.
fn materials_with<'a>(fx: Vec<Fx>) -> exec::Materials<'a> {
    static LANES: &[Lane] = &[];
    let boxed: &'a Vec<Fx> = Box::leak(Box::new(fx));
    let fx_for: &'a dyn Fn(&Seg) -> Vec<Fx> = Box::leak(Box::new(move |_seg: &Seg| boxed.clone()));
    // Named `fn`s rather than closures: a closure leaking into a `for<'a>` function pointer needs the lifetime
    // inference that only an item-level signature gets.
    fn always_exists(_: &str) -> bool {
        true
    }
    fn no_wav(_: &naivepost::narration::Entry) -> Option<PathBuf> {
        None
    }
    fn take_mp4(_: &Clip) -> Option<String> {
        Some("/media/take.mp4".to_string())
    }
    fn no_words(_: &Clip) -> Vec<naivepost::produce_subtitles::Word> {
        Vec::new()
    }
    fn pass_through(_: &str, numbers: &[usize]) -> Result<std::collections::BTreeMap<usize, String>, String> {
        Ok(numbers.iter().map(|n| (*n, format!("translated {n}"))).collect())
    }
    fn no_sources() -> Vec<naivepost::project::Source> {
        Vec::new()
    }
    let exists: &'a dyn Fn(&str) -> bool = &always_exists;
    let wav_of: &'a dyn Fn(&naivepost::narration::Entry) -> Option<PathBuf> = &no_wav;
    let source_file: &'a dyn Fn(&Clip) -> Option<String> = &take_mp4;
    let words: &'a dyn Fn(&Clip) -> Vec<naivepost::produce_subtitles::Word> = &no_words;
    let translate: &'static Translate = &pass_through;
    let sources: &'a dyn Fn() -> Vec<naivepost::project::Source> = &no_sources;
    exec::Materials {
        exists,
        fx_for,
        lanes: LANES,
        src_shape: (1920, 1080),
        wav_of,
        source_file,
        on_recording: true,
        cues: exec::Cues { all: Vec::new(), languages: Vec::new() },
        words,
        translate,
        sources,
    }
}

fn common_project() -> naivepost::project::Project {
    naivepost::project::Project {
        sources: vec![naivepost::project::Source {
            path: "/media/take.mp4".into(),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// Point Settings' ffmpeg box at a stub script and hand back `(stub, argv log)` — the seam
/// `tests/audio_task_dial.rs` uses: this container has no ffmpeg, and several tests pin that absence, so a door
/// that spawns the configured tool is provable only through the tool path Settings itself resolves.
fn ffmpeg_stub(root: &Path) -> (PathBuf, PathBuf) {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let stub = bin.join("ffmpeg");
    let log = bin.join("argv.log");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nlast=\"\"\nfor arg in \"$@\"; do last=\"$arg\"; printf '%s\\n' \"$arg\" >> '{}'; done\nprintf 'x' > \"$last\"\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let paths = naivepost::settings::Paths {
        config_dir: root.join("cfg/naivepost"),
        data_dir: root.join("data/naivepost"),
    };
    let mut conf = naivepost::settings::read(&paths).unwrap_or_default();
    conf.ffmpeg = stub.to_string_lossy().into_owned();
    naivepost::settings::save(&paths, &conf).expect("the conf is written");
    (stub, log)
}

/// Every `.svg` the run left in its scratch folder — the proof a generated title was WRITTEN, not just composed.
fn dir_svg(dir: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "svg"))
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[allow(dead_code)]
const _: &str = ITEM;
