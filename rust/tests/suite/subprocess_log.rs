//! §03-shell#8-details-confirmed-against-the-code-verification-pass — subprocess
//! logging: every command is logged before it runs, as a shell would take it, at
//! the four-space indent, and a failure repeats it with the last 400 characters.

use naivepost::subprocess::{self, FAILURE_TAIL};

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_subprocess_logging_is_quoted_and_indented()
{
    assert_eq!(subprocess::log_line("ffmpeg", &["-i", "in.mkv", "out.wav"]), "    ffmpeg -i in.mkv out.wav");

    // Only the awkward words get quoted, and a quote inside one closes around it.
    assert_eq!(subprocess::shown("rm", &["the draft"]), "rm 'the draft'");
    assert_eq!(subprocess::shown("echo", &["it's"]), "echo 'it'\\''s'");
    assert_eq!(subprocess::shown("it's a file", &[]), "'it'\\''s a file'");
    assert_eq!(subprocess::quote(""), "''");
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_subprocess_logging_happens_before_the_command_runs()
{
    let mut seen: Vec<String> = Vec::new();
    let result = subprocess::run("/bin/sh", &["-c", "exit 3"], |line| seen.push(line.to_string()));

    // The line is there even though the command never succeeded.
    assert_eq!(seen, vec!["    /bin/sh -c 'exit 3'".to_string()]);
    assert!(result.is_err());
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_subprocess_failure_repeats_the_command_and_the_last_400_characters()
{
    let mut seen: Vec<String> = Vec::new();
    let err = subprocess::run(
        "/bin/sh",
        &["-c", "printf 'A%.0s' $(seq 1 500); printf 'B%.0s' $(seq 1 400); exit 1"],
        |line| seen.push(line.to_string()),
    )
    .expect_err("a non-zero exit is a failure");

    assert!(err.starts_with("/bin/sh -c "), "{err}");
    assert!(err.contains(" failed: "), "{err}");
    let tail = err.split_once(" failed: ").unwrap().1;
    assert_eq!(tail.chars().count(), FAILURE_TAIL + 1, "the ellipsis plus 400");
    assert!(tail.starts_with('\u{2026}'), "{tail}");
    assert!(tail.ends_with('B'), "{tail}");
    assert!(!tail.contains('A'), "what scrolled past is gone");

    // Short output is kept whole, with no ellipsis pretending it was cut.
    let short = subprocess::failure("false", &[], "nothing happened");
    assert_eq!(short, "false failed: nothing happened");
}

#[test]
fn sec_03_shell_8_details_confirmed_against_the_code_verification_pass_a_successful_command_returns_its_output()
{
    let mut seen: Vec<String> = Vec::new();
    let out = subprocess::run("/bin/sh", &["-c", "echo hi"], |line| seen.push(line.to_string()))
        .expect("echo does not fail");

    assert_eq!(out, "hi\n");
    assert_eq!(seen, vec!["    /bin/sh -c 'echo hi'".to_string()]);
}
