// F3.7 Label by hand — the wire: a click on 🏷 Label reaches `fx_label`'s S1–S5 through real widgets, the
// "Label at m:ss" form draws its two fields in the spec's order, an empty name is refused without placing
// anything, and Apply puts ONE mark on the cut that one ↶ takes back.
//
// `naivepost::fx_label` holds the rules (proven by tests/suite/cut_label_hand.rs, which this file does not touch);
// this proves the Cut page can actually reach them. Before F3.7's redo there was no `label-form-*`, no
// `press_label_item` and no Apply button at all — the dropdown row fell through `press_effect_item` into the
// generic `fx_record::record_into` path, which laid down an UNNAMED record and so skipped both of §F3.7's
// refusals and the whole form. Every check here fires a widget the way GTK does
// (`emit_by_name("clicked")`) and compares against the module rather than a retyped string, so a paraphrase or
// a dropped wire fails loudly.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.
