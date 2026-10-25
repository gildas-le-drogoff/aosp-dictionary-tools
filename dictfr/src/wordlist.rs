use std::fs;
use std::io;
use std::path::Path;

use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::corpus::{self, ENGLISH, FRENCH, MID, Source, TOTAL, Unigrams, is_letter};

pub const MAX_WORDS: usize = 300_000;
const MIN_COUNT: u64 = 5;
const MAX_CANDIDATES: usize = 700_000;
const CASE_SHARE: f64 = 0.25;
const ONE_LETTER: &[&str] = &["a", "à", "y", "ô"];
const MAX_LEN: usize = 30;

pub const VARIANTS_CHECKED: usize = 3;

const MAX_ENGLISH_SHARE: f64 = 0.5;

const MIN_LANGUAGE_EVIDENCE: u64 = 20;

const PROPER_NOUN_SHARE: f64 = 0.9;
const PROPER_NOUN_MIN_COUNT: u64 = 50;

const NAME_UNEDITED_CAP: f64 = 4.0;

const COMPLETION_MIN_PPM: f64 = 1.0;
const MAX_COMPLETIONS: usize = 100_000;

const FREQ_MAX: f64 = 221.0;

pub const FREQ_FLOOR_PPM: f64 = 5e-4;

pub struct KeyStats {
    pub score: f64,

    pub raw: u64,
    pub per_source: Vec<u64>,

    pub ppm: Vec<f64>,

    pub lang: [u64; 2],

    pub variants: Vec<(String, f64)>,
}

impl KeyStats {
    fn french_enough(&self) -> bool {
        let [fr, en] = self.lang;
        fr + en < MIN_LANGUAGE_EVIDENCE || en as f64 <= MAX_ENGLISH_SHARE * (fr + en) as f64
    }
}

pub type Keys = FxHashMap<String, KeyStats>;

pub struct Entry<'a> {
    pub ppm: f64,
    pub form: &'a str,
    pub key: &'a str,
}

fn read_lines(path: &Path) -> io::Result<Vec<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text
            .lines()
            .map(|l| l.split('#').next().unwrap().trim())
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

pub fn read_list(path: &Path) -> io::Result<FxHashSet<String>> {
    Ok(read_lines(path)?.into_iter().collect())
}

pub fn read_additions(path: &Path) -> io::Result<FxHashMap<String, f64>> {
    let mut out = FxHashMap::default();
    for line in read_lines(path)? {
        let mut fields = line.split_whitespace();
        let form: String = fields
            .next()
            .unwrap_or_default()
            .nfc()
            .map(corpus::keyboard_char)
            .collect();
        let ppm = match fields.next() {
            Some(f) => f.parse().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} : « {line} »", path.display()),
                )
            })?,
            None => 0.0,
        };
        out.insert(form, ppm);
    }
    Ok(out)
}

fn add_variant(variants: &mut Vec<(String, f64)>, form: String, value: f64) {
    match variants.iter_mut().find(|(f, _)| *f == form) {
        Some((_, v)) => *v += value,
        None => variants.push((form, value)),
    }
}

fn sort_variants(variants: &mut [(String, f64)]) {
    variants.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
}

pub fn merge_sources(
    sources: &[Source],
    unigrams: Vec<Unigrams>,
    additions: &FxHashMap<String, f64>,
) -> Keys {
    let mut keys = Keys::default();
    let new_stats = || KeyStats {
        score: 0.0,
        raw: 0,
        per_source: vec![0; sources.len()],
        ppm: vec![0.0; sources.len()],
        lang: [0; 2],
        variants: Vec::new(),
    };
    for (i, (src, uni)) in sources.iter().zip(unigrams).enumerate() {
        let total = uni.values().map(|n| n[TOTAL]).sum::<u64>() as f64;
        for (form, n) in uni {
            let stats = keys.entry(form.to_lowercase()).or_insert_with(new_stats);
            let ppm = src.weight * 1e6 * n[TOTAL] as f64 / total;
            stats.score += ppm;
            stats.ppm[i] += ppm;
            stats.raw += n[TOTAL];
            stats.per_source[i] += n[TOTAL];
            stats.lang[0] += n[FRENCH];
            stats.lang[1] += n[ENGLISH];
            let mid = if n[MID] > 0 {
                n[MID] as f64
            } else {
                n[TOTAL] as f64 * 1e-3
            };
            add_variant(&mut stats.variants, form, src.weight * 1e6 * mid / total);
        }
    }
    for (form, &ppm) in additions {
        let stats = keys.entry(form.to_lowercase()).or_insert_with(new_stats);
        stats.score = stats.score.max(ppm);
        add_variant(&mut stats.variants, form.clone(), 1e-6);
    }
    keys.par_iter_mut()
        .for_each(|(_, s)| sort_variants(&mut s.variants));
    keys
}

