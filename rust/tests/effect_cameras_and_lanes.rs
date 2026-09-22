//! §06-effects#9-where-recordings-overlap — `spec/06-effects.md` §9, one test per row of its table plus one per
//! MUST-agree bullet.
//!
//! Everything here is plain data: no widget is built and no display is needed, because `rust/src/ui/window.rs` still
//! renders only Prepare. Where §9 names a number or a sentence that another module already owns — the dip, the zoom's
//! height, the audio order, whose frame `source` means — the assertion reads the owner rather than repeating a literal,
//! so this file fails when an owner drifts and not when a literal was retyped.

use naivepost::cut::{Fx, Seg};
use naivepost::cut_cam;
use naivepost::cut_effects_pass::{self as pass, Call};
use naivepost::cut_hear;
use naivepost::effect_details;
use naivepost::effect_overlap as overlap;
use naivepost::fx_lane;
use naivepost::fx_record::{self, Field};
use naivepost::render_fx;
use naivepost::tools::clips::Clips;

// --- helpers ---------------------------------------------------------------------------------------------------------

/// A footage scene of row `row` from `s` to `e`.
fn scene(row: i32, s: f64, e: f64) -> Seg {
    Seg { s, e, cam: row, ..Default::default() }
}

/// A spliced insert — its own picture, and no footage for an effect to sit on.
fn card() -> Seg {
    Seg { s: 0.0, e: 0.0, ins: "project:assets/tier.svg".into(), dur: 5.0, ..Default::default() }
}

fn kind(kind: &str, t: f64, dur: f64) -> Fx {
    Fx { kind: kind.into(), t, dur, ..Default::default() }
}

fn zoom(t: f64, dur: f64, cam: i32) -> Fx {
    Fx { cam, ..kind("zoom", t, dur) }
}

fn volume(t: f64, dur: f64, lane: &str, gain: f64) -> Fx {
    Fx { lane: lane.into(), gain, ..kind("volume", t, dur) }
}

fn speed(t: f64, dur: f64, rate: f64, snd: &str) -> Fx {
    Fx { rate, snd: snd.into(), ..kind("speed", t, dur) }
}

// --- row 1: zoom ----------------------------------------------------------------------------------------------------

/// §06-effects#9-where-recordings-overlap — `A zoom SHOULD carry cam, the row it was framed on, and act only on clips
/// shown from that camera … Over inserts: none`, against `Text, SVG … no camera, no lane`.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s1_zoom_acts_only_on_its_own_camera() {
    let framed = zoom(2.0, 4.0, 1);
    assert!(overlap::acts_on_scene(&framed, &scene(1, 0.0, 10.0)), "the row it was framed on");
    assert!(!overlap::acts_on_scene(&framed, &scene(0, 0.0, 10.0)), "any other row is somebody else's frame");

    // Over inserts: none — for either camera, since a card is not filmed from one at all.
    let insert = card();
    assert!(!overlap::acts_on_scene(&framed, &insert));
    assert!(!overlap::acts_on_scene(&zoom(2.0, 4.0, 0), &insert));

    // The kinds §9 gives no camera and no lane are drawn on every scene, a card's included.
    for other in ["text", "svg", "label", "volume", "speed"] {
        let fx = kind(other, 2.0, 4.0);
        assert!(overlap::acts_on_scene(&fx, &insert), "{other} over an insert is all or nothing");
        assert!(overlap::acts_on_scene(&fx, &scene(0, 0.0, 10.0)), "{other} on row 0");
        assert!(overlap::acts_on_scene(&fx, &scene(1, 0.0, 10.0)), "{other} on row 1");
    }

    // The record-level half of the same rule: only a zoom reads `cam`, so only a zoom can be framed on a row.
    for other in ["text", "svg", "label"] {
        let parsed = kind(other, 0.0, 1.0).effect_kind().expect("a kind §1 lists");
        assert!(!fx_record::uses(parsed, Field::Cam), "{other} has no cam field");
        assert!(!fx_record::uses(parsed, Field::Lane), "{other} has no lane field");
    }

    // The row a zoom carries is the row it acts on, and the test for "shown from" is the row itself.
    assert_eq!(fx_record::camera_row(&zoom(0.0, 2.0, 1)), Some(1));
    assert!(cut_cam::shown_from(1, 1));
    assert!(!cut_cam::shown_from(1, 0));
}

// --- row 1, the lens-switch half ------------------------------------------------------------------------------------

