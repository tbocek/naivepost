//! §08-produce#1-screen — the render legs this page hands to `produce_exec`: the session folder, the
//! speak call, the translation ask, the checkpoint answer, and the two failure notes. Split out of
//! `produce_page.rs`, which is past its size budget; the page keeps every call site.

/// The session's folder, resolved the way the page resolves everything else on this path.
pub(crate) fn session_tree() -> crate::layout::Tree {
    let root = std::env::current_dir().unwrap_or_default();
    crate::layout::Tree::new(crate::startup::session_dir(&root))
    .unwrap_or_else(|_| crate::layout::Tree::new(std::path::Path::new("session.naivepost")).unwrap())
}

/// S2's speak, over the real wire: [`crate::ui::produce_speak_leg`]'s door, which is the Narrate page's
/// own (`narrate_run_speak::dial_line` → `speak_leg::speak`), so a ▶ run dials the same audio.cpp address
/// a row's ▶ dials and files the take where `produce_stamp::wav_of` looks for it. A synthesis that fails
/// is logged by the render's own rule (`synthesis_failure_log`) and the encoder carries on — the same
/// shape as the words half's failure below, and for the same reason: one silent sentence is not a reason
/// to lose a video.
///
/// The voice travels from the run, so a take spoken by a ▶ run is the take its stamp weighs: with nothing
/// in `voice` both doors key the line as the session's own voice.
pub(crate) fn speak_a_line(
    line: &crate::produce_exec::ToSpeak,
    voice: &str,
) -> Result<(), String> {
    crate::ui::produce_speak_leg::speak_one(&session_tree(), line, voice)
        .map(|take| crate::ui::window::log_line(&format!("spoken \u{2014} {}", take.display())))
}

/// The render's translation leg (§F5.4 S3): the real leg — each batch re-reads the settings and dials
/// the LLM over HTTP through `server_leg`, so a ticked language reaches whatever `LLM_SERVER` names and
/// every ask writes its timed row. A server that is not there answers `Err`, which the walk ships as the
/// original text with the merge's warning.
///
/// §3's patience rides this leg too (`llm_retry_leg::chat` waits 5 s → 4 min on a transport death), and it
/// polls the switch this closure reads, so handing back a closure that always says false would leave ⏹
/// dead in front of a wait of up to eight minutes. It goes through [`crate::cancel_leg::cancel_check_now`],
/// the same door every other live ask in the app uses (`join_ask`, `narrate_ask`, `speak_leg`, the cut
/// passes): the CURRENT generation, because the render's batches belong to whichever run is going right
/// now, and `runqueue::bump_cancel_context` clears the switch per run so a ⏹ aimed at an earlier one
/// cannot stop this leg. `render_asked` below answers the same switch for the checkpoint between steps.
pub(crate) fn translate_leg() -> Box<crate::produce_translate::Ask> {
    let tree = session_tree();
    let stop: std::rc::Rc<dyn Fn() -> bool> =
        std::rc::Rc::new(|| crate::cancel_leg::cancel_check_now()());
    crate::produce_translate_ask::ask(&tree, &stop)
}

/// S10's checkpoint question, answered by the shell's own stop switch.
///
/// ⏹ writes [`crate::cancel_leg`] (`run::RunBar::press_stop` → `cancel_now`) and nothing else can: so a
/// run that is still walking its subprocesses stops at the next checkpoint exactly as §F5.2 S10 asks,
/// and §F5.1 S6's "production stopped" becomes reachable from a real press instead of only from an
/// injected stub. A leg that belongs to no run never sees a stop, because `start_run` clears the switch
/// for each new generation (`runqueue::bump_cancel_context`) — a ⏹ aimed at the run before cannot stop
/// this one.
pub(crate) fn render_asked(_at: crate::produce_exec::At, _which: &str) -> bool {
    !crate::cancel_leg::cancelled()
}

/// The same answer in the shape `produce_exec::run_render` takes as its `ask` argument: false once ⏹ has
/// been pressed, so the walk halts between subprocesses and reports where it stopped. Split out only so
/// `produce_snapshot::render_half` can hand the closure without naming this module's private items.
pub fn render_checkpoint(_at: crate::produce_exec::At, _which: &str) -> bool {
    render_asked(_at, _which)
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
