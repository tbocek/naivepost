# Naivepost — rewrite specification

This folder specifies Naivepost completely enough to rebuild it in another language with the same features. It was written from the prototype in `gui/` (Go, GTK4, ~106k lines) and is meant to be reviewed and adapted before the rewrite starts.

The spec is **UI driven**: every chapter starts from what is on screen, then describes each flow the screen offers as numbered steps, then the data, rules and parameters behind it. Behaviour that was implicit in the prototype (a constant in the code, a heuristic in a parser) is named as a parameter with a proposed home, and model calls are re-specified as tool-driven flows (see `00-principles.md`).

## How to read

| File | Contents |
|---|---|
| `00-principles.md` | Design rules, the two rewrite directives (no implicit constants; tools-first), the policy object, glossary |
| `01-project-and-files.md` | The project on disk: every folder, file and format |
| `02-services.md` | The four local servers, endpoints used, and the tool catalogue the app offers the model |
| `03-shell.md` | Window, tabs, run bar, log, settings, project open/new/save — flows F0.x |
| `04-prepare.md` | Prepare tab and pipeline — flows F1.x |
| `05-cut.md` | Cut tab: timeline, transport, editing, suggest, inserts — flows F2.x |
| `06-effects.md` | Effects on the Cut page and their rendering — flows F3.x |
| `07-narrate.md` | Narrate tab — flows F4.x |
| `08-produce.md` | Produce tab: render, subtitles, thumbnail, upload text — flows F5.x |
| `09-llm-and-tools.md` | LLM client contract, gate, cache, exchange log, stall watch, tool protocol |
| `10-parameters.md` | Every parameter the prototype held as a constant: value, meaning, proposed home |
| `11-flow-index.md` | All flows in one table, plus the top-level UI flow map |
| `prompts/*.md` | The twelve shipped prompts, verbatim |
| `inventory/*.md` | The raw inventories extracted from the code (source material; keep for cross-checking) |

## Conventions

- **Flows** are numbered `F<tab>.<n>` (F0 shell, F1 Prepare, F2 Cut, F3 Effects, F4 Narrate, F5 Produce, F6 LLM/tools). Steps inside a flow are `S1, S2 …`. A step that branches lists its branches as `S3a`, `S3b`.
- **Screens** are drawn as ASCII mockups. Layout is indicative; the widgets, their order, labels and tooltips are normative.
- **Parameters** are written `P.<area>.<name>` and collected in `10-parameters.md`. A flow that reads one says so.
- **Tools** the model may call are written `tool:<name>` and defined in `02-services.md` §3 and `09-llm-and-tools.md`.
- **Strings in quotes** ("…") are user-visible text from the prototype and should be kept unless the review changes them. Log lines start with `>>> ` (progress), `!!! ` (failure) or four spaces (detail).
- **MUST / SHOULD / MAY** are used in their usual sense. "Prototype:" marks a note about how the Go code did it when the spec proposes something different.

## Scope

In scope: everything the prototype does on its four tabs, the shell around them, the files it writes, the servers it talks to, and the model calls it makes. Out of scope: the Go/GTK implementation details (widget classes, goroutines), packaging (Flatpak/AppImage), and the marketing site.

## Status

Draft for review. Sections marked **REVIEW** are places where the spec proposes a change from the prototype (mostly: a constant becoming a parameter, or a parsed reply becoming a tool call) and the reviewer should confirm or adapt.
