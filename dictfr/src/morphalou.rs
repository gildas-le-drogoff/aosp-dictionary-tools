use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use flate2::read::MultiGzDecoder;

use crate::wiktionary::normalize;
use crate::wordlist::is_word;

const TRUSTED_ORIGIN: &str = "morphalou2";

const SKIPPED_TENSES: &[(&str, &str)] =
    &[("indicative", "simplePast"), ("subjunctive", "imperfect")];

const GRAPHIE: usize = 0;
const CATEGORIE: usize = 2;
const FORME: usize = 9;
const MODE: usize = 12;
const TEMPS: usize = 14;
const ORIGINES: usize = 17;

pub fn load_verbs(path: &Path) -> io::Result<Vec<(String, Vec<String>)>> {
    let file = File::open(path)
        .map_err(|e| io::Error::new(e.kind(), format!("{} : {e}", path.display())))?;
    read_verbs(BufReader::with_capacity(
        1 << 20,
        MultiGzDecoder::new(BufReader::new(file)),
    ))
}

fn verb_lemma(s: &str) -> Option<String> {
    let s = normalize(s);
    let s = s
        .strip_prefix("se ")
        .or_else(|| s.strip_prefix("s'"))
        .unwrap_or(&s);
    is_word(s).then(|| s.to_owned())
}

fn read_verbs(reader: impl BufRead) -> io::Result<Vec<(String, Vec<String>)>> {
    let mut paradigms: Vec<(String, Vec<String>)> = Vec::new();
    let mut header = true;
    let mut in_verb = false;
    for line in reader.lines() {
        let line = line?;
        if header {
            header = !line.starts_with("GRAPHIE;");
            continue;
        }
        let f: Vec<&str> = line.trim_end_matches('\r').split(';').collect();
        if f.len() <= ORIGINES {
            continue;
        }
        if !f[GRAPHIE].is_empty() {
            let lemma = (f[CATEGORIE] == "Verbe")
                .then(|| verb_lemma(f[GRAPHIE]))
                .flatten();
            in_verb = lemma.is_some();
            if let Some(lemma) = lemma {
                paradigms.push((lemma, Vec::new()));
            }
        }
        if !in_verb
            || SKIPPED_TENSES.contains(&(f[MODE], f[TEMPS]))
            || !f[ORIGINES].split_whitespace().any(|o| o == TRUSTED_ORIGIN)
        {
            continue;
        }
        let form = normalize(f[FORME]);
        let (lemma, forms) = paradigms.last_mut().unwrap();
        if is_word(&form) && form != *lemma {
            forms.push(form);
        }
    }
    paradigms.retain_mut(|(_, forms)| {
        forms.sort_unstable();
        forms.dedup();
        !forms.is_empty()
    });
    Ok(paradigms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbs() {
        let csv = "LEMME;;;;;;;;;FLEXION;;;;;;;;\r
GRAPHIE;ID;CATÉGORIE;SOUS CATÉGORIE;LOCUTION;GENRE;AUTRES LEMMES LIÉS;PHONÉTIQUE;ORIGINES;GRAPHIE;ID;NOMBRE;MODE;GENRE;TEMPS;PERSONNE;PHONÉTIQUE;ORIGINES\r
aba;3;Nom commun;;;masculine;;a b a;morphalou2;abas;8;plural;-;-;-;-;a b a;morphalou2\r
s'abader;6;Verbe;;;-;;a b a d e;morphalou2;abada;13;singular;indicative;-;simplePast;thirdPerson;;morphalou2\r
;;;;;;;;;abadait;18;singular;indicative;-;imperfect;thirdPerson;;morphalou2 dela\r
;;;;;;;;;abadais;16;singular;indicative;-;imperfect;firstPerson;;morphalou2\r
;;;;;;;;;abadais;17;singular;indicative;-;imperfect;secondPerson;;morphalou2\r
;;;;;;;;;abader;19;-;infinitive;-;-;-;;morphalou2\r
;;;;;;;;;abadasse;20;singular;subjunctive;-;imperfect;firstPerson;;morphalou2\r
;;;;;;;;;abaderas;21;singular;indicative;-;future;secondPerson;;dela lefff\r
pique niquer;7;Verbe;;;-;;;dela;pique nique;22;singular;indicative;-;present;thirdPerson;;morphalou2\r
";
        let verbs = read_verbs(csv.as_bytes()).unwrap();
        assert_eq!(
            verbs,
            [(
                "abader".to_owned(),
                vec!["abadais".to_owned(), "abadait".to_owned()]
            )]
        );
    }
}
