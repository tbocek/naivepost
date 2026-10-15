//! §06-effects#5-cards-svg-inserts — the render's CALLER for cards, [`naivepost::produce_cards`].
//!
//! `tests/cut_cards_svg.rs` pins s1–s8: that a path carries arguments, that a document declares its inputs, that
//! holes fill, what the two shipped cards look like, the seeds, "the static file is the finished card", and the SMIL
//! and CSS readers. None of that renders anything, which is why all of it could be true while a card reached ffmpeg
//! with its braces still in it. These tests (s9–s15) are the other half: one insert path in, a finished picture out
//! — the §8 sentence said, the holes filled or the card redrawn, and an animated card baked to one static SVG per
//! frame at the bake fps.
//!
//! Every assertion goes through [`naivepost::produce_cards::arrive`] / [`naivepost::produce_cards::stage`] or through
//! [`naivepost::produce_exec::plan`] and [`naivepost::produce_render::encode_command_with`], i.e. the doors a run
//! walks, and never at a private helper. Documents are written inline; no fixture file is needed because a card is
//! only ever a document plus a path.

use std::path::{Path, PathBuf};

use naivepost::cut::Seg;
use naivepost::cut_cards as cards;
use naivepost::cut_insert;
use naivepost::layout::Tree;
use naivepost::produce_cards;
use naivepost::produce_exec as exec;
use naivepost::produce_render as render;
use naivepost::produce_translate;
use naivepost::project::Produce;

fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("np-06e5-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write one card on disk and answer its path, so `stage` and `offer_seconds` read a real file.
fn card_file(tag: &str, name: &str, doc: &str) -> PathBuf {
    let dir = temp_root(tag);
    let path = dir.join(name);
    std::fs::write(&path, doc).unwrap();
    path
}

/// A plain 1920×1080 card body around some content, the shape every §5 card has.
fn canvas(body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\"\
         viewBox=\"0 0 1920 1080\"><rect width=\"1920\" height=\"1080\" fill=\"#0d1117\"/>{body}</svg>"
    )
}

// --- S9: a stamped card is drawn again by its generator -------------------------------------------------------------

/// §8 (`a shipped card … as an insert it is redrawn from the generator with the path's parameters merged over those
/// defaults`): the file's own bytes are not what gets drawn — the generator runs again. And §8's first sentence is
/// the note the render logs, with its `except that …` half naming what the path changed.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s9_a_stamped_card_is_redrawn_by_its_card() {
    // The shipped badge, stamp and all, with its own defaults recorded beside it.
    let shipped = cards::badge_svg("S", "S tier");
    assert_eq!(
        cards::stamped(&shipped),
        Some(("badge", "letter=S&caption=S tier")),
        "a shipped card records the arguments it was drawn with: {shipped}"
    );

    // Nothing changed on the path: the plain sentence, and the same picture the file already was.
    let plain = produce_cards::draw("assets/s.svg", &shipped);
    assert_eq!(plain.arrived, cards::drawn_by("badge", None));
    assert_eq!(plain.arrived, "drawn by the built-in badge card");
    assert!(plain.logs.contains(&plain.arrived.clone()), "{:?}", plain.logs);
    assert!(plain.drawn.contains(">S</text>"), "{}", plain.drawn);
    assert!(!plain.drawn.contains("{{"), "a redrawn card has no holes left");

    // One argument changed: the SAME card, drawn with the new words, and the note says which.
    let asked = produce_cards::draw("assets/s.svg?caption=Champion%20run", &shipped);
    assert!(asked.arrived.starts_with("drawn by the built-in badge card, except that "), "{asked:?}");
    assert!(asked.arrived.contains("caption"), "{}", asked.arrived);
    assert!(asked.drawn.contains("Champion run"), "{}", asked.drawn);
    assert!(!asked.drawn.contains("S tier"), "the shipped caption is gone: {}", asked.drawn);
    assert!(!asked.drawn.contains("{{"), "{}", asked.drawn);

    // The merge itself: defaults first, the path's values over them, order kept.
    let merged = produce_cards::merge_args("letter=S&caption=S tier", &[("caption".to_string(), "Best of 2026".into())]);
    assert_eq!(merged[0], ("letter".to_string(), "S".to_string()));
    assert_eq!(merged[1], ("caption".to_string(), "Best of 2026".to_string()));
    assert_eq!(produce_cards::changed_note("letter=S", &[]), None, "nothing typed is nothing to except");
}

