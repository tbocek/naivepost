//! §08-produce through the pointer: the images row's slot buttons, the thumbnail's drag-a-box and ✎, the heading
//! buttons and the run bar's ▶ / ⏹ pressed with REAL input (`common::real_input`: xdotool on the test's X display,
//! GTK's own event route), each effect asserted on the page's state. A press that calls a server is pointed at a
//! port nothing listens on, so it ends on its own failure sentence without leaving the machine.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adw::prelude::*;
use naivepost::ui;

use crate::common;
use crate::common::real_input::{drain, real_click_centre, real_drag, real_key};

static RAN: AtomicBool = AtomicBool::new(false);

/// No server answers here: every model and drawing call is refused at once.
const NOBODY: &str = "http://127.0.0.1:1";

fn ffmpeg(args: &[&str], out: &Path) {
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y"])
        .args(args)
        .arg(out)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "ffmpeg wrote {}", out.display());
}

/// One stamped 20 s recording kept whole, and two pictures of its own for the images row.
fn project(root: &Path) -> (PathBuf, Vec<PathBuf>) {
    let dir = root.join("produce.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    std::fs::create_dir_all(dir.join("images")).unwrap();
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=25:duration=20",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=20",
            "-shortest",
        ],
        &dir.join("sources/2026-10-18 19-21-52.mkv"),
    );
    let pictures: Vec<PathBuf> = ["a", "b"]
        .iter()
        .enumerate()
        .map(|(n, name)| {
            let path = dir.join(format!("images/{name}.png"));
            ffmpeg(
                &[
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("testsrc=size=320x180:rate=1:duration=1,hue=h={}", 90 * n),
                    "-frames:v",
                    "1",
                ],
                &path,
            );
            path
        })
        .collect();
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"sources": [{"path": "project:sources/2026-10-18 19-21-52.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 0.0, "e": 20.0}]}"#,
    )
    .unwrap();
    (dir, pictures)
}

/// Settings that name a model on [`NOBODY`] for every server.
fn point_settings_nowhere() {
    let paths = naivepost::settings::from_environment().expect("settings folder");
    let mut conf = naivepost::settings::Conf::default();
    conf.model = "nobody-model".to_string();
    conf.server = NOBODY.to_string();
    conf.audio_server = NOBODY.to_string();
    conf.sd_server = NOBODY.to_string();
    naivepost::settings::save(&paths, &conf).expect("settings written");
}

fn status(window: &adw::ApplicationWindow) -> String {
    ui::state(window).status
}

fn frames() -> Vec<String> {
    ui::produce_page::read_state().frames
}

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk4::Widget {
    common::widget_in(window, name).unwrap_or_else(|| panic!("the page has {name}"))
}

/// The one toplevel on screen besides the window: a chooser, a save dialog or the words dialog.
fn other_window(window: &adw::ApplicationWindow) -> Option<gtk4::Window> {
    gtk4::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk4::Window>().ok())
        .find(|w| {
            w.is_visible() && w.upcast_ref::<gtk4::Widget>() != window.upcast_ref::<gtk4::Widget>()
        })
}

/// A real Escape on the dialog a press raised, which answers Cancel and closes it.
fn escape_from(window: &adw::ApplicationWindow, what: &str) {
    drain(Duration::from_millis(50), Duration::from_millis(400));
    let dialog = other_window(window).unwrap_or_else(|| panic!("{what} is on screen"));
    real_key(&dialog, "Escape");
    drain(Duration::from_millis(50), Duration::from_millis(400));
    assert!(!dialog.is_visible(), "Escape closed {what}");
}

/// The button labelled `label` under `root`: an `adw::MessageDialog`'s responses carry no names of their own.
fn labelled(root: &gtk4::Widget, label: &str) -> Option<gtk4::Button> {
    if let Some(button) = root.downcast_ref::<gtk4::Button>() {
        if button.label().as_deref() == Some(label) {
            return Some(button.clone());
        }
    }
    let mut child = root.first_child();
    while let Some(node) = child {
        if let Some(found) = labelled(&node, label) {
            return Some(found);
        }
        child = node.next_sibling();
    }
    None
}

fn words_dialog(window: &adw::ApplicationWindow) -> gtk4::Window {
    drain(Duration::from_millis(50), Duration::from_millis(400));
    other_window(window)
        .filter(|w| w.widget_name() == "words-dialog")
        .expect("the words dialog is on screen")
}

