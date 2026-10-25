use std::fmt::Write as _;

use rustc_hash::FxHashMap;

const MAGIC: u32 = 0x9BC1_3AFE;
const VERSION: u16 = 202;

const MAX_WORD_LENGTH: usize = 48;
const CODE_POINT_TABLE_KEY: &str = "codePointTable";

const FLAG_CHILDREN_MASK: u8 = 0xC0;
const FLAG_HAS_MULTIPLE_CHARS: u8 = 0x20;
const FLAG_IS_TERMINAL: u8 = 0x10;
const FLAG_HAS_SHORTCUT_TARGETS: u8 = 0x08;
const FLAG_HAS_BIGRAMS: u8 = 0x04;
const FLAG_IS_NOT_A_WORD: u8 = 0x02;
const FLAG_IS_POSSIBLY_OFFENSIVE: u8 = 0x01;

const ATTR_HAS_NEXT: u8 = 0x80;
const ATTR_OFFSET_NEGATIVE: u8 = 0x40;
const ATTR_ADDRESS_MASK: u8 = 0x30;
const ATTR_FREQUENCY_MASK: u8 = 0x0F;

const TERMINATOR: u8 = 0x1F;
const MIN_ONE_BYTE_CHAR: u32 = 0x20;
const MAX_ONE_BYTE_CHAR: u32 = 0xFF;
const MAX_PTNODES_IN_ARRAY: usize = 0x7FFF;
const MAX_ADDRESS: i64 = 0xFF_FFFF;
const MAX_PASSES: usize = 24;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Word {
    pub word: String,

    pub freq: u8,
    pub not_a_word: bool,
    pub possibly_offensive: bool,

    pub bigrams: Vec<(String, u8)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dict {
    pub attributes: Vec<(String, String)>,
    pub words: Vec<Word>,
}

pub fn read_combined(text: &str) -> Result<(Dict, usize), String> {
    let mut lines = text.lines().filter(|l| !l.starts_with('#'));
    let header = lines.next().ok_or("fichier combined vide")?;
    let mut dict = Dict::default();
    for item in header.split(',') {
        let (k, v) = item
            .split_once('=')
            .ok_or_else(|| format!("en-tête invalide : {header}"))?;
        if k != "options" {
            dict.attributes.push((k.to_owned(), v.to_owned()));
        }
    }
    let mut shortcuts = 0;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut params = line
            .split(',')
            .map(|p| p.split_once('=').unwrap_or((p, "")));
        let Some((tag, value)) = params.next() else {
            continue;
        };
        let freq = |params: &mut dyn Iterator<Item = (&str, &str)>| -> Result<u8, String> {
            let mut f = 0;
            for (k, v) in params {
                if k == "f" {
                    f = v
                        .parse::<u8>()
                        .map_err(|_| format!("fréquence invalide : {line}"))?;
                }
            }
            Ok(f)
        };
        match tag {
            "word" => {
                let mut w = Word {
                    word: value.to_owned(),
                    ..Word::default()
                };
                for (k, v) in params {
                    match k {
                        "f" => {
                            w.freq = v
                                .parse()
                                .map_err(|_| format!("fréquence invalide : {line}"))?
                        }
                        "not_a_word" => w.not_a_word = v == "true",
                        "possibly_offensive" => w.possibly_offensive = v == "true",
                        _ => {}
                    }
                }
                dict.words.push(w);
            }
            "bigram" => {
                let f = freq(&mut params)?;
                let w = dict
                    .words
                    .last_mut()
                    .ok_or_else(|| format!("bigramme avant le premier mot : {line}"))?;
                w.bigrams.push((value.to_owned(), f));
            }
            "shortcut" => shortcuts += 1,
            _ => return Err(format!("ligne inconnue : {line}")),
        }
    }
    Ok((dict, shortcuts))
}

pub fn write_combined(dict: &Dict) -> String {
    let mut out = String::new();
    let header: Vec<String> = dict
        .attributes
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    out.push_str(&header.join(","));
    out.push('\n');
    for w in &dict.words {
        let _ = write!(out, " word={},f={}", w.word, w.freq);
        if w.not_a_word {
            out.push_str(",not_a_word=true");
        }
        if w.possibly_offensive {
            out.push_str(",possibly_offensive=true");
        }
        out.push('\n');
        for (b, f) in &w.bigrams {
            let _ = writeln!(out, "  bigram={b},f={f}");
        }
    }
    out
}

