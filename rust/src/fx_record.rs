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

/// §06-effects#1-record (`ease stores "" for linear, keeping old files byte-identical`): linear is the empty
/// string, not a name. It stays the empty string on a save so a file this build has touched still reads as one
/// it never wrote.
pub const LINEAR_EASE: &str = "";

/// §06-effects#1-record (`rate (1 own clock; 0 = stop)`): a stop is not a seventh kind, it is a speed whose
/// rate is nought — [`crate::cut_hear::rate_under`]'s reading (`if effect.rate == 0.0 { 1.0 }`) of the same
/// number, which plays the picture at its own clock while the seconds run out.
pub fn is_stop(fx: &Fx) -> bool {
    fx.effect_kind() == Some(EffectKind::Speed) && fx.rate == 0.0
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

/// §06-effects#1-record: creating a record — the mirror of [`uses`].
pub fn blank(kind: EffectKind, at: f64, dur: f64) -> Fx {
    Fx {
        kind: spell(kind).to_string(),
        t: at,
        dur,
        ..Default::default()
    }
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
