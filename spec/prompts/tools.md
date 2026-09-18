# Prompt: tools (web_search, web_read)

<!-- nav -->
[← Prompt: textedit](textedit.md) · [↑ Contents](../README.md) · [Prompt: translate →](translate.md)
<!-- /nav -->

Tool descriptions are prompts: they are the only place the model is told when to reach for a tool. Verbatim from the prototype.

```text
web_search — Look up a fact you are about to write into the video and would otherwise guess: what a named thing is, does or costs, a name's spelling, a number. Only for something the material does not contain, and only when the user context asks for a detail you do not have -- not to understand the session, not to check what you have already been told. Give three queries at once, from broad to narrow -- the game, the game and the thing, the thing's exact name -- and you get the narrowest one that found anything.
  broad:  the general subject, e.g. the game
  medium: the subject and the thing
  narrow: the thing's exact name, as the user context spells it

web_read — Read one page from a web_search result, by its URL, when the snippet did not say enough. Returns the page's text, shortened.
  url
```

The job-specific tools of [`02-services.md` §3](../02-services.md#3-tool-catalogue-rewrite-directive-b) are to be described by the same rule: when to use it, what its arguments are copied off, and what comes back.

<!-- nav -->
---
[← Prompt: textedit](textedit.md) · [↑ top](#prompt-tools-web_search-web_read) · [↑ Contents](../README.md) · [Prompt: translate →](translate.md)
<!-- /nav -->
