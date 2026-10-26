//! §05-cut §1 Pictures, "a line move and a playback tick never wait" (§03-shell §1): the frame at the line is
//! decoded on a worker and goes up when it lands, so the move returns with the old picture still up; and the
//! band's film rows paint from the held listing, so a paint lists no frame folder on the window's thread. The
//! first rewrite decoded up to three full frames inside every line move and listed every recording's folder on
//! every band paint.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A real PNG under a frame's stamped `.jpg` name, as `sec05_cut_preview_picture` writes one.
fn write_frame(tree: &naivepost::layout::Tree, base: &str, second: f64) -> std::path::PathBuf {
    let dir = tree.frames_dir(base);
    std::fs::create_dir_all(&dir).expect("frames folder");
    let path = dir.join(format!("{}.jpg", naivepost::clock::frame_name(second)));
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 96, 54).expect("a surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        cr.set_source_rgb(0.1, 0.4, 0.8);
        let _ = cr.paint();
    }
    let mut file = std::fs::File::create(&path).expect("the frame file");
    surface.write_to_png(&mut file).expect("a real PNG written");
    path
}

fn landed() {
    assert!(
        ui::background::settle_for_test(std::time::Duration::from_secs(60)),
        "the picture workers answered"
    );
    settle();
}

fn round(app: &adw::Application) {
    release_last_window();
    let root = std::env::temp_dir().join(format!("cut-never-wait-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");
    let session = naivepost::startup::session_dir(&root);
    let mut seed = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    seed.sources
        .iter_mut()
        .for_each(|source| source.footage = true);
    std::fs::create_dir_all(&session).expect("session folder");
    naivepost::project::save(&seed, &session).expect("the session is a project");
    let tree = naivepost::layout::Tree::new(&session).expect("a project folder");
    let window = ui::build_window(app, &seed, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    settle();
    ui::set_session_recordings(&[naivepost::timeline::Recording {
        base: "lecture".into(),
        start: 0.0,
        end: 120.0,
    }]);
    ui::seed_review_cut(
        &window,
        &cut::Cut {
            segs: vec![Seg {
                s: 0.0,
                e: 60.0,
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    // A frame's name is an instant on its file's clock and the folder's first frame is second 0 of the file.
    write_frame(&tree, "lecture", 0.0);
    let ten = write_frame(&tree, "lecture", 10.0);
    let twenty = write_frame(&tree, "lecture", 20.0);

    // The line on 10 s: once the listing and the decode landed, that frame is up.
    ui::cut_picture::note_line_moved(&window, 10.0);
    landed();
    assert_eq!(
        ui::cut_picture::frame_shown_for_test().as_deref(),
        Some(ten.as_path()),
        "the frame at 10 s went up once it was decoded"
    );

    // The move to 20 s returns with 10 s still up and 20 s not decoded: the decode is a worker's.
    ui::cut_picture::note_line_moved(&window, 20.0);
    assert!(
        !ui::cut_picture_cache::picture_held_for_test(&twenty),
        "the move decoded nothing itself"
    );
    assert_eq!(
        ui::cut_picture::frame_shown_for_test().as_deref(),
        Some(ten.as_path()),
        "the picture that was up stays up until the wanted one lands"
    );
    landed();
    assert_eq!(
        ui::cut_picture::frame_shown_for_test().as_deref(),
        Some(twenty.as_path()),
        "and the wanted frame goes up when it lands"
    );

    // A band paint lists no folder on this thread: it paints from the listing the worker left.
    let band = common::find_named(&window, "track-band")
        .expect("the band is on the page")
        .downcast::<gtk::DrawingArea>()
        .expect("the band is a drawing area");
    let listed = ui::cut_picture_cache::listed_here_for_test();
    // Painted for real: a queued draw runs on the window's frame clock, so the loop is turned for a while.
    let context = glib::MainContext::default();
    for _ in 0..3 {
        band.queue_draw();
        let until = std::time::Instant::now() + std::time::Duration::from_millis(300);
        while std::time::Instant::now() < until {
            if !context.iteration(false) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
    assert_eq!(
        ui::cut_picture_cache::listed_here_for_test(),
        listed,
        "a band paint listed a frame folder on the window's thread"
    );
    window.close();
}

#[test]
fn sec_05_cut_1_screen_pictures_never_wait() {
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        round(app);
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the picture round never ran inside activate"
    );
}
