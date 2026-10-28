//! Réglages propres à chaque langue. Ajouter une langue : un module avec sa constante
//! `Lang`, une entrée dans `LANGS` et un dossier `<dir>/listes/`.

pub mod en;
pub mod fr;

use std::io;
use std::path::Path;

use crate::corpus::Source;

pub type Paradigms = Vec<(String, Vec<String>)>;

pub const LANGS: &[&Lang] = &[&fr::FR, &en::EN, &en::EN_US, &en::EN_GB];

pub fn find(code: &str) -> Option<&'static Lang> {
    LANGS.iter().copied().find(|l| l.code == code)
}

pub fn codes() -> String {
    LANGS.iter().map(|l| l.code).collect::<Vec<_>>().join(", ")
}

pub struct Lang {
    /// Code de la langue : attribut `locale` et noms des fichiers (`main_en_US.dict`).
    pub code: &'static str,
    /// Dossier contenant `sources/`, `listes/` et `build-rs/`, partagé entre les variantes
    /// d'une même langue (`en` pour `en`, `en_US` et `en_GB`).
    pub dir: &'static str,
    pub description: &'static str,

    pub downloads: &'static [Download],
    pub sources: &'static [Source],
    pub wiktionary: WiktionaryRules,
    /// Lexique facultatif fournissant d'autres paradigmes pour la complétion.
    pub lexicon: Option<Lexicon>,

    /// Dictionnaires Hunspell (chemins sans extension) : une forme est valide si l'un
    /// d'eux l'accepte.
    pub hunspell: &'static [&'static str],
    /// Dictionnaires d'une variante concurrente : une forme qu'ils acceptent et que
    /// `hunspell` refuse (`colour` pour en_US) n'est admise ni par le Wiktionnaire ni
    /// comme nom propre.
    pub rival_hunspell: &'static [&'static str],

    /// Mots-outils de la langue et d'une langue parasite fréquente dans les corpus : une
    /// clé surtout vue dans des lignes de la langue parasite n'est pas admise par le
    /// Wiktionnaire. Liste parasite vide : pas de filtrage.
    pub markers: Markers,
    pub foreign: Markers,

    /// Mots d'une lettre admis (clés en minuscules).
    pub one_letter: &'static [&'static str],
    /// Corrige un mot mal reconnu par l'OCR des sous-titres. Arguments : mot, début de
    /// phrase, source non relue.
    pub ocr_fix: fn(&str, bool, bool) -> &str,
}

pub enum Download {
    /// Fichier téléchargé ; `member` : fichier à extraire de l'archive `.tar.gz`.
    Url {
        url: &'static str,
        file: &'static str,
        member: Option<&'static str>,
    },
    /// Fichier à placer à la main, avec la marche à suivre.
    Manual {
        file: &'static str,
        how: &'static str,
    },
}

pub struct Markers {
    pub code: &'static str,
    pub words: &'static [&'static str],
}

pub struct Lexicon {
    pub name: &'static str,
    pub file: &'static str,
    pub load: fn(&Path) -> io::Result<Paradigms>,
}

/// Lecture de l'extraction kaikki.org (wiktextract) du Wiktionnaire.
pub struct WiktionaryRules {
    pub file: &'static str,
    pub lang_code: &'static str,
    /// Le fichier mêle toutes les langues et `lang_code` figure dans les 400 premiers
    /// octets de chaque ligne : on écarte les autres lignes sans les analyser.
    pub prefilter: bool,
    /// Préfixes de catégories des fautes répertoriées.
    pub error_categories: &'static [&'static str],
    /// Une entrée dont tous les sens sont des variantes (`alt-of`) portant l'un de ces
    /// tags est écartée, ainsi que ses formes.
    pub variant_tags: &'static [&'static str],
    /// Formes verbales précédées de pronoms (« que j'aime ») : on garde le dernier mot,
    /// sans ces élisions. `None` : les formes de plusieurs mots sont ignorées.
    pub pronouns: Option<&'static [&'static str]>,
    /// Formes fléchies à ne pas ajouter par complétion (d'après leurs tags).
    pub rare_form: fn(&[String]) -> bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_attributes() {
        for l in LANGS {
            assert_eq!(find(l.code).map(|f| f.code), Some(l.code));
            for v in [l.code, l.description] {
                assert!(!v.contains([',', '=', '\n']), "{}", l.code);
            }
        }
    }
}
