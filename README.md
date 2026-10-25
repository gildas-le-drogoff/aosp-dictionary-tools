# Dictionnaire français moderne pour clavier Android (AOSP)

Produit un `main_fr.dict` (format binaire AOSP v202) utilisable dans AOSP Keyboard,
HeliBoard ou OpenBoard, à partir de corpus récents (OpenSubtitles 2024, presse
2022-2024, Wikipédia 2021), validé par Hunspell et le Wiktionnaire, avec bigrammes.
Le binaire est écrit directement par `dictfr` : ni Java ni `dicttool` ne sont nécessaires.

## Construire

Prérequis : Rust (cargo), `libhunspell` et le dictionnaire Hunspell `fr_FR`
(Grammalecte) dans `/usr/share/hunspell/`, `wget`.

```
bash dictfr/build.sh
```

Le script télécharge les sources si elles manquent (≈ 3,5 Go dans `fr/sources/`),
compile puis lance `dictfr`. Durée ≈ 1 min 30 sur 16 threads, 5 Go de RAM.

Résultats dans `fr/build-rs/` :

| Fichier                | Contenu                                                     |
| ---------------------- | ----------------------------------------------------------- |
| `main_fr.dict`         | dictionnaire à installer sur le clavier                     |
| `fr_wordlist.combined` | même contenu au format texte de `dicttool`                  |
| `rejets.tsv`           | mots fréquents refusés, à relire pour enrichir `ajouts.txt` |
| `admis.tsv`            | mots admis hors Hunspell (Wiktionnaire, noms propres)       |
| `rattachements.tsv`    | graphies fautives reportées sur la forme correcte (ca → ça) |

## Outils sur les dictionnaires

```
dictfr makedict [--table] liste.combined main_xx.dict   # compile une liste combined
dictfr dump main_xx.dict [liste.combined]               # décompile un dictionnaire
```

(les raccourcis
`shortcut=` sont ignorés). `--table` ajoute la table de caractères de l'en-tête v202,
utile pour les alphabets non latins ; sans elle, le fichier est identique en structure
aux dictionnaires publiés par HeliBoard. `dump` lit les versions 2, 201 et 202.

## Ajuster la liste

- `fr/listes/ajouts.txt` : formes acceptées même si Hunspell les refuse ; un second
  champ facultatif donne une fréquence minimale en ppm (`mdr 10`)
- `fr/listes/exclusions.txt` : formes (ou clés en minuscules) à ne jamais inclure

Les réglages (poids des sources, nombre de mots, seuils) sont des constantes en tête
de `dictfr/src/main.rs` et `dictfr/src/wordlist.rs`. Le détail des choix est dans
`fr/JOURNAL.md`.

## Crédits

- Format binaire : d'après le code source d'AOSP LatinIME (`FormatSpec`,
  `BinaryDictEncoderUtils`, lecteur natif `patricia_trie_policy`), Apache 2.0
- Wiktionnaire : extraction [kaikki.org](https://kaikki.org/) (wiktextract), CC BY-SA
- Fork de https://github.com/remi0s/aosp-dictionary-tools (dictionnaires grec et arabe)

<!-- lgitea  ssh://gitea@mercury/Rust/aosp-dictionary-tools.git (fetch)
lgitea  ssh://gitea@mercury/Rust/aosp-dictionary-tools.git (push)
 -->
