//! §06-effects#5-cards-svg-inserts — cards: SVG files that carry their own picture and ask for their words.
//!
//! Spec: `spec/06-effects.md` §5. It is one prose paragraph rather than numbered steps, so its rules are read here
//! as S1..S7: S1 the path's arguments, S2 what a document declares as inputs, S3 its holes, S4 the two shipped cards,
//! S5 the seeds written on first use, S6 "the static file is the finished card", S7 the bake and the card's length.
//!
//! # What lives here and what does not
//!
//! The drawing is in the file; this module holds the arithmetic around it — reading a path's arguments, filling holes,
//! deciding how long a card runs, sampling its animation at the render's frame rate. Seeding goes through
//! [`crate::cut_insert::seed_cards`], which already refuses to overwrite; nothing here touches a window (Cut is not
//! drawn yet) and nothing here runs a rasteriser.

use crate::cut_insert;

// --- S1: the path may carry arguments ------------------------------------------------------------------------------

/// S1 (`escapes % & = ? only`): the four characters that would otherwise be read as structure — `%` because it opens
/// an escape, `&` because it separates arguments, `=` because it divides a key from its value, `?` because it opens
/// the query. Spaces, commas and colons are written as they are: a title is allowed to look like a title.
pub fn esc(value: &str) -> String {
    // `%` first and alone in effect: escaping it after the others would rewrite the `%25` the earlier passes just
    // produced and turn one round trip into two.
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '%' => out.push_str("%25"),
            '&' => out.push_str("%26"),
            '=' => out.push_str("%3D"),
            '?' => out.push_str("%3F"),
            other => out.push(other),
        }
    }
    out
}

/// The inverse of [`esc`]. Applied per value, never to the whole query: a `%25` in the file is one percent sign, and
/// decoding before splitting would let a value smuggle in an extra argument.
pub fn unesc(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let rest = value;
    let mut i = 0usize;
    while i < rest.len() {
        let tail = &rest[i..];
        if tail.starts_with('%') {
            if let Some(hex) = tail.get(1..3) {
                if let Some(byte) = u8::from_str_radix(hex, 16).ok() {
                    if let Some(c) = char::from_u32(byte as u32) {
                        out.push(c);
                        i += 3;
                        continue;
                    }
                }
            }
            // A `%` that opens nothing is a percent sign: keep the file's bytes rather than dropping them.
        }
        let step = tail[1..]
            .find('%')
            .map(|at| at + 1)
            .unwrap_or(tail.len());
        out.push_str(&tail[..step]);
        i += step;
    }
    out
}

// An insert path and the arguments it carries. The pairs stay in a `Vec` because S1 says so outright — *order
/// matters* — and a form built from them lists its fields in the order the card asked for them, not alphabetically.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    pub file: String,
    pub args: Vec<(String, String)>,
}

/// S1: split `file.svg?key=value&…`. The first `?` is the only separator (a value may hold one once escaped); a bare
/// `key` carries an empty value; duplicate keys stay in order, since the last one is not automatically the one meant.
pub fn split_path(raw: &str) -> Path {
    let (file, query) = match raw.split_once('?') {
        Some((f, q)) => (f, Some(q)),
        None => (raw, None),
    };
    let args = query
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (unesc(key), unesc(value))
        })
        .collect();
    Path {
        file: file.to_string(),
        args,
    }
}

/// S1: put a path back together, escaping what would be read as structure. Round-trips exactly with [`split_path`].
pub fn join_path(path: &Path) -> String {
    if path.args.is_empty() {
        return path.file.clone();
    }
    let query: Vec<String> = path
        .args
        .iter()
        .map(|(key, value)| format!("{}={}", esc(key), esc(value)))
        .collect();
    format!("{}?{}", path.file, query.join("&"))
}

// --- S2: what a document declares ----------------------------------------------------------------------------------

