//! §08-produce#1-screen — the render legs this page hands to `produce_exec`: the session folder, the
//! speak call, the translation ask, the checkpoint answer, and the two failure notes. Split out of
//! `produce_page.rs`, which is past its size budget; the page keeps every call site.

pub(crate) use crate::ui::produce_languages::session_tree;
pub(crate) fn speak_a_line(line: &crate::produce_exec::ToSpeak, voice: &str) -> Result<(), String> {
    crate::ui::produce_speak_leg::speak_one(&session_tree(), line, voice)
        .map(|take| crate::ui::window::log_line(&format!("spoken \u{2014} {}", take.display())))
}

pub(crate) fn translate_leg() -> Box<crate::produce_translate::Ask> {
    let tree = session_tree();
    let stop: std::rc::Rc<dyn Fn() -> bool> =
        std::rc::Rc::new(|| crate::cancel_leg::cancel_check_now()());
    crate::produce_translate_ask::ask(&tree, &stop)
}

/// S10's checkpoint question, answered by the shell's own stop switch.
pub(crate) fn render_asked(_at: crate::produce_exec::At, _which: &str) -> bool {
    !crate::cancel_leg::cancelled()
}

pub fn render_checkpoint(_at: crate::produce_exec::At, _which: &str) -> bool {
    crate::run_stages::park_at_checkpoint();
    render_asked(_at, _which)
}

pub(crate) fn render_failure_note(render: &crate::produce_flow::Rendered) -> Option<String> {
    if render.ok || render.stopped {
        return None;
    }
    Some(render.size.clone())
}

pub(crate) fn words_half(run: &crate::produce_flow::Run) -> Result<(), String> {
    crate::run_stages::report_progress("writing the upload text and the thumbnail", None);
    crate::ui::produce_languages::upload_half(run)
}