// --- S10: an unstamped card gets its holes filled -------------------------------------------------------------------

/// §5 (`holes as {{name}} / {{name|fallback}} (outside comments)`) + §8 (`"filled in"`): the scope answers a hole,
/// the fallback answers it when nobody does, a comment's hole is left alone, and a value is XML-escaped on the way in.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s10_an_unstamped_card_is_filled_in() {
    let doc = canvas(
        "<!-- Input: title | Title | big --><!-- a hole in a comment: {{note}} -->\
         <text x=\"100\" y=\"200\">{{title}}</text>\
         <text x=\"100\" y=\"300\">{{who|nobody}}</text>",
    );
    let args = vec![
        ("title".to_string(), "Ranks & records".to_string()),
        ("extra".to_string(), "unused".to_string()),
    ];
    let filled = produce_cards::draw("assets/mine.svg", &doc);
    let joined = produce_cards::join_for_test(&doc, &args);

    assert_eq!(filled.arrived, cards::FILLED_IN, "\"filled in\", not \"drawn by\"");
    assert!(joined.contains("Ranks &amp; records"), "the value arrives XML-escaped: {joined}");
    assert!(joined.contains(">nobody<"), "an unanswered hole takes its fallback: {joined}");
    assert!(!joined.contains("{{title}}"), "{joined}");
    assert!(joined.contains("{{note}}"), "a hole inside a comment stays a comment: {joined}");
    // The door the render uses gives the same answer, with the note logged beside it.
    let arrived = produce_cards::arrive("assets/mine.svg", &doc);
    assert!(arrived.logs.iter().any(|line| line == cards::FILLED_IN), "{:?}", arrived.logs);
}

// --- S11: nothing to fill, and the parameters are ignored -----------------------------------------------------------

/// §8 (`"has no {{placeholders}} and was not drawn by a card, so its parameters were ignored`): a path asking for a
/// title the picture has nowhere for is a change nobody sees either in the video or in the absence of one, so it is
/// said rather than swallowed. The sentence is `cut_cards::IGNORED`, unchanged.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s11_parameters_with_nowhere_to_go_are_ignored() {
    let doc = canvas("<text x=\"100\" y=\"200\">hand-drawn, no holes</text>");
    let card = produce_cards::draw("assets/plain.svg?title=Anything", &doc);
    assert_eq!(card.arrived, cards::IGNORED);
    assert!(card.logs.iter().any(|line| line == cards::IGNORED), "{:?}", card.logs);
    assert_eq!(card.drawn, doc, "nothing was written into a picture with no holes");

    // No arguments either: then there was nothing to ignore, and the honest note is "filled in".
    let quiet = produce_cards::draw("assets/plain.svg", &doc);
    assert_eq!(quiet.arrived, cards::FILLED_IN);
}

// --- S12: self-contained, or refused --------------------------------------------------------------------------------