/// §06-effects#9-where-recordings-overlap — `A lens switch under a zoom says so: "the zoom at m:ss was framed on
/// camera N — it no longer applies here". A zoom does not cross into a clip from another camera.`
#[test]
fn sec_06_effects_9_where_recordings_overlap_s2_a_lens_switch_under_a_zoom_is_said_and_the_zoom_stops() {
    // The second is read off the same clock every other sentence about an effect quotes, so the minute spelling is
    // not this file's to guess.
    assert_eq!(cut_hear::scene_clock(65.0), "1:05");
    let said = overlap::lens_switch(65.0, 2);
    assert_eq!(said, format!("the zoom at {} was framed on camera 2 \u{2014} it no longer applies here", cut_hear::scene_clock(65.0)));
    // §9 writes that sentence with an em dash, and the log line is compared by people reading a transcript.
    assert!(said.contains('\u{2014}'), "{said}");

    // And the zoom stops where the camera changes rather than jumping across it.
    let framed = zoom(0.0, 5.0, 1);
    assert!(overlap::zoom_stops_at_the_cut(&framed, 1, 0), "a cut to another row ends the band");
    assert!(!overlap::zoom_stops_at_the_cut(&framed, 1, 1), "the same row is not a lens switch");
    // Nothing else is stopped: text is composed after the camera, so it has no camera to lose.
    assert!(!overlap::zoom_stops_at_the_cut(&kind("text", 0.0, 5.0), 1, 0));
}

// --- row 2: stop (rate 0) -------------------------------------------------------------------------------------------

/// §06-effects#9-where-recordings-overlap — `A stop at a dropped second SHOULD be refused when placed`.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s3_a_stop_at_a_dropped_second_is_refused_where_it_is_placed() {
    // Inside footage, on the first row: held.
    assert_eq!(overlap::place_stop(5.0, &[scene(0, 0.0, 10.0)]), Ok(()));

    // Over a card only: a card brings its own picture, and there is no frame of the recording to freeze on it.
    let refused = overlap::place_stop(2.0, &[card()]);
    assert!(refused.is_err());
    let reason = refused.unwrap_err();
    assert!(reason.contains(&cut_hear::scene_clock(2.0)), "{reason}");
    assert!(reason.contains("nothing can be held on it"), "{reason}");

    // In the hole between two scenes: nothing is playing at 15 s, so nothing can stand still there either.
    let dropped = overlap::place_stop(15.0, &[scene(0, 0.0, 10.0), scene(0, 20.0, 30.0)]);
    assert!(dropped.is_err());
    assert!(dropped.unwrap_err().contains(&cut_hear::scene_clock(15.0)));

    // The same second on the SECOND camera is fine: §9 resolves a stop's frame from the lens of the scene holding t,
    // so which row it comes from is not the question — the cut keeping the second is.
    assert_eq!(overlap::place_stop(5.0, &[scene(1, 0.0, 10.0)]), Ok(()));

    // A stop is a speed whose rate is 0 (§06#1's table has no stop kind), which is why this row speaks of a rate.
    let stopped = speed(5.0, 2.0, 0.0, "");
    assert!(fx_record::is_stop(&stopped));
}

// --- row 3: speed, sound "" or pitch --------------------------------------------------------------------------------

/// §06-effects#9-where-recordings-overlap — `Keep. The preview MUST play "pitch" as the render does, or say it cannot.`
#[test]
fn sec_06_effects_9_where_recordings_overlap_s4_pitch_is_previewed_as_the_render_plays_it() {
    assert!(overlap::preview_plays_pitch_as_render(""), "the default answer is sound sped with the picture");
    assert!(overlap::preview_plays_pitch_as_render("pitch"));

    // `mute` is not a pitch question, and §9 settles it the other way round: the render joins the preview.
    assert!(!overlap::preview_plays_pitch_as_render("mute"));
    // One expression over the bed — not one hush per camera, which is what silences every lane at once.
    assert_eq!(render_fx::mute_expr(&[(2.0, 4.0)]), "volume=0:enable='between(t\\,2\\,4)'");
    assert!(fx_lane::mute_silences_the_preview("mute"));

    // And this row needs no notice at all: the preview owes it an honest copy, not a warning.
    for snd in ["", "pitch", "mute"] {
        assert_eq!(overlap::says_render_only(snd), None, "{snd}");
    }
}

// --- row 4: speed, sound own or scene -------------------------------------------------------------------------------

