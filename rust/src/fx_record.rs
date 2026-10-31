//! §06-effects#1-record — `spec/06-effects.md` §1, the effect record's field table.

use crate::cut::{EffectKind, Fx};
use crate::cut_hear;

/// §06-effects#1-record — one row of §1's table. `Words` and `Source` are the file's own two keys (`text`,
/// `src`): one key serves a text's words and a label's name, exactly as the row reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Total length including the fades — the width of the bar.
    Dur,
    Trans,
    Tout,
    Ease,
    Cx,
    Cy,
    Hf,
    Wf,
    Stay,
    Rate,
    Snd,
    Gain,
    /// The `text` key: the words of a caption, the name of a label.
    Words,
    /// The `src` key: an svg's file.
    Source,
    Cam,
    Lane,
}

/// §06-effects#1-record — does this kind read this field? §1 is normative for the answer and this function is
/// that table transcribed once, so a form asked "what do I show for a volume?" never disagrees with the render.
pub fn uses(kind: EffectKind, field: Field) -> bool {
    use EffectKind as K;
    use Field as F;
    match field {
        // `t` is not in the table because it needs no row: every effect happens at a session second.
        F::Dur => true,
        F::Trans | F::Tout | F::Ease => !matches!(kind, K::Label),
        F::Cx | F::Cy | F::Hf => matches!(kind, K::Zoom | K::Text | K::Svg),
        F::Wf => matches!(kind, K::Text | K::Svg),
        F::Stay => matches!(kind, K::Zoom),
        F::Rate | F::Snd => matches!(kind, K::Speed),
        F::Gain => matches!(kind, K::Volume),
        F::Words => matches!(kind, K::Text | K::Label),
        F::Source => matches!(kind, K::Svg),
        F::Cam => matches!(kind, K::Zoom),
        F::Lane => matches!(kind, K::Volume),
    }
}

pub fn fractions_read_off_source_frame(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Zoom)
}

/// §06-effects#1-record (`ease stores "" for linear, keeping old files byte-identical`): linear is the empty
/// string, not a name. It stays the empty string on a save so a file this build has touched still reads as one
/// it never wrote.
pub const LINEAR_EASE: &str = "";

/// §06-effects#1-record: does this effect's fade have no named shape, i.e. linear?
pub fn ease_is_linear(fx: &Fx) -> bool {
    fx.ease == LINEAR_EASE
}

/// §06-effects#1-record (`keeping old files byte-identical`): will saving this effect write no `ease` key at
/// all? Only a linear fade is skipped, which is what makes an untouched file come back unchanged — the reason
/// the field has a name for "no name" instead of defaulting to `"linear"`.
pub fn keeps_the_old_bytes(fx: &Fx) -> bool {
    ease_is_linear(fx)
}

/// §06-effects#1-record (`an unknown name survives untouched`): every curve this build knows and every curve a
/// later one may add is kept as written. Dropping an unrecognised shape would silently re-time a fade someone
/// chose, on a build that was never asked to edit it.
pub fn ease_survives(ease: &str) -> bool {
    !ease.is_empty()
}

/// §06-effects#1-record (`rate (1 own clock; 0 = stop)`): a stop is not a seventh kind, it is a speed whose
/// rate is nought — [`crate::cut_hear::rate_under`]'s reading (`if effect.rate == 0.0 { 1.0 }`) of the same
/// number, which plays the picture at its own clock while the seconds run out.
pub fn is_stop(fx: &Fx) -> bool {
    fx.effect_kind() == Some(EffectKind::Speed) && fx.rate == 0.0
}

/// §06-effects#1-record (`1 own clock`): this rate leaves the clip running at its own speed, which for a stop
/// is what "the picture holds, its sound is dealt with separately" means.
pub fn own_clock(fx: &Fx) -> bool {
    fx.effect_kind() == Some(EffectKind::Speed) && fx.rate == 1.0
}

/// §06-effects#1-record (`sound answer "" / pitch / own / scene / mute`): what a rate does with the clip's
/// sound. `Default` is the empty string — say nothing and the clip keeps its own sound at the new rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Snd {
    Default,
    Pitch,
    Own,
    Scene,
    Mute,
}

/// §1's four answers, in the order the table lists them. `""` is deliberately absent: it is the absence of an
/// answer rather than one of them, which is why [`Snd::Default`] exists.
pub const SND_ANSWERS: [&str; 4] = ["pitch", "own", "scene", "mute"];

/// §06-effects#1-record: the rate's sound answer. A name this build does not know behaves as the empty one —
/// the clip's own sound, not silence — because guessing that an unknown word means "hush it" would take sound
/// out of a video on the strength of a spelling.
pub fn snd_of(fx: &Fx) -> Snd {
    match fx.snd.as_str() {
        "pitch" => Snd::Pitch,
        "own" => Snd::Own,
        "scene" => Snd::Scene,
        "mute" => Snd::Mute,
        _ => Snd::Default,
    }
}

/// §06-effects#1-record: is this one of the answers §1 lists (the empty string included)?
pub fn snd_is_known(snd: &str) -> bool {
    snd.is_empty() || SND_ANSWERS.contains(&snd)
}

/// §06-effects#1-record (`linear gain, 0 = silence`): nought is silence rather than "unset", so an effect that
/// means to be quiet says 0 and means it.
pub fn gain_is_silence(fx: &Fx) -> bool {
    fx.gain == 0.0
}

/// §06-effects#1-record (`gain … linear`): the loudest a volume effect may ask for. `// P.eng.maxGain`, read
/// from [`crate::cut_hear`] where that row lives — it is playbin's own ceiling, so this cannot raise it.
pub fn gain_ceiling() -> f64 {
    cut_hear::MAX_GAIN
}

