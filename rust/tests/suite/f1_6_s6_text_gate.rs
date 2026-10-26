//! F1.6 S6, the text gate (spec/04-prepare.md): the pixel pass's scene changes judged by the decision model, a
//! change it called the same text dropped, and a text change it never saw found inside its scene by halving,
//! within a second. Then F1.7 describes by the gate's changes. Pure half: [`naivepost::text_gate::run`] over a
//! scripted screen. Live half: the real judge through Settings' address against a fake server that answers by
//! comparing the two pictures it is shown, so nothing hands the program a canned verdict.

use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use naivepost::clock::frame_name;
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::settings::{self, Conf, Paths};
use naivepost::text_gate::{self, Found};

use crate::common;

/// The screen's text at each second: A until 12, B until 47.3, C after; a scroll from 70 to 71 (D, E, F).
fn text_at(at: f64) -> &'static str {
    match at {
        t if t < 12.0 => "A",
        t if t < 47.3 => "B",
        t if t < 70.0 => "C",
        t if t < 70.5 => "D",
        t if t < 71.0 => "E",
        _ => "F",
    }
}

#[test]
fn f1_6_s6_the_gate_keeps_real_changes_drops_false_ones_and_finds_quiet_ones() {
    // The pixel pass saw the slide switch at 12 and the scroll's start, missed the quiet change at 47.3, and
    // called a webcam move at 30 a scene change.
    let rough = [12.0, 30.0, 70.0];
    let mut asked = 0;
    let mut judge = |a: f64, b: f64| -> Result<bool, String> {
        asked += 1;
        Ok(text_at(a) != text_at(b))
    };
    let gate = text_gate::run(&rough, 90.0, 0.25, &mut judge, &mut |_| {}).expect("the gate runs");
    let at: Vec<f64> = gate.changes.iter().map(|(at, _)| *at).collect();
    assert_eq!(
        (gate.kept, gate.dropped),
        (2, 1),
        "12 and 70 kept, the webcam's 30 dropped: {gate:?}"
    );
    assert!(
        at.iter().all(|t| (*t - 30.0).abs() > 1.0),
        "nothing at the webcam move: {at:?}"
    );
    assert!(
        at.iter().any(|t| (12.0..=12.25).contains(t)),
        "the slide switch: {at:?}"
    );
    let quiet = gate
        .changes
        .iter()
        .find(|(t, found)| *found == Found::Text && (47.3..=48.3).contains(t));
    assert!(
        quiet.is_some(),
        "the quiet change at 47.3 found within a second: {gate:?}"
    );
    let scroll: Vec<f64> = at
        .iter()
        .copied()
        .filter(|t| (69.0..73.0).contains(t))
        .collect();
    assert_eq!(
        scroll.len(),
        1,
        "the scroll is one change, not three: {at:?}"
    );
    assert!(scroll[0] >= 71.0, "kept as its settled end: {scroll:?}");
    assert_eq!(gate.calls, asked);
    assert!(
        asked < 40,
        "a handful of judges for 90 s, not one per frame: {asked}"
    );
}

/// The gate judges for minutes on a long recording: it says how far it has got after every judge, so the bar
/// moves inside its stage and the status line names the place, instead of standing on the recording's name.
#[test]
fn f1_6_s6_the_gate_says_how_far_it_has_got_after_every_judge() {
    let mut asked = 0;
    let mut judge = |a: f64, b: f64| -> Result<bool, String> {
        asked += 1;
        Ok(text_at(a) != text_at(b))
    };
    let mut reports: Vec<text_gate::Reached> = Vec::new();
    let gate = text_gate::run(&[12.0, 30.0, 70.0], 90.0, 0.25, &mut judge, &mut |at| {
        reports.push(at)
    })
    .expect("the gate runs");
    let fractions: Vec<f64> = reports.iter().map(|at| at.fraction()).collect();
    assert!(
        reports.len() >= 3 + 4,
        "one report per pixel change and per probed pair: {reports:?}"
    );
    assert!(
        fractions.windows(2).all(|pair| pair[1] >= pair[0]),
        "the bar only moves forward: {fractions:?}"
    );
    assert!(
        reports
            .iter()
            .filter(|at| at.pass == 1)
            .all(|at| at.fraction() <= 0.5),
        "the first pass fills the first half: {reports:?}"
    );
    let last = reports.last().unwrap();
    assert!(
        last.fraction() > 0.95,
        "the last probe ends the bar: {last:?}"
    );
    assert_eq!(last.judges, gate.calls);
    assert_eq!(
        last.words(),
        format!("pass 2 of 2, 1:30 of 1:30, {} judge(s)", gate.calls)
    );
}

