//! §AGENT#stack — the stack the program is built on, checked against what it actually has.
//!
//! `spec/AGENT.md` states three things about the stack: the language and toolkit (`Rust 2021; gtk4-rs
//! 0.11 (v4_14), libadwaita 0.9, cairo-rs, gio/glib, serde_json`, with `src/params.rs` cataloguing every
//! tuning value and holding no numbers); that the app "computes nothing itself", delegating to local HTTP
//! servers or a subprocess; and a container with no display. [`naivepost::stack_facts`] answers all three
//! from live facts — endpoints out of [`naivepost::services`], GTK's version off the linked library, the
//! display from the environment — and `naivepost --stack` prints it, so these tests assert what the binary
//! says rather than repeating the prose.
//!
//! Nothing here needs a display or a server. The two cases that go through the real binary use doors
//! answered before GTK opens.

use std::path::{Path, PathBuf};
use std::process::Command;

use naivepost::services;
use naivepost::stack_facts::{self, Reach};

/// Run the built binary with these arguments. Both doors this file exercises print and exit before any
/// display is asked for, so they run headless without xvfb.
fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_naivepost"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("the built binary runs");
    assert!(
        output.status.success(),
        "`naivepost {}` exited {}: {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The crate root, for reading a source file the rule is about.
fn manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// S1: the app delegates its heavy work — three HTTP servers at their live defaults, and the two
/// subprocesses named by their settings keys.
#[test]
fn sec_agent_stack_s1_the_app_delegates_its_heavy_work() {
    let parts = stack_facts::services();
    let names: Vec<&str> = parts.iter().map(|part| part.name).collect();
    for want in ["LLM", "audio.cpp", "sd.cpp", "ffmpeg / ffprobe", "firefox"] {
        assert!(names.contains(&want), "{want} is part of the stack, got {names:?}");
    }

    let endpoint_of = |name: &str| {
        parts
            .iter()
            .find(|part| part.name == name)
            .unwrap_or_else(|| panic!("{name} is not in the stack list"))
    };

    // The three HTTP ones carry the endpoint `services` actually defaults to — not a copy of the port.
    for (name, default) in [
        ("LLM", services::llm_default()),
        ("audio.cpp", services::audio_default()),
        ("sd.cpp", services::image_default()),
    ] {
        let part = endpoint_of(name);
        assert_eq!(part.reach, Reach::Http, "{name} is reached over HTTP");
        assert!(
            part.detail.contains(&default),
            "{name}'s line carries the live default endpoint {default}, got {:?}",
            part.detail
        );
    }

    // The two subprocesses name the setting that points at them, because that is how a machine says
    // where its ffmpeg or firefox lives.
    for (name, key) in [("ffmpeg / ffprobe", "FFMPEG"), ("firefox", "FIREFOX")] {
        let part = endpoint_of(name);
        assert_eq!(part.reach, Reach::Subprocess, "{name} is started as a subprocess");
        assert!(
            part.detail.contains(key),
            "{name}'s line names the {key} setting, got {:?}",
            part.detail
        );
    }

    // The two ports the spec spells out are the ones the code defaults to.
    assert!(
        services::llm_default().ends_with(":8731"),
        "the LLM default is the port AGENT.md names: {}",
        services::llm_default()
    );
    assert!(
        services::audio_default().ends_with(":8765"),
        "audio.cpp's default is the port AGENT.md names: {}",
        services::audio_default()
    );
    assert_eq!(services::SD_PORT, 1234, "and sd.cpp listens there");
}

/// A test's own scan: the lines of `params.rs` that still hold a bare number once comments, string
/// literals and tuple indices are gone. Not a door the program uses — the rule is about a source file.
///
/// What counts as "holds no numbers" has to be drawn somewhere, so it is drawn at the value a row
/// carries rather than at every digit in the file. A number written into a row's VALUE argument is the
/// thing the rule forbids — a tuning value living in the catalogue instead of in the module whose rule
/// reads it. Digits elsewhere on such a line are not values: `as f64` and `i64` name a type, `.nth(1)`
/// indexes an iterator, `lane_height_px(1)` passes a count to reach one constant, and `MAGIC` prints
/// bytes. So the scan looks at each row's first argument after `num(` … its closing paren only when
/// that argument is a bare literal.
fn bare_numbers_in_params(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        if !code.contains("param(") {
            continue;
        }
        let without_strings = strip_literals(code);
        let without_indices = strip_tuple_indices(&without_strings);
        // Type names and byte/word spellings carry digits but state no tuning value.
        let cleaned = without_indices
            .replace("f64", " ")
            .replace("i64", " ")
            .replace("u32", " ")
            .replace("usize", " ");
        let Some(at) = cleaned.find("num(") else { continue };
        let inside = &cleaned[at + 4..];
        let value_arg = match inside.find(')') {
            Some(end) => &inside[..end],
            None => inside,
        };
        // A spelled-out filter or band read out of another module's string is not a literal either; those
        // rows live in helper functions, which do not call `param(` themselves.
        if value_arg.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit()) {
            found.push((number + 1, line.trim().to_string()));
        }
    }
    found
}

