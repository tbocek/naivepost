use serde_json::json;

pub const CAPTION_MIN_SECONDS: f64 = 0.3;

/// The fade the app puts on a caption's edges, in seconds.
pub const FADE_SECONDS: f64 = 0.15;

pub const DEFAULT_ZOOM_BOX: (f64, f64) = (0.5, 0.5);

pub const KINDS: [&str; 3] = ["zoom", "stop", "volume"];

/// One clip in the batch: its length in its own seconds, plus what has been put on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub n: u32,
    pub start: f64,
    pub length: f64,
    pub rate: f64,
    captions: Vec<Caption>,
    effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Caption {
    /// Offsets inside the clip, after clamping.
    pub span: (f64, f64),
    pub text: String,
    pub fade: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Effect {
    pub kind: &'static str,
    /// Offsets inside the clip, after clamping.
    pub span: (f64, f64),
    pub gain: f64,
    pub box_used: Option<(f64, f64)>,
    pub fade: f64,
}

/// A batch of clips, each with its own seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Clips {
    clips: Vec<Clip>,
}

impl Clips {
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
        }
    }

    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

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
        // Clamped into the clip: an offset past the end is the model counting from the session
        // rather than from the clip, and the clip cannot show what it does not have.
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
            return crate::tools::error(
                "a volume gain of 1 changes nothing -- leave the audio alone",
            );
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
        // A zoom without a box gets the frame's middle.
        let box_used = (kind == "zoom").then(|| box_used.unwrap_or(DEFAULT_ZOOM_BOX));
        let gain = match kind {
            "volume" => gain.unwrap_or(1.0),
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

    pub fn clip_frames(&self, clip: u32, at: Option<f64>) -> Vec<Frame> {
        let Some(at_index) = self.position(clip) else {
            return Vec::new();
        };
        let clip = &self.clips[at_index];
        let count = (clip.length / crate::prepare::MODEL_FRAME_STEP).floor() as usize + 1;
        let frames: Vec<Frame> = (0..count)
            .map(|index| Frame {
                clip: clip.n,
                // The clip's own seconds, which is the space every other number here is in.
                at: index as f64 * crate::prepare::MODEL_FRAME_STEP,
                start: clip.start + index as f64 * crate::prepare::MODEL_FRAME_STEP,
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

    fn position(&self, clip: u32) -> Option<usize> {
        self.clips.iter().position(|candidate| candidate.n == clip)
    }

    pub fn captions_of(&self, clip: u32) -> &[Caption] {
        match self.position(clip) {
            Some(at) => &self.clips[at].captions,
            None => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub clip: u32,
    pub at: f64,
    /// The same moment in session seconds, for whoever extracts the picture.
    pub start: f64,
}

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
