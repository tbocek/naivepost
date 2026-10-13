//! §06-effects#5-cards-svg-inserts — the render's CALLER for the card rules [`crate::cut_cards`] decides.
//!
//! `cut_cards` holds the arithmetic (a path's arguments, its holes, its stamp, its animation reader and its
//! length) and nothing outside it used to call any of that: a card reached ffmpeg as whatever file the cut named,
//! with its `{{holes}}` still in it and its SMIL untouched, so the three §8 sentences were never logged and an
//! animated card was drawn by whichever renderer ffmpeg happened to be built with. This module is the missing
//! caller: one insert path in, one finished picture out — the document as it will be baked, the sentence saying how
//! it was arrived at, and either one static file or a strip of frames at the bake fps.
//!
//! # What lives here and what does not
//!
//! Nothing is decided twice. The wording comes from [`cut_cards::arrived_at`] / [`cut_cards::drawn_by`], the timing
//! from [`cut_cards::card_length`] / [`cut_cards::bake_times`], the interpolation from [`cut_cards::value_at`]. This
//! only walks a document, applies those answers tag by tag, and names the files. No window, no subprocess, no
//! rasteriser: a frame is a static SVG, which is what §8 says the bake writes (`f%05d.svg`).

use crate::cut_cards::{self as cards};
use crate::cut_insert;
use crate::produce_details;

/// One card, as far as the render is concerned: what to draw, how it came about, how long it runs, and which files
/// have to exist before ffmpeg is asked for it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Card {
    /// The file the cut names, with the query stripped — the name every log line quotes.
    pub file: String,
    /// The document after its arguments were applied: holes filled, or the card redrawn. Frame 0 of a still.
    pub drawn: String,
    /// §8's note: "drawn by the built-in <name> card", "filled in", or the no-placeholders sentence.
    pub arrived: String,
    /// Some = the card's own last moving moment; None = it has none, so the insert takes the default and LOOPS.
    pub length: Option<f64>,
    /// The baked frames, `(f%05d.svg name, that frame's document)`. Empty for a card that does not animate.
    pub frames: Vec<(String, String)>,
    /// The static file to write when there is nothing to bake — a still, or a card refused mid-way.
    pub still: Option<String>,
    /// Lines the run prints about this card: the §8 note and any refusal. Naming them here keeps the reason beside
    /// the decision rather than in the caller's head.
    pub logs: Vec<String>,
}

impl Card {
    /// Does this card need a folder of frames rather than one file?
    pub fn baked(&self) -> bool {
        !self.frames.is_empty()
    }

    /// The scratch folder a baked card's frames belong in, named the way §3 lists it — the card's own stem plus
    /// `.frames`, without the folders the source path lives in, because the frames sit under this clip's scratch.
    pub fn frame_dir(&self) -> String {
        produce_details::animation_frames(&self.file)
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string()
    }

    /// The scratch name of the single static file, when there is one.
    pub fn still_name(&self) -> String {
        produce_details::card_copy(&self.file)
    }
}

/// §5 + §8: turn one insert path (which may carry `?key=value&…`) and the file's bytes into a finished card.
///
/// `doc` is the document as read; reading the file is the caller's, which keeps this checkable without a disk and
/// lets the Cut page ask the same question about a file it just probed.
pub fn arrive(raw_path: &str, doc: &str) -> Card {
    let mut card = draw(raw_path, doc);
    bake(&mut card);
    card
}

/// The first half on its own: the path's arguments applied to the document and §8's note chosen, with no timing and
/// no frames. Split out because the Cut page needs exactly this much — what the card says and how it was arrived at —
/// while the render goes on to the bake.
pub fn draw(raw_path: &str, doc: &str) -> Card {
    let path = cards::split_path(raw_path);
    draw_split(&path, doc)
}

/// [`draw`] with the path already split, for a caller that read the query for its own reasons (the insert form does).
fn draw_split(path: &cards::Path, doc: &str) -> Card {
    let mut card = Card {
        file: path.file.clone(),
        ..Default::default()
    };

    // §8's self-contained contract, asked BEFORE anything is drawn: the frames are written to a temporary folder
    // and read back from there, so an `href` naming a file points somewhere that reader cannot reach. A `data:` URL
    // is the only href that travels, because it carries its own bytes inside the document.
    if let Some(href) = cards::absolute_href(doc) {
        card.logs.push(refusal(&path.file, &href));
        card.drawn = doc.to_string();
        card.arrived = cards::arrived_at(doc, &path.args);
        card.still = Some(card.still_name());
        return card;
    }
    if cards::uses_font_face(doc) {
        card.logs.push(font_log(&path.file));
    }

    // The self-contained contract refused nothing, so the picture can be drawn.
    let (drawn, arrived) = match cards::stamped(doc) {
        Some((kind, defaults)) => {
            let args = merge_args(defaults, &path.args);
            let redrawn = redraw(kind, &args);
            let changed = changed_note(defaults, &path.args);
            (redrawn, cards::drawn_by(kind, changed.as_deref()))
        }
        None => (cards::fill(doc, &path.args), cards::arrived_at(doc, &path.args)),
    };
    card.arrived = arrived;
    card.drawn = drawn;
    card.logs.push(card.arrived.clone());
    card
}

