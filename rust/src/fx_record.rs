//! §06-effects#1-record — `spec/06-effects.md` §1, the effect record's field table.
//!
//! [`crate::cut::Fx`] is one record serving all six kinds and §1 is its field table: a field means what its kind
//! says it means, and a `–` row means the field is never read for that kind. This module holds that mapping and
//! the few readings the table states outright — which fractions are of which frame, what an empty `ease` or an
//! empty `snd` or an empty `lane` answers, when a rate is a stop, when a gain is silence — as plain functions.
//!
//! Nothing here re-decides what [`crate::cut`] already owns: the typed kind, the bar's span, whether a box was
//! written and where an unwritten one defaults all stay there, and this module calls them. The UI reads through
//! these so that a form can be built from the table rather than from six hand-written branches.

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
///
/// A `–` row means the field is never READ for that kind — it is not an error to find one in a file, since
/// [`Fx`] keeps every key and an unknown kind must cost the cut nothing.
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

/// §06-effects#1-record (`cx, cy, hf … fractions of the SOURCE frame` for a zoom, `of the OUTPUT frame` for a
/// text or a drawing): which frame a box is measured against. Only a zoom looks at the picture it reframes;
/// a caption is placed on what the video becomes. The three kinds with no box at all answer `false` because
/// they read no fractions, not because theirs are output ones — [`uses`] says that part.
///
/// A zoom has no `wf` for the same reason it has a source frame: its width follows the aspect of what it framed.
pub fn fractions_read_off_source_frame(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Zoom)
}

// --- the two fades and their shape -------------------------------------------------------------------------

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

// --- speed/stop: the rate and what its sound does ------------------------------------------------------------

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

// --- volume: the gain and what it rides ------------------------------------------------------------------------

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

// --- zoom: the row it was framed on ---------------------------------------------------------------------------

/// §06-effects#1-record (`cam (new): the camera row it was framed on`): which row a zoom belongs to, and so
/// which row's preview it bends. `None` for every kind with no `cam` row — a caption is not framed on a camera.
///
/// Row 0 is a real answer and not "unset": [`Fx::cam`] skips serialising at 0, so the default row costs no key
/// in the file while still being the first row.
pub fn camera_row(fx: &Fx) -> Option<usize> {
    (fx.effect_kind() == Some(EffectKind::Zoom)).then(|| fx.cam.max(0) as usize)
}

// --- text / svg / label: the one key that carries two things ------------------------------------------------------

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
