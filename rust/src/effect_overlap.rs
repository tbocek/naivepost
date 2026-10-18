//! §06-effects#9-where-recordings-overlap — two cameras filming the same seconds and several sounds running at
//! once, and what each effect does when it lands on ground shared between them.
//!
//! Most of §9's table is already kept elsewhere, and this module points at those owners rather than restating them:
//!
//! * `mute` and a stop silencing the **whole bed** — as the preview has always done, not only the lens camera's
//!   sound — is [`crate::render_fx::mute_expr`], one expression over the bed;
//! * the 0.15 s dip where delayed sound rejoins the picture is [`crate::fx_lane::SOUND_DIP_SECONDS`] whole and
//!   [`crate::effect_details::dip_each_side`] half, one on each side of the splice;
//! * a volume's default reach — the bed, after the mix, before the narration — is
//!   [`crate::fx_record::rides_whole_bed`] and [`crate::render_fx::audio_order`];
//! * text, svg and the label having no camera and no lane is [`crate::fx_record::uses`] with `Field::Cam` /
//!   `Field::Lane`, and the label not being drawn at all is [`crate::render_fx::is_rendered`];
//! * an effect left pointing at footage a later edit dropped is [`crate::cut_clamp::clamp_to_cut`]'s, once, at the
//!   end of a suggest — §9's stop refusal below is a different thing: it happens while placing.
//!
//! What is here is the camera-and-lane question §9 states that nothing asked yet: which scene an effect acts on,
//! what to say when the lens moves under a zoom, whether a second can be held at all, what the preview owes the
//! render about sound, and whose frame "source" means.

use crate::cut::{EffectKind, Fx, Seg};
use crate::cut_cam;
use crate::cut_effects_pass;
use crate::cut_hear;
use crate::fx_record;

/// §9 (`A zoom SHOULD carry `cam`, the row it was framed on, and act only on clips shown from that camera`;
/// `Over inserts: none`): which scene an effect draws on.
///
/// A zoom is the only kind with a camera, because its fractions are fractions of one camera's frame — put the same
/// numbers on another camera's frame and the box lands somewhere else in the picture, which is the prototype's
/// "effects stay" jump that §9 replaces. Everything else is composed after the camera (text, svg) or heard rather
/// than seen (volume, speed), so it acts on every scene including a card: those two are §9's `Over an insert or
/// freeze: all or nothing`, and a volume over a card is §9's MUST row below.
pub fn acts_on_scene(fx: &Fx, scene: &Seg) -> bool {
    if fx.effect_kind() != Some(EffectKind::Zoom) {
        return true;
    }
    !scene.is_insert() && cut_cam::shown_from(fx.cam, cut_cam::scene_row(scene))
}

/// §9 (`A stop … SHOULD be refused when placed`) reads this record the same way the rest of
/// the app does: a stop is a speed whose rate is 0.
pub fn is_a_stop(fx: &Fx) -> bool {
    crate::fx_record::is_stop(fx)
}

/// §9 (`Over inserts: none`, with the MUST row `the render MUST apply a volume over a card's
/// own sound, as the preview does`): the effects a scene actually carries — its kept list
/// minus what [`acts_on_scene`] says does not act here. A card brings its own picture, so a
/// zoom framed on a camera stops at the splice, while a volume rides straight over it.
pub fn drawn_on_scene(fx: &[Fx], scene: &Seg) -> Vec<Fx> {
    fx.iter()
        .filter(|effect| acts_on_scene(effect, scene))
        .cloned()
        .collect()
}

