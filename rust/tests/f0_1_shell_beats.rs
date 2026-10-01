//! §03-shell.md F0.1 S3 + §2: typing reaches `narration.json` on the beat; a run's line goes to the status line.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::layout::Tree;
use naivepost::narration::{self, Entry};
use naivepost::project::{self, Project, Source};
use naivepost::runqueue::{self, TRACK_DESCRIBE};
use naivepost::shell::{self, NARRATION_AUTOSAVE};
use naivepost::startup;
use naivepost::ui;

/// What the seeded line says, so the box assertion names the same bytes.
const BEFORE: &str = "written before the window existed";

/// Scratch root + session tree: the session folder resolves against the process's cwd, so move first.
fn scratch_tree(project: &Project) -> (PathBuf, Tree) {
    let dir = std::env::temp_dir().join(format!("naivepost-f01beats-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir); // a rerun on this pid starts clean
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::env::set_current_dir(&dir).expect("chdir -- the test thread is alone here");
    let sess = startup::session_dir(&std::env::current_dir().unwrap());
    project::save(project, &sess).expect("session project saved");
    (dir, Tree::new(&sess).expect("session.naivepost is a project folder"))
}

/// A footage source (unlocks every tab touched here) and the one line the Narrate row is drawn from.
fn seeded_project() -> (Project, Vec<Entry>) {
    let src = Source { path: "project:sources/lecture.mkv".into(), footage: true, ..Default::default() };
    let line = Entry { s: 5.0, e: 12.0, text: BEFORE.into(), ..Default::default() };
    (Project { sources: vec![src], ..Default::default() }, vec![line])
}
/// Write the cut clip and the narration line the rows are drawn from.
fn seed_session(tree: &Tree, lines: &[Entry]) {
    let l = &lines[0];
    let clip = cut::Cut { segs: vec![Seg { s: l.s, e: l.e, ..Default::default() }], ..Default::default() };
    cut::save(&clip, tree).expect("cut written");
    narration::save(&narration::Narration { entries: lines.to_vec(), ..Default::default() }, tree).unwrap();
}
/// One GTK app for the whole binary: `gio` refuses a second `run_with_args`, so every check shares it.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder().application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE).build();
        app.connect_activate(|app| {
            check_autosave_beat(app);
            RAN_A.store(true, Ordering::SeqCst);
            check_run_line(app);
            RAN_B.store(true, Ordering::SeqCst);
            check_the_pulse_belongs_to_the_press(app);
            RAN_PULSE.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}
static RAN_A: AtomicBool = AtomicBool::new(false);
static RAN_B: AtomicBool = AtomicBool::new(false);
static RAN_PULSE: AtomicBool = AtomicBool::new(false);
/// F0.1 S3: nothing here flushes; only the loop is driven, so the timer alone decides when the file changes.
fn check_autosave_beat(app: &adw::Application) {
    let (project, lines) = seeded_project();
    let (dir, tree) = scratch_tree(&project);
    seed_session(&tree, &lines);
    let window = ui::build_window(app, &project, "Narrate");
    window.present();
    ui::set_state(naivepost::ui::NarrateState { entries: lines, ..Default::default() }); // as on arrival
    ui::refresh(&window);
    let typed = "the half-typed line";
    assert!(!ui::note_typing(&window, 0, typed).is_empty(), "the row's handler took the typing");
    assert!(ui::read_state().entries[0].text.contains("half-typed"), "typing lands in the record at once");
    assert!(!typed_written(&tree, "half-typed"), "S3 saves 400 ms after typing, not during it");
    let entry = line_entry(&window, 0);
    assert_eq!(entry.text(), BEFORE, "the box starts from the file");
    entry.set_text(typed);
    step_past_beat();
    assert!(typed_written(&tree, "half-typed"), "the timer wrote on the beat; the box says {:?}", entry.text());
    let _ = std::fs::remove_dir_all(&dir);
}
/// Past the beat in short steps, running the loop: the standing timer's `writable` answer decides the write.
fn step_past_beat() {
    let ctx = glib::MainContext::default();
    for _ in 0..32 { ui::shell_beats::advance_clock_for_test(NARRATION_AUTOSAVE / 32); ctx.iteration(true); }
}
/// The row's live box; setting its text is what makes `changed` run, as a keystroke does.
fn line_entry(window: &adw::ApplicationWindow, index: usize) -> gtk::Entry {
    ui::find_source_widget(window, &format!("line-text-{index}"))
        .and_then(|w| w.downcast().ok()).expect("that line has a text box")
}
/// Whether `narration.json` carries `needle` -- the file the program writes, no test-side flush between.
fn typed_written(tree: &Tree, needle: &str) -> bool {
    std::fs::read_to_string(tree.narration_json()).map(|t| t.contains(needle)).unwrap_or(false)
}

/// §2: the queue's joined track line goes to the status line (the bar draws no text), an idle bar takes
/// nothing over, and a counted fraction reaches the bar through ⏹'s own repaint.
fn check_run_line(app: &adw::Application) {
    let (project, _) = seeded_project();
    let (dir, _) = scratch_tree(&project);
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    let queue = ui::run_queue(&window).expect("the window carries a run queue");
    fill(&queue);
    let line = queue.borrow().text();
    assert!(line.contains("describe 1/2: chunk 4/12"), "§2 joins the tracks' lines: {line:?}");
    let status = ui::find_status(window.upcast_ref()).expect("the log header carries the status line");
    ui::shell_beats::status_from_queue(&status, &queue.borrow(), true);
    assert_eq!(ui::state(&window).status, line, "§2: the tracks' line lands on the status line");
    let bar = ui::progress_bar(&window).expect("the run bar has a progress widget");
    assert!(bar.text().is_none() || !bar.shows_text(), "§1 badge 13: the bar draws no text of its own");
    status.set_text(shell::CUT_LOCK); // an idle bar never takes a standing sentence over
    ui::shell_beats::status_from_queue(&status, &queue.borrow(), false);
    assert_eq!(ui::state(&window).status, shell::CUT_LOCK, "an idle bar takes nothing over");
    open_run(&window, &queue, &project); // `start_run` resets the queue, so work is reported after it
    fill(&queue);
    ui::stop_button(&window).expect("the run bar has a stop button").emit_clicked();
    let shown = ui::state(&window);
    assert!(!shown.status.is_empty(), "⏹ answers the press on the status line");
    assert!(shown.progress > 0.0, "and the counted fraction reached the bar");
    let _ = std::fs::remove_dir_all(&dir);
}
/// §2 through the widgets: ▶ is what stands the idle pulse, ⏹ is what takes it down, and the bar itself
/// draws no text. Nothing here pumps the loop between the press and the assertion, so the timer that a
/// press armed cannot have retired itself -- its standing is the press's doing, not the loop's.
fn check_the_pulse_belongs_to_the_press(app: &adw::Application) {
    let (project, _) = seeded_project();
    let (dir, _) = scratch_tree(&project);
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    assert!(!ui::shell_beats::pulse_standing_for_test(), "an idle bar holds no pulse timer");

    ui::play_button(&window).expect("the run bar has a \u{25b6}").emit_by_name::<()>("clicked", &[]);
    assert!(ui::shell_beats::pulse_standing_for_test(), "\u{25b6} on a page with nothing countable yet pulses the bar");
    let bar = ui::progress_bar(&window).expect("the run bar has a progress widget");
    assert!(bar.text().is_none() || !bar.shows_text(), "\u{a7}1 badge 13: the bar draws no text of its own");

    ui::stop_button(&window).expect("the run bar has a \u{23f9}").emit_by_name::<()>("clicked", &[]);
    assert!(!ui::shell_beats::pulse_standing_for_test(), "\u{23f9} took the pulse down with the run");
    let _ = std::fs::remove_dir_all(&dir);
}
/// The call ▶ makes, on this window's own bar and queue; `start_run` resets the queue as it opens.
fn open_run(window: &adw::ApplicationWindow, queue: &Rc<RefCell<runqueue::Queue>>, project: &Project) {
    let (bar, mut log) = (ui::run_bar(window).expect("the window carries a run bar"),
        naivepost::exchanges::RunLog::default());
    let run = runqueue::start_run(&mut bar.borrow_mut(), &mut queue.borrow_mut(), &mut log,
        naivepost::run::Step::Prepare, naivepost::run::snapshot_sources(project));
    assert!(run.epoch > 0, "the run opened on this window's own bar");
}
/// One describe track, a quarter done -- §2's example line, on the window's own queue.
fn fill(queue: &Rc<RefCell<runqueue::Queue>>) {
    let [q] = [&mut *queue.borrow_mut()];
    q.job(TRACK_DESCRIBE, "describe", 1, 2);
    q.push(TRACK_DESCRIBE, 12, "chunk");
    q.prog(TRACK_DESCRIBE, 4.0 / 12.0, "chunk 4/12");
}
/// F0.1 S3 + §2, pure: the spacing and the pulse are the shell's numbers; nothing owed means no timer.
#[test]
fn f0_1_s3_the_beat_spacing() {
    let mut pending = shell::Pending::default();
    let at = Duration::from_millis(10_000);
    pending.touched(at);
    assert!(pending.owe(), "typing owes a write");
    assert!(!pending.writable(at + NARRATION_AUTOSAVE - Duration::from_millis(1)), "not a ms early");
    assert!(pending.writable(at + NARRATION_AUTOSAVE), "and lands on the beat");
    assert!(pending.flush() && !pending.flush(), "the write happens once, never twice");
    assert!(!ui::shell_beats::needs_timer(false, Some(at)), "nothing owed, no timer");
    assert!(!ui::shell_beats::needs_timer(true, None), "no deadline, no timer");
    assert!(ui::shell_beats::needs_timer(true, Some(at)));
    // §2: the bar pulses only while a run holds nothing countable.
    assert_eq!(runqueue::PROGRESS_PULSE, Duration::from_millis(150));
    assert!(ui::shell_beats::should_pulse(true, 0.0), "a model thinking pulses");
    assert!(!ui::shell_beats::should_pulse(true, 0.5), "countable work needs no pulse");
    assert!(!ui::shell_beats::should_pulse(false, 0.0), "an idle bar never pulses");
}
/// Both window checks share one `gio` app (a second `run_with_args` is refused); each asserts it ran.
#[test]
fn f0_1_s3_typing_reaches_the_file_on_the_beat() {
    window_round();
    assert!(RAN_A.load(Ordering::SeqCst), "the autosave check never ran");
}
#[test]
fn sec_02_a_runs_lines_go_to_the_status_line() {
    window_round();
    assert!(RAN_B.load(Ordering::SeqCst), "the run-line check never ran");
}
#[test]
fn f0_1_s3_the_bar_pulses_only_while_the_run_thinks() {
    window_round();
    assert!(RAN_PULSE.load(Ordering::SeqCst), "the pulse check never ran");
}
