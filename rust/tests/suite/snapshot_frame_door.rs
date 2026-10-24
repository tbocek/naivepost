//! The staging door a shot plants its own frames through, and the door the band lists them from.
//!
//! The mismatch that broke §05-cut#1-screen once: `just snapshot` staged into one folder while the band listed
//! frames from another, so the picture row silently had nothing to draw and the shot still came out green. This
//! check plants a frame through the staging door (`snapshot_lane::stage_frame`, what `write_png` uses) and reads it
//! back through the door the band asks (`cut_picture::snapshot_frame_tree` + `cut_tracks::frame_paths`) — the two
//! only agree if both name the same folder. No display needed; `set_snapshot_dir` is thread-local, so this stays a
//! plain `#[test]`.

use naivepost::snapshot_lane;
use naivepost::ui::cut_tracks;

/// Each check gets its own temp dir: the suite runs files in parallel and both doors are process/thread state.
fn temp_root(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("np-framedoor-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp root created");
    dir
}

/// F1.6 / §1: whatever the shot staged for itself is what the band then lists.
#[test]
fn f1_6_snapshot_staging_lists_what_it_staged() {
    let dir = temp_root("frame-door");
    let planted = snapshot_lane::stage_frame(&dir, "session-tape", 0.0).expect("a frame staged");
    assert!(planted.is_file(), "{} was not written", planted.display());
    naivepost::ui::cut_picture::set_snapshot_dir(&dir);
    let tree =
        naivepost::ui::cut_picture::snapshot_frame_tree().expect("the band's frame door resolves");
    let listed = cut_tracks::frame_paths(&tree, "session-tape");
    assert!(
        listed
            .iter()
            .any(|path| path.file_name() == planted.file_name()),
        "the band's door does not list the staged {}",
        planted.display()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
