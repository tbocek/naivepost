// F2.5 (Hush and mix — what the preview hears) badge **2** — spec/05-cut.md: "the recorders' band:
// one lane per separate recording or extra track, name plate with the channel layout (`mono`)", which
// `spec/img/05-lanes.png` spells as the plate `2026-09-16 17-26-20 mono`.
//
// Every rule is a plain function in `naivepost::cut_lanes`, so nothing here needs a display or a real
// ffprobe: the tool's answer arrives through `probe_channels`' `run` leg, which is the same seam
// `cut_fps::probe` and `source_probe::audio_streams` hand to their tests. The wire half — that the live
// band paints this text — lives in tests/cut_lane_plate_widgets.rs.

use naivepost::cut::Lane;
use naivepost::cut_lanes as lanes;

/// The screenshot's own lane: a separate recorder's file, one row, its first second at 1:14.
fn recorder(src: &str, name: &str) -> Lane {
    Lane {
        name: name.into(),
        src: src.into(),
        at: 74.0,
        off: 0.0,
        dur: 45.0,
    }
}

/// Badge 2, 1-channel file on one lane → `mono`: a one-channel recording says so rather than
/// calling its single channel "L", which would imply a missing right.
#[test]
fn f2_5_s2_the_lane_plate_says_the_channel_layout() {
    assert_eq!(lanes::channel_layout(1, 1, 0), "mono");
    assert_eq!(
        lanes::plate("2026-09-16 17-26-20", Some("mono")),
        "2026-09-16 17-26-20 mono",
        "the plate the spec picture shows"
    );
}

/// Badge 2, a 2-channel file drawn on ONE lane → `L=R`: it is not a mono file, it is a stereo file
/// carrying one signal in both channels, and a plate calling itself `mono` would look like the page
/// had lost a channel somewhere.
#[test]
fn f2_5_s2_a_stereo_file_on_one_lane_says_l_equals_r() {
    assert_eq!(lanes::channel_layout(2, 1, 0), "L=R");
    assert_eq!(lanes::channel_layout(6, 1, 0), "L=R", "more than two channels is still ≥ 2");
}

/// Badge 2, a 2-channel file drawn on TWO lanes → `L` then `R`, in lane order.
#[test]
fn f2_5_s2_a_stereo_file_on_two_lanes_says_l_and_r() {
    assert_eq!(lanes::channel_layout(2, 2, 0), "L");
    assert_eq!(lanes::channel_layout(2, 2, 1), "R");
    // And the counting happens over the lanes of THAT file: two rows of a mono file are L and R too,
    // because by then the lanes are the two channels being spread out, whatever the header said.
    assert_eq!(lanes::channel_layout(1, 2, 0), "L");
    assert_eq!(lanes::channel_layout(1, 2, 1), "R");
}

/// Badge 2's number comes from the tool: `probe_channels` asks ffprobe for the FIRST audio stream's
/// `channels` entry and reads back what it prints. Asserting the arguments is what proves the program
/// really asks, rather than assuming a shape.
#[test]
fn f2_5_s2_the_channel_count_comes_from_ffprobe() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let asked: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    // Cloned into the closure, because the seam's type is a `dyn Fn` with no lifetime to borrow against.
    let seen = Rc::clone(&asked);
    let stub = move |program: &str, args: &[&str]| -> Result<String, String> {
        assert_eq!(program, "ffprobe-stub", "the caller decides which binary is asked");
        *seen.borrow_mut() = args.iter().map(|arg| arg.to_string()).collect();
        Ok("2\n".to_string())
    };
    let path = std::path::Path::new("/nonexistent/mic.wav");
    assert_eq!(
        lanes::probe_channels("ffprobe-stub", path, &stub),
        Some(2),
        "a printed 2 is two channels"
    );
    let flags = asked.borrow();
    let joined = flags.join(" ");
    assert!(joined.contains("a:0"), "the FIRST AUDIO stream is what is asked: {joined}");
    assert!(joined.contains("stream=channels"), "and the field is `channels`: {joined}");
    assert!(flags.last().is_some_and(|last| last.ends_with("mic.wav")), "of THIS file: {joined}");
    drop(flags);

    // The pure halves, on their own: an empty answer (no audio stream, unreadable file) is no answer.
    assert_eq!(lanes::parse_channels("2\n"), Some(2));
    assert_eq!(lanes::parse_channels(" 1 "), Some(1));
    assert_eq!(lanes::parse_channels(""), None);
    let silent = |_: &str, _: &[&str]| -> Result<String, String> { Ok(String::new()) };
    assert_eq!(
        lanes::probe_channels("ffprobe-stub", path, &silent),
        None,
        "an empty answer is no count, not a guessed one"
    );
}

/// §00-principles' failure-is-specific-and-local: garbage from the tool answers `None`, and the plate
/// then carries the lane's name alone rather than inventing a shape.
#[test]
fn f2_5_s2_an_unprobeable_file_leaves_the_plate_a_name() {
    assert_eq!(lanes::parse_channels("N/A\n"), None);
    assert_eq!(lanes::parse_channels("channels = ?\n"), None);
    assert_eq!(lanes::parse_channels("0\n"), None, "zero channels is not a shape");
    assert_eq!(
        lanes::plate("2026-09-16 17-26-20", None),
        "2026-09-16 17-26-20",
        "no layout, just the name"
    );
}

/// The plates for a whole lane list, in `Scene.lanes`' order: each lane gets its own file's layout,
/// counted against the other lanes sharing that file. This is the state the band paints.
#[test]
fn f2_5_s2_the_plate_list_follows_the_lane_list() {
    let one = vec![recorder("project:audio/mic.wav", "mic")];
    let plates = lanes::plates(&one);
    assert_eq!(plates.len(), 1, "one plate per lane");
    assert!(
        plates[0].starts_with("mic "),
        "the plate begins with the lane's own name: {:?}",
        plates[0]
    );
    // Two lanes of one file are the file's two channels, whichever way the probe answered.
    let two = vec![
        recorder("project:audio/stereo.wav", "cam-L"),
        recorder("project:audio/stereo.wav", "cam-R"),
    ];
    let both = lanes::plates(&two);
    assert!(both[0].ends_with(" L"), "first lane of the pair: {:?}", both[0]);
    assert!(both[1].ends_with(" R"), "second lane of the pair: {:?}", both[1]);
}
