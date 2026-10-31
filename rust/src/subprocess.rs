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

/// Spawn a tool with piped stdio and hand back the still-running child.
pub fn spawn_piped(program: &str, args: &[&str]) -> Result<std::process::Child, std::io::Error> {
    std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}
