//! Numbers said as words for the aligner (spec/04-prepare.md F1.5 S4).
//!
//! The forced aligner times what a token sounds like, and a token in digits has no sound it knows: on 08-dex 18-41-59
//! it gave "0,81" (said "null Komma acht eins", 1.5 s) 80 ms and let "45" take the "USDC" after it, so a cut after
//! the number cut the number off. Asked with the number spelled out, the same window times both right. [`spell`] writes
//! the text the aligner is asked with; [`merge`] folds the spoken words it answers back into the token as written, so
//! `words.aligned.json` keeps one word per written token and nothing after the aligner sees a difference.
//!
//! Spelled in German and English only; any other language is asked as written.

use crate::requests::Word;

/// One written token and how many whitespace words it became in the spelled text (1 when it was left alone).
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub written: String,
    pub spoken: Vec<String>,
}

impl Token {
    pub fn spelled(&self) -> bool {
        self.spoken.len() != 1 || self.spoken[0] != self.written
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Lang {
    De,
    En,
}

fn lang(language: &str) -> Option<Lang> {
    match language.trim().to_lowercase().as_str() {
        "de" | "deu" | "ger" | "german" | "deutsch" => Some(Lang::De),
        "en" | "eng" | "english" => Some(Lang::En),
        _ => None,
    }
}

/// The text to ask the aligner with, and the tokens it came from. Unchanged when the language is not spelled or no
/// token holds a digit.
pub fn spell(text: &str, language: &str) -> (String, Vec<Token>) {
    let lang = lang(language);
    let tokens: Vec<Token> = text
        .split_whitespace()
        .map(|written| Token {
            written: written.to_string(),
            spoken: match lang {
                Some(lang) if written.chars().any(|c| c.is_ascii_digit()) => spell_token(written, lang),
                _ => vec![written.to_string()],
            },
        })
        .collect();
    if !tokens.iter().any(Token::spelled) {
        return (text.to_string(), tokens);
    }
    let spoken = tokens
        .iter()
        .flat_map(|t| t.spoken.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    (spoken, tokens)
}

/// A word as the aligner names it: lower case, letters, digits and apostrophes ("'26", "it's"); other punctuation
/// dropped ("0,81" → "081"). Read off qwen3-aligner's answers over 260 recordings.
fn key(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Fold the aligner's words for a spelled number back into one word named as written ("0,81" → "081", what the
/// aligner answers for the written token), timed from its first spoken word's start to its last one's end.
///
/// The answer is walked against the spoken words in order; a word that matches none of the next few is passed
/// through untouched, so an aligner that splits or joins words differently loses nothing, it only keeps a number
/// it could not follow as the words it answered.
pub fn merge(words: Vec<Word>, tokens: &[Token]) -> Vec<Word> {
    if !tokens.iter().any(Token::spelled) {
        return words;
    }
    // (token index, key) for every spoken word, in order.
    let expected: Vec<(usize, String)> = tokens
        .iter()
        .enumerate()
        .flat_map(|(i, t)| t.spoken.iter().map(move |s| (i, key(s))))
        .filter(|(_, k)| !k.is_empty())
        .collect();
    const LOOKAHEAD: usize = 8;
    let mut at = 0;
    let mut owner: Vec<Option<usize>> = Vec::with_capacity(words.len());
    for word in &words {
        let k = key(&word.word);
        let found = (at..(at + LOOKAHEAD).min(expected.len())).find(|&j| expected[j].1 == k);
        match found {
            Some(j) => {
                owner.push(Some(expected[j].0));
                at = j + 1;
            }
            None => owner.push(None),
        }
    }
    let mut out: Vec<Word> = Vec::with_capacity(words.len());
    let mut last: Option<usize> = None;
    for (word, owner) in words.into_iter().zip(owner) {
        match owner {
            Some(i) if tokens[i].spelled() => {
                if last == Some(i) {
                    let held = out.last_mut().expect("a word was pushed for this token");
                    held.start_sample = held.start_sample.min(word.start_sample);
                    held.end_sample = held.end_sample.max(word.end_sample);
                } else {
                    out.push(Word {
                        word: key(&tokens[i].written),
                        start_sample: word.start_sample,
                        end_sample: word.end_sample,
                    });
                }
                last = Some(i);
            }
            _ => {
                out.push(word);
                last = None;
            }
        }
    }
    out
}

/// One token with a digit in it, as spoken words. Leading and trailing punctuation stays on the first and last word
/// so the aligner reads the sentence as before.
fn spell_token(token: &str, lang: Lang) -> Vec<String> {
    let lead: String = token
        .chars()
        .take_while(|c| !c.is_alphanumeric() && !matches!(c, '€' | '$'))
        .collect();
    let rest = &token[lead.len()..];
    let core_len = rest
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_alphanumeric() || *c == '%' || *c == '€' || *c == '$')
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let (core, tail) = rest.split_at(core_len);
    let mut words = spell_core(core, lang);
    if words.is_empty() {
        return vec![token.to_string()];
    }
    if !lead.is_empty() {
        words[0] = format!("{lead}{}", words[0]);
    }
    if !tail.is_empty() {
        let n = words.len() - 1;
        words[n].push_str(tail);
    }
    words
}

fn spell_core(core: &str, lang: Lang) -> Vec<String> {
    let (decimal, group) = match lang {
        Lang::De => (',', '.'),
        Lang::En => ('.', ','),
    };
    let mut out = Vec::new();
    let mut unit_after: Option<&str> = None;
    let chars: Vec<char> = core.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            let start = i;
            // digits with group separators: 1.000.000 (de) / 1,000,000 (en)
            let grouped = |at: usize| {
                chars[at] == group
                    && chars.get(at + 1..at + 4).is_some_and(|d| d.iter().all(char::is_ascii_digit))
                    && !chars.get(at + 4).is_some_and(char::is_ascii_digit)
            };
            while i < chars.len() && (chars[i].is_ascii_digit() || grouped(i)) {
                i += 1;
            }
            let whole: String = chars[start..i].iter().filter(|c| c.is_ascii_digit()).collect();
            // four bare digits are read as a year where a year is what they sound like ("1997", "2023")
            let plain = i - start == whole.len();
            let fresh = start == 0 || !chars[start - 1].is_ascii_digit();
            match (plain && fresh && whole.len() == 4).then(|| year(&whole, lang)).flatten() {
                Some(words) => out.extend(words),
                None => out.extend(cardinal(&whole, lang)),
            }
            if i + 1 < chars.len() && chars[i] == decimal && chars[i + 1].is_ascii_digit() {
                out.push(match lang {
                    Lang::De => "Komma",
                    Lang::En => "point",
                }
                .to_string());
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    out.push(digit(chars[i], lang).to_string());
                    i += 1;
                }
            }
            // a dot between digits that is neither a decimal nor a thousands group is a version's "Punkt" / "point"
            // (0.9.0, 0.8-Reihe); the next part reads as a number of its own
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                out.push(match lang {
                    Lang::De => "Punkt",
                    Lang::En => "point",
                }
                .to_string());
                i += 1;
                continue;
            }
            // English ordinals and decades: 1st, 14th, 90s
            if lang == Lang::En {
                let letters: String = chars[i..].iter().take_while(|c| c.is_alphabetic()).collect();
                let lower = letters.to_lowercase();
                let n: u64 = whole.parse().unwrap_or(0);
                if matches!(lower.as_str(), "st" | "nd" | "rd" | "th") {
                    if let Some(last) = out.last_mut() {
                        *last = en_ordinal(last);
                    }
                    i += letters.chars().count();
                } else if lower == "s" && n % 10 == 0 && n > 0 {
                    if let Some(last) = out.last_mut() {
                        *last = en_plural(last);
                    }
                    i += 1;
                }
            }
            if let Some(unit) = unit_after.take() {
                out.push(unit.to_string());
            }
        } else if c == '%' {
            out.push(match lang {
                Lang::De => "Prozent",
                Lang::En => "percent",
            }
            .to_string());
            i += 1;
        } else if c == '€' || c == '$' {
            let unit = match (c, lang) {
                ('€', _) => "Euro",
                (_, Lang::De) => "Dollar",
                (_, Lang::En) => "dollars",
            };
            // "$5" is said "five dollars"; "5€" reads in order.
            if chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                unit_after = Some(unit);
            } else {
                out.push(unit.to_string());
            }
            i += 1;
        } else if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_alphabetic() {
                i += 1;
            }
            out.push(chars[start..i].iter().collect());
        } else {
            // a hyphen, slash or dot between parts: a pause, not a word
            i += 1;
        }
    }
    out
}

