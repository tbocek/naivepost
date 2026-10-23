//! F5.7 S6/S7 — the two presses that write a FILE: ⤓ export and ⤓ Save video.
//!
//! The other F5.7 file (`produce_presses_widgets.rs`) proves the walks and the sentences. This one proves
//! what lands on disk, which is the half the spec is actually about:
//!
//! - **S6** the ladder stops at the FIRST rung of 92/85/75/60/40 that fits 2 MiB, so the JPEG left on disk is
//!   that rung's own encode (not quality 40, which is what an all-five ladder always left); when nothing fits
//!   every rung is tried and the last attempt is still written, with [`naivepost::produce_runs::jpeg_target`]'s
//!   reason in the log rather than the file thrown away; the name is `<project>-thumbnail.jpg` with the
//!   extension forced, which is what the button's own save dialog would be opened on.
//! - **S7** the copy goes to the path the save door answered (the chooser's seam — `cut_svg`'s stand-in
//!   pattern), and with nobody answering it goes under that door's own offer, `<project>.<container>`.
//!
//! Real tools, no scripted bytes: ffmpeg is in this container, so the ladder really encodes and the copy really
//! copies. Only the LLM/sd.cpp legs are scripted (`script_legs` in the other file), and none of them run here.
//!
//! One `adw::Application::run` for the whole binary (a second one from another thread aborts with "Default
//! main context is already acquired by another thread!"), checks back to back inside `activate`, each failure
//! parked in its own slot and read back by the `#[test]` that names it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::common;
use adw::prelude::*;
use common::{
    fixture_dir, hold_last_window, pin_private_cwd, release_last_window,
    session_tree_of as session, settle,
};
use naivepost::cut::Cut;
use naivepost::project::Container;
use naivepost::shell::Page;
use naivepost::{produce_runs as runs, ui};

/// One slot per check, holding that block's failure text.
static NAME_FAIL: Mutex<Option<String>> = Mutex::new(None);
static LADDER_FAIL: Mutex<Option<String>> = Mutex::new(None);
static OVER_FAIL: Mutex<Option<String>> = Mutex::new(None);
static EXPORT_BUTTON_FAIL: Mutex<Option<String>> = Mutex::new(None);
static SAVE_DOOR_FAIL: Mutex<Option<String>> = Mutex::new(None);
static SAVE_DEFAULT_FAIL: Mutex<Option<String>> = Mutex::new(None);

thread_local! {
    static CURRENT_APP: std::cell::RefCell<Option<adw::Application>> =
        const { std::cell::RefCell::new(None) };
}

fn app_in_round() -> adw::Application {
    CURRENT_APP
        .with(|held| held.borrow().clone())
        .expect("run_round published the application")
}

/// Park a check's failure so the `#[test]` that owns it can report it (an assert unwinding inside GTK's
/// `extern "C"` trampoline aborts the process instead of failing a test).
fn record(slot: &Mutex<Option<String>>, label: &str, body: impl FnOnce() + std::panic::UnwindSafe) {
    let failure = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(()) => None,
        Err(payload) => Some(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string()),
        ),
    };
    let text = failure.map(|why| format!("{label}: {why}"));
    if let Some(previous) = slot.lock().expect("failure slot").take() {
        eprintln!("earlier failure in this slot: {previous}");
    }
    *slot.lock().expect("failure slot") = text;
}

/// cwd in its own folder, so each check's `session.naivepost` is its own.
fn in_root(name: &str) -> PathBuf {
    let base = std::env::current_dir().expect("cwd pinned before the run");
    let root = base.join(name);
    std::fs::create_dir_all(&root).expect("check root");
    std::env::set_current_dir(&root).expect("cwd to the check root");
    root
}

fn produce_window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Produce.label());
    hold_last_window(window.clone());
    window.present();
    settle();
    window
}

/// A cut so no press refuses for want of one, and the row at its §A defaults.
fn seed_live_cut(window: &adw::ApplicationWindow) {
    let first = naivepost::cut::Seg {
        s: 0.0,
        e: 9.5,
        ..Default::default()
    };
    let second = naivepost::cut::Seg {
        s: 10.0,
        e: 30.0,
        ..Default::default()
    };
    ui::seed_review_cut(
        window,
        &Cut {
            segs: vec![first, second],
            aspect: "16:9".to_string(),
            ..Default::default()
        },
    );
    let row = naivepost::produce_screen::defaults();
    let mut page = ui::produce_page::read_state();
    page.container = row[0].1.to_string();
    page.codec = row[1].1.to_string();
    page.preset = row[2].1.to_string();
    ui::produce_page::set_state(page);
}

