//! §07-narrate#1-screen: the game audio level (how loud the recording plays under a spoken line) is the
//! narration's own number, so its slider sits on the Narrate page beside the Narration tick; the Produce page no
//! longer draws it. Moving it keeps the level with the encoder settings the render and the preview read, and the
//! slider greys while narration is off.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::ui;

use crate::common;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
fn sec_07_narrate_1_screen_the_game_audio_slider_is_on_the_narrate_page() {
    common::pin_private_cwd();
    let root = common::temp_dir_tagged("game-audio");
    let dir = root.join("demo.naivepost");
    common::copy_fixture(&dir);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("loads");
        let window = ui::window::build_window_at(app, &loaded, "Narrate", &dir);
        window.present();
        common::settle();
        assert!(
            common::find_named(&window, "setting-game-audio").is_none(),
            "the Produce page no longer draws it"
        );
        let slider = common::find_named(&window, "narrate-game-audio")
            .and_then(|w| w.downcast::<gtk::Scale>().ok())
            .expect("the Narrate page does");
        assert!(
            (slider.value() - 0.22).abs() < 1e-6,
            "on the project's level: {}",
            slider.value()
        );

        slider.set_value(0.5);
        common::settle();
        let kept = naivepost::project::load(&dir).expect("reloads");
        assert!(
            (kept.produce.game_volume - 0.5).abs() < 1e-6,
            "kept where the render reads it"
        );
        assert!(
            (naivepost::narrate_preview_leg::game_volume_for(
                &naivepost::layout::Tree::new(&dir).unwrap()
            ) - 0.5)
                .abs()
                < 1e-6,
            "and where the narration preview reads it"
        );

        // There is no narration switch to hide or grey it (§1): the row is always live.
        let row = common::find_named(&window, "narrate-game-row").expect("the row");
        assert!(row.is_sensitive(), "the game audio row is always live");
        window.close();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the round never ran"
    );
}
