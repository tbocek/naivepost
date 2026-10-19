//! §07-narrate#6 (`Preview`), the two half-sentences the completion check found unimplemented:
//! "**when the picture stops, narration stops too** — except a line auditioned from its own ▶ over a
//! clip with no footage, which speaks against a **still frame**".
//!
//! Both are leg behaviour over REAL child processes, so both are read off `/proc` rather than from a
//! flag: a stub that only recorded calls could not tell a killed take from a live one, nor a frozen
//! picture from a running one. The injection point is the module's own `Spawn` seam — the same one
//! `spawn_player` uses in production — so the program is unchanged by being tested.
//!
//! One test per half, each over the real leg and the real `Spawn` seam.

use std::process::{Child, Stdio};

use naivepost::cut::Seg;
use naivepost::narrate_preview_leg::{self as leg, Spawn};
use naivepost::narrate_preview_media::{self as media, Cue};

use crate::common;
#[path = "speak_tts_wire_harness.rs"]
mod harness;

/// The spec item these tests belong to; asserted in each so a renamed item fails.
const ITEM: &str = "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass";

/// Write one file, creating its parent folders: the take's wav has to exist for the tick to be speaking a
/// line, and `narrate/tts/` is not there until something writes into it.
fn write_bytes(path: &std::path::Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the parent folder");
    }
    std::fs::write(path, bytes).expect("the file is written");
}

/// A process that is still there when we want to end it (`sleep 30`) and the one the scenario spawned.
type Pids = std::rc::Rc<std::cell::RefCell<Vec<u32>>>;

fn sleeper() -> Result<Child, String> {
    std::process::Command::new("sleep")
        .arg("30")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| err.to_string())
}

fn reap(pids: &Pids) {
    for pid in pids.borrow_mut().drain(..) {
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    }
}

/// Whether a pid is a LIVE process: `/proc/<pid>` existing is not enough, because a child this process
/// spawned and no longer holds is a ZOMBIE until it is reaped, and a zombie is dead. A stub that only
/// recorded calls could not tell these apart, which is the whole point of these two checks.
fn alive(pid: u32) -> bool {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    match stat.rfind(')') {
        Some(end) => match stat[end + 1..].trim_start().chars().next() {
            Some('Z') | None => false,
            Some(_) => true,
        },
        None => false,
    }
}

/// The state letter in `/proc/<pid>/stat`: R running, S sleeping, T stopped by SIGSTOP, Z a zombie.
fn proc_state(pid: u32) -> char {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    match stat.rfind(')') {
        Some(end) => stat[end + 1..].trim_start().chars().next().unwrap_or('?'),
        None => '?',
    }
}