/// The card's own length as the Cut page should offer it: Some(last moving moment), or None when nothing moves (or
/// it never stops), which is where the caller falls back to P.policy.insertDefaultSeconds. Reading the file is the
/// point of this door — the alternative was a form that always opened at 4 s and never once at the card's own time.
pub fn offer_seconds(path: &str) -> Option<f64> {
    let raw = std::fs::read_to_string(cards::split_path(path).file.as_str()).ok()?;
    let card = arrive(path, &raw);
    card.length.filter(|seconds| *seconds > 0.0)
}

/// Fill one document from a scope, through the same door [`draw`] uses. Exposed so a test pins the escaping, the
/// fallback and the comment rule against the caller rather than only against `cut_cards::fill`.
pub fn join_for_test(doc: &str, scope: &[(String, String)]) -> String {
    cards::fill(doc, scope)
}

// --- S7: the bake ---------------------------------------------------------------------------------------------------

/// One animation plus the tag it animates and the byte range of the animation tag itself — what [`frame`] writes a
/// value against, and what it cuts out of every frame.
struct Bound {    anim: cards::Anim,
    /// The element tag whose attribute is being animated, as it stands in the document.
    target: String,
    /// Byte range of the ANIMATION tag itself, removed from every frame.
    at: (usize, usize),
}

/// §5's `to frames at the render fps (default 25)`: one static SVG per frame, named as §8 spells it (`f%05d.svg`).
/// Public because the caller writes these and hands ffmpeg the pattern, so both sides need one spelling.
pub fn frame_name(index: usize) -> String {
    format!("f{index:05}.svg")
}
/// One static SVG per frame at the bake fps: `f%05d.svg`.
pub const FRAME_PATTERN: &str = "f%05d.svg";

/// §5's `to frames at the render fps (default 25)` as ffmpeg wants it: the bake fps as a whole number, since the
/// image2 demuxer takes an integer rate and no card is timed for a fractional one.
pub fn bake_fps_arg() -> String {
    format!("{}", cards::BAKE_FPS as i64)
}

/// Read every animation out of a document: each `<animate>`/`<set>`/`<animateTransform>` joined with the element it
/// sits inside. SMIL puts a child animation inside its target, which is what makes the pairing possible without a
/// real XML tree.
fn bindings(doc: &str) -> Vec<Bound> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = doc[from..].find("<animate") {
        let start = from + rel;
        let Some(end_rel) = doc[start..].find('>') else { break };
        let end = start + end_rel + 1;
        from = end;
        let Some(anim) = cards::parse_anim(&doc[start..end]) else {
            continue;
        };
        // The element this animation belongs to: the nearest open tag before it, found by walking backwards for a
        // `<x …>` that has no matching `</x>` between it and here.
        let Some(target) = enclosing_tag(doc, start) else {
            continue;
        };
        out.push(Bound {
            anim,
            target,
            at: (start, end),
        });
    }
    out
}

/// The tag a child animation animates: walk the tags before `at` and keep the last unclosed one. Self-closing tags
/// close themselves, and a closing tag pops the one it names.
fn enclosing_tag(doc: &str, at: usize) -> Option<String> {
    let mut stack: Vec<String> = Vec::new();
    let head = &doc[..at];
    let mut from = 0usize;
    while let Some(rel) = head[from..].find('<') {
        let start = from + rel;
        if start >= at {
            break;
        }
        let Some(end_rel) = head[start..].find('>') else { break };
        let end = start + end_rel + 1;
        from = end;
        let tag = &head[start..end];
        if tag.starts_with("</") {
            stack.pop();
            continue;
        }
        if tag.ends_with("/>") || tag.starts_with("<!--") || tag.starts_with("<?") || tag.starts_with("<!") {
            continue;
        }
        stack.push(tag.to_string());
    }
    stack.pop()
}

