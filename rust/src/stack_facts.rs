//! The stack this binary was built on — [`spec/AGENT.md`](../spec/AGENT.md#stack).
//!
//! The spec's own words: "`rust/` — Rust 2021; gtk4-rs 0.11 (`v4_14`), libadwaita 0.9, cairo-rs,
//! gio/glib, serde_json. `src/params.rs` catalogues every tuning value and holds no numbers";
//! "Naivepost computes nothing itself: heavy work is a request to one of four local HTTP servers
//! four local HTTP servers, with their ports in [`crate::services`] …) or an ffmpeg/GStreamer
//! subprocess"; and the container
//! has "no display, no GPU".
//!
//! # Why the program answers this
//!
//! A paragraph in a markdown file says what somebody intended; this module says what the running
//! process actually has. GTK's version comes from the linked library, the endpoints come from
//! [`crate::services`] rather than being retyped, and the display answer comes from the environment
//! the process was started in — so a machine running something else shows up here instead of quietly
//! agreeing with the prose. One door uses it: `naivepost --stack` in `main`, answered before any
//! display is asked for, which is also why it works inside the headless container.
//!
//! The `params.rs` half of the clause ("catalogues every tuning value and holds no numbers") is a rule
//! about a source file, so a test checks it by reading that file; nothing here duplicates the
//! catalogue.

use crate::services;
use adw;
use gtk4 as gtk;

/// How a piece of the stack is reached: over HTTP, as a subprocess the app spawns, or linked into the
/// binary itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// A local HTTP server the app sends a request to.
    Http,
    /// A program the app starts and reads, never a socket.
    Subprocess,
    /// Compiled and linked in: the toolkit and the serialiser.
    Linked,
}

impl Reach {
    pub fn word(self) -> &'static str {
        match self {
            Self::Http => "over HTTP",
            Self::Subprocess => "as a subprocess",
            Self::Linked => "linked in",
        }
    }
}

/// One component of the stack and what it does for the app.
pub struct Part {
    pub name: &'static str,
    pub detail: String,
    pub reach: Reach,
}

/// The outside work the app delegates to. No endpoint is typed here: each default comes from
/// [`services`], so moving a port there moves it in this answer too — the same no-drift discipline
/// [`crate::params`] keeps for tuning values.
pub fn services() -> Vec<Part> {
    vec![
        Part {
            name: "LLM",
            detail: format!(
                "{} — every text job and the vision job that describes frames",
                services::llm_default()
            ),
            reach: Reach::Http,
        },
        Part {
            name: "audio.cpp",
            detail: format!(
                "{} — speech to text, alignment, diarization, voice separation, text to speech",
                services::audio_default()
            ),
            reach: Reach::Http,
        },
        Part {
            name: "sd.cpp",
            detail: format!(
                "{} — the thumbnail, when one is drawn",
                services::image_default()
            ),
            reach: Reach::Http,
        },
        Part {
            name: "ffmpeg / ffprobe",
            detail: "the FFMPEG setting (empty = PATH) — audio extraction, frames, silence detection, \
                     every encode"
                .to_string(),
            reach: Reach::Subprocess,
        },
        Part {
            name: "firefox",
            detail: "the FIREFOX setting (or off) — headless web search, offered to three jobs as a tool"
                .to_string(),
            reach: Reach::Subprocess,
        },
    ]
}

/// The toolkit, as the process reports it. GTK's numbers are read off the linked library at run time;
/// libadwaita exposes no safe getter in 0.9, so its version comes from the C entry points the sys
/// crate binds (`adw_get_major_version` and friends) — an FFI read of a value the library publishes
/// for exactly this purpose, not a guess.
pub fn toolkit() -> Vec<Part> {
    let gtk = format!(
        "{}.{}.{} (the v4_14 API this crate is built against: {})",
        gtk::major_version(),
        gtk::minor_version(),
        gtk::micro_version(),
        // `gtk::check_version` compares against the version GTK was BUILT with, so on a newer library
        // it answers nothing; what the declared feature needs is that this library is at least 4.14.
        gtk::check_version(4, 14, 0).is_some() || gtk::minor_version() >= 14
    );
    // Safe wrappers do not exist for these three in libadwaita 0.9; the values are plain globals the
    // library exports, read once, no allocation and no ownership crossing.
    let adw_major = unsafe { adw::ffi::adw_get_major_version() };
    let adw_minor = unsafe { adw::ffi::adw_get_minor_version() };
    let adw_micro = unsafe { adw::ffi::adw_get_micro_version() };
    vec![
        Part {
            name: "rust",
            detail: format!("edition 2021 (naivepost {})", env!("CARGO_PKG_VERSION")),
            reach: Reach::Linked,
        },
        Part {
            name: "gtk",
            detail: gtk,
            reach: Reach::Linked,
        },
        Part {
            name: "libadwaita",
            detail: format!("{adw_major}.{adw_minor}.{adw_micro}"),
            reach: Reach::Linked,
        },
        Part {
            name: "cairo, gio/glib",
            detail: "the drawing area and the GObject layer gtk4-rs sits on".to_string(),
            reach: Reach::Linked,
        },
        Part {
            name: "serde_json",
            detail: "every reply, project file and request body the app reads or writes".to_string(),
            reach: Reach::Linked,
        },
    ]
}

/// What this process can see of a display. Wayland first, since that is what a host hands a container;
/// then X11; then the honest answer, which is what the container gets — `just test` and `just snapshot`
/// wrap the run in `xvfb-run`, and nothing here pretends otherwise.
pub fn display_state() -> String {
    if let Some(wayland) = std::env::var("WAYLAND_DISPLAY").ok().filter(|name| !name.is_empty()) {
        return format!("wayland {wayland}");
    }
    if let Some(display) = std::env::var("DISPLAY").ok().filter(|name| !name.is_empty()) {
        return format!("X11 DISPLAY={display}");
    }
    "no display (headless: `just test` and `just snapshot` wrap the run in xvfb-run)".to_string()
}

/// The whole answer: the toolkit, then what the app delegates to, then the display it can see.
pub fn lines() -> Vec<String> {
    let mut out: Vec<String> = toolkit()
        .into_iter()
        .chain(services())
        .map(|part| format!("{}: {} ({})", part.name, part.detail, part.reach.word()))
        .collect();
    out.push(format!("display: {}", display_state()));
    out
}
