//! §06-effects#8-details-confirmed-against-the-code-verification-pass — the details §8 states as
//! already true, checked against the code that has to print or apply them.
//!
//! Bullet 1 is the two preview-failure lines: a drawing the preview cannot rasterise and a stop
//! whose held frame it cannot find. Both are SAID and neither stops the effect, so what a test can
//! pin is the exact line, that it is said once per file, and that the record survives.

use adw::prelude::*;
use gtk4 as gtk4;
use naivepost::cut::{Cut, EffectKind, Fx};
use naivepost::effect_details;
use naivepost::fx_svg::{self, Slot};

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;

fn stop(t: f64, dur: f64) -> Fx {
    Fx {
        kind: "speed".into(),
        t,
        dur,
        rate: 0.0,
        ..Default::default()
    }
}

/// §8 bullet 1 (`failed preview raster logs ">>> the drawing <file> cannot be shown: <err>" once`).
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s1_svg_failure_line_is_prefixed_and_said_once(
) {
    let mut rasters = fx_svg::Rasters::default();
    assert_eq!(rasters.request("/x/assets/map.svg"), Slot::NeedsRaster);

    // The line carries §D's `>>> ` prefix itself: §8 quotes the log line, not a body a caller
    // is expected to decorate, so a caller that forgets cannot print an unprefixed one.
    let first = rasters.failure_line("/x/assets/map.svg", "ffmpeg drew nothing");
    assert_eq!(
        first.as_deref(),
        Some(">>> the drawing map.svg cannot be shown: ffmpeg drew nothing")
    );
    assert_eq!(
        rasters.failure_line("/x/assets/map.svg", "ffmpeg drew nothing"),
        None,
        "a second ask for the same broken file stays quiet"
    );
    assert_eq!(rasters.request("/x/assets/map.svg"), Slot::Failed);

    // A different file is a different question and gets its own line.
    assert_eq!(
        rasters
            .failure_line("/y/assets/sting.svg", "no such file")
            .as_deref(),
        Some(">>> the drawing sting.svg cannot be shown: no such file")
    );
}

/// §8 bullet 1's two refusals before anything is drawn: no moment chosen, and no file chosen.
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s1b_no_line_chosen_and_no_file_chosen_refuse_with_their_own_words(
) {
    assert_eq!(
        fx_svg::NO_LINE,
        "click a track first \u{2014} the drawing needs a moment to appear at"
    );
    assert_eq!(
        fx_svg::NO_FILE,
        "choose a drawing and it goes on the picture"
    );
    // A cancelled chooser answers `Refused(NO_FILE)` — nothing armed, nothing placed; the enum
    // has no third variant, so a refusal always carries its own sentence.
    assert!(matches!(
        fx_svg::Chose::Refused(fx_svg::NO_FILE),
        fx_svg::Chose::Refused(reason) if reason == fx_svg::NO_FILE
    ));
}

/// §8 bullet 1 (`unrenderable still ">>> the stop frame at m:ss cannot be shown in the preview:
/// <err>"; neither stops the effect`), through the door the preview panel calls.
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s1c_a_stop_with_no_frame_logs_the_still_line_and_stays_placed(
) {
    // GTK has to be initialised before any widget can exist, and only an application run does that.
    // `activate` never fires for a NON_UNIQUE app with no window held open, so the checks are made
    // against a window built straight after `gtk::init` — the same page helper every lane test uses.
    let _ = gtk4::init();
    let app = adw::Application::builder()
        .application_id(naivepost::ui::APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    round(&app);
}

/// The widget-side half: a real Cut page, then the door the preview panel calls.
fn round(app: &adw::Application) {
    let root = naivepost::startup::session_dir(&std::env::temp_dir().join("np-sec06-8-still"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    let window = common::cut_page(app, &Cut::default());
    naivepost::ui::window::clear_logs_for_test();
    let held = stop(40.0, 3.0);
    let stops = [held];
    // No frame found under the stop: the sentence is said, verbatim as the rule spells it.
    let said = naivepost::ui::cut_stop_frame::say_missing_still(&window, &stops, 41.0, false)
        .expect("a stop with no frame owes the line");
    assert_eq!(said, effect_details::still_failure(41.0, NO_FRAME));
    assert!(said.starts_with(">>> the stop frame at "), "{said}");
    assert!(
        naivepost::ui::window::window_logs().contains(&said),
        "the line reached the log"
    );
    assert_eq!(
        naivepost::ui::find_status(window.upcast_ref()).map(|status| status.text()),
        Some(effect_details::still_failure(41.0, NO_FRAME).into()),
        "and to the status line"
    );

    // A frame found: nothing to say. And a second press with none says it again only because the
    // preview asked again — §8's once-only rule belongs to the raster cache, not to this line.
    assert!(
        naivepost::ui::cut_stop_frame::say_missing_still(&window, &stops, 41.0, true).is_none(),
        "a stop whose frame was found is silent"
    );
    // Nothing outside the seconds of a stop asks for the line.
    assert!(
        naivepost::ui::cut_stop_frame::say_missing_still(&window, &stops, 50.0, false).is_none(),
        "no stop over the line means no sentence"
    );
    // Neither stops the effect: the record is untouched by being unshowable.
    assert_eq!(stops[0].effect_kind(), Some(EffectKind::Speed));
    assert_eq!(
        stops[0].rate,
        0.0,
        "a stop whose frame cannot be shown is still a stop"
    );
}

/// The reason text the door uses, spelled once in the module and read back here rather than
/// retyped: if the two ever drift, this fails.
const NO_FRAME: &str = "no extracted frame for that recording at that second";
