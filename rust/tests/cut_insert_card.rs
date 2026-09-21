// F2.12 (Insert a card, still, video or sound) — spec/05-cut.md F2.12, spec/inventory/cut.md §G.
// One test per step: where Insert may be placed (S1), which chooser opens and what kind of file came back (S2),
// what the gesture answers before anything is asked (S3), what the form holds (S4), what each mode does to the
// cut and says about it (S5), how a held card becomes an Edit (S6) and what the preview renders (S7). Every rule
// is a plain function in naivepost::cut_insert, so nothing here needs a widget or a display.

use naivepost::cut::{Cut, Seg};
use naivepost::cut_copy::footage_stretches;
use naivepost::cut_insert as ins;
use naivepost::cut_screen::cut_seconds;
use naivepost::cut_select::{Scope, Selection};
use naivepost::layout;
use std::path::Path;

/// The card's file, project-relative the one way §1 stores a path.
const CARD: &str = "assets/tier.svg";
/// A moving picture with sound of its own — the only kind that may have a lane.
const STING: &str = "assets/sting.mp4";
/// Sound alone: no picture, so it is laid over footage and never between it.
const SONG: &str = "assets/song.mp3";

fn footage(spans: &[(f64, f64)]) -> Cut {
    let mut cut = Cut::default();
    for (s, e) in spans {
        cut.segs.push(Seg { s: *s, e: *e, ..Default::default() });
    }
    cut
}

fn band(start: f64, end: f64, sound: bool) -> Selection {
    let scope = if sound { Scope::Sound { recording: "2026-09-16 17-26-20".into() } } else { Scope::Footage { row: 0 } };
    Selection { start, end, scope }
}

// --- S1: a line or a selection -------------------------------------------------------------------------------

/// F2.12 S1 — `Needs a line or a selection`: neither exists yet, so the refusal is §G's own sentence.
#[test]
fn f2_12_s1_with_nothing_placed_insert_refuses() {
    assert_eq!(ins::needs_place(None, None), Err(ins::NO_LINE_YET));
    assert_eq!(ins::NO_LINE_YET, "click the timeline where the insert goes first");
}

/// F2.12 S1 — a red line is enough on its own: an insert replaces footage rather than choosing it, so the line
/// and not a marked region is what §G's flow asks for first.
#[test]
fn f2_12_s1_a_line_alone_places_it() {
    assert_eq!(ins::needs_place(Some(42.5), None), Ok(42.5));
}

/// F2.12 S1 — a selection wins over the line and gives its own start: marking seconds and then pressing Insert
/// is a sentence that says what those seconds are for.
#[test]
fn f2_12_s1_a_selection_wins_over_the_line() {
    let marked = band(30.0, 40.0, false);
    assert_eq!(ins::needs_place(Some(99.0), Some(&marked)), Ok(30.0));
}

/// F2.12 S1 — a drag may run either way (F2.6), so the insert goes at the earlier end of the band.
#[test]
fn f2_12_s1_a_band_dragged_backwards_places_at_its_start() {
    assert_eq!(ins::needs_place(None, Some(&band(40.0, 30.0, false))), Ok(30.0));
}

// --- S2: the chooser and the file's kind ---------------------------------------------------------------------

/// F2.12 S2 — `chooser "Insert a clip, image, animation or sound"`, and the sound wording for a sound-scoped
/// selection: offering a tier board where the hand pointed at a waveform would be a picture nobody asked for.
#[test]
fn f2_12_s2_the_chooser_asks_for_what_the_scope_is() {
    assert_eq!(ins::chooser_title(None), "Insert a clip, image, animation or sound");
    assert_eq!(ins::chooser_title(Some(&band(30.0, 40.0, false))), "Insert a clip, image, animation or sound");
    assert_eq!(ins::chooser_title(Some(&band(30.0, 40.0, true))), "Insert a sound over the selected seconds");
}

/// F2.12 S2 — `opening in ‹root›/assets`: the folder cards live in, shared by every project under the root
/// (layout::assets_dir's rule rather than one of this flow's).
#[test]
fn f2_12_s2_the_chooser_opens_in_the_cards_folder() {
    let root = Path::new("/home/dev/lecture");
    assert_eq!(ins::chooser_dir(root), layout::assets_dir(root));
    assert_eq!(ins::chooser_dir(root), Path::new("/home/dev/lecture/assets"));
}

