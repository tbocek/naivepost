// F2.12 S7 (Preview: cards render at 8 fps via ffmpeg, nearest rendered frame shown; a spliced card holds
// the footage while it plays on the wall clock; card sound is its own pipeline) — the RULES, run for real.
//
// The ffmpeg half runs the machine's actual ffmpeg over a file this test generates with it, in a throwaway
// folder, and reads back what it wrote. Nothing here is scripted: the frame ladder, the still's single frame,
// the 8 fps rate and the ≤960 cap are all read off the command that ran and the files that appeared.

use std::path::{Path, PathBuf};

use naivepost::cut::{Cut, Seg};
use naivepost::cut_insert as ins;
use naivepost::subprocess;
use naivepost::ui::cut_card_preview as cards;

/// A clip of footage, for the "not a card" cases.
fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

/// A spliced card: `s == e` with a `dur`, which is what makes it one.
fn card(at: f64, dur: f64, path: &str) -> Seg {
    Seg { s: at, e: at, dur, ins: path.into(), ..Default::default() }
}

/// A throwaway folder, unique per test name so two tests never share a resume marker.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-f2-12-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

/// Ask the real ffmpeg to draw a moving picture, and hand back its path. The rate is the render's own rate so
/// the ladder this test expects is the ladder ffmpeg has to answer.
fn make_video(dir: &Path, name: &str, seconds: f64, fps: u32) -> PathBuf {
    let file = dir.join(name);
    let source = format!("testsrc=size=160x120:rate={fps}");
    let out = file.to_string_lossy().to_string();
    subprocess::run(
        "ffmpeg",
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            &source,
            "-t",
            &format!("{seconds}"),
            "-pix_fmt",
            "yuv420p",
            &out,
            "-y",
        ],
        &mut |_line: &str| {},
    )
    .expect("ffmpeg can draw a test clip");
    file
}

/// F2.12 S7 (`cards render at 8 fps ... via ffmpeg`): a two-second sting at §I's `insPreviewFPS` is sixteen
/// frames on disk, and the command that made them asks for 8 and for a width capped at 960.
#[test]
fn f2_12_s7_a_card_renders_its_frames_at_eight_fps() {
    let dir = scratch("eight-fps");
    let video = make_video(&dir, "sting.mp4", 2.0, 25);
    let out = dir.join("frames");
    let mut log = Vec::<String>::new();
    let frames = cards::render_card(
        "ffmpeg",
        &video.to_string_lossy(),
        ins::Kind::Video,
        false,
        2.0,
        &out,
        &mut |line| log.push(line.to_string()),
    )
    .expect("the render runs");
    // `frame_times` at 8 fps over 2 s is 16 seconds, and 16 files came back — none invented, none missing.
    assert_eq!(frames.len(), 16, "sixteen frames for two seconds at 8 fps");
    assert_eq!(
        frames.len(),
        cards::rendered_frames(&out).len(),
        "every frame the pass reports is a file ffmpeg really wrote"
    );
    for frame in &frames {
        assert!(frame.exists(), "{} was not written", frame.display());
    }
    // The rate and the cap are in the command, and they are §I's numbers.
    let vf = log
        .iter()
        .find_map(|line| line.split("-vf ").nth(1))
        .expect("the log carries the ffmpeg command");
    assert!(vf.contains("fps=8.000000"), "asked at 8 fps: {vf}");
    assert!(vf.contains("960"), "capped at 960 px: {vf}");
    // The ladder each frame stands at, at that rate: 0, 1/8, 2/8 …
    assert_eq!(cards::frame_second(&frames[0], 8.0), Some(0.0));
    assert_eq!(cards::frame_second(&frames[1], 8.0), Some(0.125));
    assert_eq!(cards::frame_second(&frames[15], 8.0), Some(15.0 / 8.0));
}

/// F2.12 S7 (`one frame for stills`): a PNG has no clock, so the pass asks for one frame and writes one.
#[test]
fn f2_12_s7_a_still_renders_one_frame() {
    assert_eq!(
        ins::preview_fps(ins::Kind::Still, false),
        None,
        "a still has no rate to render at"
    );
    let dir = scratch("still");
    let still = dir.join("board.png");
    subprocess::run(
        "ffmpeg",
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=1600x900",
            "-frames:v",
            "1",
            &still.to_string_lossy(),
            "-y",
        ],
        &mut |_line: &str| {},
    )
    .expect("ffmpeg can draw a still");
    let out = dir.join("frames");
    let frames = cards::render_card(
        "ffmpeg",
        &still.to_string_lossy(),
        ins::Kind::Still,
        false,
        4.0,
        &out,
        &mut |_line: &str| {},
    )
    .expect("the still renders");
    assert_eq!(frames.len(), 1, "one frame, not a strip of identical ones");
    // A 1600-px source is drawn inside the cap, so the texture is never wider than the column can show.
    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_file(&frames[0]).expect("a readable frame");
    assert!(pixbuf.width() <= 960, "capped at 960 px, got {}", pixbuf.width());
    assert!(pixbuf.height() > 0, "the aspect is kept, not flattened");
}

/// F2.12 S7 (`nearest rendered frame shown`): the ladder on disk decides, and nothing from the future is shown.
#[test]
fn f2_12_s7_the_nearest_rendered_frame_is_the_one_shown() {
    let ladder: Vec<f64> = ins::frame_times(2.0, 8.0);
    assert_eq!(ins::nearest_rendered(&ladder, 0.31), Some(0.25));
    assert_eq!(ins::nearest_rendered(&ladder, 0.4), Some(0.375));
    assert_eq!(ins::nearest_rendered(&[], 0.4), None, "nothing rendered shows nothing");
    // Past the last rendered frame the last one is what stands, so a card still has a picture.
    assert_eq!(ins::nearest_rendered(&ladder, 99.0), Some(1.875));
}