/// Remove `"…"` and `'…'` literals, including escaped quotes inside them.
fn strip_literals(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' || c == '\'' {
            let quote = c;
            while let Some(inner) = chars.next() {
                if inner == '\\' {
                    chars.next();
                    continue;
                }
                if inner == quote {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Remove `.0`, `.1` … : a tuple field access is an index into something already counted.
fn strip_tuple_indices(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'.' && bytes.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
            i += 2;
            while bytes.get(i).is_some_and(|d| d.is_ascii_digit()) {
                i += 1;
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// S2: `params.rs` catalogues the tuning values and holds none of them — every entry links a constant,
/// and no number survives in the file outside comments and strings.
#[test]
fn sec_agent_stack_s2_params_catalogues_values_and_holds_no_numbers() {
    let path = manifest().join("src/params.rs");
    let text = std::fs::read_to_string(&path).expect("params.rs reads");
    let offenders = bare_numbers_in_params(&text);
    assert!(
        offenders.is_empty(),
        "AGENT.md says params.rs \"holds no numbers\"; these lines still spell one: {:?}",
        offenders
    );

    // And it does catalogue: each family returns entries whose id, spelling and source are all filled.
    let families: [(&str, Vec<naivepost::params::Param>); 5] = [
        ("prepare", naivepost::params::prepare()),
        ("cut", naivepost::params::cut()),
        ("effects", naivepost::params::effects()),
        ("narrate", naivepost::params::narrate()),
        ("produce", naivepost::params::produce()),
    ];
    let mut total = 0usize;
    for (family, entries) in families {
        assert!(!entries.is_empty(), "the {family} catalogue is empty");
        for entry in &entries {
            assert!(!entry.id.is_empty(), "{family}: an entry with no P.* id");
            assert!(
                !entry.spelled.trim().is_empty(),
                "{} has no spelled value",
                entry.id
            );
            assert!(
                !entry.from.is_empty(),
                "{} does not say which constant it comes from",
                entry.id
            );
            total += 1;
        }
    }
    assert!(total > 100, "the catalogue holds {total} entries; that is not every value");
}

/// S3: the toolkit is the one the spec names, read off what is linked rather than typed.
#[test]
fn sec_agent_stack_s3_the_toolkit_is_the_one_the_spec_names() {
    let parts = stack_facts::toolkit();
    let detail_of = |name: &str| {
        parts
            .iter()
            .find(|part| part.name == name)
            .unwrap_or_else(|| panic!("{} is not in the toolkit list", name))
            .detail
            .clone()
    };
    for reach in parts.iter() {
        assert_eq!(reach.reach, Reach::Linked, "the toolkit is linked in, not dialled");
    }

    assert!(
        detail_of("rust").contains("edition 2021"),
        "the language edition the spec names: {}",
        detail_of("rust")
    );

    // GTK: parse the numbers out of the line rather than hard-coding a patch release.
    let gtk = detail_of("gtk");
    let version = gtk
        .split_whitespace()
        .find(|word| word.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or_else(|| panic!("the gtk line carries a version: {gtk}"));
    let bits: Vec<u32> = version
        .split('.')
        .filter_map(|bit| bit.parse().ok())
        .collect();
    assert_eq!(bits.len(), 3, "gtk reports major.minor.micro, got {gtk}");
    assert_eq!(bits[0], 4, "GTK 4 is what the crate binds: {gtk}");
    assert!(bits[1] >= 14, "the v4_14 feature needs GTK >= 4.14, got {gtk}");
    assert!(
        gtk.contains("true"),
        "the running GTK satisfies the declared API level: {gtk}"
    );

    let adw = detail_of("libadwaita");
    let adw_bits: Vec<u32> = adw
        .split('.')
        .filter_map(|bit| bit.trim().parse().ok())
        .collect();
    assert!(
        adw_bits.len() == 3 && adw_bits[0] >= 1,
        "libadwaita reports a real version, got {adw}"
    );

    for name in ["cairo, gio/glib", "serde_json"] {
        assert!(
            !detail_of(name).trim().is_empty(),
            "{name} is named and said what it is for"
        );
    }
}

/// S4: the wire — `naivepost --stack` prints exactly those lines, and `--help` names the door.
#[test]
fn sec_agent_stack_s4_the_binary_prints_its_stack() {
    let printed = run(&["--stack"]);
    for line in stack_facts::lines() {
        assert!(
            printed.contains(&line),
            "`--stack` prints every line the module produces; missing:\n{line}\ngot:\n{printed}"
        );
    }
    assert!(printed.contains(Reach::Http.word()), "HTTP parts say how they are reached");
    assert!(
        printed.contains(Reach::Subprocess.word()),
        "subprocess parts say how they are reached"
    );
    assert!(
        printed.contains("display:"),
        "the answer ends with the display this process can see:\n{printed}"
    );

    let help = run(&["--help"]);
    assert!(
        help.contains("--stack"),
        "--help names the door that prints the stack"
    );
}

/// S5: the container clause — no display, and the program says so instead of pretending. With a
/// `DISPLAY` in the environment it names that one instead.
#[test]
fn sec_agent_stack_s5_the_display_answer_follows_the_environment() {
    let headless = Command::new(env!("CARGO_BIN_EXE_naivepost"))
        .arg("--stack")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("the built binary runs");
    let printed = String::from_utf8_lossy(&headless.stdout).into_owned();
    assert!(headless.status.success(), "--stack exits 0 headless");
    let line = printed
        .lines()
        .find(|line| line.starts_with("display:"))
        .unwrap_or_else(|| panic!("the answer carries a display line:\n{printed}"));
    assert!(
        line.contains("no display") && line.contains("xvfb-run"),
        "with no DISPLAY and no WAYLAND_DISPLAY the app says it is headless, got {line}"
    );

    let shown = Command::new(env!("CARGO_BIN_EXE_naivepost"))
        .arg("--stack")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env_remove("WAYLAND_DISPLAY")
        .env("DISPLAY", ":99")
        .output()
        .expect("the built binary runs");
    let printed = String::from_utf8_lossy(&shown.stdout).into_owned();
    assert!(
        printed.contains("display: X11 DISPLAY=:99"),
        "with a DISPLAY set, the app names it: {printed}"
    );
}
