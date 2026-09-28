# naivepost

A desktop editor for sessions that are already recorded — a lecture, a game, a build, a
conversation, captured once with a camera, a separate microphone and a screen capture. Local
models transcribe the audio, read the screen, propose a cut, write and speak a narration, and
draft the title, description, subtitles and thumbnail. The app does the arithmetic: every
proposal lands on a word edge, every one can be overruled, and nothing is deleted, only marked.
Naivepost computes nothing itself. Heavy work is a request to one of four local HTTP servers
(LLM on `:8731`, audio.cpp on `:8765`, plus two more — endpoints, model ids and what waits for
what in [`../SERVICES.md`](../SERVICES.md)) or an ffmpeg/GStreamer subprocess. Nothing leaves
the machine unless an endpoint points at it.

## Build

    cd rust && just build          # cargo build --release -> rust/target/release/naivepost

System libraries, by base image: Alpine 3.24 wants `gtk4.0-dev libadwaita-dev rust cargo gcc
musl-dev`; Debian and Ubuntu want `libgtk-4-dev libadwaita-1-dev`. libc follows the toolchain and
not a flag, so do not add a `just` `os()` branch to pick one — the same recipe links musl on
Alpine and glibc on a Debian host. Cross-compiling (a glibc host wanting a musl binary, or the
reverse) needs a second toolchain this image does not have: `rustc` comes from apk and there is
no `rustup`.

## Run

    ./target/release/naivepost path/to/project.naivepost

A project is a folder ending in `.naivepost`; a path inside it is stored as `project:` plus a
root-relative path, absolute nowhere else. With no argument the last opened project is used, and
the desktop can hand a folder over directly (`*.naivepost` is registered as a MIME package).
`./target/release/naivepost --help` prints both modes without needing a display.

The GUI needs a display, which this devcontainer has none of. In here every screen is rendered
headless instead:

    cd rust && just snapshot 05-cut        # -> rust/shots/05-cut.png

The accepted screen names are the match arms in `src/snapshot.rs` — `03-window`, `03-settings`,
`03-sources`, `03-new-confirm`, `03-policy-form`, `04-prepare`, `04-prompt-picker`, `05-cut`,
`05-add`, `05-fold`, `05-rows`, `05-split`, `05-trim`, `05-remove`, `05-lanes`, `06-preview`,
`06-zoom`, `06-speed`, `06-text`, `06-volume`, `06-label`, `06-lane`, `06-svg`,
`06-effect-menu`, `07-narrate`, `07-take`, `08-produce`, `08-words` — one per picture in
`spec/img/`. There is no `--do <widget>` flag; the worked states of a screen get their own name
(`05-split`, `05-trim`, `06-svg`, `06-volume`, `06-label`, `08-words`).

Headless work needs `GSK_RENDERER=cairo GDK_BACKEND=x11 GTK_A11Y=none GSETTINGS_BACKEND=memory`
under `xvfb-run -a` — exactly what the `test` and `snapshot` recipes set, so prefer the recipes
to typing it.

## Run it for real, on your own display

The container borrows the host's Wayland socket, GPU and font:

```Dockerfile
FROM alpine:3.24
RUN apk add --no-cache gtk4.0 libadwaita mesa-dri-gallium font-dejavu
ARG UID=1000
RUN adduser -D -u $UID user
USER user
```

    docker run --rm -it \
      -e XDG_RUNTIME_DIR=/tmp \
      -e WAYLAND_DISPLAY=$WAYLAND_DISPLAY \
      -e GDK_BACKEND=wayland \
      -v $XDG_RUNTIME_DIR/$WAYLAND_DISPLAY:/tmp/$WAYLAND_DISPLAY \
      --device /dev/dri \
      <image> naivepost <project.naivepost>

The Mesa DRI drivers plus `--device /dev/dri` give it the GPU, the font keeps text from rendering
as boxes, and matching UID makes the socket accessible. Sample data in this tree:
`rust/fixtures/demo.naivepost` (a `naivepost.json` plus `cut/`) is what every snapshot renders
from; `rust/session.naivepost` is a fuller one with `prepare/` as well.

