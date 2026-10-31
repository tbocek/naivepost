//! §06-effects#9-where-recordings-overlap: two cameras filming the same seconds and several sounds at once, and what each effect does on shared ground.
//! Most of §9 is kept elsewhere: `mute`/stop silencing the whole bed ([`crate::render_fx::mute_expr`]); the 0.15 s dip ([`crate::fx_lane::SOUND_DIP_SECONDS`], [`crate::effect_details::dip_each_side`]); a volume's default reach ([`crate::fx_record::rides_whole_bed`], [`crate::render_fx::audio_order`]); text/svg/label having no camera or lane ([`crate::fx_record::uses`], [`crate::render_fx::is_rendered`]); dropped footage ([`crate::cut_clamp::clamp_to_cut`]).
//! Here: which scene an effect acts on, the lens-switch sentence, whether a second can be held, what the preview owes the render about sound, and whose frame "source" means.

use crate::cut::{EffectKind, Fx, Seg};
use crate::cut_cam;
use crate::cut_effects_pass;
use crate::cut_hear;

/// §9 (`A zoom SHOULD carry `cam`...`; `Over inserts: none`): which scene an effect draws on. Only a zoom has a camera (its fractions are of one camera's frame); everything else is composed after the camera or heard, so it acts on every scene including a card.
pub fn acts_on_scene(fx: &Fx, scene: &Seg) -> bool {
    if fx.effect_kind() != Some(EffectKind::Zoom) {
        return true;
    }
    !scene.is_insert() && cut_cam::shown_from(fx.cam, cut_cam::scene_row(scene))
}

/// §9 (`Over inserts: none`; MUST row `the render MUST apply a volume over a card's own sound`): the effects a scene carries, minus what [`acts_on_scene`] excludes. A zoom framed on a camera stops at a splice; a volume rides over it.
pub fn drawn_on_scene(fx: &[Fx], scene: &Seg) -> Vec<Fx> {
    fx.iter()
        .filter(|effect| acts_on_scene(effect, scene))
        .cloned()
        .collect()
}

/// §9 (`A zoom does not cross into a clip from another camera`) at placement: how long its band may be. `Some((length, row))` gives the trimmed length and the row that ended it, for [`lens_switch`]; `None` when it never crosses a row change or trimming would leave under [`crate::cut_clamp::MIN_SURVIVING_SECONDS`].
pub fn trim_at_lens_switch(fx: &Fx, segs: &[Seg]) -> Option<(f64, i32)> {
    let start_row = segs
        .iter()
        .find(|scene| !scene.is_insert() && scene.s <= fx.t && fx.t < scene.e)
        .map(cut_cam::scene_row)?;
    let crossing = segs
        .iter()
        .filter(|scene| !scene.is_insert())
        .find_map(|scene| {
            let row = cut_cam::scene_row(scene);
            (scene.s >= fx.t && zoom_stops_at_the_cut(fx, start_row, row)).then_some((scene.s, row))
        })?;
    let trimmed = crossing.0 - fx.t;
    (trimmed >= crate::cut_clamp::MIN_SURVIVING_SECONDS).then_some((trimmed, crossing.1))
}

/// §9 (`A lens switch under a zoom says so`): the sentence for a zoom whose camera is no longer on screen. The row is named by number as §9 spells it ([`cut_cam::row_name`] would give a recording name); the second comes from [`cut_hear::scene_clock`].
pub fn lens_switch(t: f64, cam: i32) -> String {
    format!(
        "the zoom at {} was framed on camera {cam} \u{2014} it no longer applies here",
        cut_hear::scene_clock(t)
    )
}

/// §9: the zoom stops where the camera changes, so the caller ends the band at the cut and says [`lens_switch`] once. Text, svg and sound are unaffected.
pub fn zoom_stops_at_the_cut(fx: &Fx, from_row: i32, to_row: i32) -> bool {
    fx.effect_kind() == Some(EffectKind::Zoom) && from_row != to_row
}

/// §9 (`A stop at a dropped second SHOULD be refused when placed`): can a frame be held at this second? Only whether the cut keeps it as footage matters. Half-open like every range (`spec/05-cut.md` §7), so a scene's last second cannot be held. [`no_frame`] covers holes and cards; [`crate::cut_clamp::clamp_to_cut`] drops stops a later edit orphans.
pub fn place_stop(t: f64, segs: &[Seg]) -> Result<(), String> {
    let kept = segs
        .iter()
        .any(|seg| !seg.is_overwrite_insert() && seg.s <= t && t < seg.e);
    if kept {
        return Ok(());
    }
    Err(no_frame(t))
}

/// The reason [`place_stop`] refused (§9 gives no wording): name the moment and why.
pub fn no_frame(t: f64) -> String {
    format!(
        "there is no frame at {} \u{2014} that second is not in the cut, so nothing can be held on it",
        cut_hear::scene_clock(t)
    )
}

/// §9 (`The preview plays sound at picture speed here; it SHOULD say "sound at 1× in the render only"`): the notice for `own`/`scene`, which need sound on a different clock from the picture ([`crate::fx_lane::previewable_sound`]).
pub fn says_render_only(snd: &str) -> Option<&'static str> {
    matches!(snd, "own" | "scene").then(|| "sound at 1\u{d7} in the render only")
}