/// Fire one named button and read the status line back — the same seam `produce_presses_widgets.rs` presses.
fn press(window: &adw::ApplicationWindow, name: &str) -> String {
    let button = ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the page drew no widget named {name}"));
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    ui::find_status(window.upcast_ref())
        .expect("the window has a status line")
        .text()
        .to_string()
}

/// A PNG with real high-frequency content: a flat block compresses to a few hundred bytes at every quality,
/// which would make "which rung did the ladder stop at?" unanswerable from the file.
fn noisy_png(path: &Path, wide: u32, high: u32) {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, wide as i32, high as i32)
        .expect("surface");
    {
        let context = cairo::Context::new(&surface).expect("context");
        let mut seed: u32 = 0x2545_F491;
        for y in (0..high as i32).step_by(2) {
            for x in (0..wide as i32).step_by(2) {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let shade = ((seed >> 16) & 0xff) as f64 / 255.0;
                context.set_source_rgb(shade, 1.0 - shade, (shade * 0.5) % 1.0);
                context.rectangle(f64::from(x), f64::from(y), 2.0, 2.0);
                let _ = context.fill();
            }
        }
    }
    let mut file = std::fs::File::create(path).expect("the picture is writable");
    surface.write_to_png(&mut file).expect("cairo writes a PNG");
}

/// ffmpeg pointed at a counter: every call appends one line to `log`, then runs the real ffmpeg. So the ladder
/// really encodes AND the test counts how many rungs it asked for.
fn count_ffmpeg_calls(root: &Path, log: &Path) {
    let stub = root.join("counting-ffmpeg.sh");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\necho \"$1 $2 $3\" >> {}\nexec ffmpeg \"$@\"\n",
            log.display()
        ),
    )
    .expect("the stub is written");
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("the stub runs");
    std::env::set_var("NAIVEPOST_FFMPEG", &stub);
}

