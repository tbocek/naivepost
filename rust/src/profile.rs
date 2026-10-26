//! §03-shell §5b / F0.14: profiles, User Context templates with fields. A profile is a plain-text file per
//! machine (`<config>/profiles/<name>.txt`): an optional field block between two `---` lines, then the text, in
//! which `{{name}}` stands where a field's value goes. Each field line names the control Prepare shows for it:
//!
//! ```text
//! title:    text     required "Title"
//! course:   choice+  "Course" = Distributed Systems & Blockchain | Software Engineering
//! ```
//!
//! This module is the template's rules and its storage; the Profiles window is `ui::profiles_dialog` and the
//! form on Prepare `ui::profile_form`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::settings::Paths;

/// The template syntax, as the Profiles window's ⓘ shows it (§03-shell §5b).
pub const SYNTAX: &str = "A template: an optional field block between two --- lines, then the text.

---
title:    text     required \"Title\"
episode:  number   required \"Episode\"
course:   choice+  \"Course\" = Distributed Systems | Software Engineering
language: choice   \"Spoken in\" = English | German
notes:    textarea \"Notes for the editor\"
---
This is \"{{title}}\", episode {{episode}} of {{course}}.
{{notes}}

One field per line:  name: kind [required] [\"Label\"] [= value | value ...]
  name      lower case letters, digits and _, starting with a letter
  text      one-line entry                     = its default
  number    one-line entry taking a number     = its default
  textarea  a box of several lines             = its default
  choice    dropdown of exactly the values     = the values, the first the default
  choice+   entry offering the values, any other may be typed
  required  Prepare's run waits until it is filled in
  # starts a comment line

In the text, {{name}} is replaced by the field's value; a line left empty by an empty field is dropped.

Sections, after the User Context. A section REPLACES that step's built-in prompt for a project using this
profile ({{name}} works there too); a step without one sends the built-in prompt. New starts with all of them.
  ## system      every request: the shared rules
  ## policy      the editing policy, read from the User Context
  ## describe    what is on screen (Prepare)
  ## textgate    did the slide's text change? (Prepare, decision model)
  ## fix         the transcript (Prepare)
  ## retake      abandoned takes (Prepare)
  ## textedit    repaired joins (Prepare)
  ## cut         the cut (Cut)
  ## captions    captions (Cut)
  ## speed       speed changes (Cut)
  ## effects     zooms, stops, volume (Cut)
  ## narrate     the narration (Narrate)
  ## translate   subtitles in other languages (Produce)
  ## youtube     title, description and thumbnail (Produce)
  ## produce     encoder settings, below

The Produce settings are set in the form under the template (the same controls as the Produce page); they
are kept in the file as a \"## produce\" section and set on a project when the profile is chosen.";

/// The template without its `## produce` section, and that section's rows: the Profiles window shows the text in
/// its box and the rows in the Produce form, never the rows as text.
pub fn split_produce(source: &str) -> (String, Vec<(String, String)>) {
    let (mut text, mut rows, mut inside) = (Vec::new(), Vec::new(), false);
    for line in source.lines() {
        if let Some(key) = line.trim().strip_prefix("## ").map(str::trim) {
            if key == PRODUCE {
                inside = true;
                continue;
            }
            if crate::prompts::is_known(key) {
                inside = false;
            }
        }
        if inside {
            if let Ok(row) = produce_row(line.trim()) {
                rows.push(row);
            }
            continue;
        }
        text.push(line);
    }
    (format!("{}\n", text.join("\n").trim_end()), rows)
}

/// The template with `rows` as its `## produce` section, at the end: what the Profiles window saves.
pub fn join_produce(text: &str, rows: &[(String, String)]) -> String {
    if rows.is_empty() {
        return text.to_string();
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|(row, value)| format!("{row}: {value}"))
        .collect();
    format!(
        "{}\n\n## {PRODUCE}\n{}\n",
        text.trim_end(),
        lines.join("\n")
    )
}

/// What "New" holds in the Profiles window: one big template with everything a profile can say. A field, a User
/// Context to fill in, then every step's section holding its shipped prompt (so editing one starts from the words
/// the app would send) and the Produce rows at their defaults.
pub fn starter() -> String {
    let mut out = String::from(
        "---\ntitle: text required \"Title\"\n---\nWhat this recording is, who speaks, what to keep and what to cut. \
The title of this one: {{title}}.\n",
    );
    for key in crate::prompts::KEYS {
        out.push_str(&format!(
            "\n## {key}\n{}\n",
            crate::prompts::shipped_text(key).unwrap_or_default()
        ));
    }
    out.push_str("\n## produce\n");
    for (row, value) in crate::produce_screen::defaults() {
        let value = if row == "Translate" && value.is_empty() {
            "none"
        } else {
            value
        };
        out.push_str(&format!("{row}: {value}\n"));
    }
    out
}

/// The control a field gets on Prepare (§5b's table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Number,
    TextArea,
    /// A dropdown of exactly the listed values.
    Choice,
    /// An entry with the listed values offered; any other value may be typed.
    ChoiceCustom,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Number => "number",
            Kind::TextArea => "textarea",
            Kind::Choice => "choice",
            Kind::ChoiceCustom => "choice+",
        }
    }

    fn of(word: &str) -> Option<Kind> {
        Some(match word {
            "text" => Kind::Text,
            "number" => Kind::Number,
            "textarea" => Kind::TextArea,
            "choice" => Kind::Choice,
            "choice+" => Kind::ChoiceCustom,
            _ => return None,
        })
    }

    pub fn is_choice(self) -> bool {
        matches!(self, Kind::Choice | Kind::ChoiceCustom)
    }
}

