use super::{Download, Lang, Markers, WiktionaryRules};
use crate::corpus::Source;

const HUNSPELL_US: &str = "/usr/share/hunspell/en_US";
const HUNSPELL_GB: &str = "/usr/share/hunspell/en_GB";

pub const MARKERS: Markers = Markers {
    code: "en",
    words: &[
        "the", "and", "you", "of", "is", "to", "my", "it's", "i'm", "don't", "that", "with",
        "your", "what", "this", "are", "was", "be", "for", "have", "we", "just", "know", "can't",
    ],
};

/// Graphies américaines et britanniques ; `en_US` et `en_GB` écartent celles de l'autre
/// variante.
pub const EN: Lang = Lang {
    code: "en",
    dir: "en",
    description: "Modern English (US and UK spellings; OpenSubtitles 2024 + news 2023-2024 + \
        Wikipedia 2016 + Wiktionary; Hunspell en_US + en_GB)",
    downloads: &[
        Download::Url {
            url: "https://object.pouta.csc.fi/OPUS-OpenSubtitles/v2024/mono/en.txt.gz",
            file: "opensubtitles2024_en.txt.gz",
            member: None,
        },
        Download::Url {
            url: "https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl.gz",
            file: "wiktionary_en.jsonl.gz",
            member: None,
        },
        Download::Url {
            url: "https://downloads.wortschatz-leipzig.de/corpora/eng_news_2023_1M.tar.gz",
            file: "eng_news_2023_1M.tar.gz",
            member: Some("eng_news_2023_1M/eng_news_2023_1M-sentences.txt"),
        },
        Download::Url {
            url: "https://downloads.wortschatz-leipzig.de/corpora/eng_news_2024_1M.tar.gz",
            file: "eng_news_2024_1M.tar.gz",
            member: Some("eng_news_2024_1M/eng_news_2024_1M-sentences.txt"),
        },
        Download::Url {
            url: "https://downloads.wortschatz-leipzig.de/corpora/eng_wikipedia_2016_1M.tar.gz",
            file: "eng_wikipedia_2016_1M.tar.gz",
            member: Some("eng_wikipedia_2016_1M/eng_wikipedia_2016_1M-sentences.txt"),
        },
    ],
    sources: &[
        Source {
            name: "os2024",
            files: &["opensubtitles2024_en.txt.gz"],
            leipzig: false,
            weight: 0.50,
            dedup: true,
            edited: false,
        },
        Source {
            name: "news",
            files: &[
                "eng_news_2023_1M/eng_news_2023_1M-sentences.txt",
                "eng_news_2024_1M/eng_news_2024_1M-sentences.txt",
            ],
            leipzig: true,
            weight: 0.35,
            dedup: false,
            edited: true,
        },
        Source {
            name: "wiki2016",
            files: &["eng_wikipedia_2016_1M/eng_wikipedia_2016_1M-sentences.txt"],
            leipzig: true,
            weight: 0.15,
            dedup: false,
            edited: true,
        },
    ],
    wiktionary: WiktionaryRules {
        file: "wiktionary_en.jsonl.gz",
        lang_code: "en",
        prefilter: false,
        error_categories: &[],
        variant_tags: &["rare", "obsolete", "archaic", "misspelling"],
        pronouns: None,
        rare_form,
    },
    lexicon: None,
    hunspell: &[HUNSPELL_US, HUNSPELL_GB],
    rival_hunspell: &[],
    markers: MARKERS,
    foreign: Markers {
        code: "-",
        words: &[],
    },
    one_letter: &["a", "i"],
    ocr_fix,
};

pub const EN_US: Lang = Lang {
    code: "en_US",
    description: "Modern English (US spelling; OpenSubtitles 2024 + news 2023-2024 + \
        Wikipedia 2016 + Wiktionary; Hunspell en_US)",
    hunspell: &[HUNSPELL_US],
    rival_hunspell: &[HUNSPELL_GB],
    ..EN
};

pub const EN_GB: Lang = Lang {
    code: "en_GB",
    description: "Modern English (UK spelling; OpenSubtitles 2024 + news 2023-2024 + \
        Wikipedia 2016 + Wiktionary; Hunspell en_GB)",
    hunspell: &[HUNSPELL_GB],
    rival_hunspell: &[HUNSPELL_US],
    ..EN
};

/// Les `forms` du Wiktionnaire anglais mêlent flexions et variantes (`colour` sous
/// `color`) : on ne complète qu'avec les flexions courantes.
fn rare_form(tags: &[String]) -> bool {
    tags.iter().any(|t| {
        matches!(
            t.as_str(),
            "alternative"
                | "archaic"
                | "rare"
                | "obsolete"
                | "nonstandard"
                | "dialectal"
                | "misspelling"
                | "pronunciation-spelling"
        )
    })
}

/// Confusions I/l de l'OCR des sous-titres, les plus fréquentes d'OpenSubtitles 2024 ;
/// `l` et `lt` peuvent être corrects ailleurs, d'où la limitation aux sous-titres.
fn ocr_fix(w: &str, _initial: bool, subtitles: bool) -> &str {
    if !subtitles {
        return w;
    }
    match w {
        "l" => "I",
        "l'm" => "I'm",
        "l'll" | "l'II" | "I'II" | "I'lI" => "I'll",
        "l've" => "I've",
        "l'd" => "I'd",
        "lt" => "It",
        "lt's" => "It's",
        "lt'll" => "It'll",
        "lf" => "If",
        "ls" => "Is",
        "lsn't" => "Isn't",
        "ln" => "In",
        "Iike" => "like",
        "Iove" => "love",
        "Iot" => "lot",
        "Iife" => "life",
        "Iong" => "long",
        "Ionger" => "longer",
        "Iet" => "let",
        "Iook" => "look",
        "Iooking" => "looking",
        "Iast" => "last",
        "Iost" => "lost",
        "Iate" => "late",
        "Iaw" => "law",
        "Ieft" => "left",
        "Iive" => "live",
        "Ieave" => "leave",
        "Ieast" => "least",
        "Iittle" => "little",
        _ => w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_share_sources() {
        assert_eq!(EN_US.dir, "en");
        assert_eq!(EN_GB.sources.len(), EN.sources.len());
        assert_eq!(EN_US.rival_hunspell, [HUNSPELL_GB]);
    }

    #[test]
    fn ocr_only_in_subtitles() {
        assert_eq!(ocr_fix("l'm", false, true), "I'm");
        assert_eq!(ocr_fix("lt", true, false), "lt");
        assert!(!rare_form(&["past".into()]));
        assert!(rare_form(&["alternative".into(), "UK".into()]));
    }
}