/// §06-effects#9-where-recordings-overlap — `Keep. The preview plays sound at picture speed here; it SHOULD say
/// "sound at 1× in the render only".`
#[test]
fn sec_06_effects_9_where_recordings_overlap_s5_own_and_scene_say_the_render_only() {
    for snd in ["own", "scene"] {
        assert_eq!(overlap::says_render_only(snd), Some("sound at 1\u{d7} in the render only"), "{snd}");
    }

    // The sentence and the preview's limit are the same split, read from the owner rather than hard-coded here.
    for snd in ["", "pitch", "mute"] {
        assert!(fx_lane::previewable_sound(snd), "{snd} is playable");
    }
    for snd in ["own", "scene"] {
        assert!(!fx_lane::previewable_sound(snd), "{snd} needs two clocks");
    }

    // The dip where that sound rejoins the picture is one owner's number, whole; §9's `0.15 s dip` and the half this
    // side of the splice come from the same constant.
    assert_eq!(fx_lane::SOUND_DIP_SECONDS, 0.15); // P.eng.soundDipSeconds
    assert_eq!(effect_details::dip_each_side(), fx_lane::SOUND_DIP_SECONDS / 2.0);
}

// --- row 5: speed mute, and stop ------------------------------------------------------------------------------------

/// §06-effects#9-where-recordings-overlap — `MUST: silence the whole bed, as the preview does` (the render ran its
/// hush before the lanes were mixed in, so separate recordings and extra tracks stayed audible).
#[test]
fn sec_06_effects_9_where_recordings_overlap_s6_a_mute_or_a_stop_silences_the_whole_bed() {
    // Two spans, one enable expression: a bed is silenced, so joining them inside it is what reaches every lane at
    // once instead of hushing one camera's sound and leaving the rest running.
    let expr = render_fx::mute_expr(&[(2.0, 4.0), (7.0, 9.0)]);
    assert_eq!(expr, "volume=0:enable='between(t\\,2\\,4)+between(t\\,7\\,9)'");
    let single = render_fx::mute_expr(&[(2.0, 4.0)]);
    assert_eq!(expr, single.replace("between(t\\,2\\,4)", "between(t\\,2\\,4)+between(t\\,7\\,9)"));

    // Preview and render agree on what `mute` does to the sound the person hears.
    assert!(fx_lane::mute_silences_the_preview("mute"));
    assert!(!fx_lane::mute_silences_the_preview(""));

    // The prototype half §9 says the render must stop doing: a hush is a per-lane list, and it is not the whole bed.
    let hushed = Seg { s: 0.0, e: 10.0, quiet: vec!["mic".into()], ..Default::default() };
    assert_eq!(cut_hear::hush(&hushed), &["mic".to_string()], "one lane named, not the bed");
    // An insert's `quiet` answers nothing, so it silences no lane at all.
    assert!(cut_hear::hush(&card()).is_empty());
}

// --- row 6: volume ---------------------------------------------------------------------------------------------------

/// §06-effects#9-where-recordings-overlap — `A volume SHOULD carry lane: "" the whole bed (today), or one lane's name,
/// gained before the mix`.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s7_a_volume_reaches_what_its_lane_names() {
    let bed = volume(0.0, 2.0, "", 2.0);
    assert!(overlap::gain_reaches(&bed, "mic"), "the whole bed is every lane");
    assert!(overlap::gain_reaches(&bed, "room"));
    assert!(fx_record::rides_whole_bed(&bed));
    // The bed's cue rides the graph §4 gives it — after the lanes are summed.
    assert!(!overlap::gained_before_the_mix(&bed));

    let named = volume(0.0, 2.0, "mic", 2.0);
    assert!(overlap::gain_reaches(&named, "mic"));
    assert!(!overlap::gain_reaches(&named, "room"), "one lane's name reaches that lane alone");
    assert!(!fx_record::rides_whole_bed(&named));
    assert!(overlap::gained_before_the_mix(&named), "a named lane is gained before the mix");

    // Where a whole-bed cue sits, from the render's own order.
    assert_eq!(render_fx::audio_order(), vec!["lane mix", "volume cues", "narration"]);

    // Only a volume is heard, so only a volume has anything to apply over a card.
    assert!(overlap::applies_over_a_card(&bed));
    for other in ["zoom", "text", "label"] {
        assert!(!overlap::applies_over_a_card(&kind(other, 0.0, 2.0)), "{other}");
    }
}

// --- row 7: text / svg, and row 8: label -----------------------------------------------------------------------------

