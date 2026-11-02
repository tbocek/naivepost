// F3.6 Volume by hand — the wire: a click on 🔊 Volume reaches `fx_volume`'s S1–S5 through real widgets, the
// "Volume a – b" form draws its five fields in the spec's order, and Apply puts ONE record on the cut that one
// ↶ takes back.
//
// `naivepost::fx_volume` holds the rules (proven by tests/suite/cut_volume_hand.rs, which this file does not touch);
// this proves the Cut page can actually reach them. Before F3.6's redo there was no `volume-form-*`, no
// `press_volume_item` and no Apply button at all — the dropdown row fell through `press_effect_item` into the
// generic `fx_record::record_into` path, so every rule in the module was unreachable from the UI. Every check
// here fires a widget the way GTK does (`emit_by_name("clicked")`) and compares against the module rather than
// a retyped string, so a paraphrase or a dropped wire fails loudly.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.
