//! §06-effects#4-render-how-each-effect-becomes-ffmpeg — checked against [`naivepost::render_fx`].
//!
//! The spec section is a bullet list, not numbered steps: its six bullets are read here as S1..S6 and its closing
//! frame-box rule as S7. Nothing in these tests runs ffmpeg or opens a window — the module returns filter text and
//! plain numbers, which is the whole reason a render rule can be pinned without rendering. Every time value below is
//! an exact binary number, so a comparison that must be exact stays exact; only the two log lines and the frame boxes
//! are compared whole, everything else asks about the fragment of a filter string that carries the meaning.

use naivepost::cut::{Fx, Seg};
use naivepost::cut_hear;
use naivepost::fx_lane;
use naivepost::fx_zoom;
use naivepost::params;
use naivepost::render_fx as render;

/// A footage scene from `s` to `e`.
fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

/// A spliced card: no footage under it, its own length.
fn card(dur: f64) -> Seg {
    Seg {
        s: 0.0,
        e: 0.0,
        ins: "project:assets/tier.svg".into(),
        dur,
        ..Default::default()
    }
}

fn fx(kind: &str, t: f64, dur: f64) -> Fx {
    Fx {
        kind: kind.into(),
        t,
        dur,
        ..Default::default()
    }
}

/// A speed effect over `[t, t+dur)` at `rate`.
fn speed(t: f64, dur: f64, rate: f64) -> Fx {
    Fx {
        rate,
        ..fx("speed", t, dur)
    }
}

/// A zoom on a rect, for the camera cases. `hf` of the source height, centred unless said otherwise.
fn zoom(t: f64, dur: f64, hf: f64, cx: f64, cy: f64) -> Fx {
    Fx {
        hf: Some(hf),
        cx: Some(cx),
        cy: Some(cy),
        stay: true,
        ..fx("zoom", t, dur)
    }
}

const HD: (i32, i32) = (1920, 1080);

// --- S1: speed -----------------------------------------------------------------------------------------------------

/// S1 (`clips split at rate boundaries, the middle carries the rate (setpts=PTS/rate)`): one scene, three rates, and
/// only the stretch that was sped gets a filter. `Inserts and spliced cards keep their own clock` is the other half:
/// a card's length IS its timing, so re-timing it would move every other card in the video.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s1_speed_splits_the_clip_and_carries_the_rate() {
    let fx = [speed(2.0, 2.0, 2.0)];
    let pictures = render::split_pictures(&fx, &seg(0.0, 6.0));
    assert_eq!(pictures.len(), 3);
    assert_eq!((pictures[0].from, pictures[0].to), (0.0, 2.0));
    assert_eq!((pictures[1].from, pictures[1].to), (2.0, 4.0));
    assert_eq!((pictures[2].from, pictures[2].to), (4.0, 6.0));
    assert_eq!(pictures[0].setpts, None);
    assert_eq!(pictures[2].setpts, None);
    assert_eq!(pictures[1].setpts.as_deref(), Some("setpts=PTS/2"));

    // A card comes back as one picture and carries no clock of its own.
    let card = render::split_pictures(&fx, &card(5.0));
    assert_eq!(card.len(), 1);
    assert_eq!(card[0].setpts, None);
    assert_eq!(card[0].rate, 1.0);

    // An overwriting insert runs for the footage it replaces and is likewise left alone.
    let over = Seg {
        s: 0.0,
        e: 4.0,
        ins: "project:assets/badge.svg".into(),
        ..Default::default()
    };
    let overwritten = render::split_pictures(&fx, &over);
    assert_eq!(overwritten.len(), 1);
    assert_eq!(overwritten[0].setpts, None);

    // The four sound answers §4 lists, chosen by the stored answer.
    assert_eq!(render::sound_filters(2.0, "").join(","), "atempo=2");
    assert_eq!(render::sound_filters(4.0, "").join(","), "atempo=2,atempo=2");
    assert_eq!(render::sound_filters(0.25, "").join(","), "atempo=0.5,atempo=0.5");
    assert!(render::sound_filters(1.0, "").is_empty(), "1× needs no filter");
    assert_eq!(
        render::sound_filters(2.0, "pitch").join(","),
        "asetrate=48000*2,aresample=48000"
    );
    let mute = render::sound_filters(2.0, "mute").join(",");
    assert!(mute.contains("volume=0:enable="), "{mute}");

    // own / scene: the sound stays at 1× and the read head's dips mark where it splices back in.
    let dip = fx_lane::SOUND_DIP_SECONDS;
    assert_eq!(dip, 0.15); // P.eng.soundDipSeconds
    assert!(params::find("P.eng.soundDipSeconds").is_some());
    for snd in ["own", "scene"] {
        let head = render::sound_filters(2.0, snd).join(",");
        assert!(head.contains("1x"), "{snd}: {head}");
        // The windows the head opens and closes on, taken from the same helper a caller would plan them with.
        let (in_at, out_at) = render::read_head((0.0, 10.0));
        assert_eq!((in_at, out_at), (dip, 10.0 - dip));
        assert!(head.contains(&format!("{}", in_at)), "{snd}: {head}");
    }
    // The dips sit at each end of the run, `dip` in from either side.
    assert_eq!(render::read_head((0.0, 10.0)), (dip, 10.0 - dip));
    // A run shorter than two dips shares what it has rather than going silent whole.
    let short = render::read_head((0.0, 0.1));
    assert!(short.0 <= short.1);
}