/// §7's "static card files are finished pictures" applied to the bake: every frame is a plain SVG with the
/// animations already resolved into it, so nothing downstream has to understand SMIL at all.
fn bake(card: &mut Card) {
    let bounds = bindings(&card.drawn);
    let css = css_rules(&card.drawn);
    // A CSS animation naming a set of @keyframes that is not in the file: there is nothing to interpolate, so the
    // card is drawn as a still and the reason is said (§8's sentence, verbatim).
    let lost_css = css.iter().any(|rule| rule.property.is_empty());
    if lost_css {
        card.logs.push(no_keyframes(&card.file));
    }

    let timed: Vec<cards::Anim> = bounds.iter().map(|bound| bound.anim.clone()).collect();
    // Does anything here never stop? Read off the rules themselves, because [`cards::card_length`]'s `None` cannot
    // tell "never ends" apart from "nothing moves".
    let endless = endless_animation(&timed, &css);
    if endless {
        // §8 (`An indefinitely repeating document has no own length: takes the default insert length, loops for its
        // slot`): there is no last frame to bake to, so the card is drawn as ONE static picture and the encode loops
        // that picture — `-stream_loop -1` with `-t` trimming it, which is what makes looping safe.
        card.length = None;
        card.still = Some(card.still_name());
        return;
    }
    let own = cards::card_length(&timed).or_else(|| css_length(&css));
    // A CSS animation with no @keyframes is a still by §8, whatever the SMIL beside it says: there is nothing to
    // interpolate for that half, so baking frames would promise motion the file cannot supply. Asked BEFORE any
    // length is read, because a rule whose keyframes are missing still carries a duration and would otherwise be
    // timed as if it animated.
    if lost_css {
        card.length = None;
        card.still = Some(card.still_name());
        return;
    }
    if own.filter(|t| *t > 0.0).is_none() {
        // Nothing moves: no length of its own, and one static file is enough.
        card.length = None;
        card.still = Some(card.still_name());
        return;
    }
    // A card that stops carries its own last moving moment as its length; the strip runs over exactly that.
    card.length = own;
    let span = own.unwrap_or(0.0);
    let times = cards::bake_times(span);
    if times.is_empty() {
        card.still = Some(card.still_name());
        return;
    }
    // The last frame is the finished picture, not the last instant strictly inside the run: `bake_times(2.0)` stops
    // at 1.96 s of a 2 s animation, so a card that ends on its own gets one more frame at its own end — otherwise the
    // frozen value never reaches the screen.
    let mut moments = times;
    if let Some(end) = own.filter(|t| *t > 0.0) {
        // The grid stops one frame short of the run's end (`bake_times(2.0)` ends at 1.96 s), so the card's own
        // second of arrival is added: that frame shows the settled picture rather than a value still mid-ramp.
        if moments.last().copied().unwrap_or(0.0) < end - 1e-9 {
            moments.push(end);
        }
    }
    for (index, t) in moments.iter().enumerate() {
        card.frames
            .push((frame_name(index), frame(card, &bounds, &css, *t)));
    }
}

/// Does any animation here never stop? Read off the rules themselves rather than off [`cards::card_length`]'s `None`,
/// which cannot tell "never ends" apart from "nothing moves".
fn endless_animation(animations: &[cards::Anim], css: &[CssRule]) -> bool {
    animations.iter().any(|anim| anim.repeat == f64::INFINITY)
        || css.iter().any(|rule| rule.keyframes.is_some() && rule.repeat == f64::INFINITY)
}

/// One frame: every animation's value at `t` written into the attribute it animates, the animation tags taken out,
/// and the CSS rules' interpolated declarations laid on their selectors as inline styles.
fn frame(card: &Card, bounds: &[Bound], css: &[CssRule], t: f64) -> String {
    // Rewrite back-to-front so the byte ranges collected earlier stay valid.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    // The stylesheet goes first if any rule was baked: a frame carries its values inline or not at all, and leaving
    // `animation:` in place would have ffmpeg's own renderer animate the same picture a second time over ours.
    if !css.is_empty() {
        if let Some(at) = style_block(&card.drawn) {
            edits.push((at.0, at.1, String::new()));
        }
    }
    for bound in bounds {
        let Some(value) = cards::value_at(&bound.anim, t) else {
            // Let go: past the end with no freeze the attribute returns to the file's own value, which is what the
            // frame must show — so the tag simply goes and the element stays as written.
            edits.push((bound.at.0, bound.at.1, String::new()));
            continue;
        };
        let base = cards::attribute(&bound.target, &bound.anim.attr).unwrap_or_default();
        let written = if bound.anim.additive {
            add_base(&value, &base)
        } else {
            value
        };
        let tag = put_attribute(&bound.target, &bound.anim.attr, &written);
        edits.push((bound.at.0, bound.at.1, String::new()));
        if let Some(at) = card.drawn.find(&bound.target) {
            edits.push((at, at + bound.target.len(), tag));
        }
    }
    for rule in css {
        let Some(style) = rule.style_at(t) else { continue };
        for at in find_selector(&card.drawn, &rule.selector) {
            let tag = &card.drawn[at.0..at.1];
            edits.push((at.0, at.1, add_style(tag, &style)));
        }
    }
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut out = card.drawn.clone();
    for (start, end, text) in edits {
        out.replace_range(start..end, &text);
    }
    out
}

