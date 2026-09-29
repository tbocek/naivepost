//! §01-project-and-files#3-cutcutjson — the cut file's record and, above everything,
//! the exact shape it writes: which keys a plain scene leaves out, and the four lane
//! keys that never do.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use naivepost::cut::{self, cut_segments, Cut, EffectKind, Fx, Lane, Seg, NO_CUT_YET};
use naivepost::layout::Tree;

#[allow(dead_code)] // every test binary compiles this whole module
mod common;
use common::{seg};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-cut-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A project folder inside a throwaway root, holding the given file text (or no file).
fn with_file(tag: &str, text: Option<&str>) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    if let Some(text) = text {
        std::fs::write(dir.join("cut/cut.json"), text).unwrap();
    }
    (root, Tree::new(&dir).unwrap())
}

fn read(t: &Tree) -> String {
    std::fs::read_to_string(t.cut_json()).unwrap()
}

/// §3's example, verbatim.
const SPEC_CUT: &str = r#"{
  "segs": [{"s": 1.5, "e": 34.7, "cam": 0, "quiet": ["mic"]},
           {"s": 40.0, "e": 40.0, "ins": "assets/tier.svg?S=Dust II", "dur": 4.0, "mute": true},
           {"s": 60.0, "e": 65.0, "ins": "project:assets/sting.wav", "ss": 2.0, "lane": "mic"}],
  "aspect": "9:16",
  "fx": [{"kind": "zoom", "t": 12.0, "dur": 3.0, "trans": 1, "tout": 1, "cx": 0.5, "cy": 0.5, "hf": 0.6}],
  "shift": {"cam2": -1.25},
  "rows": {"cam2": 1},
  "lanes": [{"name": "cam-2", "src": "project:sources/cam.mkv", "at": 100.0, "off": 30.0, "dur": 20.0}],
  "nrows": 2,
  "folds": [[34.7, 40.0]]
}"#;