/// F2.12 S2 — `where the built-in SVG cards and CARDS.md are written on first use`, and nothing at all the
/// second time: these are starting points, so the chooser never opens empty and never rewrites a card.
#[test]
fn f2_12_s2_the_cards_are_written_once() {
    let dir = std::env::temp_dir().join(format!("naivepost-cards-once-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cards = [("tier.svg", "<svg>{{title}}</svg>")];

    let first = ins::seed_cards(&dir, &cards);
    assert_eq!(first, vec!["tier.svg".to_string(), ins::CARD_GUIDE_FILE.to_string()]);
    assert_eq!(std::fs::read_to_string(dir.join("tier.svg")).unwrap(), "<svg>{{title}}</svg>");

    // Second open: nothing to write, and the folder says so by changing not at all.
    let second = ins::seed_cards(&dir, &cards);
    assert!(second.is_empty(), "wrote {second:?} on a folder that already had them");
    let _ = std::fs::remove_dir_all(&dir);
}

/// F2.12 S2 — never over a card someone edited: only the note is added to a folder holding a restyled board.
#[test]
fn f2_12_s2_an_edited_card_is_left_alone() {
    let dir = std::env::temp_dir().join(format!("naivepost-cards-kept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tier.svg"), "<svg>mine</svg>").unwrap();

    let wrote = ins::seed_cards(&dir, &[("tier.svg", "<svg>{{title}}</svg>")]);
    assert_eq!(wrote, vec![ins::CARD_GUIDE_FILE.to_string()], "the guide is missing and may be added");
    assert_eq!(std::fs::read_to_string(dir.join("tier.svg")).unwrap(), "<svg>mine</svg>", "restyled by hand");
    let _ = std::fs::remove_dir_all(&dir);
}

/// F2.12 S2 — the note carries what someone writing a card can get wrong silently: the hole declaration the
/// form reads off the file.
#[test]
fn f2_12_s2_the_note_shows_the_declaration_the_form_reads() {
    assert_eq!(ins::CARD_GUIDE_FILE, "CARDS.md");
    assert!(ins::CARD_GUIDE.contains("<!-- Input: title | Title |"), "the format the form reads");
    assert!(ins::CARD_GUIDE.contains("[logo]"), "the Logo… picker's flag");
}

/// F2.12 S2 — `insKind by extension: video|svg|audio|still`, read off the name and not off a probe, with the
/// card's parameters stripped first: `?S=Dust II` is what makes it a different board, not part of its name.
#[test]
fn f2_12_s2_a_file_is_what_its_extension_says() {
    assert_eq!(ins::kind("a.mp4"), ins::Kind::Video);
    assert_eq!(ins::kind("a.MKV"), ins::Kind::Video, "case does not matter");
    for ext in ["mov", "webm", "avi", "m4v", "mpg", "mpeg", "ts"] {
        assert_eq!(ins::kind(&format!("a.{ext}")), ins::Kind::Video, "{ext}");
    }
    assert_eq!(ins::kind("tier.svg"), ins::Kind::Svg);
    for ext in ["mp3", "wav", "ogg", "oga", "flac", "m4a", "aac", "opus"] {
        assert_eq!(ins::kind(&format!("a.{ext}")), ins::Kind::Audio, "{ext}");
    }
    for ext in ["png", "jpg", "jpeg", "webp", "bmp", "gif"] {
        assert_eq!(ins::kind(&format!("a.{ext}")), ins::Kind::Still, "{ext}");
    }
    // A name nothing here knows is a still: it has no seconds of its own, which is the safe reading.
    assert_eq!(ins::kind("diagram.xyz"), ins::Kind::Still);
    assert_eq!(ins::kind("noextension"), ins::Kind::Still);
    assert_eq!(ins::kind("assets/tier.svg?S=Dust%20II&A=Nuke"), ins::Kind::Svg, "parameters are not a name");
}

// --- S3: what the gesture answers first ----------------------------------------------------------------------

/// F2.12 S3 — `selection → its length, overwrite`: the two answers stay together, because the selection that
/// gave the card its seconds is also what says those seconds are for.
#[test]
fn f2_12_s3_a_selection_lends_its_length_and_means_over() {
    let (splice, length) = ins::default_modes(Some(&band(30.0, 38.5, false)));
    assert!(!splice, "over the footage");
    assert_eq!(length, Some(8.5));
}

/// F2.12 S3 — `none → splice, the file's own length`: no length yet, because the file has to be chosen before
/// its length can be read — inventing one at the press would put a number in the form the file contradicts.
#[test]
fn f2_12_s3_no_selection_splices_and_leaves_the_length_to_the_file() {
    let (splice, length) = ins::default_modes(None);
    assert!(splice, "between the footage");
    assert_eq!(length, None, "read once the chooser comes back");
}

/// F2.12 S3 — a band under `P.policy.minSceneSeconds` is too short to place seconds over, so it reads as no
/// selection at all and the card goes between the footage instead of nowhere.
#[test]
fn f2_12_s3_a_band_under_the_floor_is_no_selection() {
    let (splice, length) = ins::default_modes(Some(&band(30.0, 30.4, false)));
    assert!(splice);
    assert_eq!(length, None);
}

/// F2.12 S3 — `the file's own length … else P.policy.insertDefaultSeconds = 4`. A still has no seconds of its
/// own, and an SVG whose CSS animation has no @keyframes in the file is drawn as a still: same fallback.
#[test]
fn f2_12_s3_a_file_with_no_length_of_its_own_falls_back_to_four_seconds() {
    // P.policy.insertDefaultSeconds = 4, catalogued from cut_insert::DEFAULT_SECONDS.
    assert_eq!(ins::DEFAULT_SECONDS, 4.0);
    assert_eq!(ins::default_length(Some(12.75), false), 12.75, "a sting of its own length");
    assert_eq!(ins::default_length(None, false), ins::DEFAULT_SECONDS, "a still");
    assert_eq!(ins::default_length(None, true), ins::DEFAULT_SECONDS, "an SVG with nothing animating it");
    assert_eq!(ins::default_length(Some(0.0), false), ins::DEFAULT_SECONDS, "a probe that found nothing");
}

// --- S4: the form in the column ------------------------------------------------------------------------------

/// F2.12 S4 — `one entry per declared card field`: the dialog comes out of the file, in the order the file asks,
/// with the hint keeping any bar it contains.
#[test]
fn f2_12_s4_the_card_declares_its_own_form() {
    let svg = br#"<svg>
        <!-- Input: title | Title | over the board, empty for none -->
        <text>{{title}}</text>
        <!-- Input: S[keep logo] | Tier S | what is in it, comma separated -->
</svg>"#;
    let fields = ins::card_fields(svg);
    assert_eq!(fields.len(), 2);
    assert_eq!((fields[0].key.as_str(), fields[0].label.as_str()), ("title", "Title"));
    assert_eq!(fields[0].hint, "over the board, empty for none");
    assert!(!fields[0].logo && !fields[0].keep, "no flags, so two plain entries");
    assert_eq!((fields[1].key.as_str(), fields[1].label.as_str()), ("S", "Tier S"));
    // The Logo… picker is a flag on the declaration, not a guess about what the key is called.
    assert!(fields[1].logo, "a mark to pick rather than type");
    assert!(fields[1].keep);
}

/// F2.12 S4 — a repeat of one key (compared without case) is dropped: two widgets writing one value would leave
/// which wins to wherever the hand clicked. A declaration with no label says its key back.
#[test]
fn f2_12_s4_one_hole_is_asked_for_once() {
    let svg = b"<!-- Input: Title | First | what is in it --> <!-- Input: title -->";
    let fields = ins::card_fields(svg);
    assert_eq!(fields.len(), 1, "the second declaration is the same hole");
    assert_eq!((fields[0].key.as_str(), fields[0].label.as_str()), ("Title", "First"), "the first wins");
    // A declaration with no label says its key back rather than leaving an entry nameless.
    let bare = ins::card_fields(b"<!-- Input: caption -->");
    assert_eq!((bare[0].key.as_str(), bare[0].label.as_str()), ("caption", "caption"));
    assert_eq!(bare[0].hint, "", "and says nothing about it");
}

/// F2.12 S4 — the three radios' wording, verbatim from §G: what the card DOES to the footage, said in full.
#[test]
fn f2_12_s4_the_three_radios_say_what_they_do() {
    assert_eq!(
        ins::BETWEEN_LABEL,
        "Insert BETWEEN the footage \u{2014} the video gets longer by the card, nothing filmed is lost"
    );
    assert_eq!(
        ins::OVER_LABEL,
        "Play OVER the footage \u{2014} the card replaces those seconds (the same as Remove)"
    );
    assert_eq!(
        ins::LANE_LABEL,
        "Put it on a LANE of its own \u{2014} a row of the band to cut to, and nothing is cut yet"
    );
}

/// F2.12 S4 — `LANE … (video only)`: a still on a lane would be a row that never changes and an audio file has
/// no picture for the green to choose.
#[test]
fn f2_12_s4_only_a_moving_picture_may_have_a_row() {
    assert!(ins::lane_offered(ins::Kind::Video));
    assert!(!ins::lane_offered(ins::Kind::Still), "a row that never changes");
    assert!(!ins::lane_offered(ins::Kind::Svg), "a card is drawn where it lands");
    assert!(!ins::lane_offered(ins::Kind::Audio), "and sound has no picture to cut to");
}

/// F2.12 S4 — `sound tick — shown only when there is a sound to answer for`: a sound insert IS the sound, and a
/// lane cuts nothing, so both have no question to ask. Over footage that carries sound, the question opens even
/// for a file that brought none.
#[test]
fn f2_12_s4_the_sound_question_opens_only_when_it_has_an_answer() {
    assert!(!ins::sound_open(ins::Kind::Audio, ins::Mode::Over, true, true), "it is the sound");
    assert!(!ins::sound_open(ins::Kind::Video, ins::Mode::Lane, true, true), "nothing under it yet");
    // Spliced: only the file's own sound is in play — there is no footage under a splice.
    assert!(ins::sound_open(ins::Kind::Svg, ins::Mode::Between, true, true));
    assert!(!ins::sound_open(ins::Kind::Svg, ins::Mode::Between, false, true), "and a still brings none");
    // Over: either side opens it.
    assert!(ins::sound_open(ins::Kind::Still, ins::Mode::Over, false, true), "the session's own carries on");
    assert!(ins::sound_open(ins::Kind::Video, ins::Mode::Over, true, false));
    assert!(!ins::sound_open(ins::Kind::Still, ins::Mode::Over, false, false), "nothing to answer for");
}

/// F2.12 S4 — one stored flag, two sentences: the tick says what the mode makes true (§G's wording both ways).
#[test]
fn f2_12_s4_the_tick_says_whichever_sentence_the_mode_means() {
    assert_eq!(ins::tick_label(true), "Play it SILENT \u{2014} the insert's own sound is not used");
    assert_eq!(
        ins::tick_label(false),
        "Keep the sound running under it \u{2014} only the picture is replaced"
    );
    assert_eq!(ins::tick_label(true), ins::SILENT_LABEL);
    assert_eq!(ins::tick_label(false), ins::KEEP_LABEL);
}

/// F2.12 S4 — the Seconds entry: what was typed, or the length already settled on. `//
/// P.policy.minSceneSeconds` = 1.0 is the floor, read from cut_select where §10's row is catalogued.
#[test]
fn f2_12_s4_seconds_keeps_the_default_for_junk_and_for_a_flash() {
    assert!(ins::seconds_accepts(1.0), "P.policy.minSceneSeconds");
    assert!(!ins::seconds_accepts(0.5));
    assert_eq!(ins::seconds_or_default("3", 4.0), 3.0);
    assert_eq!(ins::seconds_or_default(" 2.5 ", 4.0), 2.5, "spaces from an entry");
    assert_eq!(ins::seconds_or_default("", 4.0), 4.0, "an emptied entry is not a length");
    assert_eq!(ins::seconds_or_default("four", 4.0), 4.0, "unparsable keeps the default");
    assert_eq!(ins::seconds_or_default("0.5", 4.0), 4.0, "under a second there is nothing to grab");
}

// --- S5: placing it ------------------------------------------------------------------------------------------

/// F2.12 S5 — `spliced → s == e with dur`: the cut is opened there and the footage carries on from the very next
/// frame, so no session second is given up (footage_stretches unchanged) while the finished video gets longer by
/// exactly the card's own seconds.
#[test]
fn f2_12_s5_a_spliced_card_costs_no_session_time() {
    let mut cut = footage(&[(0.0, 30.0), (40.0, 60.0)]);
    let was = cut_seconds(&cut);
    let placed = ins::place(&mut cut, CARD, 35.0, ins::Mode::Between, 4.0, false).unwrap();

    let card = &cut.segs[cut.segs.len() - 1];
    assert_eq!((card.s, card.e), (35.0, 35.0), "s == e is what makes it spliced");
    assert_eq!(card.dur, 4.0);
    assert_eq!(card.ins, CARD);
    assert!(card.is_insert() && !card.is_overwrite_insert());
    // The footage under it is exactly as it was: the hole at 30–40 is still a hole.
    assert_eq!(footage_stretches(&cut, 0.0, 60.0), vec![(0.0, 30.0), (40.0, 60.0)]);
    assert_eq!(cut_seconds(&cut) - was, 4.0, "the video is longer by the card");
    assert_eq!(placed.segs.len(), 1);
}

/// F2.12 S5 — `overwriting → replaces those seconds`: the footage under the card is gone as － Remove would take
/// it and everything outside the span survives untouched, byte for byte.
#[test]
fn f2_12_s5_over_takes_exactly_those_seconds() {
    let cut = footage(&[(0.0, 30.0), (40.0, 60.0)]);
    let before = cut.segs.clone();
    let mut after = cut.clone();
    ins::place(&mut after, "assets/logo.png", 10.0, ins::Mode::Over, 5.0, false).unwrap();

    let card = after.segs.iter().find(|seg| !seg.ins.is_empty()).expect("the card is in the cut");
    assert_eq!((card.s, card.e), (10.0, 15.0));
    assert_eq!(card.dur, 0.0, "an overwriting insert runs for the footage it replaced");
    assert!(card.is_overwrite_insert() && !card.is_insert());
    // The clip it landed in came back either side of it; the other clip was never touched.
    let untouched: Vec<&Seg> = after.segs.iter().filter(|seg| seg.ins.is_empty()).collect();
    assert_eq!(untouched.len(), 3, "0–10, 15–30 and the second clip");
    assert!(untouched.contains(&&before[1]), "the clip at 40–60 is byte-identical to itself");
    assert_eq!(cut_seconds(&after), cut_seconds(&cut), "over costs nothing the cut was not already");
}

/// F2.12 S5 — a card over a hole is drawn over nothing: it covers only the footage that is there, so an insert
/// never becomes a scene out of thin air.
#[test]
fn f2_12_s5_over_a_hole_covers_no_footage() {
    let mut cut = footage(&[(0.0, 30.0)]);
    ins::place(&mut cut, CARD, 40.0, ins::Mode::Over, 4.0, false).unwrap();
    let cards: Vec<&Seg> = cut.segs.iter().filter(|seg| !seg.ins.is_empty()).collect();
    assert!(cards.is_empty(), "there was no picture to put it over");
}

/// F2.12 S5 — `sound → over kept footage`, one piece per stretch, each carrying its own part of the file; the
/// picture is left exactly as it was.
#[test]
fn f2_12_s5_a_sound_is_laid_over_every_kept_stretch() {
    let mut cut = footage(&[(0.0, 30.0), (40.0, 60.0)]);
    let before = cut.segs.clone();
    let placed = ins::place(&mut cut, SONG, 20.0, ins::Mode::Between, 30.0, false).unwrap();

    assert_eq!(placed.segs.len(), 2, "one piece per stretch of footage");
    assert_eq!((placed.segs[0].s, placed.segs[0].e), (20.0, 30.0));
    assert_eq!((placed.segs[1].s, placed.segs[1].e), (40.0, 50.0));
    // The second piece resumes ten seconds in: two parts of one song, not its opening twice.
    assert_eq!(placed.segs[1].ss, 20.0);
    let kept: Vec<(f64, f64)> = cut.segs.iter().filter(|seg| seg.ins.is_empty()).map(|s| (s.s, s.e)).collect();
    assert_eq!(kept, vec![(0.0, 20.0), (50.0, 60.0)], "the picture is as it was outside the span");
    assert_eq!(cut_seconds(&cut), cut_seconds(&footage(&before.iter().map(|s| (s.s, s.e)).collect::<Vec<_>>())));
}

/// F2.12 S5 — a sound with no picture under it refuses and changes nothing: `the cut keeps no footage at m:ss —
/// <file> is a sound, and one needs a picture under it`.
#[test]
fn f2_12_s5_a_sound_with_no_picture_under_it_refuses() {
    let mut cut = footage(&[(0.0, 30.0)]);
    let before = cut.clone();
    let err = ins::place(&mut cut, SONG, 45.0, ins::Mode::Over, 6.0, false).unwrap_err();
    assert_eq!(err, "the cut keeps no footage at 00:45 — song.mp3 is a sound, and one needs a picture under it");
    assert_eq!(cut, before, "a refusal edits nothing");
}

/// F2.12 S5 — `LANE … a row of its own`: the file becomes a row and NOTHING is added to the cut, because a lane
/// that arrived already green would be a cut nobody made. A second copy of the same file cannot share the name.
#[test]
fn f2_12_s5_a_lane_is_a_row_and_cuts_nothing() {
    let mut cut = footage(&[(0.0, 30.0)]);
    let before = cut.segs.clone();
    let placed = ins::place(&mut cut, STING, 12.0, ins::Mode::Lane, 8.0, false).unwrap();

    assert_eq!(cut.segs, before, "nothing is added to the cut by this");
    assert!(placed.segs.is_empty());
    assert_eq!(cut.lanes.len(), 1);
    let lane = &cut.lanes[0];
    assert_eq!((lane.src.as_str(), lane.at, lane.dur), (STING, 12.0, 8.0));
    assert_eq!(lane.name, "sting.mp4", "named off the file");

    // A second copy gets its own name: Lane::name keys Cut::rows and every scene's quiet list.
    ins::place(&mut cut, STING, 20.0, ins::Mode::Lane, 8.0, false).unwrap();
    assert_eq!(cut.lanes[1].name, "sting.mp4-2");
    assert_eq!(cut_seconds(&cut), cut_seconds(&footage(&[(0.0, 30.0)])), "a lane is not a scene");
}

/// F2.12 S5 — the status line, whole: §G's template in its own order, including what the finished video now
/// measures and the ↶ that takes it back. One decimal for the card's length, as §G spells `X s`.
#[test]
fn f2_12_s5_the_status_names_the_card_and_both_lengths() {
    let mut cut = footage(&[(0.0, 30.0)]);
    let placed = ins::place(&mut cut, "assets/tier.svg?S=Dust%20II", 12.0, ins::Mode::Between, 4.0, false).unwrap();
    assert_eq!(
        placed.status,
        "tier.svg inserted at 00:12 for 4.0 s, between the footage, which is cut open for it \u{2014} \
         the cut is now 00:34 (was 00:30) \u{2014} \u{21b6} Undo takes it back"
    );
    assert_eq!(placed.how, "between the footage, which is cut open for it");

    // Silent says why the sound is not there; over says what to do with its edges.
    let mut other = footage(&[(0.0, 30.0)]);
    let silent = ins::place(&mut other, STING, 5.0, ins::Mode::Between, 4.0, true).unwrap();
    assert_eq!(silent.how, ins::BETWEEN_SILENT_LABEL);
    assert!(silent.status.contains(ins::BETWEEN_SILENT_LABEL));
    let mut third = footage(&[(0.0, 30.0)]);
    let over = ins::place(&mut third, CARD, 5.0, ins::Mode::Over, 4.0, false).unwrap();
    assert_eq!(over.how, "over the footage \u{2014} drag its edges to retime it");
    // A lane names itself in the sentence and says what to press next.
    let mut fourth = footage(&[(0.0, 30.0)]);
    let lane = ins::place(&mut fourth, STING, 5.0, ins::Mode::Lane, 4.0, false).unwrap();
    assert_eq!(lane.how, "on a lane of its own (sting.mp4) \u{2014} select on that row and press \u{ff0b} Add to cut to it");
}

// --- S6: a held card is edited, not duplicated -----------------------------------------------------------------

/// F2.12 S6 — `Right-click or double left click a card … Insert becomes Edit`: holding one is a statement about
/// what someone is working on. Footage is not held for an edit, because there are no answers to re-open.
#[test]
fn f2_12_s6_a_held_card_turns_the_button_into_edit() {
    let card = Seg { s: 12.0, e: 12.0, ins: CARD.into(), dur: 4.0, ..Default::default() };
    assert!(ins::hold(Some(&card)));
    assert!(!ins::hold(Some(&Seg { s: 0.0, e: 30.0, ..Default::default() })), "footage has no form");
    assert!(!ins::hold(None));
    assert_eq!(ins::edit_verb(true), "Edit");
    assert_eq!(ins::edit_verb(false), "Insert");
}

/// F2.12 S6 — the card is found again by what it IS rather than where it was: a trim, a merge or another insert
/// renumbers the list while the dialog is open, so a kept index would open the wrong card. Its parameters are
/// part of its identity — tier.svg and tier.svg?S=Dust II are different boards.
#[test]
fn f2_12_s6_the_card_is_re_found_by_identity() {
    let mut cut = Cut::default();
    cut.segs.push(Seg { s: 12.0, e: 12.0, ins: CARD.into(), dur: 4.0, ..Default::default() });
    cut.segs.push(Seg { s: 20.0, e: 20.0, ins: "assets/tier.svg?S=Dust%20II".into(), dur: 4.0, ..Default::default() });

    assert_eq!(ins::find_card(&cut, CARD, 12.0), Some(0));
    assert_eq!(ins::find_card(&cut, "assets/tier.svg?S=Dust%20II", 20.0), Some(1));
    // Gone: ⌦ took it while the window was open, so §G's sentence is the answer rather than an edit of nothing.
    assert_eq!(ins::find_card(&cut, CARD, 99.0), None);
    assert_eq!(ins::GONE, "that card is no longer in the cut");
}

/// F2.12 S6 — `switching modes returns or takes footage`: between→over costs the seconds as － Remove would,
/// over→between gives them back and lengthens the video by the card; anything else changes nothing underneath.
#[test]
fn f2_12_s6_switching_modes_returns_or_takes_footage() {
    assert_eq!(ins::switching_takes_footage(ins::Mode::Between, ins::Mode::Over), Some(true));
    assert_eq!(ins::switching_takes_footage(ins::Mode::Over, ins::Mode::Between), Some(false));
    assert_eq!(ins::switching_takes_footage(ins::Mode::Between, ins::Mode::Between), None);
    assert_eq!(ins::switching_takes_footage(ins::Mode::Over, ins::Mode::Over), None);
    // A lane moving touches no footage at all: nothing was cut to it.
    assert_eq!(ins::switching_takes_footage(ins::Mode::Lane, ins::Mode::Over), None);
    assert_eq!(ins::switching_takes_footage(ins::Mode::Over, ins::Mode::Lane), None);
}

// --- S7: what the preview shows while it plays -----------------------------------------------------------------

/// F2.12 S7 — `cards render at 8 fps (one frame for stills)`: a picture that never changes needs one texture and
/// not sixty-four of the same one, and an SVG with a CSS animation but no @keyframes is drawn as a still.
#[test]
fn f2_12_s7_moving_pictures_render_at_eight_frames_a_second() {
    assert_eq!(ins::PREVIEW_FPS, 8.0);
    assert_eq!(ins::preview_fps(ins::Kind::Video, false), Some(8.0));
    assert_eq!(ins::preview_fps(ins::Kind::Svg, true), Some(8.0));
    assert_eq!(ins::preview_fps(ins::Kind::Svg, false), None, "a still card");
    assert_eq!(ins::preview_fps(ins::Kind::Still, false), None);
    assert_eq!(ins::preview_fps(ins::Kind::Audio, false), None, "sound has no frames to show");
}

/// F2.12 S7: the seconds each rendered frame stands at — the first included and the last one kept, because it is
/// what plays in the card's final tenth of a second. Nothing to render answers with nothing.
#[test]
fn f2_12_s7_the_frames_are_counted_from_the_first() {
    assert_eq!(ins::frame_times(2.0, 8.0).len(), 16);
    let times = ins::frame_times(0.5, 8.0);
    assert_eq!((times.first(), times.last()), (Some(&0.0), Some(&0.375)));
    assert_eq!(ins::frame_times(0.0, 8.0).len(), 0);
    assert_eq!(ins::frame_times(4.0, 0.0).len(), 0, "no rate, no frames");
}

/// F2.12 S7 — `nearest rendered frame shown`: nothing is decoded on the fly, so the playhead gets the closest
/// texture that has arrived, and an empty strip shows nothing rather than guessing.
#[test]
fn f2_12_s7_the_nearest_rendered_frame_is_shown() {
    let times = ins::frame_times(1.0, 8.0);
    assert_eq!(ins::nearest_rendered(&times, 0.34), Some(0.375), "the closest texture that has arrived");
    assert_eq!(ins::nearest_rendered(&times, 0.30), Some(0.25), "and the other way round");
    assert_eq!(ins::nearest_rendered(&times, 0.0), Some(0.0));
    assert_eq!(ins::nearest_rendered(&[], 1.0), None);
}

/// F2.12 S7 — `48-texture cap` and `rendered via ffmpeg at ≤960 px`: a minute of a sting at 8 fps is four
/// hundred and eighty textures nobody will look at, and the preview is a film strip in a column rather than the
/// exported picture.
#[test]
fn f2_12_s7_the_strip_is_capped_in_frames_and_pixels() {
    assert_eq!(ins::TEXTURE_CAP, 48);
    assert_eq!(ins::preview_width(1920), 960);
    assert_eq!(ins::preview_width(640), 640, "never widened");
}

/// F2.12 S7: how ffmpeg is asked for the frames — an argument vector only, so the shape of the call is testable
/// and the running of it stays with subprocess. `-an` because a strip has no business decoding sound it will not
/// play: a card's own audio is its own pipeline.
#[test]
fn f2_12_s7_the_render_asks_for_frames_and_no_sound() {
    let args = ins::ffmpeg_frames("ffmpeg", "assets/tier.svg?S=Dust%20II", 8.0, 1920, "/tmp/strip/%03d.png");
    assert_eq!(args[0], "ffmpeg");
    assert!(args.windows(2).any(|w| w[0] == "-i" && w[1] == "assets/tier.svg?S=Dust%20II"), "the card as named");
    assert!(args.contains(&"-an".to_string()), "no audio in a film strip");
    let vf = args.iter().find(|a| a.starts_with("fps=")).expect("the frame rate and width are the -vf filter");
    assert!(vf.starts_with("fps=8.000000"), "{vf}");
    assert!(vf.contains("min(960,-2)"), "capped at 960 px: {vf}");
    assert_eq!(args.last().unwrap(), "/tmp/strip/%03d.png");
}

/// F2.12 S7 — `a spliced card holds the footage while it plays on the wall clock`: the playhead cannot carry a
/// card that costs no session time, so the picture waits under it — worth saying because everywhere else on this
/// page the picture moves when the clock does.
#[test]
fn f2_12_s7_a_spliced_card_holds_the_footage() {
    assert_eq!(ins::held_status("assets/tier.svg?S=Dust%20II"), "tier.svg \u{2014} the footage is held while it plays");
    assert_eq!(ins::playhead_advance(true, 3.5, 1.0), 0.0, "held: the footage waits");
    assert_eq!(ins::playhead_advance(false, 3.5, 1.0), 3.5);
    // Released and at half speed, seconds are still seconds — the card just holds them for longer.
    assert_eq!(ins::playhead_advance(false, 3.5, 0.5), 1.75);
}
