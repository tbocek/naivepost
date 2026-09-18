# Naivepost — rewrite specification

<!-- nav -->
<span>← start</span> · [↑ All flows](11-flow-index.md) · [00 Principles and rewrite directives →](00-principles.md)

**Prompts:** [captions](prompts/captions.md) · [cut](prompts/cut.md) · [describe](prompts/describe.md) · [effects](prompts/effects.md) · [fix](prompts/fix.md) · [narrate](prompts/narrate.md) · [policy](prompts/policy.md) · [retake](prompts/retake.md) · [speed](prompts/speed.md) · [system](prompts/system.md) · [textedit](prompts/textedit.md) · [tools](prompts/tools.md) · [translate](prompts/translate.md) · [youtube](prompts/youtube.md)

**Inventories:** [cut](inventory/cut.md) · [effects](inventory/effects.md) · [narrate](inventory/narrate.md) · [prepare](inventory/prepare.md) · [produce](inventory/produce.md) · [shell](inventory/shell.md)
<!-- /nav -->

Specifies Naivepost fully enough to rebuild it, same features, in another language. Written from the prototype in `gui/` (Go, GTK4, ~106k lines); to be reviewed and adapted before the rewrite starts.

The spec is **UI driven**: each chapter starts from the screen, then its flows as numbered steps, then the data, rules and parameters behind them. Behaviour implicit in the prototype (a code constant, a parser heuristic) is named as a parameter with a proposed home; model calls are re-specified as tool-driven flows (see [`00-principles.md`](00-principles.md)).

## How to read

| File | Contents |
|---|---|
| [`00-principles.md`](00-principles.md) | Design rules, the two rewrite directives (no implicit constants; tools-first), the policy object, glossary |
| [`01-project-and-files.md`](01-project-and-files.md) | The project on disk: every folder, file and format |
| [`02-services.md`](02-services.md) | The four local servers, endpoints used, and the tool catalogue the app offers the model |
| [`03-shell.md`](03-shell.md) | Window, tabs, run bar, log, settings, project open/new/save — flows F0.x |
| [`04-prepare.md`](04-prepare.md) | Prepare tab and pipeline — flows F1.x |
| [`05-cut.md`](05-cut.md) | Cut tab: timeline, transport, editing, suggest, inserts — flows F2.x |
| [`06-effects.md`](06-effects.md) | Effects on the Cut page and their rendering — flows F3.x |
| [`07-narrate.md`](07-narrate.md) | Narrate tab — flows F4.x |
| [`08-produce.md`](08-produce.md) | Produce tab: render, subtitles, thumbnail, upload text — flows F5.x |
| [`09-llm-and-tools.md`](09-llm-and-tools.md) | LLM client contract, gate, cache, exchange log, stall watch, tool protocol |
| [`10-parameters.md`](10-parameters.md) | Every parameter the prototype held as a constant: value, meaning, proposed home |
| [`11-flow-index.md`](11-flow-index.md) | All flows in one table, plus the top-level UI flow map |
| [`12-decisions.md`](12-decisions.md) | What the model decides and what the app decides, job by job — the audit behind directive B |
| [`prompts/*.md`](prompts) | The twelve shipped prompts, verbatim |
| [`inventory/*.md`](inventory) | Raw inventories extracted from the code (source material; keep for cross-checking) |

## Conventions

- **Flows**: numbered `F<tab>.<n>` (F0 shell, F1 Prepare, F2 Cut, F3 Effects, F4 Narrate, F5 Produce, F6 LLM/tools); steps `S1, S2 …`; branches `S3a`, `S3b`.
- **Screens**: screenshots of the prototype on a real project (the ETH lecture), in `img/`, with numbered red markers and a key under each. A screen the prototype lacks (the editing-policy form) is an SVG tagged "proposed". Layout indicative; widgets, their order, labels and tooltips normative. Staged states (demo narration lines, effects placed for a shot) are said so in the caption.
- **Navigation**: every page opens and closes with a bar — ← previous chapter · ↑ contents · next chapter → — and a chapter with flows opens with the list of them. Every flow heading carries ← previous flow · ↑ its chapter · all flows · next flow →, so flows read end to end across chapters. Every chapter, section (§), flow id and prompt named in the text is a link. The bars sit between `<!-- nav -->` markers; a chapter, section or flow added later needs its bars and its neighbours' bars edited to match.
- **Every flow is drawn and written**: under each `F<n>.<m>` heading, a Mermaid flowchart (red: a refusal; green: where the flow ends well), or real before/after screenshots for the editing verbs, then the numbered steps. Mermaid renders on GitHub and GitLab; other viewers show its source. Where a diagram and the steps disagree, the steps are normative.
- **Parameters**: `P.<area>.<name>`, the area being the value's home ([`00-principles.md` §3](00-principles.md#3-rewrite-directive-a--no-implicit-behaviour)): `P.policy.*` editing policy (project, derived from the User Context), `P.machine.*` machine settings, `P.project.*` project settings, `P.eng.*` engineering constants. Every cited name is a row in [`10-parameters.md`](10-parameters.md); a flow that reads one says so.
- **Tools** the model may call: `tool:<name>`, defined in [`02-services.md` §3](02-services.md#3-tool-catalogue-rewrite-directive-b) and [`09-llm-and-tools.md`](09-llm-and-tools.md).
- **Strings in quotes** ("…"): the prototype's user-visible text; keep unless the review changes it. Log strings are quoted with the prototype's prefix — `>>> ` progress, `!!! ` failure, four spaces detail — only to name their level. New: the log prints the text alone, no prefix and no indent; a failure shows in red, a detail line dimmed.
- **MUST / SHOULD / MAY** in their usual sense. "Prototype:" notes how the Go code did it where the spec proposes otherwise.

## Scope

In: everything the prototype does on its four tabs, the shell, the files it writes, the servers it talks to, the model calls it makes. Out: Go/GTK implementation details (widget classes, goroutines), packaging (Flatpak/AppImage), the marketing site.

## Status

Draft for review. Every chapter verified against the code a second time (tab chapters carry "Details confirmed against the code"; 01, 02, 09, 10 had corrections folded in; the twelve prompts in [`prompts/`](prompts) diff clean against the shipped text). Every change from the prototype is stated as decided, with "Prototype:" saying what the code did and "New" what the code has not got at all. **REVIEW** marks the few decisions still open for the reviewer.

<!-- nav -->
---
<span>← start</span> · [↑ top](#naivepost--rewrite-specification) · [↑ All flows](11-flow-index.md) · [00 Principles and rewrite directives →](00-principles.md)
<!-- /nav -->
