//! §03-shell.md §5's last sentence: "Each section's ⓘ explains at length what the server is expected
//! to speak."
//!
//! The dialog draws four sections (Writing, Cutting, Audio, Drawing) and each one talks to a different
//! machine with a different API. §5 says the ⓘ has to say, AT LENGTH, what that machine must answer —
//! which is the part of spec/02-services.md §1 a person typing an address actually needs: an address
//! that answers something else looks identical to one that answers nothing.
//!
//! **How the explanation reaches the person.** The ⓘ is a button carrying its paragraph as its
//! tooltip, the same shape the shell's own ⓘ uses (`window.rs`'s `tab-info-button`, whose tooltip
//! follows `Shell::info_tip()`). A tooltip needs no second window, so the text cannot get lost behind
//! a popup GTK cannot paint into a snapshot, and it is readable by a pointer and by a test alike. A
//! click therefore does nothing beyond showing it: no handler is wired, because inventing one would
//! give the text two places to be shown from.
//!
//! Nothing here decides a value: the slots counts live on the spins beside the model boxes
//! (`settings::SLOT_TIP`), and this file only says what each box's server owes in return.

use adw::prelude::*;
use gtk4 as gtk;

/// The four sections, in dialog order: the widget-name key and the heading §5's inventory prints.
/// `settings.rs` passes the display text (the two optional servers with " (optional)" appended) and
/// derives its names from it through [`section_key`], so the list lives once here.
pub const SECTIONS: [(&str, &str); 4] = [
    ("writing", "Writing"),
    ("cutting", "Cutting"),
    ("audio", "Audio"),
    ("drawing", "Drawing"),
];

/// The word §5's picture puts after the ⓘ of the two optional servers. Spelled here rather than read
/// back out of the heading string, so a section can never gain the suffix without gaining the label.
pub const OPTIONAL_WORD: &str = "optional";

/// The heading text §5's inventory prints for the two optional servers, kept so [`section_key`] can
/// tell "Audio" from "Audio (optional)" without either spelling drifting into the other.
const OPTIONAL_SUFFIX: &str = " (optional)";

/// The key for a heading as the dialog spells it, which is how `settings.rs` derives the ⓘ name from
/// the text it was already passing. Unknown headings have no ⓘ, and stay exactly as they were.
pub fn section_key(heading: &str) -> Option<&'static str> {
    let bare = heading.strip_suffix(OPTIONAL_SUFFIX).unwrap_or(heading);
    SECTIONS
        .iter()
        .find(|(_, title)| *title == bare)
        .map(|(key, _)| *key)
}

/// The ⓘ control's icon: the app's OWN packed ⓘ, from `icons/` -- the same one §1's header uses
/// (`window.rs`'s `tab-info-button`, `window_page_box.rs`'s per-page ⓘ), and `icons.rs` lists it among
/// the names the theme search must find. GTK has no info icon of its own that reads as ⓘ on every
/// theme, and the circled-letter codepoint (ⓘ) renders as tofu on the fonts this container ships.
const INFO_ICON: &str = "help-about-symbolic";

/// Whether §5 marks this section optional — the word sits after the ⓘ in the picture, not inside the
/// heading, so it is carried separately rather than baked into the title.
pub fn is_optional(heading: &str) -> bool {
    heading.ends_with(OPTIONAL_SUFFIX)
}

/// What this section's server is expected to speak, in the length §5 asks for. `None` for a key this
/// module does not know, which is what makes an unlisted section impossible to give a wrong ⓘ.
pub fn explanation(section: &str) -> Option<&'static str> {
    Some(match section {
        "writing" => WRITING,
        "cutting" => CUTTING,
        "audio" => AUDIO,
        "drawing" => DRAWING,
        _ => return None,
    })
}

