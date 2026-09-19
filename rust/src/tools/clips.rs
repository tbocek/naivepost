//! §02-services.md §3 tool catalogue, rewrite directive B: the clips pass's own tools.
//!
//! A clip's own seconds are the unit here: everything the model sends is an offset inside the clip it
//! was shown, so nothing has to know where that clip sits in the session. The app then places what it
//! asked for — clamped into the clip, with the fades it always adds — and says back what landed. That
//! report is the difference between a tool and a guess: a caption shortened at the edge, a zoom whose
//! box was moved inside the frame, and a speed that had to come down are all facts the model cannot
//! see unless they are told.
//!
//! The kinds are `zoom`, `stop` and `volume`. They stay as these three words rather than mapping onto
//! [`crate::cut::EffectKind`]: that enum has no `Stop` (a stop is a speed whose rate is 0) and does have
//! `Text`/`Svg`/`Label`, which this pass never places — an enum shared between the two would be a list
//! of cases each side has to remember to reject.

use serde_json::json;

/// A caption shorter than this is dropped. P.policy.captionMinSeconds
pub const CAPTION_MIN_SECONDS: f64 = 0.3;

/// The fade the app puts on a caption's edges, in seconds — its own rule, reported so the model can see
/// the caption as it will play rather than as it was written. §3.7's "with its fades" is this number; no
/// parameter names it, which is why it lives here and not in 10-parameters.md.
pub const FADE_SECONDS: f64 = 0.15;

/// How wide and tall the default zoom box is, as shares of the frame — what a zoom with no box gets, and
/// why every zoom in the prototype was a centred punch-in.
pub const DEFAULT_ZOOM_BOX: (f64, f64) = (0.5, 0.5);

/// The three words `add_effect` takes. Kept as data so the error can list them without repeating the
/// match arms below in prose.
pub const KINDS: [&str; 3] = ["zoom", "stop", "volume"];

/// One clip in the batch: its length in its own seconds, plus what has been put on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// The clip's number in the batch — what the model sends as `clip`.
    pub n: u32,
    /// Where the clip sits in the session; kept so a caller can find its frames.
    pub start: f64,
    /// Its length, which is the whole coordinate space this module works in.
    pub length: f64,
    /// The rate applied to it, 1.0 for none.
    pub rate: f64,
    captions: Vec<Caption>,
    effects: Vec<Effect>,
}

/// A caption as placed.
#[derive(Debug, Clone, PartialEq)]
pub struct Caption {
    /// Offsets inside the clip, after clamping.
    pub span: (f64, f64),
    pub text: String,
    /// The fade applied to each edge.
    pub fade: f64,
}

/// An effect as placed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Effect {
    /// One of [`KINDS`].
    pub kind: &'static str,
    /// Offsets inside the clip, after clamping.
    pub span: (f64, f64),
    /// Volume's linear gain; 1.0 for the kinds that have no gain, 0.0 for a stop.
    pub gain: f64,
    /// The box a zoom actually used, as (width, height) shares of the frame.
    pub box_used: Option<(f64, f64)>,
    /// The fade the app adds to an effect's edges.
    pub fade: f64,
}

/// A batch of clips, each with its own seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Clips {
    clips: Vec<Clip>,
    /// Seconds between the frames this pass may ask for. P.project.frameInterval
    frame_interval: f64,
}

impl Clips {
    /// A batch of clips, each `(number, session start, length)`.
    pub fn new(clips: &[(u32, f64, f64)]) -> Clips {
        Clips {
            clips: clips
                .iter()
                .map(|(n, start, length)| Clip {
                    n: *n,
                    start: *start,
                    length: *length,
                    rate: 1.0,
                    captions: Vec::new(),
                    effects: Vec::new(),
                })
                .collect(),
            frame_interval: 1.0,
        }
    }

    /// The batch's clips.
    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    /// Seconds between the frames `clip_frames` walks.
    pub fn set_frame_interval(&mut self, seconds: f64) {
        self.frame_interval = seconds;
    }