/// §9 (`A zoom does not cross into a clip from another camera`), applied where a zoom is
/// placed: how long its band may be. The first kept footage scene after the one it starts in
/// that is filmed from another row ends it, and [`zoom_stops_at_the_cut`] is that decision.
/// `Some((length, row))` names the trimmed length and the row that ended it, so the caller can
/// say [`lens_switch`]; `None` when the band never crosses a row change, or when trimming would
/// leave it under [`crate::cut_clamp::MIN_SURVIVING_SECONDS`] — a sliver of a zoom is not worth
/// an edit, so the whole band stays as placed.
pub fn trim_at_lens_switch(fx: &Fx, segs: &[Seg]) -> Option<(f64, i32)> {
    let start_row = segs
        .iter()
        .find(|scene| !scene.is_insert() && scene.s <= fx.t && fx.t < scene.e)
        .map(cut_cam::scene_row)?;
    let crossing = segs.iter().filter(|scene| !scene.is_insert()).find_map(|scene| {
        let row = cut_cam::scene_row(scene);
        (scene.s >= fx.t && zoom_stops_at_the_cut(fx, start_row, row)).then_some((scene.s, row))
    })?;
    let trimmed = crossing.0 - fx.t;
    (trimmed >= crate::cut_clamp::MIN_SURVIVING_SECONDS).then_some((trimmed, crossing.1))
}

/// §9 (`A lens switch under a zoom says so: "the zoom at m:ss was framed on camera N — it no longer applies
/// here"`): the sentence for a zoom whose camera stopped being the one on screen. The row is named by its **number**,
/// as §9 spells it — [`cut_cam::row_name`] would give the recording's name, which is what the gutter and the badges
/// use and would not match this line. The second comes from [`cut_hear::scene_clock`], the clock every other
/// sentence about an effect quotes.
pub fn lens_switch(t: f64, cam: i32) -> String {
    format!(
        "the zoom at {} was framed on camera {cam} \u{2014} it no longer applies here",
        cut_hear::scene_clock(t)
    )
}

/// §9 (`A zoom does not cross into a clip from another camera`): the box no longer jumps with the lens — it stops
/// where the camera changes, so the caller ends the band at the cut and says [`lens_switch`] once. Nothing else is
/// stopped by a lens switch: text and svg are composed on the output frame after the camera, and sound has no
/// camera to lose.
pub fn zoom_stops_at_the_cut(fx: &Fx, from_row: i32, to_row: i32) -> bool {
    fx.effect_kind() == Some(EffectKind::Zoom) && from_row != to_row
}

/// §9 (`A stop at a dropped second SHOULD be refused when placed`): can a frame be held at this second? A stop's
/// picture is the frame at `t` from the lens of the scene holding `t`, so the only question is whether the cut keeps
/// that second as footage — which camera it comes from is not the question, and §9 resolves the lens at render time.
///
/// The window is half-open like every other range in the app (`spec/05-cut.md` §7: preview and render agree on
/// half-open ranges), so a scene's last second belongs to whatever starts there and cannot be held.
/// [`no_frame`] answers both ways a second can be missing — one in a hole the cut dropped, one over a card, where
/// there is no frame to freeze. This is the refusal at placement time;
/// [`crate::cut_clamp::clamp_to_cut`] still drops a stop that a later edit leaves pointing at nothing.
pub fn place_stop(t: f64, segs: &[Seg]) -> Result<(), String> {
    let kept = segs
        .iter()
        .any(|seg| !seg.is_overwrite_insert() && seg.s <= t && t < seg.e);
    if kept {
        return Ok(());
    }
    Err(no_frame(t))
}

/// The reason [`place_stop`] refused. §9 gives no wording, so this is ours, in §7's voice — name the moment and the
/// reason for it, and say what follows: a stop holds a frame, and there is none at a second the cut does not keep.
pub fn no_frame(t: f64) -> String {
    format!(
        "there is no frame at {} \u{2014} that second is not in the cut, so nothing can be held on it",
        cut_hear::scene_clock(t)
    )
}

/// §9 (`The preview plays sound at picture speed here; it SHOULD say "sound at 1× in the render only"`): what to
/// tell the person when they ask for `own` or `scene` and the preview cannot do it. Those two need the sound running
/// on a different clock from the picture, which one player cannot do — [`crate::fx_lane::previewable_sound`] is that
/// rule for the lane's own purposes, and this is the sentence that goes with it. Saying it beats playing at picture
/// speed quietly: the person hears something the video will not say.
pub fn says_render_only(snd: &str) -> Option<&'static str> {
    matches!(snd, "own" | "scene").then(|| "sound at 1\u{d7} in the render only")
}

