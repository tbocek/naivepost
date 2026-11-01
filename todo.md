# Todo

## Simplify the code

Biggest gain first. No behaviour changes; the full suite stays green.

1. **Shared state.** About 174 `thread_local!` blocks hold page state, the main reason the UI code is hard to follow. Move them into per-page state structs. Riskiest item and touches most of the UI, so do it as its own project, one page at a time.
2. **Large files.** `rust/src/ui/produce_page.rs`, `cut_tracks.rs` and `settings.rs` are split. What is left to thin: `build_owned` in `settings.rs`, about 400 lines in one function.
3. **Remaining duplicates.** Exact copies are shared. Left on purpose, because the names or values differ per module: `stat_recording`, `two_messages`, `human_weight`, `panel_height`, `seeded`. Worth a look by hand: the form hands (`press_*_apply`, `wire_*_buttons`), `text_gate::stage` and `describe_call::stage`, `drive_walk` and its `_for_test` twin.
4. **Functions only tests call.** 308 functions, about 2,300 lines; 73 of them are `_for_test` seams (about 550 lines), which are fine. Most lines: `produce_details`, `audit_gaps`, `ui/narrate_band`, `render_fx`, `run_stages`, `cut_trim`, `fx_record`, `tools/describe`, `decision_audit`. Some are spec rules the app implements somewhere else, so those tests may check code the app never runs. Needs a careful read, not a script: deleting one also deletes its tests.

## Noticed, not changed

- `narrate_preview_leg::unquoted` and `settings::unquote` differ: `\\n` in the first is a real newline, in the second the letter n. The first looks like a bug.
