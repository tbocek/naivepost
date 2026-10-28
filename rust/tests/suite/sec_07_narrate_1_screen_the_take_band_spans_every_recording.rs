//! §07-narrate#1-screen **10**, on a real page: a narrator filmed in two recordings gets a take band across what
//! the cut keeps of both, and a take picked in the second recording shows on it where that recording's kept
//! stretch lies, once the session's recordings are measured. The first rewrite drew the first tagged recording
//! alone, whole.

use std::process::Command;

use adw::prelude::*;

use naivepost::ui;

use crate::common;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
fn sec_07_narrate_1_screen_the_take_band_spans_what_the_cut_keeps_of_every_narrator_recording() {
    common::pin_private_cwd();
    let root = common::temp_dir_tagged("narrate-tape");
    let dir = root.join("tape.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    // Two 10 s recordings, the second made 20 s after the first, both the narrator's.
    for name in ["2026-10-25 10-00-00.mkv", "2026-10-25 10-00-20.mkv"] {
        let made = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x90:rate=4:duration=10",
            ])
            .args(["-f", "lavfi", "-i", "sine=duration=10", "-shortest"])
            .arg(dir.join("sources").join(name))
            .status()
            .expect("ffmpeg runs");
        assert!(made.success());
    }
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"sources": [
            {"path": "project:sources/2026-10-25 10-00-00.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []},
            {"path": "project:sources/2026-10-25 10-00-20.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}]}"#,
    )
    .unwrap();
    // The cut keeps 2–5 s of the first and 22–26 s of the session, which is 2–6 s of the second: a 7 s tape.
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 2.0, "e": 5.0, "cam": 0}, {"s": 22.0, "e": 26.0, "cam": 0}]}"#,
    )
    .unwrap();
    // A take in the second recording's own seconds: 3–5 s of it is 4–6 s of the band.
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    naivepost::narrate_data::save_takes(
        &tree,
        "2026-10-25 10-00-20",
        &[naivepost::narrate_data::Take { s: 3.0, e: 5.0 }],
    )
    .unwrap();
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("loads");
        naivepost::startup::set_open_dir(&dir);
        let window = ui::window::build_window_at(app, &loaded, "Narrate", &dir);
        window.set_default_size(1280, 900);
        window.present();
        common::wait_for_stable_layout(&window);
        common::wait_until(
            "the band spans the 7 s the cut keeps of both recordings",
            || {
                common::settle_workers();
                ui::narrate_band::painted_seconds()
                    .is_some_and(|seconds| (seconds - 7.0).abs() < 0.05)
            },
        );
        common::wait_until(
            "the second recording's take shows at 4–6 s of the band",
            || {
                common::settle();
                let takes = ui::narrate_page::read_state().takes;
                takes.len() == 1
                    && (takes[0].0 - 4.0).abs() < 0.05
                    && (takes[0].1 - 6.0).abs() < 0.05
            },
        );
        window.close();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the round never ran"
    );
}
