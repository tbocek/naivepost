// §06-effects#1-record / F3.1 S1 — the wire: a click on the ✚ Effect dropdown reaches each kind's rule through
// the real buttons.
//
// `naivepost::fx_record` holds the record's rules (proven by tests/suite/effect_record_creation.rs); this file proves
// the Cut page's six named dropdown rows reach them. Five kinds record straight through the shared closure; ⊕ Zoom
// does NOT -- §F3.1 S1 makes it ARM a drag instead, adding nothing to the cut until a box is drawn (see
// tests/suite/zoom_drag_widgets.rs for that placement). Every check fires a widget the way GTK does
// (`emit_by_name("clicked")`) and asserts the SAME state the logic test asserts — the cut's `fx` list, the kind
// recorded, the whole-bed reading, the exact status sentence — never a painted pixel.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.
