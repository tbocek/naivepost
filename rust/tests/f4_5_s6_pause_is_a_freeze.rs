//! F4.5 S6, at the leg's own doors: pausing a preview FREEZES both players and a thaw walks the SAME
//! ones on, instead of killing them and re-cueing (a re-cue is a black frame where a still belongs).
//!
//! No GTK and no settings here — `start_running`/`stop_running`/`hold_paused` take the spawn seam as an
//! argument, which is the same injection point the program uses for its own player (`spawn_player`). The
//! test hands it a real long-lived child so liveness and frozen-ness are read off `/proc`, not asserted
//! from a flag: a fake that only records calls could not tell SIGSTOP from SIGKILL.

use std::process::{Child, Stdio};

use naivepost::cut::Seg;
use naivepost::narrate_preview_leg::{self as leg, Spawn};

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;

use common::entry_at_mid as entry;

/// A session folder under /tmp: enough for `Tree::new` and for the footage check to answer "no recording",
/// which is what makes the picture's spawn the ONLY child this scenario has to keep alive.
struct Session {
    dir: std::path::PathBuf,
    tree: naivepost::layout::Tree,
}

fn session(tag: &str) -> Session {
    let dir = std::env::temp_dir().join(format!("naivepost-f45freeze-{tag}.naivepost"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("session folder");
    Session {
        tree: naivepost::layout::Tree::new(&dir).expect("a .naivepost folder is a project"),
        dir,
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The state letter in `/proc/<pid>/stat`: R running, S sleeping, T stopped by a signal, Z a zombie.
/// Read straight from the file — the whole point is what the OS says about the process.
fn proc_state(pid: u32) -> char {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    // The comm field is parenthesised and may itself hold spaces: the state follows the LAST ')'.
    match stat.rfind(')') {
        Some(end) => stat[end + 1..].trim_start().chars().next().unwrap_or('?'),
        None => '?',
    }
}

/// Whether the pid is still there at all (not reaped, not gone).
fn alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// The state letter once a `SIGSTOP` has actually landed: a signal is queued, not applied, so reading
/// `/proc` on the same instruction the kill returned can still show it running. Bounded — a player that
/// never stops is the failure this test exists to catch, and it must fail rather than wait.
fn proc_state_after_stop(pid: u32) -> char {
    for _ in 0..100 {
        let state = proc_state(pid);
        if state != 'R' && state != 'D' {
            return state;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    proc_state(pid)
}

/// A player that outlives a pause: `sleep 30`, piped so nothing waits on it.
fn sleeper() -> Result<Child, String> {
    std::process::Command::new("sleep")
        .arg("30")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| err.to_string())
}

/// The pids of every player this scenario spawned, so no `sleep` outlives the test whichever way it ends.
/// Shared with the spawn closure through an `Rc` because the `Spawn` trait object it is handed lives for
/// `'static`.
type Pids = std::rc::Rc<std::cell::RefCell<Vec<u32>>>;

fn reap(pids: &Pids) {
    for pid in pids.borrow_mut().drain(..) {
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    }
}

#[test]
fn f4_5_s6_pause_freezes_both_players_and_a_thaw_walks_them_on() {
    common::pin_private_cwd();
    let session = session("freeze");
    let cut = leg::cut_of(
        vec![
            Seg { s: 0.0, e: 10.0, ins: "a.mp4".into(), dur: 10.0, ..Default::default() },
            Seg { s: 10.0, e: 20.0, ins: "b.mp4".into(), dur: 10.0, ..Default::default() },
        ],
        vec![],
    );
    let entries = vec![entry(0.0, 10.0, "first line")];
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

    // Nothing running yet: the press starts one, it does not freeze an empty slot.
    assert_eq!(leg::hold_paused(true), None, "with no preview there is nothing to freeze");

    // The footage has to exist for the picture's cue to spawn at all (S1 refuses a cue onto nothing), so
    // write what a real session has: the cut and one recording its camera row names.
    naivepost::cut::save(&cut, &session.tree).expect("cut written");
    // `footage_path` resolves a relative source against the process's cwd, so the recording lives here.
    std::fs::write(std::env::current_dir().expect("cwd").join("cam0.mp4"), b"stub footage")
        .expect("footage on disk");
    let project = naivepost::project::Project {
        sources: vec![naivepost::project::Source {
            path: "cam0.mp4".into(),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    naivepost::project::save(&project, session.tree.dir()).expect("project written");

    // Start through the door the page's own ▶ uses.
    let said = leg::start_running(&session.tree, &cut, &entries, 0.0, 0.5, 1.0, spawn)
        .expect("the preview started");
    assert!(said.contains("playing the cut from"), "started with: {said}");
    let pids = leg::running_children();
    assert_eq!(pids.len(), 1, "one player is running: {pids:?}");
    let pid = pids[0];
    assert!(alive(pid), "the player was spawned for real");
    assert_ne!(proc_state(pid), 'T', "it plays, it does not start frozen");

    // ⏸ freezes it: SIGSTOP, so the process is still there holding its place.
    assert_eq!(leg::hold_paused(true), Some(true), "freezing a running preview reports the hold");
    assert!(alive(pid), "a pause keeps the player alive");
    assert_eq!(proc_state_after_stop(pid), 'T', "the player is stopped, not killed");
    assert_eq!(leg::running_children(), vec![pid], "⏹ still owns the frozen player");

    // ▶ thaws the SAME player rather than building a new pipeline.
    assert_eq!(leg::hold_paused(false), Some(true), "something WAS held, so this press walks on");
    assert_eq!(leg::running_children(), vec![pid], "the thaw walks the same player on");
    assert!(matches!(proc_state(pid), 'S' | 'R'), "the player runs again: {}", proc_state(pid));

    // A second ▶ over a preview that was already playing answers false: that press must re-cue.
    assert_eq!(leg::hold_paused(false), Some(false), "nothing was held, so the next press re-cues");

    // ⏹ reaches a HELD player too (§F4.5 S6's ⏹ stops both players, frozen or not) — which is what makes
    // a freeze safe rather than a leak — and ends it for real. Freeze once more first: the hold flag lives
    // in the slot, so this is the state ⏹ must be able to end.
    assert_eq!(leg::hold_paused(true), Some(true), "⏸ over an already-held preview still reports a hold");
    let (_stopped, killed) = leg::stop_running();
    assert_eq!(killed, vec![pid], "⏹ names the player it ended");
    // The slot is empty now, and `stop` clears the held flag with it: after ⏹ there is nothing to freeze,
    // so a ▶ on the next press starts a fresh preview rather than thawing one that no longer exists.
    assert_eq!(leg::hold_paused(true), None, "with nothing running there is nothing to freeze");
    assert!(leg::running_children().is_empty(), "the slot is empty after ⏹");
    reap(&spawned);
}