/// §8's contract (`self-contained … the render folder resolves no relative href, refuses an absolute one`; `machine
/// fonts as a family list, no @font-face`): three cases, none of them silent. Only the absolute href stops the bake.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s12_a_card_carries_its_own_images_and_fonts() {
    // A data: URL travels with the document, so it bakes.
    let inline = canvas(
        "<image href=\"data:image/png;base64,iVBORw0KGgo=\" x=\"0\" y=\"0\"/>\
         <animate attributeName=\"opacity\" values=\"0;1\" keyTimes=\"0;1\" dur=\"2s\" fill=\"freeze\"/>",
    );
    let ok = produce_cards::arrive("assets/data.svg", &inline);
    assert!(ok.baked(), "a self-contained animated card bakes: {:?}", ok.logs);
    assert!(ok.logs.iter().all(|line| !line.starts_with("!!!")), "{:?}", ok.logs);

    // An empty href is how an attribute says "no image here" and is left alone.
    let empty = canvas("<image href=\"\" x=\"0\" y=\"0\"/>");
    assert_eq!(cards::absolute_href(&empty), None);

    // An absolute href points somewhere the render folder cannot reach: refused, named, and said with `!!!`.
    let linked = canvas("<image href=\"/home/dev/logo.png\" x=\"0\" y=\"0\"/>");
    let refused = produce_cards::arrive("assets/linked.svg", &linked);
    assert!(!refused.baked(), "a refused card is not baked");
    let refusal = refused.logs.iter().find(|line| line.starts_with("!!!")).expect("the refusal is logged");
    assert!(refusal.contains("/home/dev/logo.png"), "{refusal}");
    assert_eq!(refused.logs.first().map(String::as_str), Some(refusal.as_str()), "it comes first: {:?}", refused.logs);
    assert_eq!(refused.still.as_deref(), Some("linked-filled.svg"), "and the clip still gets one file");

    // A webfont the render cannot fetch: said, not fatal — the card still draws in a machine font.
    let webfont = canvas("<style>@font-face { font-family: X }</style><text x=\"1\" y=\"2\">hi</text>");
    let faced = produce_cards::arrive("assets/font.svg", &webfont);
    assert!(faced.logs.iter().any(|line| line.contains("@font-face") && line.starts_with(">>>")), "{:?}", faced.logs);
    assert!(!faced.logs.iter().any(|line| line.starts_with("!!!")), "a font is not a refusal: {:?}", faced.logs);
}

// --- S13: the SMIL bake ---------------------------------------------------------------------------------------------

/// §5 (`Render bakes SMIL … to frames at the render fps (default 25)`): a card whose animation runs 2 s bakes
/// `bake_times(2.0)` frames, each a static SVG — no animation tag survives into a frame — and the values a frame
/// carries are the ones [`cards::value_at`] answers at that second.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s13_an_animated_card_bakes_one_static_svg_per_frame() {
    let doc = canvas(
        "<text x=\"100\" y=\"200\" opacity=\"0\">Hello<animate attributeName=\"opacity\" values=\"0;1\"\
         keyTimes=\"0;1\" dur=\"2s\" fill=\"freeze\"/></text>",
    );
    let card = produce_cards::arrive("assets/titled.svg", &doc);

    // The last moving moment is 2 s, and the shipped stillness is NOT added to it (§5: the generator leaves it
    // inside its own timeline, so it is already part of the last moving moment).
    assert_eq!(card.length, Some(2.0), "{:?}", card.length);
    assert_eq!(cards::STILLNESS_SECONDS, 2.2);
    let times = cards::bake_times(2.0);
    // `bake_times` is the frame grid; the bake adds the finished frame at the animation's own end, so a 2 s card
    // lands its frozen value on screen instead of stopping one frame short of it.
    assert_eq!(times.len(), 50, "2 s at 25 fps");
    assert_eq!(card.frames.len(), times.len() + 1, "one frame per moment, plus the finished one");
    assert_eq!(card.frame_dir(), "titled.frames", "{}", card.frame_dir());
    assert_eq!(card.still, None, "a baked card has no single static file");

    // Named as §8 spells it, and in order.
    assert_eq!(card.frames.first().unwrap().0, "f00000.svg");
    assert_eq!(card.frames.last().unwrap().0, format!("f{:05}.svg", card.frames.len() - 1));
    assert_eq!(produce_cards::frame_name(7), "f00007.svg");

    // Frame 0 is the animation's start; a middle frame is the interpolated value; the last holds the frozen end.
    let anim = cards::parse_anim(
        "<animate attributeName=\"opacity\" values=\"0;1\" keyTimes=\"0;1\" dur=\"2s\" fill=\"freeze\"/>",
    )
    .unwrap();
    let first = &card.frames[0].1;
    let middle = &card.frames[25].1;
    // Read the last frame fresh: `frames` grew after the earlier bindings were taken.
    let last = card.frames.last().cloned().unwrap();
    assert!(first.contains("opacity=\"0\""), "frame 0 reads the start: {first}");
    let expected = cards::value_at(&anim, times[25]).unwrap();
    assert!(middle.contains(&format!("opacity=\"{expected}\"")), "frame 25 should read {expected}: {middle}");
    // A middle frame reads exactly what `value_at` answers at that second.
    let _ = first;

    // The point of the bake: every frame is a plain picture. No animation tags, and none of the source's SMIL.
    for (name, text) in &card.frames {
        assert!(!text.contains("<animate"), "{name} still carries an animation tag");
        assert!(!text.contains("</text><animate"), "{name}");
        assert!(text.contains("<svg"), "{name} is still a document");
    }
    // The strip is what ffmpeg reads: a pattern under the scratch folder, not the card's own path. `bake_times(2.0)`
    // stops just short of the end (the last frame it asks for is 1.96 s), so the frozen value is what the LAST frame
    // carries — and it carries it once, not twice.
    assert_eq!(last.1.matches("opacity=").count(), 1, "one opacity per frame: {}", last.1);
    assert!(last.1.contains("opacity=\"1\""), "freeze holds the end: {}", last.1);
    assert!(produce_cards::frame_pattern(Path::new("/tmp/x/a.frames")).ends_with("a.frames/f%05d.svg"));
}

