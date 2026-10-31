//! F3.2 Aspect ratio — `spec/06-effects.md` F3.2.

use crate::cut::{Cut, Fx};
use crate::fx_zoom;

// --- the dropdown -----------------------------------------------------------------------------------------------

/// F3.2: the choice that asks for nothing — the footage's own shape.
pub const SOURCE: &str = "source";

/// F3.2 (`Dropdown source / 9:16 / 1:1 / 4:5 / 16:9`): the five shapes, in the order the dropdown lists them —
/// spec/06-effects.md:93 and spec/inventory/effects.md §B agree on it, and it is a shape-order (source first, then
/// tallest to widest) rather than an alphabetical one, so the list cannot be sorted without moving words under
/// someone's pointer.
pub const ASPECTS: [&str; 5] = ["source", "9:16", "1:1", "4:5", "16:9"];

/// F3.2: the dropdown's tooltip, verbatim from spec/inventory/effects.md §B.
pub const DROPDOWN_HELP: &str = concat!(
    "the shape of the finished video \u{2014} source is the footage's own, 9:16 is a vertical short. ",
    "The whole frame fits inside it (bars either side) until \u{25AD} View frames a region; ",
    "the outline on the preview is what the finished video shows"
);

// --- what a pick stores ------------------------------------------------------------------------------------------

/// F3.2 (`holds the whole frame for 1 s`): how long the framing zoom holds.
pub const HOLD_SECONDS: f64 = 1.0;

const FIRST_SECOND: &str = "0:00";

/// F3.2: what `cut.aspect` holds after a pick.
pub fn stored(choice: &str) -> String {
    if choice == SOURCE {
        return String::new();
    }
    choice.to_string()
}

// --- the three branches: what to store, what to add, what to say ------------------------------------------------

const SOURCE_STATUS: &str =
    "aspect: the source's own \u{2014} the video comes out the shape it was filmed";

/// What picking a shape did: the aspect to store, the zoom to add (F3.2 adds one in exactly one branch), and the
/// line the page prints.
#[derive(Debug, Clone, PartialEq)]
pub struct Chosen {
    /// What `cut.aspect` becomes — [`stored`] spells the choice for the file.
    pub aspect: String,
    /// A zoom to add, or `None`.
    pub zoom: Option<Fx>,
    pub status: String,
}

/// F3.2: press a choice and this answers everything about that press — what to store, whether a zoom comes with
/// it, and what to say.
pub fn choose(current: &Cut, choice: &str) -> Chosen {
    if choice == SOURCE {
        return Chosen {
            aspect: String::new(),
            zoom: None,
            status: SOURCE_STATUS.to_string(),
        };
    }

    if fx_zoom::a_staying_zoom_exists(&current.fx) {
        return Chosen {
            aspect: stored(choice),
            zoom: None,
            status: format!(
                "aspect {} \u{2014} the zooms on the lane decide the framing",
                choice
            ),
        };
    }

    // The whole frame, centred, from the first second — [`fx_zoom::SETTLED`] read as a rect, so this cannot drift
    // from what F3.1's camera path calls the settled frame.
    Chosen {
        aspect: stored(choice),
        zoom: Some(fx_zoom::new_zoom(
            (
                fx_zoom::SETTLED.cx,
                fx_zoom::SETTLED.cy,
                fx_zoom::SETTLED.hf,
            ),
            0.0,
            HOLD_SECONDS,
            true,
            0,
        )),
        status: format!(
            "aspect {} \u{2014} a \u{2295} zoom at {FIRST_SECOND} holds the whole frame, centred",
            choice
        ),
    }
}

/// F3.2: do the pick.
pub fn apply(cut: &mut Cut, choice: &str) -> String {
    let chosen = choose(cut, choice);
    cut.aspect = chosen.aspect;
    if let Some(zoom) = chosen.zoom {
        cut.fx.push(zoom);
    }
    chosen.status
}
