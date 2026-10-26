# Prompt: textedit

<!-- nav -->
[← Prompt: system](system.md) · [↑ Contents](../README.md) · [Prompt: tools (web_search, web_read) →](tools.md)
<!-- /nav -->

Shipped default. The prototype's wording asked for the joined text back ({"joined":"…"}); the rewrite asks for the words to remove, quoted, and shows the join three ways. Measured on 07-news's 18 joins against what should go (2026-10-26, qwen3.8-flash-next, thinking on): the prototype's shape answered almost nothing usable (125 of 127 asks empty, with tools offered beside it); this shape answered every join.

```text
You are repairing the join where a recording stopped and the next take started.

The speaker stumbled, stopped recording, and said it again -- sometimes starting a sentence or two further back. So the words just before [CUT] and the words just after it can be the same thing said twice, or a sentence abandoned half way. What the video plays has to run on as one piece of speech.

You are shown the join three ways: a WIDE VIEW of the speech around it, where this join is marked [CUT] and every other join is marked [another take starts] (those are not yours); the words CLOSE TO THE CUT; and BEFORE (the end of the take that was interrupted) and AFTER (the whole take that follows, up to where the next one starts).

HOW TO FIND THE CUT. Read the first words of AFTER, then look back from [CUT] for where those same words -- or nearly the same: a plural, a number spelled differently, a word swapped -- were already said in BEFORE. Remove from exactly that point in BEFORE up to [CUT], and nothing earlier: the words before that point were said once and stay. If AFTER's first words were never said in BEFORE, look for a sentence BEFORE left unfinished right at [CUT] and remove only that unfinished tail. A take can be abandoned entirely; then all of it goes, back to where it started.

BEFORE gives way first: what was said last is what the speaker meant to keep. Remove words from the start of AFTER only where the join still does not read without it, and then as few as possible.

ANSWER with the words you remove, copied exactly as you were given them, strict JSON and nothing else:

  {"drop_before":"<the words removed from the end of BEFORE, or empty>","drop_after":"<the words removed from the start of AFTER, or empty>"}

"drop_before" must be the LAST words of BEFORE and "drop_after" the FIRST words of AFTER, word for word: they are checked against what you were shown, and a quote that does not match is refused.

Removing nothing is a whole answer, and a common one: {"drop_before":"","drop_after":""}. A speaker who stopped to change a slide did not stumble.

EXAMPLE 1. BEFORE ends "...the protocol has three rounds of voting, and each round" [CUT] AFTER starts "Each round takes about two seconds...". AFTER restarts at "each round", so remove "and each round" and keep "the protocol has three rounds of voting,":
  {"drop_before":"and each round","drop_after":""}
It now reads "the protocol has three rounds of voting, Each round takes about two seconds".

EXAMPLE 2. BEFORE ends "...and that was the last point on this slide." [CUT] AFTER starts "Now to the second paper...". Nothing is said twice and nothing is left unfinished:
  {"drop_before":"","drop_after":""}

THE TEST IS THE READING. Read the join aloud as it is after your removal: the sentence has to finish, and nothing may be said twice.

WHAT WAS SAID, NOT THE SCRIPT. The USER CONTEXT may hold the script the speaker read from. It is what they meant to say, not what they said: a sentence said differently from the script, or said in place of it, is speech like any other and stays. Use the script to see where a sentence was going; never remove something because the script words it otherwise.

PUNCTUATION IS A HINT. The words carry the transcript's punctuation where it has any, and a sentence end is not always marked -- "it should And the next" is two sentences. Read for the sense, not the commas.
```

<!-- nav -->
---
[← Prompt: system](system.md) · [↑ top](#prompt-textedit) · [↑ Contents](../README.md) · [Prompt: tools (web_search, web_read) →](tools.md)
<!-- /nav -->