// --- S14: the CSS subset, and what it refuses -----------------------------------------------------------------------

/// §5 (`and a CSS subset (opacity, transform, fill; simple selectors; animation longhands; standard easings)`) plus
/// §8's verbatim sentence for a CSS animation whose @keyframes are missing.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s14_css_animations_bake_and_a_missing_keyframe_is_a_still() {
    // A rule on a class, ramping opacity through `ease-in-out` over 2 s.
    let doc = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\">\
               <style>.fade { animation: fade 2s ease-in-out 0s 1 forwards }\
               @keyframes fade { from { opacity: 0 } to { opacity: 1 } }</style>\
               <rect class=\"fade\" width=\"10\" height=\"10\"/></svg>";
    let rules = produce_cards::css_rules(doc);
    assert_eq!(rules.len(), 1, "{rules:?}");
    assert_eq!(rules[0].selector, ".fade");
    assert_eq!(rules[0].property, "opacity");
    assert_eq!(rules[0].keyframes.as_deref(), Some("fade"));
    assert_eq!(rules[0].steps.len(), 2);
    // The easing is the standard one, and it actually shapes the ramp: at half time an ease-in-out is behind linear.
    assert_eq!(rules[0].easing, cards::easing("ease-in-out").unwrap());
    assert!(rules[0].style_at(1.0).unwrap().contains("opacity:"));
    let shaped = rules[0].style_at(1.0).unwrap();
    let value: f64 = shaped.split(':').nth(1).unwrap().trim().parse().unwrap();
    assert!((0.35..0.5).contains(&value), "ease-in-out at half time: {shaped}");
    // Past the end with `forwards`: the last step is held, which is exactly what the word means.
    assert_eq!(rules[0].style_at(3.0).as_deref(), Some("opacity:1"), "forwards holds the last step");

    let card = produce_cards::arrive("assets/css.svg", doc);
    assert_eq!(card.length, Some(2.0), "{:?}", card.length);
    assert_eq!(card.frames.len(), cards::bake_times(2.0).len() + 1);
    assert!(card.frames[40].1.contains("style=\"opacity:"), "{}", card.frames[40].1);
    for (_, text) in &card.frames {
        // The sheet is dropped from every frame: a baked frame carries its values inline, and leaving the rules in
        // would let whichever renderer ffmpeg uses animate the same picture a second time on top of ours.
        assert!(!text.contains("@keyframes"), "a frame is a picture, not a stylesheet: {text}");
        assert!(!text.contains("<style"), "and no stylesheet at all: {text}");
    }

    // A CSS animation naming a keyframe set that is not in the file: a still, and §8's sentence word for word.
    let lost = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\">\
                <style>.fade { animation: vanished 2s linear forwards } .other { opacity: 0.5 }</style>\
                <rect class=\"fade\" width=\"10\" height=\"10\"/></svg>";
    assert_eq!(
        produce_cards::css_rules(lost).len(),
        1,
        "a rule whose @keyframes are missing is still a rule, so the reason can be said"
    );
    let card = produce_cards::arrive("assets/assets/lost.css.svg", lost);
    assert_eq!(card.frames.len(), 0, "nothing to interpolate means no frames");
    assert_eq!(card.length, None, "and so no length of its own");
    assert!(
        card.logs
            .iter()
            .any(|line| line == "lost.css.svg: a CSS animation with no @keyframes in the file \u{2014} drawn as a still"),
        "{:?}",
        card.logs
    );

    // Outside the granted properties the bake refuses rather than approximating — but it says which rule it dropped
    // by dropping it: a stroke-width animation is not baked, and the card is a still.
    let outside = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\">\
                   <style>.bar { animation: grow 2s linear forwards }\
                   @keyframes grow { from { stroke-width: 1 } to { stroke-width: 8 } }</style>\
                   <rect class=\"bar\" width=\"10\" height=\"10\"/></svg>";
    assert!(!cards::css_supported("stroke-width: 3"));
    assert_eq!(produce_cards::css_rules(outside).len(), 1, "the rule is read; its property is not one §5 grants");
    assert_eq!(
        produce_cards::css_rules(outside)[0].property,
        "",
        "nothing in those keyframes is a granted property, so there is nothing to ramp"
    );
    let still = produce_cards::arrive("assets/outside.svg", outside);
    assert!(still.frames.is_empty(), "the unsupported half is not approximated");
}

