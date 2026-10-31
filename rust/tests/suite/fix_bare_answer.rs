//! A one-line block answered with the sentence alone keeps the fix; anything less certain is still refused.

use naivepost::fix_transcripts::{fix_block, Reply};
use naivepost::textfmt::Line;

fn line(text: &str) -> Line {
    Line {
        start: 0.6,
        end: 4.76,
        speaker: "SPEAKER_00".into(),
        text: text.into(),
        ..Default::default()
    }
}

const ASR: &str = "then the black cats it's a wild dry at the moment so see you next week";

fn answered(lines: &[Line], bare: &str) -> (bool, Vec<String>) {
    let bare = bare.to_string();
    let out = fix_block(lines, "", |_| Ok(vec![Reply::Bare(bare.clone())])).unwrap();
    (
        out.valid,
        out.lines.iter().map(|l| l.text.clone()).collect(),
    )
}

#[test]
fn a_one_line_block_takes_the_sentence_and_keeps_its_time_and_speaker() {
    let fixed = "Then the black cats, it's a wild ride at the moment. So see you next week.";
    let lines = [line(ASR)];
    let out = fix_block(&lines, "", |_| Ok(vec![Reply::Bare(fixed.to_string())])).unwrap();
    assert!(out.valid);
    assert_eq!(out.lines[0].text, fixed);
    assert_eq!((out.lines[0].start, out.lines[0].end), (0.6, 4.76));
    assert_eq!(out.lines[0].speaker, "SPEAKER_00");
}

#[test]
fn a_remark_about_the_line_is_not_the_line() {
    let (valid, texts) = answered(&[line(ASR)], "Sure, I can help with that transcript.");
    assert!(!valid);
    assert_eq!(texts, [ASR]);
}

#[test]
fn a_longer_block_cannot_tell_where_a_sentence_belongs() {
    let (valid, _) = answered(&[line(ASR), line("second line")], ASR);
    assert!(!valid);
}
