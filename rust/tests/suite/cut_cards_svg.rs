//! §06-effects#5-cards-svg-inserts — checked against [`naivepost::cut_cards`].
//!
//! The spec section is one prose paragraph with no numbered steps, so its rules are read as S1..S7 (path arguments,
//! declared inputs, holes, the two shipped cards, the seeds, "the static file is the finished card", the bake and the
//! card's length) — and S7 is split in two tests so the SMIL/CSS reader and the length rule can fail apart.
//!
//! Nothing here renders a frame or runs ffmpeg: the module returns parsed data and strings, which is what lets a card
//! rule be pinned without a rasteriser. Every time and size below is an exact binary number, and lerped values are
//! compared with a tolerance rather than as formatted text.

use naivepost::cut_cards as cards;
use naivepost::cut_insert;
use naivepost::params;

// --- S1: the path's arguments ---------------------------------------------------------------------------------------

/// S1 (`An insert path may carry ?key=value&… (order matters; escapes % & = ? only)`): the order is the card's own —
/// it is what lists its inputs first — so a form built from these pairs must not sort them.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s1_a_path_carries_its_arguments_in_order() {
    let path = cards::split_path("project:assets/tier.svg?S=Dust II&A=Ace");
    assert_eq!(path.file, "project:assets/tier.svg");
    assert_eq!(
        path.args,
        vec![
            ("S".to_string(), "Dust II".to_string()),
            ("A".to_string(), "Ace".to_string())
        ]
    );

    // A key with no `=` is an argument with nothing in it — the card's fallback says what it means.
    let bare = cards::split_path("card.svg?keep");
    assert_eq!(bare.args, vec![("keep".to_string(), String::new())]);

    // The escape set is exactly those four characters; everything else a title can contain is written as it stands.
    assert_eq!(cards::esc("100% & = ?"), "100%25 %26 %3D %3F");
    assert_eq!(cards::unesc("100%25 %26 %3D %3F"), "100% & = ?");
    assert_eq!(cards::esc("a,b: c/d"), "a,b: c/d");

    // The reason the set exists: a value holding all four survives a write and a read unchanged.
    let tricky = cards::Path {
        file: "project:assets/badge.svg".into(),
        args: vec![("title".into(), "100% & ? = done".into())],
    };
    let written = cards::join_path(&tricky);
    assert_eq!(cards::split_path(&written), tricky, "{written}");
}

// --- S2: declared inputs --------------------------------------------------------------------------------------------

/// S2 (`Documents declare inputs as <!-- Input: key[flags] | Label | hint --> (flags keep, logo)`): the form's field
/// order is the declaration order, so a card that asks for its logo first gets a logo field first.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s2_a_document_declares_its_inputs() {
    let doc = "<svg>\
        <!-- Input: title | Title | big and short -->\
        <!-- Input: logo[logo] | Logo -->\
        <!-- Input: board[keep,logo] | Board -->\
        <!-- Input: note[nonsense] -->\
        </svg>";
    let found = cards::inputs(doc);
    assert_eq!(found.len(), 4);

    assert_eq!(found[0].key, "title");
    assert_eq!(found[0].label, "Title");
    assert_eq!(found[0].hint, "big and short");
    assert!(!found[0].keep && !found[0].logo);

    assert_eq!(found[1].key, "logo");
    assert!(found[1].logo, "the flag is what makes it a file chooser");
    assert_eq!(found[1].hint, "", "a declaration may stop after the label");

    assert!(found[2].keep && found[2].logo, "both flags combine");

    // An unknown flag is ignored rather than fatal: a card written for a newer build still fills in.
    assert_eq!(found[3].key, "note");
    assert!(!found[3].keep && !found[3].logo);
    assert_eq!(found[3].label, "");

    let names: Vec<&str> = found.iter().map(|i| i.key.as_str()).collect();
    assert_eq!(names, ["title", "logo", "board", "note"]);
}

// --- S3: holes ------------------------------------------------------------------------------------------------------

