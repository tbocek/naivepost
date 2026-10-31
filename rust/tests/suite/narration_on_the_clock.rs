//! Spoken lines: heard once, whole, where they were put.

use crate::common;
use naivepost::cut::Seg;
use naivepost::narrate_preview::{tick, Take, Tick};
use naivepost::narrate_preview_leg::{
    audition_is_current, hold_audition, next_audition, stop_running,
};
use naivepost::narrate_screen::tag_moves_line;
use naivepost::narration::{lines_to_speak, load, save, save_held, Entry, Narration, Silent};
use naivepost::produce_exec::fit_spoken;
use naivepost::produce_render::{Clip, Placed};

fn entry(text: &str, pos: &str) -> Entry {
    Entry {
        s: 0.0,
        e: 10.0,
        text: text.into(),
        pos: pos.into(),
        ..Default::default()
    }
}

#[test]
fn a_line_already_speaking_is_not_started_again() {
    let segs = [Seg {
        s: 0.0,
        e: 10.0,
        ..Default::default()
    }];
    let entries = [entry("words", "")];
    let takes = [Take::Exists];
    assert!(matches!(
        tick(1.0, &segs, &entries, &takes, None),
        Tick::Speak { .. }
    ));
    assert_eq!(tick(1.1, &segs, &entries, &takes, Some(0)), Tick::Idle);
}

#[test]
fn a_caption_is_read_and_never_spoken() {
    let spoken = lines_to_speak(&[entry("said", ""), entry("read", "top"), entry("", "")]);
    assert_eq!(spoken.len(), 1);
    assert_eq!(spoken[0].text, "said");
}

#[test]
fn a_second_on_the_session_clock_lands_as_a_place_in_the_clip() {
    assert_eq!(tag_moves_line(100.0, 120.0, 110.0, 3.0), (10.0, false));
    assert_eq!(
        tag_moves_line(100.0, 120.0, 119.5, 3.0),
        (3.0, true),
        "the last second is refused"
    );
    assert_eq!(
        tag_moves_line(100.0, 120.0, 90.0, 3.0),
        (3.0, true),
        "outside the clip"
    );
}

fn clip(on_screen: f64, lines: &[(f64, f64)]) -> Clip {
    Clip {
        no: 1,
        on_screen,
        rate: 1.0,
        lines: lines
            .iter()
            .enumerate()
            .map(|(entry_index, (at, speech))| Placed {
                entry_index,
                at: *at,
                speech: *speech,
                tempo: 1.0,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn takes_longer_than_their_room_no_longer_overlap() {
    let mut clips = [clip(10.0, &[(5.0, 8.0), (6.0, 8.0)])];
    let logs = fit_spoken(&mut clips, &|_| 100.0);
    let lines = &clips[0].lines;
    let first_ends = lines[0].at + lines[0].speech / lines[0].tempo;
    assert!(lines[1].at >= first_ends, "{lines:?}");
    assert!(
        !logs.is_empty() || clips[0].on_screen > 10.0,
        "and it says so or grows: {logs:?}"
    );
}

#[test]
fn a_clip_grows_to_hear_a_line_out_and_keeps_room_it_does_not_need() {
    let mut clips = [clip(10.0, &[(8.0, 3.0)])];
    fit_spoken(&mut clips, &|_| 100.0);
    assert!(
        clips[0].on_screen > 10.0 || clips[0].lines[0].at + 3.0 <= 10.0,
        "{:?}",
        clips[0]
    );
    let mut fits = [clip(10.0, &[(1.0, 3.0)])];
    fit_spoken(&mut fits, &|_| 100.0);
    assert_eq!(fits[0].on_screen, 10.0);
    assert_eq!(fits[0].lines[0].at, 1.0);
}

#[test]
fn lines_sort_by_clip_then_place() {
    let line = |s: f64, at: f64| Entry {
        s,
        at,
        ..Default::default()
    };
    let mut lines = [line(10.0, 5.0), line(10.0, 1.0), line(0.0, 9.0)];
    naivepost::narration::sort_entries(&mut lines);
    let order: Vec<(f64, f64)> = lines.iter().map(|l| (l.s, l.at)).collect();
    assert_eq!(order, [(0.0, 9.0), (10.0, 1.0), (10.0, 5.0)]);
}

#[test]
fn a_new_take_is_a_different_file_for_the_render_too() {
    let dir = std::env::temp_dir().join(format!("roll-{}.naivepost", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    let mut line = entry("words", "");
    let first = naivepost::produce_stamp::wav_of(&tree, &line, Some("own"), line.roll);
    line.roll = 1;
    let second = naivepost::produce_stamp::wav_of(&tree, &line, Some("own"), line.roll);
    assert_ne!(first, second);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_held_save_does_not_bring_back_a_deleted_clip() {
    let (root, tree) = common::tree_in("held-save-silent");
    let mut on_file = Narration::default();
    on_file.silent.push(Silent { s: 4.0, e: 9.0 });
    save(&on_file, &tree).unwrap();
    let mut held = Narration::default();
    held.entries.push(entry("words", ""));
    save_held(&held, &tree).unwrap();
    let back = load(&tree).unwrap();
    assert_eq!(back.entries.len(), 1, "the held lines are written");
    assert_eq!(back.silent.len(), 1, "the file's marker stays");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_newer_press_or_a_stop_ends_the_audition_before_it() {
    let first = next_audition();
    assert!(audition_is_current(first));
    let child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = child.id();
    hold_audition(child, None);
    let second = next_audition();
    assert!(!audition_is_current(first), "the older press may not play");
    let gone = || {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map_or(true, |stat| stat.contains(") Z "))
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !gone() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(gone(), "the voice ended");
    let _ = stop_running();
    assert!(!audition_is_current(second), "⏹ counts too");
}
