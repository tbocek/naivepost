//! Running a tool and saying so (§03-shell#8-details-confirmed-against-the-code-verification-pass).

use std::process::{Command, Stdio};

pub const FAILURE_TAIL: usize = 400;

fn bare(word: &str) -> bool {
    !word.is_empty()
        && word.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-')
        })
}

pub fn quote(arg: &str) -> String {
    if bare(arg) {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

/// The command as a shell would take it.
pub fn shown(program: &str, args: &[&str]) -> String {
    let mut line = quote(program);
    for arg in args {
        line.push(' ');
        line.push_str(&quote(arg));
    }
    line
}

pub fn log_line(program: &str, args: &[&str]) -> String {
    format!("    {}", shown(program, args))
}

/// The last `max_chars` characters, marked with an ellipsis when it had to cut.
pub fn tail_of(output: &str, max_chars: usize) -> String {
    let chars: Vec<char> = output.chars().collect();
    if chars.len() <= max_chars {
        return output.to_string();
    }
    let kept: String = chars[chars.len() - max_chars..].iter().collect();
    format!("\u{2026}{kept}")
}

pub fn failure(program: &str, args: &[&str], output: &str) -> String {
    format!(
        "{} failed: {}",
        shown(program, args),
        tail_of(output, FAILURE_TAIL)
    )
}

pub fn run<F: FnMut(&str)>(program: &str, args: &[&str], mut log: F) -> Result<String, String> {
    log(&log_line(program, args));

    let out = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| format!("{} failed: {err}", shown(program, args)))?;

    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));

    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    Err(failure(program, args, &combined))
}

pub fn run_listed<F: FnMut(&str)>(
    program: &str,
    args: &[&str],
    mut log: F,
) -> Result<String, String> {
    log(&log_line(program, args));
    let child = spawn_piped(program, args)
        .map_err(|err| format!("{} failed: {err}", shown(program, args)))?;
    let pid = child.id();
    crate::run_stages::list_child(pid);
    let out = child.wait_with_output();
    crate::run_stages::unlist_child(pid);
    let out = out.map_err(|err| format!("{} failed: {err}", shown(program, args)))?;
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    Err(failure(program, args, &combined))
}

/// [`run_listed`], with the seconds a running ffmpeg says it has written handed to `on_time` as it goes: the `time=`
/// of the stats line it rewrites on stderr (ended by a carriage return, not a newline) every half second.
pub fn run_listed_timed<F: FnMut(&str), T: FnMut(f64)>(
    program: &str,
    args: &[&str],
    mut log: F,
    mut on_time: T,
) -> Result<String, String> {
    use std::io::Read;
    log(&log_line(program, args));
    let mut child = spawn_piped(program, args)
        .map_err(|err| format!("{} failed: {err}", shown(program, args)))?;
    let pid = child.id();
    crate::run_stages::list_child(pid);
    // stdout on its own thread, so neither pipe fills while the other is read.
    let mut stdout = child.stdout.take();
    let out_thread = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(pipe) = stdout.as_mut() {
            let _ = pipe.read_to_end(&mut out);
        }
        out
    });
    let mut err = Vec::new();
    if let Some(mut pipe) = child.stderr.take() {
        let mut chunk = [0u8; 4096];
        let mut line = Vec::new();
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            err.extend_from_slice(&chunk[..n]);
            for &b in &chunk[..n] {
                if b == b'\r' || b == b'\n' {
                    if let Some(t) = stats_time(&String::from_utf8_lossy(&line)) {
                        on_time(t);
                    }
                    line.clear();
                } else {
                    line.push(b);
                }
            }
        }
    }
    let status = child.wait();
    crate::run_stages::unlist_child(pid);
    let out = out_thread.join().unwrap_or_default();
    let status = status.map_err(|err| format!("{} failed: {err}", shown(program, args)))?;
    let mut combined = String::from_utf8_lossy(&out).into_owned();
    combined.push_str(&String::from_utf8_lossy(&err));
    if status.success() {
        return Ok(String::from_utf8_lossy(&out).into_owned());
    }
    Err(failure(program, args, &combined))
}

/// The seconds in an ffmpeg stats line's `time=HH:MM:SS.ss`; None for any other line, and for `time=N/A`.
pub fn stats_time(line: &str) -> Option<f64> {
    let at = line.find("time=")? + "time=".len();
    let field = line[at..].split_whitespace().next()?;
    let negative = field.starts_with('-');
    let mut parts = field.trim_start_matches('-').split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    let t =
        h.parse::<f64>().ok()? * 3600.0 + m.parse::<f64>().ok()? * 60.0 + s.parse::<f64>().ok()?;
    Some(if negative { 0.0 } else { t })
}

/// Spawn a tool with piped stdio and hand back the still-running child.
pub fn spawn_piped(program: &str, args: &[&str]) -> Result<std::process::Child, std::io::Error> {
    std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}
