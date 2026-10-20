# naivepost

Desktop video editor for sessions already recorded. Local models transcribe, read
the screen, propose a cut, write and speak a narration, and draft the title,
description, subtitles and thumbnail; the app does the arithmetic, and every
proposal can be overruled. Nothing leaves the machine unless an endpoint points at it.

## Which tree is which

`rust/` is the application being built, written against `spec/`. `gui/` is the Go
prototype `spec/` was written from: read it for observed behaviour; new work goes in `rust/`.

## Stack

- `rust/` — Rust 2021; gtk4-rs 0.11 (`v4_14`), libadwaita 0.9, cairo-rs, gio/glib,
  serde_json. `src/params.rs` catalogues every tuning value and holds no numbers.
- Naivepost computes nothing itself: heavy work is a request to one of four local
  HTTP servers (LLM `:8731`, audio.cpp `:8765`, plus two — endpoints, model ids
  and what waits for what are in `SERVICES.md`) or an ffmpeg/GStreamer subprocess.
- Container: Alpine 3.24 with rust/cargo/gcc, gtk4.0-dev, libadwaita-dev, just,
  xvfb — no display, no GPU. No Go toolchain here; check `command -v go` before
  proposing a Go command (CI runs the Go suite in debian:forky).

## Layout

- `spec/` the rewrite spec: `00-principles.md` … `12-decisions.md`, one file per
  screen or flow; `spec/prompts/` the shipped prompt wording, `spec/inventory/`
  per-screen widget lists, `spec/12-decisions.md` decisions already taken — do not
  re-litigate what is written there.
- `rust/src` app code, one module per concern; `rust/tests` integration tests in
  ONE test binary, `tests/all.rs`: each test file is a module in `tests/suite/` (a new
  file needs its `mod <name>;` line in `tests/suite/mod.rs`, never a `[[test]]` in
  Cargo.toml), one file per spec flow, names like `f1_13_s5_…` = flow, step, case;
  `just test` runs each file in a process of its own; the fixture at
  `rust/fixtures/demo.naivepost` is what snapshots render from.
- `Readme.md` the app's behaviour as it works today; `assets/`, `docs/` icons.

## Commands (from `rust/`)

    just build                 # cargo build --release -> rust/target/release/naivepost
                               # (GTK 4 app: runs on a host with a display, not in this container)
    just test                  # xvfb-run, GSK_RENDERER=cairo; several tests build real widgets;
                               # ~4 min: run it once into a file, then grep/tail the file
    just test f1_13            # only the test files whose name contains f1_13
    just snapshot 05-cut       # -> rust/shots/05-cut.png; one name per spec screen (03-window,
                               # 04-prepare, 05-cut, ...); compare each with spec/img/<same name>

- `src/ui/window.rs` is the shell and the pages; read a function in it with
  `read_file symbol=<name>` rather than by line ranges.- F3.5 ▨ SVG and the other hand-drawn effects (`fx_zoom`/`fx_text`/`cut_speed`) all arm, never
  record, on the dropdown press: the widget layer holds state only and each rule
  lives in its module. ▨ SVG asks for the FILE before it arms, answers `fx_svg::NO_LINE` before
  any chooser opens, and never toggles its arm off on a second deliberate choice.
  `preview-panel` carries ONE left-button drag; its drag-end dispatches svg → text → zoom because two
  button-1 grab handlers on one widget would race and GTK's capture order, not the user's intent,
  would decide. A drawing's camera layer stays UP — its box is measured against the output frame,
  unlike a zoom's, which needs the source. Every form is tab-scoped (`svg-form-<tab label>`:
  page_box runs once per tab, so an unscoped name exists four times per window). Apply goes through
  `record_edit`, never `seed_review_cut`, so one ↶ takes it back; a refused Apply keeps the form open.
  `just snapshot <name>` renders each form by driving the same doors the
  widgets fire (never a state poke); every Cut-page snapshot needs a `set_default_size` before
  `present()` or the page collapses to one column.

