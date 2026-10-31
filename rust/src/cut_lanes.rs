//! §05-cut#2 — cut lanes built from a source's own metadata. `spec/05-cut.md` §B: "Cut lanes: {name, src,
//! at, off, dur} windows of a file on a row of its own; **built from the source's metadata or probed**; name,
//! name-2 …".

use crate::cut::Lane;

pub fn lane_name(taken: &[String], src: &str) -> String {
    crate::cut_copy::lane_name(src, taken)
}

pub fn lane_from(source_path: &str, at: f64, duration_seconds: f64, taken: &[String]) -> Lane {
    let stem = std::path::Path::new(source_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(source_path)
        .to_string();
    Lane {
        name: lane_name(taken, &stem),
        src: source_path.to_string(),
        at,
        off: 0.0,
        dur: duration_seconds,
    }
}

pub fn probe_duration(
    configured_ffmpeg: &str,
    path: &str,
    log: &mut impl FnMut(&str),
) -> Result<f64, String> {
    crate::run_stages::probe_duration(configured_ffmpeg, std::path::Path::new(path), log)
}

pub fn no_probe_status(path: &str, reason: &str) -> String {
    format!("{path}: no length from ffprobe \u{2014} {reason}")
}

pub fn channel_layout(
    chans: u8,
    lanes_on_this_file: usize,
    lane_index_on_file: usize,
) -> &'static str {
    if lanes_on_this_file >= 2 {
        return if lane_index_on_file == 0 { "L" } else { "R" };
    }
    if chans >= 2 {
        return "L=R";
    }
    "mono"
}

pub fn plate(name: &str, layout: Option<&str>) -> String {
    match layout {
        Some(layout) => format!("{name} {layout}"),
        None => name.to_string(),
    }
}

pub fn channels_args(path: &std::path::Path) -> Vec<String> {
    vec![
        "-v".to_string(),
        "error".to_string(),
        "-select_streams".to_string(),
        "a:0".to_string(),
        "-show_entries".to_string(),
        "stream=channels".to_string(),
        "-of".to_string(),
        "default=nw=1:nk=1".to_string(),
        path.display().to_string(),
    ]
}

pub fn parse_channels(answer: &str) -> Option<u8> {
    let text = answer.trim();
    if text.is_empty() {
        return None;
    }
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let chans: u8 = digits.parse().ok()?;
    (chans > 0).then_some(chans)
}

pub fn probe_channels(
    ffprobe: &str,
    path: &std::path::Path,
    run: &crate::source_probe::Tool,
) -> Option<u8> {
    let asked = channels_args(path);
    let flags: Vec<&str> = asked.iter().map(String::as_str).collect();
    let answer = run(ffprobe, &flags).ok()?;
    parse_channels(&answer)
}

fn channels_of(lane_src: &str) -> Option<u8> {
    let root: Option<std::path::PathBuf> = crate::ui::window::narrate_session_tree()
        .and_then(|tree| tree.dir().parent().map(|root| root.to_path_buf()))
        .or_else(|| std::env::current_dir().ok());
    let path = match root {
        Some(root) => {
            let project = crate::startup::open_dir();
            crate::layout::resolve(&root, &project, lane_src)
        }
        None => std::path::PathBuf::from(lane_src),
    };
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Option<u8>>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(held) = cache.lock().ok().and_then(|slot| slot.get(&path).copied()) {
        return held;
    }
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    let tool = crate::services::ffprobe_for(&configured, "ffprobe");
    let found = probe_channels(&tool, &path, &crate::source_probe::run_ffprobe);
    if found.is_none() {
        crate::ui::window::log_line(&format!(
            "{tool} said nothing about {}: the lane's plate shows its name only",
            path.display()
        ));
    }
    if let Ok(mut slot) = cache.lock() {
        slot.insert(path.to_path_buf(), found);
    }
    found
}

pub fn plates(lanes: &[Lane]) -> Vec<String> {
    let mut per_file: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for lane in lanes {
        *per_file.entry(lane.src.as_str()).or_insert(0) += 1;
    }
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    lanes
        .iter()
        .map(|lane| {
            let index = seen.entry(lane.src.as_str()).or_insert(0);
            let on_file = *index;
            *index += 1;
            let layout = channel_layout(
                channels_of(&lane.src).unwrap_or(1),
                per_file.get(lane.src.as_str()).copied().unwrap_or(1),
                on_file,
            );
            plate(&lane.name, Some(layout))
        })
        .collect()
}