/// How many rungs the counting stub was asked for.
fn rung_count(log: &Path) -> usize {
    std::fs::read_to_string(log)
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

/// Pull the byte count out of `>>> exported <path> (X.Y MiB)`, rounding the printed MiB back into bytes.
#[allow(dead_code)]
fn quoted_bytes(line: &str) -> u64 {
    let inside = line.rsplit('(').next().expect("the line carries a weight");
    let inside = inside
        .strip_suffix(')')
        .expect("the weight closes its bracket");
    let mib: f64 = inside
        .trim_end_matches(" MiB")
        .trim()
        .parse()
        .unwrap_or_else(|why| panic!("`{inside}` is a MiB count: {why}"));
    (mib * 1024.0 * 1024.0) as u64
}

// ---- S6: the ladder, on disk ------------------------------------------------------

/// The rung rule on its own terms: `force_suffix` replaces whatever extension a dialog was typed with, and
/// the export's default name is the project's own.
fn check_names() {
    assert_eq!(runs::force_suffix("pic.png", ".jpg"), "pic.jpg");
    assert_eq!(
        runs::force_suffix("pic", ".jpg"),
        "pic.jpg",
        "no extension to replace"
    );
    assert_eq!(
        runs::force_suffix("final.v2.png", ".jpg"),
        "final.v2.jpg",
        "only the last dot is an extension"
    );
    assert_eq!(runs::export_default("demo"), "demo-thumbnail.jpg");
}

/// The first fitting rung ENDS the ladder: one encode, and the file on disk weighs exactly the number the
/// ladder returned. An all-five ladder would have left quality 40 here and reported its size.
fn check_ladder_stops(root: &Path) {
    let tree = session(root);
    std::fs::create_dir_all(tree.publish_dir()).expect("publish folder");
    noisy_png(&tree.thumbnail_png(), 640, 360);
    let log = root.join("rung-calls.log");
    std::fs::remove_file(&log).ok();
    count_ffmpeg_calls(root, &log);

    let out = root.join("one-rung.jpg");
    let sizes = naivepost::produce_upload_media::jpeg_ladder(
        &tree.thumbnail_png(),
        &out,
        &runs::JPEG_QUALITIES,
        |_| (),
    );
    let written = std::fs::metadata(&out).expect("a JPEG was written").len();
    assert_eq!(
        sizes.last(),
        Some(&written),
        "the LAST weight the ladder returned is the file that stands: {sizes:?} vs {written}"
    );
    assert_eq!(
        sizes.len(),
        rung_count(&log),
        "one encode per rung the ladder returned — it stops at the first that fits"
    );
    assert!(
        written <= runs::JPEG_MAX_BYTES,
        "the rung it stopped on fits 2 MiB: {written} bytes"
    );
    assert_eq!(
        runs::jpeg_rung(&sizes),
        Some(sizes.len() - 1),
        "and that rung is the one chosen"
    );
    std::env::remove_var("NAIVEPOST_FFMPEG");
}

/// Nothing fits: every rung is tried, the last attempt is STILL written (spec: "last attempt written even if
/// it still doesn't fit", never rescaled), and `jpeg_target`'s reason is what the log carries. A stub ffmpeg
/// writes a 3 MiB file whatever the quality, so no rung can fit.
fn check_last_attempt_written(root: &Path) {
    let tree = session(root);
    std::fs::create_dir_all(tree.publish_dir()).expect("publish folder");
    noisy_png(&tree.thumbnail_png(), 640, 360);
    let fat = root.join("fat.jpg");
    std::fs::write(&fat, vec![7u8; 3 * 1024 * 1024]).expect("a fat JPEG to hand back");
    let stub = root.join("fat-ffmpeg.sh");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nlast=\"\"\nfor a in \"$@\"; do last=\"$a\"; done\ncp {} \"$last\"\n",
            fat.display()
        ),
    )
    .expect("the stub is written");
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("the stub runs");
    std::env::set_var("NAIVEPOST_FFMPEG", &stub);

    let log = root.join("over-calls.log");
    std::fs::remove_file(&log).ok();
    let out = root.join("over.jpg");
    let sizes = naivepost::produce_upload_media::jpeg_ladder(
        &tree.thumbnail_png(),
        &out,
        &runs::JPEG_QUALITIES,
        |_| (),
    );
    assert_eq!(
        sizes.len(),
        runs::JPEG_QUALITIES.len(),
        "nothing fits, so every rung is tried: {sizes:?}"
    );
    let written = std::fs::metadata(&out)
        .expect("the last attempt is still written")
        .len();
    assert!(
        written > runs::JPEG_MAX_BYTES,
        "and it is the over-cap file: {written}"
    );
    assert_eq!(
        runs::jpeg_rung(&sizes),
        Some(sizes.len() - 1),
        "the last attempt is the chosen rung"
    );
    let reason = runs::jpeg_target(written).expect_err("over the cap, so the bound has a reason");
    assert!(
        reason.contains("2 MiB after 92/85/75"),
        "the reason names the rungs the parameter row lists: {reason}"
    );
    std::env::remove_var("NAIVEPOST_FFMPEG");
}

/// The export's own door, through the module that owns it: `export_path(None)` is what the press is handed
/// when nobody answered the dialog, and it must be the §F5.7 name — `<project>-thumbnail.jpg` in the produce
/// folder, extension forced — not the `<video>` tag's `produce/final.jpg`.
fn check_export_button(_root: &Path) {
    let offered = ui::produce_press_wiring::offered_export_name();
    assert!(
        offered.ends_with("session-thumbnail.jpg"),
        "named after the project the session folder is: {offered}"
    );
    assert!(offered.contains("/produce/"), "beside the video: {offered}");
    // A dialog that typed `pic.png` still gets a JPEG — the extension is forced, not suggested.
    assert!(
        ui::produce_press_wiring::export_path(Some("/tmp/pic.png")).ends_with("pic.jpg"),
        "a typed name keeps its path and takes the forced extension"
    );
    // The refusal still comes first: with no picture on disk the button names both ways out.
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let said = press(&window, "thumbnail-export");
    assert_eq!(
        said,
        runs::EXPORT_NO_PICTURE,
        "no picture, its own sentence: {said:?}"
    );
    window.close();
    settle();
}

// ---- S7: the copy goes where the door says ---------------------------------------