/// One declared input. `keep` means the value stays with the insert rather than being re-asked each time; `logo`
/// means the value names an image file — both change the widget the form builds, which is why they are in the file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Input {
    pub key: String,
    pub keep: bool,
    pub logo: bool,
    pub label: String,
    pub hint: String,
}

/// S2 (`Documents declare inputs as <!-- Input: key[flags] | Label | hint -->`) in document order.
pub fn inputs(doc: &str) -> Vec<Input> {
    let mut out = Vec::new();
    for comment in comments(doc) {
        let Some(body) = comment.trim().strip_prefix("Input:") else {
            continue;
        };
        let mut parts = body.split('|').map(str::trim);
        let Some(key_part) = parts.next() else {
            continue;
        };
        let (key, flags) = match key_part.split_once('[') {
            Some((k, rest)) => (k.trim(), rest.trim_end_matches(']').trim()),
            None => (key_part.trim(), ""),
        };
        if key.is_empty() {
            continue;
        }
        let mut input = Input {
            // Lower-cased on the way in because the scope a card is filled from is keyed lower-case: `{{Name}}` and
            // `{{name}}` are the same hole, and an input declared `Name` must reach it.
            key: key.to_lowercase(),
            label: parts.next().unwrap_or_default().to_string(),
            hint: parts.next().unwrap_or_default().to_string(),
            ..Default::default()
        };
        for flag in flags.split([' ', ',']).map(str::trim).filter(|f| !f.is_empty()) {
            match flag {
                "keep" => input.keep = true,
                "logo" => input.logo = true,
                // An unknown flag is ignored rather than fatal: a card written for a newer build still fills in.
                _ => {}
            }
        }
        out.push(input);
    }
    out
}

/// Every `<!-- … -->` body, in order, without its delimiters. Comments are the only place a document talks about
/// itself, and they are also where a hole can hide as documentation — so both readers need the same split.
fn comments(doc: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = doc;
    while let Some(start) = rest.find("<!--") {
        let after = &rest[start + 4..];
        let Some(end) = after.find("-->") else { break };
        out.push(after[..end].to_string());
        rest = &after[end + 3..];
    }
    out
}

/// The three characters that change what a document means. Quotes are left alone: holes sit in text content and in
/// already-quoted attributes this module never builds by concatenation.
fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// S3: write the scope into the document. A value is XML-escaped on the way in — a title with an ampersand is a text
/// node, not markup, and a card that stops parsing loses its picture along with its words. No answer and no fallback
/// writes nothing: a card without a caption stays uncaptioned rather than printing its own hole name on the video.
pub fn fill(doc: &str, scope: &[(String, String)]) -> String {
    let mut out = String::with_capacity(doc.len());
    let mut pos = 0usize;
    for (hole, (start, end)) in find_holes(doc) {
        out.push_str(&doc[pos..start]);
        let answer = scope
            .iter()
            .find(|(key, _)| *key == hole.name)
            .map(|(_, value)| value.as_str())
            .filter(|value| !value.is_empty())
            .or(hole.fallback.as_deref());
        if let Some(value) = answer {
            out.push_str(&xml_escape(value));
        }
        pos = end;
    }
    out.push_str(&doc[pos..]);
    out
}

/// The document with each `<!-- … -->` blanked to spaces **byte for byte**, so every offset in the result matches the
/// original. Blanking rather than deleting is what lets [`fill`] write into the untouched text at these offsets; a
/// hole that only a comment mentioned has no braces left here, which is how §5's "(outside comments)" is enforced.
fn blank_comments(doc: &str) -> String {
    let bytes = doc.as_bytes();
    let mut out = bytes.to_vec();
    let text = std::str::from_utf8(bytes).unwrap_or(doc);
    let mut from = 0usize;
    while let Some(rel) = text[from..].find("<!--") {
        let start = from + rel;
        let after = start + 4;
        let Some(rel_end) = text[after..].find("-->") else { break };
        let end = after + rel_end + 3;
        for byte in out.iter_mut().take(end).skip(start) {
            *byte = b' ';
        }
        from = end;
    }
    // Blanking ASCII bytes cannot split a UTF-8 sequence: every replaced byte was part of `<!--`/`-->`, and the bytes
    // in between are copied unchanged.
    String::from_utf8(out).expect("blanking comments keeps valid UTF-8")
}