// --- S2: the stop --------------------------------------------------------------------------------------------------

/// S2 (`a still of the frame at t … overlaid with alpha fades for the frozen spans; footage runs on at 1×`): a stop is
/// not a rate, so it never reaches S1's split — and the chain it does produce carries both fades and the window.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s2_a_stop_freezes_a_frame_and_the_footage_runs_on() {
    let stop = Fx {
        trans: 0.5,
        tout: 0.5,
        ..fx("stop", 4.0, 2.0)
    };
    assert!(naivepost::fx_record::is_stop(&stop));

    let chain = render::still_chain(stop.t, (4.0, 6.0), (stop.trans, stop.tout), HD).join(";");
    assert!(chain.contains("alpha=1"), "{chain}");
    assert_eq!(chain.matches("alpha=1").count(), 2, "one fade each end: {chain}");
    assert!(chain.contains("enable=between(t"), "{chain}");

    // The footage under it runs at its own speed: a stop carries no rate to split on.
    let fx = [stop];
    assert!(render::footage_stays_at_one(&fx));
    assert!(render::split_pictures(&fx, &seg(0.0, 10.0))
        .iter()
        .all(|p| p.setpts.is_none()));

    // A sped stretch is the opposite answer, so the rule above is not just always-true.
    assert!(!render::footage_stays_at_one(&[speed(1.0, 2.0, 2.0)]));
}

// --- S3: the camera ------------------------------------------------------------------------------------------------

