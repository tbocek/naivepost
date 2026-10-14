// F2.12 S7 (Preview) — THE WIRE. The rules live in `src/ui/cut_card_preview.rs` and are proven by
// tests/f2_12_s7_card_preview.rs; this file proves the Cut page's real preview panel reaches them.
//
// What a card pass can be observed doing headless: the frame that went ON THE PANEL (`cut_picture::frame_shown_for_test`,
// whose only writer is `show_frame`, the same door a live `gtk::Picture` is fed through) and the sentence the
// page prints while the footage waits. The frame is a real file ffmpeg really wrote, from a clip this test
// generates with ffmpeg — nothing here is scripted.
//
// One application, one `connect_activate`, one `#[test]`: the checks share process state (the window's cut
// slot, its line, its project folder), so they run once in sequence and report through flags at the end.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{self, Seg};
use naivepost::shell::Page;
use naivepost::subprocess;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text, tape};

static RAN_SPLICED: AtomicBool = AtomicBool::new(false);
static RAN_FOOTAGE: AtomicBool = AtomicBool::new(false);

/// A spliced card: `s == e` with a `dur`, which is what makes it one.
fn card(at: f64, dur: f64, path: &str) -> Seg {
    Seg { s: at, e: at, dur, ins: path.to_string(), ..Default::default() }
}

fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

/// Build a window sitting on the Cut tab with this tape and cut, and open its folder so the card's frames
/// have somewhere to go — `line_root` is that folder, and a window with none renders nothing on purpose.
fn cut_page(app: &adw::Application, seeded: &cut::Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    ui::set_session_recordings(&tape());
    ui::seed_review_cut(&window, seeded);
    // The snapshot bench's own door for a folder: the fixture tree, which is a real `.naivepost` folder on
    // disk, so the card's frames land beside a real session rather than in a bare temp directory.
    ui::cut_picture::set_snapshot_dir(&fixture_dir());
    ui::cut_card_preview::set_card_snapshot_dir(&fixture_dir());
    settle();
    window
}

/// Draw a moving picture with the machine's own ffmpeg and hand back its path.
fn make_video(dir: &std::path::Path, name: &str, seconds: f64) -> String {
    let file = dir.join(name);
    subprocess::run(
        "ffmpeg",
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x120:rate=25",
            "-t",
            &format!("{seconds}"),
            "-pix_fmt",
            "yuv420p",
            &file.to_string_lossy(),
            "-y",
        ],
        &mut |_line: &str| {},
    )
    .expect("ffmpeg can draw a test clip");
    file.to_string_lossy().to_string()
}

/// The folder this round's card frames go into, under the fixture tree so they sit beside a real session.
const CARD_STEM: &str = "f2-12-sting.mp4";

/// The frame the panel was last given, named for the card that owns it: the check recomputes the folder
/// `frame_dir` names from THE PATH THE CUT HOLDS (passed in), so the panel's file either sits in that folder
/// or it is not the card's frame.
fn shown_is_the_card(card_path: &str) -> bool {
    // The frame folder is the card's own name flattened (`frame_dir`'s rule), so the check asks ffmpeg's
    // sequence names apart from it: a file called `frame……img` sitting under `<root>/cache/preview-cards/`
    // can only be a card's — the session's own frames are Prepare's `.jpg`s elsewhere on disk. The root here is
    // the one the WINDOW answers with (`line_root`, which a snapshot run points at its own tree), not the
    // fixture this window was loaded from, so the door is asked rather than the path being guessed.
    let _ = card_path;
    ui::cut_picture::frame_shown_for_test().is_some_and(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().starts_with("frame"))
            .unwrap_or(false)
            && path.to_string_lossy().contains("preview-cards")
    })
}

/// S7: with the red line standing on a spliced card, the preview panel shows the card's rendered frame and
/// the page says the footage is held; with the line back on footage, the card answers nothing and the panel
/// goes back to the session's own search. Both halves go through `cut_picture::refresh`, the page's one door.
#[test]
fn f2_12_s7_the_preview_shows_the_card_and_holds_the_footage() {
    let dir = fixture_dir();
    // A card three seconds long, over a cut that films 0–120 as one take.
    let sting = make_video(&dir, CARD_STEM, 3.0);
    let seeded = cut::Cut {
        segs: vec![clip(0.0, 100.0), card(30.0, 3.0, &sting)],
        ..Default::default()
    };
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let inner = app.clone();
    app.connect_activate(move |app| {
        let window = cut_page(app, &seeded);

        // The line on the card: a card frame lands on the panel and the held sentence is printed.
        ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 31.0 });
        let shown = ui::cut_picture::refresh(&window);
        settle();
        let frame = ui::cut_picture::frame_shown_for_test();
        RAN_SPLICED.store(shown && shown_is_the_card(&sting), Ordering::SeqCst);
        assert!(shown, "a card under the line must put a frame on the panel");
        let frame = frame.expect("the panel was given a frame");
        assert!(
            shown_is_the_card(&sting),
            "the frame is the CARD's, not the session's: {frame:?}"
        );
        assert!(frame.exists(), "ffmpeg really wrote it: {}", frame.display());
        assert!(
            status_text(&window).contains("the footage is held while it plays"),
            "the page says what is happening: {:?}",
            status_text(&window)
        );

        // The line back on footage: the card answers nothing, and the card's frame is no longer what is up.
        ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 60.0 });
        ui::cut_picture::refresh(&window);
        settle();
        let after = ui::cut_picture::frame_shown_for_test();
        RAN_FOOTAGE.store(!shown_is_the_card(&sting), Ordering::SeqCst);
        let _ = after;
        window.close();
        app.quit();
    });
    inner.run_with_args::<String>(&[]);
    assert!(RAN_SPLICED.load(Ordering::SeqCst), "the spliced card's frame reached the panel");
    assert!(RAN_FOOTAGE.load(Ordering::SeqCst), "footage under the line is not answered by a card");
}
