use std::borrow::Cow;
use std::fs::File;
use std::hash::BuildHasher;
use std::io::{self, BufReader, Read};
use std::path::Path;
use std::sync::Mutex;
use std::sync::mpsc::sync_channel;
use std::thread;

use flate2::read::MultiGzDecoder;
use rayon::prelude::*;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

pub struct Source {
    pub name: &'static str,

    pub files: &'static [&'static str],

    pub leipzig: bool,

    pub weight: f64,

    pub dedup: bool,

    pub edited: bool,
}

pub const TOTAL: usize = 0;

pub const MID: usize = 1;

pub const FRENCH: usize = 2;
pub const ENGLISH: usize = 3;
pub type Counts = [u64; 4];

pub type Unigrams = FxHashMap<String, Counts>;

const CHUNK: u64 = 8 << 20;
const REPORT_EVERY: u64 = 1 << 30;
const SHARDS: usize = 256;

const SENTENCE_START: &[char] = &['.', '!', '?', '…', ':', '«', '"', '—', '-'];

const DEDUP_MIN_WORDS: usize = 8;

const FRENCH_MARKERS: &[&str] = &[
    "de", "la", "le", "les", "et", "des", "est", "que", "pas", "je", "il", "une", "du", "qui",
    "ne", "à", "pour", "dans", "c'est", "vous", "tu", "nous", "mais", "avec", "suis", "elle", "ça",
    "j'ai", "sur", "au", "sont", "très", "tout", "bien", "oui", "moi", "toi", "rien",
];
const ENGLISH_MARKERS: &[&str] = &[
    "the", "and", "you", "of", "is", "to", "my", "it's", "i'm", "don't", "that", "with", "your",
    "what", "this", "are", "was", "be", "for", "have", "we", "just", "know", "can't",
];

pub fn is_letter(c: char) -> bool {
    c.is_alphabetic() && !c.is_numeric()
}

pub fn keyboard_char(c: char) -> char {
    match c {
        '’' | 'ʼ' | '‘' | '´' | '`' => '\'',
        '‐' | '‑' => '-',
        _ => c,
    }
}

fn cp1252_byte(c: char) -> Option<u8> {
    let b = match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        'Ž' => 0x8E,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9A,
        '›' => 0x9B,
        'œ' => 0x9C,
        'ž' => 0x9E,
        'Ÿ' => 0x9F,

        '\u{81}' | '\u{8D}' | '\u{8F}' | '\u{90}' | '\u{9D}' => c as u8,
        _ if (c as u32) < 0x80 || (0xA0..=0xFF).contains(&(c as u32)) => c as u8,
        _ => return None,
    };
    Some(b)
}