// --- S3: holes -----------------------------------------------------------------------------------------------------

/// One hole and what it says when nobody answers it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hole {
    pub name: String,
    pub fallback: Option<String>,
}

/// S3 (`holes as {{name}} / {{name|fallback}} (outside comments)`).
pub fn holes(doc: &str) -> Vec<Hole> {
    find_holes(doc)
        .into_iter()
        .map(|(hole, _)| hole)
        .collect()
}

/// The holes with the byte range each one occupies — what [`fill`] writes over. A hole inside `<!-- … -->` is skipped:
/// a card explains its own holes in a comment, and replacing that would erase the explanation for whoever writes the
/// next card.
fn find_holes(doc: &str) -> Vec<(Hole, (usize, usize))> {
    let blanked = blank_comments(doc);
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = blanked[from..].find("{{") {
        let start = from + rel;
        let Some(rel_end) = blanked[start..].find("}}") else { break };
        let end = start + rel_end + 2;
        // Read the name out of the ORIGINAL text; the blanked copy only says where braces really are.
        let body = doc[start + 2..end - 2].trim();
        from = end;
        if body.is_empty() || body.contains('{') || body.contains('\n') {
            continue;
        }
        let (name, fallback) = match body.split_once('|') {
            // Lower-cased: `{{Name}}` and `{{name}}` are the same hole, and a scope keyed lower-case must reach both.
            Some((n, f)) => (n.trim().to_lowercase(), Some(f.to_string())),
            None => (body.to_lowercase(), None),
        };
        out.push((Hole { name, fallback }, (start, end)));
    }
    out
}

// --- the canvas, and the two shipped cards -------------------------------------------------------------------------

/// S6 (`Canvas 1920×1080`): every card is drawn at the render's landscape frame size. A card that came out smaller
/// would be scaled by the encoder and lose the crispness that makes a title readable on a phone.
pub const CANVAS: (i32, i32) = (1920, 1080);

/// S6 (`dark background`): §5 says only "dark", so the value is ours — near-black with a hair of blue, which keeps
/// white text at maximum contrast without the flat dead look of pure black on a bright display.
pub const BACKGROUND: &str = "#0d1117";

/// S6 (`2.2 s stillness at the end`): `// card.stillnessSeconds`. The generator leaves this much time with nothing
/// moving, so the last thing a viewer sees is the finished board rather than an animation's tail.
pub const STILLNESS_SECONDS: f64 = 2.2;

/// S4: the two cards that ship with the app, named as `data-naivepost` spells them.
pub const BADGE: &str = "badge";
pub const TIER: &str = "tier";

/// S4 (`a board of rows S A B C D F`) in the order the board lists them — best to worst, which is also the order the
/// tiers are read aloud in.
pub const TIER_ROWS: [&str; 6] = ["S", "A", "B", "C", "D", "F"];

/// S6: is this the size a card is drawn at?
pub fn is_frame_size(size: (i32, i32)) -> bool {
    size == CANVAS
}

/// S6 (`Every animation starts at 0 and waits inside keyTimes, so the static file is the finished card`): the first
/// keyTime must be 0 — the animation is already at its finished state when nothing has run yet — the last must reach
/// 1, and they must ascend. A card whose animation starts part-way through shows a half-drawn picture to anyone who
/// opens the file, which is exactly what this rule exists to prevent.
pub fn starts_at_zero_and_waits(key_times: &[f64]) -> bool {
    let (Some(first), Some(last)) = (key_times.first(), key_times.last()) else {
        return false;
    };
    if key_times.len() < 2 || *first != 0.0 || *last != 1.0 {
        return false;
    }
    key_times.windows(2).all(|pair| pair[0] < pair[1])
}

