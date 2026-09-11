package main

// The spelling the subtitles carry.
//
// The words a subtitle is built from come off the ALIGNER: it is handed the
// recogniser's transcript and answers where each word falls, to about 20 ms,
// which is the only source of times in the app accurate enough to caption with
// (wordCues). What it hands back is bare lowercase, so dressWords puts the
// case and punctuation back from the transcript it was given -- the raw one,
// straight off the speech model.
//
// And that is where the session context stopped. The transcript pass reads it
// ("use the script's spelling for names, companies and numbers I say aloud")
// and writes what it settled on into the session timeline; the subtitles never
// read that file, so they went on saying what the recogniser heard. One
// recogniser writes "rsa two hundred and sixty" where another writes "RSA-260"
// -- the same seconds of the same video, captioned two ways depending on which
// model was loaded that day, and the sentence in the user context asking for
// one of them had no effect on the subtitles at all.
//
// fixWords closes that: the words keep their times and take the transcript's
// spelling. It is a re-dressing and never a rewrite -- no word gains a time it
// did not have, and a stretch that cannot be matched keeps what the recogniser
// wrote rather than guessing.

import "strings"

// fixRunReach is how far apart the two spellings of one stretch may be before
// the walk gives up on it: "two hundred and sixty" against "RSA-260" is four
// words against one, and a window of six covers every rewrite of a number, a
// name or a company seen so far. Past it the walk has lost its place, and
// dressing the rest of the line from the wrong offset would put the right
// words on the wrong seconds.
const fixRunReach = 6

// fixWords re-dresses the aligned words with what the transcript pass wrote
// for the same seconds. rows are the merged timeline's (session.tsv): the
// fixed text, on the session clock, with the recording each line came off.
//
// Only .raw changes -- the word used for MATCHING (.w) stays the recogniser's,
// because every pass that reads the words reads that one, and only the
// subtitles read the other (wordCues).
func fixWords(words []srcWord, rows []tsvRow) {
	if len(words) == 0 || len(rows) == 0 {
		return
	}
	// a cursor per recording: the words and the rows are both in time order,
	// so each row picks up where the last one of its recording left off
	at := map[string]int{}
	idx := map[string][]int{}
	for i, w := range words {
		idx[w.src] = append(idx[w.src], i)
	}
	for _, r := range rows {
		if r.spk == "EVENT" || strings.TrimSpace(r.text) == "" {
			continue
		}
		in := idx[r.src]
		var mine []*srcWord
		k := at[r.src]
		for ; k < len(in); k++ {
			w := &words[in[k]]
			mid := (w.s + w.e) / 2
			if mid < r.s-0.01 {
				continue // said before this line: a word the row does not cover
			}
			if mid > r.e+0.01 {
				break
			}
			mine = append(mine, w)
		}
		at[r.src] = k
		redress(mine, strings.Fields(r.text))
	}
}

// redress walks one line's words against one line's fixed text and hands each
// word its spelling. Where the two spell a stretch differently, the whole of
// the fixed spelling goes on the FIRST word of the stretch and the rest are
// emptied: every word keeps its own seconds, so a cut through the middle of
// such a stretch loses the words rather than showing words the video does not
// play -- which is the one thing subtitles built from the aligner exist to
// prevent.
func redress(ws []*srcWord, fixed []string) {
	// A line with NOTHING in common is not a line that was respelled: it is a
	// line the walk should not be trusted with -- a row matched to the wrong
	// recording's words, a clock that moved under one of them. Left as it was
	// heard, it is one line captioned the way it always was; rewritten from a
	// text that shares no word with it, it is the wrong words on these
	// seconds. One word in common is enough to believe the pairing.
	if !anyShared(ws, fixed) {
		return
	}
	pending := "" // fixed words with no time of their own, waiting for one
	put := func(w *srcWord, s string) {
		w.raw = strings.TrimSpace(pending + s)
		pending = ""
	}
	i, j := 0, 0
	for i < len(ws) && j < len(fixed) {
		if bareWord(fixed[j]) == ws[i].w {
			put(ws[i], fixed[j])
			i, j = i+1, j+1
			continue
		}
		di, dj, ok := fixResync(ws, fixed, i, j)
		switch {
		case !ok:
			// nothing within reach spells the same: the walk takes them as
			// the same word anyway -- it is the same position in a line that
			// matched either side of here -- and the transcript's spelling
			// wins, which is the whole point. "RSA-1024" against a heard
			// "rsa" resyncs on neither and is the right word for that second.
			// The next word is matched normally, so a walk that really has
			// lost its place finds itself again rather than running on.
			put(ws[i], fixed[j])
			i, j = i+1, j+1
		case di == 0:
			// the transcript has words the recogniser never heard: they have
			// no seconds, so they ride on the next word that does
			pending += strings.Join(fixed[j:j+dj], " ") + " "
			j += dj
		default:
			put(ws[i], strings.Join(fixed[j:j+dj], " "))
			for k := i + 1; k < i+di; k++ {
				ws[k].raw = ""
			}
			i, j = i+di, j+dj
		}
	}
	// whatever the line ends with and the words do not: onto the last word
	// that has a time, for the same reason
	if j < len(fixed) && len(ws) > 0 {
		last := ws[min(i, len(ws))-1]
		last.raw = strings.TrimSpace(last.raw + " " + strings.Join(fixed[j:], " "))
	}
	// ...and the other way round -- words left over when the line has run out.
	// They are the tail of a stretch the transcript folded into fewer words
	// ("two hundred and fifty" into "RSA-250"), and the fold is already
	// printed on the first of them: kept, they would be said twice.
	for ; i < len(ws); i++ {
		ws[i].raw = ""
	}
}

// anyShared is whether the two spellings of a line have a single word in
// common, which is what makes the walk below a re-dressing rather than a
// guess.
func anyShared(ws []*srcWord, fixed []string) bool {
	have := make(map[string]bool, len(ws))
	for _, w := range ws {
		have[w.w] = true
	}
	for _, f := range fixed {
		if have[bareWord(f)] {
			return true
		}
	}
	return false
}

// fixResync is the shortest pair of runs that puts the walk back in step: how
// many words of the recogniser's and how many of the transcript's to skip
// before the two spell the same word again. Shortest by total length, so one
// word against four is found before two against five.
func fixResync(ws []*srcWord, fixed []string, i, j int) (int, int, bool) {
	for d := 1; d <= 2*fixRunReach; d++ {
		for di := 0; di <= d && di <= fixRunReach; di++ {
			dj := d - di
			if dj > fixRunReach || i+di >= len(ws) || j+dj >= len(fixed) {
				continue
			}
			if bareWord(fixed[j+dj]) == ws[i+di].w {
				return di, dj, true
			}
		}
	}
	return 0, 0, false
}
