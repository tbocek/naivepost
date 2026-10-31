//! §12 entry 19's second half: the clamp's drop count becomes a line the user can act on.

use crate::cut::Fx;
use crate::cut_clamp::Clamped;

const NAMED_MAX: usize = 6;

pub fn describe(fx: &Fx) -> String {
    let kind = match fx.effect_kind() {
        // A stop is a speed whose rate is 0 (§1's column), and §9 refuses it as a STOP, so the log says stop.
        Some(_) if crate::fx_record::is_stop(fx) => "stop".to_string(),
        Some(kind) => crate::fx_record::label_of(kind).to_lowercase(),
        None => format!("{} effect", fx.kind),
    };
    format!("{kind} at {}", crate::tools::mm_ss(fx.t))
}

pub fn dropped_log(clamped: &Clamped) -> Option<String> {
    if clamped.dropped == 0 {
        return None;
    }
    Some(crate::cut_clamp::log_line(clamped.dropped))
}

pub fn dropped_names(clamped: &Clamped) -> Option<String> {
    if clamped.dropped == 0 || clamped.dropped_lines.is_empty() {
        return None;
    }
    let mut tail = String::new();
    let mut named: Vec<&str> = Vec::with_capacity(NAMED_MAX);
    for (index, note) in clamped.dropped_lines.iter().enumerate() {
        if index < NAMED_MAX {
            named.push(note.as_str());
        } else if tail.is_empty() {
            tail = format!("... and {} more", clamped.dropped_lines.len() - NAMED_MAX);
        }
    }
    if !tail.is_empty() {
        named.push(tail.as_str());
    }
    Some(named.join(", "))
}
