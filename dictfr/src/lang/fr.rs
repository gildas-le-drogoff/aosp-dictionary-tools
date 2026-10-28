use super::{Download, Lang, Lexicon, Markers, WiktionaryRules};
use crate::corpus::Source;
use crate::morphalou;

pub const FR: Lang = Lang {
    code: "fr",
    dir: "fr",
    description: "Français moderne (OpenSubtitles 2024 + presse 2022-2024 + \
        Wikipédia 2021 + Wiktionnaire + Morphalou ; Hunspell Grammalecte)",
    downloads: &[
        Download::Url {
            url: "https://object.pouta.csc.fi/OPUS-OpenSubtitles/v2024/mono/fr.txt.gz",
            file: "opensubtitles2024_fr.txt.gz",
            member: None,
        },
        Download::Url {
            url: "https://kaikki.org/dictionary/downloads/fr/fr-extract.jsonl.gz",
            file: "wiktionnaire_fr.jsonl.gz",
            member: None,
        },
        Download::Url {
            url: "https://downloads.wortschatz-leipzig.de/corpora/fra_news_2022_1M.tar.gz",
            file: "fra_news_2022_1M.tar.gz",
            member: Some("fra_news_2022_1M/fra_news_2022_1M-sentences.txt"),
        },
        Download::Url {
            url: "https://downloads.wortschatz-leipzig.de/corpora/fra_news_2023_1M.tar.gz",
            file: "fra_news_2023_1M.tar.gz",
            member: Some("fra_news_2023_1M/fra_news_2023_1M-sentences.txt"),
        },
        Download::Url {
            url: "https://downloads.wortschatz-leipzig.de/corpora/fra_news_2024_1M.tar.gz",
            file: "fra_news_2024_1M.tar.gz",
            member: Some("fra_news_2024_1M/fra_news_2024_1M-sentences.txt"),
        },
        Download::Url {
            url: "https://downloads.wortschatz-leipzig.de/corpora/fra_wikipedia_2021_1M.tar.gz",
            file: "fra_wikipedia_2021_1M.tar.gz",
            member: Some("fra_wikipedia_2021_1M/fra_wikipedia_2021_1M-sentences.txt"),
        },
        Download::Manual {
            file: "morphalou3.1.csv.gz",
            how: "télécharger Morphalou 3.1 (format CSV « tout en un ») sur \
                https://www.ortolang.fr/market/lexicons/morphalou puis : \
                gzip -c Morphalou3.1_CSV.csv > fr/sources/morphalou3.1.csv.gz",
        },
    ],
    sources: &[
        Source {
            name: "os2024",
            files: &["opensubtitles2024_fr.txt.gz"],
            leipzig: false,
            weight: 0.50,
            dedup: true,
            edited: false,
        },
        Source {
            name: "news",
            files: &[
                "fra_news_2022_1M/fra_news_2022_1M-sentences.txt",
                "fra_news_2023_1M/fra_news_2023_1M-sentences.txt",
                "fra_news_2024_1M/fra_news_2024_1M-sentences.txt",
            ],
            leipzig: true,
            weight: 0.35,
            dedup: false,
            edited: true,
        },
        Source {
            name: "wiki2021",
            files: &["fra_wikipedia_2021_1M/fra_wikipedia_2021_1M-sentences.txt"],
            leipzig: true,
            weight: 0.15,
            dedup: false,
            edited: true,
        },
    ],
    wiktionary: WiktionaryRules {
        file: "wiktionnaire_fr.jsonl.gz",
        lang_code: "fr",
        prefilter: true,
        error_categories: &["Fautes "],
        variant_tags: &["rare", "obsolete", "archaic"],
        pronouns: Some(&["j", "m", "t", "s", "n", "l", "qu"]),
        rare_form,
    },
    lexicon: Some(Lexicon {
        name: "Morphalou",
        file: "morphalou3.1.csv.gz",
        load: morphalou::load_verbs,
    }),
    hunspell: &["/usr/share/hunspell/fr_FR"],
    rival_hunspell: &[],
    markers: Markers {
        code: "fr",
        words: &[
            "de", "la", "le", "les", "et", "des", "est", "que", "pas", "je", "il", "une", "du",
            "qui", "ne", "à", "pour", "dans", "c'est", "vous", "tu", "nous", "mais", "avec",
            "suis", "elle", "ça", "j'ai", "sur", "au", "sont", "très", "tout", "bien", "oui",
            "moi", "toi", "rien",
        ],
    },
    foreign: super::en::MARKERS,
    one_letter: &["a", "à", "y", "ô"],
    ocr_fix,
};

/// Passé simple et subjonctif imparfait : voir `fr/JOURNAL.md`.
fn rare_form(tags: &[String]) -> bool {
    let has = |t: &str| tags.iter().any(|x| x == t);
    has("archaic")
        || has("rare")
        || has("obsolete")
        || (has("subjunctive") && has("imperfect"))
        || (has("indicative") && has("past") && !has("multiword-construction") && !has("anterior"))
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
