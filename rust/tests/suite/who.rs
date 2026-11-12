//! spec/04-prepare.md F1.16: who is who — each speaker's name and camera, from the User Context, a frame
//! of each camera and the talk.

use std::fs;
use std::path::Path;

use crate::common;
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::textfmt::SessionLine;
use naivepost::who::{self, Person};

fn line(start: f64, end: f64, who: &str, text: &str) -> SessionLine {
    SessionLine {
        start,
        end,
        source: "cam2".to_string(),
        who: who.to_string(),
        text: text.to_string(),
    }
}

fn talk() -> Vec<SessionLine> {
    vec![
        line(
            0.0,
            4.0,
            "SPEAKER_01",
            "Welcome. With me today is Ada. What do you work on?",
        ),
        line(
            4.5,
            40.0,
            "SPEAKER_00",
            "Thanks for having me. I build compilers, mostly.",
        ),
        line(40.5, 43.0, "SPEAKER_01", "And why compilers?"),
        line(
            43.5,
            90.0,
            "SPEAKER_00",
            "Because every program goes through one.",
        ),
        line(10.0, 12.0, "EVENT", "Calm; same view."),
    ]
}

fn cameras() -> Vec<String> {
    vec!["cam2".to_string(), "cam1".to_string()]
}

/// S2: the speakers in the order of their first word, with how long each talks and how often they ask.
#[test]
fn f1_16_s2_the_speakers_are_counted_from_the_session() {
    let found = who::speakers(&talk());
    assert_eq!(found.len(), 2, "the EVENT line is nobody's voice");
    assert_eq!(found[0].id, "SPEAKER_01", "the first heard comes first");
    assert_eq!(found[0].questions, 2);
    assert!((found[0].seconds - 6.5).abs() < 1e-9);
    assert_eq!(found[1].questions, 0);
    assert!((found[1].seconds - 82.0).abs() < 1e-9);
}

/// S2: the ask carries the context, the cameras by name, the speakers' tallies and the first lines.
#[test]
fn f1_16_s2_the_brief_holds_what_the_model_needs() {
    let brief = who::brief("I am the host, in the grey hoodie.", &cameras(), &talk());
    assert!(brief.contains("USER CONTEXT:\nI am the host, in the grey hoodie."));
    assert!(brief.contains("CAMERAS: cam2, cam1"));
    let tally = brief
        .lines()
        .find(|line| line.starts_with("SPEAKER_01: talks for "))
        .expect("the first speaker's tally");
    assert!(
        tally.ends_with("in the whole session, 2 of their lines are questions"),
        "{tally}"
    );
    assert!(brief.contains("] SPEAKER_01: Welcome. With me today is Ada. What do you work on?\n"));
    assert!(
        !brief.contains("Calm; same view."),
        "pictures' events are not talk"
    );
}

/// S3: a good answer is taken, fenced or not; every way it can be wrong is refused with the sentence the
/// model is sent back.
#[test]
fn f1_16_s3_the_answer_is_checked_against_the_session() {
    let heard = who::speakers(&talk());
    let good = "```json\n{\"people\":[\
        {\"speaker\":\"SPEAKER_01\",\"name\":\"Thomas\",\"camera\":\"cam2\",\"because\":\"asks; grey hoodie\"},\
        {\"speaker\":\"SPEAKER_00\",\"name\":\"Ada\",\"camera\":\"cam1\",\"because\":\"answers\"}]}\n```";
    let who = who::parse(good, &heard, &cameras()).expect("a fenced answer is read");
    assert_eq!(who.people[0].camera, "cam2");
    assert_eq!(
        who::said(&who.people[0]),
        ">>> who: SPEAKER_01 is Thomas, shown on cam2 -- asks; grey hoodie"
    );

    let refusal = |answer: &str| who::parse(answer, &heard, &cameras()).unwrap_err();
    assert_eq!(
        refusal("I think Thomas is the host."),
        "there is no JSON object in the answer"
    );
    assert!(refusal("{\"people\":[]}").contains("one entry per speaker"));
    assert!(refusal(
        "{\"people\":[{\"speaker\":\"SPEAKER_07\",\"camera\":\"cam2\"},{\"speaker\":\"SPEAKER_00\",\"camera\":\"cam1\"}]}"
    )
    .starts_with("\"SPEAKER_07\" is not a speaker of this session -- they are SPEAKER_01, SPEAKER_00"));
    assert!(refusal(
        "{\"people\":[{\"speaker\":\"SPEAKER_01\",\"camera\":\"cam9\"},{\"speaker\":\"SPEAKER_00\",\"camera\":\"cam1\"}]}"
    )
    .starts_with("\"cam9\" is not a camera of this session"));
    assert_eq!(
        refusal(
            "{\"people\":[{\"speaker\":\"SPEAKER_01\",\"camera\":\"cam2\"},{\"speaker\":\"SPEAKER_00\",\"camera\":\"cam2\"}]}"
        ),
        "cam2 is named for two speakers -- every camera shows one person"
    );
    assert_eq!(
        refusal("{\"people\":[{\"speaker\":\"SPEAKER_01\",\"camera\":\"cam2\"}]}"),
        "SPEAKER_00 needs exactly one entry"
    );
    // A speaker no camera shows is a whole answer.
    assert!(who::parse(
        "{\"people\":[{\"speaker\":\"SPEAKER_01\",\"camera\":\"cam2\"},{\"speaker\":\"SPEAKER_00\",\"camera\":\"\"}]}",
        &heard,
        &cameras()
    )
    .is_ok());
}

