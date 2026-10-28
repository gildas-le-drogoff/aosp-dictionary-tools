use std::io;
use std::path::Path;

use rustc_hash::FxHashSet;
use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

use crate::corpus::{self, keyboard_char};
use crate::lang::WiktionaryRules;
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

const PSEUDO_FORM_TAGS: &[&str] = &["table-tags", "inflection-template"];

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
    categories: Vec<Category>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Category {
    Name(String),
    Object { name: String },
}

impl Category {
    fn name(&self) -> &str {
        match self {
            Category::Name(name) | Category::Object { name } => name,
        }
    }
}

#[derive(Deserialize)]
struct Sense {
    #[serde(default)]
    tags: Vec<String>,
}

impl Entry {
    fn is_error_or_rare_variant(&self, rules: &WiktionaryRules) -> bool {
        let tagged = |s: &Sense, t: &str| s.tags.iter().any(|x| x == t);
        self.categories.iter().any(|c| {
            rules
                .error_categories
                .iter()
                .any(|p| c.name().starts_with(p))
        }) || (!self.senses.is_empty()
            && self
                .senses
                .iter()
                .all(|s| tagged(s, "alt-of") && rules.variant_tags.iter().any(|t| tagged(s, t))))
    }
}

#[derive(Deserialize)]
struct Form {
    form: String,
    #[serde(default)]
    tags: Vec<String>,
}

impl Form {
    fn is_pseudo(&self) -> bool {
        self.tags
            .iter()
            .any(|t| PSEUDO_FORM_TAGS.contains(&t.as_str()))
    }
}

#[derive(Default)]
pub struct Wiktionary {
    pub forms: FxHashSet<String>,

    pub paradigms: Vec<(String, Vec<String>)>,

    errors: FxHashSet<String>,
}

pub fn normalize(s: &str) -> String {
    s.nfc().map(keyboard_char).collect()
}

fn form_word(form: &str, pronouns: Option<&[&str]>) -> Option<String> {
    let form = normalize(form);
    let last = if let Some(pronouns) = pronouns {
        let last = form.split_whitespace().last()?;
        match last.split_once('\'') {
            Some((p, rest)) if pronouns.contains(&p) => rest,
            _ => last,
        }
    } else if form.contains(char::is_whitespace) {
        return None;
    } else {
        &form
    };
    is_word(last).then(|| last.to_owned())
}

pub fn load(path: &Path, rules: &WiktionaryRules) -> io::Result<Wiktionary> {
    let needle = format!("\"lang_code\": \"{}\"", rules.lang_code);
    let mut w = corpus::process(
        path,
        "wiktionnaire",
        Wiktionary::default,
        |mut acc, chunk| {
            for line in chunk.lines() {
                let head = &line[..line.floor_char_boundary(400)];
                if rules.prefilter && !head.contains(&needle) {
                    continue;
                }
                let Ok(e) = serde_json::from_str::<Entry>(line) else {
                    continue;
                };
                if e.lang_code != rules.lang_code || SKIPPED_POS.contains(&e.pos.as_str()) {
                    continue;
                }
                let word = normalize(&e.word);
                if e.is_error_or_rare_variant(rules) {
                    acc.errors.insert(word);
                    continue;
                }
                if !is_word(&word) {
                    continue;
                }
                let pronouns = rules.pronouns.filter(|_| e.pos == "verb");
                let mut common = Vec::new();
                for f in &e.forms {
                    if !f.is_pseudo()
                        && let Some(w) = form_word(&f.form, pronouns)
                        && w != word
                    {
                        if !(rules.rare_form)(&f.tags) {
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
    use crate::lang::en::EN;
    use crate::lang::fr::FR;

    #[test]
    fn forms() {
        let verb = FR.wiktionary.pronouns;
        assert_eq!(form_word("que j’aime", verb).as_deref(), Some("aime"));
        assert_eq!(
            form_word("il/elle/on mange", verb).as_deref(),
            Some("mange")
        );
        assert_eq!(form_word("en ayant mangé", verb).as_deref(), Some("mangé"));
        assert_eq!(form_word("je m’appelle", verb).as_deref(), Some("appelle"));
        assert_eq!(
            form_word("aujourd’hui", None).as_deref(),
            Some("aujourd'hui")
        );
        assert_eq!(form_word("pommes de terre", None), None);
    }

    #[test]
    fn english_entry() {
        let line = r#"{"word": "colour", "lang_code": "en", "pos": "verb", "forms": [{"form": "colours", "tags": ["present", "singular", "third-person"]}, {"form": "coloured", "tags": ["past"]}, {"form": "color", "tags": ["alternative", "US"]}, {"form": "no-table-tags", "tags": ["table-tags"]}], "senses": [{"tags": ["transitive"]}], "categories": [{"name": "British English", "kind": "other"}]}"#;
        let e: Entry = serde_json::from_str(line).unwrap();
        assert!(!e.is_error_or_rare_variant(&EN.wiktionary));
        let forms: Vec<&Form> = e.forms.iter().filter(|f| !f.is_pseudo()).collect();
        assert_eq!(forms.len(), 3);
        assert!((EN.wiktionary.rare_form)(&forms[2].tags));
        assert!(!(EN.wiktionary.rare_form)(&forms[1].tags));

        let line = r#"{"word": "recieve", "lang_code": "en", "pos": "verb", "senses": [{"tags": ["alt-of", "misspelling"]}]}"#;
        let e: Entry = serde_json::from_str(line).unwrap();
        assert!(e.is_error_or_rare_variant(&EN.wiktionary));
    }
}
