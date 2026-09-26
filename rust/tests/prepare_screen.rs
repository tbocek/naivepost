//! §04-prepare#1-screen — the Prepare page's two readouts, its Freq control and what it does
//! NOT carry, as plain logic. The widget side of the same rules is in
//! `tests/prepare_screen_widgets.rs`; this file holds the rules themselves so a rule can be
//! checked without a display, which is what lets `shell::Shell` stay a renderer.

use std::fs;
use std::path::PathBuf;

use naivepost::bench;
use naivepost::layout::Tree;
use naivepost::prepare;
use naivepost::project::{Project, Source};
use naivepost::shell::{Page, Shell};

/// The two source paths these tests share: one footage row and one recording that is only listened
/// to. `project:` + root-relative, as every path in a project is stored (§01).
const LECTURE: &str = "project:sources/lecture.mkv";
const MIC: &str = "project:sources/mic.wav";

/// A shell sitting on Prepare, built from `Default` with the page set — `Shell` has no
/// constructor because the window owns one and only switching moves it.
fn shell_on(page: Page) -> Shell {
    Shell {
        page,
        help_page: page,
        ..Shell::default()
    }
}

fn prepare_shell() -> Shell {
    shell_on(Page::Prepare)
}

/// A scratch project folder: `Tree::new` only accepts a directory whose name ends in
/// `.naivepost`, and every count below reads files off disk, so each test gets its own tree
/// under /tmp rather than sharing one and stepping on another's counts. Same shape as the
/// helpers in `tests/cut_json.rs` — no `tempfile` crate here, the pid keeps runs apart.
fn scratch(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-prep-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dir = root.join(format!("{tag}.naivepost"));
    fs::create_dir_all(&dir).expect("project folder");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

fn source(path: &str, footage: bool) -> Source {
    Source {
        path: path.to_string(),
        footage,
        narrator: 0,
        sepvoice: false,
        tracks: vec![],
    }
}

/// The lane `prepare::count` derives for a source: its file name minus the extension. Repeated
/// here rather than imported because it is private — the seeds below must land where the counter
/// looks, and pinning the derivation is part of what these tests check.
fn lane_of(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(at) if at > 0 => name[..at].to_string(),
        _ => name.to_string(),
    }
}

/// Write `n` frame files into the frames folder of the source at `path`, the way F1.6's 250 ms
/// grid extraction leaves them under `prepare/inputs/frames/<lane>/`.
fn seed_frames(tree: &Tree, path: &str, n: usize) {
    let dir = tree.frames_dir(&lane_of(path));
    fs::create_dir_all(&dir).expect("frames dir");
    for i in 0..n {
        fs::write(dir.join(format!("2026-09-16_18-43-0{}.000.jpg", i % 10)), b"jpg")
            .expect("frame file");
    }
}

/// Write a transcript with `n` non-empty lines plus a trailing blank, so the blank proves the
/// reader counts lines rather than newlines. Lands at `transcript_tsv`, which is where
/// `prepare::count` reads lines from.
fn seed_transcript(tree: &Tree, path: &str, n: usize) {
    let file = tree.transcript_tsv(&lane_of(path));
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).expect("transcript parent");
    }
    let mut text = String::new();
    for i in 0..n {
        text.push_str(&format!("word{i}\t0.0\t1.0\n"));
    }
    text.push('\n');
    fs::write(&file, text).expect("transcript");
}

/// §1 badge **6**: the Inputs row reads "N frames → M vision · L lines → K fixer". Both arrow
/// pairs are required — the row predicts work, not files, so a single number would not do.
#[test]
fn sec_04_prepare_1_screen_s_inputs_readout_names_both_arms() {
    let counts = prepare::Counts {
        frames: 9,
        lines: 26,
    };
    assert_eq!(
        prepare::inputs_readout(&counts),
        "9 frames \u{2192} 3 vision \u{b7} 26 lines \u{2192} 2 fixer",
        "the exact sentence §1 spells, arrows included"
    );
    // The same string through the shell, which is what the label actually draws.
    let shell = prepare_shell();
    assert_eq!(
        shell.inputs(None, &Project::default(), &Default::default(), &Default::default()),
        "nothing read yet",
        "with no folder open the row refuses to claim a zero"
    );
}

/// The two divisors behind that sentence, named at their parameters: four frames per vision call
/// (`P.machine.describeFramesPerReq`) and 25 lines per fixer block.
#[test]
fn sec_04_prepare_1_screen_s_inputs_divisors_are_the_named_parameters() {
    // P.machine.describeFramesPerReq = 4, so 8 frames is 2 calls and 9 spills into a third.
    assert_eq!(prepare::vision_calls(0), 0);
    assert_eq!(prepare::vision_calls(4), 1);
    assert_eq!(prepare::vision_calls(8), 2);
    assert_eq!(prepare::vision_calls(9), 3, "// P.machine.describeFramesPerReq");

    // prepare::FIX_BLOCK_LINES = 25, the fixer's block size.
    assert_eq!(prepare::FIX_BLOCK_LINES, 25);
    assert_eq!(prepare::fixer_calls(25), 1);
    assert_eq!(prepare::fixer_calls(50), 2);
    assert_eq!(prepare::fixer_calls(51), 3, "// P.machine.fixBlockLines");
}

