//! spec/04-prepare.md F1.16: who is who.
//!
//! The transcript tells voices apart and calls them SPEAKER_00, SPEAKER_01; the cameras are files. Which
//! voice is whose, and which file shows whom, is already said three times over: the User Context says who
//! the people are, the pictures show who sits where, and the talk shows who asks and who answers. One model
//! call reads the three and `prepare/who.json` keeps the answer, for [`crate::cut_follow`] to switch the
//! picture by.
//!
//! The rules (what is asked, what a valid answer is) are plain functions; the door at the bottom reads the
//! session, takes one frame of each camera and dials the model.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::exchanges::{Message, Part};
use crate::layout::{self, Tree};
use crate::project::Project;
use crate::run_stages;
use crate::textfmt::SessionLine;

/// The step's name in the request log and the marker the walk dials on.
pub const STEP: &str = "who";
pub const WHO_TASK: &str = "naivepost:who";

/// P.eng.whoLines: how many spoken lines from the start of the session the model reads.
pub const WHO_LINES: usize = 60;
/// A frame is shown this wide: enough to tell a hoodie from a jacket.
const FRAME_WIDTH: u32 = 768;

/// The shipped wording.
pub const PROMPT: &str = "You are told who is who in one recorded conversation, and you write it down. Nothing else.

You are given:
- the USER CONTEXT: what the editor says about the session and the people in it;
- one picture per CAMERA, each named by its file. Every camera films one person;
- the SPEAKERS the transcript tells apart, each with how long they talk in the whole session and how many of their lines are questions;
- the first spoken lines of the session, each with its speaker.

Work out, for every speaker, which person of the user context it is and which camera shows that person. Use what the context says about how a person looks or where they sit against the pictures, and what it says about what they do (the host asks the questions and introduces the guest; the guest gives the long answers; someone is greeted by name) against the lines.