fn write_camera(at: &Path) {
    let ok = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=64x48:rate=10:duration=100",
        ])
        .args(["-c:v", "libx264", "-preset", "ultrafast"])
        .arg(at)
        .status()
        .expect("ffmpeg runs")
        .success();
    assert!(ok, "ffmpeg wrote {}", at.display());
}

fn session(tag: &str) -> (Tree, Project) {
    let dir = common::temp_dir_created(tag).join("talk.naivepost");
    fs::create_dir_all(dir.join("sources")).unwrap();
    let tree = Tree::new(&dir).expect("a project folder");
    for name in ["cam2.mkv", "cam1.mkv"] {
        write_camera(&dir.join("sources").join(name));
    }
    fs::create_dir_all(tree.session_tsv().parent().unwrap()).unwrap();
    naivepost::textfmt::write_session(&talk(), &tree.session_tsv()).unwrap();
    let project = Project {
        context: "I am Thomas, the host, in the grey hoodie.".to_string(),
        sources: ["cam2.mkv", "cam1.mkv"]
            .iter()
            .map(|name| Source {
                path: format!("project:sources/{name}"),
                footage: true,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    (tree, project)
}

/// S1–S4 through the door with the model answered for: it is shown one frame of each camera by name, a
/// refused answer goes back with its sentence, the good one is written and said, and a second ▶ keeps it.
#[test]
fn f1_16_the_pass_asks_once_writes_who_json_and_keeps_it() {
    let (tree, project) = session("who_pass");
    let conf = naivepost::settings::Conf::default();
    let asked = std::cell::RefCell::new(Vec::<(String, Vec<String>, usize)>::new());
    let model = |_system: &str, user: &str, frames: &[who::Frame]| -> Result<String, String> {
        asked.borrow_mut().push((
            user.to_string(),
            frames.iter().map(|frame| frame.camera.clone()).collect(),
            frames
                .iter()
                .map(|frame| frame.jpeg.len())
                .min()
                .unwrap_or(0),
        ));
        Ok(if asked.borrow().len() == 1 {
            "{\"people\":[{\"speaker\":\"SPEAKER_01\",\"camera\":\"cam2\"}]}".to_string()
        } else {
            "{\"people\":[{\"speaker\":\"SPEAKER_01\",\"name\":\"Thomas\",\"camera\":\"cam2\",\"because\":\"asks the questions; the grey hoodie is on cam2\"},\
             {\"speaker\":\"SPEAKER_00\",\"name\":\"Ada\",\"camera\":\"cam1\",\"because\":\"gives the long answers\"}]}"
                .to_string()
        })
    };
    let mut said = Vec::new();
    who::pass(
        &tree,
        &project,
        &conf,
        &mut |line: &str| said.push(line.to_string()),
        &model,
    )
    .expect("the pass runs");
    let asked = asked.into_inner();
    assert_eq!(asked.len(), 2, "asked again after the refusal");
    assert_eq!(asked[0].1, ["cam2", "cam1"], "one frame per camera, named");
    assert!(asked[0].2 > 100, "and each a real picture");
    assert!(
        asked[1]
            .0
            .contains("Your last answer was refused: SPEAKER_00 needs exactly one entry"),
        "the refusal goes back: {}",
        asked[1].0
    );
    assert!(said.iter().any(|line| line
        == ">>> who: SPEAKER_01 is Thomas, shown on cam2 -- asks the questions; the grey hoodie is on cam2"));
    let kept = who::read(&tree).expect("who.json is written");
    assert_eq!(
        kept.people[1],
        Person {
            speaker: "SPEAKER_00".to_string(),
            name: "Ada".to_string(),
            camera: "cam1".to_string(),
            because: "gives the long answers".to_string(),
        }
    );

    // A second ▶: the file stands, a person's correction with it, and nobody is asked.
    let never = |_: &str, _: &str, _: &[who::Frame]| -> Result<String, String> {
        panic!("who.json is there -- the model is not asked again")
    };
    let mut again = Vec::new();
    who::pass(
        &tree,
        &project,
        &conf,
        &mut |line: &str| again.push(line.to_string()),
        &never,
    )
    .unwrap();
    assert_eq!(again, [">>> who: already settled -- kept as it is"]);
}

/// S4: a model that never gives a valid answer leaves no file, says what that means, and Prepare goes on.
#[test]
fn f1_16_s4_no_valid_answer_is_said_and_is_not_a_failure() {
    let (tree, project) = session("who_none");
    let conf = naivepost::settings::Conf::default();
    let model = |_: &str, _: &str, _: &[who::Frame]| -> Result<String, String> {
        Ok("no idea".to_string())
    };
    let mut said = Vec::new();
    who::pass(
        &tree,
        &project,
        &conf,
        &mut |line: &str| said.push(line.to_string()),
        &model,
    )
    .expect("not a failure");
    assert_eq!(said.last().map(String::as_str), Some(who::NO_ANSWER));
    assert!(who::read(&tree).is_none());
}

/// S1: the row is Prepare's last and only for a session of two cameras or more.
#[test]
fn f1_16_s1_the_stage_is_prepares_last_and_needs_two_cameras() {
    let (_, project) = session("who_plan");
    let plan = naivepost::run_stages::plan(naivepost::run::Step::Prepare, &project);
    assert_eq!(plan.last().map(|stage| stage.name), Some("who"));
    let mut one = project.clone();
    one.sources.truncate(1);
    let plan = naivepost::run_stages::plan(naivepost::run::Step::Prepare, &one);
    assert!(plan.iter().all(|stage| stage.name != "who"));
}
