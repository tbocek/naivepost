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

Every tab, row and transport icon is a named `-symbolic` icon looked up in the desktop's icon
theme — `go/icons/` ships the app icon only. The list is spelled once as `icons::NAMED_SYMBOLIC_ICONS`
and `tests/suite/icon_theme_symbols.rs` fails if a widget asks for a name outside it, or if a tree
cannot resolve one; the AppImage carries `adwaita-icon-theme` inside for this reason. libc follows the toolchain and
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

The window opens at 1240x740 (spec §1) and shrinks down to about 792x538: every page scrolls inside
its own half of the divider, so nothing in the shell pins the window's size. `tests/suite/window_shrinks.rs`
is what keeps that true.

The GUI needs a display, which this devcontainer has none of. In here every screen is rendered
headless instead:

    cd rust && just snapshot 05-cut        # -> rust/shots/05-cut.png

The accepted screen names are the match arms in `src/snapshot.rs` and its `snapshot_*` modules — 40 of
them: `03-log-link`, `03-new-confirm`, `03-policy-form`, `03-runbar`, `03-runbar-busy`,
`03-settings`, `03-sources`, `03-window`, `04-prepare`, `04-prepared`, `04-prompt-picker`,
`05-add`, `05-copy`, `05-cut`, `05-fold`, `05-insert`, `05-insert-form`, `05-lanes`,
`05-remove`, `05-rows`, `05-split`, `05-suggest`, `05-trim`, `05-words`, `06-decorations`,
`06-effect-menu`, `06-flows`, `06-label`, `06-lane`, `06-preview`, `06-speed`, `06-speeds`,
`06-svg`, `06-text`, `06-volume`, `06-zoom`, `07-narrate`, `07-take`, `08-produce`, `08-words`.
Twenty-six of them stand for a picture in `spec/img/`; the rest are program-side screens named after
the flow they record. There is no `--do <widget>` flag; the worked states of a screen get their own
name (`05-split`, `05-trim`, `06-svg`, `06-volume`, `06-label`, `08-words`). Where §1 or §4 pictures a
SUBJECT rather than a page (a popover, the picker list, the lane, the framed preview, §4's sources
crop), that subject takes `<name>.png` and the page it was staged on goes beside it as
`<name>-page.png`.

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

Several tests build real GTK widgets, so they run under xvfb. All test files build into one
binary, `tests/all.rs`, and `just test` runs each file in a process of its own; `just test f1_13`
runs only the files whose name contains `f1_13`. Current state: **518** `test result: ok` lines
(the unit tests, then one per test file), **3266 passed**, **0 failed**. Lint: `cargo clippy --all-targets
--quiet` from `rust/` — clean but for three pre-existing `non_snake_case` test names in
`tests/suite/joins_flow.rs`.

## What the spec covers

`spec/00-principles.md` … `spec/12-decisions.md`, one file per screen or flow, each chapter
starting from the screen, then its flows as numbered steps, then the data and rules behind them.
`spec/prompts/` the prompt wording, `spec/inventory/` per-screen widget lists, `spec/img/` the
reference pictures, `spec/11-flow-index.md` all 68 flows, `spec/12-decisions.md` decisions
already taken. **302 items** are covered by named tests under `rust/tests` (names like
`f1_13_s5_…` = flow, step, case); one item is not done, listed below.

## Not done

One spec item is open:

- **§06-effects#3-flows** — waiting on the user's answer in `spec/QUESTIONS.md:5`: whether the
  captions/speeds/decorations passes offer the F3.9–F3.11 tools on the wire (as `02-services.md`
  §3.7 and each flow's "Tools:" line list them) or send no tools and parse one JSON reply, as §3's
  lead-in sentence reads to the audit. `spec/QUESTIONS.md:11` already records that the prototype
  sends no tools for these three passes. The code today parses one JSON reply.

Gaps found by this pass and left open:

- **Prompts are read from the source tree, not installed beside the binary.**
  `prompts::shipped_file()`/`read_shipped()` walk up from the executable or the working directory
  and accept `<root>/prompts/<key>.md` or `<root>/spec/prompts/<key>.md`, so a checkout does read
  its shipped wording (that is what the bench shows and what `shipped_text` returns). Nothing
  copies `spec/prompts/` next to an installed binary, so an install must ship them there or the
  bench starts from the per-machine files under `~/.config/naivepost/prompts`. Needs a decision:
  vendor the prompts into the package, or declare them user-supplied.
- **Eight §10 parameter rows have no reader.** `params.rs` catalogues 70 ids, each naming the const
  that holds its value; eight of those consts appear nowhere outside the catalogue and in no test:
  `cut.panedTopPx`, `cut.panedPreviewPx`, `effects.renderZoomDepthCap`, `effects.renderZoomFps`,
  `effects.renderTierShortSide` (the render-tier trio belongs to the §06 flows item above) plus
  `effects.textAscent`, `effects.textLineHeight`, `effects.textMinPoints`. They are catalogued but
  never read through it, not wrong.
- **Snapshot fidelity.** All 40 accepted names render, and every one of the 26 `spec/img/*.png`
  pictures has a twin in `rust/shots/`. Remaining differences are content, not layout: the spec
  pictures show a lecture day (8 sources, thumbnails along the tape) while `fixtures/demo.naivepost`
  carries one source and a two-clip cut; `05-add`, `05-trim` and `05-remove` are drawn as
  before/after pairs in the spec while one snapshot writes one PNG (the after state plus the status
  sentence it logged); `04-prompt-picker`'s page twin stays dark on purpose, because the light-theme
  CSS pin is scoped to the picker subject alone. Fourteen names have no spec picture at all
  (`03-log-link`, `03-policy-form`, `03-runbar`, `03-runbar-busy`, `04-prepared`, `05-copy`,
  `05-insert`, `05-insert-form`, `05-suggest`, `05-words`, `06-flows`, `06-speeds`,
  `06-decorations`, `06-svg`), and `03-runbar.svg` is a proposed-state drawing, not a screen.

Checked this pass and found sound, so recorded rather than fixed: every format the spec names has
both a reader and a writer (`naivepost.json`, `requests.tsv`, `cut/cut.json` — read at `cut.rs::load`,
written only by `cut.rs::save`; `retakes.tsv`, `transcript.txt`/`.tsv`, `narration.json`,
`takes.json`, `publish.json`, `scenes.tsv`, `voice_ref_base.wav`, `produce/final.stamp`, and the
`final[.<code>].srt`/`.vtt` sidecars); the shell draws every screen (§1's four tabs, the settings
window through `settings-button`, the log expander and the status line); and every header-bar and
run-bar badge §1 numbers exists as a named widget. The inventory files describe controls by their
visible label (＋ Add, ⇲ Lane, ✐ Captions) and never by widget name, so flow entry points cannot be
checked mechanically against them — they were checked by rendering each screen instead.

Nothing here commits anything; `git commit` is left to whoever wants it.