#[test]
fn f1_6_s6_a_judge_that_never_answers_leaves_the_pixel_pass() {
    let mut judge = |_: f64, _: f64| -> Result<bool, String> { Err("connection refused".into()) };
    let gave_up = text_gate::run(&[12.0, 30.0], 90.0, 0.25, &mut judge, &mut |_| {});
    assert_eq!(
        gave_up,
        Err("connection refused".to_string()),
        "three dead calls and it stops"
    );
}

/// A fake LLM: each chat ask carries two pictures; it calls `judge_text` with whether their bytes differ, and
/// keeps the model each ask named, in order.
fn fake_judge() -> (String, Arc<Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let asks = Arc::new(Mutex::new(Vec::<String>::new()));
    let counted = asks.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
                if header.to_ascii_lowercase().starts_with("content-length:") {
                    length = header[15..].trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let images: Vec<String> = body["messages"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
                .filter_map(|part| part["image_url"]["url"].as_str().map(str::to_string))
                .collect();
            let changed = images.len() == 2 && images[0] != images[1];
            counted
                .lock()
                .unwrap()
                .push(body["model"].as_str().unwrap_or_default().to_string());
            let args = format!("{{\\\"changed\\\":{changed},\\\"what\\\":\\\"compared\\\"}}");
            let answer = format!(
                "{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[{{\"function\":{{\"name\":\"judge_text\",\"arguments\":\"{args}\"}}}}]}}}}]}}"
            );
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                answer.len()
            );
        }
    });
    (url, asks)
}