/// §9 (`The preview MUST play "pitch" as the render does, or say it cannot`): which sounds the preview can copy on the picture's clock. `mute` joins the other way (render silences the bed, [`crate::render_fx::mute_expr`]); `own`/`scene` are [`says_render_only`]'s pair.
pub fn preview_plays_pitch_as_render(snd: &str) -> bool {
    matches!(snd, "" | "pitch")
}

/// §9 (`A volume SHOULD carry `lane``): which sound a gain acts on. Unnamed is the whole bed ([`crate::cut_hear::gain_under`]); a named one reaches that lane alone.
pub fn gain_reaches(fx: &Fx, lane: &str) -> bool {
    match fx.lane() {
        None => true,
        Some(name) => name == lane,
    }
}

/// §9 (`The render MUST apply a volume over a card's own sound`): does this gain act on a clip hearing exactly the lanes in `heard`? A whole-bed volume reaches every clip (a card hears no lane, its own sound is the bed); a lane-named cue needs that lane heard.
pub fn gain_reaches_the_clip(fx: &Fx, heard: &[String]) -> bool {
    match fx.lane() {
        None => true,
        Some(name) => heard.iter().any(|lane| lane == name),
    }
}

/// §9 (`or one lane's name, gained before the mix`): a named lane is gained before the lanes are summed; the whole bed after the mix and before the narration ([`crate::render_fx::audio_order`]).
pub fn gained_before_the_mix(fx: &Fx) -> bool {
    fx.lane().is_some()
}

/// §9 (`While an effect form is open, the preview MUST show the lens camera`): which row the preview draws; with a form open the watched row steps aside.
pub fn preview_camera(form_open: bool, watched: Option<i32>, lens: i32) -> i32 {
    if form_open {
        return lens;
    }
    watched.unwrap_or(lens)
}

/// §9 (`With cameras of different shapes, both MUST use the first footage clip's frame`): whose shape "source" means. The first footage clip wins, else the playing camera's shape; [`FALLBACK_SHAPE`] when no frame has been read.
pub fn source_frame(first_footage: Option<(i32, i32)>, playing: Option<(i32, i32)>) -> (i32, i32) {
    first_footage.or(playing).unwrap_or(FALLBACK_SHAPE)
}

/// The shape used when no frame has been read: the shipped preview declaration.
pub const FALLBACK_SHAPE: (i32, i32) = (
    crate::ui::window::PREVIEW_PANEL_W as i32,
    crate::ui::window::PREVIEW_PANEL_H as i32,
);

/// §9 (`With more than one camera row or lane the cut brief names the rows and lanes`): whether a brief needs to say anything about cameras and lanes.
pub fn names_rows_and_lanes(rows: usize, lanes: usize) -> bool {
    rows > 1 || lanes > 1
}

/// The brief's one header line naming camera rows and heard lanes, or `None` in the single-camera case ([`names_rows_and_lanes`]). Models know nothing of them unless told, hence a proposed volume hits the whole bed (§9).
pub fn brief_lines(rows: &[String], lanes: &[String]) -> Option<String> {
    if !names_rows_and_lanes(rows.len(), lanes.len()) {
        return None;
    }
    let mut line = String::from("SHOWN FROM: ");
    if rows.len() > 1 {
        line.push_str(&format!("camera {}", rows.join(", camera ")));
    } else {
        line.push_str("one camera");
    }
    if lanes.len() > 1 {
        line.push_str(&format!(", HEARD LANES: {}", lanes.join(", ")));
    } else if lanes.len() == 1 {
        line.push_str(&format!(", HEARD LANE: {}", lanes[0]));
    }
    Some(line)
}

/// §9 (`suggested scenes all take row 0`): the row a suggested scene is placed on; the cut pass knows nothing of cameras.
pub const SUGGESTED_ROW: i32 = 0;

/// §9: which row a suggested scene is filmed from. A row the reply NAMED is kept; one the session does not have falls back to [`SUGGESTED_ROW`]. Rows are the stored numbers [`crate::cut_cam::scene_row`] answers.
pub fn suggested_scene_cam(named: Option<i32>, rows: &[i32]) -> i32 {
    match named {
        Some(row) if rows.contains(&row) => row,
        _ => SUGGESTED_ROW,
    }
}

/// The same door read off the scene a suggestion lands into, before storing: its stored `cam` (an insert names none) against this session's rows. Called from [`crate::ui::cut_suggest_land`].
pub fn suggested_scene_cam_of(scene: &Seg, rows: &[i32]) -> i32 {
    suggested_scene_cam(
        if scene.is_insert() {
            None
        } else {
            Some(scene.cam)
        },
        rows,
    )
}

/// §9 (`suggested zooms are always centred at height 0.6`): the framing a model proposes, aimed at no picture. The height is [`cut_effects_pass::ZOOM_HEIGHT`], shared with §F3.11 and `effects.proposedZoomHeight`.
pub fn suggested_zoom_is_centred() -> (f64, f64, f64) {
    (0.5, 0.5, cut_effects_pass::ZOOM_HEIGHT)
}