/// One row's entry: `Name|logo.png`, and the logo is optional because most rows have nothing to show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Item {
    pub name: String,
    pub logo: Option<String>,
}

/// S4 (`items "Name|logo.png"`): a comma-separated list; a bare `Name` has no logo and an empty list has no items.
pub fn parse_items(list: &str) -> Vec<Item> {
    list.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| match entry.split_once('|') {
            Some((name, logo)) => Item {
                name: name.trim().to_string(),
                logo: Some(logo.trim().to_string()),
            },
            None => Item {
                name: entry.to_string(),
                logo: None,
            },
        })
        .collect()
}

/// The inverse of [`parse_items`], exact enough that a form's Save cannot reformat someone's list.
pub fn join_items(items: &[Item]) -> String {
    items
        .iter()
        .map(|item| match &item.logo {
            Some(logo) => format!("{}|{}", item.name, logo),
            None => item.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// One arrival on the board's `new` list, with its own timing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Arrival {
    pub label: String,
    /// When it lands, in card seconds; `None` means the board decides.
    pub delay: Option<f64>,
    pub logo: Option<String>,
}

/// S4 (`a new list of arrivals with per-item timing`): `A[1.2s]: a.svg, B: b.svg`. A timing that cannot be read is
/// dropped rather than rejecting the item — an arrival whose delay was typed wrong still belongs on the board.
pub fn parse_arrivals(list: &str) -> Vec<Arrival> {
    list.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (head, logo) = match entry.split_once(':') {
                Some((h, l)) => (h.trim(), Some(l.trim().to_string())),
                None => (entry.trim(), None),
            };
            let (label, delay) = match head.split_once('[') {
                Some((l, rest)) => (
                    l.trim().to_string(),
                    parse_delay(rest.trim_end_matches(']')),
                ),
                None => (head.to_string(), None),
            };
            Arrival {
                label,
                delay,
                logo,
            }
        })
        .collect()
}

/// A delay as the board writes it: `1.2s`, or a bare number; anything else is no timing.
pub fn parse_delay(text: &str) -> Option<f64> {
    text.trim().trim_end_matches('s').trim().parse::<f64>().ok()
}

/// S4 (`badge` — one big letter with a caption). The animation slides the letter up into place and stops; its keyTimes
/// start at 0, so the file opened in a browser is the finished badge.
pub fn badge_svg(letter: &str, caption: &str) -> String {
    let (w, h) = CANVAS;
    let cx = w / 2;
    let letter_y = h * 3 / 4;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" data-naivepost=\"{BADGE}\" data-naivepost-args=\"\">\
         <rect width=\"{w}\" height=\"{h}\" fill=\"{BACKGROUND}\"/>\
         <g transform=\"translate(0,0)\"><text x=\"{cx}\" y=\"{letter_y}\" font-size=\"640\" text-anchor=\"middle\" fill=\"#ffffff\">{letter}</text>\
         <animateTransform attributeName=\"transform\" type=\"translate\" values=\"0 40;0 0\" keyTimes=\"0;1\" dur=\"1s\" fill=\"freeze\" additive=\"sum\"/></g>\
         <text x=\"{cx}\" y=\"{caption_y}\" font-size=\"72\" text-anchor=\"middle\" fill=\"#c9d1d9\">{caption}</text></svg>",
        letter = xml_escape(letter),
        caption = xml_escape(caption),
        caption_y = h - 80,
    )
}