/// `additive="sum"` means "on top of the attribute's own value", and [`cut_cards::number`] deliberately leaves the
/// base to the caller so it is not added twice. Numbers add arithmetically; a transform list adds term by term,
/// which is how SMIL composes two transforms on one axis at a time.
fn add_base(value: &str, base: &str) -> String {
    let number = |text: &str| text.trim().parse::<f64>();
    if let (Ok(one), Ok(two)) = (number(value), number(base)) {
        return format!("{}", one + two);
    }
    let terms: Vec<&str> = value.split_whitespace().collect();
    let bases: Vec<&str> = base.split_whitespace().collect();
    terms
        .iter()
        .enumerate()
        .map(|(i, term)| match (term.parse::<f64>(), bases.get(i).and_then(|b| b.parse::<f64>().ok())) {
            (Ok(v), Some(b)) => format!("{}", v + b),
            _ => (*term).to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Write `key="value"` into a tag, replacing the attribute if it is already there. When the element spells the same
/// name twice (an outer tag and a nested one, as a `<g>` around a `<text>` both carry `transform` in a badge), only
/// the FIRST is written: that is the element the child animation belongs to.
fn put_attribute(tag: &str, key: &str, value: &str) -> String {
    let quoted = format!("{key}=\"");
    if let Some(start) = tag.find(&quoted) {
        let from = start + quoted.len();
        if let Some(end) = tag[from..].find('"') {
            let mut out = String::from(&tag[..start]);
            out.push_str(&format!("{key}=\"{}\"", cards::esc(value)));
            out.push_str(&tag[from + end + 1..]);
            return out;
        }
    }
    let close = tag.rfind(if tag.ends_with("/>") { "/>" } else { ">" }).unwrap_or(0);
    let mut out = String::from(&tag[..close]);
    out.push_str(&format!(" {key}=\"{}\"", cards::esc(value)));
    out.push_str(&tag[close..]);
    out
}

/// Lay a declaration on a tag's own `style`, since an inline style outranks the sheet the animation lived in.
fn add_style(tag: &str, style: &str) -> String {
    let quoted = "style=\"";
    if let Some(start) = tag.find(quoted) {
        let from = start + quoted.len();
        if let Some(end) = tag[from..].find('"') {
            let mut out = String::from(&tag[..start]);
            out.push_str(&format!("style=\"{};{}", &tag[from..from + end], style));
            out.push_str(&tag[from + end..]);
            return out;
        }
    }
    let close = tag.rfind(if tag.ends_with("/>") { "/>" } else { ">" }).unwrap_or(0);
    let mut out = String::from(&tag[..close]);
    out.push_str(&format!(" style=\"{style}\""));
    out.push_str(&tag[close..]);
    out
}

/// The byte range of the document's `<style …>…</style>` block, braces and all — what a baked frame drops.
fn style_block(doc: &str) -> Option<(usize, usize)> {
    let open = doc.find("<style")?;
    let end = doc[open..].find("</style>")? + open + "</style>".len();
    Some((open, end))
}

/// Where a simple selector's elements are: `.class` matches a `class` attribute holding that word, `#id` an `id`,
/// a bare name the tag's own name. §5 grants only these, and they are exactly the three that can be matched without
/// a document tree.
fn find_selector(doc: &str, selector: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for part in selector.split(',').map(str::trim) {
        if part.is_empty() || !cards::selector_simple(part) {
            continue;
        }
        let mut from = 0usize;
        while let Some(rel) = doc[from..].find('<') {
            let start = from + rel;
            let Some(end_rel) = doc[start..].find('>') else { break };
            let end = start + end_rel + 1;
            from = end;
            let tag = &doc[start..end];
            let name = tag[1..].split_whitespace().next().unwrap_or("").trim_end_matches('/');
            let hit = if let Some(id) = part.strip_prefix('#') {
                cards::attribute(tag, "id").is_some_and(|v| v == id)
            } else if let Some(class) = part.strip_prefix('.') {
                cards::attribute(tag, "class")
                    .is_some_and(|v| v.split_whitespace().any(|word| word == class))
            } else {
                name == part
            };
            if hit {
                out.push((start, end));
            }
        }
    }
    out
}

// --- S7: the CSS subset ---------------------------------------------------------------------------------------------

/// One `@keyframes` step: the fraction it lands at and the declarations written there.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Step {
    pub at: f64,
    pub decls: Vec<(String, String)>,
}

/// One animated rule: what it selects, the properties it ramps, the curve, and when it runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CssRule {
    pub selector: String,
    pub property: String,
    pub steps: Vec<Step>,
    pub easing: (f64, f64, f64, f64),
    pub begin: f64,
    pub dur: f64,
    pub repeat: f64,
    pub forwards: bool,
    /// `None` when the animation names a set of @keyframes the file does not contain.
    pub keyframes: Option<String>,
}

impl CssRule {
    /// The declaration this rule writes at `t`, already interpolated — or `None` before it begins, and after the end
    /// unless `forwards` holds it.
    pub fn style_at(&self, t: f64) -> Option<String> {
        if self.steps.is_empty() || t < self.begin {
            return None;
        }
        let run = self.dur.max(0.0);
        let elapsed = t - self.begin;
        if self.repeat != f64::INFINITY && run > 0.0 && elapsed > run * self.repeat {
            if !self.forwards {
                return None;
            }
            let last = &self.steps.last()?;
            return Some(format!("{}:{}", self.property, last.decls.first()?.1));
        }
        let clock = if run > 0.0 && self.repeat != f64::INFINITY {
            elapsed % run
        } else {
            elapsed
        };
        let linear = if run <= 0.0 { 1.0 } else { (clock / run).min(1.0) };
        let shaped = ease(self.easing, linear);
        let first = self.steps.first()?;
        if shaped <= first.at {
            return Some(decl_of(self, &first.decls));
        }
        for pair in self.steps.windows(2) {
            let (lo, hi) = (&pair[0], &pair[1]);
            if shaped <= hi.at {
                let span = hi.at - lo.at;
                let at = if span <= 0.0 { 1.0 } else { (shaped - lo.at) / span };
                let from = decl_value(self, &lo.decls)?;
                let to = decl_value(self, &hi.decls)?;
                return Some(format!("{}:{}", self.property, mix(from, to, at)));
            }
        }
        Some(decl_of(self, &self.steps.last()?.decls))
    }
}

fn decl_of(rule: &CssRule, decls: &[(String, String)]) -> String {
    decl_value(rule, decls).map_or(String::new(), |v| format!("{}:{}", rule.property, v))
}

fn decl_value(rule: &CssRule, decls: &[(String, String)]) -> Option<String> {
    decls
        .iter()
        .find(|(key, _)| key == &rule.property)
        .map(|(_, value)| value.clone())
}

/// Two values of one property and a fraction between them, through the bake's own three kinds.
fn mix(from: String, to: String, at: f64) -> String {
    match (cards::parse_value(&from), cards::parse_value(&to)) {
        (Some(cards::Value::Colour(a)), Some(cards::Value::Colour(b))) => {
            let step = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * at).round().clamp(0.0, 255.0) as u8;
            format!("#{:02x}{:02x}{:02x}", step(a[0], b[0]), step(a[1], b[1]), step(a[2], b[2]))
        }
        _ => {
            // Numbers and lengths ramp in the open; anything else holds the start until the step arrives.
            let unit = to.chars().rev().take_while(|c| !c.is_ascii_digit() && *c != '.').collect::<String>();
            let unit = unit.chars().rev().collect::<String>();
            let a = from.trim_end_matches(unit.as_str()).trim().parse::<f64>();
            let b = to.trim_end_matches(unit.as_str()).trim().parse::<f64>();
            match (a, b) {
                (Ok(a), Ok(b)) => format!("{}{}", a + (b - a) * at, unit),
                _ => from,
            }
        }
    }
}

