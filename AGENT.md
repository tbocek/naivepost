# naivepost

Desktop video editor for sessions already recorded. Local models transcribe, read
the screen, propose a cut, write and speak a narration, and draft the title,
description, subtitles and thumbnail; the app does the arithmetic, and every
proposal can be overruled. Nothing leaves the machine unless an endpoint points at it.

## Which tree is which

`rust/` is the application being built, written against `spec/`. `gui/` is the Go
prototype `spec/` was written from: read it for observed behaviour, put new work in
`rust/`.

## Stack

- `rust/` — Rust 2021; gtk4-rs 0.11 (`v4_14`), libadwaita 0.9, cairo-rs, gio/glib,
  serde_json. `src/params.rs` catalogues every tuning value and holds no numbers.
- `gui/` — Go 1.26.5; gotk4 0.4.1, go-gst 1.4.1, coder/websocket. Prototype.- Naivepost computes nothing itself: heavy work is a request to one of four local
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
- `rust/src` app code, one module per concern; `rust/tests` integration tests, one
  file per spec flow, names like `f1_13_s5_…` = flow, step, case; the fixture at
  `rust/fixtures/demo.naivepost` is what snapshots render from.
- `Readme.md` the app's behaviour as it works today; `assets/`, `docs/` icons and
  screenshots; `release.sh` + `ch.bocek.naivepost.yml` packaging.

## Commands (from `rust/`)

    just test                  # xvfb-run, GSK_RENDERER=cairo; several tests build real widgets
                               # ~4 min: run detached to a log, a piped run dies at ~2 min idle
    just snapshot 03-window    # -> rust/shots/03-window.png (03-sources, 04-prepare, 05-cut, …)
                               # names are accepted for every page, but only Prepare draws widgets;
                               # so no Cut/Narrate/Produce screen can be compared with spec/img yet

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
  (`machine.`, `preview.`, `project.`). §10 §3 spells only one `P.project.*` id — Freq —
  and `params::Family::Project` answers to that one alone.
- `cache/llm` is keyed on the exact request, images included, so an unchanged
  step costs nothing and an edited prompt misses.
- Failure is specific and local: name the model and the reason where it failed,
  carry on elsewhere.
- A project is a folder ending in `.naivepost`; a path in it is stored as
  `project:` + root-relative, absolute nowhere else.
- Tests pin reasons, not only outputs — prompt wording, the order two things happen.
