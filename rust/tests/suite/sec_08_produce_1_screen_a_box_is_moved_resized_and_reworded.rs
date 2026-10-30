//! §08-produce#1-screen **7**: every box on the thumbnail is edited where it is seen, the title band included
//! (the prototype's `textOverlay`): the pointer names what a press would do, a drag from a box's middle moves
//! it and a drag on its border resizes it, the box follows the hand while the drag lasts, the title's ✎ rewords
//! the line printed on the picture, and every edit is printed onto `thumbnail.png` at once, with no run. The
//! first rewrite drew the dashed borders but every press on them drew a new box: nothing could be moved,
//! resized or reworded, and the title had no ✎ at all.

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

/// A drawn 16:9 thumbnail carrying its title in the default band and one marked text low on the left.
fn project(root: &Path) -> PathBuf {
    let dir = root.join("boxes.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"publish": {"frames": [], "own": false, "title": "The video", "thumb_title": "Old line",
            "title_seeded": true, "texts": [{"cx": 0.25, "cy": 0.75, "wf": 0.2, "hf": 0.2, "text": "Hi"}],
            "prompt": "", "negative": "", "description": ""}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"aspect": "16:9", "segs": [{"s": 0.0, "e": 2.0}]}"#,
    )
    .unwrap();
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    png(&tree.thumbnail_plain_png(), 1600, 900);
    dir
}

fn words_box(dialog: &adw::MessageDialog) -> gtk::TextView {
    dialog
        .extra_child()
        .and_then(|child| child.downcast::<gtk::Frame>().ok())
        .and_then(|frame| frame.child())
        .and_then(|child| child.downcast::<gtk::ScrolledWindow>().ok())
        .and_then(|scroller| scroller.child())
        .and_then(|child| child.downcast::<gtk::TextView>().ok())
        .expect("the dialog's words box")
}