/// A cubic-bezier curve solved for `x` by bisection — enough accuracy for a frame grid and no dependency.
fn ease(curve: (f64, f64, f64, f64), x: f64) -> f64 {
    let (x1, y1, x2, y2) = curve;
    if (x1, y1, x2, y2) == (0.0, 0.0, 1.0, 1.0) || !(0.0..=1.0).contains(&x) {
        return x;
    }
    let poly = |t: f64, a: f64, b: f64| 3.0 * t * (1.0 - t) * (1.0 - t) * a + 3.0 * t * t * (1.0 - t) * b + t * t * t;
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if poly(mid, x1, x2) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    poly((lo + hi) / 2.0, y1, y2)
}

/// Every animated rule in the document's `<style>` blocks, with its `@keyframes` looked up in the same file.
pub fn css_rules(doc: &str) -> Vec<CssRule> {
    let mut out = Vec::new();
    let mut rest = doc;
    while let Some(open) = rest.find("<style") {
        // Start the body AFTER the `<style …>` tag's own `>`: slicing from `<style`'s end would leave the `>` at the
        // head of the first selector, and `>.fade` is not a simple selector.
        let Some(tag_end) = rest[open..].find('>') else { break };
        let body_from = open + tag_end + 1;
        let Some(close) = rest[body_from..].find("</style>") else { break };
        let body = &rest[body_from..body_from + close];
        rest = &rest[body_from + close..];
        let sheets = keyframes_in(body);
        let mut from = 0usize;
        while let Some(rel) = body[from..].find('{') {
            let sel_start = from + rel;
            let selector = body[from..sel_start].trim();
            // The block's own closing brace, found by counting: a `@keyframes` rule opens another brace per step, so
            // the first `}` is a step's, not the rule's. Without this a rule written after its keyframes would be
            // read as `{ from { opacity: 0`, which is no rule at all.
            let bytes = body.as_bytes();
            let mut depth = 1usize;
            let mut at = sel_start + 1;
            while at < bytes.len() && depth > 0 {
                match bytes[at] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    break;
                }
                at += 1;
            }
            let decls = &body[sel_start + 1..at.min(body.len())];
            from = (at + 1).min(body.len());
            if selector.is_empty() || selector.starts_with('@') {
                continue;
            }
            // A comma-listed selector is simple only if every part is; `selector_simple` says so for the whole list.
            if !cards::selector_simple(selector) {
                continue;
            }
            if !cards::css_supported(decls) {
                continue;
            }
            let Some(rule) = animated_rule(selector, decls, &sheets) else {
                continue;
            };
            out.push(rule);
        }
    }
    out
}

