package main

// Answers kept, so the same question is not paid for twice.
//
// Describing an hour of footage is the longest thing this app does: one vision
// call per few frames, seven seconds each, hundreds of them. The step already
// resumes -- a chunk in the event log is not described again -- but a ⏹ that
// lands in the middle arms a fresh start (undFreshStart), and a fresh start on
// unchanged footage used to mean paying the whole twenty minutes again for
// answers that could only come back the same.
//
// So the answer is filed under the question. The key is the WHOLE request: the
// system prompt, the user text -- the rolling state, the speech, the context --
// and the frames themselves, which ride inside it as data URLs. Nothing that
// decides the answer is outside the key, which is the only way a cache like
// this can be safe: change the wording, the interval, the scale or a word of
// the context and every affected chunk is asked again, because the question is
// not the same question.
//
// It lives under the project's cache/ beside the waveforms, which is the folder
// for things that can be thrown away and made again. Deleting it costs time and
// nothing else.

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
)

// llmCacheDir is where a step's answers are kept.
func (a *App) llmCacheDir(step string) string {
	return filepath.Join(a.outDir, "cache", "llm", step)
}

// askKey digests a request into the name of the file its answer belongs in.
// Everything the model is sent goes in, in order; JSON is the spelling, so the
// key is exactly as stable as the request the server would receive.
func askKey(parts ...any) string {
	h := sha256.New()
	for _, p := range parts {
		b, err := json.Marshal(p)
		if err != nil {
			// unmarshalable means unsendable: a key that cannot be computed
			// must not collide with one that can
			return ""
		}
		h.Write(b)
		h.Write([]byte{0})
	}
	return hex.EncodeToString(h.Sum(nil))
}

// cachedReply is the answer to this exact request, if it has been asked before.
func (a *App) cachedReply(step, key string) (string, bool) {
	if key == "" || a.outDir == "" {
		return "", false
	}
	b, err := os.ReadFile(filepath.Join(a.llmCacheDir(step), key))
	if err != nil || len(b) == 0 {
		return "", false
	}
	return string(b), true
}

// keepReply files one. Best-effort: a cache that cannot be written is a step
// that runs at its ordinary speed, which is what it did before there was one.
func (a *App) keepReply(step, key, reply string) {
	if key == "" || a.outDir == "" || reply == "" {
		return
	}
	dir := a.llmCacheDir(step)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return
	}
	os.WriteFile(filepath.Join(dir, key), []byte(reply), 0o644)
}