/// §9 (`The preview MUST play "pitch" as the render does, or say it cannot`): the empty answer and `pitch` both move
/// the sound with the picture's clock — [`crate::render_fx::asetrate_chain`] is that on the render's side — so the
/// preview owes them an honest copy rather than a notice.
///
/// `mute` answers false for a different reason from `own`/`scene`: it is not a pitch question at all, and §9 settles
/// it by making the **render** join the preview (silence the whole bed, [`crate::render_fx::mute_expr`]) rather than
/// the other way round. `own` and `scene` are [`says_render_only`]'s pair.
/// §9 (`The preview MUST play "pitch" as the render does, or say it cannot`): which sound
/// answers the preview can copy on the picture's clock. `mute` joins the other way round — the
/// render goes silent for the whole bed ([`crate::render_fx::mute_expr`]) — and `own`/`scene`
/// are [`says_render_only`]'s pair, so this is the row of §9's table that owes no notice.
pub fn preview_plays_pitch_as_render(snd: &str) -> bool {
    matches!(snd, "" | "pitch")
}

/// §9 (`A volume SHOULD carry `lane`: "" the whole bed (today), or one lane's name`): which sound a gain acts on. An
/// unnamed volume is the whole bed — the lens camera's sound plus every heard lane, which is what
/// [`crate::cut_hear::gain_under`] answers today — and reaches every lane by definition; a named one reaches that
/// lane alone, which is the reach §9 added because there was no way to get at one lane before.
pub fn gain_reaches(fx: &Fx, lane: &str) -> bool {
    match fx.lane() {
        None => true,
        Some(name) => name == lane,
    }
}

/// §9 (`The render MUST apply a volume over a card's own sound, as the preview does`): does this gain act on a
/// clip that hears exactly the lanes in `heard`? A whole-bed volume reaches the clip whatever it hears — a card,
/// an insert or a freeze has no heard lane at all and its OWN sound is the bed the gain acts on, which is why the
/// plan may not ask "is any of these lanes heard" of a bed cue (the bug this fixes: a card segment hears nothing,
/// so every cue was filtered out and the render applied no volume over a card while the preview did). A
/// lane-named cue still needs that very lane heard; a duck aimed at a silent recording reaches nothing.
pub fn gain_reaches_the_clip(fx: &Fx, heard: &[String]) -> bool {
    match fx.lane() {
        // The whole bed: always gained, card or footage, however many lanes the scene mixes in.
        None => true,
        Some(name) => heard.iter().any(|lane| lane == name),
    }
}

/// §9 (`or one lane's name, gained before the mix`): where in the graph a cue runs. A named lane is gained **before**
/// the lanes are summed; the whole bed rides **after** the mix and before the narration, which is
/// [`crate::render_fx::audio_order`]'s sequence. The two cannot be swapped: a per-lane gain placed after the mix
/// would have to be applied to a sum it can no longer take apart.
pub fn gained_before_the_mix(fx: &Fx) -> bool {
    fx.lane().is_some()
}

/// §9 (`While an effect form is open, the preview MUST show the lens camera`): which row the preview draws. A form
/// is opened to judge one effect against the picture it will change, so the watched row has to step aside — a zoom
/// framed on the lens and shown over another camera's frame would be judged against the wrong picture. With no form
/// open, watching a row is what the click meant.
pub fn preview_camera(form_open: bool, watched: Option<i32>, lens: i32) -> i32 {
    if form_open {
        return lens;
    }
    watched.unwrap_or(lens)
}