/// §06-effects#1-record (`lane (new): "" = the whole bed; a lane's name = that lane only`): does this volume
/// ride everything? Asked through [`Fx::lane`] so the empty string is read in one place. A kind with no `lane`
/// row never rides the whole bed either — it rides nothing.
pub fn rides_whole_bed(fx: &Fx) -> bool {
    fx.effect_kind() == Some(EffectKind::Volume) && fx.lane().is_none()
}

/// §06-effects#1-record (`cam (new): the camera row it was framed on`): which row a zoom belongs to, and so
/// which row's preview it bends. `None` for every kind with no `cam` row — a caption is not framed on a camera.
pub fn camera_row(fx: &Fx) -> Option<usize> {
    (fx.effect_kind() == Some(EffectKind::Zoom)).then(|| fx.cam.max(0) as usize)
}

/// §06-effects#1-record (`text / src … the words | the file | – | the name`): the `text` key, which is a
/// caption's words on a text and a label's name on a label. Which of the two it is comes from the kind, exactly
/// as the row reads; splitting it into two keys would leave a second answer to keep in step.
pub fn words(fx: &Fx) -> &str {
    &fx.text
}

/// §06-effects#1-record (`src`): an svg's file. Its own key, so a caption that happens to look like a path is
/// still words.
pub fn drawing(fx: &Fx) -> &str {
    &fx.src
}

/// The length a hand-added effect starts with, so its bar is visible before F3.8 resizes it.
pub const NEW_EFFECT_SECONDS: f64 = 2.0;

/// The kind spelled the way `cut.json` writes it — [`crate::cut::EffectKind::parse`]'s table read
/// backwards. `"speed"`, never `"stop"`: §1 says a stop is a rate of nought, and `parse` folds both
/// spellings into Speed, so writing `"stop"` would put a second name on one kind.
fn spell(kind: EffectKind) -> &'static str {
    match kind {
        EffectKind::Zoom => "zoom",
        EffectKind::Speed => "speed",
        EffectKind::Text => "text",
        EffectKind::Svg => "svg",
        EffectKind::Volume => "volume",
        EffectKind::Label => "label",
    }
}

/// The word a person reads for this kind — the same words the `EFFECT_ITEMS` labels carry
/// (`src/cut_screen.rs`), so the status line and the dropdown cannot disagree about what was added.
pub fn label_of(kind: EffectKind) -> &'static str {
    match kind {
        EffectKind::Zoom => "Zoom",
        EffectKind::Speed => "Speed",
        EffectKind::Text => "Text",
        EffectKind::Svg => "SVG",
        EffectKind::Volume => "Volume",
        EffectKind::Label => "Label",
    }
}

/// The JSON key a [`Field`] is written under. Kept beside `json_key`'s caller rather than derived from
/// the enum's name so a renamed field cannot silently change the file format.
fn json_key(field: Field) -> &'static str {
    match field {
        Field::Dur => "dur",
        Field::Trans => "trans",
        Field::Tout => "tout",
        Field::Ease => "ease",
        Field::Cx => "cx",
        Field::Cy => "cy",
        Field::Hf => "hf",
        Field::Wf => "wf",
        Field::Stay => "stay",
        Field::Rate => "rate",
        Field::Snd => "snd",
        Field::Gain => "gain",
        Field::Words => "text",
        Field::Source => "src",
        Field::Cam => "cam",
        Field::Lane => "lane",
    }
}

/// §06-effects#1-record: creating a record — the mirror of [`uses`].
pub fn blank(kind: EffectKind, at: f64, dur: f64) -> Fx {
    Fx {
        kind: spell(kind).to_string(),
        t: at,
        dur,
        ..Default::default()
    }
}

/// §1's column rule stated as a check: for a freshly recorded effect of this kind, a field the table marks
/// `–` costs no key in the serialised record.
pub fn recorded_keys_are_the_kinds_own(kind: EffectKind, dur: f64) -> bool {
    let value = match serde_json::to_value(blank(kind, 0.0, dur)) {
        Ok(value) => value,
        Err(_) => return false,
    };
    let Some(map) = value.as_object() else {
        return false;
    };
    // `kind` and `t` are always written: an effect that is no kind or happens at no second is not a record.
    if map.get("kind").and_then(|v| v.as_str()) != Some(spell(kind)) || !map.contains_key("t") {
        return false;
    }
    [
        Field::Dur,
        Field::Trans,
        Field::Tout,
        Field::Ease,
        Field::Cx,
        Field::Cy,
        Field::Hf,
        Field::Wf,
        Field::Stay,
        Field::Rate,
        Field::Snd,
        Field::Gain,
        Field::Words,
        Field::Source,
        Field::Cam,
        Field::Lane,
    ]
    .into_iter()
    // A field the table grants may be present or absent depending on its value (the storage skips neutral
    // values), so only the denial is checkable here: a `–` row must never reach the file.
    .all(|field| uses(kind, field) || !map.contains_key(json_key(field)))
}

/// Add one record of this kind to the cut and hand it back, so the caller can place or report it
/// without reaching into `cut.fx`.
pub fn record_into(cut: &mut crate::cut::Cut, kind: EffectKind, at: f64, dur: f64) -> Fx {
    let made = blank(kind, at, dur);
    cut.fx.push(made.clone());
    made
}

/// What the page says after recording one. The undo clause is not decoration: recording goes through
/// `ui::record_edit`, so ↶ does take it back (F2.13).
pub fn recorded_status(kind: EffectKind) -> String {
    format!(
        "{} recorded \u{2014} \u{21b6} Undo takes it back",
        label_of(kind)
    )
}