/// One field of a template.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub kind: Kind,
    pub required: bool,
    /// What Prepare writes beside the control; the name when the line gives none.
    pub label: String,
    /// A choice's values; a text, number or textarea's default (one value).
    pub values: Vec<String>,
}

impl Field {
    /// The value a field starts with: a text's default, a choice's first value, nothing for a required choice.
    pub fn default_value(&self) -> String {
        if self.kind.is_choice() && self.required {
            return String::new();
        }
        self.values.first().cloned().unwrap_or_default()
    }
}

/// A parsed template: its fields in order, and the text the values go into.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Template {
    pub fields: Vec<Field>,
    /// The User Context: the text before the first section.
    pub text: String,
    /// Per-step sections, in the file's order: the step's prompt key (`narrate`, `youtube`, …) and what is added
    /// to that step's shipped prompt for a project using this profile (§5b).
    pub sections: Vec<(String, String)>,
    /// The `## produce` rows: the Produce page's encoder rows, `(row, value)` as the page spells them (§5b),
    /// set on the project when the profile is chosen.
    pub produce: Vec<(String, String)>,
}

/// Read a template, or say every error with its line.
pub fn parse(source: &str) -> Result<Template, Vec<String>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut errors = Vec::new();
    let mut fields: Vec<Field> = Vec::new();
    let (text_from, block) = if lines.first().map(|l| l.trim_end()) == Some("---") {
        match lines.iter().skip(1).position(|l| l.trim_end() == "---") {
            Some(close) => (close + 2, &lines[1..close + 1]),
            None => {
                return Err(vec![
                    "line 1: the field block opened with --- is never closed".into(),
                ])
            }
        }
    } else {
        (0, &lines[0..0])
    };
    for (offset, line) in block.iter().enumerate() {
        let number = offset + 2;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match field_of(trimmed) {
            Ok(field) => {
                if fields.iter().any(|f| f.name == field.name) {
                    errors.push(format!("line {number}: `{}` is declared twice", field.name));
                } else {
                    fields.push(field);
                }
            }
            Err(why) => errors.push(format!("line {number}: {why}")),
        }
    }
    // The text, cut into the User Context (before the first `## step` line) and one section per step: a line
    // `## narrate` starts the section for that step; any other `##` line is ordinary text.
    let mut context: Vec<&str> = Vec::new();
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    for (offset, line) in lines[text_from.min(lines.len())..].iter().enumerate() {
        let heading = line
            .trim()
            .strip_prefix("## ")
            .map(str::trim)
            .filter(|key| crate::prompts::is_known(key) || *key == PRODUCE);
        match heading {
            Some(key) => {
                if sections.iter().any(|(held, _)| held == key) {
                    errors.push(format!(
                        "line {}: the section `{key}` appears twice",
                        text_from + offset + 1
                    ));
                }
                sections.push((key.to_string(), Vec::new()));
            }
            None => match sections.last_mut() {
                Some((_, body)) => body.push(line),
                None => context.push(line),
            },
        }
    }
    let text = context.join("\n");
    // `## produce` holds encoder rows, not prompt words: read and checked line by line.
    let mut produce: Vec<(String, String)> = Vec::new();
    let mut in_produce = false;
    for (offset, line) in lines[text_from.min(lines.len())..].iter().enumerate() {
        let number = text_from + offset + 1;
        if let Some(key) = line.trim().strip_prefix("## ").map(str::trim) {
            if crate::prompts::is_known(key) || key == PRODUCE {
                in_produce = key == PRODUCE;
                continue;
            }
        }
        if !in_produce || line.trim().is_empty() || line.trim().starts_with('#') {
            continue;
        }
        match produce_row(line.trim()) {
            Ok(row) => produce.push(row),
            Err(why) => errors.push(format!("line {number}: {why}")),
        }
    }
    let sections: Vec<(String, String)> = sections
        .into_iter()
        .filter(|(key, _)| key != PRODUCE)
        .map(|(key, body)| (key, body.join("\n").trim().to_string()))
        .collect();
    // Every `{{name}}` in the text must be a field above.
    for (offset, line) in lines[text_from.min(lines.len())..].iter().enumerate() {
        let mut rest = *line;
        while let Some(open) = rest.find("{{") {
            let after = &rest[open + 2..];
            let Some(close) = after.find("}}") else {
                errors.push(format!(
                    "line {}: `{{{{` is never closed",
                    text_from + offset + 1
                ));
                break;
            };
            let name = after[..close].trim();
            if !fields.iter().any(|f| f.name == name) {
                errors.push(format!(
                    "line {}: `{name}` in the text is not a field above",
                    text_from + offset + 1
                ));
            }
            rest = &after[close + 2..];
        }
    }
    if errors.is_empty() {
        Ok(Template {
            fields,
            text,
            sections,
            produce,
        })
    } else {
        Err(errors)
    }
}