/// The copy's destination is the path the save door answered, so the copy lands OUTSIDE the project, where
/// a person actually points a save dialog. The door is the chooser's seam; the page's Change… stand-in is the
/// same pattern (`produce_slots::next_stood_in`).
fn check_save_uses_the_door(root: &Path) {
    let tree = session(root);
    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).expect("produce folder");
    std::fs::write(&video, vec![3u8; 9000]).expect("a rendered video to copy");
    let chosen = root.join("elsewhere/my-cut.mp4");
    std::fs::create_dir_all(chosen.parent().unwrap()).expect("the chosen folder");
    ui::produce_press_wiring::answer_path(Some(chosen.to_string_lossy().as_ref()));

    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let said = ui::produce_page::press_save_video_from_button(&window);
    assert!(said.starts_with("saved "), "the confirm line: {said:?}");
    assert!(
        said.contains(chosen.to_string_lossy().as_ref()),
        "the copy went to the chosen path, not to the project's own: {said:?}"
    );
    let copy = std::fs::metadata(&chosen).expect("the copy exists where it was told");
    assert_eq!(copy.len(), 9000, "and it is the render's bytes");
    assert!(
        video.is_file(),
        "the render's own output stays where ▶ wrote it"
    );
    ui::produce_press_wiring::answer_path(None);
    window.close();
    settle();
}

/// Nobody answered the dialog: the copy still goes somewhere, under the door's own offer —
/// `<project>.<container>` for whatever the Container row says.
fn check_save_default_name(root: &Path) {
    let tree = session(root);
    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).expect("produce folder");
    std::fs::write(&video, vec![5u8; 4096]).expect("a rendered video to copy");
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    ui::produce_press_wiring::answer_path(None);

    assert_eq!(
        ui::produce_press_wiring::offered_save_name(),
        runs::save_default("session", Container::Mp4),
        "the offer is the project name and the row's container"
    );
    let said = ui::produce_page::press_save_video_from_button(&window);
    assert!(said.starts_with("saved "), "{said:?}");
    assert!(
        said.contains("session.mp4"),
        "named after the project: {said:?}"
    );
    let landed = tree.produce_dir().join("session.mp4");
    assert!(
        landed.is_file(),
        "under the offer, in the produce folder: {said:?}"
    );
    window.close();
    settle();
}

/// The checks, back to back on the one thread the application owns.
fn run_round(app: &adw::Application) {
    CURRENT_APP.with(|held| *held.borrow_mut() = Some(app.clone()));
    record(&NAME_FAIL, "f5_7_s6 names", check_names);
    record(
        &LADDER_FAIL,
        "f5_7_s6 ladder stops at the first fitting rung",
        std::panic::AssertUnwindSafe(|| check_ladder_stops(&in_root("ladder-stop"))),
    );
    record(
        &OVER_FAIL,
        "f5_7_s6 last attempt written",
        std::panic::AssertUnwindSafe(|| check_last_attempt_written(&in_root("ladder-over"))),
    );
    record(
        &EXPORT_BUTTON_FAIL,
        "f5_7_s6 export through the button",
        std::panic::AssertUnwindSafe(|| check_export_button(&in_root("export-button"))),
    );
    record(
        &SAVE_DOOR_FAIL,
        "f5_7_s7 save uses the door",
        std::panic::AssertUnwindSafe(|| check_save_uses_the_door(&in_root("save-door"))),
    );
    record(
        &SAVE_DEFAULT_FAIL,
        "f5_7_s7 save default name",
        std::panic::AssertUnwindSafe(|| check_save_default_name(&in_root("save-default"))),
    );
}

/// Take one slot's failure and panic with it, so the failing clause names itself.
fn report(slot: &Mutex<Option<String>>) {
    if let Some(why) = slot.lock().expect("failure slot").take() {
        panic!("{why}");
    }
}

#[test]
fn f5_7_s6_s7_paths_and_ladder_on_disk() {
    pin_private_cwd();
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        app.run_with_args::<&str>(&[]);
    });
    for slot in [
        &NAME_FAIL,
        &LADDER_FAIL,
        &OVER_FAIL,
        &EXPORT_BUTTON_FAIL,
        &SAVE_DOOR_FAIL,
        &SAVE_DEFAULT_FAIL,
    ] {
        report(slot);
    }
}
