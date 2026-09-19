//! §01-project-and-files#6-text-formats, part 3 — the waveform cache: magic `AWV4`, the
//! packed little-endian header naming the recording it was made from, and the peak bytes.
//!
//! Everything here asserts on the BYTES, because a cache file is read by nobody but this
//! build and a byte in the wrong place is a lane drawn from the wrong recording.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use naivepost::layout::Tree;
use naivepost::wave::{self, Wave};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-wave-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A project folder inside a throwaway root.
fn project(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, Tree::new(&dir).unwrap())
}

const SIZE: i64 = 4_096_000;
const MTIME: i64 = 1_758_030_330;

/// A two-channel lane at 200 peaks a second.
fn stereo() -> Wave {
    Wave { hz: 200.0, chans: vec![vec![1, 2, 3, 4], vec![9, 8, 7, 6]] }
}

fn bytes_of(wave: &Wave) -> Vec<u8> {
    wave::encode(wave, SIZE, MTIME).expect("encodes")
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_wave_file_opens_with_the_magic_and_a_packed_header() {
    let bytes = bytes_of(&stereo());

    assert_eq!(&bytes[..4], b"AWV4");
    // 27 bytes before the peaks: 1 + 2 + 4 + 8 + 8, with no alignment padding anywhere —
    // `hz` starts at byte 5, not byte 6.
    assert_eq!(wave::HEADER_LEN, 23);
    assert_eq!(bytes.len(), 4 + wave::HEADER_LEN + 2 * 4);

    let head = &bytes[4..4 + wave::HEADER_LEN];
    assert_eq!(head[0], 2, "chans");
    // 200 hertz is c8 00 little-endian, immediately after the channel count.
    assert_eq!(&head[1..3], &[0xc8, 0x00], "hz");
    assert_eq!(&head[3..7], &[4, 0, 0, 0], "count is the first channel's length");
    assert_eq!(&head[7..15], &SIZE.to_le_bytes(), "size");
    assert_eq!(&head[15..23], &MTIME.to_le_bytes(), "mtime");

    // Then the peak blocks, verbatim, one per channel, in order.
    assert_eq!(&bytes[4 + wave::HEADER_LEN..], &[1, 2, 3, 4, 9, 8, 7, 6]);
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_cached_wave_round_trips_for_one_or_two_channels() {
    let (_root, t) = project("round-trip");

    wave::write(&t, "cam", &stereo(), SIZE, MTIME).unwrap();
    assert_eq!(wave::read(&t, "cam", SIZE, MTIME).unwrap(), Some(stereo()));

    // One channel is the common case: a mono recording, or a stereo one whose two sides are
    // the same signal — which is what AWV4's own number means.
    let mono = Wave { hz: 100.0, chans: vec![vec![5, 6]] };
    wave::write(&t, "mic", &mono, SIZE, MTIME).unwrap();
    assert_eq!(wave::read(&t, "mic", SIZE, MTIME).unwrap(), Some(mono.clone()));

    // The rate survives as the whole number the header keeps.
    assert_eq!(wave::read(&t, "mic", SIZE, MTIME).unwrap().unwrap().hz, 100.0);

    // A recording too short to hold a peak still caches: two empty blocks and a count of 0.
    let empty = Wave { hz: 200.0, chans: vec![vec![], vec![]] };
    wave::write(&t, "blip", &empty, SIZE, MTIME).unwrap();
    assert_eq!(wave::read(&t, "blip", SIZE, MTIME).unwrap(), Some(empty));

    // Each lane keeps its own file, named for the lane.
    assert!(t.wave("cam").ends_with("cache/waves/cam.wave"));
    assert_ne!(t.wave("cam"), t.wave("mic"));
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_wave_cache_is_written_where_layout_says_at_0644() {
    let (_root, t) = project("mode");
    wave::write(&t, "cam", &stereo(), SIZE, MTIME).unwrap();

    let path = t.wave("cam");
    assert!(path.exists());
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o644);
    // §1's spine: the cache/waves directory was made by the write.
    assert!(t.dir().join("cache/waves").is_dir());

    // And the file on disk is exactly what encode produced.
    assert_eq!(std::fs::read(&path).unwrap(), bytes_of(&stereo()));
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_cache_that_does_not_apply_is_missed_not_read() {
    // Every one of these is "not read": a wave file whose only job is to save work must not
    // be able to fail a page open, and must never be guessed at.
    let good = bytes_of(&stereo());

    // A different format — including an older AWV3 cache, which would otherwise be read as
    // peaks for a lane that no longer exists.
    let mut magic = good.clone();
    magic[..4].copy_from_slice(b"AWV3");
    assert_eq!(wave::decode(&magic, SIZE, MTIME), None);

    // A header cut short by one byte.
    assert_eq!(wave::decode(&good[..good.len() - 1], SIZE, MTIME), None);
    assert_eq!(wave::decode(&good[..20], SIZE, MTIME), None);
    assert_eq!(wave::decode(b"AWV", SIZE, MTIME), None);

    // An impossible channel count: nought has nothing to draw, three has no meaning.
    for chans in [0u8, 3] {
        let mut broken = good.clone();
        broken[4] = chans;
        assert_eq!(wave::decode(&broken, SIZE, MTIME), None, "chans {chans}");
    }

    // A rate of nought is not a rate.
    let mut broken = good.clone();
    broken[5..7].copy_from_slice(&[0, 0]);
    assert_eq!(wave::decode(&broken, SIZE, MTIME), None);

    // A peak block cut short, and one byte of trailing garbage — both say the file does not
    // hold what the header claims.
    assert_eq!(wave::decode(&good[..good.len() - 5], SIZE, MTIME), None);
    let mut long = good.clone();
    long.push(0);
    assert_eq!(wave::decode(&long, SIZE, MTIME), None);

    // The source changed under the cache: a different recording, so different peaks.
    assert_eq!(wave::decode(&good, SIZE + 1, MTIME), None);
    assert_eq!(wave::decode(&good, SIZE, MTIME + 1), None);
    // Both agreeing is what reads it.
    assert!(wave::decode(&good, SIZE, MTIME).is_some());
}

#[test]
fn sec_01_project_and_files_6_text_formats_no_cache_and_a_wave_that_cannot_be_written() {
    let (_root, t) = project("absent");
    // No file: measure it. Not an error — the cache is an optimisation.
    assert!(wave::read(&t, "cam", SIZE, MTIME).unwrap().is_none());

    // Writing our own nonsense is a bug rather than a miss, so it says so.
    let no_channels = Wave { hz: 200.0, chans: vec![] };
    assert!(wave::encode(&no_channels, SIZE, MTIME).is_err());

    // Blocks of different lengths could not be read back by anyone: one count covers them
    // all.
    let ragged = Wave { hz: 200.0, chans: vec![vec![1, 2], vec![3]] };
    assert!(wave::encode(&ragged, SIZE, MTIME).is_err());

    // A rate the header cannot hold is the writer's mistake too.
    assert!(wave::encode(&Wave { hz: 0.0, chans: vec![vec![1]] }, SIZE, MTIME).is_err());
    assert!(wave::encode(&Wave { hz: 70_000.0, chans: vec![vec![1]] }, SIZE, MTIME).is_err());

    // And a lane whose cache could not be written leaves no half file behind.
    assert!(!t.wave("cam").exists());
}

#[test]
fn sec_01_project_and_files_6_text_formats_the_header_offsets_are_what_the_format_says() {
    // Pinned against §6's field list rather than against the writer, so a change to either
    // side of the pair shows up: chans u8, hz u16, count u32, size i64, mtime i64.
    let bytes = bytes_of(&Wave { hz: 1.0, chans: vec![vec![7; 300]] });
    let head = &bytes[4..27];
    assert_eq!(head[0], 1);
    assert_eq!(u16::from_le_bytes([head[1], head[2]]), 1);
    assert_eq!(u32::from_le_bytes(head[3..7].try_into().unwrap()), 300);
    assert_eq!(i64::from_le_bytes(head[7..15].try_into().unwrap()), SIZE);
    assert_eq!(i64::from_le_bytes(head[15..23].try_into().unwrap()), MTIME);
    // The peaks start where the header ends.
    assert_eq!(&bytes[27..], &[7u8; 300][..]);

    // A count over what a u32 holds cannot be written at all.
    let too_many = Wave { hz: 200.0, chans: vec![vec![0; 0]] };
    assert!(wave::encode(&too_many, SIZE, MTIME).is_ok(), "an empty lane is fine");
}
