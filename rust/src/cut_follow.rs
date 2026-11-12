//! spec/05-cut.md F2.15: the camera follows the speaker.
//!
//! Two cameras that filmed the same seconds are two rows, and a suggested scene is shown from one of them.
//! In a conversation with a camera on each person the picture has one rule a machine can keep: show
//! whoever is speaking. It is kept here, by rule and not by a model, as the last step of a suggestion:
//! who has the floor is read off the session's spoken lines, each speaker's camera off `prepare/who.json`
//! ([`crate::who`]), and every kept scene is split where the floor changes hands.

use crate::cut::Seg;
use crate::layout::Tree;
use crate::timeline::Recording;

/// P.policy.cameraLeadSeconds: the camera changes this long before the new speaker's first word.
pub const LEAD_SECONDS: f64 = 0.3;
/// P.policy.interjectionSeconds: a shorter stretch of talk inside the other's does not take the picture.
pub const INTERJECTION_SECONDS: f64 = 1.5;
/// P.policy.minShotSeconds: no shot shorter than this is made.
pub const MIN_SHOT_SECONDS: f64 = 2.5;
/// One speaker's lines nearer than this are one stretch of talk.
const SAME_TALK_GAP: f64 = 1.0;
/// After an interjection the other goes on within this, or it was a turn after all.
const RESUME_SECONDS: f64 = 2.0;

/// One stretch of one speaker's talk, on the session clock, with the camera row that shows them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Talk {
    pub s: f64,
    pub e: f64,
    pub row: i32,
}

/// A change of camera: from `at` on, the picture is `row`'s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Switch {
    pub at: f64,
    pub row: i32,
}

/// S3: the lines of speakers who have a camera, as stretches of talk: one speaker's lines less than a
/// second apart are one stretch. `lines` are `(start, end, row)`, in any order.
pub fn stretches(lines: &[(f64, f64, i32)]) -> Vec<Talk> {
    let mut sorted = lines.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<Talk> = Vec::new();
    for (s, e, row) in sorted {
        // The last stretch of the SAME row, even with the other's interjection in between: two lines of
        // one person are one stretch when they are close, whoever coughed meanwhile.
        match out.iter_mut().rev().find(|talk| talk.row == row) {
            Some(last) if s - last.e < SAME_TALK_GAP => last.e = last.e.max(e),
            _ => out.push(Talk { s, e, row }),
        }
    }
    out.sort_by(|a, b| a.s.total_cmp(&b.s));
    out
}

/// S3: is this stretch an interjection, given the stretches around it? Short, and either inside the
/// other's talk or followed by the other going on within two seconds.
fn is_interjection(talk: &Talk, all: &[Talk]) -> bool {
    if talk.e - talk.s >= INTERJECTION_SECONDS {
        return false;
    }
    all.iter().any(|other| {
        other.row != talk.row
            && ((other.s <= talk.s && other.e >= talk.e)
                || (other.s < talk.s && other.e > talk.s - RESUME_SECONDS && {
                    // The same other speaker is heard again soon after.
                    all.iter().any(|next| {
                        next.row == other.row
                            && next.s >= talk.s
                            && next.s - talk.e <= RESUME_SECONDS
                    })
                }))
    })
}

/// S3 + S4: who has the floor, as the switches of the picture. Returns the switches (the first is where
/// the picture starts, at minus infinity) and how many interjections were left on the other's camera.
pub fn switches(lines: &[(f64, f64, i32)]) -> (Vec<Switch>, usize) {
    let all = stretches(lines);
    let floor: Vec<Talk> = all
        .iter()
        .filter(|talk| !is_interjection(talk, &all))
        .copied()
        .collect();
    let interjections = all.len() - floor.len();
    let mut out: Vec<Switch> = Vec::new();
    let mut last_end = f64::NEG_INFINITY;
    for talk in &floor {
        match out.last() {
            None => out.push(Switch {
                at: f64::NEG_INFINITY,
                row: talk.row,
            }),
            Some(held) if held.row == talk.row => {}
            Some(held) => {
                // A moment before the first word, never before the one before has finished; while both
                // talk, at the first word.
                let at = if last_end > talk.s {
                    talk.s
                } else {
                    (talk.s - LEAD_SECONDS).max(last_end)
                };
                // S4: a shot that would be shorter than the shortest is not made: the picture stays
                // where it was before it, and this switch is judged from there.
                if at - held.at < MIN_SHOT_SECONDS && out.len() >= 2 {
                    out.pop();
                    if out.last().is_some_and(|before| before.row == talk.row) {
                        last_end = last_end.max(talk.e);
                        continue;
                    }
                }
                out.push(Switch { at, row: talk.row });
            }
        }
        last_end = last_end.max(talk.e);
    }
    (out, interjections)
}

/// The row the picture is on at `t`.
fn row_at(switches: &[Switch], t: f64) -> Option<i32> {
    switches
        .iter()
        .rev()
        .find(|switch| switch.at <= t)
        .map(|switch| switch.row)
}