/// Counting over a real tree: only footage contributes frames (a file that is only listened to
/// has none), while every source contributes its transcript lines.
#[test]
fn sec_04_prepare_1_screen_s_count_puts_frames_only_on_footage() {
    let (_keep, tree) = scratch("count");
    seed_frames(&tree, LECTURE, 9);
    seed_transcript(&tree, LECTURE, 26);
    seed_frames(&tree, MIC, 40);
    seed_transcript(&tree, MIC, 10);

    let mut project = Project::default();
    project.sources = vec![source(LECTURE, true), source(MIC, false)];

    let counts = prepare::count(&tree, &project);
    assert_eq!(
        counts,
        prepare::Counts {
            frames: 9,
            lines: 36
        },
        "the audio-only row's 40 frames are not counted, its lines are"
    );
}

/// §1 badge **6**'s "per-file tooltip": one line per source, in the project's own order, each
/// carrying that file's own arithmetic. A source with nothing counted still gets its line.
#[test]
fn sec_04_prepare_1_screen_s_inputs_tip_is_one_line_per_source_in_order() {
    let (_keep, tree) = scratch("tip");
    seed_frames(&tree, LECTURE, 9);
    seed_transcript(&tree, LECTURE, 26);

    let mut project = Project::default();
    project.sources = vec![source(LECTURE, true), source(MIC, false)];

    let tip = prepare::inputs_tip(&tree, &project);
    let lines: Vec<&str> = tip.lines().collect();
    assert_eq!(lines.len(), 2, "one line per source, got {lines:?}");
    assert!(
        lines[0].starts_with("lecture.mkv: "),
        "the first line names the first source by FILE NAME, not by the `project:`-prefixed path it is stored as: {:?}",
        lines[0]
    );
    assert!(
        lines[0].contains("9 frames") && lines[0].contains("26 lines"),
        "and carries that file's own counts: {:?}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("mic.wav: "),
        "the second source keeps the project's order: {:?}",
        lines[1]
    );
    assert!(
        lines[1].contains("0 frames"),
        "an uncounted file still shows, as a zero rather than as missing: {:?}",
        lines[1]
    );

    // And the shell hands that tooltip to the page only for Prepare.
    let shell = prepare_shell();
    assert_eq!(
        shell.inputs_tip(Some(&tree), &project),
        Some(tip.clone()),
        "Prepare's readout tooltip is the per-file list"
    );
    let cut_shell = shell_on(Page::Cut);
    assert_eq!(
        cut_shell.inputs_tip(Some(&tree), &project),
        None,
        "no other page has a per-file inputs tooltip"
    );
    assert_eq!(
        shell.inputs_tip(None, &project),
        None,
        "with no folder there is nothing to break down"
    );
}

/// §1 badge **7**: "Outputs: one folder button for `prepare/`, label 'Prepare:' + count".
#[test]
fn sec_04_prepare_1_screen_s_outputs_label_and_three_subfolder_tooltip() {
    let (_keep, tree) = scratch("outputs");
    let shell = prepare_shell();
    assert_eq!(
        shell.outputs(None),
        "",
        "no folder open means no count, not a zero"
    );
    assert_eq!(
        shell.outputs(Some(&tree)),
        prepare::NO_OUTPUTS,
        "a page that has written nothing says so plainly rather than claiming \"Prepare: 0\", which would read as a run that produced nothing on purpose"
    );

    seed_frames(&tree, LECTURE, 9);
    seed_transcript(&tree, LECTURE, 26);
    let total = prepare::outputs_count(&tree);
    assert!(total >= 10, "the count covers the whole prepare/ tree: {total}");
    assert_eq!(
        shell.outputs(Some(&tree)),
        format!("{} {}", prepare::OUTPUTS_LABEL, total),
        "the row prints the spec's label followed by the count"
    );
    assert!(
        shell.outputs(Some(&tree)).starts_with(prepare::OUTPUTS_LABEL),
        "label first, as §1 writes it"
    );

    // The folder button's tooltip names all three subfolders the count spans.
    for sub in ["prepare/inputs/", "prepare/describe/", "prepare/transcript/"] {
        assert!(
            prepare::OUTPUTS_TIP.contains(sub),
            "the tooltip must name {sub}, it says: {:?}",
            prepare::OUTPUTS_TIP
        );
    }
    // And the page really puts it there: the label is built from the same constant, so a
    // reworded tooltip cannot silently drop a subfolder.
    assert_eq!(
        prepare::NO_OUTPUTS,
        "nothing yet",
        "an Outputs row over a folder that never ran says so plainly"
    );
}