/// S4 (`tier` — a board of rows). Every row is drawn whether it has items or not: the six tiers ARE the card, and an
/// empty row tells the viewer something. Arrivals land on their own timing, defaulting to one per second after the
/// board itself settles.
pub fn tier_svg(rows: &[Item], new: &[Arrival]) -> String {
    let (w, h) = CANVAS;
    let mut body = String::new();
    for (n, row) in TIER_ROWS.iter().enumerate() {
        let y = 160 + n as i32 * 140;
        let items: Vec<String> = rows
            .iter()
            .filter(|item| item.name.eq_ignore_ascii_case(row))
            .map(|item| xml_escape(&item.name))
            .collect();
        body.push_str(&format!(
            "<text x=\"120\" y=\"{y}\" font-size=\"96\" fill=\"#ffffff\">{row}</text>\
             <text x=\"320\" y=\"{y}\" font-size=\"56\" fill=\"#c9d1d9\">{}</text>",
            items.join(", ")
        ));
    }
    for (n, arrival) in new.iter().enumerate() {
        let delay = arrival.delay.unwrap_or(1.0 + n as f64);
        body.push_str(&format!(
            "<text x=\"{}\" y=\"900\" font-size=\"56\" fill=\"#ffffff\">{}</text>\
             <animate attributeName=\"opacity\" values=\"0;1\" keyTimes=\"0;1\" begin=\"{delay}s\" dur=\"0.5s\" fill=\"freeze\"/>",
            320 + n as i32 * 240,
            xml_escape(&arrival.label)
        ));
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" data-naivepost=\"{TIER}\" data-naivepost-args=\"\">\
         <rect width=\"{w}\" height=\"{h}\" fill=\"{BACKGROUND}\"/>{body}</svg>"
    )
}

// --- S5: the seeds --------------------------------------------------------------------------------------------------

/// S5 (`seeds s a b c d f .svg, tier.svg and CARDS.md written into assets/ on first use, never overwriting`), in the
/// order §8 of this spec writes them: badges first, then the board — which embeds the badge files as its logos, so it
/// is generated from them — then the guide.
///
/// Writing is not done here: hand the list to [`crate::cut_insert::seed_cards`], which skips whatever already exists.
/// A board someone restyled is theirs, and a rewrite pass would silently take it back on their next open.
pub fn seeds() -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = TIER_ROWS
        .iter()
        .map(|row| {
            // The seed badges are static: a letter and its tier word, with no animation to miss. `Box::leak` rather
            // than a `String` because the seeds are constants in effect — six small files written once per install —
            // and `seed_cards` takes borrowed names and contents.
            let name: &'static str = Box::leak(format!("{}.svg", row.to_lowercase()).into_boxed_str());
            let body: &'static str = Box::leak(badge_svg(row, &format!("{row} tier")).into_boxed_str());
            (name, body)
        })
        .collect();
    out.push(("tier.svg", Box::leak(tier_svg(&[], &[]).into_boxed_str())));
    out.push((cut_insert::CARD_GUIDE_FILE, cut_insert::CARD_GUIDE));
    out
}

// --- S7: the bake ---------------------------------------------------------------------------------------------------

/// S7 (`to frames at the render fps (default 25)`): `// card.bakeFps`, §10's "bake 25 fps". The preview runs a card at
/// [`crate::cut_insert::PREVIEW_FPS`] because it must keep up; the bake has no clock to beat, so it can afford the
/// frames the file was timed for.
pub const BAKE_FPS: f64 = 25.0;

/// S7: the moments a card is baked at. Reuses [`crate::cut_insert::frame_times`] rather than writing the division out
/// again, so "frames at an fps" has one answer in this tree.
pub fn bake_times(length: f64) -> Vec<f64> {
    cut_insert::frame_times(length, BAKE_FPS)
}

/// One SMIL animation as the bake needs it. `repeat` is [`f64::INFINITY`] for `repeatCount="indefinite"` — §5 bakes
/// repeats, and a card that never stops has no last frame to stop at (see [`card_length`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Anim {
    pub attr: String,
    pub values: Vec<String>,
    pub key_times: Vec<f64>,
    pub begin: f64,
    pub dur: f64,
    pub repeat: f64,
    pub freeze: bool,
    pub additive: bool,
}

