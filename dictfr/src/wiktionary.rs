use std::io;
use std::path::Path;

use rustc_hash::FxHashSet;
use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

use crate::corpus::{self, keyboard_char};
use crate::wordlist::is_word;

const SKIPPED_POS: &[&str] = &[
    "typographic variant",
    "prefix",
    "suffix",
    "interfix",
    "infix",
    "circumfix",
    "symbol",
    "character",
    "phrase",
    "proverb",
];

const INFLECTED_POS: &[&str] = &["verb", "noun", "adj"];

#[derive(Deserialize)]
struct Entry {
    word: String,
    lang_code: String,
    #[serde(default)]
    pos: String,
    #[serde(default)]
    forms: Vec<Form>,
    #[serde(default)]
    senses: Vec<Sense>,
    #[serde(default)]
    categories: Vec<String>,
}

#[derive(Deserialize)]
struct Sense {
    #[serde(default)]
    tags: Vec<String>,
}

impl Entry {
    fn is_error_or_rare_variant(&self) -> bool {
        let tagged = |s: &Sense, t: &str| s.tags.iter().any(|x| x == t);
        self.categories.iter().any(|c| c.starts_with("Fautes "))
            || (!self.senses.is_empty()
                && self.senses.iter().all(|s| {
                    tagged(s, "alt-of")
                        && (tagged(s, "rare") || tagged(s, "obsolete") || tagged(s, "archaic"))
                }))
    }
}

#[derive(Deserialize)]
struct Form {
    form: String,
    #[serde(default)]
    tags: Vec<String>,
}

impl Form {
    fn is_rare(&self) -> bool {
        let has = |t: &str| self.tags.iter().any(|x| x == t);
        has("archaic")
            || has("rare")
            || has("obsolete")
            || (has("subjunctive") && has("imperfect"))
            || (has("indicative")
                && has("past")
                && !has("multiword-construction")
                && !has("anterior"))
    }
}

#[derive(Default)]
pub struct Wiktionary {
    pub forms: FxHashSet<String>,

    pub paradigms: Vec<(String, Vec<String>)>,

    errors: FxHashSet<String>,
}

fn normalize(s: &str) -> String {
    s.nfc().map(keyboard_char).collect()
}

fn form_word(form: &str, verb: bool) -> Option<String> {
    let form = normalize(form);
    let last = if verb {
        let last = form.split_whitespace().last()?;
        match last.split_once('\'') {
            Some(("j" | "m" | "t" | "s" | "n" | "l" | "qu", rest)) => rest,
            _ => last,
        }
    } else if form.contains(char::is_whitespace) {
        return None;
    } else {
        &form
    };
    is_word(last).then(|| last.to_owned())
}

pub fn load(path: &Path) -> io::Result<Wiktionary> {
    let mut w = corpus::process(
        path,
        "wiktionnaire",
        Wiktionary::default,
        |mut acc, chunk| {
            for line in chunk.lines() {
                let head = &line[..line.floor_char_boundary(400)];
                if !head.contains("\"lang_code\": \"fr\"") {
                    continue;
                }
                let Ok(e) = serde_json::from_str::<Entry>(line) else {
                    continue;
                };
                if e.lang_code != "fr" || SKIPPED_POS.contains(&e.pos.as_str()) {
                    continue;
                }
                let word = normalize(&e.word);
                if e.is_error_or_rare_variant() {
                    acc.errors.insert(word);
                    continue;
                }
                if !is_word(&word) {
                    continue;
                }
                let verb = e.pos == "verb";
                let mut common = Vec::new();
                for f in &e.forms {
                    if let Some(w) = form_word(&f.form, verb)
                        && w != word
                    {
                        if !f.is_rare() {
                            common.push(w.clone());
                        }
                        acc.forms.insert(w);
                    }
                }
                acc.forms.insert(word.clone());
                common.sort_unstable();
                common.dedup();
                if !common.is_empty() && INFLECTED_POS.contains(&e.pos.as_str()) {
                    acc.paradigms.push((word, common));
                }
            }
            acc
        },
        |mut a, mut b| {
            if a.forms.len() < b.forms.len() {
                std::mem::swap(&mut a, &mut b);
            }
            a.forms.extend(b.forms);
            a.paradigms.extend(b.paradigms);
            a.errors.extend(b.errors);
            a
        },
    )?;
    let errors = std::mem::take(&mut w.errors);
    w.forms.retain(|f| !errors.contains(f));
    for (_, forms) in &mut w.paradigms {
        forms.retain(|f| !errors.contains(f));
    }
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms() {
        assert_eq!(form_word("que j’aime", true).as_deref(), Some("aime"));
        assert_eq!(
            form_word("il/elle/on mange", true).as_deref(),
            Some("mange")
        );
        assert_eq!(form_word("en ayant mangé", true).as_deref(), Some("mangé"));
        assert_eq!(form_word("je m’appelle", true).as_deref(), Some("appelle"));
        assert_eq!(
            form_word("aujourd’hui", false).as_deref(),
            Some("aujourd'hui")
        );
        assert_eq!(form_word("pommes de terre", false), None);
    }
}