/// §06-effects#9-where-recordings-overlap — `Label … read only by the model briefs ("MARKED"); nothing rendered`, and
/// `Text, SVG … after the camera: no camera, no lane. Over an insert or freeze: all or nothing, by the clip's start
/// second`.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s8_text_svg_and_the_label_have_neither_camera_nor_lane() {
    for name in ["text", "svg", "label"] {
        let fx = kind(name, 2.0, 4.0);
        let parsed = fx.effect_kind().expect("a kind §1 lists");
        assert!(!fx_record::uses(parsed, Field::Cam), "{name} reads no cam");
        assert!(!fx_record::uses(parsed, Field::Lane), "{name} reads no lane");

        // All or nothing: the whole thing is drawn over the clip or not at all, so there is no partial case here and
        // nothing to split — `render_fx::split_pictures` splits on rates alone, and none of these carry one.
        assert!(overlap::acts_on_scene(&fx, &card()));
        let over_card = render_fx::split_pictures(&[fx.clone()], &card());
        assert_eq!(over_card.len(), 1, "{name} leaves the card one picture");
        assert_eq!(over_card[0].rate, 1.0);
    }

    // What §4 renders: a caption and a drawing, and never a label — it is read by the model briefs only.
    assert!(!render_fx::is_rendered("label"));
    assert!(render_fx::is_rendered("text"));
    assert!(render_fx::is_rendered("svg"));
}

// --- the two MUST-agree bullets ---------------------------------------------------------------------------------------

/// §06-effects#9-where-recordings-overlap — `While an effect form is open, the preview MUST show the lens camera`, and
/// `With cameras of different shapes, both MUST use the first footage clip's frame`.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s9_preview_and_render_agree_on_the_camera_and_on_the_frame() {
    // A form open shows the lens, not the watched row: a zoom framed on the lens cannot be judged on another camera.
    assert_eq!(overlap::preview_camera(true, Some(2), 0), 0);
    assert_eq!(overlap::preview_camera(false, Some(2), 0), 2, "with no form open the click stands");
    assert_eq!(overlap::preview_camera(false, None, 0), 0, "and nothing watched yet is the lens");

    // The output frame under aspect "source" is the first footage clip's shape, and the camera currently playing
    // never reaches the box at all.
    let (first, playing) = ((1920, 1080), (1080, 1920));
    assert_eq!(overlap::source_frame(first, playing), (1920, 1080));
    assert_eq!(render_fx::frame_box("source", first.0, first.1), overlap::source_frame(first, playing));
    assert_ne!(render_fx::frame_box("source", playing.0, playing.1), overlap::source_frame(first, playing));
}

// --- the model passes -------------------------------------------------------------------------------------------------

/// §06-effects#9-where-recordings-overlap — `The model passes know nothing of cameras and lanes: suggested scenes all
/// take row 0, suggested zooms are always centred at height 0.6 … so a proposed volume hits the whole bed. With more
/// than one camera row or lane the cut brief names the rows and lanes`.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s10_the_model_passes_know_no_cameras_or_lanes() {
    assert!(!overlap::brief_names_rows_and_lanes(1, 1), "one of each is nothing to name");
    assert!(overlap::brief_names_rows_and_lanes(2, 1));
    assert!(overlap::brief_names_rows_and_lanes(1, 2));

    assert_eq!(overlap::SUGGESTED_ROW, 0);
    let (cx, cy, height) = overlap::suggested_zoom_is_centred();
    assert_eq!((cx, cy), (0.5, 0.5));
    assert_eq!(height, pass::ZOOM_HEIGHT, "the height is the placement pass's own number");
    assert_eq!(pass::ZOOM_HEIGHT, 0.6); // effects.proposedZoomHeight

    // A suggestion placed the way F3.11 places one: no row named, so it lands on the first; and its volume carries no
    // lane, which is what makes it a gain on the whole bed.
    let clips = Clips::new(&[(1, 0.0, 10.0), (2, 10.0, 10.0)]);
    let suggested_zoom = pass::place(
        &clips,
        &[Call { clip: 1, kind: "zoom".into(), start: 1.0, end: 4.0, gain: None }],
    );
    let zoomed = &suggested_zoom.effects[0];
    assert_eq!(zoomed.cam, overlap::SUGGESTED_ROW);
    assert_eq!((zoomed.cx, zoomed.cy, zoomed.hf), (Some(0.5), Some(0.5), Some(pass::ZOOM_HEIGHT)));

    let suggested_volume = pass::place(
        &clips,
        &[Call { clip: 1, kind: "volume".into(), start: 1.0, end: 4.0, gain: Some(0.5) }],
    );
    let gained = &suggested_volume.effects[0];
    assert!(gained.lane.is_empty(), "a proposed volume names no lane");
    assert_eq!(gained.cam, overlap::SUGGESTED_ROW);
    assert!(overlap::suggested_volume_reaches_the_bed(gained));
}