/// S3 (`sampled at clip ends and every zoom's breakpoints, straight lines between`): an unsampled breakpoint is an
/// unsampled move — a hold that is not sampled renders as the camera drifting on instead of standing still.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s3_the_camera_is_sampled_at_ends_and_breakpoints() {
    let fx = [zoom(2.0, 4.0, 0.5, 0.5, 0.5)];
    let samples = render::camera_samples(&fx, 0.0, 10.0);
    let at: Vec<f64> = samples.iter().map(|s| s.t).collect();
    // The scene's own ends are always in, whatever the zoom says.
    assert!(at.contains(&0.0) && at.contains(&10.0), "{at:?}");
    // …and so is the zoom's arrival.
    assert!(at.contains(&2.0), "{at:?}");
    for pair in at.windows(2) {
        assert!(pair[0] < pair[1], "sorted and deduped: {at:?}");
    }

    // What it reads is the preview's camera, so the two cannot drift apart: outside every zoom the frame is settled.
    let outside = samples.iter().find(|s| s.t == 0.0).expect("the scene's first second");
    assert_eq!(outside.cx, fx_zoom::SETTLED.cx);
    assert_eq!(outside.hf, fx_zoom::SETTLED.hf);

    // A zoom that arrives at the frame it was already on never moves the camera: static. (The scene is not static by
    // default — a staying zoom parks the camera on its rect, which is exactly what S1's walk says it does.)
    let held = [zoom(2.0, 4.0, 1.0, 0.5, 0.5)];
    let held_samples = render::camera_samples(&held, 0.0, 10.0);
    assert!(render::is_static(&held_samples), "{held_samples:?}");
    let glide = [Fx {
        stay: false,
        trans: 1.0,
        tout: 1.0,
        ..zoom(2.0, 4.0, 0.5, 0.5, 0.5)
    }];
    assert!(!render::is_static(&render::camera_samples(&glide, 0.0, 10.0)));

    // Static: two filters and no zoompan.
    let still = render::camera_chain(&held_samples, HD, HD).join(";");
    assert!(still.contains("crop=") && still.contains("scale="), "{still}");
    assert!(!still.contains("zoompan"), "a camera with nowhere to go: {still}");

    // Moving: zoompan on the grid §4 names, and black only where a rectangle reaches past the source.
    let moving = render::camera_samples(&glide, 0.0, 10.0);
    let chain = render::camera_chain(&moving, HD, HD).join(";");
    assert!(chain.contains("zoompan"), "{chain}");
    assert!(chain.contains(&format!("fps={}", render::ZOOM_GRID_FPS)), "{chain}");
    assert_eq!(render::ZOOM_GRID_FPS, 30.0);
    assert!(!chain.contains("pad="), "a centred box never leaves the frame: {chain}");
    let deep = [zoom(2.0, 4.0, 0.25, 0.1, 0.1)];
    let off_edge = render::camera_chain(&render::camera_samples(&deep, 0.0, 10.0), HD, HD).join(";");
    assert!(off_edge.contains("pad="), "the box reaches past the edge: {off_edge}");

    // The piecewise expression: a ramp between two samples, flat past either end.
    let expr = render::piece_expr(&[0.0, 4.0], &[1.0, 3.0]);
    // Commas inside a filter's own expression are escaped for the graph, so that is what the text carries.
    assert!(expr.contains("if(lt(it\\,"), "{expr}");
    assert!(expr.contains("clip("), "{expr}");
    assert!(expr.ends_with("\\,3.0000)"), "flat past the end: {expr}");

    // Deeper than ffmpeg's cap: pulled back, and said so.
    let deeper = [zoom(2.0, 4.0, 0.05, 0.5, 0.5)];
    let (capped, log) = render::cap_zoom(&render::camera_samples(&deeper, 0.0, 10.0), HD, HD, 1);
    assert!(log.is_some());
    assert_eq!(
        log.as_deref(),
        Some("clip 1: the zoom goes deeper than ffmpeg's 10× \u{2014} it is rendered at 10×")
    );
    assert!(!log.unwrap().contains("--"), "an em dash, not two hyphens");
    assert!(render::max_zoom(&capped, HD, HD) <= render::ZOOM_DEPTH_CAP + 0.001);

    let shallow = [zoom(2.0, 4.0, 0.5, 0.5, 0.5)];
    let (_, log) = render::cap_zoom(&render::camera_samples(&shallow, 0.0, 10.0), HD, HD, 1);
    assert_eq!(log, None);
    assert_eq!(render::ZOOM_DEPTH_CAP, 10.0);
}

// --- S4: words and drawings ----------------------------------------------------------------------------------------

/// S4 (`composited after the camera and burned subtitles … with alpha fades and an enable window`): a title is put on
/// the finished frame, so it holds still while the camera moves under it.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s4_words_and_drawings_go_after_the_camera() {
    let order = render::picture_order();
    let camera = order.iter().position(|p| *p == "camera").expect("camera");
    let subs = order
        .iter()
        .position(|p| p.contains("subtitles"))
        .expect("burned subtitles");
    let words = order.iter().position(|p| p.contains("text")).expect("text and svg");
    assert!(camera < subs && subs < words, "{order:?}");

    let chain = render::overlay_chain("text", (2.0, 5.0), (0.5, 0.5), (100, 200, 800, 300)).join(";");
    assert!(chain.contains("looped"), "{chain}");
    assert!(chain.contains("enable=between(t"), "{chain}");
    assert_eq!(chain.matches("alpha=1").count(), 2, "both fades: {chain}");

    // The generated title: frame-sized, transparent, and painted outline-then-fill. SVG draws in document order, so
    // the stroke has to be written first or the letters lose their edge on a bright picture.
    let svg = render::title_svg("results", 1920, 1080);
    assert!(svg.contains("width=\"1920\"") && svg.contains("height=\"1080\""), "{svg}");
    assert!(!svg.contains("<rect"), "no background: {svg}");
    let stroke = svg.find("stroke-linejoin").expect("round joins");
    let black = svg.find("stroke=\"black\"").expect("the outline");
    let fill = svg.find("fill=\"white\"").expect("the letters");
    assert!(stroke < black && black < fill, "outline under letters: {svg}");

    // The user's own drawing goes in at its box and nothing else is done to it.
    let drawn = render::scaled_into_box("project:assets/arrow.svg", (0, 0, 640, 480)).join(";");
    assert!(drawn.contains("project:assets/arrow.svg"), "{drawn}");
    assert!(drawn.contains("scale=640:480"), "{drawn}");
}

// --- S5: volume ----------------------------------------------------------------------------------------------------