fn digit(c: char, lang: Lang) -> &'static str {
    let n = c.to_digit(10).unwrap_or(0) as usize;
    match lang {
        Lang::De => ["null", "eins", "zwei", "drei", "vier", "fünf", "sechs", "sieben", "acht", "neun"][n],
        Lang::En => ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"][n],
    }
}

/// Four digits as a year is said, when it is said differently from the number: German 1100-1999
/// ("neunzehnhundertsiebenundneunzig"), English 1100-1999 and 2010-2099 ("nineteen ninety seven", "twenty twenty three").
fn year(digits: &str, lang: Lang) -> Option<Vec<String>> {
    let n: u64 = digits.parse().ok()?;
    let (hi, lo) = (n / 100, n % 100);
    match lang {
        Lang::De if (1100..2000).contains(&n) => {
            let mut word = de_below_thousand(hi);
            word.push_str("hundert");
            word.push_str(&de_below_thousand(lo));
            if lo == 1 {
                word.push('s');
            }
            Some(vec![word])
        }
        Lang::En if (1100..2000).contains(&n) || (2010..2100).contains(&n) => {
            let mut words = en_below_thousand(hi);
            match lo {
                0 => words.push("hundred".into()),
                1..=9 => {
                    words.push("oh".into());
                    words.extend(en_below_thousand(lo));
                }
                _ => words.extend(en_below_thousand(lo)),
            }
            Some(words)
        }
        _ => None,
    }
}

