# Dictionnaires modernes pour clavier Android (AOSP)

Produit des `main_<langue>.dict` (format binaire AOSP v202) utilisables dans AOSP
Keyboard, HeliBoard ou OpenBoard, à partir de corpus récents (OpenSubtitles 2024,
presse, Wikipédia), validés par Hunspell et le Wiktionnaire, avec bigrammes.
Le binaire est écrit directement par `dictfr` : ni Java ni `dicttool` ne sont nécessaires.

| Langue  | Dossier | Orthographe                                        | Journal         |
| ------- | ------- | -------------------------------------------------- | --------------- |
| `fr`    | `fr/`   | Hunspell Grammalecte                               | `fr/JOURNAL.md` |
| `en`    | `en/`   | américaine et britannique (Hunspell en_US + en_GB) | `en/JOURNAL.md` |
| `en_US` | `en/`   | américaine (graphies britanniques écartées)        | `en/JOURNAL.md` |
| `en_GB` | `en/`   | britannique (graphies américaines écartées)        | `en/JOURNAL.md` |

## Construire

Prérequis : Rust (cargo), `libhunspell`, les dictionnaires Hunspell de la langue dans
`/usr/share/hunspell/` (`fr_FR` de Grammalecte ; `en_US` et `en_GB`), `wget`.

```
bash dictfr/build.sh fr en_US en_GB
```

Le script compile `dictfr`, télécharge les sources manquantes dans `<dossier>/sources/`
(≈ 3,5 Go pour le français, 11 Go pour l'anglais, partagés par `en`, `en_US` et `en_GB`),
puis construit chaque langue. Durée sur 16 threads : ≈ 1 min 10 et 5 Go de RAM pour le
français, ≈ 4 min et 10 Go pour chaque variante anglaise.

Seul Morphalou (français) est à placer à la main : télécharger Morphalou 3.1 (CSV « tout
en un ») sur [Ortolang](https://www.ortolang.fr/market/lexicons/morphalou) puis
`gzip -c Morphalou3.1_CSV.csv > fr/sources/morphalou3.1.csv.gz`.

Résultats dans `<dossier>/build-rs/`, par exemple pour `en_US` :

| Fichier                      | Contenu                                                     |
| ---------------------------- | ----------------------------------------------------------- |
| `main_en_US.dict`            | dictionnaire à installer sur le clavier                     |
| `en_US_wordlist.combined`    | même contenu au format texte de `dicttool`                  |
| `en_US_rejets.tsv`           | mots fréquents refusés, à relire pour enrichir `ajouts.txt` |
| `en_US_admis.tsv`            | mots admis hors Hunspell (Wiktionnaire, noms propres)       |
| `en_US_rattachements.tsv`    | graphies fautives reportées sur la forme correcte (ca → ça) |

## Outils sur les dictionnaires

```
dictfr build <langue>                                   # ce que fait build.sh, sources présentes
dictfr sources <langue>                                 # sources à télécharger (URL, fichier)
dictfr makedict [--table] liste.combined main_xx.dict   # compile une liste combined
dictfr dump main_xx.dict [liste.combined]               # décompile un dictionnaire
```

(les raccourcis
`shortcut=` sont ignorés). `--table` ajoute la table de caractères de l'en-tête v202,
utile pour les alphabets non latins ; sans elle, le fichier est identique en structure
aux dictionnaires publiés par HeliBoard. `dump` lit les versions 2, 201 et 202.

## Ajuster la liste

- `<dossier>/listes/ajouts.txt` : formes acceptées même si Hunspell les refuse ; un
  second champ facultatif donne une fréquence minimale en ppm (`mdr 10`)
- `<dossier>/listes/exclusions.txt` : formes (ou clés en minuscules) à ne jamais inclure
- `ajouts_<langue>.txt`, `exclusions_<langue>.txt` : en plus des précédents, pour une
  seule variante (`exclusions_en_GB.txt`)

Les réglages propres à une langue (sources, poids, Hunspell, règles du Wiktionnaire,
corrections d'OCR) sont dans `dictfr/src/lang/<langue>.rs` ; les réglages communs
(nombre de mots, seuils) sont des constantes en tête de `dictfr/src/main.rs` et
`dictfr/src/wordlist.rs`.

## Ajouter une langue

1. Créer `dictfr/src/lang/xx.rs` sur le modèle de `en.rs` : téléchargements, corpus,
   extraction kaikki.org du Wiktionnaire, dictionnaires Hunspell, mots-outils,
   mots d'une lettre, corrections d'OCR ; l'ajouter à `LANGS` dans `lang/mod.rs`.
2. Créer `xx/listes/ajouts.txt` et `xx/listes/exclusions.txt`.
3. `bash dictfr/build.sh xx`, puis relire `xx_rejets.tsv` et `xx_admis.tsv`.

Une variante régionale (`en_US`) reprend la langue de base (`..EN`) en changeant le
code, la description et les dictionnaires Hunspell ; elle partage ses sources et ses
listes.

## Crédits

- Format binaire : d'après le code source d'AOSP LatinIME (`FormatSpec`,
  `BinaryDictEncoderUtils`, lecteur natif `patricia_trie_policy`), Apache 2.0
- Wiktionnaire et Wiktionary : extraction [kaikki.org](https://kaikki.org/) (wiktextract), CC BY-SA
- Morphalou 3.1 : ATILF – CNRS, diffusé par [Ortolang](https://hdl.handle.net/11403/morphalou/v3.1), LGPL-LR
- Fork de https://github.com/remi0s/aosp-dictionary-tools (dictionnaires grec et arabe)

<!-- lgitea  ssh://gitea@mercury/Rust/aosp-dictionary-tools.git (fetch)
lgitea  ssh://gitea@mercury/Rust/aosp-dictionary-tools.git (push)
 -->