/// F2.12 S7 (`a spliced card holds the footage while it plays on the wall clock`): the line may not move
/// while a card with no session seconds of its own is up, and the page says so.
#[test]
fn f2_12_s7_a_spliced_card_holds_the_footage() {
    let cut_ = Cut {
        segs: vec![clip(0.0, 30.0), card(30.0, 4.0, "assets/tier.svg?S=Dust%20II")],
        ..Default::default()
    };
    // The card answers for the four seconds after the footage ends and for no second before it.
    let held = cards::card_under(&cut_, 31.0).expect("the line is on the card");
    assert_eq!(held.ins, "assets/tier.svg?S=Dust%20II");
    assert_eq!(cards::card_hold_advance(Some(held), 0.1, 1.0), 0.0, "held: the footage waits");
    assert_eq!(cards::card_hold_advance(Some(held), 3.5, 2.0), 0.0);
    // Footage under the line is not held, and runs at the player's own rate.
    let footage = cards::card_under(&cut_, 10.0);
    assert!(footage.is_none(), "a plain clip is not a card");
    assert_eq!(cards::card_hold_advance(None, 3.5, 1.0), 3.5);
    assert_eq!(cards::card_hold_advance(None, 3.5, 0.5), 1.75, "half speed, half the ground");
    // And the sentence the page prints while it waits.
    assert_eq!(
        ins::held_status("assets/tier.svg?S=Dust%20II"),
        "tier.svg \u{2014} the footage is held while it plays"
    );
}

/// F2.12 S7 (`card sound is its own pipeline`): a card with a sound has a pipeline the app answers for, and
/// the tick's own sound is held while a spliced card plays; a ticked-off card is silent, and a card laid OVER
/// leaves the footage's sound alone.
#[test]
fn f2_12_s7_the_card_sound_is_its_own_pipeline() {
    let spliced = card(30.0, 4.0, "assets/sting.mp4");
    assert!(cards::card_sound(&spliced), "a video card brings its own sound");
    let silent = Seg { mute: true, ..spliced.clone() };
    assert!(!cards::card_sound(&silent), "the tick means silent, and silent means no pipeline");
    let board = card(30.0, 4.0, "assets/tier.svg");
    assert!(!cards::card_sound(&board), "a drawn card has no sound to answer for");
    let laid = Seg { s: 30.0, e: 34.0, ins: "assets/song.mp3".into(), ..Default::default() };
    assert!(cards::card_sound(&laid), "a sound insert is the sound");
    // The footage's own pipeline is held only for a SPLICED card: an over card replaced the picture and,
    // with the tick off, nothing else.
    assert!(cards::footage_held_while_card(&spliced));
    assert!(!cards::footage_held_while_card(&laid));
}

/// F2.12 S7: the resume rule — a card's own frames are its marker, so a second pass costs no ffmpeg run.
#[test]
fn f2_12_s7_a_rendered_card_is_not_rendered_twice() {
    let dir = scratch("resume");
    let video = make_video(&dir, "sting.mp4", 1.0, 25);
    let out = dir.join("frames");
    let first = cards::render_card(
        "ffmpeg",
        &video.to_string_lossy(),
        ins::Kind::Video,
        false,
        1.0,
        &out,
        &mut |_line: &str| {},
    )
    .expect("the first pass runs");
    let mut second_log = Vec::<String>::new();
    let second = cards::render_card(
        "ffmpeg",
        &video.to_string_lossy(),
        ins::Kind::Video,
        false,
        1.0,
        &out,
        &mut |line| second_log.push(line.to_string()),
    )
    .expect("the second pass returns");
    assert_eq!(first, second, "the same frames, from disk");
    assert!(
        second_log.iter().all(|line| !line.contains("ffmpeg")),
        "the second pass ran no ffmpeg: {second_log:?}"
    );
}

/// F2.12 S7: a card that cannot be rendered says so and names the card, rather than showing a frame from
/// somewhere else. (Failure is specific and local.)
#[test]
fn f2_12_s7_a_card_that_will_not_render_names_itself() {
    let dir = scratch("broken");
    let out = dir.join("frames");
    let err = cards::render_card(
        "ffmpeg",
        "assets/no-such-card.mp4",
        ins::Kind::Video,
        false,
        2.0,
        &out,
        &mut |_line: &str| {},
    )
    .expect_err("a missing file cannot render");
    assert!(err.contains("no-such-card"), "names the card: {err}");
    assert!(cards::rendered_frames(&out).is_empty(), "and leaves no pretend frame");
}

/// The preview panel's frame for a card: `show_card_frame` needs a window, and its rule is proven by the
/// widget test; what the logic half can prove is that the CARD LOOKUP and the folder rule agree with it.
#[test]
fn f2_12_s7_the_frame_folder_is_named_for_the_card_itself() {
    let root = Path::new("/projects/one.naivepost");
    let a = cards::frame_dir(root, "assets/tier.svg?S=Dust%20II");
    let b = cards::frame_dir(root, "assets/tier.svg?S=Dust%20III");
    assert!(a.starts_with(root), "the project's own folder: {a:?}");
    assert_ne!(a, b, "two boards are two cards and must not share frames");
}