pub fn is_word(s: &str) -> bool {
    s.split(['\'', '-'])
        .all(|part| !part.is_empty() && part.chars().all(is_letter))
}

fn deaccent(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.nfd().filter(|&c| !is_combining_mark(c)) {
        match c {
            'œ' => out.push_str("oe"),
            'Œ' => out.push_str("OE"),
            'æ' => out.push_str("ae"),
            'Æ' => out.push_str("AE"),
            _ => out.push(c),
        }
    }
    out
}

pub fn candidates<'a>(
    keys: &'a Keys,
    additions: &FxHashMap<String, f64>,
    exclusions: &FxHashSet<String>,
) -> Vec<&'a str> {
    let added: FxHashSet<String> = additions.keys().map(|a| a.to_lowercase()).collect();
    let eligible = |key: &str| {
        let len = key.chars().count();
        is_word(key)
            && len <= MAX_LEN
            && (len > 1 || ONE_LETTER.contains(&key))
            && !exclusions.contains(key)
    };
    let mut c: Vec<(&str, f64)> = keys
        .par_iter()
        .filter(|(k, s)| (s.raw >= MIN_COUNT || added.contains(*k)) && eligible(k))
        .map(|(k, s)| (k.as_str(), s.score))
        .collect();
    c.par_sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    c.truncate(MAX_CANDIDATES);
    c.into_iter().map(|(k, _)| k).collect()
}

pub fn checked_forms<'a>(key: &'a str, stats: &'a KeyStats) -> impl Iterator<Item = &'a str> {
    std::iter::once(key).chain(
        stats
            .variants
            .iter()
            .take(VARIANTS_CHECKED)
            .map(|(f, _)| f.as_str())
            .filter(move |f| *f != key),
    )
}

pub struct Validation {
    pub valid: FxHashSet<String>,

    pub folds: Vec<(String, String)>,

    pub accepted: Vec<(String, &'static str, f64)>,
}

pub fn validate<'a>(
    keys: &'a Keys,
    candidates: &[&'a str],
    hunspell: &FxHashSet<&str>,
    wiktionary: &FxHashSet<String>,
    sources: &[Source],
    additions: &FxHashMap<String, f64>,
    exclusions: &FxHashSet<String>,
) -> Validation {
    let allowed = |f: &str| !exclusions.contains(f);
    let mut valid: FxHashSet<String> = FxHashSet::default();
    let mut has_valid: FxHashSet<&str> = FxHashSet::default();
    for &key in candidates {
        for f in checked_forms(key, &keys[key]) {
            if (hunspell.contains(f) || additions.contains_key(f)) && allowed(f) {
                valid.insert(f.to_owned());
                has_valid.insert(key);
            }
        }
    }

    let mut by_base: FxHashMap<String, &str> = FxHashMap::default();
    for &key in candidates {
        if has_valid.contains(key) {
            index(keys, &mut by_base, key);
        }
    }
    let edited: Vec<usize> = (0..sources.len()).filter(|&i| sources[i].edited).collect();

    let mut folds = Vec::new();
    let mut accepted = Vec::new();
    for &key in candidates {
        let stats = &keys[key];

        let accented = by_base
            .get(&deaccent(key))
            .filter(|&&t| t != key && keys[t].score >= stats.score)
            .copied();
        if let Some(t) = accented {
            if !has_valid.contains(key) {
                folds.push((key.to_owned(), t.to_owned()));
            }
            continue;
        }
        let french = stats.french_enough();
        let from_wiktionary: Vec<&str> = checked_forms(key, stats)
            .filter(|f| !valid.contains(*f) && wiktionary.contains(*f) && allowed(f))
            .collect();
        if french && !from_wiktionary.is_empty() {
            for f in from_wiktionary {
                valid.insert(f.to_owned());
                accepted.push((f.to_owned(), "wiktionnaire", stats.score));
            }
            if has_valid.insert(key) {
                index(keys, &mut by_base, key);
            }
            continue;
        }
        if has_valid.contains(key) {
            continue;
        }

        let total: f64 = stats.variants.iter().map(|v| v.1).sum();
        if let Some((best, v)) = stats.variants.first()
            && best.chars().any(char::is_uppercase)
            && *v >= PROPER_NOUN_SHARE * total
            && edited.iter().map(|&i| stats.per_source[i]).sum::<u64>() >= PROPER_NOUN_MIN_COUNT
            && french
            && allowed(best)
        {
            valid.insert(best.clone());
            accepted.push((best.clone(), "nom propre", stats.score));
        }
    }
    Validation {
        valid,
        folds,
        accepted,
    }
}

fn index<'a>(keys: &Keys, by_base: &mut FxHashMap<String, &'a str>, key: &'a str) {
    let base = deaccent(key);
    if by_base
        .get(&base)
        .is_none_or(|&k| keys[k].score < keys[key].score)
    {
        by_base.insert(base, key);
    }
}