    /// `add_caption(clip, start, end, text)` — ok with the caption as placed, or an error naming what
    /// made it unplaceable.
    pub fn add_caption(&mut self, clip: u32, start: f64, end: f64, text: &str) -> String {
        let Some(at) = self.position(clip) else {
            return crate::tools::error(&format!(
                "clip {clip} is not in this batch -- it holds {} clip(s)",
                self.clips.len()
            ));
        };
        if text.trim().is_empty() {
            return crate::tools::error("the caption has no words in it");
        }
        let length = self.clips[at].length;
        // Clamped into the clip: an offset past the end is the model counting from the session rather
        // than from the clip, and the clip cannot show what it does not have.
        let placed = (start.clamp(0.0, length), end.clamp(0.0, length));
        if placed.1 - placed.0 < CAPTION_MIN_SECONDS {
            return crate::tools::error(&format!(
                "clip {clip} leaves {:.2} s for that caption -- under {CAPTION_MIN_SECONDS} s it is \
                 dropped, so move it inside the clip's 0..{length:.2} s",
                placed.1 - placed.0
            ));
        }
        let fade = FADE_SECONDS.min((placed.1 - placed.0) / 2.0);
        self.clips[at].captions.push(Caption {
            span: placed,
            text: text.trim().to_string(),
            fade,
        });
        crate::tools::ok(&json!({
            "clip": clip,
            "span": [placed.0, placed.1],
            "seconds": placed.1 - placed.0,
            "fade": fade,
            "text": text.trim(),
            "clamped": placed.0 != start || placed.1 != end,
        }))
    }

    /// `set_clip_speed(clip, rate)` — ok with the rate as applied and the on-screen length.
    ///
    /// A captioned clip may not be sped up. The prompt says so, and §3.7 says saying it again is the
    /// point: silently dropping the rate would leave the model believing its speed is in the cut, and a
    /// caption that runs at a different rate from its clip is the bug this rule exists to prevent.
    pub fn set_clip_speed(&mut self, clip: u32, rate: f64) -> String {
        let Some(at) = self.position(clip) else {
            return crate::tools::error(&format!("clip {clip} is not in this batch"));
        };
        if !self.clips[at].captions.is_empty() && rate > 1.0 {
            return crate::tools::error(&format!(
                "clip {clip} has a caption and cannot be sped up -- speed it before adding captions, \
                 or leave the rate at 1"
            ));
        }
        let length = self.clips[at].length;
        // The same clamp the cut pass uses, so one rule about rates holds in both places.
        let applied = crate::tools::cutpass::apply_rate(rate, length);
        self.clips[at].rate = applied;
        crate::tools::ok(&json!({
            "clip": clip,
            "asked": rate,
            "applied": applied,
            // On screen: the clip's own seconds divided by the rate.
            "on_screen": length / applied,
            "reason": (applied != rate).then(|| rate_reason(rate, applied, length)),
        }))
    }