- F3.6 🔊 Volume opens `volume-form-<tab label>` and records nothing on the press: no band ≥
  `cut_speed::MIN_MARKED_SECONDS` and no placed line means `fx_volume::NO_SECONDS` before anything
  is drawn. Percent and stored gain are ONE number (`fx_volume::gain_of_percent`/`percent_of`),
  topping out at `fx_record::gain_ceiling()`×100. No visual (`fx_lane::drawn_paused(Volume)` false);
  heard via `ui::paused_preview_gain` = `fx_volume::heard_while_paused`.
- F3.7 🏷 Label opens `label-form-<tab label>` (Name + Length, floor
  `fx_label::MIN_SECONDS` 0.4 s) and records nothing on the press; no band ≥
  `cut_speed::MIN_MARKED_SECONDS` and no placed line means `fx_label::NO_SECONDS`
  before anything is drawn. An EMPTY name is refused with `fx_label::NO_NAME` and the
  form stays open; over 10 chars (`NAME_MAX_CHARS`) it truncates rather than refuses.
  A label is a TAG on the lane (`fx_lane::drawn_paused(Label)` true), never a render filter;
  `narrate_pass` reads it as MARKED.
- F3.8 working a band already on the lane: its DRAWN width is
  `cut_bar_width_px(dur)` floored at 48 px, re-set every refresh, because the grip
  (≥ `fx_band::GRIP_MIN_PX` 30) and ✕ (≥ `KILL_MIN_PX` 32) rules read the
  on-screen width. Pick-up walks the line to the band's start and a drag/nudge
  carries it along (`set_line_position` in `drag_held_band`/`nudge_held_effect`) or
  the refresh reads the old line against the new position and drops the hold. ✎ Edit
  re-finds by kind + start and always reads the LIVE box (`fx_band::live_box`).
- F3.9 captions proposed after the cut run through `ui::run_captions_pass`
  (✐ Captions, gated by `Pass::Captions`, whose grey-out is re-read from the same
  predicate the pass checks). Clips go out in batches of `cut_captions::BATCH`
  (P.machine.captionBatch 5) as clip-relative offsets only; a clip number outside
  the batch loses the WHOLE reply and costs one retry (`cut_captions::retries`),
  under-floor or wordless captions are skipped; accepted ones are text effects via `record_edit`. A
  batch that keeps failing logs `cut_captions::skipped`.
- F3.10 ⏩ Speeds (`ui::run_speeds_pass`, gated by `Pass::Speeds`): ONE call over the kept segments +
  captions + footage total; a rate > 1 over a captioned clip is refused per call and the refusal is
  SAID; rate ≈ 1 ignored; same-rate stretches nearer than `P.policy.speedGapSeconds` merge; two rounds
  then `cut_speed_pass::no_answer()`. Placed via `record_edit`.
- F4.2 The narration call: `src/narrate_call.rs` holds the two-message builder (`request`, plus
  `request_from_tree` for a live session) and `drive`, the attempt loop: entries matched to clips by
  echoed bounds FORWARD ONLY within `narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS`, so a replay of a
  passed clip is refused, not re-sorted; an unanswered clip or an all-empty batch fails the attempt,
  thinking goes off after one that wrote nothing, and rejections reach the log through the caller's closure.
- F3.11 decorations proposed after the cut run through `ui::run_effects_pass`
  (✨ Decorations, also a run-bar job, NOT in `cut-toolbar`) and land by way of
  `cut_effects_pass::place`. ONE call over every kept clip told its captions and its
  rate; a clip outside the list loses the WHOLE reply, while an unknown kind or an
  empty span is skipped per call and SAID. App defaults are the app's, not the tool's:
  zoom centred at `ZOOM_HEIGHT` 0.6 with glide `min(1, d/3)`, a stop recorded as a
  speed of rate 0, volume ramps `min(1, d/4)` with gain 1 or none ignored and 0 kept.
  Two rounds then `cut_effects_pass::no_answer()`, via `record_edit`.
- F3.12 `cut_clamp::clamp_to_cut` (floor `P.eng.effectMinSurvivingSeconds`) runs from TWO doors —
  `suggest::apply` on the model's answer and `ui::clamp_effects_to_cut` after any verb writes its
  segments — and writes via `record_edit`, so one ↶ undoes the clamp as its own step.