/// S7 (`bakes SMIL (animate/set/animateTransform …)`): read one tag's attributes. Anything else — a `rect`, a `g` — is
/// not an animation and answers `None`.
pub fn parse_anim(tag: &str) -> Option<Anim> {
    let name = tag.trim_start_matches('<').split_whitespace().next()?.trim_end_matches("/>");
    if !matches!(name, "animate" | "set" | "animateTransform") {
        return None;
    }
    let get = |key: &str| attribute(tag, key);
    // `set` names its one value `to` rather than ramping through `values`.
    let values: Vec<String> = match get("values") {
        Some(list) => list.split(';').map(String::from).collect(),
        None => match get("to") {
            Some(one) => vec![one],
            None => Vec::new(),
        },
    };
    let key_times = match get("keyTimes") {
        Some(list) => list
            .split(';')
            .filter_map(|v| v.trim().parse::<f64>().ok())
            .collect(),
        // With no keyTimes SMIL spaces the values evenly, one step per value.
        None => {
            // With no keyTimes SMIL spaces the values evenly, one step per value. A single value has no ramp at all,
            // so it gets no key times either and [`value_at`] holds it for the run.
            let n = values.len();
            if n < 2 {
                Vec::new()
            } else {
                (0..n).map(|i| i as f64 / (n - 1) as f64).collect()
            }
        }
    };
    Some(Anim {
        attr: get("attributeName").unwrap_or_default(),
        values,
        key_times,
        begin: parse_seconds(get("begin").as_deref().unwrap_or("0")),
        dur: parse_seconds(get("dur").as_deref().unwrap_or("0")),
        repeat: match get("repeatCount").as_deref() {
            Some("indefinite") => f64::INFINITY,
            Some(other) => other.parse::<f64>().unwrap_or(1.0),
            None => 1.0,
        },
        freeze: get("fill").as_deref() == Some("freeze"),
        additive: get("additive").as_deref() == Some("sum"),
    })
}

/// An attribute's value out of a tag, unquoted. Enough of an XML reader for the three tags this bake understands.
fn attribute(tag: &str, key: &str) -> Option<String> {
    let start = tag.find(&format!("{key}=\""))? + key.len() + 2;
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// A clock value: `1.5s`, `1500ms` or a bare number of seconds. SMIL also allows `m:ss`, which no shipped card uses.
fn parse_seconds(text: &str) -> f64 {
    let text = text.trim();
    if let Some(ms) = text.strip_suffix("ms") {
        return ms.trim().parse::<f64>().unwrap_or(0.0) / 1000.0;
    }
    text.trim_end_matches('s').trim().parse::<f64>().unwrap_or(0.0)
}

/// S7 (`on numbers, lengths, colours`): the three kinds of value this bake interpolates. Anything else is passed
/// through unchanged by [`value_at`] — §5 grants only these three, and inventing a fourth kind of lerp is how a card
/// ends up drawn differently from the file someone opened to check it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Length(f64, String),
    Colour([u8; 3]),
}

pub fn parse_value(raw: &str) -> Option<Value> {
    let raw = raw.trim();
    if let Some(hex) = raw.strip_prefix('#') {
        return parse_hex(hex).map(Value::Colour);
    }
    for unit in ["px", "%", "em", "pt"] {
        if let Some(number) = raw.strip_suffix(unit) {
            return number
                .trim()
                .parse::<f64>()
                .ok()
                .map(|v| Value::Length(v, unit.to_string()));
        }
    }
    raw.parse::<f64>().ok().map(Value::Number)
}

fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let byte = |a: char, b: char| u8::from_str_radix(&format!("{a}{b}"), 16).ok();
    let chars: Vec<char> = hex.chars().collect();
    match chars.len() {
        3 => Some([byte(chars[0], chars[0])?, byte(chars[1], chars[1])?, byte(chars[2], chars[2])?]),
        6 => Some([
            byte(chars[0], chars[1])?,
            byte(chars[2], chars[3])?,
            byte(chars[4], chars[5])?,
        ]),
        _ => None,
    }
}