// --- S15: the card's length, offered and looped ---------------------------------------------------------------------

/// §5 (`Card length = last moving moment, offered as the insert's length`) + §8 (`An indefinitely repeating document
/// has no own length: takes the default insert length, loops for its slot`). Both halves reach the program: the form
/// through [`produce_cards::offer_seconds`], the encode through the `card` input kind.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s15_the_length_reaches_the_form_and_the_encode() {
    let timed_doc = canvas(
        "<text x=\"100\" y=\"200\" opacity=\"0\">Hi<animate attributeName=\"opacity\" values=\"0;1\"\
         keyTimes=\"0;1\" dur=\"2s\" fill=\"freeze\"/></text>",
    );
    let endless_doc = canvas(
        "<text x=\"100\" y=\"200\" opacity=\"0\">Spin<animate attributeName=\"opacity\" values=\"0;1\"\
         keyTimes=\"0;1\" dur=\"2s\" repeatCount=\"indefinite\"/></text>",
    );
    let still_doc = canvas("<text x=\"100\" y=\"200\">A still</text>");

    let timed = card_file("len-timed", "timed.svg", &timed_doc);
    let endless = card_file("len-endless", "endless.svg", &endless_doc);
    let still = card_file("len-still", "still.svg", &still_doc);

    // Offered to the form: the card's own seconds, and nothing at all for a still or an endless card, which is where
    // the caller falls back to P.policy.insertDefaultSeconds.
    assert_eq!(produce_cards::offer_seconds(&timed.to_string_lossy()), Some(2.0));
    assert_eq!(produce_cards::offer_seconds(&endless.to_string_lossy()), None);
    assert_eq!(produce_cards::offer_seconds(&still.to_string_lossy()), None);
    assert_eq!(cut_insert::DEFAULT_SECONDS, 4.0);
    assert_eq!(
        cut_insert::default_length(produce_cards::offer_seconds(&endless.to_string_lossy()), true),
        4.0,
        "an endlessly repeating card takes the default insert length"
    );

    // And the encode: an endless card's frames are looped, a timed card's are not.
    let endless_card = produce_cards::arrive("assets/endless.svg", &endless_doc);
    assert_eq!(endless_card.length, None);
    // An endlessly repeating card has no strip to run out: it is drawn as ONE static picture and the encode loops
    // that picture for the slot — §8's `takes the default insert length, loops for its slot`.
    assert!(!endless_card.baked(), "no strip, one looped picture: {:?}", endless_card.logs);
    assert_eq!(endless_card.still.as_deref(), Some("endless-filled.svg"));
    let staged = stage_for(&endless, &endless_doc);
    assert!(staged.loops, "{staged:?}");
    assert_eq!(staged.inputs[0].0, "svg");
    let argv = encode(&staged, true);
    assert!(argv.contains(&"-stream_loop".to_string()) && argv.contains(&"-1".to_string()), "{argv:?}");
    // A looped still keeps the still's own rate: `-loop 1 -framerate 30` with `-stream_loop -1` in front of it.
    // The bake fps belongs to a baked strip, which is what the next check reads.
    assert!(argv.iter().any(|a| a == "-loop"), "a single picture is still looped as a still: {argv:?}");
    // A single static picture carries `-loop 1 -framerate 30` beside its `-stream_loop -1`: it IS a still, looped
    // for the slot (§8), so no "no -loop here" assertion belongs on this branch.

    let timed_staged = stage_for(&timed, &timed_doc);
    assert!(!timed_staged.loops, "{timed_staged:?}");
    let timed_argv = encode(&timed_staged, false);
    assert!(!timed_argv.contains(&"-stream_loop".to_string()), "{timed_argv:?}");

    // A still card writes ONE filled file and asks for it as a looped still, exactly as before this item.
    let still_staged = stage_for(&still, &still_doc);
    assert_eq!(still_staged.inputs[0].0, "svg");
    assert_eq!(still_staged.files[0].0.file_name().unwrap().to_string_lossy(), "still-filled.svg");
    assert!(!still_staged.files[0].1.contains("{{"));
    let still_argv = encode(&still_staged, false);
    assert!(still_argv.iter().any(|a| a == "-loop"), "a still is looped as it always was: {still_argv:?}");
}

