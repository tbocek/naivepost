//! F5.7 Page runs — the walk's legs for `own`, built over hand-written legs rather than over the page.
//!
//! §F5.7's `↻` over the thumbnail is the only press that hands the picture back to the model, and the spec's
//! line says so in a parenthesis: *"redraw from current images and instruction (clears `own`; always draws)"*.
//! Clearing matters beyond the repaint: decision 34 stops reprinting words onto a picture the person drew, so
//! a redraw that left `own` true would paint a new picture and then refuse to put the title on it.
//!
//! So the rule lives in `produce_presses::press_redraw_thumbnail` as ONE leg call, and what this file pins is
//! the three things a test can only see from outside that walk: that a refusal touches nothing, that the clear
//! lands BEFORE the draw (a clear after it would leave the drawn picture marked as the person's own for the
//! whole print pass), and that the door that owns the flag writes it where the next opening of the project
//! will read it — `publish.json`, not just the screen.
//!
//! `tests/produce_presses_widgets.rs` proves the button reaches this walk; nothing here touches GTK.

use std::cell::RefCell;
use std::rc::Rc;

use naivepost::produce_presses::{self as presses, ClearOwn, Legs};
use naivepost::produce_upload as upload;
use naivepost::project::Publish;
use naivepost::startup::session_dir;
use naivepost::layout::Tree;

const ITEM: &str = "F5.7";

/// A session folder of its own: `publish.json` is written through the live session's tree, so a case that
/// shares a folder with another cannot tell whose write landed.
fn session(tag: &str) -> (std::path::PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f57-clear-own-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    let dir = session_dir(&root);
    std::fs::create_dir_all(&dir).expect("session folder");
    (root, Tree::new(&dir).expect("the session folder is a project tree"))
}

/// The order the legs answered in, shared between the recording legs and the test.
type Calls = Rc<RefCell<Vec<String>>>;

/// A press's legs with every leg recorded and every leg succeeding, except `clear_own`, which the case hands
/// over. A cut always exists: the refusal half sets `has_cut` false itself.
fn walk(calls: &Calls, clear_own: ClearOwn) -> Legs {
    let for_poll = calls.clone();
    let for_print = calls.clone();
    let for_model = calls.clone();
    let for_draw = calls.clone();
    Legs {
        has_cut: Box::new(|| true),
        has_video: Box::new(|| true),
        has_picture: Box::new(|| true),
        rendering: Box::new(|| false),
        words_of: Box::new(|kind| vec![format!("{kind} words")]),
        instruction: Box::new(|| "a wide shot of the item".to_string()),
        references: Box::new(Vec::new),
        title_band: Box::new(|| 0.6),
        brief: Box::new(|| "the brief".to_string()),
        ask_draw: Box::new(move |_body| {
            for_draw.borrow_mut().push("draw".to_string());
            Ok(upload::DrawnJob { id: "job-1".to_string() })
        }),
        poll: Box::new(move |_job| {
            for_poll.borrow_mut().push("poll".to_string());
            Ok(upload::PollState {
                status: "done".to_string(),
                ahead: 0,
                done: true,
                png: Vec::new(),
            })
        }),
        print: Box::new(move |kind, _words| {
            for_print.borrow_mut().push(format!("print-{kind}"));
            Ok(())
        }),
        ask_model: Box::new(move |_brief| {
            for_model.borrow_mut().push("model".to_string());
            Ok("TITLE: new\nTHUMBNAIL: new instruction\nDESCRIPTION: new paragraph".to_string())
        }),
        encode: Box::new(|_argv| Ok(())),
        copy: Box::new(|_to| Ok(())),
        jpeg_sizes: Box::new(|_at| vec![1]),
        was_stopped: Box::new(|| false),
        clear_own,
        count_prints: true,
    }
}

/// The door's own leg, over a held `own` and the session's `publish.json` — the same pair
/// `produce_press_wiring::legs` wires, written here from its parts so the case can watch both.
fn page_door(own: Rc<std::cell::Cell<bool>>, tree: Tree) -> ClearOwn {
    Box::new(move || {
        own.set(false);
        let mut publish = naivepost::ui::produce_snapshot::merge_publish(
            Some(naivepost::project::Publish {
                own: own.get(),
                thumb_title: "keep me".to_string(),
                ..Default::default()
            }),
            &naivepost::publish::load(&tree).unwrap_or_default(),
        );
        publish.own = own.get();
        naivepost::publish::save(&publish, &tree)
    })
}

/// A press that refuses for want of a cut must touch nothing, `own` least of all.
#[test]
fn f5_7_s1_a_refused_redraw_clears_nothing() {
    let (_root, tree) = session("refused");
    let own = Rc::new(std::cell::Cell::new(true));
    naivepost::publish::save(
        &Publish {
            own: true,
            ..Default::default()
        },
        &tree,
    )
    .expect("a picture that is the person's own is recorded");
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut legs = walk(&calls, page_door(own.clone(), tree.clone()));
    legs.has_cut = Box::new(|| false);

    let out = presses::press_redraw_thumbnail(false, &legs, |_line| {});

    assert!(
        out.refused,
        "{ITEM}: the thumbnail's own refusal, not a silent no-op"
    );
    assert_eq!(
        out.status,
        naivepost::produce_runs::REDRAW_NO_CUT,
        "and it is the refusal's own sentence"
    );
    assert!(own.get(), "the flag stands until the press actually starts");
    assert!(
        naivepost::publish::load(&tree).unwrap().own,
        "and the file it will be read from next time stands with it"
    );
    assert!(
        calls.borrow().is_empty(),
        "a refused press runs no leg at all, least of all one that writes: {:?}",
        calls.borrow()
    );
}