/// The state letter once a `SIGSTOP` has landed — a signal is queued, not applied, so reading `/proc`
/// on the instruction the kill returned can still show it running. Bounded: a player that never stops
/// is the failure, and it must fail rather than wait.
fn proc_state_after_stop(pid: u32) -> char {
    // 2.5 s of tries, not a fixed 10 ms x 100: under a full `just test` run a SIGSTOP can take longer to
    // land than on an idle machine, and a check that fails at random fails other rounds that are not at fault.
    for _ in 0..250 {
        let state = proc_state(pid);
        if state != 'R' && state != 'D' {
            return state;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    proc_state(pid)
}

use common::entry_at_mid as entry;

/// A session folder: `Tree::new` needs the `.naivepost` name, and the take's wav lives under it.
struct Session {
    tree: naivepost::layout::Tree,
}

fn session(tag: &str) -> Session {
    let dir = std::env::temp_dir().join(format!("np-s6preview-{tag}.naivepost"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("session folder");
    Session {
        tree: naivepost::layout::Tree::new(&dir).expect("a .naivepost folder is a project"),
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.tree.dir());
    }
}

/// §6: "when the picture stops, narration stops too" — and the other half of that same rule, spelled in
/// [`leg::Preview::picture_finished`]'s own words: "**Only the picture answers this**: the voice child
/// outliving the picture is the bug, not a condition". So a take whose player finished FIRST is not an
/// ending: the walk carries on and the picture stays owned. Both players arrive through the program's own
/// doors (`start_running`, then the tick walking into the line) over the `Spawn` seam it spawns with.
#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s8_a_take_that_ends_first_leaves_the_picture_playing(
) {
    common::pin_private_cwd();
    assert_eq!(ITEM, "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass");
    let session = session("take-ends-first");
    let spawned: Pids = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let entries = vec![entry(0.0, 10.0, "first line")];
    // The take has to exist on disk for the tick to be speaking a line at all (S2's three branches), so
    // write one the size of a real reply, under the key the program itself asks with.
    let key = naivepost::narration::key_for(&entries[0], Some(""));
    write_bytes(
        &naivepost::narrate_tts::take_path(&session.tree, &key),
        &harness::wav_bytes(4096),
    );
    let cut = leg::cut_of(vec![Seg { s: 0.0, e: 10.0, dur: 10.0, ..Default::default() }], vec![]);
    naivepost::cut::save(&cut, &session.tree).expect("cut written");
    std::fs::write(std::env::current_dir().expect("cwd").join("cam0.mp4"), b"stub footage")
        .expect("footage on disk");
    naivepost::project::save(
        &naivepost::project::Project {
            sources: vec![naivepost::project::Source {
                path: "cam0.mp4".into(),
                footage: true,
                ..Default::default()
            }],
            ..Default::default()
        },
        session.tree.dir(),
    )
    .expect("project written");

    // First beat: a live picture, so the take is spawned for real and both players are owned.
    let mut spawn_fn = {
        let recorded = spawned.clone();
        move |_program: &str, _args: &[String]| {
            let child = sleeper()?;
            recorded.borrow_mut().push(child.id());
            Ok(child)
        }
    };
    let spawn: &mut Spawn = &mut spawn_fn;
    let said = leg::start_running(&session.tree, &cut, &entries, 0.0, 1.0, 1.0, spawn)
        .expect("the preview starts over the session's own footage");
    assert!(said.contains("playing"), "started: {said}");
    let first = leg::tick_loop(&session.tree, &cut, &entries, 0.6, 1.0, 1.0, false, spawn);
    assert!(matches!(first, leg::Stepped::Speaking { .. }), "the tick entered the line: {first:?}");
    let pids = leg::running_children();
    assert_eq!(pids.len(), 2, "picture and take are both running: {pids:?}");
    let picture_pid = *pids.first().expect("the picture's pid");
    let voice_pid = *pids.last().expect("the take's pid");

    // Now only the TAKE has ended — exactly what a synthesis shorter than its clip looks like from the
    // tick's side. A SIGKILL is queued, not applied, so give it a bounded beat to land.
    unsafe { libc::kill(voice_pid as libc::pid_t, libc::SIGKILL) };
    for _ in 0..100 {
        if !alive(voice_pid) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!alive(voice_pid), "the take's player is gone: pid {voice_pid}");
    let stepped = leg::tick_loop(&session.tree, &cut, &entries, 3.0, 1.0, 1.0, false, spawn);
    assert!(
        !matches!(stepped, leg::Stepped::Paused),
        "a take that finished first is not the end of the preview, got {stepped:?}"
    );
    assert!(alive(picture_pid), "the picture keeps playing: pid {picture_pid} was stopped");
    assert!(
        leg::running_children().contains(&picture_pid),
        "and the preview still owns it: {:?}",
        leg::running_children()
    );
    reap(&spawned);
}

/// The same rule, over the door the page presses: the running preview's picture exits, and the next
/// beat ends the take rather than walking on over a frozen frame.
#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s8_the_running_preview_notices_its_picture_exit(
) {
    common::pin_private_cwd();
    let session = session("running-ends");
    let spawned: Pids = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let entries = vec![entry(0.0, 10.0, "first line")];
    let key = naivepost::narration::key_for(&entries[0], Some(""));
    write_bytes(
        &naivepost::narrate_tts::take_path(&session.tree, &key),
        &harness::wav_bytes(4096),
    );
    let cut = leg::cut_of(vec![Seg { s: 0.0, e: 10.0, dur: 10.0, ..Default::default() }], vec![]);
    naivepost::cut::save(&cut, &session.tree).expect("cut written");
    // The picture's cue needs a recording on disk, spelled the way a one-camera session spells it.
    std::fs::write(std::env::current_dir().expect("cwd").join("cam0.mp4"), b"stub footage")
        .expect("footage on disk");
    naivepost::project::save(
        &naivepost::project::Project {
            sources: vec![naivepost::project::Source {
                path: "cam0.mp4".into(),
                footage: true,
                ..Default::default()
            }],
            ..Default::default()
        },
        session.tree.dir(),
    )
    .expect("project written");

    // First beat: a live picture, so the take is spawned for real and both players are owned.
    let mut spawn_fn = {
        let recorded = spawned.clone();
        move |_program: &str, _args: &[String]| {
            let child = sleeper()?;
            recorded.borrow_mut().push(child.id());
            Ok(child)
        }
    };
    let spawn: &mut Spawn = &mut spawn_fn;
    let said = leg::start_running(&session.tree, &cut, &entries, 0.0, 1.0, 1.0, spawn)
        .expect("the preview starts over the session's own footage");
    assert!(said.contains("playing"), "started: {said}");
    // The take needs the voice player to appear, and it only does once the tick walks into the line.
    let first = leg::tick_loop(&session.tree, &cut, &entries, 0.6, 1.0, 1.0, false, spawn);
    assert!(matches!(first, leg::Stepped::Speaking { .. }), "the tick entered the line: {first:?}");
    let pids = leg::running_children();
    assert_eq!(pids.len(), 2, "picture and take are both running: {pids:?}");
    let voice_pid = *pids.last().expect("the take's pid");

    // Now the picture has ended for real: SIGKILL the PLAYER (the first pid is the picture's, `start_running`
    // spawned it before the tick spawned the take) and let the next beat notice, which is exactly what a
    // player that ran to its end looks like from the tick's side.
    let picture_pid = *pids.first().expect("the picture's pid");
    unsafe { libc::kill(picture_pid as libc::pid_t, libc::SIGKILL) };
    // One beat of grace for the queued signal to land (bounded: no signal landing is the failure).
    for _ in 0..100 {
        if !alive(picture_pid) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ended = leg::tick_loop(&session.tree, &cut, &entries, 3.0, 1.0, 1.0, false, spawn);
    assert!(matches!(ended, leg::Stepped::Paused), "the tick reports the ending: {ended:?}");
    assert!(
        !alive(voice_pid),
        "§6's `when the picture stops, narration stops too`: pid {voice_pid} outlived it"
    );
    assert_eq!(
        leg::running_children(),
        Vec::<u32>::new(),
        "both players are handed back, none left to kill"
    );
    reap(&spawned);
}

/// §6's exception: a line auditioned over a clip with NO footage speaks against a still frame, so the
/// picture is cued and FROZEN rather than left out. The freeze is read off `/proc` as `T`.
#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s8_a_line_over_no_footage_holds_a_still_frame(
) {
    common::pin_private_cwd();
    let session = session("still-frame");
    // A cut with one clip and nothing on disk to play it: `cue_files` answers empty, which is the case.
    let cut = leg::cut_of(vec![Seg { s: 0.0, e: 10.0, dur: 10.0, ..Default::default() }], vec![]);
    let spawned: Pids = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut spawn_fn = {
        let recorded = spawned.clone();
        move |_program: &str, _args: &[String]| {
            let child = sleeper()?;
            recorded.borrow_mut().push(child.id());
            Ok(child)
        }
    };
    let spawn: &mut Spawn = &mut spawn_fn;

    // A cue with no footage and no still-frame freeze is still the refusal the page has always answered.
    let plain = media::cue_picture(
        "player",
        &session.tree,
        &cut,
        4.0,
        0.5,
        media::Freeze::Play,
        spawn,
    );
    assert!(
        plain.is_err(),
        "with nothing to show a normal cue refuses rather than claim a picture: {plain:?}"
    );

    // The still-frame cue starts a player AND freezes it: a frame that holds while the line speaks.
    let (child, cue): (Child, Cue) =
        leg::still_frame(&session.tree, &cut, 4.0, 0.0, spawn).expect("the still frame is cued");
    assert_eq!(cue.pid, child.id(), "the cue names the child it spawned");
    assert_eq!(proc_state_after_stop(child.id()), 'T', "the frame is held, not played");
    assert!(alive(child.id()), "a frozen player is still there to be resumed or killed");
    reap(&spawned);
}