#[test]
fn sec_08_produce_every_surface_answers_real_input() {
    common::pin_private_cwd();
    let root = common::temp_dir_tagged("produce-real-input");
    let (dir, pictures) = project(&root);
    point_settings_nowhere();
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Produce", &dir);
        window.set_default_size(1280, 900);
        window.present();
        common::wait_for_stable_layout(&window);
        common::settle();
        common::settle_workers();

        // §1 item 1: Add image… raises the image chooser; Escape leaves the row as it was.
        let asked = ui::produce_choosers::asks_for_test("image");
        real_click_centre(&button(&window, "add-image-button"));
        assert_eq!(
            ui::produce_choosers::asks_for_test("image"),
            asked + 1,
            "a real click on Add image… raised the chooser"
        );
        escape_from(&window, "the image chooser");
        assert!(frames().is_empty(), "a closed chooser added nothing");
        // The chooser's answer door, as a pick lands: two slots, base first.
        for picture in &pictures {
            ui::produce_choosers::answer_image(&window, None, picture);
        }
        common::settle();
        let two = frames();
        assert_eq!(two.len(), 2, "two pictures picked, two slots");

        // §1 item 3: Make base on the second slot moves it to the front.
        real_click_centre(&button(&window, "image-make-base-1"));
        assert_eq!(
            frames(),
            vec![two[1].clone(), two[0].clone()],
            "a real click on Make base put the reference first (status \"{}\")",
            status(&window)
        );

        // Set Thumbnail: the base as it is, cropped on a worker.
        real_click_centre(&button(&window, "image-set-thumbnail-0"));
        common::settle_workers();
        common::settle();
        assert!(
            status(&window).starts_with("thumbnail taken from"),
            "a real click on Set Thumbnail took the picture: \"{}\"",
            status(&window)
        );

        // §1 item 7: a drag on the thumbnail asks for the words that fill the box; Save prints them.
        let plate = button(&window, "thumbnail-picture");
        let (w, h) = (f64::from(plate.width()), f64::from(plate.height()));
        real_drag(&plate, (w * 0.25, h * 0.3), (w * 0.75, h * 0.6), 1);
        let dialog = words_dialog(&window);
        let entry = common::find_in(dialog.upcast_ref(), "words-dialog-entry")
            .expect("the dialog's text box");
        real_click_centre(&entry);
        for key in ["h", "i"] {
            real_key(&dialog, key);
        }
        real_click_centre(&labelled(dialog.upcast_ref(), "Save").expect("Save"));
        drain(Duration::from_millis(50), Duration::from_millis(400));
        let texts = ui::produce_page::read_state().texts;
        assert!(
            texts.len() == 1 && texts[0].text.contains("hi"),
            "the dragged box holds the typed words: {texts:?}"
        );

        // ✎ on that box rewords it or takes it away: Remove takes it away.
        real_click_centre(&button(&window, "words-chip-0"));
        let dialog = words_dialog(&window);
        real_click_centre(&labelled(dialog.upcast_ref(), "Remove").expect("Remove"));
        drain(Duration::from_millis(50), Duration::from_millis(400));
        assert!(
            ui::produce_page::read_state().texts.is_empty(),
            "Remove took the box away"
        );

        // Change… raises the chooser for that slot; remove drops the second slot.
        let asked = ui::produce_choosers::asks_for_test("image");
        real_click_centre(&button(&window, "image-change-0"));
        assert_eq!(
            ui::produce_choosers::asks_for_test("image"),
            asked + 1,
            "a real click on Change… raised the chooser"
        );
        escape_from(&window, "the Change… chooser");
        real_click_centre(&button(&window, "image-remove-1"));
        assert_eq!(
            frames(),
            vec![two[1].clone()],
            "remove dropped the second slot"
        );

        // §1 items 6 and 10: ⤓ export and ⤓ save each ask where first.
        for (name, which) in [
            ("thumbnail-export", "export"),
            ("transcode-save", "save-video"),
        ] {
            let asked = ui::produce_choosers::asks_for_test(which);
            real_click_centre(&button(&window, name));
            assert_eq!(
                ui::produce_choosers::asks_for_test(which),
                asked + 1,
                "a real click on {name} raised its save dialog"
            );
            escape_from(&window, which);
        }

        // ↻ over the thumbnail and ↻ beside Title walk on a worker and land their own sentence; nobody answers here.
        for name in ["thumbnail-redraw", "title-suggest"] {
            let before = status(&window);
            let pressed = button(&window, name);
            real_click_centre(&pressed);
            common::settle_workers();
            common::settle();
            assert!(
                status(&window) != before && pressed.is_sensitive(),
                "a real click on {name} walked and landed: \"{}\"",
                status(&window)
            );
        }

        // F5.1 and §03-shell §2: ▶ opens the run, ⏹ stops it between two of the encoder's steps. Pressed before
        // ↻ Transcode, whose finished video would put S2's Overwrite question in front of the run.
        naivepost::produce_translate::set_spawn_for_test(spawn_after(Duration::from_millis(700)));
        let play = ui::play_button(&window).expect("the run bar's \u{25b6}");
        real_click_centre(&play);
        assert_eq!(
            ui::running_step(&window),
            Some(naivepost::run::Step::Produce),
            "a real click on \u{25b6} opened the Produce run (status \"{}\")",
            status(&window)
        );
        let stop = ui::stop_button(&window).expect("the run bar's \u{23f9}");
        real_click_centre(&stop);
        assert!(
            ui::background::settle_for_test(Duration::from_secs(60)),
            "the render's worker answered"
        );
        common::settle();
        assert!(
            ui::running_step(&window).is_none()
                && status(&window) == naivepost::produce_render::stopped_log(),
            "a real click on \u{23f9} stopped the run: \"{}\"",
            status(&window)
        );

        // §1 item 10's ↻ encodes again, on the scripted encoder.
        naivepost::produce_translate::set_spawn_for_test(spawn_after(Duration::ZERO));
        let before = status(&window);
        real_click_centre(&button(&window, "transcode-again"));
        common::settle_workers();
        common::settle();
        assert!(
            status(&window) != before,
            "a real click on ↻ Transcode encoded: \"{}\"",
            status(&window)
        );

        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}

/// The scripted encoder: each command's output appears on disk after `wait`.
fn spawn_after(wait: Duration) -> naivepost::produce_translate::SpawnSlot {
    std::sync::Arc::new(move |command: &naivepost::produce_exec::Command| {
        std::thread::sleep(wait);
        if let Some(out) = command.argv.last() {
            let path = Path::new(out);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, b"a rendered file of some length");
        }
        Ok(())
    })
}
