//! The page, `publish.json` and the project's record agree on the thumbnail, and a call's answer is only what it changed.

use naivepost::layout::Tree;
use naivepost::produce_upload_call::Collected;
use naivepost::project::{self, Publish};
use naivepost::ui::produce_languages::render_answers;
use naivepost::ui::produce_page as page;

fn tree(tag: &str) -> Tree {
    let dir = std::env::temp_dir().join(format!("thumb-record-{tag}.naivepost"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a project folder");
    Tree::new(&dir).expect("a project tree")
}

fn answer(base: &Publish, publish: Publish, wrote: bool) -> String {
    render_answers(
        base,
        &Collected {
            publish,
            wrote,
            answers: Vec::new(),
        },
    )
}

#[test]
fn set_thumbnail_keeps_the_row_and_a_reworded_line() {
    let mut state = page::read_state();
    state.frames = vec!["frame:1.0".into(), "frame:2.0".into()];
    state.title = "the video".into();
    state.thumb_title = "my line".into();
    state.title_seeded = true;
    page::set_state(state);
    page::set_own_thumbnail();
    let state = page::read_state();
    assert!(state.own);
    assert_eq!(state.frames, ["frame:1.0", "frame:2.0"]);
    assert_eq!(state.thumb_title, "my line");
}

#[test]
fn set_thumbnail_prints_the_title_when_no_line_stands() {
    let mut state = page::read_state();
    state.frames = vec!["frame:1.0".into()];
    state.title = "the video".into();
    state.thumb_title = String::new();
    state.title_seeded = false;
    page::set_state(state);
    page::set_own_thumbnail();
    let state = page::read_state();
    assert_eq!(state.thumb_title, "the video");
    assert!(state.title_seeded);
}

#[test]
fn the_record_wins_over_publish_json_when_the_project_opens() {
    let tree = tree("wins");
    let record = Publish {
        title: "typed".into(),
        own: false,
        ..Default::default()
    };
    let mut held = project::load(tree.dir()).expect("an empty project");
    held.publish = Some(record);
    project::save(&held, tree.dir()).expect("saved");
    naivepost::publish::save(
        &Publish {
            title: "stale".into(),
            description: "only here".into(),
            own: true,
            ..Default::default()
        },
        &tree,
    )
    .expect("written");
    let opened = page::load_page(&tree);
    assert_eq!(opened.title, "typed");
    assert!(!opened.own, "a cleared `own` does not come back");
    assert_eq!(
        opened.description, "only here",
        "what the record lacks is filled"
    );
}

#[test]
fn publish_json_answers_for_a_project_with_no_record() {
    let tree = tree("no-record");
    naivepost::publish::save(
        &Publish {
            own: true,
            ..Default::default()
        },
        &tree,
    )
    .expect("written");
    assert!(page::load_page(&tree).own);
}

#[test]
fn a_description_alone_does_not_turn_the_printed_line_into_the_title() {
    let base = Publish {
        title: "the video".into(),
        thumb_title: "my line".into(),
        ..Default::default()
    };
    let told = answer(
        &base,
        Publish {
            description: "new words".into(),
            ..base.clone()
        },
        true,
    );
    assert_eq!(told, "new words");
}

#[test]
fn an_untouched_row_is_not_replayed_as_an_instruction() {
    let base = Publish {
        own: true,
        frames: vec!["project:images/a.png".into()],
        ..Default::default()
    };
    let told = answer(
        &base,
        Publish {
            title: "new".into(),
            thumb_title: "new".into(),
            ..base.clone()
        },
        true,
    );
    assert_eq!(told, "TITLE: new");
}

#[test]
fn a_new_frame_is_an_answer() {
    let base = Publish::default();
    let told = answer(
        &base,
        Publish {
            own: true,
            frames: vec!["frame:9.0".into()],
            ..Default::default()
        },
        true,
    );
    assert_eq!(told, "THUMBNAIL: frame:9.0");
}

#[test]
fn a_call_that_wrote_nothing_answers_nothing_and_one_that_repeated_itself_still_answers() {
    let base = Publish {
        title: "same".into(),
        ..Default::default()
    };
    assert_eq!(answer(&base, base.clone(), false), "");
    assert_eq!(answer(&base, base.clone(), true), "TITLE: same");
}
