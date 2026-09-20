//! Running a tool and saying so (§03-shell#8-details-confirmed-against-the-code-verification-pass).
//!
//! ffmpeg is the long pole of every step, so the log has to show what it asked
//! for: the command as a shell would take it, quoted, at the four-space indent,
//! before it runs rather than after — when a render hangs there is no "after".
//! A failure repeats the command and the last 400 characters of its output.

use std::process::{Command, Stdio};

/// How much of a failing tool's output the error carries. Enough for ffmpeg's
/// own complaint, short enough to read in a log line.
pub const FAILURE_TAIL: usize = 400;

/// The characters a shell takes unquoted. Anything else is quoted so what the
/// log shows is what the command was — an argument with a space in it is one
/// argument, and an unquoted rendering would suggest otherwise.
fn bare(word: &str) -> bool {
    !word.is_empty()
        && word.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-')
        })
}

/// One argument as a shell would take it. A `'` cannot be escaped inside single
/// quotes, so the word closes around it: `'\''`.
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

/// The log line naming a command before it runs. Four spaces is the detail
/// indent, so a wall of ffmpeg lines stays quieter than the step's own news.
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

/// A failed command, with the command itself in it: the log line naming it can
/// be hundreds of lines back by the time the failure is worth reading.
pub fn failure(program: &str, args: &[&str], output: &str) -> String {
    format!(
        "{} failed: {}",
        shown(program, args),
        tail_of(output, FAILURE_TAIL)
    )
}

/// Run a tool, logging the command first. Its stdout comes back on success;
/// stdout and stderr together come back inside the error on failure.
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