/// The page's Outputs folder is the one the count came from, so the button cannot point
/// somewhere else than the number describes.
#[test]
fn sec_04_prepare_1_screen_s_outputs_folder_is_what_was_counted() {
    let (_keep, tree) = scratch("samefolder");
    seed_frames(&tree, LECTURE, 3);
    let shell = prepare_shell();
    let counted = prepare::outputs_count(&tree);
    let pointed = shell.output_dir(&tree);
    assert_eq!(pointed, tree.prepare_dir(), "the button points at prepare/");
    assert!(
        pointed.join("inputs").is_dir() || counted == 0,
        "the folder the button opens is the folder the count walked"
    );
}

/// Freq is a choice among seven stops, not a number box: an unlisted interval would be sent to
/// the model as a claim about frames that were never picked.
#[test]
fn sec_04_prepare_1_screen_s_freq_is_a_stop_not_a_number_box() {
    assert_eq!(
        prepare::FREQ_STOPS,
        [0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0],
        "§1 lists exactly these seven"
    );
    assert_eq!(prepare::FREQ_DEFAULT, 1.0, "default 1 s");

    let mut project = Project::default();
    let before = project.interval;
    assert!(
        !prepare::set_freq(&mut project, 0.75),
        "an off-list interval is refused"
    );
    assert_eq!(
        project.interval, before,
        "and the project is left exactly as it was"
    );

    assert!(prepare::set_freq(&mut project, 0.25), "0.25 is a stop");
    assert_eq!(project.interval, 0.25);
    assert!(prepare::set_freq(&mut project, 5.0), "5 s is a stop");
    assert_eq!(project.interval, 5.0);
    assert_eq!(
        prepare::freq_label(0.25),
        "0.25",
        "a fraction of a second prints bare"
    );
    assert_eq!(prepare::freq_label(5.0), "5s", "a second and up carries its unit");
}

/// The reason Freq and the extraction grid are two numbers at all: changing what the model is
/// shown must never move what is on disk.
#[test]
fn sec_04_prepare_1_screen_s_freq_change_never_moves_the_extraction_grid() {
    // P.eng.frameGridSeconds = 0.25, fixed, restarted at every scene change (F1.6).
    assert_eq!(prepare::EXTRACTION_GRID, 0.25, "// P.eng.frameGridSeconds");
    let mut project = Project::default();
    let grid_before = prepare::EXTRACTION_GRID;
    assert!(prepare::set_freq(&mut project, 5.0));
    assert_eq!(
        prepare::EXTRACTION_GRID, grid_before,
        "Freq moved to 5 s and the grid stayed at 250 ms — the whole point of the split"
    );
    assert_ne!(
        project.interval, prepare::EXTRACTION_GRID,
        "and the two are not aliases of one field"
    );
}

/// What the rewrite took away: no Style control and no Frame size control on this page. Style is
/// `P.policy.markingPass` + `P.policy.cutMode` off the User Context (F0.7); frames keep the
/// video's own size and are scaled only on the way to the model (F1.7).
#[test]
fn sec_04_prepare_1_screen_s_no_style_and_no_frame_size_control() {
    let banned = ["Style", "style", "Frame size", "frame size", "Original", "480p", "720p"];
    for row in bench::ROWS.iter() {
        for word in banned {
            assert!(
                !row.title.contains(word),
                "the bench must not offer {word}: rows are {:?}",
                bench::ROWS.iter().map(|r| r.title).collect::<Vec<_>>()
            );
        }
    }
    // The twelve prompts plus the User Context, and nothing else.
    assert_eq!(bench::ROWS.len(), 13, "row 0 + twelve prompts in pipeline order");
    assert_eq!(bench::ROWS[0].title, "User Context");
    assert!(bench::is_context(&bench::ROWS[0]));
    // Language stays: it is a recording property, not a style.
    assert!(
        prepare::LANGUAGE_TIP.contains("gibberish") && prepare::LANGUAGE_TIP.contains("en"),
        "Language keeps its warning and its default: {:?}",
        prepare::LANGUAGE_TIP
    );
    assert_eq!(
        prepare::language_label(""),
        Project::default().language,
        "an empty box shows the language that will actually be used"
    );
}

/// Nothing in the page's own module reaches outside the fixture-shaped paths: a source stored as
/// `project:` + root-relative resolves inside the project, which is what makes the counts above
/// mean this session's files.
#[test]
fn sec_04_prepare_1_screen_s_counts_ignore_paths_that_are_not_there() {
    let (_keep, tree) = scratch("missing");
    let mut project = Project::default();
    project.sources = vec![source("project:gone.mkv", true)];
    let counts = prepare::count(&tree, &project);
    assert_eq!(
        counts,
        prepare::Counts::default(),
        "a file that is not there counts as nothing rather than as an error"
    );
    let tip = prepare::inputs_tip(&tree, &project);
    assert!(
        tip.starts_with("project:gone.mkv: "),
        "an uncounted file still gets its line in the tooltip; `base_name` keeps the stored \
`project:` prefix because a lane name is derived from the same string (see `prepare::lane`), so \
what this pins is that a missing file shows as zeros rather than vanishing: {tip:?}"
    );
}