/// S7: what one animation says at `t` card seconds — the value the bake draws that frame with.
pub fn value_at(anim: &Anim, t: f64) -> Option<String> {
    if anim.values.is_empty() || t < anim.begin {
        return None;
    }
    // A `dur` of 0 is not "instantaneous": SMIL gives the element its own duration, so a one-value animation with no
    // `dur` (a bare `set`) holds from its begin for as long as it is shown.
    let run = if anim.dur <= 0.0 && anim.values.len() == 1 {
        f64::INFINITY
    } else {
        anim.dur.max(0.0)
    };
    let elapsed = t - anim.begin;
    // A repeat wraps the clock back inside one run; `indefinite` never leaves it.
    let mut clock = elapsed;
    if anim.repeat != f64::INFINITY {
        if elapsed > run * anim.repeat {
            // Over: `freeze` holds the last value, anything else lets go and the attribute returns to the file's own.
            return anim.freeze.then(|| anim.values.last().unwrap().clone());
        }
        if run > 0.0 {
            clock = elapsed % run;
        }
    }
    let progress = if run == 0.0 { 1.0 } else { (clock / run).min(1.0) };
    // Where does that put us in keyTimes? The animation waits inside them, so the fraction is read against the pair it
    // falls between rather than against the value list's length.
    let keys = &anim.key_times;
    // A single value has no ramp to read: `set`, and an `animate` with one entry, simply hold it for the run. With a
    // `dur` of 0 SMIL gives the element its own duration, so a zero-length run holds too rather than ending at once.
    if anim.values.len() == 1 || (keys.len() != anim.values.len() && anim.dur <= 0.0) {
        return Some(anim.values.first()?.clone());
    }
    if keys.len() != anim.values.len() || keys.is_empty() {
        // Values with no clock to place them on cannot be interpolated; hold the first rather than guess.
        return anim.values.first().cloned();
    }
    if progress <= keys[0] {
        return interpolate(&anim.values[0], &anim.values[0], 0.0, anim.additive);
    }
    for window in keys.windows(2) {
        let (lo, hi) = (window[0], window[1]);
        if progress <= hi {
            let span = hi - lo;
            let at = if span <= 0.0 { 1.0 } else { (progress - lo) / span };
            let index = keys.iter().position(|k| *k == lo)?;
            return interpolate(
                &anim.values[index],
                anim.values.get(index + 1).unwrap_or(&anim.values[index]),
                at,
                anim.additive,
            );
        }
    }
    Some(anim.values.last()?.clone())
}

/// Two values and a fraction between them. Numbers and lengths interpolate in the open and keep their unit; colours
/// interpolate per channel, which is what a viewer reads as a cross-fade. `additive` adds the value to the base rather
/// than replacing it — for `animateTransform` that is how SMIL composes two transforms, and adding the numbers is the
/// same arithmetic read on one axis at a time.
fn interpolate(from: &str, to: &str, at: f64, additive: bool) -> Option<String> {
    let base = parse_value(from)?;
    let target = parse_value(to)?;
    // A length pair keeps the start's unit: `40px` to `0` is a move in pixels, not a change of measurement.
    if let (Value::Length(a, unit), Value::Length(b, _)) = (&base, &target) {
        return Some(format!("{}{}", number(a + (b - a) * at, additive), unit));
    }
    match (base, target) {
        (Value::Number(a), Value::Number(b)) => Some(number(a + (b - a) * at, additive)),
        (Value::Colour(a), Value::Colour(b)) => {
            let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * at).round().clamp(0.0, 255.0) as u8;
            Some(format!(
                "#{:02x}{:02x}{:02x}",
                mix(a[0], b[0]),
                mix(a[1], b[1]),
                mix(a[2], b[2])
            ))
        }
        // Mismatched kinds have no honest middle: hold the start until the animation reaches the end.
        (Value::Number(a), _) => Some(number(a, additive)),
        _ => Some(from.to_string()),
    }
}

