# Naivepost

How to read what Prepare is doing. The full description is in [`spec/Readme.md`](spec/Readme.md).

## What the status line means

While frames are extracted, the bottom of the
window can read `pass 2 of 2, 0:44 of 1:10, 17 judge(s)`. That is the **text
gate**: pass 2 of 2 of the frame step, 44 s
into a 1:10 recording, and 17 model calls so far. A judge is one such call: it
gets two screenshots a few seconds apart and answers whether the text of the main
content changed, ignoring the webcam overlay, the cursor, browser tabs and the
taskbar clock. Its answers choose the frames Describe sees. A recording with few
slide changes needs few judges. When a recording's gate ends, the log says how
it went: `text gate: N scene change(s) -- … ; M judge(s)`, and also how many judges
failed and the first error.

## Input and output of each step

The first four rows are real output from a
German lecture; the others show each file's shape with made-up content.

| Step | Input | Output |
|---|---|---|
| Audio | `2026-11-01 10-58-42.mkv` | `inputs/<source>/voice16k.wav`, 16 kHz mono |
| Speech to text | that wav | `transcript.tsv`: `1.41  11.41  SPEAKER_00  wikipedia funktioniert im prinzip genauso …` |
| Alignment | the wav and the heard words | `words.aligned.json`: `{"word":"wikipedia","start_sample":22522,"end_sample":31482}` (22522 / 16000 = 1.41 s) |
| Text gate | two screenshots | `judge_text({"changed": true, "what": "Terminal now shows \"nano\" typed at the prompt (previously empty)"})` |
| Describe | the chosen frames, the words heard, the summary so far | `EVENT [+0s]: same`, `EVENT [+1s]: Calm; a new slide on server-side rendering appears.`, `STATE: A lecture on web architecture, now at server-side rendering.` |
| Fix | `wikipedia funktioniert im prinzip genauso` | `Wikipedia funktioniert im Prinzip genauso.`, with the same times and speaker |
| Merge | the fixed lines and the events | `session.txt`: `0:01 Wikipedia funktioniert im Prinzip genauso.` |
| Mark what was said twice | the numbered lines | `retakes.tsv`: the stretches said twice, marked abandoned rather than deleted |