#[test]
fn sec_08_produce_1_screen_a_box_is_moved_resized_and_reworded_on_the_picture() {
    common::pin_private_cwd();
    let root = common::temp_dir("produce-boxes");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Produce", &dir);
        window.set_default_size(1280, 980);
        window.present();
        ui::produce_page::hold_window_for_test(&window);
        common::wait_for_stable_layout(&window);
        common::settle();
        let tree = naivepost::layout::Tree::new(&dir).unwrap();
        let printed = tree.thumbnail_png();
        assert!(!printed.is_file(), "nothing printed before the first edit");

        let plate = common::widget_in(&window, "thumbnail-picture").expect("the plate");
        let (px, py, pw, ph) = ui::produce_paint::picture_on_plate(
            f64::from(plate.width()),
            f64::from(plate.height()),
        );
        let at = |fx: f64, fy: f64| (px + fx * pw, py + fy * ph);

        // The pointer says what a press does: the move hand in the title's middle, the side arrows on the
        // marked text's right border, nothing special clear of every box.
        let motion = plate
            .observe_controllers()
            .into_iter()
            .filter_map(|c| c.ok())
            .find_map(|c| c.downcast::<gtk::EventControllerMotion>().ok())
            .expect("the plate's pointer watcher");
        let cursor_at = |(x, y): (f64, f64)| {
            motion.emit_by_name::<()>("motion", &[&x, &y]);
            plate
                .cursor()
                .and_then(|c| c.name())
                .map(|n| n.to_string())
                .unwrap_or_default()
        };
        assert_eq!(cursor_at(at(0.5, 0.25)), "move", "inside the title band");
        assert_eq!(
            cursor_at(at(0.35, 0.75)),
            "col-resize",
            "on the mark's right border"
        );
        assert_eq!(
            cursor_at(at(0.25, 0.65)),
            "row-resize",
            "on the mark's top border"
        );
        assert_eq!(
            cursor_at(at(0.35, 0.65)),
            "nesw-resize",
            "on its top right corner"
        );
        assert_eq!(cursor_at(at(0.75, 0.75)), "default", "clear of every box");

        let gesture = ui::produce_words_press::words_gesture_for_test(&window).expect("the drag");

        // The title band, taken by its middle and carried 12 % of the picture down.
        let from = at(0.5, 0.25);
        gesture.emit_by_name::<()>("drag-begin", &[&from.0, &from.1]);
        let down = 0.12 * ph;
        gesture.emit_by_name::<()>("drag-update", &[&0.0f64, &down]);
        let (held, live) = ui::produce_words_press::dragging().expect("the band follows the hand");
        assert_eq!(held, ui::produce_words_press::Hold::Title);
        assert!(
            (live.cy - 0.37).abs() < 0.01,
            "painted where the hand has it: {live:?}"
        );
        assert_eq!(
            ui::produce_paint::title_band(),
            live,
            "the plate paints the band from the drag"
        );
        gesture.emit_by_name::<()>("drag-end", &[&0.0f64, &down]);
        common::settle();
        assert!(
            ui::produce_words_press::dragging().is_none(),
            "the drag let go"
        );
        assert!(
            ui::produce_words::open_dialog().is_none(),
            "a drag on a box moves it; it does not ask for words for a new one"
        );
        let state = ui::produce_page::read_state();
        let band = state.title_box.expect("the title has a box of its own now");
        assert!(
            (band.cy - 0.37).abs() < 0.01 && (band.cx - 0.5).abs() < 0.01,
            "the band moved down, not across: {band:?}"
        );
        assert!(
            (band.wf - 1.0).abs() < 0.01 && (band.hf - 0.4).abs() < 0.01,
            "and kept its size"
        );
        assert_eq!(state.texts.len(), 1, "no new box was drawn");

        // The marked text, taken by its right border and pulled 10 % wider: the left edge stays.
        let from = at(0.35, 0.75);
        let right = 0.10 * pw;
        gesture.emit_by_name::<()>("drag-begin", &[&from.0, &from.1]);
        gesture.emit_by_name::<()>("drag-update", &[&right, &0.0f64]);
        gesture.emit_by_name::<()>("drag-end", &[&right, &0.0f64]);
        common::settle();
        let mark = ui::produce_page::read_state().texts[0].clone();
        assert!(
            (mark.wf - 0.30).abs() < 0.01 && (mark.cx - 0.30).abs() < 0.01,
            "the right border moved, the left one stayed: {mark:?}"
        );
        assert!(
            (mark.hf - 0.2).abs() < 0.01 && (mark.cy - 0.75).abs() < 0.01 && mark.text == "Hi",
            "its height, place and words are as they were: {mark:?}"
        );

        // Every edit is printed onto the thumbnail the export and the upload take, without a run.
        common::wait_until("the words printed onto thumbnail.png", || printed.is_file());

        // The title's ✎ rewords the line ON THE PICTURE; the video's title stays as it is.
        let chip = common::widget_in(&window, "words-chip-title")
            .and_then(|w| w.downcast::<gtk::Button>().ok())
            .expect("the title wears a ✎");
        chip.emit_clicked();
        common::wait_until("the words dialog opened", || {
            ui::produce_words::open_dialog().is_some()
        });
        let dialog = ui::produce_words::open_dialog().expect("the dialog");
        let view = words_box(&dialog);
        let buffer = view.buffer();
        assert_eq!(
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .as_str(),
            "Old line",
            "the dialog opens on the printed line"
        );
        buffer.set_text("New line");
        dialog.response("save");
        common::settle();
        let state = ui::produce_page::read_state();
        assert_eq!(state.thumb_title, "New line");
        assert_eq!(
            state.title, "The video",
            "the video's title is not the picture's line"
        );

        // Remove takes the line off the picture, and its ✎ with it.
        let chip = common::widget_in(&window, "words-chip-title")
            .and_then(|w| w.downcast::<gtk::Button>().ok())
            .expect("the title still wears a ✎");
        chip.emit_clicked();
        common::wait_until("the words dialog opened again", || {
            ui::produce_words::open_dialog().is_some()
        });
        ui::produce_words::open_dialog().unwrap().response("remove");
        common::settle();
        let state = ui::produce_page::read_state();
        assert!(
            state.thumb_title.is_empty() && state.title_seeded,
            "off for good: {state:?}"
        );
        assert!(
            common::widget_in(&window, "words-chip-title").is_none(),
            "no ✎ for a line not printed"
        );
        window.close();
    });
}
