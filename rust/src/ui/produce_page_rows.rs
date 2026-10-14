//! §08-produce#1-screen — the render legs this page hands to `produce_exec`: the session folder, the speak
//! refusal, the translation ask, the checkpoint answer, and the two failure notes. Split out of
//! `produce_page.rs`, which is past its size budget; the page keeps every call site.

/// The session's folder, resolved the way the page resolves everything else on this path.
pub(crate) fn session_tree() -> crate::layout::Tree {
    let root = std::env::current_dir().unwrap_or_default();
    crate::layout::Tree::new(crate::startup::session_dir(&root))
    .unwrap_or_else(|_| crate::layout::Tree::new(std::path::Path::new("session.naivepost")).unwrap())
}

/// S2's speak: no audio.cpp here, so every line reports why. A synthesis that fails is logged by the
/// render's own rule (`synthesis_failure_log`) and the encoder carries on — the same shape as the words
/// half's failure above, and for the same reason: one silent sentence is not a reason to lose a video.
pub(crate) fn speak_a_line(line: &crate::produce_exec::ToSpeak) -> Result<(), String> {
    Err(format!(
        "no speech server here: {} was not synthesized",
        line.text.chars().take(24).collect::<String>()
    ))
}

/// The render's translation leg (§F5.4 S3): the real leg — each batch re-reads the settings and dials
/// the LLM over HTTP through `server_leg`, so a ticked language reaches whatever `LLM_SERVER` names and
/// every ask writes its timed row. A server that is not there answers `Err`, which the walk ships as the
/// original text with the merge's warning.
pub(crate) fn translate_leg() -> Box<crate::produce_translate::Ask> {
    let tree = session_tree();
    let stop: std::rc::Rc<dyn Fn() -> bool> = std::rc::Rc::new(|| false);
    crate::produce_translate_ask::ask(&tree, &stop)
}

/// S10's checkpoint question. This page runs the whole render inside one press and the shell's cancel
/// flag is F0.x's, so the answer is always "go on": the seam is here because `produce_exec` is where a
/// stop is *placed* between the subprocesses, and wiring the real flag replaces one closure, not the walk.
pub(crate) fn render_asked(_at: crate::produce_exec::At, _which: &str) -> bool {
    true
}

/// The render's own failure, if it had one, for §B's `produce FAILED: …` line. A stop is not a failure
/// and gets no such line, which is why the report's `stopped` outranks `ok` here.
pub(crate) fn render_failure_note(render: &crate::produce_flow::Rendered) -> Option<String> {
    if render.ok || render.stopped {
        return None;
    }
    Some(render.size.clone())
}

/// F5.6's half of the run, forwarded to `produce_languages::upload_half`, which builds the brief, asks
/// the model and drives the picture. It reports its own failure; `finish_produce` logs it with the spec's
/// tail and the render carries on.
pub(crate) fn words_half(run: &crate::produce_flow::Run) -> Result<(), String> {
    crate::ui::produce_languages::upload_half(run)
}