/// The section that holds encoder rows rather than prompt words.
pub const PRODUCE: &str = "produce";

/// One `## produce` line: `Row: value`, the row and the value as the Produce page spells them.
fn produce_row(line: &str) -> Result<(String, String), String> {
    use crate::produce_screen as screen;
    // A `#` after the value is a comment (the ⓘ help lists each row's choices that way).
    let line = line.split(" #").next().unwrap_or(line);
    let (row, value) = line
        .split_once(':')
        .map(|(row, value)| (row.trim(), value.trim()))
        .ok_or("a produce line is `Row: value`, e.g. `Preset: veryslow`")?;
    let rows: Vec<&str> = screen::defaults().iter().map(|(row, _)| *row).collect();
    if !rows.contains(&row) {
        return Err(format!(
            "`{row}` is not a Produce row -- one of {}",
            rows.join(", ")
        ));
    }
    let tick = matches!(row, "Frame timing" | "Channels" | "Frame edges");
    let good = match row {
        _ if tick => matches!(value, "on" | "off"),
        "Translate" => {
            value == "none"
                || value
                    .split(',')
                    .map(str::trim)
                    .all(|language| screen::TRANSLATE_LANGUAGES.contains(&language))
        }
        "Game audio" => value.parse::<f64>().is_ok_and(|v| (0.0..=1.0).contains(&v)),
        "Quality (CRF)" => value
            .parse::<u32>()
            .is_ok_and(|v| (screen::CRF_MIN..=screen::CRF_MAX).contains(&v)),
        _ => screen::options(row).is_some_and(|options| options.contains(&value)),
    };
    if !good {
        let allowed = match row {
            _ if tick => "on or off".to_string(),
            "Translate" => format!(
                "none, or some of {}",
                screen::TRANSLATE_LANGUAGES.join(", ")
            ),
            "Game audio" => "a number from 0 to 1".to_string(),
            "Quality (CRF)" => format!("{} to {}", screen::CRF_MIN, screen::CRF_MAX),
            _ => screen::options(row)
                .map(|options| options.join(", "))
                .unwrap_or_default(),
        };
        return Err(format!("`{value}` is not a {row} -- {allowed}"));
    }
    Ok((row.to_string(), value.to_string()))
}

