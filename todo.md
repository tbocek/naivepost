# Todo

## Simplify the code

Biggest gain first. No behaviour changes; the full suite stays green.

1. **Shared state.** About 173 `thread_local!` blocks hold page state, the main reason the UI code is hard to follow. Move them into per-page state structs. Riskiest item and touches most of the UI, so do it as its own project, one page at a time.
2. **Large files.** Split or thin `rust/src/ui/produce_page.rs` (about 1,700 lines), `rust/src/ui/cut_tracks.rs` and `rust/src/ui/settings.rs`.
3. **Remaining duplicates.** `cut_picture_rows` and `produce_paint`, `window_divider` and `window_run_scroller`, and the `snapshot_*` files.
4. **Functions only tests call.** About 250 functions, roughly 1,500 lines. Some are spec rules the app implements somewhere else, so those tests may check code the app never runs. Needs a careful read, not a script.