Answer with strict JSON and nothing around it:
{\"people\":[{\"speaker\":\"SPEAKER_00\",\"name\":\"...\",\"camera\":\"...\",\"because\":\"one short line: what in the talk and what in the picture told you\"}]}

One entry per speaker, the speaker's label and the camera's name copied exactly as given. Every camera shows a different person, so no camera is named twice. A speaker no camera shows gets \"camera\":\"\". A name the context does not give is the person's part in the conversation (\"the guest\").";

/// One speaker as the transcript holds them.
#[derive(Debug, Clone, PartialEq)]
pub struct Speaker {
    pub id: String,
    /// Seconds they talk in the whole session.
    pub seconds: f64,
    /// How many of their lines end in a question mark.
    pub questions: usize,
}

/// One person of the answer, as `who.json` keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Person {
    pub speaker: String,
    pub name: String,
    /// The camera's recording name; empty for a speaker no camera shows.
    pub camera: String,
    pub because: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Who {
    pub people: Vec<Person>,
}

pub fn who_path(tree: &Tree) -> PathBuf {
    tree.prepare_dir().join("who.json")
}

/// `prepare/who.json`, when it is there and reads.
pub fn read(tree: &Tree) -> Option<Who> {
    let text = std::fs::read_to_string(who_path(tree)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Is this label a voice the diarizer told apart (never an EVENT line or the narrator's microphone)?
pub fn is_speaker(who: &str) -> bool {
    who.starts_with("SPEAKER")
}

/// S2: the speakers of the session, in the order of their first word.
pub fn speakers(lines: &[SessionLine]) -> Vec<Speaker> {
    let mut found: Vec<Speaker> = Vec::new();
    for line in lines.iter().filter(|line| is_speaker(&line.who)) {
        let index = match found.iter().position(|speaker| speaker.id == line.who) {
            Some(index) => index,
            None => {
                found.push(Speaker {
                    id: line.who.clone(),
                    seconds: 0.0,
                    questions: 0,
                });
                found.len() - 1
            }
        };
        found[index].seconds += (line.end - line.start).max(0.0);
        if line.text.trim_end().ends_with('?') {
            found[index].questions += 1;
        }
    }
    found
}

/// S2: the user message's text, before the pictures.
pub fn brief(context: &str, cameras: &[String], lines: &[SessionLine]) -> String {
    let mut out = String::new();
    out.push_str("USER CONTEXT:\n");
    out.push_str(if context.trim().is_empty() {
        "(none)"
    } else {
        context.trim()
    });
    out.push_str("\n\nCAMERAS: ");
    out.push_str(&cameras.join(", "));
    out.push_str("\n\nSPEAKERS:\n");
    for speaker in speakers(lines) {
        out.push_str(&format!(
            "{}: talks for {} in the whole session, {} of their lines are questions\n",
            speaker.id,
            crate::tools::mm_ss(speaker.seconds),
            speaker.questions
        ));
    }
    out.push_str("\nTHE FIRST LINES:\n");
    for line in lines
        .iter()
        .filter(|line| is_speaker(&line.who))
        .take(WHO_LINES)
    {
        out.push_str(&format!(
            "[{}] {}: {}\n",
            crate::tools::mm_ss(line.start),
            line.who,
            line.text.trim()
        ));
    }
    out
}

/// The JSON object inside a reply: a model often wraps it in a code fence or a sentence.
fn json_of(reply: &str) -> Option<&str> {
    let from = reply.find('{')?;
    let to = reply.rfind('}')?;
    (to > from).then(|| &reply[from..=to])
}

/// S3: the answer checked against the session. The refusal is the sentence the model is sent back.
pub fn parse(reply: &str, speakers: &[Speaker], cameras: &[String]) -> Result<Who, String> {
    let json = json_of(reply).ok_or("there is no JSON object in the answer")?;
    let who: Who = serde_json::from_str(json)
        .map_err(|err| format!("the answer is not the JSON asked for ({err})"))?;
    if who.people.is_empty() {
        return Err("\"people\" is empty -- one entry per speaker".to_string());
    }
    let mut shown: Vec<&str> = Vec::new();
    for person in &who.people {
        if !speakers.iter().any(|speaker| speaker.id == person.speaker) {
            return Err(format!(
                "\"{}\" is not a speaker of this session -- they are {}",
                person.speaker,
                speakers
                    .iter()
                    .map(|speaker| speaker.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if person.camera.is_empty() {
            continue;
        }
        if !cameras.contains(&person.camera) {
            return Err(format!(
                "\"{}\" is not a camera of this session -- they are {}",
                person.camera,
                cameras.join(", ")
            ));
        }
        if shown.contains(&person.camera.as_str()) {
            return Err(format!(
                "{} is named for two speakers -- every camera shows one person",
                person.camera
            ));
        }
        shown.push(&person.camera);
    }
    for speaker in speakers {
        if who
            .people
            .iter()
            .filter(|person| person.speaker == speaker.id)
            .count()
            != 1
        {
            return Err(format!("{} needs exactly one entry", speaker.id));
        }
    }
    Ok(who)
}

/// S4's line for one person.
pub fn said(person: &Person) -> String {
    let shown = if person.camera.is_empty() {
        "shown on no camera".to_string()
    } else {
        format!("shown on {}", person.camera)
    };
    format!(
        ">>> who: {} is {}, {shown} -- {}",
        person.speaker,
        if person.name.trim().is_empty() {
            "not named"
        } else {
            person.name.trim()
        },
        person.because.trim()
    )
}

pub const NO_ANSWER: &str = "!!! who: the model did not say who is who -- the cameras will not follow the speakers; write prepare/who.json by hand or press \u{25b6} again";

// ---------------------------------------------------------------------------------------------------------
// The door.

/// The camera rows of a project: its footage, by recording name.
pub fn cameras(project: &Project) -> Vec<(String, String)> {
    project
        .sources
        .iter()
        .filter(|source| source.footage)
        .map(|source| {
            (
                crate::audio_task::source_lane(&source.path),
                source.path.clone(),
            )
        })
        .collect()
}

/// S1: one row, as Prepare's last, when the session has two cameras or more.
pub fn stage(project: &Project) -> run_stages::Stage {
    let tasks = if cameras(project).len() >= 2 {
        vec![run_stages::Task {
            label: "working out who is who".to_string(),
            program: WHO_TASK.to_string(),
            args: Vec::new(),
            already_done: false,
            session: None,
        }]
    } else {
        Vec::new()
    };
    run_stages::Stage {
        track: crate::runqueue::TRACK_FIX,
        name: "who",
        phase: 2,
        of: 2,
        kind: "ask",
        tasks,
    }
}

/// The walk's door.
pub fn run_task(_task: &run_stages::Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let paths = crate::settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let session = crate::startup::open_dir();
    let tree =
        Tree::new(&session).map_err(|_| format!("{}: not a project folder", session.display()))?;
    let project = crate::project::load(&session)?;
    let conf = crate::settings::read(&paths).unwrap_or_default();
    pass(&tree, &project, &conf, log, &|system, user, frames| {
        ask(&tree, &conf, system, user, frames)
    })
}

/// One camera's picture for the ask: its name and a JPEG.
pub struct Frame {
    pub camera: String,
    pub jpeg: Vec<u8>,
}

/// S1–S4 with the model behind a closure, so a test answers for it.
pub fn pass(
    tree: &Tree,
    project: &Project,
    conf: &crate::settings::Conf,
    log: &mut impl FnMut(&str),
    model: &dyn Fn(&str, &str, &[Frame]) -> Result<String, String>,
) -> Result<(), String> {
    if who_path(tree).exists() {
        log(">>> who: already settled -- kept as it is");
        return Ok(());
    }
    let rows = cameras(project);
    let lines = crate::textfmt::read_session(&tree.session_tsv())?;
    let heard = speakers(&lines);
    if rows.len() < 2 || heard.len() < 2 {
        // One camera or one voice: nothing to tell apart, nothing said.
        return Ok(());
    }
    // One frame of each camera, where the session's longest line is being said: somebody is talking and
    // both people are in their seats.
    let at = lines
        .iter()
        .filter(|line| is_speaker(&line.who))
        .max_by(|a, b| (a.end - a.start).total_cmp(&(b.end - b.start)))
        .map(|line| (line.start + line.end) / 2.0)
        .unwrap_or(0.0);
    let names: Vec<String> = project
        .sources
        .iter()
        .map(|source| crate::audio_task::source_lane(&source.path))
        .collect();
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
    let clock = crate::clock::clock(&borrowed);
    let mut frames = Vec::new();
    for (name, stored) in &rows {
        let index = names.iter().position(|other| other == name).unwrap_or(0);
        let path = layout::resolve(tree.dir(), tree.dir(), stored);
        let jpeg = frame_of(
            tree,
            &path,
            name,
            (at - clock.offset(index)).max(0.0),
            &conf.ffmpeg,
            log,
        )?;
        frames.push(Frame {
            camera: name.clone(),
            jpeg,
        });
    }
    let camera_names: Vec<String> = rows.iter().map(|(name, _)| name.clone()).collect();
    let user = brief(&project.context, &camera_names, &lines);

    let mut refused = String::new();
    for _ in 0..crate::roles::LLM_ATTEMPTS {
        if crate::cancel_leg::cancelled() {
            return Ok(());
        }
        let asked = if refused.is_empty() {
            user.clone()
        } else {
            format!("{user}\nYour last answer was refused: {refused}. Answer again.")
        };
        let reply = match model(PROMPT, &asked, &frames) {
            Ok(reply) => reply,
            Err(why) => {
                log(&format!("!!! who: {why}"));
                break;
            }
        };
        match parse(&reply, &heard, &camera_names) {
            Ok(who) => {
                let json =
                    serde_json::to_string_pretty(&who).map_err(|err| format!("who.json: {err}"))?;
                std::fs::create_dir_all(tree.prepare_dir())
                    .and_then(|()| std::fs::write(who_path(tree), json + "\n"))
                    .map_err(|err| format!("{}: {err}", who_path(tree).display()))?;
                for person in &who.people {
                    log(&said(person));
                }
                return Ok(());
            }
            Err(why) => {
                log(&format!(">>> who: the answer was refused -- {why}"));
                refused = why;
            }
        }
    }
    log(NO_ANSWER);
    Ok(())
}

/// One frame of a camera at `at` seconds into its file, as a JPEG.
fn frame_of(
    tree: &Tree,
    path: &Path,
    name: &str,
    at: f64,
    configured_ffmpeg: &str,
    log: &mut impl FnMut(&str),
) -> Result<Vec<u8>, String> {
    let scratch = tree.dir().join("cache");
    std::fs::create_dir_all(&scratch).map_err(|err| format!("{}: {err}", scratch.display()))?;
    let out = scratch.join(format!("who-{name}.jpg"));
    let ffmpeg = crate::separate_media::program(configured_ffmpeg);
    let (seconds, shown, to) = (
        format!("{at:.3}"),
        path.display().to_string(),
        out.display().to_string(),
    );
    let scale = format!("scale={FRAME_WIDTH}:-2");
    let args = [
        "-v",
        "error",
        "-y",
        "-ss",
        &seconds,
        "-i",
        &shown,
        "-frames:v",
        "1",
        "-vf",
        &scale,
        "-q:v",
        "4",
        &to,
    ];
    let ran = crate::subprocess::run_listed(&ffmpeg, &args, |line| log(line));
    let bytes = std::fs::read(&out);
    let _ = std::fs::remove_file(&out);
    ran.map_err(|err| format!("{name}: no frame to show the model -- {err}"))?;
    bytes.map_err(|_| format!("{name}: ffmpeg wrote no frame at {seconds} s"))
}

/// The live ask: the main model, thinking off, the pictures named by their camera.
fn ask(
    tree: &Tree,
    conf: &crate::settings::Conf,
    system: &str,
    user: &str,
    frames: &[Frame],
) -> Result<String, String> {
    let model = conf.model.trim().to_string();
    let mut parts = vec![Part::Text(user.to_string())];
    for frame in frames {
        parts.push(Part::Text(format!("CAMERA {}:", frame.camera)));
        parts.push(Part::Image(crate::llm_request::data_url(
            &frame.jpeg,
            "image/jpeg",
        )));
    }
    let messages = vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system.to_string())],
        },
        Message {
            role: "user".to_string(),
            parts,
        },
    ];
    let endpoint = crate::services::llm_endpoint(conf);
    let body = crate::llm_request::body(
        STEP,
        &model,
        &messages,
        crate::exchanges::Mode::Execute,
        None,
        true,
    )?
    .to_string();
    let mut row =
        crate::server_leg::base_row(crate::services::Server::Llm, &model, "POST", "", STEP, STEP);
    row.job = STEP.to_string();
    let sent = crate::llm_retry_leg::dial(
        STEP,
        tree,
        &row,
        &endpoint,
        &body,
        &crate::cancel_leg::cancel_check_now(),
        &|_| {},
        &crate::llm_retry_leg::nap,
    )
    .answer
    .map_err(|why| format!("the call to {model} failed: {why}"))?;
    if !sent.ok() {
        return Err(format!(
            "{model} answered {}: {}",
            sent.status,
            sent.body.trim()
        ));
    }
    let reply = crate::llm_request::reply_from(&sent.content_type, &sent.body)?;
    Ok(reply.answer().to_string())
}
