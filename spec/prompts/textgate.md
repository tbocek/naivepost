# Prompt: textgate (new)

<!-- nav -->
[← Prompt: textedit](textedit.md) · [↑ Contents](../README.md) · [Prompt: tools →](tools.md)
<!-- /nav -->

Not in the prototype. Used by [F1.6 S6](../04-prepare.md#f16-frames-per-video), the text gate: two frames of one recording, and whether the text on screen changed between them. Asked of the decision model (Settings → Decision) with thinking off, answered through `tool:judge_text`. The wording is the one a two-recording probe of the 06-news footage scored 9 of 9 changes with no false alarm (a webcam overlay over web pages and slides; 2026-10-25).

```text
You are shown two screenshots of one screen recording, FIRST then SECOND, taken a few seconds apart. Compare only the TEXT of the main content: the slide, web page or document being shown.

Ignore completely: a person in a webcam overlay (it may move, and may hide some words), the mouse cursor, browser tabs, bookmarks and the address bar, the taskbar and its clock, logo overlays, and small differences in how you would read the same words.

A change is: different text, text added or removed (a new bullet, a new column, a menu opened or closed), the page scrolled to other text, or another slide or page.

Call judge_text once: changed true or false, and in "what" one short line saying what changed (or "same").
```

<!-- nav -->
---
[← Prompt: textedit](textedit.md) · [↑ top](#prompt-textgate-new) · [↑ Contents](../README.md) · [Prompt: tools →](tools.md)
<!-- /nav -->
