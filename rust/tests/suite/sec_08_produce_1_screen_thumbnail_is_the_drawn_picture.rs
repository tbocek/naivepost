//! §08-produce#1-screen **7**: the thumbnail is the DRAWN picture (`thumbnail-plain.png`, the words printed over
//! it as the render prints them), and a word box is a fraction of that picture wherever it sits on the plate.
//! The first rewrite showed the image row's base there instead, measured a dragged box against the whole plate
//! and printed the words against the picture, so on a letterboxed thumbnail a box landed below where it was
//! drawn. Also the page's other readings off the project: the base's shape and the video's (the crop box's
//! rule) and the separate recordings the tooltip names as mixed.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::ui;

use crate::common;

fn png(path: &Path, wide: i32, high: i32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let pixbuf =
        gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, false, 8, wide, high)
            .expect("a pixbuf");
    pixbuf.fill(0x3060_c0ff);
    pixbuf.savev(path, "png", &[]).expect("png written");
}

/// A square base in the image row, a wide (4:1) drawn thumbnail, a 16:9 cut and one separate recording.
fn project(root: &Path) -> PathBuf {
    let dir = root.join("thumb.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    png(&dir.join("base.png"), 100, 100);
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"sources": [{"path": "project:sources/mic.wav", "footage": false, "narrator": 0, "sepvoice": false, "tracks": []}],
            "publish": {"frames": ["project:base.png"], "own": false, "texts": [], "title": "", "prompt": "a ship",
            "negative": "", "description": ""}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"aspect": "16:9", "segs": [{"s": 0.0, "e": 2.0}]}"#,
    )
    .unwrap();
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    png(&tree.thumbnail_plain_png(), 400, 100);
    dir
}

#[test]
fn sec_08_produce_1_screen_the_thumbnail_is_the_drawn_picture_and_a_box_measures_on_it() {
    common::pin_private_cwd();
    let root = common::temp_dir("produce-thumb");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Produce", &dir);
        window.set_default_size(1280, 980);
        window.present();
        ui::produce_page::hold_window_for_test(&window);
        common::wait_for_stable_layout(&window);
        common::settle();

        let state = ui::produce_page::read_state();
        assert_eq!(
            state.base_shape,
            Some((100, 100)),
            "the base's own shape, off its file"
        );
        assert_ne!(
            state.video_shape,
            (0, 0),
            "and the video's, off the cut's aspect"
        );
        assert!(
            naivepost::produce_screen::shows_crop_box(state.base_shape, state.video_shape),
            "so the square base wears the 16:9 crop box"
        );
        assert_eq!(
            state.mixed_recordings,
            vec!["mic".to_string()],
            "the separate recording is mixed"
        );

        let plate = common::widget_in(&window, "thumbnail-picture").expect("the plate");
        let (w, h) = (f64::from(plate.width()), f64::from(plate.height()));
        let (px, py, pw, ph) = ui::produce_paint::picture_on_plate(w, h);
        assert!(
            (pw / ph - 4.0).abs() < 0.05,
            "the plate shows the drawn 4:1 thumbnail, not the square base: {pw} x {ph} on {w} x {h}"
        );

        // A box drawn from 30 % to 60 % across the picture and 20 % to 80 % down it, clear of every snap target.
        let from = (px + 0.30 * pw, py + 0.20 * ph);
        let to = (px + 0.60 * pw, py + 0.80 * ph);
        let gesture = ui::produce_words_press::words_gesture_for_test(&window).expect("the drag");
        gesture.emit_by_name::<()>("drag-begin", &[&from.0, &from.1]);
        gesture.emit_by_name::<()>("drag-end", &[&(to.0 - from.0), &(to.1 - from.1)]);
        common::settle();
        common::wait_until("the words dialog opened", || {
            ui::produce_words::open_dialog().is_some()
        });
        let dialog = ui::produce_words::open_dialog().expect("the dialog");
        let view = dialog
            .extra_child()
            .and_then(|child| child.downcast::<gtk::Frame>().ok())
            .and_then(|frame| frame.child())
            .and_then(|child| child.downcast::<gtk::ScrolledWindow>().ok())
            .and_then(|scroller| scroller.child())
            .and_then(|child| child.downcast::<gtk::TextView>().ok())
            .expect("the dialog's words box");
        view.buffer().set_text("Episode 6");
        dialog.response("save");
        common::settle();
        let marks = ui::produce_page::read_state().texts;
        assert_eq!(marks.len(), 1, "one drag, one box");
        let mark = &marks[0];
        assert!(
            (mark.cx - 0.45).abs() < 0.01 && (mark.cy - 0.5).abs() < 0.01,
            "the box's centre is where it was drawn ON THE PICTURE: ({:.3}, {:.3})",
            mark.cx,
            mark.cy
        );
        assert!(
            (mark.wf - 0.30).abs() < 0.01 && (mark.hf - 0.60).abs() < 0.01,
            "and its size is a fraction of the picture: {:.3} x {:.3}",
            mark.wf,
            mark.hf
        );
        window.close();
    });
}