/// One field line: `name: kind [required] ["Label"] [= value | value …]`.
fn field_of(line: &str) -> Result<Field, String> {
    let (name, rest) = line
        .split_once(':')
        .ok_or("a field line is `name: kind …`")?;
    let name = name.trim();
    let named = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !named {
        return Err(format!(
            "`{name}` -- a field's name is lower case letters, digits and _, starting with a letter"
        ));
    }
    let (head, values) = match rest.split_once('=') {
        Some((head, values)) => (head, Some(values)),
        None => (rest, None),
    };
    let mut head = head.trim();
    let kind_word: String = head.chars().take_while(|c| !c.is_whitespace()).collect();
    let kind = Kind::of(&kind_word).ok_or_else(|| {
        format!("kind `{kind_word}` -- one of text, number, textarea, choice, choice+")
    })?;
    head = head[kind_word.len()..].trim_start();
    let mut required = false;
    if let Some(after) = head.strip_prefix("required") {
        required = true;
        head = after.trim_start();
    }
    let label = if let Some(quoted) = head.strip_prefix('"') {
        let (label, after) = quoted
            .split_once('"')
            .ok_or("the label's closing \" is missing")?;
        if !after.trim().is_empty() {
            return Err(format!(
                "`{}` after the label is not understood",
                after.trim()
            ));
        }
        label.to_string()
    } else if head.is_empty() {
        name.to_string()
    } else {
        return Err(format!(
            "`{head}` is not understood -- after the kind come `required` and a \"Label\""
        ));
    };
    let values: Vec<String> = values
        .map(|values| {
            values
                .split('|')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if kind.is_choice() && values.is_empty() {
        return Err(format!(
            "`{name}` is a {} with no values -- list them after =",
            kind.word()
        ));
    }
    if !kind.is_choice() && values.len() > 1 {
        return Err(format!(
            "`{name}` is {} and takes one default, not a list",
            kind.word()
        ));
    }
    Ok(Field {
        name: name.to_string(),
        kind,
        required,
        label,
        values,
    })
}

/// Is `value` one this field takes? A number field takes digits with one `.` or `,` and a leading `-`; a
/// `choice` only its listed values; an empty value is always taken (whether it may stay empty is
/// [`missing`]'s question).
pub fn accepts(field: &Field, value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return true;
    }
    match field.kind {
        Kind::Number => value
            .strip_prefix('-')
            .unwrap_or(value)
            .replace(',', ".")
            .parse::<f64>()
            .is_ok_and(|n| n.is_finite()),
        Kind::Choice => field.values.iter().any(|v| v == value),
        _ => true,
    }
}

/// The values a project holds for this template: its own where it has them, each field's default otherwise.
pub fn values_for(
    template: &Template,
    held: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    template
        .fields
        .iter()
        .map(|field| {
            let value = held
                .get(&field.name)
                .cloned()
                .unwrap_or_else(|| field.default_value());
            (field.name.clone(), value)
        })
        .collect()
}

/// The User Context: the text with every `{{name}}` replaced by its value. A line left holding nothing but
/// whitespace after the replacement is dropped (an empty optional field leaves no blank line).
pub fn fill(template: &Template, values: &BTreeMap<String, String>) -> String {
    fill_text(&template.text, values)
}

/// A step's section filled in the same way, `None` when the template has none for it (or it is empty).
pub fn section(
    template: &Template,
    key: &str,
    values: &BTreeMap<String, String>,
) -> Option<String> {
    let (_, body) = template.sections.iter().find(|(held, _)| held == key)?;
    let filled = fill_text(body, values);
    (!filled.trim().is_empty()).then_some(filled)
}

/// The section for step `key` of the profile the open project uses, filled in with the project's values:
/// what `settings::prompt_text` sends in place of the step's shipped prompt. `None` with no profile, a profile this
/// machine lacks or cannot read, or no section for the step.
pub fn active_section(paths: &Paths, key: &str) -> Option<String> {
    let project = crate::project::load(&crate::startup::open_dir()).ok()?;
    let used = project.profile?;
    let template = parse(&load(paths, &used.name).ok()?).ok()?;
    let values = values_for(&template, &used.values);
    section(&template, key, &values)
}

fn fill_text(text: &str, values: &BTreeMap<String, String>) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let had_field = line.contains("{{");
        let mut filled = String::new();
        let mut rest = line;
        while let Some(open) = rest.find("{{") {
            filled.push_str(&rest[..open]);
            let after = &rest[open + 2..];
            let Some(close) = after.find("}}") else {
                filled.push_str(&rest[open..]);
                rest = "";
                break;
            };
            let name = after[..close].trim();
            filled.push_str(values.get(name).map(String::as_str).unwrap_or(""));
            rest = &after[close + 2..];
        }
        filled.push_str(rest);
        if had_field && filled.trim().is_empty() {
            continue;
        }
        out.push(filled);
    }
    out.join("\n").trim_end().to_string()
}

