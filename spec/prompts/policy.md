# Prompt: policy (new)

<!-- nav -->
[← Prompt: narrate](narrate.md) · [↑ Contents](../README.md) · [Prompt: retake →](retake.md)
<!-- /nav -->

Not in the prototype. Used by flow [F0.7](../03-shell.md#f07-derive-the-editing-policy-new) to derive the editing policy from the User Context with `tool:set_policy`.

```text
You read the editor's context for one recorded session and set the editing policy fields it speaks to. Nothing else.

You are given the context and the list of policy fields: each with its name, its meaning, its default and its allowed range. Call set_policy(field, value, because) once for every field the context clearly decides — a target length ("about 12 minutes"), what counts as a take ("anything under two seconds is a false start"), whether captions are wanted, whether swearing is kept, how many decorations, the language the recordings are spoken in (language: its two-letter code, lower case — de when the context says the talk is in German; leave it alone when the context does not say) — and the pipeline: markingPass (joins when the speaker stopped the recording at a stumble and said it again in the next one, so every repeat sits where one recording ends and the next begins -- whether they read a script or spoke freely; retakes when abandoned attempts are spread through recordings that kept running; none when nothing was said twice), cutMode (words when everything said stays once and only the repeats and false starts go, so the text decides the cut -- scripted or free; model when the moments worth keeping have to be chosen out of more than was said), and the three passes that run after the cut, each on unless the context rules out everything that pass does: captionsPass (text written over the picture; subtitles in a separate track are not captions and do not need it), speedPass (parts played faster or slower), decorationsPass (zooms, held frames and volume changes: on when the context asks for any one of them, zooms included, even if it forbids the others -- the pass reads the context itself and leaves those out; off only when the context wants no zoom, no held frame and no volume change). Decide each pass by what it does, not by a sentence about another one. Quote the words of the context in "because".

Leave every other field alone: a default is a whole answer, and a value the context does not name is a value you would be inventing. Call finish when you are done.
```

<!-- nav -->
---
[← Prompt: narrate](narrate.md) · [↑ top](#prompt-policy-new) · [↑ Contents](../README.md) · [Prompt: retake →](retake.md)
<!-- /nav -->