    /// `add_effect(clip, kind, start, end, gain, box)` — ok with the effect as placed.
    ///
    /// `kind` is one of [`KINDS`]; anything else is refused rather than guessed at, because an effect
    /// that silently becomes something else is worse in a render than one that is missing.
    pub fn add_effect(
        &mut self,
        clip: u32,
        kind: &str,
        start: f64,
        end: f64,
        gain: Option<f64>,
        box_used: Option<(f64, f64)>,
    ) -> String {
        let Some(at) = self.position(clip) else {
            return crate::tools::error(&format!("clip {clip} is not in this batch"));
        };
        // Matched against the literals so the effect holds a 'static str, not a borrow of the argument.
        let kind: &'static str = match kind {
            "zoom" => "zoom",
            "stop" => "stop",
            "volume" => "volume",
            other => {
                return crate::tools::error(&format!(
                    "{other:?} is not an effect this pass places -- send one of {}",
                    KINDS.join(", ")
                ))
            }
        };
        if kind == "volume" && gain == Some(1.0) {
            return crate::tools::error("a volume gain of 1 changes nothing -- leave the audio alone");
        }
        let length = self.clips[at].length;
        let placed = (start.clamp(0.0, length), end.clamp(0.0, length));
        if placed.1 <= placed.0 {
            return crate::tools::error(&format!(
                "clip {clip} leaves nothing between {} and {} -- the span is empty after clamping it \
                 into the clip's 0..{length:.2} s",
                crate::tools::mm_ss(start),
                crate::tools::mm_ss(end)
            ));
        }
        let fade = FADE_SECONDS.min((placed.1 - placed.0) / 2.0);
        // A zoom without a box gets the frame's middle — reported as such so the model can see it aimed
        // nowhere in particular and send a box. A stop freezes on its rate; volume carries its gain.
        let box_used = (kind == "zoom").then(|| box_used.unwrap_or(DEFAULT_ZOOM_BOX));
        let gain = match kind {
            "volume" => gain.unwrap_or(1.0),
            // A stop is a speed whose rate is 0: it holds the frame for the span.
            "stop" => 0.0,
            _ => 1.0,
        };
        self.clips[at].effects.push(Effect {
            kind,
            span: placed,
            gain,
            box_used,
            fade,
        });
        crate::tools::ok(&json!({
            "clip": clip,
            "kind": kind,
            "span": [placed.0, placed.1],
            "fade": fade,
            "gain": gain,
            "box": box_used,
            "clamped": placed.0 != start || placed.1 != end,
        }))
    }

    /// `get_frames(clip, at)` — the clip's frames, so a zoom can be aimed at something.
    ///
    /// This pass sees pictures; the prototype showed it none, which is why every zoom it proposed was a
    /// centred punch-in. With `at` set, only the frame nearest that offset comes back.
    pub fn clip_frames(&self, clip: u32, at: Option<f64>) -> Vec<Frame> {
        let Some(at_index) = self.position(clip) else {
            return Vec::new();
        };
        let clip = &self.clips[at_index];
        let count = (clip.length / self.frame_interval).floor() as usize + 1;
        let frames: Vec<Frame> = (0..count)
            .map(|index| Frame {
                clip: clip.n,
                // The clip's own seconds, which is the space every other number here is in.
                at: index as f64 * self.frame_interval,
                start: clip.start + index as f64 * self.frame_interval,
            })
            .collect();
        match at {
            None => frames,
            Some(wanted) => frames
                .into_iter()
                .min_by(|a, b| {
                    (a.at - wanted)
                        .abs()
                        .partial_cmp(&(b.at - wanted).abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .into_iter()
                .collect(),
        }
    }

    /// The index of a clip in the batch.
    fn position(&self, clip: u32) -> Option<usize> {
        self.clips.iter().position(|candidate| candidate.n == clip)
    }

    /// The captions placed on one clip.
    pub fn captions_of(&self, clip: u32) -> &[Caption] {
        match self.position(clip) {
            Some(at) => &self.clips[at].captions,
            None => &[],
        }
    }

    /// The effects placed on one clip.
    pub fn effects_of(&self, clip: u32) -> &[Effect] {
        match self.position(clip) {
            Some(at) => &self.clips[at].effects,
            None => &[],
        }
    }
}

/// One frame this pass can show, named by the clip and the offset it was asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub clip: u32,
    /// Offset inside the clip.
    pub at: f64,
    /// The same moment in session seconds, for whoever extracts the picture.
    pub start: f64,
}

/// The sentence explaining a rate that is not the one asked for. Spelled out here rather than reached for
/// in the cut pass, whose wording is about a segment and this one's about a clip.
fn rate_reason(rate: f64, applied: f64, seconds: f64) -> String {
    use crate::tools::cutpass::{MAX_RATE, MIN_CLIP_SECONDS};
    if rate > MAX_RATE {
        return format!(
            "the fastest this pass plays a clip is {MAX_RATE}, so it stays on screen for {:.2} s",
            seconds / applied
        );
    }
    format!(
        "{rate} would leave {:.2} s on screen, under the shortest clip the render makes \
         ({MIN_CLIP_SECONDS} s)",
        seconds / rate
    )
}
