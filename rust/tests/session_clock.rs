// §01-project-and-files#8-session-clock — 8. Session clock.
//
// Spec: spec/01-project-and-files.md §8. Every source is placed by its file-name timestamp and
// the earliest is zero; an unstamped file sits at the session start, never at its mtime;
// nothing stamped means everything starts at 0:00; the clock always spans the whole session;
// frames are named by their exact second on the file's clock.
//
// §8 numbers no steps, so the clauses below are numbered in the order the spec states them:
// s1 the earliest stamp is zero, s2 the accepted spellings, s3 an unstamped file, s4 nothing
// stamped, s5 the whole session, s6 frame names.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use naivepost::clock::{clock, frame_name, name_stamp, read_frame};

/// `2026-08-08 19:55:15` on the clock this module reads names with — the instant several of
/// the accepted spellings below claim.
fn at(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> f64 {
    // Built from a frame name so the tests never repeat the arithmetic they check.
    read_frame(&format!("{y:04}-{mo:02}-{d:02}_{h:02}-{mi:02}-{s:02}.000"))
        .expect("a well-formed frame name")
}

#[test]
fn sec_01_project_and_files_8_session_clock_s1_earliest_stamp_is_zero() {
    // "Every source is placed by its file-name timestamp; the earliest is zero."
    let names = [
        "2026-08-08 19-57-15.mp4",
        "2026-08-08 19-55-15.mp4",
        "2026-08-08 20-55-15.mp4",
    ];
    let c = clock(&names);
    assert_eq!(c.zero, at(2026, 8, 8, 19, 55, 15), "zero is the earliest stamp");
    // The list's order changes nothing: each file sits where its own name says.
    assert_eq!(c.offset(0), 120.0);
    assert_eq!(c.offset(1), 0.0);
    assert_eq!(c.offset(2), 3600.0);

    // The same three in a different order place identically, and `zero` follows the earliest.
    let c = clock(&[names[2], names[1], names[0]]);
    assert_eq!(c.zero, at(2026, 8, 8, 19, 55, 15));
    assert_eq!(c.offset(0), 3600.0);
    assert_eq!(c.offset(1), 0.0);
    assert_eq!(c.offset(2), 120.0);

    // A path is fine: only the file's own name names a moment, not the folder around it.
    let in_folder = clock(&["/home/bocek/Videos/2026-08-08 19-55-15.mp4"]);
    assert_eq!(in_folder.offset(0), 0.0);
    assert_eq!(
        clock(&["2026-08-08 19-55-15/take.mp4"]).offset(0),
        0.0,
        "a stamped folder says when the session was made, not when this file was"
    );
}

#[test]
fn sec_01_project_and_files_8_session_clock_s2_every_accepted_spelling() {
    // "Accepted spellings (year first, century 19/20): `2026-08-08 19-55-15`,
    // `…-20260808-195900-0`, `VID_20250814_213311`, `20250814213311`,
    // `2025.08.14 - 21.33.11.03`, `2026-08-08 at 7.55.15 PM`, `2026-08-08T19:55:15`, and bare
    // unix seconds 2017–2033."

    // OBS, with spaces and dashes for the time.
    assert_eq!(name_stamp("2026-08-08 19-55-15.mp4"), Some(at(2026, 8, 8, 19, 55, 15)));
    // Quest, a run of digits with a trailing suffix after the seconds.
    assert_eq!(name_stamp("quest-20260808-195900-0.mp4"), Some(at(2026, 8, 8, 19, 59, 0)));
    // A phone camera.
    assert_eq!(name_stamp("VID_20250814_213311.mp4"), Some(at(2025, 8, 14, 21, 33, 11)));
    // A dashcam: fourteen digits and nothing between them.
    assert_eq!(name_stamp("20250814213311.mp4"), Some(at(2025, 8, 14, 21, 33, 11)));
    // ShadowPlay: dots throughout, and a fourth group after the seconds.
    assert_eq!(name_stamp("2025.08.14 - 21.33.11.03.mp4"), Some(at(2025, 8, 14, 21, 33, 11)));
    // QuickTime: a one-digit hour with mandatory separators, and the half of the day.
    assert_eq!(name_stamp("2026-08-08 at 7.55.15 PM.mp4"), Some(at(2026, 8, 8, 19, 55, 15)));
    // ISO, glued by a T.
    assert_eq!(name_stamp("2026-08-08T19:55:15.mp4"), Some(at(2026, 8, 8, 19, 55, 15)));

    // Two spellings of one instant land on the same second, which is what puts them on one
    // clock at all.
    let c = clock(&["2026-08-08T19:55:15.mp4", "2026-08-08 19-55-15.mov"]);
    assert_eq!(c.offset(0), 0.0);
    assert_eq!(c.offset(1), 0.0, "the same moment written twice is the same place");

    // The century is pinned to 19/20: an old tape still names its own second.
    assert_eq!(name_stamp("1998-03-01 08-00-00.mp4"), Some(at(1998, 3, 1, 8, 0, 0)));
    assert_eq!(frame_name(at(1998, 3, 1, 8, 0, 0)), "1998-03-01_08-00-00.000");

    // The 12-hour form names its half of the day; midnight under `am` is zero, noon is
    // already twelve.
    assert_eq!(name_stamp("2026-08-08 at 12.05.00 AM.mp4"), Some(at(2026, 8, 8, 0, 5, 0)));
    assert_eq!(name_stamp("2026-08-08 at 12.05.00 PM.mp4"), Some(at(2026, 8, 8, 12, 5, 0)));
    assert_eq!(name_stamp("2026-08-08 at 7.55.15 am.mp4"), Some(at(2026, 8, 8, 7, 55, 15)));

    // Bare unix seconds, which is what an iPhone screen recording carries: the pin's floor
    // (2017-07-14) is asserted here and 2022-12-31 is a mid-window reading. The prototype writes
    // one with its digits glued to the word (`RPReplay_Final1723456789.mp4`); its `(?:\A|\D)`
    // bound is zero-width and this engine's `[^0-9]` is consumed, but a letter is not a digit, so
    // both read that name. What neither reads is an eleven-digit run: it is a serial number.
    assert_eq!(name_stamp("RPReplay_Final_1500000000.mp4"), Some(1_500_000_000.0)); // 2017-07-14: the pin's floor
    assert_eq!(name_stamp("RPReplay_Final_1672527599.mp4"), Some(1_672_527_599.0)); // 2022-12-31
    assert_eq!(name_stamp("take-1723456789.wav"), Some(1_723_456_789.0));
    // A glued name reads too, once the run is exactly ten digits: the left bound this engine
    // consumes rather than asserts (`(?:^|[^0-9])`) costs nothing when the letter before it is
    // not a digit, which is why the prototype's zero-width `(?:\A|\D)` and this pattern agree.
    assert_eq!(
        name_stamp("RPReplay_Final17234567890.mp4"),
        None,
        "eleven digits are a serial number, not a stamp: the bound this engine consumes rather \
         than asserts is what refuses a run longer than ten"
    );

    // And a stamp is placed by the clock like any other: an iPhone recording an hour before
    // an OBS one sits 3600 s behind it.
    let c = clock(&["2026-08-08 19-55-15.mp4", "clip-1786399000.wav"]);
    assert_eq!(c.offset(0), 0.0);
    assert!(c.offset(1) > 0.0, "the unix stamp is later than the OBS one");
}

#[test]
fn sec_01_project_and_files_8_session_clock_s2_impossible_or_day_first_names_are_no_stamp() {
    // "Always year first (day/month order is a guess that misplaces by weeks); century pinned
    // to 19/20; parseStamp rejects month 13 and hour 25."
    assert_eq!(name_stamp("2026-13-08 19-55-15.mp4"), None, "month 13");
    assert_eq!(name_stamp("2026-08-08 25-55-15.mp4"), None, "hour 25");
    assert_eq!(name_stamp("2026-08-32 19-55-15.mp4"), None, "day 32");
    assert_eq!(name_stamp("2026-02-30 19-55-15.mp4"), None, "a February that never was");
    assert_eq!(name_stamp("1899-08-08 19-55-15.mp4"), None, "the century is pinned to 19/20");

    // Day first reads as no stamp at all rather than as a guess.
    assert_eq!(name_stamp("14.08.2025 21-33-11.mp4"), None);
    assert_eq!(name_stamp("14-08-2025 21-33-11.mp4"), None);

    // A bare run of digits only counts inside the window, and only as ten digits bounded by
    // non-digits.
    assert_eq!(name_stamp("video-1499999999.mp4"), None, "2017 is the floor");
    assert_eq!(name_stamp("video-2147483647.mp4"), None, "and 2033 the ceiling");
    assert_eq!(name_stamp("video-1500000000.mp4"), Some(1_500_000_000.0));
    assert_eq!(name_stamp("clip-17234567890.mp4"), None, "eleven digits is a serial");

    // A digit run that only resembles a date cannot mask the real stamp after it: every
    // candidate gets a chance.
    assert_eq!(
        name_stamp("1999-99-99 VID_20250814_213311.mp4"),
        Some(at(2025, 8, 14, 21, 33, 11))
    );
    assert_eq!(
        name_stamp("h264-1920x1080-2026-08-08-19-55-15.mp4"),
        Some(at(2026, 8, 8, 19, 55, 15))
    );

    // So an impossible name is unstamped: it sits at the session start and warns.
    let c = clock(&["2026-08-08 19-55-15.mp4", "2026-13-08 19-55-15.mp4"]);
    assert_eq!(c.offset(1), 0.0);
    assert!(c.needs_warning(1));
}

#[test]
fn sec_01_project_and_files_8_session_clock_s3_unstamped_sits_at_the_session_start_not_its_mtime()
{
    // "An unstamped file sits at the session start (never at its mtime); its row shows a
    // warning."
    let dir = std::env::temp_dir().join(format!(
        "np-clock-{}-s3",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // A recording with no stamp in its name, given an mtime years after the session.
    let path: PathBuf = dir.join("mic-recording.wav");
    fs::write(&path, "audio").unwrap();
    // `FileTimes` is the setter's return value, not a handle: build it, then hand it over.
    let times = fs::FileTimes::new()
        .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(978_307_200));
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(times)
        .unwrap();

    let c = clock(&[
        "2026-08-08 19-55-15.mp4",
        path.file_name().unwrap().to_str().unwrap(),
    ]);
    assert_eq!(c.offset(1), 0.0, "at the session start, not at its mtime");
    assert!(c.needs_warning(1), "its row shows a warning");
    assert!(!c.needs_warning(0), "a stamped row does not");

    // Nothing read the disk to find that out: the same name in another folder is placed the
    // same way, which is why the signature takes names and no stat or ffprobe happens.
    let elsewhere = clock(&["2026-08-08 19-55-15.mp4", "mic-recording.wav"]);
    assert_eq!(elsewhere.offset(1), 0.0);

    // An mtime inside the session's own hour would have placed it wrongly, which is the point:
    // a copied file's mtime is when it was copied.
    let stamped_at = c.sources[1].at;
    assert_eq!(stamped_at, c.zero);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn sec_01_project_and_files_8_session_clock_s4_nothing_stamped_starts_at_zero() {
    // "Nothing stamped → everything starts at 0:00."
    let names = ["screen.mp4", "mic.wav", "camera.mov"];
    let c = clock(&names);
    assert_eq!(c.zero, 0.0);
    for i in 0..names.len() {
        assert_eq!(c.offset(i), 0.0);
        assert!(c.needs_warning(i), "{} warns", names[i]);
    }

    // An empty session is the same case with nothing to warn about.
    let none = clock(&[]);
    assert_eq!(none.zero, 0.0);
    assert!(none.sources.is_empty());
}

#[test]
fn sec_01_project_and_files_8_session_clock_s5_the_clock_spans_the_whole_session() {
    // "The clock always spans the whole session; a subset would move zero."
    let first = "2026-08-08 19-55-15.mp4";
    let later = "2026-08-08 20-55-15.mp4";

    let whole = clock(&[first, later]);
    assert_eq!(whole.offset(1), 3600.0);

    // Ask about the same file with the earliest recording left out and it moves: an hour of
    // session vanishes from under it. This is the reason callers hand over every source.
    let subset = clock(&[later]);
    assert_eq!(subset.offset(0), 0.0);
    assert_ne!(whole.offset(1), subset.offset(0));

    // Including a file with no stamp in the list does not move anything — it lands on zero
    // rather than becoming it.
    let with_unstamped = clock(&[first, "mic.wav", later]);
    assert_eq!(with_unstamped.zero, whole.zero);
    assert_eq!(with_unstamped.offset(2), 3600.0);

    // And a source the session does not contain is placed by the same rule as one it does:
    // the list handed over is what defines the clock, so a sting on a cut lane asked about
    // alongside the session gets its own stamp.
    let with_extra = clock(&[first, later, "sting-2026-08-08 19-56-15.wav"]);
    assert_eq!(with_extra.offset(2), 60.0);
    assert_eq!(with_extra.zero, whole.zero, "zero did not move");
}

#[test]
fn sec_01_project_and_files_8_session_clock_s6_frames_are_named_by_their_exact_second() {
    // "Frames are named by their exact second on the file's clock
    // (`YYYY-MM-DD_HH-MM-SS.mmm`), never from a sorted position."
    let base = at(2026, 8, 8, 19, 55, 15);
    assert_eq!(frame_name(base), "2026-08-08_19-55-15.000");
    // The 250 ms grid of §1's inputs/frames/<source>/.
    assert_eq!(frame_name(base + 0.25), "2026-08-08_19-55-15.250");
    assert_eq!(frame_name(base + 0.5), "2026-08-08_19-55-15.500");
    assert_eq!(frame_name(base + 0.75), "2026-08-08_19-55-15.750");
    assert_eq!(frame_name(base + 1.0), "2026-08-08_19-55-16.000", "the next second");

    // No index and no collision suffix: the name is a function of the instant alone, so
    // naming the same instants backwards gives the same names.
    let forwards: Vec<String> = (0..4).map(|n| frame_name(base + n as f64 * 0.25)).collect();
    let backwards: Vec<String> = (0..4)
        .rev()
        .map(|n| frame_name(base + n as f64 * 0.25))
        .collect();
    assert_eq!(forwards.len(), 4);
    for name in &forwards {
        assert!(backwards.contains(name), "{name} depends on the order");
    }
    let mut sorted = forwards.clone();
    sorted.sort();
    assert_eq!(sorted, forwards, "the names sort in the order they were made");

    // Read back: the inverse of the same spelling, and nothing else.
    for (n, name) in forwards.iter().enumerate() {
        assert_eq!(read_frame(name), Some(base + n as f64 * 0.25), "{name}");
    }
    assert_eq!(read_frame("f000001.jpg"), None, "a frame from before frames were stamped");
    assert_eq!(read_frame("2026-08-08_19-55-15.00"), None, "three digits of millisecond");

    // A frame on the file's own clock: the instant a name claims renders back as that name.
    let stamp = name_stamp("VID_20250814_213311.mp4").unwrap();
    assert_eq!(frame_name(stamp), "2025-08-14_21-33-11.000");

    // The last millisecond of a second, and the rounding that carries into the next one.
    assert_eq!(frame_name(base + 0.999), "2026-08-08_19-55-15.999");
    assert_eq!(frame_name(base + 0.9996), "2026-08-08_19-55-16.000");
}