/// §5's ⓘ for the writing server.
const WRITING: &str = "The writing server is an OpenAI-compatible chat endpoint: it answers \
POST /v1/chat/completions, streaming, and accepts the tool calls the steps send with them, and \
GET /v1/models lists what it serves so Fetch models can offer real ids. One address covers every \
text and vision job, so the model named in the Model box has to see images too -- Prepare sends \
video frames to it. An empty Server box means the loopback default; a host typed without a scheme is \
read as https. The API key travels as Authorization: Bearer only when it is not empty, so a server \
that needs no key stays blank rather than sending an empty header.";

/// §5's ⓘ for the cutting tools.
const CUTTING: &str = "Cutting is not a server: ffmpeg does the work and ffprobe, taken from the same \
folder as the binary you name, measures what came back. It is expected to carry the filters the steps \
ask for -- rubberband, subtitles, loudnorm, atempo, amix, adelay and alimiter -- and the encoders \
libx264, libx265, aac and libopus; Test reads the version and reports whichever of those are missing, \
because a build without one fails mid-run rather than now. firefox is the model's web-search tool: \
\"off\" is a complete answer here (the model is simply offered no search), and anything else has to be \
a real firefox that can run one headless search.";

/// §5's ⓘ for the audio server.
const AUDIO: &str = "The audio server answers GET /health and GET /v1/models, where every model \
carries the task it serves -- asr, diar, tts, sep or align -- which is how a model id gets checked \
against the job it was typed into rather than merely being present. Speech goes out as POST \
/v1/audio/speech, files arrive by POST /v1/ui/upload with the name in X-Audiocpp-Filename, and a \
task runs as POST /v1/tasks/run with the model and its request; POST /v1/tasks/unload_all_models is \
the housekeeping call every run ends with, quietly and best effort. Each model box below names one of \
these tasks, and Test asks the catalogue about that one, so a model served under another task is \
refused by name instead of failing later.";

/// §5's ⓘ for the drawing server.
const DRAWING: &str = "The drawing server is sd-server: it answers GET /sdcpp/v1/capabilities, takes \
a draw as POST /sdcpp/v1/img_gen and hands back a job id, then GET /sdcpp/v1/jobs/<id> carries that \
job until it is drawn, and a cancelled job is ended by POST /sdcpp/v1/jobs/<id>/cancel. A port here \
answering something OpenAI-shaped is NOT sd-server -- the Test says so out loud, because a chat \
tenant squatting on 1234 otherwise reads as a server that merely has no capabilities. Only the \
thumbnail depends on it: down, the render completes and the drawing half fails alone.";

/// §5's ⓘ button for one section: the icon, its stable name, and the explanation as its tooltip.
///
/// Named `settings-info-<key>` so a test and the snapshot find it the way they find every other control
/// on this dialog. `None` for an unknown key: the caller then draws the heading alone, as it did before.
pub fn info_button(section: &str) -> Option<gtk::Button> {
    let text = explanation(section)?;
    // A text glyph, not an icon name: see [`INFO_GLYPH`] about themes. Circled-letter glyphs render as
    // squares on the fonts this container ships (observed in `shots/03-settings.png`), and the shell's
    // own ⓘ is an icon for the same reason -- so this one borrows that icon rather than a codepoint.
    let button = gtk::Button::from_icon_name(INFO_ICON);
    button.set_widget_name(&format!("settings-info-{section}"));
    button.set_tooltip_text(Some(text));
    button.set_valign(gtk::Align::Center);
    button.add_css_class("flat");
    // The ⓘ is read, not pressed: its whole content is the tooltip, so a press must not look like a
    // dead control. Sensitive stays true -- an insensitive button shows no tooltip at all.
    Some(button)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_explains_a_server_in_a_paragraph() {
        for (key, _) in SECTIONS {
            let text = explanation(key).expect("each §5 section has an explanation");
            assert!(
                text.chars().count() > 120,
                "{key}: §5 asks for length, got {text:?}"
            );
        }
        assert!(
            explanation("nope").is_none(),
            "an unknown section gets no ⓘ"
        );
    }
}