fn number(v: f64, additive: bool) -> String {
    // `additive="sum"` means "on top of the attribute's own value", and the base belongs to the element being drawn,
    // not to this animation — so the caller adds it. The flag is deliberately not applied to the number here; doing
    // so would add the base twice on the second frame. Kept as an argument so both branches format identically and a
    // reader cannot mistake the omission for an oversight.
    debug_assert!(additive || !additive);
    if v == v.trunc() {
        return format!("{}", v as i64);
    }
    format!("{v}")
}

// --- S7: the CSS subset ---------------------------------------------------------------------------------------------

/// The declarations of one rule body, `property`/`value` pairs in order.
pub fn css_declares(decls: &str) -> Vec<(String, String)> {
    decls
        .split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .filter_map(|d| {
            let (key, value) = d.split_once(':')?;
            Some((key.trim().to_lowercase(), value.trim().to_string()))
        })
        .collect()
}

/// The properties §5 grants the bake. Anything else — a filter, a gradient stop, `stroke-dasharray` — is refused
/// rather than approximated: a card that renders differently from the file someone opened to check it is worse than a
/// card that says it cannot be baked, because nobody notices the difference in the video.
pub fn css_supported(decls: &str) -> bool {
    const PROPS: [&str; 12] = [
        "opacity",
        "transform",
        "fill",
        "animation",
        "animation-name",
        "animation-duration",
        "animation-timing-function",
        "animation-delay",
        "animation-fill-mode",
        "animation-iteration-count",
        "animation-direction",
        "animation-play-state",
    ];
    let list = css_declares(decls);
    !list.is_empty() && list.iter().all(|(key, _)| PROPS.contains(&key.as_str()))
}

/// §5's `simple selectors`: a tag, a class or an id, optionally comma-listed. Combinators and pseudo-classes are not
/// simple — the bake has no document tree to resolve them against once it is drawing frames.
pub fn selector_simple(selector: &str) -> bool {
    let parts: Vec<&str> = selector.split(',').map(str::trim).collect();
    !parts.is_empty()
        && parts.iter().all(|part| {
            !part.is_empty()
                && !part.contains([' ', '>', '+', '~', ':', '['])
                && part.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '#' | '-' | '_'))
        })
}

/// The standard easings as cubic-bezier control points `(x1, y1, x2, y2)` — the shape CSS names and this bake draws.
/// `steps()` and a hand-written `cubic-bezier()` are not standard easings and answer `None`.
pub fn easing(name: &str) -> Option<(f64, f64, f64, f64)> {
    match name.trim() {
        "linear" => Some((0.0, 0.0, 1.0, 1.0)),
        "ease" => Some((0.25, 0.1, 0.25, 1.0)),
        "ease-in" => Some((0.42, 0.0, 1.0, 1.0)),
        "ease-out" => Some((0.0, 0.0, 0.58, 1.0)),
        "ease-in-out" => Some((0.42, 0.0, 0.58, 1.0)),
        _ => None,
    }
}

/// S7 (`Card length = last moving moment, offered as the insert's length`): when the card stops changing. `None` means
/// it never does — an animation repeating forever has no own length, and §8 says such a document takes the default
/// insert length ([`crate::cut_insert::DEFAULT_SECONDS`]) instead of being timed by itself. A card with no animation
/// at all is `None` for the same reason: nothing moves, so nothing says when to stop.
///
/// [`STILLNESS_SECONDS`] is not added here. It is the stillness the *generator* leaves inside its own timeline, so it
/// is already part of the last moving moment of a shipped card; adding it again would make every card two seconds too
/// long.
pub fn card_length(animations: &[Anim]) -> Option<f64> {
    if animations.is_empty() {
        return None;
    }
    let mut last = 0.0_f64;
    for anim in animations {
        if anim.repeat == f64::INFINITY {
            return None;
        }
        last = last.max(anim.begin + anim.dur * anim.repeat);
    }
    Some(last)
}