fn en_ordinal(word: &str) -> String {
    match word {
        "one" => "first".into(),
        "two" => "second".into(),
        "three" => "third".into(),
        "five" => "fifth".into(),
        "eight" => "eighth".into(),
        "nine" => "ninth".into(),
        "twelve" => "twelfth".into(),
        w if w.ends_with('y') => format!("{}ieth", &w[..w.len() - 1]),
        w => format!("{w}th"),
    }
}

fn en_plural(word: &str) -> String {
    match word.strip_suffix('y') {
        Some(stem) => format!("{stem}ies"),
        None => format!("{word}s"),
    }
}

/// A whole number as words; past a trillion, or with a leading zero (a code, "007"), digit by digit.
fn cardinal(digits: &str, lang: Lang) -> Vec<String> {
    let by_digit = || digits.chars().map(|c| digit(c, lang).to_string()).collect();
    if digits.len() > 1 && digits.starts_with('0') {
        return by_digit();
    }
    match digits.parse::<u64>() {
        Ok(n) if n < 1_000_000_000_000 => match lang {
            Lang::De => de(n),
            Lang::En => en(n),
        },
        _ => by_digit(),
    }
}

fn de(n: u64) -> Vec<String> {
    if n == 0 {
        return vec!["null".into()];
    }
    if n == 1 {
        return vec!["eins".into()];
    }
    let mut out = Vec::new();
    let mut rest = n;
    for (size, one, many) in [
        (1_000_000_000, "eine Milliarde", "Milliarden"),
        (1_000_000, "eine Million", "Millionen"),
    ] {
        let k = rest / size;
        rest %= size;
        if k == 1 {
            out.extend(one.split(' ').map(String::from));
        } else if k > 1 {
            out.push(de_below_million(k));
            out.push(many.into());
        }
    }
    if rest > 0 {
        out.push(de_below_million(rest));
    }
    // a 1 that ends the number is "eins" (hunderteins); inside a word it is "ein" (einhundert, einundzwanzig)
    if n % 100 == 1 {
        if let Some(last) = out.last_mut() {
            last.push('s');
        }
    }
    out
}

/// German writes everything under a million as one word.
fn de_below_million(n: u64) -> String {
    let thousands = n / 1000;
    let rest = n % 1000;
    let mut s = String::new();
    // said "tausend", "hundert", not "eintausend", "einhundert"
    if thousands == 1 {
        s.push_str("tausend");
    } else if thousands > 0 {
        s.push_str(&de_below_thousand(thousands));
        s.push_str("tausend");
    }
    if rest > 0 {
        s.push_str(&de_below_thousand(rest));
    }
    s
}

fn de_below_thousand(n: u64) -> String {
    const ONES: [&str; 20] = [
        "", "ein", "zwei", "drei", "vier", "fünf", "sechs", "sieben", "acht", "neun", "zehn", "elf", "zwölf",
        "dreizehn", "vierzehn", "fünfzehn", "sechzehn", "siebzehn", "achtzehn", "neunzehn",
    ];
    const TENS: [&str; 10] = [
        "", "", "zwanzig", "dreißig", "vierzig", "fünfzig", "sechzig", "siebzig", "achtzig", "neunzig",
    ];
    let mut s = String::new();
    let hundreds = (n / 100) as usize;
    let rest = (n % 100) as usize;
    if hundreds > 1 {
        s.push_str(ONES[hundreds]);
    }
    if hundreds > 0 {
        s.push_str("hundert");
    }
    if rest > 0 && rest < 20 {
        s.push_str(ONES[rest]);
    } else if rest >= 20 {
        if rest % 10 > 0 {
            s.push_str(ONES[rest % 10]);
            s.push_str("und");
        }
        s.push_str(TENS[rest / 10]);
    }
    s
}

fn en(n: u64) -> Vec<String> {
    if n == 0 {
        return vec!["zero".into()];
    }
    let mut out = Vec::new();
    let mut rest = n;
    for (size, name) in [(1_000_000_000, "billion"), (1_000_000, "million"), (1000, "thousand")] {
        let k = rest / size;
        rest %= size;
        if k > 0 {
            out.extend(en_below_thousand(k));
            out.push(name.into());
        }
    }
    out.extend(en_below_thousand(rest));
    out
}

fn en_below_thousand(n: u64) -> Vec<String> {
    const ONES: [&str; 20] = [
        "", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
        "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
    ];
    const TENS: [&str; 10] = [
        "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    let mut out = Vec::new();
    let hundreds = (n / 100) as usize;
    let rest = (n % 100) as usize;
    if hundreds > 0 {
        out.push(ONES[hundreds].to_string());
        out.push("hundred".into());
    }
    if rest > 0 && rest < 20 {
        out.push(ONES[rest].to_string());
    } else if rest >= 20 {
        out.push(TENS[rest / 10].to_string());
        if rest % 10 > 0 {
            out.push(ONES[rest % 10].to_string());
        }
    }
    out
}