- §07-narrate#1-screen: the Narrate page's rules live in `src/narrate_screen.rs` (wording in
  `src/narrate_details.rs`, the tick in `src/narrate_off.rs`); its surface and wires live in
  `src/ui/narrate_page.rs` — `build()` draws it, `wire()` attaches every handler after `set_content`,
  flows push what it shows through `set_state`/`refresh`. Names follow §1's numbering and are listed in
  `spec/inventory/narrate.md`; `refresh` rebuilds the rows whole, so `wire_row_buttons` re-attaches them
  each time. `narrate-picture` and `take-band` are bare `DrawingArea`s painted by a CSS plate class
  (`install_page_css_once`) — no `connect_draw` handler exists in `src/`.
- F4.1 ▶ Write and speak: the run's rules live in `src/narrate_run.rs` — refusal order busy → no cut →
  narration off → no session timeline, `why`/`log_line` for S3's reason, `plan_run`, `fold_written`
  (keeps the user's silent list through a rewrite, 12-decisions §3.24), `speak_line`/`speak_pass`/
  `tally`, and the S7 endings as STATUS-LINE strings. The window seam is `run_narrate_on` +
  `clear_narrate_bar` under the `F4.1` header in `src/ui/window.rs`; ▶ refuses pre-press
  (`narrate_asked`) and FORWARDS at the end of `play.connect_clicked`, with `refuse_narrate_run_with_busy`
  because that forward lands after `runqueue::start_run` already put this press's own run in the bar.
  Tests: `tests/suite/narrate_write_and_speak.rs`, `tests/suite/narrate_run_widgets.rs`.
- F4.3/F4.4/F4.8 narrate rows: `narrate_screen::mirror_fit` runs the render's own
  `produce_render::fit` ladder over a clip's lines together, so a row never promises a remedy the render
  would not apply; `narrate_tts::speak_line` works through refusal order reference → health/clone →
  upload → reply with both network legs as caller-supplied closures (no test dials a socket);
  `narrate_off.rs` answers every narration-off question (`greyed`/`apply_greys`, `set_off` writing only
  `naivepost.json`, `lines_to_speak`, `skips`).
- §08-produce#1-screen: page `src/ui/produce_page.rs` (surface + thin wires, no rule in it), rules
  in `src/produce_screen.rs`, press costs in `src/produce_runs.rs`, the ✎ reword/place dialog in
  `src/ui/produce_words.rs` — an `adw::MessageDialog` is its OWN top level, so a test finds it through
  `produce_words::open_dialog()`, never by walking the page tree. `row_widget` names every settings row
  `setting-<row>` (Translate's ticks live inside `setting-translate`). `refresh` rebuilds that grid
  whole because a row's presence is itself drawn (§F4.8 hides Game audio and keeps its value), so the
  rebuild must be followed by `wire_settings` or the fresh rows are decorative; each control is seeded
  from the held state inside `build_settings`, so a repaint never shows a default. `wire` holds the
  WINDOW in `ROOT`, not just the content root, so `draw_slots` can attach per-slot buttons by name.
  Tests: `produce_screen.rs`, `produce_screen_widgets.rs` (blocks e/f/g).

## Rules the code holds to

- Models propose, the machine places: a model is never asked for a timestamp it
  would have to compute. It answers in words, line numbers or clip-relative
  seconds; the app turns that into a cut from the aligner's word times.
- Every cut lands on a word edge. The audio envelope may only choose *where
  between two known words* a splice falls, never which words it touches.
- Nothing is deleted, only marked: the transcript keeps every word.
- A step's own output file is its resume marker — check it exists first, so an
  interrupted run resumes instead of restarting.
- A tuning number lives once, in the module whose rule uses it, with its `P.*` id.
  `params.rs` catalogues them per section (`prepare()` §04#4, `cut()` §05#6,
  `effects()` §06#6, `produce()` §08#4, `project_settings()` §10#3) and holds none; a value §10 gives no `P.` id takes a bare prefix
  (`machine.`, `preview.`, `project.`).
- `cache/llm` is keyed on the exact request, images included, so an unchanged
  step costs nothing and an edited prompt misses.
- Failure is specific and local: name the model and the reason where it failed,
  carry on elsewhere.
- A project is a folder ending in `.naivepost`; a path in it is stored as
  `project:` + root-relative, absolute nowhere else.
- Tests pin reasons, not only outputs — prompt wording, the order two things happen.
