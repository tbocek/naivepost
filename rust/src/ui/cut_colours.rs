//! §05-cut.md §6's "colours" — every tint the Cut page paints, one named constant each.
//!
//! `spec/inventory/cut.md` §D lists them by name and §10 counts them among the engineering values a tuning
//! pass may move, so they are parameters: one place, one spelling, read by whoever paints. The alpha travels
//! with the colour where the inventory spells one (the kept tint's 0.3).

/// The type every tint here is: cairo's components, 0.0..1.0.
pub type Rgb = (f64, f64, f64);

/// Band ground: the prototype's dark tape (`go/cut.go`'s track background).
pub const GROUND: Rgb = (0.13, 0.13, 0.13);
/// The selection band's ground across each recording's width (`bandGround`), a shade off the tape.
pub const BAND_GROUND: Rgb = (0.16, 0.16, 0.17);
pub const RULER_INK: Rgb = (0.62, 0.62, 0.62);
/// The kept scene's green (§1's badge 25); the inventory's alpha is `RGBA(0.2,0.8,0.3,0.3)`.
pub const KEPT_TINT: Rgb = (0.2, 0.8, 0.3);
pub const KEPT_TINT_ALPHA: f64 = 0.3;
/// A kept bar's end grips, lighter than its fill: these are the pixels a press takes.
pub const KEPT_BORDER: Rgb = (0.5, 0.92, 0.58);
pub const KEPT_EDGE: Rgb = (0.15, 0.85, 0.25);
/// The selection's blue (§1's badge 27).
pub const SELECTION_BLUE: Rgb = (0.25, 0.5, 0.9);
/// An insert's violet band (§1: "every insert a violet band across every row").
pub const INSERT_VIOLET: Rgb = (0.55, 0.35, 0.75);
/// Where a recording runs on its picture row, under its frames: a shade off the dark tape.
pub const FILM_GROUND: Rgb = (0.18, 0.18, 0.2);
/// A picture cell whose frame has not been decoded yet.
pub const SLATE: Rgb = (0.25, 0.26, 0.29);
/// A cut lane's own row: a plate grey the footage rows do not use, so a lane reads as another row.
pub const LANE_GROUND: Rgb = (0.2, 0.21, 0.25);
pub const LANE_BORDER: Rgb = (0.35, 0.35, 0.4);

/// The seconds an OVER card took from the footage: the dropped-gap grey.
pub const OVER_EATEN: Rgb = (0.1, 0.1, 0.11);
/// A recording's striped edge, where its tape starts and ends (§1's "amber striped").
pub const EDGE_AMBER: Rgb = (0.9, 0.7, 0.2);
/// A take the join pass removed whole: "flagged for a look, not an error" (F1.10), so a yellow wash and frame, not red.
pub const JOIN_WASH: Rgb = (0.98, 0.85, 0.1);
/// The frame round that take.
pub const JOIN_FRAME: Rgb = (0.96, 0.8, 0.05);
/// In green, out red (§1's own words for the flags).
pub const MARK_IN_GREEN: Rgb = (0.15, 0.7, 0.25);
pub const MARK_OUT_RED: Rgb = (0.85, 0.2, 0.2);
/// The scrim over a stretch the cut dropped (§F2.2 S6): a solid neutral grey, since any alpha strong enough
/// to beat the 0.95 ground also muddies the kept fills. Held here so the band and the placeholder strip dim alike.
pub const SCRIM: Rgb = (0.45, 0.45, 0.45);
/// The rose wash under a speed effect (§1). It is `fx_lane::bar_colour(Speed)`'s own number: the lane's bar
/// and the wash over the pictures are the same mark seen twice, so one place holds it.
pub const SPEED_TINT: Rgb = (0.92, 0.42, 0.6);
/// The dashed white outline around the row a click set the preview to watch.
pub const WATCH_OUTLINE: Rgb = (1.0, 1.0, 1.0);
/// §1's playhead: "2 px line on its own layer", RGB(0.9, 0.15, 0.15) per inventory/cut.md §A.
pub const PLAYHEAD_RGB: Rgb = (0.9, 0.15, 0.15);
/// §B's gutter: "a 30 px column down the left edge of EVERY band". Black, as `spec/img/05-cut.png` draws it.
pub const GUTTER_GROUND: Rgb = (0.0, 0.0, 0.0);
/// The 1 px divider between the gutter and the tape.
pub const GUTTER_EDGE: Rgb = (0.29, 0.29, 0.33);

/// One tint spelled the way §10's engineering line spells a list of numbers: `0.2 0.8 0.3`.
pub fn spell(rgb: Rgb) -> String {
    [rgb.0, rgb.1, rgb.2]
        .into_iter()
        .map(|part| {
            let text = format!("{part}");
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        })
        .collect::<Vec<String>>()
        .join(" ")
}
