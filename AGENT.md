# naivepost

Desktop video editor for sessions already recorded: local models transcribe, read the
screen, propose a cut, write and speak a narration and draft the upload text; every
proposal can be overruled. `spec/` says what to build; this file says how to work here.

## Trees

`rust/` is the application, built against `spec/`. `go/` is the Go prototype the spec
was written from: read it for observed behaviour; new work goes in `rust/`.

## Environment

- Container: Alpine (`.devcontainer/`) with the Rust toolchain, GTK 4 and libadwaita
  dev packages, just and Xvfb. No display and no GPU: the app's window exists only in
  tests and in `just snapshot`, both under Xvfb. No Go toolchain; check `command -v go`
  before proposing a Go command.
- Heavy work is a request to a local HTTP server (endpoints and model ids in
  `SERVICES.md`) or an ffmpeg/GStreamer subprocess.

## Layout

- `spec/`: one file per screen or flow; `spec/prompts/` the shipped prompt wording,
  `spec/inventory/` per-screen widget lists, `spec/12-decisions.md` decisions already
  taken: do not re-litigate them.
- `rust/src`: one module per concern. A tuning value lives in the module whose rule
  uses it, with its `P.*` id; `src/params.rs` only catalogues them.
- `rust/tests`: ONE test binary, `tests/all.rs`. Each test file is a module in
  `tests/suite/` and needs its `mod <name>;` line in `tests/suite/mod.rs`; never a
  `[[test]]` in Cargo.toml.
- `rust/fixtures/demo.naivepost`: the project snapshots render from.

## Commands (from `rust/`)

    just build              # release build; the window cannot be shown in this container
    just test               # all test files side by side; run it once into a file,
                            # then grep/tail the file instead of running it again
    just test <part>        # only the test files whose name contains <part>
    just snapshot <name>    # -> rust/shots/<name>.png, named after its spec/img/<name>.png

## Conventions

- Every change to the cut goes through `ui::record_edit`, so one ↶ takes it back.
