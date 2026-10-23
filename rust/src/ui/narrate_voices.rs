//! §07-narrate#1-screen, number **9**: "Add file…" — copy one recording into the voices folder and
//! use it (§A.6). The chooser is thin: it picks a path and hands it to [`add_voice_file`], which does
//! the real work (convert with ffmpeg, name it, list it) so the tested seam and the clicked one are
//! the same function.

use adw::prelude::*;
use gtk4 as gtk;

/// The chooser's own title, §A.6 quoted: "Choose a voice sample".
const CHOOSER_TITLE: &str = "Choose a voice sample";

/// **9** Add file…: open the chooser. A cancel says so and copies nothing; an accepted file goes
/// through [`add_voice_file`] like any other door would.
#[expect(
    deprecated,
    reason = "the 4.10 replacement cannot offer a filtered native chooser"
)]
pub fn press_add_file(window: &adw::ApplicationWindow) {
    let chooser = gtk::FileChooserNative::builder()
        .title(CHOOSER_TITLE)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .build();
    let filter = gtk::FileFilter::new();
    // One suffix: a recording to clone from is audio, and the folder only ever lists `.wav`.
    filter.set_name(Some("audio"));
    filter.add_pattern("*.wav");
    chooser.add_filter(&filter);
    let win = window.clone();
    chooser.connect_response(move |chooser, response| {
        let picked = if response == gtk::ResponseType::Accept {
            chooser.file().as_ref().and_then(|f| f.path())
        } else {
            None
        };
        let said = match picked {
            Some(path) => match add_voice_file(&win, &path) {
                Ok(name) => name,
                Err(why) => why,
            },
            None => refused_no_file(),
        };
        set_status(&win, &said);
    });
    chooser.show();
}

/// Put one sentence on the shell's status line. The chooser's answer has to reach the same line the
/// page writes to, and `say` is private to the page, so this goes straight to the widget.
fn set_status(window: &adw::ApplicationWindow, said: &str) {
    if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
        status.set_text(said);
    }
}

/// The refusal when no file was chosen. Its own sentence, because a cancelled dialog must not read as
/// a copy that happened.
pub fn refused_no_file() -> String {
    "no file chosen \u{2014} nothing was copied into the voices folder".to_string()
}

/// Copy one recording into the voices folder as mono PCM wav under a sanitised, never-overwriting
/// name, then offer it as a voice. Returns the sentence for the status line; `Err` carries its own,
/// naming the file and why (§00: failure is specific and local).
///
/// The conversion is a real ffmpeg run (`-ac 1 -vn`, pcm wav) through [`crate::subprocess::run`], the
/// same leg every other audio step uses, so a machine without ffmpeg gets ffmpeg's reason rather than
/// a placeholder.
pub fn add_voice_file(
    window: &adw::ApplicationWindow,
    source: &std::path::Path,
) -> Result<String, String> {
    if !source.exists() {
        return Err(format!(
            "{} is not there \u{2014} nothing was copied",
            source.display()
        ));
    }
    let folder = voices_folder();
    crate::settings::ensure_voices_folder(&folder)?;
    let taken = crate::voices::list_wavs(&folder);
    // One rule for the never-overwriting name: `narrate_details::added_file_name` owns §6's `name-2`,
    // `name-3` sequence, and [`crate::voices::target_name`] delegates to it so a second copy of the same
    // recording can never overwrite (or silently re-key) the first.
    let name = crate::voices::target_name(&taken, source);
    let target = folder.join(&name);
    // §6 (`"Add file…"`): say what is about to happen before the conversion runs — a pasted recording is
    // made mono PCM by ffmpeg, and that is the step a slow copy is waiting on.
    crate::ui::window::log_line(&crate::narrate_details::reference_convert_log(&name));
    let ffmpeg = ffmpeg_program();
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        source.to_string_lossy().into_owned(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        target.to_string_lossy().into_owned(),
    ];
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::subprocess::run(&ffmpeg, &borrowed, |_| {})
        .map_err(|why| format!("{} could not be copied: {why}", source.display()))?;
    // Offer it now: the picker's rows come from the page state, so adding the name and refreshing is
    // what makes the new voice selectable without a restart.
    crate::ui::narrate_page::offer_voice(&name);
    crate::ui::narrate_page::refresh(window);
    Ok(format!(
        "{name} added to the voices folder \u{2014} pick it in the voice dropdown"
    ))
}

/// The voices folder this machine reads: Settings' `AUDIOCPP_VOICES`, else the legacy models root's
/// `voices/`, else the platform's own — [`crate::settings::voices_folder`]'s rule alone, fed by the
/// conf this process would read. The legacy key is read here the way `settings::read` documents it:
/// its `voices/` subfolder stands in when `AUDIOCPP_VOICES` stayed empty.
pub fn voices_folder() -> std::path::PathBuf {
    let paths = crate::settings::from_environment();
    let conf = paths
        .as_ref()
        .and_then(|paths| crate::settings::read(paths).ok())
        .unwrap_or_default();
    let legacy = std::env::var(crate::settings::LEGACY_MODELS).unwrap_or_default();
    let data_dir = paths
        .map(|paths| paths.data_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    crate::settings::voices_folder(
        &conf.voices,
        &legacy,
        std::env::var("FLATPAK_ID").is_ok(),
        &data_dir,
    )
}

/// Which ffmpeg to run: the Settings box, else the bare name — the same reading `speak_leg` uses so
/// a configured path applies here too.
fn ffmpeg_program() -> String {
    let conf = crate::settings::from_environment()
        .and_then(|paths| crate::settings::read(&paths).ok())
        .unwrap_or_default();
    if conf.ffmpeg.trim().is_empty() {
        "ffmpeg".to_string()
    } else {
        conf.ffmpeg.clone()
    }
}
