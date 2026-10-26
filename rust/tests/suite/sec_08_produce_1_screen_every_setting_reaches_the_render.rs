//! §08-produce#1-screen, the encoder settings, through their REAL widgets: every dropdown (Codec, Preset,
//! Resolution, Frame rate, Audio, Subtitles) set with `set_selected` and every tick (Frame timing, Channels,
//! Frame edges) toggled with `set_active` on the widget the page drew. Each change must land in the project file
//! (`naivepost.json`'s `produce`, what the next opening and the render read) and move the render's up-to-date
//! stamp (§F5.3), and where the render's own argv builder reads it, show up in what ffmpeg is told. Before this,
//! only Container and the CRF slider were driven through a widget; the rest were tested on `set_setting` alone.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use naivepost::project::Produce;
use naivepost::ui;

/// The stamp the render would compute for these settings and nothing else, so a change shows.
fn stamp_of(tree: &naivepost::layout::Tree, settings: &Produce) -> String {
    naivepost::produce_stamp::Input {
        settings,
        segs: &[],
        lines: &[],
        sources: &[],
        aspect: "",
        voice: "",
        fx: &[],
        lanes: &[],
        shape: (1920, 1080),
    }
    .stamp_with(tree, None, 0)
}

/// The produce settings as the project file holds them right now.
fn kept(session: &std::path::Path) -> Produce {
    naivepost::project::load(session)
        .expect("the project file reads")
        .produce
}

#[test]
fn sec_08_produce_1_screen_every_setting_reaches_the_project_and_the_render() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let root = common::temp_dir_created("produce-settings");
        let session = root.join("settings.naivepost");
        std::fs::create_dir_all(&session).expect("a project folder");
        let project = naivepost::project::Project::default();
        naivepost::project::save(&project, &session).expect("the project file");
        naivepost::startup::set_open_dir(&session);
        let tree = naivepost::layout::Tree::new(&session).expect("a project tree");
        let window = ui::window::build_window_at(app, &project, "Produce", &session);
        window.present();
        common::settle_workers();

        for row in [
            "Codec",
            "Preset",
            "Resolution",
            "Frame rate",
            "Audio",
            "Subtitles",
        ] {
            let name = ui::produce_page::row_widget(row);
            let picker = common::widget_in(&window, &name)
                .and_then(|widget| widget.downcast::<gtk4::DropDown>().ok())
                .unwrap_or_else(|| panic!("the page drew the {row} dropdown ({name})"));
            let options = naivepost::produce_screen::options(row).expect("the row has options");
            let before = kept(&session);
            let stamp_before = stamp_of(&tree, &before);
            let next = (picker.selected() as usize + 1) % options.len();
            picker.set_selected(next as u32);
            common::settle();
            let after = kept(&session);
            assert_ne!(
                after, before,
                "{row} set to {:?} through its widget landed in naivepost.json",
                options[next]
            );
            assert_eq!(
                after,
                ui::produce_page::row_settings(),
                "{row}: the file holds what the page shows"
            );
            assert_ne!(
                stamp_of(&tree, &after),
                stamp_before,
                "{row}: the render's up-to-date stamp moved, so the next ▶ encodes again"
            );
            match row {
                "Codec" | "Preset" => {
                    let argv = naivepost::produce_render::codec_args(&after).join(" ");
                    assert_ne!(
                        argv,
                        naivepost::produce_render::codec_args(&before).join(" "),
                        "{row}: what ffmpeg is told changed: {argv}"
                    );
                }
                "Frame rate" => {
                    let rate = |p: &Produce| {
                        naivepost::produce_render::frame_rate_args(
                            naivepost::produce_render::frame_rate_name(p.frame_rate),
                            p.vfr,
                        )
                    };
                    assert_ne!(rate(&after), rate(&before), "{row}: ffmpeg's rate changed");
                }
                _ => {}
            }
        }

        for (tick, read) in [
            (
                "tick-peak-rate-vfr",
                (|p: &Produce| p.vfr) as fn(&Produce) -> bool,
            ),
            ("tick-mono", |p: &Produce| p.mono),
            ("tick-blurred", |p: &Produce| p.blurred_edges),
        ] {
            let check = common::widget_in(&window, tick)
                .and_then(|widget| widget.downcast::<gtk4::CheckButton>().ok())
                .unwrap_or_else(|| panic!("the page drew {tick}"));
            let before = kept(&session);
            let stamp_before = stamp_of(&tree, &before);
            check.set_active(!check.is_active());
            common::settle();
            let after = kept(&session);
            assert_eq!(
                read(&after),
                check.is_active(),
                "{tick} toggled through its widget landed in naivepost.json"
            );
            assert_ne!(
                stamp_of(&tree, &after),
                stamp_before,
                "{tick}: the render's up-to-date stamp moved"
            );
        }
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