#[test]
fn sec_01_project_and_files_3_cutcutjson_the_specs_example_round_trips() {
    let (_root, t) = with_file("spec-example", Some(SPEC_CUT));
    let c = cut::load(&t).expect("the spec's own file loads");

    assert_eq!(c.segs.len(), 3);
    // A footage scene on row 0 that does not hear "mic".
    assert_eq!((c.segs[0].s, c.segs[0].e), (1.5, 34.7));
    assert_eq!(c.segs[0].cam, 0);
    assert_eq!(c.segs[0].quiet, ["mic".to_string()]);
    // s == e with a length: the spliced card.
    assert!(c.segs[1].is_insert());
    assert_eq!(c.segs[1].dur, 4.0);
    assert!(c.segs[1].mute);
    // An insert over footage: it carries no length of its own.
    assert!(!c.segs[2].is_insert());
    assert!(c.segs[2].is_overwrite_insert());
    assert_eq!(c.segs[2].ss, 2.0);
    assert_eq!(c.segs[2].lane(), Some("mic"));

    assert_eq!(c.aspect, "9:16");
    assert_eq!(c.fx.len(), 1);
    let zoom = &c.fx[0];
    assert_eq!((zoom.kind.as_str(), zoom.t, zoom.dur), ("zoom", 12.0, 3.0));
    assert_eq!((zoom.trans, zoom.tout), (1.0, 1.0));
    assert_eq!(zoom.cx, Some(0.5));
    assert_eq!(zoom.cy, Some(0.5));
    assert_eq!(zoom.hf, Some(0.6));

    assert_eq!(c.shift.get("cam2"), Some(&-1.25));
    assert_eq!(c.rows.get("cam2"), Some(&1));
    assert_eq!(c.lanes.len(), 1);
    let lane = &c.lanes[0];
    assert_eq!(lane.name, "cam-2");
    assert_eq!(lane.src, "project:sources/cam.mkv");
    assert_eq!((lane.at, lane.off, lane.dur), (100.0, 30.0, 20.0));
    assert_eq!(c.nrows, 2);
    assert_eq!(c.folds, [[34.7, 40.0]]);

    // One writer, and what it writes reads back the same.
    cut::save(&c, &t).unwrap();
    assert_eq!(cut::load(&t).unwrap(), c);
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_lanes_always_write_name_src_at_dur_and_drop_only_zero_off() {
    let c = Cut {
        segs: vec![seg(0.0, 5.0)],
        lanes: vec![Lane {
            name: "mic".to_string(),
            src: "project:sources/mic.wav".to_string(),
            // A lane that starts at nought and runs for nought still says so.
            at: 0.0,
            off: 0.0,
            dur: 0.0,
        }],
        ..Default::default()
    };
    let (_root, t) = with_file("lane-shape", None);
    cut::save(&c, &t).unwrap();
    let text = read(&t);

    for key in ["\"name\"", "\"src\"", "\"at\": 0.0", "\"dur\": 0.0"] {
        assert!(text.contains(key), "{key} missing from\n{text}");
    }
    assert!(!text.contains("\"off\""), "a zero off must not be written\n{text}");

    // Only `off` is the one that may be dropped.
    let mut moved = c.clone();
    moved.lanes[0].off = 30.0;
    cut::save(&moved, &t).unwrap();
    assert!(read(&t).contains("\"off\": 30.0"), "{}", read(&t));
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_an_ordinary_segment_writes_only_what_it_sets() {
    let (_root, t) = with_file("plain-seg", None);

    cut::save(&Cut::default(), &t).unwrap();
    assert!(read(&t).contains("\"segs\": []"), "{}", read(&t));

    let c = Cut { segs: vec![seg(1.5, 34.7)], ..Default::default() };
    cut::save(&c, &t).unwrap();
    let text = read(&t);
    assert!(text.contains("\"s\": 1.5") && text.contains("\"e\": 34.7"), "{text}");
    for key in ["\"ins\"", "\"dur\"", "\"rate\"", "\"ss\"", "\"mute\"", "\"cam\"", "\"lane\"", "\"quiet\"", "\"split\""] {
        assert!(!text.contains(key), "{key} written by a plain scene\n{text}");
    }
    // And the keys an empty cut says nothing about.
    for key in ["\"aspect\"", "\"fx\"", "\"shift\"", "\"rows\"", "\"lanes\"", "\"nrows\"", "\"folds\""] {
        assert!(!text.contains(key), "{key} written by an empty cut\n{text}");
    }
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_a_spliced_insert_is_s_eq_e_with_a_length_and_an_overwrite_is_not() {
    let spliced = Seg { ins: "assets/tier.svg".to_string(), dur: 4.0, ..seg(40.0, 40.0) };
    assert!(spliced.is_insert());
    assert!(!spliced.is_overwrite_insert());

    // s == e with no length is an empty scene, not a card.
    let empty = seg(40.0, 40.0);
    assert!(!empty.is_insert());

    let over = Seg { ins: "project:sources/sting.wav".to_string(), ..seg(60.0, 65.0) };
    assert!(!over.is_insert());
    assert!(over.is_overwrite_insert());

    // Footage is neither.
    assert!(!seg(1.5, 34.7).is_overwrite_insert());
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_ins_follows_the_one_path_rule() {
    let root = Path::new("/data/naivepost");
    let project = Path::new("/data/naivepost/lecture.naivepost");

    let inside = Seg { ins: "project:sources/sting.wav".to_string(), ..seg(0.0, 1.0) };
    assert_eq!(
        inside.insert_asset(root, project),
        Some(project.join("sources/sting.wav"))
    );

    // A card under <root>/assets, with its declared inputs in the query — stripped
    // from the path, kept as the pair it is.
    let card = Seg { ins: "assets/tier.svg?S=Dust II".to_string(), dur: 4.0, ..seg(0.0, 0.0) };
    assert_eq!(card.insert_asset(root, project), Some(root.join("assets/tier.svg")));
    assert_eq!(
        card.insert_query(),
        Some(vec![("S".to_string(), "Dust II".to_string())])
    );

    let absolute = Seg { ins: "/abs/x.wav".to_string(), ..seg(0.0, 1.0) };
    assert_eq!(absolute.insert_asset(root, project), Some(PathBuf::from("/abs/x.wav")));

    // A pasted stretch of the session is seconds, not a file.
    let pasted = Seg { ins: "copy:12.5".to_string(), ..seg(0.0, 1.0) };
    assert_eq!(pasted.insert_asset(root, project), None);
    assert_eq!(pasted.copy_seconds(), Some(12.5));

    // Footage has neither.
    let footage = seg(0.0, 1.0);
    assert_eq!(footage.insert_asset(root, project), None);
    assert_eq!(footage.copy_seconds(), None);
    assert_eq!(footage.insert_query(), None);
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_quiet_decides_what_a_scene_hears_and_lane_says_what_it_replaces() {
    let scene = Seg { quiet: vec!["mic".to_string()], ..seg(0.0, 5.0) };
    assert!(!scene.hears("mic"));
    assert!(scene.hears("cam-2"));

    // An overlaid sound replaces a lane; it does not silence it.
    let overlay = Seg {
        ins: "project:sources/sting.wav".to_string(),
        lane: "mic".to_string(),
        ..seg(0.0, 5.0)
    };
    assert!(overlay.hears("mic"));
    assert_eq!(overlay.lane(), Some("mic"));

    let everything = seg(0.0, 5.0);
    assert!(everything.hears("mic") && everything.hears("anything"));
    // Absent and "" both mean "everything audible", and stay distinguishable.
    assert_eq!(everything.lane(), None);
    let spelled = Seg { lane: String::new(), ..seg(0.0, 5.0) };
    assert_eq!(spelled.lane(), None);
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_folds_are_saved_and_are_view_only() {
    let mut c = Cut { segs: vec![seg(1.5, 34.7), seg(40.0, 65.0)], ..Default::default() };
    let (_root, t) = with_file("folds", None);

    cut::save(&c, &t).unwrap();
    assert!(!read(&t).contains("folds"), "{}", read(&t));

    c.folds.push([34.7, 40.0]);
    cut::save(&c, &t).unwrap();
    let text = read(&t);
    assert!(text.contains("34.7") && text.contains("40.0"), "{text}");
    assert_eq!(cut::load(&t).unwrap(), c);

    // Folding changes what the page shows and nothing else: the same segments with the
    // same seconds, which is as far as "ignored by the render" reaches in this file.
    let mut unfolded = c.clone();
    unfolded.folds.clear();
    assert_eq!(unfolded.segs, c.segs);
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_nrows_is_a_floor_under_the_row_count() {
    let c = Cut { nrows: 2, ..Default::default() };
    assert_eq!(c.row_count(1), 2);
    // A floor, not a ceiling: more recordings than the floor means more rows.
    assert_eq!(c.row_count(3), 3);
    assert_eq!(Cut::default().row_count(2), 2);
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_one_function_writes_the_file_and_a_missing_one_is_no_cut() {
    let (_root, t) = with_file("io", None);
    // No file yet: nothing was cut, which is not an error.
    assert_eq!(cut::load(&t).unwrap(), Cut::default());

    let c = Cut { segs: vec![seg(1.5, 34.7)], ..Default::default() };
    cut::save(&c, &t).unwrap();
    assert_eq!(cut::load(&t).unwrap(), c);
    // §1: files are 0644.
    let mode = std::fs::metadata(t.cut_json()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o644);

    // A file that is not JSON is an error naming the path, never a silent empty cut.
    std::fs::write(t.cut_json(), "{ not json").unwrap();
    let err = cut::load(&t).unwrap_err();
    assert!(err.contains("cut.json"), "{err}");
}

/// The maps are BTreeMap so a save is byte-stable; pinned here because the autosave
/// rule of §2 compares bytes and a shuffled key order would look like a change.
#[test]
fn sec_01_project_and_files_3_cutcutjson_shift_and_rows_save_in_key_order() {
    let mut c = Cut { segs: vec![seg(0.0, 1.0)], ..Default::default() };
    for (name, value) in [("cam3", -3.0), ("cam2", -1.25), ("cam1", 0.5)] {
        c.shift.insert(name.to_string(), value);
        c.rows.insert(name.to_string(), 1);
    }
    let (_root, t) = with_file("order", None);
    cut::save(&c, &t).unwrap();
    let text = read(&t);
    let at = |key: &str| text.find(key).unwrap_or(usize::MAX);
    assert!(at("\"cam1\"") < at("\"cam2\""), "{text}");
    assert!(at("\"cam2\"") < at("\"cam3\""), "{text}");

    // Byte-stable: the same cut saved twice writes the same bytes, which is what lets
    // §2's autosave skip a write that changed nothing.
    let first = text.clone();
    cut::save(&c, &t).unwrap();
    assert_eq!(read(&t), first);
}

/// An effect with only the two keys every kind needs.
fn fx(kind: &str, t: f64) -> Fx {
    Fx { kind: kind.to_string(), t, ..Default::default() }
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_an_effect_record_carries_its_kinds_fields() {
    let zoom = Fx {
        dur: 3.0,
        trans: 1.0,
        tout: 1.0,
        cx: Some(0.5),
        cy: Some(0.5),
        hf: Some(0.6),
        ..fx("zoom", 12.0)
    };
    assert_eq!(zoom.effect_kind(), Some(EffectKind::Zoom));
    // The bar is t ..= t + dur, and dur already counts the fades (06 §1).
    assert_eq!(zoom.spans(), (12.0, 15.0));

    let stop = Fx { rate: 0.0, snd: "mute".to_string(), ..fx("stop", 30.0) };
    assert_eq!(stop.effect_kind(), Some(EffectKind::Speed));
    let speed = Fx { rate: 2.0, snd: "pitch".to_string(), ..fx("speed", 30.0) };
    assert_eq!((speed.rate, speed.snd.as_str()), (2.0, "pitch"));

    let text = Fx {
        text: "the words".to_string(),
        cx: Some(0.5),
        cy: Some(0.78),
        wf: Some(0.8),
        hf: Some(0.16),
        ..fx("text", 4.0)
    };
    assert!(text.has_box());
    let svg = Fx { src: "assets/mark.svg".to_string(), ..fx("svg", 6.0) };
    assert_eq!(svg.src, "assets/mark.svg");
    let volume = Fx { gain: 0.5, lane: "mic".to_string(), ..fx("volume", 8.0) };
    assert_eq!((volume.gain, volume.lane()), (0.5, Some("mic")));
    // A label's `text` is the name, not words to draw — the kind says which.
    let label = Fx { text: "Q&A".to_string(), ..fx("label", 9.0) };
    assert_eq!(label.effect_kind(), Some(EffectKind::Label));

    // One record for all six kinds, so every one of them round-trips through the file.
    let c = Cut {
        segs: vec![seg(0.0, 1.0)],
        fx: vec![zoom, stop, speed, text, svg, volume, label],
        ..Default::default()
    };
    let (_root, t) = with_file("fx-kinds", None);
    cut::save(&c, &t).unwrap();
    assert_eq!(cut::load(&t).unwrap(), c);

    // A kind this build does not know costs the cut nothing but that effect's typed view.
    let odd = fx("hologram", 1.0);
    assert_eq!(odd.effect_kind(), None);
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_ease_stores_empty_for_linear_and_is_never_filled_in() {
    // "" IS linear, and stays "" — §1 says so explicitly, to keep old files
    // byte-identical. So an empty ease is omitted on save (the key absent reads back as
    // the same linear), and a name this build does not know survives untouched rather
    // than being corrected to one it knows.
    let linear = fx("text", 1.0);
    assert_eq!(linear.ease, "");
    let (_root, t) = with_file("ease-linear", None);
    cut::save(&Cut { segs: vec![seg(0.0, 1.0)], fx: vec![linear.clone()], ..Default::default() }, &t)
        .unwrap();
    assert!(!read(&t).contains("\"ease\""), "{}", read(&t));
    assert_eq!(cut::load(&t).unwrap().fx[0], linear);

    let named = Fx { ease: "glide".to_string(), ..linear };
    cut::save(&Cut { segs: vec![seg(0.0, 1.0)], fx: vec![named.clone()], ..Default::default() }, &t)
        .unwrap();
    assert!(read(&t).contains("\"ease\": \"glide\""), "{}", read(&t));
    assert_eq!(cut::load(&t).unwrap().fx[0], named);
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_a_missing_box_falls_back_to_its_kinds_default() {
    // A caption with no box of its own — written by an older build, or proposed by the
    // model — goes in the lower third (06 §1 and §F3.7).
    let caption = fx("text", 1.0);
    assert!(!caption.has_box());
    assert_eq!(caption.centre(), (0.5, 0.78));

    // A stored box wins, and only the parts stored are overridden.
    let placed = Fx { cx: Some(0.2), ..caption.clone() };
    assert!(placed.has_box());
    assert_eq!(placed.centre(), (0.2, 0.78));

    // A drawing is put mid-frame (§1: "svg: middle"). Its band's SIZE is not this file's
    // to decide — 06 §F3.8 owns it — so nothing here invents a width or height.
    assert_eq!(fx("svg", 1.0).centre(), (0.5, 0.5));

    // A zoom has no output-frame default: its cx/cy are fractions of the SOURCE frame,
    // a different frame entirely. It reports mid-frame and says it stored nothing.
    let zoom = fx("zoom", 1.0);
    assert_eq!(zoom.centre(), (0.5, 0.5));
    assert!(!zoom.has_box());
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_legacy_view_becomes_a_zoom_and_legacy_mute_becomes_snd_mute() {
    // A "view" was a region the camera kept — the same record under a name this build
    // calls zoom. Whatever preset name it carried is gone; the region lives in the box.
    let (_root, t) = with_file(
        "legacy-fx",
        Some(r#"{"segs":[{"s":0,"e":1}],"fx":[
            {"kind":"view","t":30,"trans":2,"dur":5,"tout":1,"cx":0.3,"cy":0.5,"hf":0.6},
            {"kind":"speed","t":40,"dur":2,"rate":0,"mute":true}]}"#),
    );
    let c = cut::load(&t).unwrap();

    assert_eq!(c.fx[0].kind, "zoom");
    assert_eq!(c.fx[0].effect_kind(), Some(EffectKind::Zoom));
    assert_eq!((c.fx[0].cx, c.fx[0].hf), (Some(0.3), Some(0.6)));

    assert_eq!(c.fx[1].snd, "mute");

    // Read and migrated once: neither spelling is ever written again. `snd: "mute"`
    // legitimately contains the letters, so match the key rather than the word.
    cut::save(&c, &t).unwrap();
    let text = read(&t);
    assert!(!text.contains("\"view\""), "{text}");
    assert!(!text.contains("\"mute\":"), "{text}");
    assert!(text.contains("\"snd\": \"mute\""), "{text}");
    // And the file still reads back as the migrated cut.
    assert_eq!(cut::load(&t).unwrap(), c);
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_legacy_whole_cut_sound_becomes_per_scene_quiet() {
    let recordings: Vec<String> = ["mic", "cam-2"].iter().map(|s| s.to_string()).collect();
    let (_root, t) = with_file(
        "legacy-sound",
        Some(r#"{"segs":[{"s":0,"e":10},{"s":10,"e":20,"quiet":["cam-2"]}],"sound":"mic"}"#),
    );
    let mut c = cut::load(&t).unwrap();

    let note = c.migrate_sound(&recordings).expect("a sound to migrate");
    // The silent scene is left alone; the other silences everything but mic.
    assert_eq!(c.segs[0].quiet, ["cam-2".to_string()]);
    assert_eq!(c.segs[1].quiet, ["cam-2".to_string()]);
    assert_eq!(
        note,
        "this cut was heard on mic from end to end; that is now said scene by scene, \
         and every scene has been set to silence the other lanes"
    );

    // The key is gone for good.
    cut::save(&c, &t).unwrap();
    assert!(!read(&t).contains("\"sound\""), "{}", read(&t));
    // Nothing left to do a second time.
    assert_eq!(c.migrate_sound(&recordings), None);

    // A sound naming no known recording: nothing was heard, so every lane goes quiet.
    let mut deaf = Cut {
        segs: vec![seg(0.0, 1.0)],
        sound: Some("ghost".to_string()),
        ..Default::default()
    };
    assert!(deaf.migrate_sound(&recordings).is_some());
    assert_eq!(deaf.segs[0].quiet, recordings);
    // And the key does not survive a save.
    let (_root, ghost) = with_file("legacy-sound-ghost", None);
    cut::save(&deaf, &ghost).unwrap();
    assert!(!read(&ghost).contains("\"sound\""), "{}", read(&ghost));

    // No sound at all is not a migration and gets no note.
    let (_root, none) = with_file("legacy-sound-none", Some(r#"{"segs":[{"s":0,"e":1}]}"#));
    let mut silent = cut::load(&none).unwrap();
    assert_eq!(silent.migrate_sound(&recordings), None);
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_existing_segments_unlock_narrate_and_produce() {
    let file = Cut { segs: vec![seg(0.0, 5.0)], ..Default::default() };
    let editor = [seg(1.0, 2.0)];

    // The live editor's if it has any — even when the file holds a different cut.
    assert_eq!(cut_segments(Some(&editor), &file).unwrap(), &editor[..]);
    // An editor holding none is not a cut: fall back to the file's.
    assert_eq!(cut_segments(Some(&[]), &file).unwrap(), &file.segs[..]);
    // An unsaved editor with segments does unlock ▶, whatever the file says.
    let empty = Cut::default();
    assert_eq!(cut_segments(Some(&editor), &empty).unwrap(), &editor[..]);

    // And when neither has any, the caller's refusal is §3's sentence exactly.
    assert!(cut_segments(None, &empty).is_none());
    assert_eq!(NO_CUT_YET, "no cut yet — build one on the Cut step first");
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_a_file_with_empty_segs_unlocks_nothing() {
    // §3's own case: `{"segs":[]}` is a file that says nothing was cut.
    let (_root, t) = with_file("empty-segs", Some(r#"{"segs":[]}"#));
    let c = cut::load(&t).unwrap();
    assert!(c.segs.is_empty());
    assert_eq!(cut_segments(None, &c), None);
    // And the shape it saves in is that same file — "segs" always written.
    cut::save(&c, &t).unwrap();
    assert!(read(&t).contains("\"segs\": []"), "{}", read(&t));
}