/// The `@keyframes` blocks of one sheet, by name. `from`/`to` and percentages both appear in real cards, so both
/// are read; a name listed twice keeps the later block, which is what a cascade does.
fn keyframes_in(body: &str) -> Vec<(String, Vec<Step>)> {
    let mut out: Vec<(String, Vec<Step>)> = Vec::new();
    let mut rest = body;
    while let Some(rel) = rest.find("@keyframes") {
        let after = &rest[rel + "@keyframes".len()..];
        let Some(brace) = after.find('{') else { break };
        let name = after[..brace].trim().trim_matches('"').to_string();
        // Find the block's own closing brace by counting, since a keyframe step opens another.
        let mut depth = 1usize;
        let mut i = brace + 1;
        let bytes = after.as_bytes();
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            if depth > 0 {
                i += 1;
            }
        }
        let inner = &after[brace + 1..i.min(after.len())];
        out.push((name, steps_in(inner)));
        rest = &after[i.saturating_add(1).min(after.len())..];
    }
    out
}

/// The steps of one `@keyframes` block, in ascending order of the fraction they land at.
fn steps_in(body: &str) -> Vec<Step> {
    let mut out: Vec<Step> = Vec::new();
    let mut rest = body;
    while let Some(rel) = rest.find('{') {
        let sel_start = rest[..rel].trim();
        let selector = sel_start.split(',').next().unwrap_or_default().trim();
        let Some(end_rel) = rest[rel..].find('}') else { break };
        let decls = &rest[rel + 1..rel + end_rel];
        rest = &rest[rel + end_rel + 1..];
        if selector.is_empty() {
            continue;
        }
        let at = match selector {
            "from" => 0.0,
            "to" => 1.0,
            other => other
                .strip_suffix('%')
                .and_then(|v| v.trim().parse::<f64>().ok())
                .map(|v| v / 100.0)
                .unwrap_or(0.0),
        };
        out.push(Step {
            at,
            decls: cards::css_declares(decls),
        });
    }
    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    out
}

/// The one property a rule animates, its timing and its keyframes. A rule that declares an animation but names no
/// known @keyframes keeps its name so the caller can say which card was drawn as a still.
fn animated_rule(selector: &str, decls: &str, sheets: &[(String, Vec<Step>)]) -> Option<CssRule> {
    let declared = cards::css_declares(decls);
    let value = declared
        .iter()
        .find(|(key, _)| key == "animation" || key == "animation-name")
        .map(|(_, value)| value.clone())?;
    let mut rule = CssRule {
        selector: selector.to_string(),
        // The property the rule ramps comes from its OWN declarations when it names one (`opacity: 0` beside the
        // animation), and otherwise from the keyframes it points at — a rule that only says `animation: fade 2s`
        // takes whatever property `@keyframes fade` writes, which is the whole point of naming them.
        property: declared
            .iter()
            .find(|(key, _)| matches!(key.as_str(), "opacity" | "transform" | "fill"))
            .map(|(key, _)| key.clone())
            .unwrap_or_default(),
        easing: cards::easing("linear")?,
        repeat: 1.0,
        ..Default::default()
    };
    // The shorthand's words in any order: a duration, a delay, a count, a name, a standard easing.
    for word in value.split_whitespace() {
        if let Some(name) = cards::easing(word) {
            rule.easing = name;
            continue;
        }
        let seconds = |text: &str| text.strip_suffix('s').and_then(|v| v.parse::<f64>().ok());
        if let Some(d) = seconds(word) {
            if rule.dur <= 0.0 {
                rule.dur = d;
            } else if rule.begin <= 0.0 {
                rule.begin = d;
            }
            continue;
        }
        if word.parse::<f64>().is_ok() {
            rule.repeat = word.parse().unwrap_or(1.0);
            continue;
        }
        if word == "infinite" {
            rule.repeat = f64::INFINITY;
            continue;
        }
        if word == "forwards" {
            rule.forwards = true;
            continue;
        }
        if word.contains('(') {
            continue;
        }
        rule.keyframes = Some(word.to_string());
    }
    // The longhand form wins over the shorthand's guess when both are present.
    for (key, value) in &declared {
        match key.as_str() {
            "animation-name" => rule.keyframes = Some(value.clone()),
            "animation-duration" => rule.dur = value.strip_suffix('s')?.trim().parse().unwrap_or(rule.dur),
            "animation-delay" => rule.begin = value.strip_suffix('s')?.trim().parse().unwrap_or(rule.begin),
            "animation-timing-function" => rule.easing = cards::easing(value).unwrap_or(rule.easing),
            "animation-iteration-count" => {
                rule.repeat = if value == "infinite" {
                    f64::INFINITY
                } else {
                    value.parse().unwrap_or(1.0)
                }
            }
            "animation-fill-mode" => rule.forwards = value == "forwards" || value == "both",
            _ => {}
        }
    }
    let steps = rule
        .keyframes
        .as_ref()
        .and_then(|name| sheets.iter().find(|(known, _)| known == name))
        .map(|(_, steps)| steps.clone());
    rule.steps = steps.unwrap_or_default();
    if rule.property.is_empty() {
        // The property is whatever the keyframes write, first step wins. When the named @keyframes are NOT in the
        // file there is nothing to read it from — and that is exactly §8's "a CSS animation with no @keyframes in
        // the file", so the rule is kept with an empty property and `bake` says the sentence rather than dropping it
        // here in silence.
        if let Some((key, _)) = rule.steps.first().and_then(|step| {
            step.decls
                .iter()
                .find(|(key, _)| matches!(key.as_str(), "opacity" | "transform" | "fill"))
        }) {
            rule.property = key.clone();
        }
    }
    Some(rule)
}