// --- the render's own doors -----------------------------------------------------------------------------------------

/// Stage one card the way `produce_exec::plan` does: a clip whose source is this card, resolved to this path.
fn stage_for(path: &Path, _doc: &str) -> produce_cards::Staged {
    let clip = render::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 4.0,
        rate: 1.0,
        frame: (1920, 1080),
        src: (1920, 1080),
        source: path.to_string_lossy().to_string(),
        scratch: temp_root("scratch"),
        ..Default::default()
    };
    produce_cards::stage(&clip, Some(&path.to_string_lossy())).expect("an SVG card stages")
}

/// The encode command for a staged card, through the door `plan` uses.
fn encode(staged: &produce_cards::Staged, loops: bool) -> Vec<String> {
    let clip = render::Clip {
        no: 1,
        on_screen: 4.0,
        rate: 1.0,
        frame: (1920, 1080),
        ..Default::default()
    };
    let settings: Produce = Default::default();
    let inputs: Vec<(&str, Option<&str>)> = staged
        .inputs
        .iter()
        .map(|(kind, path)| (kind.as_str(), Some(path.as_str())))
        .collect();
    render::encode_command_with(&clip, &settings, &inputs, None, Path::new("out.mp4"), loops)
}

/// The whole plan leg: a card clip's §8 note lands in the run's log lines beside the clip's other news, and its
/// frames are on disk before the command that reads them is built.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s15b_the_plan_stages_the_card_before_it_encodes_it() {
    let doc = canvas(
        "<text x=\"100\" y=\"200\" opacity=\"0\">Board<animate attributeName=\"opacity\" values=\"0;1\"\
         keyTimes=\"0;1\" dur=\"2s\" fill=\"freeze\"/></text>",
    );
    let root = temp_root("plan");
    let project = root.join("demo.naivepost");
    std::fs::create_dir_all(&project).unwrap();
    let tree = Tree::new(&project).unwrap();
    let card_path = root.join("board.svg");
    std::fs::write(&card_path, &doc).unwrap();

    let cut = naivepost::cut::Cut {
        segs: vec![Seg { s: 0.0, e: 0.0, dur: 4.0, cam: 0, ins: card_path.to_string_lossy().to_string(), ..Default::default() }],
        ..Default::default()
    };
    // Named `fn`s rather than closures: `Materials` holds `&dyn Fn` over a higher-ranked lifetime, which only an
    // item-level signature infers (the same reason `tests/sec_06_effects_4_render.rs` writes them this way).
    fn always_exists(_: &str) -> bool {
        true
    }
    fn no_wav(_: &naivepost::narration::Entry) -> Option<PathBuf> {
        None
    }
    fn no_words(_: &render::Clip) -> Vec<naivepost::produce_subtitles::Word> {
        Vec::new()
    }
    fn fx_none(_seg: &Seg) -> Vec<naivepost::cut::Fx> {
        Vec::new()
    }
    let exists: &dyn Fn(&str) -> bool = &always_exists;
    let wav_of: &dyn Fn(&naivepost::narration::Entry) -> Option<PathBuf> = &no_wav;
    let words: &dyn Fn(&render::Clip) -> Vec<naivepost::produce_subtitles::Word> = &no_words;
    let fx_for: &dyn Fn(&Seg) -> Vec<naivepost::cut::Fx> = &fx_none;
    fn sources_none() -> Vec<naivepost::project::Source> {
        Vec::new()
    }
    let sources: &dyn Fn() -> Vec<naivepost::project::Source> = &sources_none;
    fn translate_pass(
        _text: &str,
        numbers: &[usize],
    ) -> Result<std::collections::BTreeMap<usize, String>, String> {
        Ok(numbers.iter().map(|n| (*n, format!("t {n}"))).collect())
    }
    let translate: &produce_translate::Ask = &translate_pass;
    let source_file = |_clip: &render::Clip| Some(card_path.to_string_lossy().to_string());
    let mats = exec::Materials {
        exists: &exists,
        fx_for: &fx_for,
        lanes: &[],
        src_shape: (1920, 1080),
        wav_of: &wav_of,
        source_file: &source_file,
        on_recording: true,
        footage_left: &naivepost::produce_render::unknown_footage,
        filmed_runs: naivepost::produce_plan_logs::NO_TAPE,
        base_of: &|_seg| String::new(),
        wav_seconds: &|_entry| None,
        cues: exec::Cues::default(),
        sources: &sources,
        words: &words,
        translate,
        // No ffprobe claim here: this file is about the bake, not the size check.
        probe: &|_file| None,
    };
    let run = exec_run(&cut, &tree);
    let plan = exec::plan(&run, &tree, &mats);

    // §8's note is in the plan's log lines, and it is the "filled in" kind: this card is hand-written, not stamped.
    assert!(
        plan.plan_logs.iter().any(|line| line == cards::FILLED_IN),
        "{:?}",
        plan.plan_logs
    );
    // The frames exist on disk before the encode command names them, under the scratch folder §3 lists.
    let frames = tree.clips_dir().join("board.frames");
    let count = std::fs::read_dir(&frames).map(|rd| rd.count()).unwrap_or(0);
    // The frame grid plus the finished frame the bake adds at the animation's own end.
    assert_eq!(count, cards::bake_times(2.0).len() + 1, "one static SVG per frame in {}", frames.display());
    let first = std::fs::read_to_string(frames.join("f00000.svg")).unwrap();
    assert!(!first.contains("<animate"), "a written frame is a finished picture: {first}");
    assert!(
        plan.encodes[0].command.argv.iter().any(|a| a.ends_with("f%05d.svg")),
        "{:?}",
        plan.encodes[0].command.argv
    );
    // §5 (`to frames at the render fps (default 25)`): ffmpeg has to be told the strip's own rate, and the number it
    // is told is `P.card.bakeFps`, spelled by `produce_cards` because the bake owns it.
    let argv = &plan.encodes[0].command.argv;
    let rate = argv.iter().position(|a| a == "-framerate").expect("the strip carries its rate");
    assert_eq!(argv[rate + 1], produce_cards::bake_fps_arg(), "{argv:?}");
    assert_eq!(argv[rate + 1], "25", "P.card.bakeFps: {argv:?}");
    // §5's `-t` goes on the OUTPUT, so after every input — including the one naming this strip. A `-t` before `-i`
    // would trim what the filters were built from.
    let limit = argv.iter().position(|a| a == "-t").expect("the insert is trimmed to its slot");
    assert_eq!(argv[limit + 1], format!("{:.3}", 4.0), "{argv:?}");
    assert!(
        limit > argv.iter().position(|a| a.ends_with("f%05d.svg")).unwrap(),
        "the limit follows the input it trims: {argv:?}"
    );
}