/// S5 (`one volume=<expr>:eval=frame per cue, in turn`): in turn means two overlapping cues multiply, which is what
/// the preview's mix already says they do.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s5_one_volume_filter_per_cue_in_turn() {
    let duck = Fx {
        gain: 0.5,
        trans: 0.5,
        tout: 0.5,
        ..fx("volume", 2.0, 4.0)
    };
    let mute = Fx {
        gain: 0.0,
        ..fx("volume", 8.0, 2.0)
    };
    let loud = Fx {
        gain: 40.0,
        ..fx("volume", 16.0, 2.0)
    };
    let cues = render::gain_cues(&[duck.clone(), mute, loud], 0.0, 1.0, 30.0);
    assert_eq!(cues.len(), 3);
    assert_eq!(cues[1].gain, 0.0, "a mute is kept, not skipped");
    assert_eq!(cues[2].gain, cut_hear::MAX_GAIN); // and a shout is held to what the pipeline accepts
    assert_eq!(cut_hear::MAX_GAIN, 10.0);

    let chain = render::gain_chain(&cues, "snd");
    assert_eq!(chain.matches("volume=volume='").count(), 3);
    assert_eq!(chain.matches(":eval=frame").count(), 3);
    // In turn: the second stage reads the first's label.
    assert!(chain.contains("[gv0]volume=volume='"), "{chain}");
    for cue in &cues {
        assert!(render::gain_expr(cue).starts_with("if(between(t,"));
    }

    // Under a speed effect a cue stretches with the sound it rides.
    let stretched = render::gain_cues(&[duck.clone()], 0.0, 2.0, 30.0);
    assert_eq!(stretched[0].to - stretched[0].from, 2.0);

    assert_eq!(
        render::audio_order(),
        vec!["lane mix", "volume cues", "narration"]
    );
}

// --- S6: the label -------------------------------------------------------------------------------------------------

/// S6 (`Label: nothing`): the mark survives because F3.7 exists to make it survive; the render's answer is no filter
/// text at all, so a label cannot change a picture by accident.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s6_a_label_renders_nothing() {
    assert!(!render::is_rendered("label"));
    for kind in ["zoom", "speed", "stop", "text", "svg", "volume"] {
        assert!(render::is_rendered(kind), "{kind} renders");
    }
    // An unknown kind in an old file is not something this build knows how to draw either.
    assert!(!render::is_rendered("marker"));

    let only = [Fx {
        text: "results".into(),
        ..fx("label", 2.0, 4.0)
    }];
    assert!(render::clip_filters(&only, &seg(0.0, 10.0), HD, HD).is_empty());

    // A kind that does render is not empty, so the assertion above is about labels and not about the assembler.
    let glide = Fx {
        stay: false,
        trans: 1.0,
        tout: 1.0,
        hf: Some(0.5),
        cx: Some(0.5),
        cy: Some(0.5),
        ..fx("zoom", 2.0, 4.0)
    };
    assert!(!render::clip_filters(&[glide], &seg(0.0, 10.0), HD, HD).is_empty());
}

// --- S7: the frame box ---------------------------------------------------------------------------------------------

/// S7 (`no aspect → the footage's own; with an aspect the resolution tier names the short side (1080p on 9:16 =
/// 1080×1920), even sides`): naming the tier by an edge is what lets a project change shape and keep its sharpness.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s7_the_frame_box_is_the_tier_on_the_short_side() {
    assert_eq!(render::frame_box("", 1920, 1080), (1920, 1080));
    assert_eq!(render::frame_box("source", 1280, 720), (1280, 720));

    assert_eq!(render::frame_box("9:16", 1920, 1080), (1080, 1920));
    assert_eq!(render::frame_box("16:9", 1920, 1080), (1920, 1080));
    assert_eq!(render::frame_box("1:1", 1920, 1080), (1080, 1080));
    assert_eq!(render::frame_box("4:5", 1920, 1080), (1080, 1350));

    // Even sides whatever the ratio works out to — every encoder this project offers is 4:2:0.
    for aspect in ["9:16", "16:9", "1:1", "4:5", "3:4"] {
        let (w, h) = render::frame_box(aspect, 1920, 1080);
        assert_eq!((w % 2, h % 2), (0, 0), "{aspect} came out {w}x{h}");
    }
    assert_eq!(render::frame_box("3:4", 1920, 1080), (1080, 1440));

    // A shape nobody spelled right behaves like no shape: the output's aspect is not worth stopping a video over.
    assert_eq!(render::frame_box("wide", 1280, 720), (1280, 720));
    assert_eq!(render::frame_box("9:", 1280, 720), (1280, 720));
    assert_eq!(render::TIER_SHORT_SIDE, 1080);
}