struct Node {
    chars: Vec<char>,

    word: Option<usize>,
    children: Option<usize>,

    bigrams: Vec<(usize, u8)>,
}

struct Trie {
    nodes: Vec<Node>,

    arrays: Vec<Vec<usize>>,
}

impl Trie {
    fn build(&mut self, words: &[(Vec<char>, usize)], depth: usize) -> usize {
        let id = self.arrays.len();
        self.arrays.push(Vec::new());
        let mut i = 0;
        while i < words.len() {
            let c = words[i].0[depth];
            let j = i + words[i..].iter().take_while(|w| w.0[depth] == c).count();
            let group = &words[i..j];
            let (first, last) = (&group[0].0, &group[group.len() - 1].0);
            let mut end = depth + 1;
            while end < first.len() && end < last.len() && first[end] == last[end] {
                end += 1;
            }

            let terminal = first.len() == end;
            let node = self.nodes.len();
            self.nodes.push(Node {
                chars: first[depth..end].to_vec(),
                word: terminal.then_some(group[0].1),
                children: None,
                bigrams: Vec::new(),
            });
            self.arrays[id].push(node);
            let rest = &group[usize::from(terminal)..];
            if !rest.is_empty() {
                self.nodes[node].children = Some(self.build(rest, end));
            }
            i = j;
        }
        id
    }
}

fn address_size(offset: i64) -> usize {
    match offset.unsigned_abs() {
        0..=0xFF => 1,
        0x100..=0xFFFF => 2,
        _ => 3,
    }
}

fn count_size(n: usize) -> usize {
    if n <= 0x7F { 1 } else { 2 }
}

fn bigram_freq_diff(unigram: u8, bigram: u8) -> u8 {
    let bigram = bigram.max(unigram);
    let step = (255 - u32::from(unigram)) as f32 / (1.5 + 15.0);
    if step == 0.0 {
        return 0;
    }
    let first_step_start = 1.0 + f32::from(unigram) + step / 2.0;
    let d = ((f32::from(bigram) - first_step_start) / step) as i32;
    d.clamp(0, 15) as u8
}

fn bigram_freq_from_diff(unigram: u8, diff: u8) -> u8 {
    let step = (255 - u32::from(unigram)) as f32 / (1.5 + 15.0);
    (f32::from(unigram) + (f32::from(diff) + 1.0) * step) as u8
}

struct Encoder<'a> {
    dict: &'a Dict,
    trie: Trie,
    table: FxHashMap<char, u8>,
}

impl Encoder<'_> {
    fn char_size(&self, c: char) -> usize {
        if self.table.contains_key(&c) { 1 } else { 3 }
    }

    fn fixed_size(&self, n: &Node) -> usize {
        let chars: usize = n.chars.iter().map(|&c| self.char_size(c)).sum();
        1 + chars + usize::from(n.chars.len() > 1) + usize::from(n.word.is_some())
    }

    fn put_char(&self, out: &mut Vec<u8>, c: char) {
        match self.table.get(&c) {
            Some(&b) => out.push(b),
            None => out.extend_from_slice(&(c as u32).to_be_bytes()[1..]),
        }
    }
}

fn put_header_string(out: &mut Vec<u8>, s: &str) {
    for c in s.chars() {
        let cp = c as u32;
        if (MIN_ONE_BYTE_CHAR..=MAX_ONE_BYTE_CHAR).contains(&cp) {
            out.push(cp as u8);
        } else {
            out.extend_from_slice(&cp.to_be_bytes()[1..]);
        }
    }
    out.push(TERMINATOR);
}

fn put_address(out: &mut Vec<u8>, offset: i64) {
    let v = offset.unsigned_abs();
    let size = address_size(offset);
    out.extend_from_slice(&v.to_be_bytes()[8 - size..]);
}

