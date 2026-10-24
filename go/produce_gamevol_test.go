package main

// The game-audio slider and the difference between "0" and "not stored".

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

// TestASilencedGameComesBackSilent is the bug this file exists for. The slider
// runs from 0, so 0 is a pick a person can make -- game fully muted under the
// narration -- and it has to survive the save. It used to not: the page read a
// stored 0 as "this project has no game_vol" and put the slider back at 0.22,
// so the game came back at 22% every time the project was reopened.
func TestASilencedGameComesBackSilent(t *testing.T) {
	st := defaultProdSettings()
	st.GameVol = 0
	b, err := json.Marshal(st)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), `"game_vol":0`) {
		t.Fatalf("marshalled settings %s, want a stored zero", b)
	}
	var got prodSettings
	if err := json.Unmarshal(b, &got); err != nil {
		t.Fatal(err)
	}
	if got.GameVol != 0 {
		t.Errorf("a saved silence reloaded as %v, want 0", got.GameVol)
	}
}

// TestAProjectWithoutAGameVolKeepsTheDefault is the other half, and the reason
// the fix is a decoder rather than a deleted line: a project written before the
// setting existed carries no game_vol at all, and dropping the guard on its own
// would have loaded those as 0 -- silently muting the game in every old project.
// Decoded as a whole Project, because that pointer field is the real load path.
func TestAProjectWithoutAGameVolKeepsTheDefault(t *testing.T) {
	var p Project
	if err := json.Unmarshal([]byte(`{"produce":{"container":"mp4"}}`), &p); err != nil {
		t.Fatal(err)
	}
	if p.Produce == nil {
		t.Fatal("no produce block decoded")
	}
	if p.Produce.GameVol != 0.22 {
		t.Errorf("a project with no game_vol loaded %v, want the 0.22 default",
			p.Produce.GameVol)
	}
	// and a stored level still wins over that seed, through the same path
	var q Project
	if err := json.Unmarshal([]byte(`{"produce":{"game_vol":0.5}}`), &q); err != nil {
		t.Fatal(err)
	}
	if q.Produce.GameVol != 0.5 {
		t.Errorf("a stored 0.5 loaded as %v", q.Produce.GameVol)
	}
}

// TestTheGameVolSliderIsSetUnconditionally pins the page to that decision. The
// seeding only helps if applyProdSettings stops second-guessing it, so the line
// that writes the slider must carry no "> 0" test of its own.
func TestTheGameVolSliderIsSetUnconditionally(t *testing.T) {
	b, err := os.ReadFile("produce.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	if strings.Contains(src, "if st.GameVol > 0 {") {
		t.Error("produce.go still guards the slider with if st.GameVol > 0")
	}
	if !strings.Contains(src, "\tp.gvol.SetValue(st.GameVol)\n") {
		t.Error("produce.go does not set the slider from st.GameVol")
	}
}