/// When the CSS animations stop: the last one's end, and `None` if any of them never does.
fn css_length(rules: &[CssRule]) -> Option<f64> {
    if rules.is_empty() {
        return None;
    }
    let mut last = 0.0f64;
    for rule in rules {
        if rule.keyframes.is_none() || rule.repeat == f64::INFINITY {
            return None;
        }
        last = last.max(rule.begin + rule.dur * rule.repeat);
    }
    Some(last)
}

// --- §8: the sentences -----------------------------------------------------------------------------------------------

/// §8 (`refuses an absolute one`): named with the href, because "self-contained" on its own tells nobody which line
/// of their file to change.
pub fn refusal(file: &str, href: &str) -> String {
    format!("!!! card {file}: an href pointing at {href} \u{2014} a card has to carry its own images, so it is not baked")
}

/// §8 (`machine fonts as a family list, no @font-face`): said, not fatal — the card still draws, in whatever face
/// the machine has, which is what a webfont would have failed to guarantee anyway.
pub fn font_log(file: &str) -> String {
    format!(">>> card {file}: it asks for a @font-face the render cannot fetch \u{2014} a machine font is used")
}

/// §8, verbatim: a CSS animation whose @keyframes are not in the file has nothing to interpolate, so the card is a
/// still and the person who wrote it is told which half is missing.
pub fn no_keyframes(file: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    format!("{name}: a CSS animation with no @keyframes in the file \u{2014} drawn as a still")
}

/// The defaults a shipped card carries, as pairs. The stamp's argument string is the same `key=value&…` spelling as
/// a path's query, so one reader serves both.
pub fn merge_args(defaults: &str, path: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = cards::split_path(&format!("c?{defaults}")).args;
    for (key, value) in path {
        let lower = key.to_lowercase();
        match out.iter_mut().find(|(known, _)| known == &lower) {
            Some(slot) => slot.1 = value.clone(),
            None => out.push((lower, value.clone())),
        }
    }
    out
}

/// Which argument the path changed, for §8's "except that …" half: `None` when the card was drawn exactly as it
/// shipped, because then there is nothing to except.
pub fn changed_note(defaults: &str, path: &[(String, String)]) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let base = cards::split_path(&format!("c?{defaults}")).args;
    let changed: Vec<String> = path
        .iter()
        .filter(|(key, value)| {
            let lower = key.to_lowercase();
            base.iter()
                .find(|(known, _)| known == &lower)
                .is_none_or(|(_, known)| known != value)
        })
        .map(|(key, value)| format!("{key} says \"{value}\""))
        .collect();
    (!changed.is_empty()).then(|| changed.join(", "))
}