## Test

    cd rust && just test > /tmp/test.log 2>&1     # ~3-4 min; then read the summary in the file

Several tests build real GTK widgets, so they run under xvfb. Current state: **242**
`test result: ok` lines, **2013 passed**, **0 failed**. Lint: `cargo clippy --all-targets
--quiet` from `rust/`.

## What the spec covers

`spec/00-principles.md` … `spec/12-decisions.md`, one file per screen or flow, each chapter
starting from the screen, then its flows as numbered steps, then the data and rules behind them.
`spec/prompts/` the prompt wording, `spec/inventory/` per-screen widget lists, `spec/img/` the
reference pictures, `spec/11-flow-index.md` all 68 flows, `spec/12-decisions.md` decisions
already taken. 288 items are covered by named tests under `rust/tests` (names like
`f1_13_s5_…` = flow, step, case). The root `Readme.md` describes the app's behaviour as it
works today.

## Not done

Blocked at the last ledger pass:

- **§12-decisions#narrate-and-produce** — no test names it; the other three sections of that
  chapter are covered (`#prepare`, `#cut-and-effects`, `#1-the-five-homes`, `#5-gaps-…`).
- **F4.6 Choose the voice and build the reference** — no test and no implementation of the
  automatic reference. Only hand-picked takes work; the five `ref*` parameters are deliberately
  absent from the catalogue because there is no rule behind them yet.
- **F4.7 Edit lines** — no test. The widgets exist (`narrate-add-line`, per-row buttons) and some
  rules are in (`narrate_screen::remove_line`, the 1 s "a line already starts here" window), but
  the flow's own steps are untested.
- **F0.3, F0.5, F1.10, F3.9** — blocked for missing visual review, not for missing code. Each
  has passing step tests (`press_stop.rs`, `run_bookkeeping.rs`, `joins_widgets.rs`,
  `cut_captions_proposed.rs`) and this pass did render and look at their screens.
- **F2.2 Play the cut** — blocked at the ledger because the planner asked a question instead of
  guessing. Its step tests exist and pass (20 hits in `rust/tests`), so this is an unanswered
  spec question rather than missing work.
- **F4.5** — was sent back for a redo with no code changed; its tests now pass
  (`narrate_preview_cut.rs`, 12 cases).
- **§08-produce#1-screen** — cleared this pass: the uncalled `last_entry_text` in
  `src/ui/produce_words.rs` was deleted.

Gaps found by this pass and left open:

- **Shipped prompt wording is not in the tree.** `prompts::shipped_file()` answers
  `prompts/system.md` and friends, but no `prompts/` directory exists and nothing calls
  `shipped_file()`. A fresh install therefore has no system prompt until the user supplies one
  under `~/.config/naivepost/prompts/<key>.txt`; what gets sent is assembled by
  `prompt_assembly::cut_system`/`user_message` from caller-supplied pieces. Needs a decision:
  vendor `prompts/` into the repo and wire the loader, or declare prompts user-supplied-only.
- **18 of the 28 snapshot names render the base page.** Only 10 have a seed block in
  `src/snapshot.rs` (`04-prompt-picker`, `05-cut`, `05-fold`, `05-rows`, `06-label`,
  `06-lane`, `06-svg`, `06-volume`, `07-narrate`, `08-produce`); the rest collapse into two
  identical groups (Prepare-at-rest, bare Cut page) rather than showing the distinct state their
  `spec/img/` picture shows.
- **Some Cut-page snapshots render narrow.** A Cut-page shot needs `set_default_size` before
  `present()` or the page collapses to one column; the seeded screens do this, the unseeded ones
  (`06-preview`, `05-add`, `05-lanes`, …) do not.
- `03-runbar.svg` has no snapshot name — it is a proposed-state drawing, not a renderable screen.

Nothing here commits anything; `git commit` is left to whoever wants it.