pub fn apply_folds(keys: &mut Keys, folds: &[(String, String)]) {
    for (src, dst) in folds {
        let Some(s) = keys.remove(src) else { continue };
        let t = keys.get_mut(dst).expect("cible de rattachement");
        t.score += s.score;
        t.raw += s.raw;
        for (a, b) in t.per_source.iter_mut().zip(&s.per_source) {
            *a += b;
        }
        for (a, b) in t.ppm.iter_mut().zip(&s.ppm) {
            *a += b;
        }
        t.lang[0] += s.lang[0];
        t.lang[1] += s.lang[1];

        let bases: Vec<String> = t.variants.iter().map(|(f, _)| deaccent(f)).collect();
        for (f, v) in s.variants {
            let base = deaccent(&f);
            if let Some(i) = bases.iter().position(|b| *b == base) {
                t.variants[i].1 += v;
            }
        }
        sort_variants(&mut t.variants);
    }
}

fn choose_forms<'a>(
    key: &'a str,
    stats: &'a KeyStats,
    valid: &FxHashSet<&str>,
) -> Vec<(&'a str, f64)> {
    let var = &stats.variants;
    let total: f64 = var.iter().map(|v| v.1).sum();
    let get = |form: &str| var.iter().find(|v| v.0 == form).map_or(0.0, |v| v.1);
    let lower_share = if total > 0.0 { get(key) / total } else { 1.0 };
    let caps: Vec<&(String, f64)> = var
        .iter()
        .filter(|(f, _)| f != key && valid.contains(f.as_str()))
        .collect();

    let mut forms = Vec::new();
    if valid.contains(key) && lower_share >= CASE_SHARE {
        forms.push((key, lower_share));
    }
    if let Some((f, v)) = caps.first()
        && v / total >= CASE_SHARE
    {
        forms.push((f.as_str(), v / total));
    }
    if !forms.is_empty() {
        let norm: f64 = forms.iter().map(|f| f.1).sum();
        return forms.into_iter().map(|(f, s)| (f, s / norm)).collect();
    }

    let mut best = valid.contains(key).then(|| (key, get(key)));
    for (f, v) in caps {
        if best.is_none_or(|(_, b)| *v > b) {
            best = Some((f.as_str(), *v));
        }
    }
    best.map(|(f, v)| (f, if total > 0.0 { v / total } else { 1.0 }))
        .into_iter()
        .collect()
}

fn capitalized_score(stats: &KeyStats, sources: &[Source]) -> f64 {
    let (mut edited, mut other) = (0.0, 0.0);
    for (src, ppm) in sources.iter().zip(&stats.ppm) {
        if src.edited {
            edited += ppm
        } else {
            other += ppm
        }
    }
    edited + other.min(NAME_UNEDITED_CAP * edited)
}

pub fn select<'a>(
    keys: &'a Keys,
    candidates: &[&'a str],
    valid: &FxHashSet<&str>,
    sources: &[Source],
    additions: &FxHashMap<String, f64>,
) -> (Vec<Entry<'a>>, Vec<&'a str>) {
    let mut entries = Vec::new();
    let mut rejected = Vec::new();
    for &key in candidates {
        let stats = &keys[key];
        let forms = choose_forms(key, stats, valid);

        let added = stats.variants.iter().filter_map(|(f, _)| {
            let hint = additions.get(f)?;
            (!forms.iter().any(|(g, _)| g == f)).then_some((f.as_str(), *hint))
        });
        let added: Vec<(&str, f64)> = added.collect();
        if forms.is_empty() && added.is_empty() {
            rejected.push(key);
        }
        for (form, share) in forms {
            let score = if form.starts_with(char::is_uppercase) {
                capitalized_score(stats, sources)
            } else {
                stats.score
            };
            let hint = additions.get(form).copied().unwrap_or(0.0);
            entries.push(Entry {
                ppm: (score.max(1e-3) * share).max(hint),
                form,
                key,
            });
        }
        for (form, hint) in added {
            entries.push(Entry {
                ppm: hint.max(1e-3),
                form,
                key,
            });
        }
    }
    entries.par_sort_unstable_by(|a, b| b.ppm.total_cmp(&a.ppm).then_with(|| b.form.cmp(a.form)));
    entries.truncate(MAX_WORDS);
    (entries, rejected)
}