/// §9 (`With cameras of different shapes, both MUST use the first footage clip's frame`):
/// whose shape "source" means. The render has always taken the first footage clip's; the
/// preview took whichever camera was playing, so a 9:16 lecture and a 16:9 slide deck each
/// got their own answer. This is the one rule both sides now read: the first footage clip
/// wins, and only when there is no footage clip at all does the playing camera's shape stand.
///
/// The shape a side falls back to when it has read no frame at all is [`FALLBACK_SHAPE`].
pub fn source_frame(
    first_footage: Option<(i32, i32)>,
    playing: Option<(i32, i32)>,
) -> (i32, i32) {
    first_footage.or(playing).unwrap_or(FALLBACK_SHAPE)
}

/// The shape a side falls back to when it has read no frame at all: the shipped preview
/// declaration, which is what an empty plate is measured like until a picture arrives.
pub const FALLBACK_SHAPE: (i32, i32) = (
    crate::ui::window::PREVIEW_PANEL_W as i32,
    crate::ui::window::PREVIEW_PANEL_H as i32,
);

/// §9 (`With more than one camera row or lane the cut brief names the rows and lanes`):
/// whether a model brief has to say anything about cameras and lanes at all. One row and
/// one lane is the single-camera case the passes were written for — nothing to name.
pub fn names_rows_and_lanes(rows: usize, lanes: usize) -> bool {
    rows > 1 || lanes > 1
}

/// The brief's one header line naming the camera rows and the heard lanes, or `None` in the
/// single-camera-and-single-lane case ([`names_rows_and_lanes`] decides). The model passes
/// know nothing of cameras and lanes unless this line says so, which is why a proposed volume
/// hits the whole bed (§9's last paragraph). One format for every pass that asks.
pub fn brief_lines(rows: &[String], lanes: &[String]) -> Option<String> {
    // The one question this line asks is §9's: is there more than one of either to name?
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

/// §9 (`suggested scenes all take row 0`): the row a suggested scene is placed on, since the cut pass knows nothing
/// of cameras. With more than one row the brief names them (§9's last sentence) and a suggestion can name one back;
/// until then every scene is filmed from the first.
pub const SUGGESTED_ROW: i32 = 0;

/// §9 (`suggested scenes all take row 0`, with `the cut brief names the rows`): which row a suggested scene is
/// filmed from, read at the door that places it. A reply that NAMED a row is kept — that is what naming the rows
/// in the brief is for, a suggestion may say which camera it means — and a row the session does not have falls
/// back to [`SUGGESTED_ROW`], because placing a scene on a camera nothing is recorded on is not a suggestion an
/// app may land. Rows here are the stored numbers [`crate::cut_cam::scene_row`] answers for a scene on the page.
pub fn suggested_scene_cam(named: Option<i32>, rows: &[i32]) -> i32 {
    match named {
        Some(row) if rows.contains(&row) => row,
        _ => SUGGESTED_ROW,
    }
}

/// The same door read off the scene a suggestion lands into, before it is stored: the row it named (its stored
/// `cam`, which is all the lens badge reads and writes; an insert names none, having no picture of its own) asked
/// against the rows this session has. The one call site that asks is [`crate::ui::cut_suggest_land`]'s placement.
pub fn suggested_scene_cam_of(scene: &Seg, rows: &[i32]) -> i32 {
    suggested_scene_cam(if scene.is_insert() { None } else { Some(scene.cam) }, rows)
}

/// §9 (`suggested zooms are always centred at height 0.6`): the framing a model proposes, with no picture in front
/// of it to aim at. The height is [`cut_effects_pass::ZOOM_HEIGHT`] — the same number §F3.11 places with and
/// `effects.proposedZoomHeight` catalogues — so a suggestion and what is placed from it cannot disagree.
pub fn suggested_zoom_is_centred() -> (f64, f64, f64) {
    (0.5, 0.5, cut_effects_pass::ZOOM_HEIGHT)
}

/// §9 (`a proposed volume hits the whole bed`): what a decoration pass asks for when it proposes a volume — no lane,
/// because it is not told there are any. Read from [`fx_record::rides_whole_bed`] rather than asserted here so the
/// two cannot drift: the record's own reading is the answer.
pub fn suggested_volume_reaches_the_bed(fx: &Fx) -> bool {
    fx_record::rides_whole_bed(fx)
}