/// A project with one 60 s lecture: its frames on the 250 ms grid (each picture's bytes its text), the scaled
/// copies beside them, and a pixel pass that saw the switch at 12 and a webcam move at 30 that changed no text.
fn lecture_scene(
    url: String,
) -> (
    std::path::PathBuf,
    std::path::PathBuf,
    Paths,
    Conf,
    Project,
    Tree,
) {
    let root = common::temp_dir_tagged("text-gate");
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(session.join("sources")).unwrap();
    std::fs::write(session.join("sources/lecture.mkv"), b"mkv").unwrap();
    let paths = Paths {
        config_dir: root.join("cfg/naivepost"),
        data_dir: root.join("data/naivepost"),
    };
    let mut conf = Conf::default();
    conf.server = url;
    conf.model = "qwen".into();
    let project = Project {
        sources: vec![Source {
            path: "project:sources/lecture.mkv".into(),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    naivepost::project::save(&project, &session).unwrap();
    let tree = Tree::new(&session).unwrap();
    let (frames, scaled) = (tree.frames_dir("lecture"), tree.llm_frames_dir("lecture"));
    std::fs::create_dir_all(&frames).unwrap();
    std::fs::create_dir_all(&scaled).unwrap();
    for step in 0..240 {
        let at = step as f64 * 0.25;
        std::fs::write(frames.join(format!("{}.jpg", frame_name(at))), text_at(at)).unwrap();
        std::fs::write(
            scaled.join(naivepost::describe::scaled_name(0.0, at)),
            text_at(at),
        )
        .unwrap();
    }
    std::fs::write(tree.scenes_tsv("lecture"), "12.000\t9.1\n30.000\t1.2\n").unwrap();
    settings::save(&paths, &conf).unwrap();
    (root, session, paths, conf, project, tree)
}

/// The walk shows the gate's progress: while the one recording's row runs, the bar moves inside it and the status
/// line says the pass, the place and the judges, not only the recording's name.
#[test]
fn f1_6_s6_the_walk_moves_the_bar_while_the_gate_judges() {
    let _scene = common::scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (url, _asks) = fake_judge();
    let (root, session, _paths, _conf, project, _tree) = lecture_scene(url);
    std::env::set_var("XDG_CONFIG_HOME", root.join("cfg"));
    std::env::set_var("XDG_DATA_HOME", root.join("data"));
    naivepost::startup::set_open_dir(&session);

    let mut queue = naivepost::runqueue::Queue::new();
    let mut seen: Vec<(f64, String)> = Vec::new();
    let drive = naivepost::run_stages::drive_walk_watched_for_test(
        &[text_gate::stage(&project)],
        &mut queue,
        |_| {},
        &[],
        &mut |queue, _| seen.push((queue.fraction(), queue.text_and_tip().0)),
    );
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");

    assert!(
        matches!(drive, naivepost::run_stages::Drive::Done { ran: 1, .. }),
        "{drive:?}"
    );
    // The row names itself the moment it starts, before its first judge: the walk once kept the label of the
    // task that had just ENDED on the status line for as long as the next one ran.
    let first = seen
        .iter()
        .find(|(_, line)| line.contains("lecture"))
        .expect("the status named the recording");
    assert!(
        first.1.ends_with("reading the text of lecture"),
        "the first status for the row is its own label: {seen:?}"
    );
    let inside: Vec<&(f64, String)> = seen
        .iter()
        .filter(|(fraction, _)| *fraction > 0.0 && *fraction < 1.0)
        .collect();
    assert!(
        inside.len() >= 5,
        "the bar moved inside the one row, once per judge: {seen:?}"
    );
    assert!(
        inside.windows(2).all(|pair| pair[1].0 >= pair[0].0),
        "and only forward: {seen:?}"
    );
    assert!(
        inside.iter().any(|(_, line)| line
            .contains("reading the text of lecture -- pass 2 of 2, ")
            && line.contains(" of 1:00, ")),
        "the status line says where the gate is: {seen:?}"
    );
}

#[test]
fn f1_6_s6_the_live_gate_writes_textscenes_and_describe_reads_them() {
    let (url, asks) = fake_judge();
    let (_root, _session, paths, mut conf, _project, tree) = lecture_scene(url);

    // No decision model: the gate still runs, judged by the main model.
    let mut said = Vec::new();
    let main = text_gate::gate_source(
        &tree,
        &paths,
        &conf,
        "lecture",
        0,
        &mut |l| said.push(l.to_string()),
        &mut |_| {},
    )
    .expect("the gate runs without a decision model");
    let before = asks.lock().unwrap().len();
    assert!(before > 0, "the main model was asked");
    assert_eq!(before, main.calls, "every judge went over the wire");
    assert!(
        asks.lock().unwrap().iter().all(|model| model == "qwen"),
        "no decision model, so the main model judged: {:?}",
        asks.lock().unwrap()
    );
    said.clear();

    conf.decision_model = "clef".into();
    settings::save(&paths, &conf).unwrap();
    let mut reports = Vec::new();
    let gate = text_gate::gate_source(
        &tree,
        &paths,
        &conf,
        "lecture",
        0,
        &mut |l| said.push(l.to_string()),
        &mut |at| reports.push(at),
    )
    .expect("the gate ran");
    let at: Vec<f64> = gate.changes.iter().map(|(at, _)| *at).collect();
    assert_eq!(
        gate.dropped, 1,
        "the webcam's 30 was the same text: {said:?}"
    );
    assert!(
        at.iter().any(|t| (47.3..=48.3).contains(t)),
        "the quiet change found: {at:?}"
    );
    assert!(
        said.iter()
            .any(|l| l.contains("text gate:") && l.contains("1 dropped")),
        "said: {said:?}"
    );
    let asked = asks.lock().unwrap().len();
    assert_eq!(
        asked - before,
        gate.calls,
        "the decision model is a different cache key, so it was asked anew"
    );
    let last = reports.last().expect("the gate said how far it had got");
    assert_eq!(
        last.judges, gate.calls,
        "the last report counts every judge: {last:?}"
    );
    assert_eq!(last.pass, 2, "it ended in the probing pass: {last:?}");
    assert!(
        asks.lock().unwrap()[before..]
            .iter()
            .all(|model| model == "clef"),
        "every judge went to the decision model"
    );

    // F1.7 describes by the gate's changes.
    let described = naivepost::describe::read_changes(&tree, "lecture");
    assert_eq!(
        described,
        at.iter()
            .map(|t| (t * 1000.0).round() / 1000.0)
            .collect::<Vec<_>>()
    );
    assert!(std::fs::read_to_string(text_gate::path(&tree, "lecture"))
        .unwrap()
        .contains("\ttext\n"));

    // A rerun asks nothing: every answer is in the cache, keyed on the two pictures.
    text_gate::gate_source(&tree, &paths, &conf, "lecture", 0, &mut |_| {}, &mut |_| {})
        .expect("the gate ran again");
    assert_eq!(
        asks.lock().unwrap().len(),
        asked,
        "the rerun was answered from the cache"
    );

    // A newer pixel pass makes the gate's file stale: describe goes back to it until the gate runs again.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(tree.scenes_tsv("lecture"), "12.000\t9.1\n").unwrap();
    assert_eq!(
        naivepost::describe::read_changes(&tree, "lecture"),
        vec![12.0]
    );
}