pub fn encode(dict: &Dict, code_point_table: bool) -> Result<Vec<u8>, String> {
    let valid = |w: &str| (1..MAX_WORD_LENGTH).contains(&w.chars().count());
    let mut index: FxHashMap<&str, usize> = FxHashMap::default();
    let mut merged: Vec<Word> = Vec::new();
    let mut skipped = 0;
    for w in &dict.words {
        if !valid(&w.word) {
            skipped += 1;
            continue;
        }
        match index.get(w.word.as_str()) {
            Some(&i) => {
                let m = &mut merged[i];
                m.freq = m.freq.max(w.freq);
                m.not_a_word &= w.not_a_word;
                m.possibly_offensive |= w.possibly_offensive;
                m.bigrams.extend(w.bigrams.iter().cloned());
            }
            None => {
                index.insert(&w.word, merged.len());
                merged.push(w.clone());
            }
        }
    }

    for b in dict
        .words
        .iter()
        .flat_map(|w| w.bigrams.iter().map(|(b, _)| b.as_str()))
    {
        if valid(b) && !index.contains_key(b) {
            index.insert(b, merged.len());
            merged.push(Word {
                word: b.to_owned(),
                ..Word::default()
            });
        }
    }
    drop(index);
    if skipped > 0 {
        eprintln!("makedict : {skipped} mots vides ou trop longs ignorés");
    }
    if merged.is_empty() {
        return Err("aucun mot à écrire".into());
    }
    let merged = Dict {
        attributes: dict.attributes.clone(),
        words: merged,
    };

    let mut sorted: Vec<(Vec<char>, usize)> = merged
        .words
        .iter()
        .enumerate()
        .map(|(i, w)| (w.word.chars().collect(), i))
        .collect();
    sorted.sort_unstable();
    let mut trie = Trie {
        nodes: Vec::new(),
        arrays: Vec::new(),
    };
    trie.build(&sorted, 0);
    if let Some(a) = trie.arrays.iter().find(|a| a.len() > MAX_PTNODES_IN_ARRAY) {
        return Err(format!(
            "tableau de {} nœuds, maximum {MAX_PTNODES_IN_ARRAY}",
            a.len()
        ));
    }
    let mut word_node = vec![0; merged.words.len()];
    for (i, n) in trie.nodes.iter().enumerate() {
        if let Some(w) = n.word {
            word_node[w] = i;
        }
    }
    let by_word: FxHashMap<&str, usize> = merged
        .words
        .iter()
        .enumerate()
        .map(|(i, w)| (w.word.as_str(), i))
        .collect();
    for (i, w) in merged.words.iter().enumerate() {
        let mut seen = FxHashMap::default();
        for (b, f) in &w.bigrams {
            let Some(&t) = by_word.get(b.as_str()) else {
                continue;
            };

            let diff = bigram_freq_diff(merged.words[t].freq, *f);
            let list = &mut trie.nodes[word_node[i]].bigrams;
            match seen.get(&t) {
                Some(&k) => list[k] = (word_node[t], diff),
                None => {
                    seen.insert(t, list.len());
                    list.push((word_node[t], diff));
                }
            }
        }
    }

    let one_byte: Vec<char> = if code_point_table {
        let mut occurrences: FxHashMap<char, u64> = FxHashMap::default();
        for n in &trie.nodes {
            for &c in &n.chars {
                *occurrences.entry(c).or_default() += 1;
            }
        }
        let mut by_count: Vec<(char, u64)> = occurrences.into_iter().collect();
        by_count.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        by_count.truncate((MAX_ONE_BYTE_CHAR - MIN_ONE_BYTE_CHAR + 1) as usize);
        by_count.into_iter().map(|(c, _)| c).collect()
    } else {
        (MIN_ONE_BYTE_CHAR..=MAX_ONE_BYTE_CHAR)
            .filter_map(char::from_u32)
            .collect()
    };
    let table: FxHashMap<char, u8> = one_byte
        .iter()
        .enumerate()
        .map(|(i, &c)| (c, (MIN_ONE_BYTE_CHAR as usize + i) as u8))
        .collect();
    let table_string: Option<String> = code_point_table.then(|| one_byte.iter().collect());

    let enc = Encoder {
        dict: &merged,
        trie,
        table,
    };
    let body = enc.layout()?;

    let mut out = Vec::with_capacity(body.len() + 1024);
    out.extend_from_slice(&MAGIC.to_be_bytes());
    out.extend_from_slice(&VERSION.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&[0; 4]);
    for (k, v) in &enc.dict.attributes {
        if k != CODE_POINT_TABLE_KEY {
            put_header_string(&mut out, k);
            put_header_string(&mut out, v);
        }
    }
    if let Some(t) = &table_string {
        put_header_string(&mut out, CODE_POINT_TABLE_KEY);
        put_header_string(&mut out, t);
    }
    let header_size = out.len() as u32;
    out[8..12].copy_from_slice(&header_size.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

impl Encoder<'_> {
    fn layout(&self) -> Result<Vec<u8>, String> {
        let (nodes, arrays) = (&self.trie.nodes, &self.trie.arrays);

        let mut node_size: Vec<usize> = nodes
            .iter()
            .map(|n| {
                self.fixed_size(n) + 3 * usize::from(n.children.is_some()) + 4 * n.bigrams.len()
            })
            .collect();
        let mut array_size: Vec<usize> = arrays
            .iter()
            .map(|a| count_size(a.len()) + a.iter().map(|&n| node_size[n]).sum::<usize>())
            .collect();
        let mut array_old = vec![0i64; arrays.len()];
        let mut node_old = vec![0i64; nodes.len()];
        let mut pos = 0i64;
        for (a, list) in arrays.iter().enumerate() {
            array_old[a] = pos;
            let mut p = pos + count_size(list.len()) as i64;
            for &n in list {
                node_old[n] = p;
                p += node_size[n] as i64;
            }
            pos += array_size[a] as i64;
        }
        let mut array_new = array_old.clone();
        let mut node_new = node_old.clone();

        let mut passes = 0;
        loop {
            let mut changed = false;
            let mut start = 0i64;
            for (a, list) in arrays.iter().enumerate() {
                array_new[a] = start;
                let mut size = count_size(list.len());
                for &n in list {
                    node_new[n] = array_new[a] + size as i64;
                    changed |= node_new[n] != node_old[n];
                    let node = &nodes[n];
                    let mut ns = self.fixed_size(node);
                    if let Some(c) = node.children {
                        let off = (ns + size) as i64;
                        let offset = if array_old[c] < array_old[a] {
                            array_new[c] - (array_new[a] + off)
                        } else {
                            array_old[c] - (array_old[a] + off)
                        };
                        ns += address_size(offset);
                    }
                    for &(t, _) in &node.bigrams {
                        let off = (ns + size + 1) as i64;
                        let old_base = array_old[a] + off;
                        let offset = if node_old[t] < old_base {
                            node_new[t] - (array_new[a] + off)
                        } else {
                            node_old[t] - old_base
                        };
                        ns += 1 + address_size(offset);
                    }
                    node_size[n] = ns;
                    size += ns;
                }
                if size > array_size[a] {
                    return Err("taille de tableau en hausse pendant la compression".into());
                }
                if size != array_size[a] {
                    array_size[a] = size;
                    changed = true;
                }
                start += size as i64;
            }
            array_old.copy_from_slice(&array_new);
            node_old.copy_from_slice(&node_new);
            passes += 1;
            if !changed {
                break;
            }
            if passes > MAX_PASSES {
                return Err("compression des adresses : trop de passes".into());
            }
        }

        let total: usize = array_size.iter().sum();
        let mut out = Vec::with_capacity(total);
        for (a, list) in arrays.iter().enumerate() {
            if out.len() as i64 != array_new[a] {
                return Err("adresse de tableau incohérente".into());
            }
            let count = list.len();
            if count_size(count) == 1 {
                out.push(count as u8);
            } else {
                out.extend_from_slice(&((count as u16) | 0x8000).to_be_bytes());
            }
            for &n in list {
                let start = out.len();
                if start as i64 != node_new[n] {
                    return Err("adresse de nœud incohérente".into());
                }
                self.write_node(&mut out, n, &array_new, &node_new)?;
                if out.len() - start != node_size[n] {
                    return Err("taille de nœud incohérente".into());
                }
            }
        }
        if out.len() as i64 > MAX_ADDRESS {
            eprintln!(
                "makedict : attention, trie de {} octets (> 16 Mo)",
                out.len()
            );
        }
        Ok(out)
    }

    fn write_node(
        &self,
        out: &mut Vec<u8>,
        n: usize,
        array_addr: &[i64],
        node_addr: &[i64],
    ) -> Result<(), String> {
        let node = &self.trie.nodes[n];
        let word = node.word.map(|w| &self.dict.words[w]);
        let flags_pos = out.len();
        out.push(0);
        for &c in &node.chars {
            self.put_char(out, c);
        }
        let mut flags = 0;
        if node.chars.len() > 1 {
            out.push(TERMINATOR);
            flags |= FLAG_HAS_MULTIPLE_CHARS;
        }
        if let Some(w) = word {
            out.push(w.freq);
            flags |= FLAG_IS_TERMINAL;
            if w.not_a_word {
                flags |= FLAG_IS_NOT_A_WORD;
            }
            if w.possibly_offensive {
                flags |= FLAG_IS_POSSIBLY_OFFENSIVE;
            }
        }
        if let Some(c) = node.children {
            let offset = array_addr[c] - out.len() as i64;
            if offset <= 0 || offset > MAX_ADDRESS {
                return Err(format!("adresse des enfants hors limites : {offset}"));
            }
            flags |= (address_size(offset) as u8) << 6;
            put_address(out, offset);
        }
        for (i, &(t, diff)) in node.bigrams.iter().enumerate() {
            let flag_pos = out.len();
            out.push(0);
            let offset = node_addr[t] - out.len() as i64;
            if offset.abs() > MAX_ADDRESS {
                return Err(format!("adresse de bigramme hors limites : {offset}"));
            }
            let mut f = (address_size(offset) as u8) << 4 | diff;
            if i + 1 < node.bigrams.len() {
                f |= ATTR_HAS_NEXT;
            }
            if offset < 0 {
                f |= ATTR_OFFSET_NEGATIVE;
            }
            out[flag_pos] = f;
            put_address(out, offset);
        }
        if !node.bigrams.is_empty() {
            flags |= FLAG_HAS_BIGRAMS;
        }
        out[flags_pos] = flags;
        Ok(())
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    table: Option<Vec<char>>,
}

impl Reader<'_> {
    fn byte(&self, pos: usize) -> Result<u8, String> {
        self.buf
            .get(pos)
            .copied()
            .ok_or_else(|| format!("lecture hors du fichier ({pos})"))
    }

    fn uint(&self, pos: &mut usize, size: usize) -> Result<u32, String> {
        let mut v = 0;
        for _ in 0..size {
            v = v << 8 | u32::from(self.byte(*pos)?);
            *pos += 1;
        }
        Ok(v)
    }

    fn char(&self, pos: &mut usize, table: Option<&[char]>) -> Result<Option<char>, String> {
        let b = self.byte(*pos)?;
        if u32::from(b) < MIN_ONE_BYTE_CHAR {
            if b == TERMINATOR {
                *pos += 1;
                return Ok(None);
            }
            let cp = self.uint(pos, 3)?;
            return char::from_u32(cp)
                .map(Some)
                .ok_or_else(|| format!("code invalide {cp:#x}"));
        }
        *pos += 1;
        match table {
            Some(t) => t
                .get(usize::from(b) - MIN_ONE_BYTE_CHAR as usize)
                .copied()
                .map(Some)
                .ok_or_else(|| format!("octet {b:#x} absent de la table de caractères")),
            None => Ok(Some(char::from(b))),
        }
    }

    fn string(&self, pos: &mut usize, table: Option<&[char]>) -> Result<String, String> {
        let mut s = String::new();
        while let Some(c) = self.char(pos, table)? {
            s.push(c);
        }
        Ok(s)
    }
}

struct RawWord {
    word: String,
    freq: u8,
    flags: u8,

    bigrams: Vec<(usize, u8)>,
}

pub fn decode(buf: &[u8]) -> Result<Dict, String> {
    let mut r = Reader { buf, table: None };
    let mut pos = 0;
    if r.uint(&mut pos, 4)? != MAGIC {
        return Err("nombre magique absent : ce n'est pas un dictionnaire AOSP".into());
    }
    let version = r.uint(&mut pos, 2)?;
    if !matches!(version, 2 | 201 | 202) {
        return Err(format!(
            "version {version} non prise en charge (2, 201 ou 202)"
        ));
    }
    pos += 2;
    let header_size = r.uint(&mut pos, 4)? as usize;
    let mut dict = Dict::default();
    while pos < header_size {
        let k = r.string(&mut pos, None)?;
        let v = r.string(&mut pos, None)?;
        if k == CODE_POINT_TABLE_KEY {
            r.table = Some(v.chars().collect());
        } else {
            dict.attributes.push((k, v));
        }
    }

    let mut words: Vec<RawWord> = Vec::new();
    let mut at: FxHashMap<usize, usize> = FxHashMap::default();

    let mut stack = vec![(header_size, String::new())];
    let mut visited = 0usize;
    while let Some((array_pos, prefix)) = stack.pop() {
        visited += 1;
        if visited > buf.len() {
            return Err("structure cyclique".into());
        }
        let mut pos = array_pos;
        let first = r.byte(pos)?;
        let count = if first < 0x80 {
            pos += 1;
            usize::from(first)
        } else {
            (r.uint(&mut pos, 2)? & 0x7FFF) as usize
        };
        let mut children = Vec::new();
        for _ in 0..count {
            let node_pos = pos;
            let flags = r.byte(pos)?;
            pos += 1;
            let table = r.table.as_deref();
            let mut word = prefix.clone();
            if flags & FLAG_HAS_MULTIPLE_CHARS != 0 {
                word.push_str(&r.string(&mut pos, table)?);
            } else {
                word.push(r.char(&mut pos, table)?.ok_or("caractère manquant")?);
            }
            let freq = if flags & FLAG_IS_TERMINAL != 0 {
                let f = r.byte(pos)?;
                pos += 1;
                Some(f)
            } else {
                None
            };
            let children_size = usize::from((flags & FLAG_CHILDREN_MASK) >> 6);
            if children_size > 0 {
                let base = pos;
                let offset = r.uint(&mut pos, children_size)? as usize;
                children.push((base + offset, word.clone()));
            }
            if flags & FLAG_HAS_SHORTCUT_TARGETS != 0 {
                let mut p = pos;
                let size = r.uint(&mut p, 2)? as usize;
                pos += size;
            }
            let mut bigrams = Vec::new();
            if flags & FLAG_HAS_BIGRAMS != 0 {
                loop {
                    let f = r.byte(pos)?;
                    pos += 1;
                    let origin = pos as i64;
                    let size = usize::from((f & ATTR_ADDRESS_MASK) >> 4);
                    let offset = i64::from(r.uint(&mut pos, size)?);
                    let target = if f & ATTR_OFFSET_NEGATIVE != 0 {
                        origin - offset
                    } else {
                        origin + offset
                    };
                    bigrams.push((target as usize, f & ATTR_FREQUENCY_MASK));
                    if f & ATTR_HAS_NEXT == 0 {
                        break;
                    }
                }
            }
            if let Some(freq) = freq {
                at.insert(node_pos, words.len());
                words.push(RawWord {
                    word,
                    freq,
                    flags,
                    bigrams,
                });
            }
        }

        stack.extend(children.into_iter().rev());
    }

    for w in &words {
        let bigrams = w
            .bigrams
            .iter()
            .map(|&(t, diff)| {
                let target = at.get(&t).map(|&i| &words[i]).ok_or_else(|| {
                    format!("bigramme de « {} » vers une position invalide", w.word)
                })?;
                Ok((
                    target.word.clone(),
                    bigram_freq_from_diff(target.freq, diff),
                ))
            })
            .collect::<Result<_, String>>()?;
        dict.words.push(Word {
            word: w.word.clone(),
            freq: w.freq,
            not_a_word: w.flags & FLAG_IS_NOT_A_WORD != 0,
            possibly_offensive: w.flags & FLAG_IS_POSSIBLY_OFFENSIVE != 0,
            bigrams,
        });
    }
    Ok(dict)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(w: &str, freq: u8, bigrams: &[(&str, u8)]) -> Word {
        Word {
            word: w.to_owned(),
            freq,
            bigrams: bigrams.iter().map(|&(b, f)| (b.to_owned(), f)).collect(),
            ..Word::default()
        }
    }

    fn sorted(mut d: Dict) -> Dict {
        d.words.sort_by(|a, b| a.word.cmp(&b.word));
        d
    }

    fn quantized(d: &Dict) -> Dict {
        let freq: FxHashMap<&str, u8> = d.words.iter().map(|w| (w.word.as_str(), w.freq)).collect();
        let mut q = d.clone();
        for w in &mut q.words {
            for (b, f) in &mut w.bigrams {
                let u = freq[b.as_str()];
                *f = bigram_freq_from_diff(u, bigram_freq_diff(u, *f));
            }
        }
        q
    }

    fn roundtrip(d: &Dict) {
        for table in [false, true] {
            let bytes = encode(d, table).unwrap();
            assert_eq!(
                sorted(decode(&bytes).unwrap()),
                sorted(quantized(d)),
                "table={table}"
            );
        }
    }

    #[test]
    fn small() {
        let d = Dict {
            attributes: vec![
                ("dictionary".into(), "main:fr".into()),
                ("locale".into(), "fr".into()),
                ("description".into(), "Français œ".into()),
            ],
            words: vec![
                word("de", 221, &[("la", 240), ("l'homme", 200)]),
                word("la", 213, &[]),
                word("l'homme", 150, &[("de", 230)]),
                word("le", 209, &[("le", 220)]),
                word("lequel", 100, &[]),
                word("les", 205, &[]),
                word("cœur", 170, &[("de", 250)]),
                word("Ἀθῆναι", 10, &[]),
                word("😀", 5, &[("cœur", 30)]),
                word("a", 200, &[]),
                word("à", 210, &[("la", 255)]),
            ],
        };
        roundtrip(&d);
    }

    #[test]
    fn large_arrays_and_many_chars() {
        let mut words = Vec::new();
        for i in 0..600u32 {
            let c = char::from_u32(0x4E00 + i).unwrap();
            words.push(word(&format!("{c}x"), (i % 256) as u8, &[]));
            words.push(word(&format!("{c}xy{i}"), 3, &[]));
        }
        let n = words.len();
        for i in 0..n {
            let target = words[(i * 7919) % n].word.clone();
            words[i].bigrams.push((target, 255));
        }
        roundtrip(&Dict {
            attributes: vec![("locale".into(), "zh".into())],
            words,
        });
    }

    #[test]
    fn duplicates_and_missing_bigram_target() {
        let d = Dict {
            attributes: vec![],
            words: vec![word("chat", 100, &[("noir", 200)]), word("chat", 120, &[])],
        };
        let out = sorted(decode(&encode(&d, false).unwrap()).unwrap());
        assert_eq!(out.words.len(), 2);
        assert_eq!(out.words[0].word, "chat");
        assert_eq!(out.words[0].freq, 120);
        assert_eq!(out.words[1], word("noir", 0, &[]));
    }

    #[test]
    fn combined_text() {
        let text = "dictionary=main:fr,locale=fr,date=1\n word=bonjour,f=200,flags=,originalFreq=200\n  bigram=toi,f=230\n word=toi,f=150,possibly_offensive=true\n";
        let (d, shortcuts) = read_combined(text).unwrap();
        assert_eq!(shortcuts, 0);
        assert_eq!(d.words[0].bigrams, vec![("toi".to_owned(), 230)]);
        assert!(d.words[1].possibly_offensive);
        assert_eq!(read_combined(&write_combined(&d)).unwrap().0, d);
    }

    #[test]
    fn bigram_quantization() {
        assert_eq!(bigram_freq_diff(100, 100), 0);
        assert_eq!(bigram_freq_diff(100, 50), 0);
        assert_eq!(bigram_freq_diff(0, 255), 15);
        assert_eq!(bigram_freq_diff(255, 255), 0);
        for u in 0..=200u8 {
            for d in 0..16u8 {
                let back = bigram_freq_diff(u, bigram_freq_from_diff(u, d));
                assert!(back.abs_diff(d) <= 1, "u={u} d={d} back={back}");
            }
        }
    }
}
