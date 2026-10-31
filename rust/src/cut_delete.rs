//! §05-cut#8 (`⌦ refuses`) — `spec/05-cut.md` §8's first bullet, the ⌦ half.

use crate::cut::Seg;
use crate::cut_hear;
use crate::cut_select::Scope;

/// §05-cut#8 (`⌦ refuses "⌦ drops footage — the selection is <base>'s sound"`): a lane's seconds are not the
/// pictures' seconds, and a verb that removes scenes cannot be pointed at a sound. The copy and insert verbs
/// aim at sound happily; dropping is not one of them.
pub fn drops_footage(base: &str) -> String {
    format!("\u{2326} drops footage \u{2014} the selection is {base}'s sound")
}

/// §05-cut#8 (`⌦ refuses "the playhead is not on a kept scene — click a green one, or drag a region"`): ⌦ with
/// nothing held and no scene under the line. Green is what the band tints a kept scene, so the sentence points
/// at the picture rather than at a word that would need explaining.
pub fn no_kept_scene() -> &'static str {
    "the playhead is not on a kept scene \u{2014} click a green one, or drag a region"
}

/// §05-cut#8: what ⌦ answers before it deletes anything — `Some(sentence)` when the hand and the line give it
/// nothing to remove, `None` when it may go ahead.
pub fn refuses(scope: Option<&Scope>, scene: Option<&Seg>) -> Option<String> {
    if let Some(Scope::Sound { recording }) = scope {
        return Some(drops_footage(recording));
    }
    if scene.is_none() {
        return Some(no_kept_scene().to_string());
    }
    None
}

/// §05-cut#8: the scene ⌦ would take at `t` — the [`crate::cut_hear::scene_at`] index read back as the scene
/// itself, so a caller holding the list hands [`refuses`] the thing it asks for. `None` is a second the cut
/// removed, which is exactly the answer that makes ⌦ refuse.
pub fn scene_under(segs: &[Seg], t: f64) -> Option<&Seg> {
    let index = cut_hear::scene_at(segs, t)?;
    segs.get(index)
}