/// S3 (`holes as {{name}} / {{name|fallback}} (outside comments)`): a card explains its own holes in a comment, and
/// filling that in would erase the explanation for whoever writes the next card.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s3_holes_are_filled_or_left_outside_comments() {
    let doc = "<svg><!-- {{hidden}} --><text>{{title}}</text><text>{{name|Anonymous}}</text></svg>";

    let names: Vec<String> = cards::holes(doc).into_iter().map(|h| h.name).collect();
    assert_eq!(names, ["title", "name"], "the comment is documentation");

    let scope = [("title".to_string(), "Dust II".to_string())];
    let filled = cards::fill(doc, &scope);
    assert!(filled.contains(">Dust II<"), "{filled}");
    assert!(filled.contains(">Anonymous<"), "the fallback speaks when the scope is silent");
    // The comment survives byte for byte, hole and all.
    assert!(filled.contains("<!-- {{hidden}} -->"), "{filled}");

    // A hole with neither answer writes nothing: a card without a caption stays uncaptioned rather than printing its
    // own hole name on the picture.
    let bare = cards::fill("<text>{{caption}}</text>", &[]);
    assert_eq!(bare, "<text></text>");

    // A value with an ampersand is text, not markup — escaped on the way in so the file still parses.
    let escaped = cards::fill(
        "<text>{{title}}</text>",
        &[("title".to_string(), "Tom & Jerry <3".into())],
    );
    assert!(escaped.contains("Tom &amp; Jerry &lt;3"), "{escaped}");
    // The hole is gone entirely, so nothing unescaped of the value is left to trip a parser.
    assert!(!escaped.contains("{{"));
}

// --- S4: the two shipped cards ---------------------------------------------------------------------------------------

/// S4 (`tier` — a board of rows S A B C D F with items "Name|logo.png" and a `new` list of arrivals with per-item
/// timing; `badge` — one big letter with a caption).
#[test]
fn sec_06_effects_5_cards_svg_inserts_s4_the_two_shipped_cards_have_their_shape() {
    assert_eq!(cards::TIER_ROWS, ["S", "A", "B", "C", "D", "F"]);

    let items = cards::parse_items("Dust|dust.png, Ace");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].logo.as_deref(), Some("dust.png"));
    assert_eq!(items[1].logo, None, "a bare name has no logo");
    assert_eq!(cards::join_items(&items), "Dust|dust.png, Ace");
    assert!(cards::parse_items("").is_empty());

    let arrivals = cards::parse_arrivals("A[1.2s]: a.svg, B: b.svg");
    assert_eq!(arrivals.len(), 2);
    assert_eq!(arrivals[0].delay, Some(1.2));
    assert_eq!(arrivals[0].logo.as_deref(), Some("a.svg"));
    assert_eq!(arrivals[1].delay, None, "no timing means the board decides");
    assert_eq!(arrivals[1].logo.as_deref(), Some("b.svg"));

    // A delay that cannot be read is no delay — the arrival still belongs on the board.
    let mistyped = cards::parse_arrivals("C[soon]: c.svg");
    assert_eq!(mistyped.len(), 1);
    assert_eq!(mistyped[0].label, "C");
    assert_eq!(mistyped[0].delay, None);

    for doc in [cards::badge_svg("S", "top"), cards::tier_svg(&items, &arrivals)] {
        assert!(doc.contains("width=\"1920\"") && doc.contains("height=\"1080\""), "{doc}");
        assert!(doc.contains(cards::BACKGROUND), "a dark background: {doc}");
        assert!(!doc.contains("{{"), "a shipped card asks for nothing: {doc}");
    }
    // And each names itself, which is how an insert knows it was drawn by a card.
    assert!(cards::badge_svg("S", "t").contains(&format!("data-naivepost=\"{}\"", cards::BADGE)));
    assert!(cards::tier_svg(&[], &[]).contains(&format!("data-naivepost=\"{}\"", cards::TIER)));
}

// --- S5: the seeds ---------------------------------------------------------------------------------------------------

