package main

// What ran, in the log, before it ran.

import (
	"os/exec"
	"strings"
	"testing"
)

// Every subprocess this app starts is an ffmpeg, and what it was asked to do is
// the whole of what comes out: a flag in the wrong place is a video two frames
// out of sync in one player and fine in another (movflags, produce.go). The
// command therefore goes in the log BEFORE it runs -- not only when it fails,
// and not only in the source.
func TestEveryCommandIsLoggedBeforeItRuns(t *testing.T) {
	body := funcBody(t, "pipeline.go", `func \(a \*App\) runCmd\(`)
	log := strings.Index(body, "a.logCmd(name, args)")
	run := strings.Index(body, "cmd.Run()")
	if log < 0 || run < 0 || log > run {
		t.Errorf("runCmd does not log its command before running it (%d, %d)", log, run)
	}
	// the failure carries it too: an error is read where it lands, and hunting
	// back up the log for the line that goes with it is the part nobody does
	if !strings.Contains(body, "cmdLine(name, args)") {
		t.Error("a failed command does not say what was run")
	}
	// ...and the one that reports progress, which is the long ffmpeg of the lot
	prog := funcBody(t, "pipeline.go", `func \(a \*App\) ffmpegProgress\(`)
	if i, j := strings.Index(prog, "a.logCmd(ffTool"), strings.Index(prog, "cmd.Start()"); i < 0 || i > j {
		t.Error("the frame extraction runs unlogged")
	}
}

// The line is pasteable, which is the whole point of printing it: the paths in
// this app have spaces in them ("2026-09-10 15-04-39.mkv"), so a line printed
// with a bare %v looks runnable and is not.
func TestTheLoggedCommandCanBePastedIntoAShell(t *testing.T) {
	args := []string{"-v", "error", "-y",
		"-i", "/mnt/rec/2026-09-10 15-04-39.mkv",
		"-vf", "scale=-2:1080,fps=30",
		"-movflags", "+faststart+negative_cts_offsets",
		"-metadata:s:s:0", "title=In the news",
		"-i", "it's here.wav",
		"/tmp/out file.mp4"}
	line := cmdLine("ffmpeg", args)

	// let a real shell take it apart again: every argument has to come back
	// exactly as it went in
	out, err := exec.Command("sh", "-c", "set -- "+line+`; shift; for a in "$@"; do printf '%s\n' "$a"; done`).Output()
	if err != nil {
		t.Fatalf("the line is not a line a shell will take: %v\n%s", err, line)
	}
	got := strings.Split(strings.TrimRight(string(out), "\n"), "\n")
	if len(got) != len(args) {
		t.Fatalf("%d arguments came back out of %d:\n%s\n%q", len(got), len(args), line, got)
	}
	for i := range args {
		if got[i] != args[i] {
			t.Errorf("argument %d came back as %q, want %q", i, got[i], args[i])
		}
	}
	// and what needs no quoting keeps none, or every line is a wall of quotes
	if strings.Contains(line, `'-v'`) || strings.Contains(line, `'scale=-2:1080,fps=30'`) {
		t.Errorf("plain arguments are being quoted:\n%s", line)
	}
}