/// The first required field left empty, in the form's order: what Prepare's ▶ refuses for (F0.14 S6).
pub fn first_missing<'a>(
    template: &'a Template,
    values: &BTreeMap<String, String>,
) -> Option<&'a Field> {
    template.fields.iter().find(|field| {
        field.required
            && values
                .get(&field.name)
                .is_none_or(|value| value.trim().is_empty())
    })
}

/// F0.14 S1's line under the template: what it declares.
pub fn summary(template: &Template) -> String {
    let mut keys: Vec<&str> = template
        .sections
        .iter()
        .map(|(key, _)| key.as_str())
        .collect();
    if !template.produce.is_empty() {
        keys.push(PRODUCE);
    }
    let sections = if keys.is_empty() {
        String::new()
    } else {
        format!("; sections for {}", keys.join(", "))
    };
    if template.fields.is_empty() {
        return format!("no fields -- the text is the User Context as it stands{sections}");
    }
    let each: Vec<String> = template
        .fields
        .iter()
        .map(|field| {
            format!(
                "{} ({}{})",
                field.label,
                field.kind.word(),
                if field.required { ", required" } else { "" }
            )
        })
        .collect();
    format!(
        "{} field(s): {}{sections}",
        template.fields.len(),
        each.join(", ")
    )
}

// ---- storage: one file per profile in the settings folder --------------------------------------------

/// `<config>/profiles/`.
pub fn dir(paths: &Paths) -> PathBuf {
    paths.config_dir.join("profiles")
}

/// A profile's file name: its name with `/` and control characters made `-`.
pub fn file_of(paths: &Paths, name: &str) -> PathBuf {
    let safe: String = name
        .trim()
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    dir(paths).join(format!("{safe}.txt"))
}

/// Every profile on this machine, by name, sorted.
pub fn list(paths: &Paths) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir(paths))
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.strip_suffix(".txt").map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

pub fn load(paths: &Paths, name: &str) -> Result<String, String> {
    let file = file_of(paths, name);
    std::fs::read_to_string(&file).map_err(|why| format!("{}: {why}", file.display()))
}

/// Write a profile, refusing a template with errors (S2): the first error is the answer.
pub fn save(paths: &Paths, name: &str, source: &str) -> Result<PathBuf, String> {
    if name.trim().is_empty() {
        return Err("a profile needs a name".to_string());
    }
    if let Err(errors) = parse(source) {
        return Err(errors[0].clone());
    }
    let file = file_of(paths, name);
    std::fs::create_dir_all(dir(paths))
        .map_err(|why| format!("{}: {why}", dir(paths).display()))?;
    std::fs::write(&file, source).map_err(|why| format!("{}: {why}", file.display()))?;
    Ok(file)
}

pub fn delete(paths: &Paths, name: &str) -> Result<(), String> {
    let file = file_of(paths, name);
    std::fs::remove_file(&file).map_err(|why| format!("{}: {why}", file.display()))
}
