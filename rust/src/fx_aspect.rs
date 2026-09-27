//! F3.2 Aspect ratio — `spec/06-effects.md` F3.2.
//!
//! The dropdown in the form column gives the finished video its shape. Two of its five choices only report; one
//! of them also places a zoom, and that is the whole flow: pick 9:16 on footage nobody has framed yet and the
//! video would come out with bars, so the same press puts a staying zoom at 0:00 holding the whole frame. All of
//! it is arithmetic and three sentences, so it lives here. The Cut page's dropdown (`aspect-choice` in
//! `rust/src/ui/window.rs`) holds no rule of its own: it lists [`ASPECTS`], tooltips [`DROPDOWN_HELP`], and forwards
//! the pick through `ui::press_aspect`, which calls [`apply`] and records the result as one Undo step.

use crate::cut::{Cut, Fx};
use crate::fx_zoom;

// --- the dropdown -----------------------------------------------------------------------------------------------

/// F3.2: the choice that asks for nothing — the footage's own shape. Named here so a caller never compares a bare
/// `"source"` string and the module has one spelling of it.
pub const SOURCE: &str = "source";

/// F3.2 (`Dropdown source / 9:16 / 1:1 / 4:5 / 16:9`): the five shapes, in the order the dropdown lists them —
/// spec/06-effects.md:93 and spec/inventory/effects.md §B agree on it, and it is a shape-order (source first, then
/// tallest to widest) rather than an alphabetical one, so the list cannot be sorted without moving words under
/// someone's pointer.
pub const ASPECTS: [&str; 5] = ["source", "9:16", "1:1", "4:5", "16:9"];

/// F3.2: the dropdown's tooltip, verbatim from spec/inventory/effects.md §B. The one choice it explains is the one
/// nobody guesses: `9:16 is a vertical short`.
pub const DROPDOWN_HELP: &str = concat!(
    "the shape of the finished video \u{2014} source is the footage's own, 9:16 is a vertical short. ",
    "The whole frame fits inside it (bars either side) until \u{25AD} View frames a region; ",
    "the outline on the preview is what the finished video shows"
);

// --- what a pick stores ------------------------------------------------------------------------------------------

/// F3.2 (`holds the whole frame for 1 s`): how long the framing zoom holds. §10 files this as
/// `P.eng.aspectStaySeconds` (1.0, "length of the staying zoom an aspect change places"; prototype `cut_fx.go`
/// 1.0), and F3.2's own name `effects.aspectHoldSeconds` covers the same value — both rows in
/// [`crate::params::cut`] read this one constant, so neither spelling can drift from the other or from the rule.
///
/// This is NOT [`crate::fx_zoom::DEFAULT_SECONDS`]: that is an effect someone placed by hand and named in
/// `P.policy.effectDefaultSeconds` (zoom 3). This one is a side-effect of choosing a shape, exists to give the
/// camera something to settle on from the first frame rather than to be watched, and F3.2 says one second.
pub const HOLD_SECONDS: f64 = 1.0;

/// F3.2 (`a ⊕ zoom at 0:00`): the stamp that sentence prints. Spelled out rather than through [`crate::tools::mm_ss`],
/// which pads its minutes and would say `00:00` — the spec's status line does not, and this tree pins quoted UI
/// strings whole.
const FIRST_SECOND: &str = "0:00";

/// F3.2: what `cut.aspect` holds after a pick.
///
/// `source` stores the empty string, which is what the file already means by it: §10 spells the project's default
/// as `aspect (source)`, `Cut::aspect` is `skip_serializing_if = "String::is_empty"` so an unshaped cut writes no
/// key at all, and [`fx_zoom::aspect_is_set`] reads empty as 'no shape chosen'. Writing `"source"` into the file
/// would invent a fourth state — neither unset nor a shape.
///
/// [`crate::cut_screen::ASPECT_DEFAULT`] must NOT be written either: its `"16:9"` is what the Aspect ratio row
/// prints for an unset aspect, not something anyone chose.
pub fn stored(choice: &str) -> String {
    if choice == SOURCE {
        return String::new();
    }
    choice.to_string()
}

// --- the three branches: what to store, what to add, what to say ------------------------------------------------

/// F3.2 (`source`): the report for the footage's own shape, quoted whole by the spec's last sentence.
const SOURCE_STATUS: &str = "aspect: the source's own \u{2014} the video comes out the shape it was filmed";

/// What picking a shape did: the aspect to store, the zoom to add (F3.2 adds one in exactly one branch), and the
/// line the page prints. The three branches of the flowchart differ only in these three answers.
#[derive(Debug, Clone, PartialEq)]
pub struct Chosen {
    /// What `cut.aspect` becomes — [`stored`] spells the choice for the file.
    pub aspect: String,
    /// A zoom to add, or `None`. The person never drew this box, so it is not in a form anywhere: it is what the
    /// pick needs the camera to be doing from the first frame.
    pub zoom: Option<Fx>,
    pub status: String,
}

/// F3.2: press a choice and this answers everything about that press — what to store, whether a zoom comes with
/// it, and what to say. A pick of the shape already set is not special-cased: both non-source branches ask about
/// the zoom on the lane rather than about the string, and the status line reports the pick either way (a person
/// who re-picks 9:16 is told what frames it, which is the useful answer).
pub fn choose(current: &Cut, choice: &str) -> Chosen {
    if choice == SOURCE {
        // Nothing added, and nothing taken away: a pick of `source` says what shape the video comes out, not that
        // someone's zooms were a mistake. A staying zoom on the lane still reframes the footage inside those bars,
        // and only its own round (F3.8) removes it.
        return Chosen { aspect: String::new(), zoom: None, status: SOURCE_STATUS.to_string() };
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
    // from what F3.1's camera path calls the settled frame. `stay` because it is a reframing and not an event, and
    // that alone gives it no glides ([`fx_zoom::fades`]) — a camera with nowhere to pull back to has no fades.
    Chosen {
        aspect: stored(choice),
        zoom: Some(fx_zoom::new_zoom(
            (fx_zoom::SETTLED.cx, fx_zoom::SETTLED.cy, fx_zoom::SETTLED.hf),
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

/// F3.2: do the pick. One call changes the cut and hands back the status line, so the aspect and the zoom it
/// brings arrive together — which is what makes them one Undo step: the page pushes [`crate::cut::History`] once
/// around this call, not once per field.
pub fn apply(cut: &mut Cut, choice: &str) -> String {
    let chosen = choose(cut, choice);
    cut.aspect = chosen.aspect;
    if let Some(zoom) = chosen.zoom {
        cut.fx.push(zoom);
    }
    chosen.status
}