/// S5 (`seeds s a b c d f .svg, tier.svg and CARDS.md written into assets/ on first use, never overwriting`): badges
/// first, because §8 records that the board embeds them as logos and is generated from them.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s5_the_seeds_are_badges_then_the_board_then_the_guide() {
    let seeds = cards::seeds();
    let names: Vec<&str> = seeds.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        [
            "s.svg",
            "a.svg",
            "b.svg",
            "c.svg",
            "d.svg",
            "f.svg",
            "tier.svg",
            cut_insert::CARD_GUIDE_FILE
        ]
    );
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "no file is written twice");
    assert_eq!(names[7], cut_insert::CARD_GUIDE_FILE);
    assert!(!seeds[7].1.is_empty(), "the guide has to say something");

    // "Never overwriting" is the existing writer's rule, so it is proved through it rather than restated.
    let dir = std::env::temp_dir().join(format!("naivepost-cards-{}", std::process::id()));
    let theirs = "mine, not the seed\n";
    std::fs::create_dir_all(&dir).expect("a temp folder");
    std::fs::write(dir.join("s.svg"), theirs).expect("a file to keep");

    let first = cut_insert::seed_cards(&dir, &seeds);
    assert_eq!(first.len(), seeds.len() - 1, "the badge already there is not written: {first:?}");
    assert!(!first.contains(&"s.svg".to_string()));

    let second = cut_insert::seed_cards(&dir, &seeds);
    assert!(second.is_empty(), "a second open writes nothing: {second:?}");
    assert_eq!(
        std::fs::read_to_string(dir.join("s.svg")).expect("read it back"),
        theirs,
        "a card someone restyled stays theirs"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// --- S6: the static file is the finished card --------------------------------------------------------------------------

/// S6 (`Canvas 1920×1080, dark background, 2.2 s stillness at the end. Every animation starts at 0 and waits inside
/// keyTimes, so the static file is the finished card`): an animation whose first keyTime is not 0 shows a half-drawn
/// picture to anyone who opens the file.
#[test]
fn sec_06_effects_5_cards_svg_inserts_s6_the_static_file_is_the_finished_card() {
    assert_eq!(cards::CANVAS, (1920, 1080));
    assert!(cards::is_frame_size((1920, 1080)));
    assert!(!cards::is_frame_size((1280, 720)));
    assert_eq!(cards::STILLNESS_SECONDS, 2.2); // card.stillnessSeconds

    let row = params::cut()
        .into_iter()
        .find(|row| row.id == "card.stillnessSeconds")
        .expect("catalogued");
    assert_eq!(row.spelled, "2.2");

    assert!(cards::starts_at_zero_and_waits(&[0.0, 0.4, 1.0]));
    // Nothing waits at the start, so the static file would be caught mid-move.
    assert!(!cards::starts_at_zero_and_waits(&[0.25, 1.0]));
    // Not ascending: keyTimes are a clock, and this one runs backwards.
    assert!(!cards::starts_at_zero_and_waits(&[0.0, 1.0, 0.5]));
    // Never reaches the end, so the finished state is never the settled one.
    assert!(!cards::starts_at_zero_and_waits(&[0.0, 0.5]));
    assert!(!cards::starts_at_zero_and_waits(&[]));

    // And what the generators emit obeys it: every keyTimes list in both shipped cards waits at 0 and reaches 1.
    let badge = cards::badge_svg("S", "top");
    let board = cards::tier_svg(&[], &[]);
    for doc in [badge.as_str(), board.as_str()] {
        let mut rest = doc;
        let mut found = 0;
        while let Some(at) = rest.find("keyTimes=\"") {
            let list = &rest[at + "keyTimes=\"".len()..];
            let list = &list[..list.find('"').expect("a closing quote")];
            let times: Vec<f64> = list.split(';').filter_map(|v| v.parse().ok()).collect();
            assert!(cards::starts_at_zero_and_waits(&times), "{list} in a shipped card");
            found += 1;
            rest = &list[list.len()..];
        }
        // The badge animates its letter in; a board with no arrivals is already the finished picture.
        assert_eq!(found, usize::from(doc == badge));
    }
}

// --- S7a: the bake reads SMIL and CSS ---------------------------------------------------------------------------------

/// S7 (`Render bakes SMIL (animate/set/animateTransform on numbers, lengths, colours; values/keyTimes; repeat; freeze;
/// additive) and a CSS subset (opacity, transform, fill; simple selectors; animation longhands; standard easings)`).
#[test]
fn sec_06_effects_5_cards_svg_inserts_s7_the_bake_reads_smil_and_css() {
    let anim = cards::parse_anim(
        "<animate attributeName=\"x\" values=\"0;10\" keyTimes=\"0;1\" dur=\"2s\" begin=\"1s\"/>",
    )
    .expect("an animate tag is an animation");

    // Before it begins there is nothing to draw.
    assert_eq!(cards::value_at(&anim, 0.5), None);
    // Half way through the run: half of the way from 0 to 10.
    let half = cards::value_at(&anim, 2.0).expect("inside the run");
    assert!((half.parse::<f64>().unwrap() - 5.0).abs() < 1e-6, "{half}");
    // Past the end with no `fill="freeze"`: the attribute lets go and returns to the file's own value.
    assert_eq!(cards::value_at(&anim, 3.5), None);

    let held = cards::parse_anim(
        "<animate attributeName=\"x\" values=\"0;10\" keyTimes=\"0;1\" dur=\"2s\" fill=\"freeze\"/>",
    )
    .expect("an animate tag");
    assert_eq!(cards::value_at(&held, 4.0).as_deref(), Some("10"));

    // A repeat wraps the clock: past the first run it reads the second, from its start again.
    let twice = cards::parse_anim(
        "<animate attributeName=\"x\" values=\"0;10\" keyTimes=\"0;1\" dur=\"2s\" repeatCount=\"2\"/>",
    )
    .expect("an animate tag");
    let wrapped = cards::value_at(&twice, 2.5).expect("the second run");
    assert!((wrapped.parse::<f64>().unwrap() - 2.5).abs() < 1e-6, "{wrapped}");
    // Two runs and then it is over.
    assert_eq!(cards::value_at(&twice, 4.5), None);

    // `indefinite` never ends, so there is no frame at which the animation has finished.
    let forever = cards::parse_anim(
        "<animate attributeName=\"x\" values=\"0;10\" keyTimes=\"0;1\" dur=\"2s\" repeatCount=\"indefinite\"/>",
    )
    .expect("an animate tag");
    assert!(cards::value_at(&forever, 1000.0).is_some());

    // `additive="sum"` is read and carried; for a transform that is how SMIL composes two of them, which is why the
    // bake adds numbers rather than replacing them.
    let sum = cards::parse_anim(
        "<animateTransform attributeName=\"transform\" type=\"translate\" values=\"0;10\" keyTimes=\"0;1\" dur=\"2s\" additive=\"sum\"/>",
    )
    .expect("a transform animates too");
    assert!(sum.additive && sum.attr == "transform");

    // `set` has one value and no ramp: it holds from its begin, and only for as long as the file says — with no
    // `dur` and no `fill="freeze"` there is nothing to hold past.
    let set = cards::parse_anim("<set attributeName=\"visibility\" to=\"visible\" begin=\"1s\"/>")
        .expect("a set is an animation");
    assert_eq!(set.values, vec!["visible".to_string()]);
    assert_eq!(cards::value_at(&set, 2.0), Some("visible".to_string()));
    assert_eq!(cards::value_at(&set, 0.5), None, "before it begins");

    // Anything else in the file is not an animation.
    assert!(cards::parse_anim("<rect width=\"10\" height=\"10\"/>").is_none());

    // The three kinds of value §5 lists — and nothing else is interpolated.
    assert_eq!(cards::parse_value("12"), Some(cards::Value::Number(12.0)));
    assert_eq!(
        cards::parse_value("48px"),
        Some(cards::Value::Length(48.0, "px".into()))
    );
    assert_eq!(
        cards::parse_value("50%"),
        Some(cards::Value::Length(50.0, "%".into()))
    );
    assert_eq!(
        cards::parse_value("#0a0a0a"),
        Some(cards::Value::Colour([10, 10, 10]))
    );
    // A three-digit hex is the same colour doubled.
    assert_eq!(
        cards::parse_value("#fff"),
        Some(cards::Value::Colour([255, 255, 255]))
    );
    assert_eq!(cards::parse_value("translate(4)"), None);

    // CSS: the three properties plus the animation longhands, and nothing else.
    let ok = "opacity: 0; transform: translateX(4px); fill: #fff; animation: 2s ease-in 0.5s 1 forwards fade";
    assert!(cards::css_supported(ok), "{ok}");
    assert_eq!(cards::css_declares("opacity: 0; fill: red").len(), 2);
    // Outside the subset the bake refuses rather than approximating — a card that renders differently from the file
    // someone opened to check it is worse than one that says it cannot be baked.
    assert!(!cards::css_supported("stroke-width: 3"));
    assert!(!cards::css_supported("filter: blur(2px)"));

    assert!(cards::selector_simple("rect"));
    assert!(cards::selector_simple(".row"));
    assert!(cards::selector_simple("#board, .row"));
    assert!(!cards::selector_simple(".a > .b"));
    assert!(!cards::selector_simple(".a .b"));
    assert!(!cards::selector_simple(".a:hover"));

    for name in ["linear", "ease", "ease-in", "ease-out", "ease-in-out"] {
        let (x1, y1, x2, y2) = cards::easing(name).expect("a standard easing");
        assert!((0.0..=1.0).contains(&x1) && (0.0..=1.0).contains(&x2), "{name}");
        assert!((0.0..=1.0).contains(&y1) && (0.0..=1.0).contains(&y2), "{name}");
    }
    // A stepped timing is not a standard easing and the bake has no curve to draw it with.
    assert_eq!(cards::easing("steps(4)"), None);
}

// --- S7b: the card's length -------------------------------------------------------------------------------------------

/// S7 (`to frames at the render fps (default 25). Card length = last moving moment, offered as the insert's length`).
#[test]
fn sec_06_effects_5_cards_svg_inserts_s8_the_card_is_as_long_as_it_moves() {
    assert_eq!(cards::BAKE_FPS, 25.0); // card.bakeFps
    // The bake reuses the tree's one definition of "frames at an fps" rather than writing its own division.
    assert_eq!(
        cards::bake_times(2.0),
        cut_insert::frame_times(2.0, 25.0),
        "{}",
        cards::bake_times(2.0).len()
    );
    assert!(cards::bake_times(0.0).is_empty());

    let anim = |begin: f64, dur: f64, repeat: f64| cards::Anim {
        attr: "opacity".into(),
        values: vec!["0".into(), "1".into()],
        key_times: vec![0.0, 1.0],
        begin,
        dur,
        repeat,
        freeze: true,
        additive: false,
    };

    // The LAST moving moment, not the sum: two animations that overlap end once, and the later one decides.
    assert_eq!(cards::card_length(&[anim(0.0, 2.0, 1.0), anim(1.0, 4.0, 1.0)]), Some(5.0));
    // A repeat runs the clock out: begin + dur × repeat.
    assert_eq!(cards::card_length(&[anim(0.5, 2.0, 2.0)]), Some(4.5));

    // An animation that repeats forever has no own length, so the insert takes the default instead of hanging on a
    // number nobody chose.
    let endless = cards::card_length(&[anim(0.0, 2.0, f64::INFINITY)]);
    assert_eq!(endless, None);
    assert!(endless.is_none());
    assert_eq!(cut_insert::DEFAULT_SECONDS, 4.0);

    // Nothing moves: also no length of its own, for the same reason.
    assert_eq!(cards::card_length(&[]), None);
}