pub fn complete<'a>(
    entries: &[Entry],
    paradigms: impl IntoIterator<Item = &'a (String, Vec<String>)>,
    exclusions: &FxHashSet<String>,
) -> Vec<&'a str> {
    let mut lemma_ppm: FxHashMap<&str, f64> = FxHashMap::default();
    let mut present: FxHashSet<&str> = FxHashSet::default();
    for e in entries {
        *lemma_ppm.entry(e.key).or_default() += e.ppm;
        present.insert(e.form);
        present.insert(e.key);
    }
    let mut out: Vec<(f64, &str)> = Vec::new();
    for (lemma, forms) in paradigms {
        let Some(&ppm) = lemma_ppm.get(lemma.to_lowercase().as_str()) else {
            continue;
        };
        if ppm < COMPLETION_MIN_PPM {
            continue;
        }
        for f in forms {
            if !f.chars().any(char::is_uppercase)
                && (2..=MAX_LEN).contains(&f.chars().count())
                && !present.contains(f.as_str())
                && !exclusions.contains(f)
            {
                out.push((ppm, f));
            }
        }
    }
    out.sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    let mut seen = FxHashSet::default();
    out.retain(|(_, f)| seen.insert(*f));
    out.truncate(MAX_COMPLETIONS);
    out.into_iter().map(|(_, f)| f).collect()
}

pub fn to_freq(ppm: f64, lmin: f64, lmax: f64) -> u8 {
    scale(ppm, lmin, lmax, FREQ_MAX)
}

pub fn to_bigram_freq(ppm: f64, lmin: f64, lmax: f64) -> u8 {
    scale(ppm, lmin, lmax, 255.0)
}

fn scale(ppm: f64, lmin: f64, lmax: f64, max: f64) -> u8 {
    let x = (ppm.ln() - lmin) / (lmax - lmin);
    (1.0 + ((FREQ_MAX - 1.0) * x).round()).clamp(1.0, max) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deaccented() {
        assert_eq!(deaccent("Écoute"), "Ecoute");
        assert_eq!(deaccent("cœur"), "coeur");
        assert_eq!(deaccent("l'État"), "l'Etat");
        assert_eq!(deaccent("ça"), "ca");
    }

    fn stats(variants: &[(&str, f64)]) -> KeyStats {
        KeyStats {
            score: 100.0,
            raw: 1000,
            per_source: vec![1000],
            ppm: vec![100.0],
            lang: [0, 0],
            variants: variants.iter().map(|&(f, v)| (f.to_owned(), v)).collect(),
        }
    }

    #[test]
    fn rare_valid_case_keeps_only_its_share() {
        let s = stats(&[("ca", 90.0), ("Ca", 6.0), ("CA", 4.0)]);
        let valid: FxHashSet<&str> = ["CA"].into_iter().collect();
        let forms = choose_forms("ca", &s, &valid);
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0].0, "CA");
        assert!((forms[0].1 - 0.04).abs() < 1e-9);
    }

    #[test]
    fn folds_move_counts_to_accented_form() {
        let mut keys = Keys::default();
        keys.insert(
            "écoute".into(),
            stats(&[("écoute", 50.0), ("Écoute", 30.0)]),
        );
        keys.insert("ecoute".into(), stats(&[("Ecoute", 20.0), ("ecoute", 5.0)]));
        apply_folds(&mut keys, &[("ecoute".into(), "écoute".into())]);
        let t = &keys["écoute"];
        assert_eq!(t.score, 200.0);
        assert_eq!(
            t.variants,
            [("écoute".to_owned(), 55.0), ("Écoute".to_owned(), 50.0)]
        );
        assert!(!keys.contains_key("ecoute"));
    }
}