/// The parenthetical itself, and its place in the order: `own` is false and `publish.json` says so BEFORE the
/// picture is drawn, because every step after the draw (the reprint above all) asks whether the picture is the
/// person's own and would leave a freshly drawn picture bare.
#[test]
fn f5_7_s1_the_clear_lands_after_the_opening_and_before_the_draw() {
    let (_root, tree) = session("order");
    let own = Rc::new(std::cell::Cell::new(true));
    naivepost::publish::save(
        &Publish {
            own: true,
            thumb_title: "an old title".to_string(),
            crop: naivepost::project::Crop {
                x: 0.0,
                y: 0.25,
            },
            ..Default::default()
        },
        &tree,
    )
    .expect("a drawn-over picture starts as the person's own");
    let calls = Rc::new(RefCell::new(Vec::new()));
    // Watch `own` at the moment the draw answers: the only way to catch a clear that lands too late.
    let seen_at_draw = Rc::new(std::cell::Cell::new(true));
    let legs = Legs {
        ask_draw: {
            let own = own.clone();
            let seen = seen_at_draw.clone();
            let calls = calls.clone();
            Box::new(move |_body| {
                seen.set(own.get());
                calls.borrow_mut().push("draw".to_string());
                Ok(upload::DrawnJob { id: "job-1".to_string() })
            })
        },
        ..walk(&calls, page_door(own.clone(), tree.clone()))
    };

    let out = presses::press_redraw_thumbnail(false, &legs, |_line| {});

    assert!(!out.refused, "the press ran: {:?}", out.error);
    assert!(
        !out.calls.contains(&"refused".to_string()),
        "no refusal on a press with a cut and an instruction"
    );
    assert!(!seen_at_draw.get(), "the picture was the app's by the time the draw was sent");
    assert!(!own.get(), "and stayed that way");
    let read = naivepost::publish::load(&tree).expect("publish.json");
    assert!(!read.own, "§F5.7's \"clears `own`\" reaches the file, not only the screen");
    assert_eq!(
        read.thumb_title, "keep me",
        "clearing the flag rewrites nothing else — the instruction and the words are the next legs' business"
    );
    assert_eq!(
        read.crop.y, 0.25,
        "and what no page row holds survives the write: a bare `publish_of_state` would drop the crop"
    );
    let order = out.calls.iter().map(String::as_str).collect::<Vec<_>>();
    let clear = order
        .iter()
        .position(|c| *c == "clear-own")
        .expect("the clear is counted as a call of its own");
    let draw = order
        .iter()
        .position(|c| *c == "draw")
        .expect("the draw follows it");
    assert!(
        clear < draw,
        "{ITEM} orders the clear before the draw: {order:?}"
    );
    assert_eq!(
        out.sd_calls, 1,
        "the press still costs exactly one sd.cpp call \u{2014} clearing is free"
    );
}

/// A clear that cannot write is a failure of the press, not a note in the log: the alternative is a picture
/// drawn over one the record still calls the person's own, which is the state decision 34 then refuses to fix.
#[test]
fn f5_7_s1_a_clear_that_refuses_ends_the_press_before_any_draw() {
    let (_root, _tree) = session("refusing-clear");
    let calls = Rc::new(RefCell::new(Vec::new()));
    let legs = walk(
        &calls,
        Box::new(|| Err::<(), String>("publish.json could not be written".to_string())),
    );

    let out = presses::press_redraw_thumbnail(false, &legs, |_line| {});

    let said = out.error.clone().unwrap_or_default();
    assert!(
        said.contains("publish.json could not be written"),
        "the trouble is named where it happened: {said:?}"
    );
    assert_eq!(out.sd_calls, 0, "a press that could not clear draws nothing");
    assert!(
        !calls.borrow().iter().any(|c| c == "draw"),
        "and spends no GPU time doing it: {:?}",
        calls.borrow()
    );
}

/// The state half of the door is `produce_page::clear_own_thumbnail`, the mirror of `set_own_thumbnail`: the
/// page's held flag, and nothing else on it.
#[test]
fn f5_7_s1_the_page_mirror_clears_only_the_flag() {
    let mut state = naivepost::ui::produce_page::read_state();
    state.own = true;
    state.thumb_title = "keep me".to_string();
    naivepost::ui::produce_page::set_state(state);

    naivepost::ui::produce_page::clear_own_thumbnail();

    let after = naivepost::ui::produce_page::read_state();
    assert!(!after.own, "the press that hands the picture back leaves no mark of ownership");
    assert_eq!(after.thumb_title, "keep me", "it clears the flag, not the words");
}
