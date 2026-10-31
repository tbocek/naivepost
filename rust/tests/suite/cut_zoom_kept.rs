//! The zoom a project opens at: the one left there, or the whole session across the band.

use adw::prelude::*;
use naivepost::timeline::Recording;
use naivepost::ui;
use naivepost::ui::cut_view;

use crate::common;

fn project(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(dir.join("naivepost.json"), "{}").unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 0.0, "e": 600.0}]}"#,
    )
    .unwrap();
    dir
}

#[test]
fn a_project_opens_fitted_then_at_the_zoom_it_was_left_at() {
    common::one_app_round(|app| {
        cut_view::hold_open_zoom_for_test(false);
        let root = common::temp_dir_created("zoom-kept");
        let first = project(&root, "first.naivepost");
        let model = naivepost::project::load(&first).unwrap();
        let window = ui::window::build_window_at(app, &model, "Cut", &first);
        window.present();
        ui::set_session_recordings(&[Recording {
            base: "cam".to_string(),
            start: 0.0,
            end: 600.0,
        }]);
        common::settle();
        cut_view::settle_view(&window);
        let view = cut_view::view_px(cut_view::surface_width(&window));
        assert!(
            (cut_view::pps() - view / 600.0).abs() < 1e-6,
            "fitted: {} over {view}",
            cut_view::pps()
        );

        ui::press_zoom(&window, true);
        let left_at = cut_view::pps();
        let kept: f64 = std::fs::read_to_string(first.join("cut/zoom.txt"))
            .expect("the zoom is kept")
            .trim()
            .parse()
            .unwrap();
        assert!((kept - left_at).abs() < 0.001);
        window.close();
        common::settle();

        let second = project(&root, "second.naivepost");
        std::fs::write(second.join("cut/zoom.txt"), "12.5").unwrap();
        let model = naivepost::project::load(&second).unwrap();
        let window = ui::window::build_window_at(app, &model, "Cut", &second);
        window.present();
        common::settle();
        cut_view::settle_view(&window);
        assert_eq!(cut_view::pps(), 12.5, "the saved zoom wins over the fit");
        window.close();
        common::settle();
        let _ = std::fs::remove_dir_all(root);
    });
}