/// §8 (`it is redrawn from the generator with the path's parameters merged over those defaults`): the two shipped
/// generators, chosen by the stamp's kind. An unknown kind is left as the file stands — somebody's card that names
/// itself after ours still gets its holes filled, which is the honest half of the answer.
pub fn redraw(kind: &str, args: &[(String, String)]) -> String {
    let get = |key: &str| {
        args.iter()
            .find(|(known, _)| known == key)
            .map(|(_, value)| value.clone())
    };
    match kind {
        cards::BADGE => cards::badge_svg(
            get("letter").as_deref().unwrap_or("?"),
            get("caption").as_deref().unwrap_or(""),
        ),
        cards::TIER => {
            // The board's rows come from its own item list, keyed by tier letter; `new` carries per-item timing.
            let items: Vec<cards::Item> = args
                .iter()
                .flat_map(|(key, value)| cards::parse_items(&format!("{key}|{value}")))
                .collect();
            let arrivals = match get("new") {
                Some(list) => cards::parse_arrivals(&list),
                None => Vec::new(),
            };
            cards::tier_svg(&items, &arrivals)
        }
        _ => String::new(),
    }
}

/// Is this file a card the render should arrive at? Any SVG insert goes through this module: an animated one is
/// baked, a plain one is filled and written as one static file, and both cases need §8's note said.
pub fn is_card(path: &str) -> bool {
    matches!(cut_insert::kind(path), cut_insert::Kind::Svg)
}

/// The two stamp attributes a shipped card carries, named here because the caller that writes a redrawn card back to
/// disk has to keep them on the root tag — without them the file it writes is no longer a card of ours.
pub const STAMP_KEYS: [&str; 2] = [cards::STAMP, cards::STAMP_ARGS];

// --- §5: what the render does with a card ---------------------------------------------------------------------------

/// One card staged for its clip's encode: which input ffmpeg should open, and what had to be written first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Staged {
    /// `(kind, path)` in the shape [`crate::produce_render::encode_command`] takes. `"card"` is a folder of baked
    /// frames read as a sequence; `"svg"` is one static document, looped like any other still picture.
    pub inputs: Vec<(String, String)>,
    /// Files to write before the command is built: the filled card, or one document per frame.
    pub files: Vec<(std::path::PathBuf, String)>,
    /// Lines the run prints about this card — §8's note and any refusal — said beside the clip they belong to.
    pub logs: Vec<String>,
    /// Some = the baked card has no own length, so its slot is longer than one pass of it and ffmpeg must loop the
    /// frames. `-t` on the output (`produce_render::output_limit`) is what makes looping safe.
    pub loops: bool,
}

/// Read the card's file, arrive at it, and write what the bake produced under this clip's scratch folder.
///
/// Returns `None` when the clip's source is not an SVG insert, when there is nothing to open (the caller already
/// skipped such a clip), or when the file cannot be read — a card that vanished is `produce_render`'s missing-insert
/// sentence, not a second one here. A refused card (an absolute `href`) still answers, with its static file, so the
/// refusal reaches the log rather than being swallowed.
pub fn stage(clip: &crate::produce_render::Clip, resolved: Option<&str>) -> Option<Staged> {
    let path = resolved?;
    if !is_card(&clip.source) {
        return None;
    }
    let doc = std::fs::read_to_string(path).ok()?;
    let card = arrive(&clip.source, &doc);
    let mut staged = Staged { logs: card.logs.clone(), ..Default::default() };
    if card.baked() {
        // §8: `the bake writes one static SVG per frame (f%05d.svg)`, into a folder of its own beside the file it
        // was baked from. ffmpeg reads the pattern, so the frames are only inputs once they are on disk.
        let dir = clip.scratch.join(card.frame_dir());
        if std::fs::create_dir_all(&dir).is_err() {
            return None;
        }
        for (name, text) in &card.frames {
            staged.files.push((dir.join(name), text.clone()));
        }
        // The `%` of the pattern is spelled out by `frame_pattern`; the caller hands it over as the input path.
        staged.inputs.push(("card".to_string(), frame_pattern(&dir)));
        staged.loops = card.length.is_none();
        return Some(staged);
    }
    // One static document: the filled card (or the redrawn one) under the scratch name §3 lists. A card that never
    // animates is looped by the caller exactly as a still is, and `-t` trims it to the slot the cut gave it.
    let out = clip.scratch.join(card.still_name());
    staged.files.push((out.clone(), card.drawn.clone()));
    staged.inputs.push(("svg".to_string(), out.display().to_string()));
    // A single static picture needs looping whenever the clip is longer than one frame of it, which is every card
    // that has no strip of its own: §8's `takes the default insert length, loops for its slot`. The loop flag is the
    // caller's to set — `produce_render::encode_command_with` takes it as an argument — so a test that stages a card
    // and one that encodes it cannot disagree about what "loops" means.
    staged.loops = card.length.is_none();
    Some(staged)
}

/// The ffmpeg image2 pattern for a baked card's frames, given the folder they were written into.
pub fn frame_pattern(dir: &std::path::Path) -> String {
    dir.join("f%05d.svg").display().to_string()
}