/// S5: every kept scene split at the switches inside it, each piece shown from the camera of whoever holds
/// the floor. A switch nearer than [`MIN_SHOT_SECONDS`] to a scene's edge moves onto that edge; a second a
/// camera did not film is shown from the scene's own row. Inserts are left alone. `filmed(row, s, e)` says
/// whether that row's camera filmed the whole stretch.
pub fn apply(
    segs: &[Seg],
    switches: &[Switch],
    filmed: &dyn Fn(i32, f64, f64) -> bool,
) -> Vec<Seg> {
    let mut out = Vec::new();
    for seg in segs {
        if seg.is_insert() || seg.e <= seg.s {
            out.push(seg.clone());
            continue;
        }
        // The switches that fall well inside this scene.
        let inside: Vec<&Switch> = switches
            .iter()
            .filter(|switch| {
                switch.at > seg.s + MIN_SHOT_SECONDS && switch.at < seg.e - MIN_SHOT_SECONDS
            })
            .collect();
        let mut from = seg.s;
        let mut pieces: Vec<(f64, f64)> = Vec::new();
        for switch in &inside {
            pieces.push((from, switch.at));
            from = switch.at;
        }
        pieces.push((from, seg.e));
        for (index, (s, e)) in pieces.iter().enumerate() {
            // A switch pulled onto the scene's start decides the first piece; the picture at the
            // piece's own middle decides the rest (a switch pulled onto the end is the next scene's).
            let ask = if index == 0 {
                (s + MIN_SHOT_SECONDS).min((s + e) / 2.0)
            } else {
                (s + e) / 2.0
            };
            let wanted = row_at(switches, ask).unwrap_or(seg.cam);
            let row = if filmed(wanted, *s, *e) {
                wanted
            } else {
                seg.cam
            };
            out.push(Seg {
                s: *s,
                e: *e,
                cam: row,
                // A piece that starts at a switch starts at a border nobody merges away.
                split: seg.split || index > 0,
                ..seg.clone()
            });
        }
    }
    // Two pieces of one scene that ended on the same camera are one piece again.
    let mut merged: Vec<Seg> = Vec::new();
    for seg in out {
        match merged.last_mut() {
            Some(last)
                if !last.is_insert()
                    && !seg.is_insert()
                    && last.cam == seg.cam
                    && (last.e - seg.s).abs() < 1e-9
                    && seg.split
                    && !segs.iter().any(|orig| (orig.s - seg.s).abs() < 1e-9) =>
            {
                last.e = seg.e;
            }
            _ => merged.push(seg),
        }
    }
    merged
}

/// How many changes of camera a cut holds, and the seconds shown from each row.
pub fn tally(segs: &[Seg]) -> (usize, Vec<(i32, f64)>) {
    let footage: Vec<&Seg> = segs.iter().filter(|seg| !seg.is_insert()).collect();
    let changes = footage
        .windows(2)
        .filter(|pair| pair[0].cam != pair[1].cam)
        .count();
    let mut shown: Vec<(i32, f64)> = Vec::new();
    for seg in footage {
        match shown.iter_mut().find(|(row, _)| *row == seg.cam) {
            Some((_, seconds)) => *seconds += seg.e - seg.s,
            None => shown.push((seg.cam, seg.e - seg.s)),
        }
    }
    shown.sort_by_key(|(row, _)| *row);
    (changes, shown)
}

/// The whole flow for a landed cut: S1's who.json, S3's lines, S5 onto the scenes. `recordings` and `rows`
/// are the page's (each footage recording and the row it is drawn on). `None` when there is nothing to
/// follow, with the sentence why; `Some` with the new scenes and the log's lines.
pub fn follow(
    tree: &Tree,
    segs: &[Seg],
    recordings: &[Recording],
    rows: &[usize],
) -> Result<(Vec<Seg>, Vec<String>), String> {
    let who = crate::who::read(tree).ok_or("no prepare/who.json says who is on which camera")?;
    let row_of = |camera: &str| -> Option<i32> {
        recordings
            .iter()
            .position(|recording| recording.base == camera)
            .and_then(|index| rows.get(index))
            .map(|row| *row as i32)
    };
    let mut logs = Vec::new();
    let mut speaker_rows: Vec<(String, i32)> = Vec::new();
    for person in &who.people {
        if let Some(row) = row_of(&person.camera) {
            speaker_rows.push((person.speaker.clone(), row));
            logs.push(format!(
                ">>> cameras: {} is {}, shown on {}",
                person.speaker,
                if person.name.trim().is_empty() {
                    "not named"
                } else {
                    person.name.trim()
                },
                person.camera
            ));
        }
    }
    let mut distinct: Vec<i32> = speaker_rows.iter().map(|(_, row)| *row).collect();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.len() < 2 {
        return Err(
            "who.json puts fewer than two speakers on a camera of this session".to_string(),
        );
    }
    let lines: Vec<(f64, f64, i32)> = crate::textfmt::read_session(&tree.session_tsv())?
        .iter()
        .filter_map(|line| {
            let row = speaker_rows
                .iter()
                .find(|(speaker, _)| *speaker == line.who)?
                .1;
            Some((line.start, line.end, row))
        })
        .collect();
    if lines.is_empty() {
        return Err("no spoken line has a speaker with a camera".to_string());
    }
    let (found, interjections) = switches(&lines);
    let filmed = |row: i32, s: f64, e: f64| {
        recordings.iter().zip(rows).any(|(recording, on)| {
            *on as i32 == row && recording.start <= s + 1e-6 && recording.end >= e - 1e-6
        })
    };
    let landed = apply(segs, &found, &filmed);
    let (changes, shown) = tally(&landed);
    let name_of = |row: i32| {
        recordings
            .iter()
            .zip(rows)
            .find(|(_, on)| **on as i32 == row)
            .map(|(recording, _)| recording.base.clone())
            .unwrap_or_else(|| format!("camera {row}"))
    };
    logs.push(format!(
        ">>> cameras: {changes} switch(es) -- {}; {interjections} interjection(s) left on the other's camera",
        shown
            .iter()
            .map(|(row, seconds)| format!("{} for {}", name_of(*row), crate::tools::mm_ss(*seconds)))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    Ok((landed, logs))
}
