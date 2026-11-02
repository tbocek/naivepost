// F3.1 S1 + S2 — the wire: arming ⊕ Zoom and dragging a box on the preview reach `fx_zoom`'s rules through
// the real widgets.
//
// `naivepost::fx_zoom` holds the arm, the free rectangle and every default (proven by tests/suite/cut_zoom_by_hand.rs);
// this file proves the Cut page's named widgets reach them. Every check fires a widget the way GTK does
// (`emit_by_name("clicked")`, `emit_by_name("drag-end")`) or reads state the press left behind, and asserts the
// SAME answers the logic test asserts -- the refusal string, the arm flag, the camera layer's class and opacity,
// the form's own fields -- never a painted pixel.
//
// One application, one `connect_activate`, exactly one `Application::run`: `g_application_run` refuses a second
// claimant of the default main context. cwd is pinned to this binary's own temp root BEFORE anything builds,
// because the page resolves its session through `startup::session_dir(current_dir())`.

thread_local! {
    static LAST_BUILD: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}
