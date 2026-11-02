//! §06-effects#4-render-how-each-effect-becomes-ffmpeg — checked against [`naivepost::render_fx`].
//!
//! The spec section is a bullet list, not numbered steps: its six bullets are read here as S1..S6 and its closing
//! frame-box rule as S7. Nothing in these tests runs ffmpeg or opens a window — the module returns filter text and
//! plain numbers, which is the whole reason a render rule can be pinned without rendering. Every time value below is
//! an exact binary number, so a comparison that must be exact stays exact; only the two log lines and the frame boxes
//! are compared whole, everything else asks about the fragment of a filter string that carries the meaning.

use naivepost::render_fx as render;

// --- S1: speed -----------------------------------------------------------------------------------------------------

// --- S2: the stop --------------------------------------------------------------------------------------------------

// --- S3: the camera ------------------------------------------------------------------------------------------------

// --- S4: words and drawings ----------------------------------------------------------------------------------------

// --- S5: volume ----------------------------------------------------------------------------------------------------

// --- S6: the label -------------------------------------------------------------------------------------------------

/// S6 (`Label: nothing`): the mark survives because F3.7 exists to make it survive; the render's answer is no filter
/// text at all, so a label cannot change a picture by accident.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s6_a_label_renders_nothing() {
    assert!(!render::is_rendered("label"));
    for kind in ["zoom", "speed", "stop", "text", "svg", "volume"] {
        assert!(render::is_rendered(kind), "{kind} renders");
    }
    // An unknown kind in an old file is not something this build knows how to draw either.
    assert!(!render::is_rendered("marker"));
}

// --- S7: the frame box ---------------------------------------------------------------------------------------------

/// S7 (`no aspect → the footage's own; with an aspect the resolution tier names the short side (1080p on 9:16 =
/// 1080×1920), even sides`): naming the tier by an edge is what lets a project change shape and keep its sharpness.
#[test]
fn sec_06_effects_4_render_how_each_effect_becomes_ffmpeg_s7_the_frame_box_is_the_tier_on_the_short_side(
) {
    assert_eq!(render::frame_box("", 1920, 1080), (1920, 1080));
    assert_eq!(render::frame_box("source", 1280, 720), (1280, 720));

    assert_eq!(render::frame_box("9:16", 1920, 1080), (1080, 1920));
    assert_eq!(render::frame_box("16:9", 1920, 1080), (1920, 1080));
    assert_eq!(render::frame_box("1:1", 1920, 1080), (1080, 1080));
    assert_eq!(render::frame_box("4:5", 1920, 1080), (1080, 1350));

    // Even sides whatever the ratio works out to — every encoder this project offers is 4:2:0.
    for aspect in ["9:16", "16:9", "1:1", "4:5", "3:4"] {
        let (w, h) = render::frame_box(aspect, 1920, 1080);
        assert_eq!((w % 2, h % 2), (0, 0), "{aspect} came out {w}x{h}");
    }
    assert_eq!(render::frame_box("3:4", 1920, 1080), (1080, 1440));

    // A shape nobody spelled right behaves like no shape: the output's aspect is not worth stopping a video over.
    assert_eq!(render::frame_box("wide", 1280, 720), (1280, 720));
    assert_eq!(render::frame_box("9:", 1280, 720), (1280, 720));
    assert_eq!(render::TIER_SHORT_SIDE, 1080);
}