fn repair_mojibake(s: &str, out: &mut String) {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(b0 @ 0xC2..=0xF4) = cp1252_byte(c) {
            let n = if b0 < 0xE0 {
                2
            } else if b0 < 0xF0 {
                3
            } else {
                4
            };
            let bytes: Option<Vec<u8>> = (0..n)
                .map(|k| chars.get(i + k).and_then(|&c| cp1252_byte(c)))
                .collect();
            if let Some(bytes) = bytes
                && bytes[1..].iter().all(|b| (0x80..=0xBF).contains(b))
                && let Ok(fixed) = std::str::from_utf8(&bytes)
                && let Some(d) = fixed.chars().next()
                && matches!(d as u32, 0xA0..=0x24F | 0x2000..=0x206F | 0x20AC)
            {
                out.push(d);
                i += n;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
}

fn strip_tags(s: &str, out: &mut String) {
    let mut close = None;
    for c in s.chars() {
        match (close, c) {
            (None, '{') => close = Some('}'),
            (None, '<') => close = Some('>'),
            (Some(end), _) if c == end => {
                close = None;
                out.push(' ');
            }
            (Some(_), _) => {}
            (None, _) => out.push(c),
        }
    }
}

fn clean<'a>(line: &'a str, buf: &'a mut String) -> &'a str {
    let mut s = Cow::Borrowed(line);
    if s.contains(['{', '<']) {
        let mut t = String::with_capacity(s.len());
        strip_tags(&s, &mut t);
        s = Cow::Owned(t);
    }
    if s.contains(['Ã', 'Â', 'Å']) || s.contains("â€") {
        let mut t = String::with_capacity(s.len());
        repair_mojibake(&s, &mut t);
        s = Cow::Owned(t);
    }
    if s.is_ascii() && !s.contains('`') {
        return match s {
            Cow::Borrowed(s) => s,
            Cow::Owned(t) => {
                *buf = t;
                buf
            }
        };
    }
    buf.clear();
    if is_nfc_quick(s.chars()) == IsNormalized::Yes {
        buf.extend(s.chars().map(keyboard_char));
    } else {
        buf.extend(s.nfc().map(keyboard_char));
    }
    buf
}

enum Token<'a> {
    Word(&'a str),

    Other(char),
}

fn tokens<'a>(s: &'a str, mut f: impl FnMut(Token<'a>)) {
    let mut it = s.char_indices().peekable();
    let mut prev = ' ';
    while let Some((start, c)) = it.next() {
        if c.is_whitespace() || !is_letter(c) {
            if !c.is_whitespace() {
                f(Token::Other(c));
            }
            prev = c;
            continue;
        }
        let mut end = start + c.len_utf8();
        loop {
            while let Some(&(i, d)) = it.peek()
                && is_letter(d)
            {
                end = i + d.len_utf8();
                it.next();
            }
            match it.peek() {
                Some(&(i, '\'' | '-')) if s[i + 1..].chars().next().is_some_and(is_letter) => {
                    it.next();
                }
                _ => break,
            }
        }
        let glued = prev.is_numeric() || it.peek().is_some_and(|&(_, d)| d.is_numeric());
        f(if glued {
            Token::Other('0')
        } else {
            Token::Word(&s[start..end])
        });
        prev = s[..end].chars().next_back().unwrap_or(' ');
    }
}

fn ocr_fix(w: &str, initial: bool, subtitles: bool) -> &str {
    match w {
        "Ie" => "le",
        "ie" => "je",
        "ies" => "les",
        "Ia" => "la",
        "Ies" => "les",
        "Iui" => "lui",
        "Ià" => "là",
        "Ieur" => "leur",
        "Ieurs" => "leurs",
        "ll" if initial => "Il",
        "ll" => "il",
        "lls" if initial => "Ils",
        "lls" => "ils",
        "IIs" => "Ils",
        "II" if initial || subtitles => "Il",
        _ => w,
    }
}

fn is_marker(word: &str, list: &[&str]) -> bool {
    word.len() <= 6
        && list
            .iter()
            .any(|m| m.eq_ignore_ascii_case(word) || lower_eq(word, m))
}

fn lower_eq(word: &str, marker: &str) -> bool {
    !word.is_ascii() && word.chars().flat_map(char::to_lowercase).eq(marker.chars())
}

struct Seen(Vec<Mutex<FxHashSet<u64>>>);

impl Seen {
    fn new() -> Self {
        Self((0..SHARDS).map(|_| Default::default()).collect())
    }

    fn is_duplicate(&self, line: &str) -> bool {
        if line.split_whitespace().nth(DEDUP_MIN_WORDS - 1).is_none() {
            return false;
        }
        let h = FxBuildHasher.hash_one(line.trim());
        !self.0[(h >> 56) as usize % SHARDS]
            .lock()
            .unwrap()
            .insert(h)
    }
}

fn sentences<'a>(
    chunk: &'a str,
    src: &'a Source,
    seen: &'a Seen,
) -> impl Iterator<Item = &'a str> + 'a {
    chunk
        .lines()
        .map(move |line| {
            if src.leipzig {
                line.split_once('\t').map_or("", |(_, s)| s)
            } else {
                line
            }
        })
        .filter(move |line| !(src.dedup && seen.is_duplicate(line)))
}

pub fn process<T: Send>(
    path: &Path,
    name: &str,
    identity: impl Fn() -> T + Sync + Send,
    fold: impl Fn(T, &str) -> T + Sync + Send,
    reduce: impl Fn(T, T) -> T + Sync + Send,
) -> io::Result<T> {
    let file = File::open(path)
        .map_err(|e| io::Error::new(e.kind(), format!("{} : {e}", path.display())))?;
    let mut input: Box<dyn Read + Send> = if path.extension().is_some_and(|e| e == "gz") {
        Box::new(MultiGzDecoder::new(BufReader::with_capacity(1 << 20, file)))
    } else {
        Box::new(file)
    };
    let (tx, rx) = sync_channel::<Vec<u8>>(4 * rayon::current_num_threads());

    thread::scope(|s| {
        let reader = s.spawn(move || -> io::Result<()> {
            let mut carry = Vec::new();
            let (mut total, mut next_report) = (0, REPORT_EVERY);
            loop {
                let mut buf = std::mem::take(&mut carry);
                let n = input.by_ref().take(CHUNK).read_to_end(&mut buf)? as u64;
                let eof = n < CHUNK;
                if !eof && let Some(p) = buf.iter().rposition(|&b| b == b'\n') {
                    carry = buf.split_off(p + 1);
                }
                total += n;
                if total >= next_report {
                    log!("  {name} : {:.1} Go de texte lus", total as f64 / 1e9);
                    next_report += REPORT_EVERY;
                }
                if !buf.is_empty() && tx.send(buf).is_err() {
                    break;
                }
                if eof {
                    break;
                }
            }
            Ok(())
        });
        let result = rx
            .into_iter()
            .par_bridge()
            .fold(&identity, |acc, chunk| {
                fold(acc, &String::from_utf8_lossy(&chunk))
            })
            .reduce(&identity, &reduce);
        reader.join().expect("thread de lecture")?;
        Ok(result)
    })
}

fn merge_unigrams(mut a: Unigrams, mut b: Unigrams) -> Unigrams {
    if a.len() < b.len() {
        std::mem::swap(&mut a, &mut b);
    }
    for (w, n) in b {
        let e = a.entry(w).or_default();
        for i in 0..n.len() {
            e[i] += n[i];
        }
    }
    a
}

pub fn count_unigrams(dir: &Path, src: &Source) -> io::Result<Unigrams> {
    let seen = Seen::new();
    let mut total = Unigrams::default();
    for file in src.files {
        let uni = process(
            &dir.join(file),
            src.name,
            Unigrams::default,
            |mut uni, chunk| {
                let mut buf = String::new();

                let mut words: Vec<(usize, usize, bool)> = Vec::new();
                for line in sentences(chunk, src, &seen) {
                    let text = clean(line, &mut buf);
                    words.clear();
                    let (mut fr, mut en) = (0, 0);
                    let mut initial = true;
                    tokens(text, |tok| match tok {
                        Token::Other(c) => initial |= SENTENCE_START.contains(&c),
                        Token::Word(w) => {
                            fr += usize::from(is_marker(w, FRENCH_MARKERS));
                            en += usize::from(is_marker(w, ENGLISH_MARKERS));
                            let start = w.as_ptr() as usize - text.as_ptr() as usize;
                            words.push((start, start + w.len(), initial));
                            initial = false;
                        }
                    });
                    let (is_fr, is_en) = (u64::from(fr > en), u64::from(en > fr));

                    let shouting = words.len() >= 3 && !text.chars().any(char::is_lowercase);
                    for &(start, end, initial) in &words {
                        let w = ocr_fix(&text[start..end], initial, !src.edited);
                        let add = [1, u64::from(!initial && !shouting), is_fr, is_en];
                        match uni.get_mut(w) {
                            Some(n) => {
                                for i in 0..n.len() {
                                    n[i] += add[i];
                                }
                            }
                            None => {
                                uni.insert(w.to_owned(), add);
                            }
                        }
                    }
                }
                uni
            },
            merge_unigrams,
        )?;
        total = merge_unigrams(total, uni);
    }
    Ok(total)
}

pub fn count_bigrams(
    dir: &Path,
    src: &Source,
    ids: &FxHashMap<&str, u32>,
    heads: u32,
) -> io::Result<FxHashMap<u64, u64>> {
    let shards: Vec<Mutex<FxHashMap<u64, u64>>> = (0..SHARDS).map(|_| Default::default()).collect();
    let seen = Seen::new();
    for file in src.files {
        process(
            &dir.join(file),
            src.name,
            || (),
            |(), chunk| {
                let mut local = FxHashMap::<u64, u32>::default();
                let mut buf = String::new();
                for line in sentences(chunk, src, &seen) {
                    let mut prev = None;
                    tokens(clean(line, &mut buf), |tok| {
                        let cur = match tok {
                            Token::Other(_) => None,
                            Token::Word(w) if w.chars().any(char::is_uppercase) => {
                                let w = ocr_fix(w, true, !src.edited);
                                ids.get(w.to_lowercase().as_str()).copied()
                            }
                            Token::Word(w) => ids.get(ocr_fix(w, false, !src.edited)).copied(),
                        };
                        if let (Some(a), Some(b)) = (prev, cur)
                            && a < heads
                        {
                            *local.entry(u64::from(a) << 32 | u64::from(b)).or_default() += 1;
                        }
                        prev = cur;
                    });
                }
                let mut parts = vec![Vec::new(); SHARDS];
                for (k, n) in local {
                    parts[(k >> 32) as usize % SHARDS].push((k, n));
                }
                for (shard, part) in shards.iter().zip(parts) {
                    if !part.is_empty() {
                        let mut m = shard.lock().unwrap();
                        for (k, n) in part {
                            *m.entry(k).or_default() += u64::from(n);
                        }
                    }
                }
            },
            |(), ()| (),
        )?;
    }
    Ok(shards
        .into_iter()
        .flat_map(|m| m.into_inner().unwrap())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleaned(s: &str) -> String {
        clean(s, &mut String::new()).to_owned()
    }

    #[test]
    fn mojibake() {
        assert_eq!(
            cleaned("disponible sur lâ€™application"),
            "disponible sur l'application"
        );
        assert_eq!(
            cleaned("Ã‰tÃ© Ã\u{a0}la plage, Å“uvre"),
            "Été àla plage, œuvre"
        );
        assert_eq!(cleaned("de danseâ€Š», note"), "de danse\u{200A}», note");

        assert_eq!(
            cleaned("Âge, Ålesund, voilà\u{a0}»"),
            "Âge, Ålesund, voilà\u{a0}»"
        );
    }

    #[test]
    fn tags() {
        assert_eq!(cleaned("{\\pos(192,245)}LE 2 AOÛT"), " LE 2 AOÛT");
        assert_eq!(cleaned("<i>Je sais</i>"), " Je sais ");
    }

    fn words(s: &str) -> Vec<&str> {
        let mut out = Vec::new();
        tokens(s, |t| {
            if let Token::Word(w) = t {
                out.push(w);
            }
        });
        out
    }

    #[test]
    fn glued_to_digits() {
        assert_eq!(
            words("le 1er mai, la 3ème fois, en MP3, la Covid-19"),
            ["le", "mai", "la", "fois", "en", "la", "Covid"]
        );
        assert_eq!(words("l'homme peut-être"), ["l'homme", "peut-être"]);
    }

    #[test]
    fn markers() {
        assert!(is_marker("De", FRENCH_MARKERS));
        assert!(is_marker("Ça", FRENCH_MARKERS));
        assert!(is_marker("THE", ENGLISH_MARKERS));
        assert!(!is_marker("thé", ENGLISH_MARKERS));
    }
}