/// §8 (`An indefinitely repeating document has no own length: takes the default insert length, loops for its slot`),
/// as far as the RUN is concerned. The form half is s15's; this is the encode half, driven through the same
/// [`exec::plan`] door s15b uses, because `-stream_loop -1` and the `-t` that makes looping safe are decided there
/// and nowhere else.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s15c_the_run_loops_a_card_with_no_own_length() {
    let doc = canvas(
        "<text x=\"100\" y=\"200\" opacity=\"0\">Spin<animate attributeName=\"opacity\" values=\"0;1\"\
         keyTimes=\"0;1\" dur=\"2s\" repeatCount=\"indefinite\"/></text>",
    );
    let root = temp_root("plan-loop");
    let project = root.join("demo.naivepost");
    std::fs::create_dir_all(&project).unwrap();
    let tree = Tree::new(&project).unwrap();
    let card_path = root.join("spin.svg");
    std::fs::write(&card_path, &doc).unwrap();
    // The slot the Cut page gave it: nothing of its own to offer, so P.policy.insertDefaultSeconds.
    assert_eq!(produce_cards::offer_seconds(&card_path.to_string_lossy()), None);
    let seconds = cut_insert::default_length(produce_cards::offer_seconds(&card_path.to_string_lossy()), true);
    assert_eq!(seconds, cut_insert::DEFAULT_SECONDS);

    let cut = naivepost::cut::Cut {
        segs: vec![Seg { s: 0.0, e: 0.0, dur: seconds, cam: 0, ins: card_path.to_string_lossy().to_string(), ..Default::default() }],
        ..Default::default()
    };
    fn always_exists(_: &str) -> bool {
        true
    }
    fn no_wav(_: &naivepost::narration::Entry) -> Option<PathBuf> {
        None
    }
    fn no_words(_: &render::Clip) -> Vec<naivepost::produce_subtitles::Word> {
        Vec::new()
    }
    fn fx_none(_seg: &Seg) -> Vec<naivepost::cut::Fx> {
        Vec::new()
    }
    let exists: &dyn Fn(&str) -> bool = &always_exists;
    let wav_of: &dyn Fn(&naivepost::narration::Entry) -> Option<PathBuf> = &no_wav;
    let words: &dyn Fn(&render::Clip) -> Vec<naivepost::produce_subtitles::Word> = &no_words;
    let fx_for: &dyn Fn(&Seg) -> Vec<naivepost::cut::Fx> = &fx_none;
    fn sources_none() -> Vec<naivepost::project::Source> {
        Vec::new()
    }
    let sources: &dyn Fn() -> Vec<naivepost::project::Source> = &sources_none;
    fn translate_pass(
        _text: &str,
        numbers: &[usize],
    ) -> Result<std::collections::BTreeMap<usize, String>, String> {
        Ok(numbers.iter().map(|n| (*n, format!("t {n}"))).collect())
    }
    let translate: &produce_translate::Ask = &translate_pass;
    let source_file = |_clip: &render::Clip| Some(card_path.to_string_lossy().to_string());
    let mats = exec::Materials {
        exists: &exists,
        fx_for: &fx_for,
        lanes: &[],
        src_shape: (1920, 1080),
        wav_of: &wav_of,
        source_file: &source_file,
        on_recording: true,
        footage_left: &naivepost::produce_render::unknown_footage,
        filmed_runs: naivepost::produce_plan_logs::NO_TAPE,
        base_of: &|_seg| String::new(),
        wav_seconds: &|_entry| None,
        cues: exec::Cues::default(),
        sources: &sources,
        words: &words,
        translate,
        probe: &|_file| None,
    };
    let plan = exec::plan(&exec_run(&cut, &tree), &tree, &mats);

    // The card still says how it was arrived at, even though it is the endless kind.
    assert!(
        plan.plan_logs.iter().any(|line| line == cards::FILLED_IN),
        "{:?}",
        plan.plan_logs
    );
    let argv = &plan.encodes[0].command.argv;
    // Its slot is longer than one pass of it, so the picture runs again: `-stream_loop -1`.
    let loops = argv.iter().position(|a| a == "-stream_loop").expect("an endless card loops");
    assert_eq!(argv[loops + 1], "-1", "{argv:?}");
    // And looping is only safe because `-t` trims the loop back to the seconds the cut asked for.
    let limit = argv.iter().position(|a| a == "-t").expect("the loop is trimmed to the slot");
    assert_eq!(argv[limit + 1], format!("{:.3}", seconds), "{argv:?}");
}

/// A run carrying this cut, with the settings a render would use.
fn exec_run(cut: &naivepost::cut::Cut, tree: &Tree) -> naivepost::produce_flow::Run {
    naivepost::produce_flow::snapshot(
        Some(cut.clone()),
        tree,
        Vec::new(),
        Produce::default(),
        &naivepost::project::Project::default(),
        None,
        "1".to_string(),
    )
}
