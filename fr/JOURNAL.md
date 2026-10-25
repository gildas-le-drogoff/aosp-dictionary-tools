# Journal — Dictionnaire français moderne pour clavier Android (AOSP)

Objectif : produire un `main_fr.dict` (format binaire AOSP v202) plus moderne que les dictionnaires existants, à partir de corpus récents, filtré orthographiquement, et utilisable dans AOSP Keyboard / HeliBoard / OpenBoard.

## Arborescence

| Chemin    | Contenu                                                |
| --------- | ------------------------------------------------------ |
| `dictfr/` | Programme de construction (Rust + rayon) et `build.sh` |

## `dictfr/`

- `corpus.rs` : décompression du `.gz` dans un thread, paquets de 8 Mo répartis avec rayon. Même tokenisation et même distinction début / hors début de phrase.
- Deux passes sur les corpus : unigrammes, puis bigrammes **exacts** (plus d'élagage approximatif) restreints aux mots retenus, ce qui borne la mémoire.
- `hunspell.rs` : appel direct à `libhunspell` (une instance par thread) au lieu de processus `hunspell -a`.
- `wordlist.rs` : mêmes règles que `build_wordlist.py` (score pondéré, choix de casse, échelle de fréquence 1–254, bigrammes de lift > 1).
- Résultat : 244 362 mots, 533 897 bigrammes, `main_fr.dict` de 3,8 Mo ; ≈ 50 s au total sur 16 threads, 3,2 Go de RAM au maximum.

## `makedict` (`makedict.rs`)

- **Version** : le lecteur natif refuse les versions 2 et 201 depuis Android 6 (« Dictionary versions 2 and 201 are incompatible ») ; on écrit donc la **202**.
- Trie Patricia, tableaux de nœuds en parcours préfixe, enfants triés par code de caractère, adresses relatives sur 1 à 3 octets calculées par passes successives comme `BinaryDictEncoderUtils.computeAddresses`, mots de 48 caractères ou plus ignorés, bigrammes sur 4 bits relatifs à la fréquence du mot visé.
- Table de caractères (`codePointTable`) facultative : elle ne gagne que 206 octets sur 3,78 Mo pour le français ; on l'omet par défaut, comme les dictionnaires d'HeliBoard.
- Validation sur le dictionnaire français d'HeliBoard (v202, construit par l'outil AOSP) et sa liste source : le décodeur retrouve les 190 425 mots, fréquences et drapeaux à l'identique ; l'encodeur, appliqué à la même liste, donne un fichier qui se relit à l'identique (1 323 590 octets contre 1 328 844 : les 343 raccourcis sont ignorés).
- `dictfr` écrit désormais `main_fr.dict` lui-même ; sous-commandes `makedict` et `dump`.

## Enrichissement et nettoyage

Nettoyage des corpus (`corpus.rs`) :

- Texte mal décodé réparé (`lâ€™homme` → `l'homme` : 3 804 lignes de presse 2024).
- Balises de sous-titres retirées (`{\pos(192,245)}` faisait de « pos » le 3ᵉ rejet).
- Lignes d'au moins 8 mots dédoublonnées dans OpenSubtitles (plusieurs versions d'un même film : 41 % de doublons sur un échantillon) : 1 115 M → 943 M d'occurrences.
- Lettres collées à un chiffre ignorées (`1er`, `3ème`, `MP3`).
- Confusions I/l/j de l'OCR des sous-titres corrigées (`Ie` → `le`, `ll` → `il`, `ie` → `je`, `II` → `Il`).
- Lignes tout en capitales ignorées pour le choix de la casse.
- Langue de chaque ligne estimée par des mots-outils français / anglais.

Validation (`wordlist.rs`) :

- Une forme refusée par Hunspell est admise si le Wiktionnaire la connaît et que ses contextes ne sont pas majoritairement anglais (`the` : 73 % de contextes anglais → refusé ; `okay`, `wesh`, `TikTok`, `Bardella`, `ChatGPT` → admis). Exclus : variantes typographiques, fautes répertoriées (`disez`), variantes rares (`monsieurs`).
- Une clé sans graphie valide est rattachée à la même forme accentuée si celle-ci est au moins aussi fréquente : 20 612 rattachements (`plait` → `plaît`, `soeur` → `sœur`, `etre` → `être`, `ca` → `ça`, `meme` → `même`), y compris pour les bigrammes.
- Noms propres absents des deux lexiques admis s'ils sont attestés dans la presse (≥ 50 occurrences, majuscule ≥ 90 %) : 2 310 (`Netanyahu`, `Sonko`, `Xbox`…).
- Correction : quand seule une casse rare était valide, elle héritait de toutes les occurrences de la clé (`CA` à f 182 avec les « ca » mis pour « ça ») ; elle ne garde plus que sa part (f 110).
- Graphies à majuscule : la part des sous-titres est plafonnée à 4 fois celle de la presse et de Wikipédia (personnages de séries surreprésentés).
- Formes fléchies des lemmes fréquents (≥ 1 ppm) absentes des corpus ajoutées depuis le Wiktionnaire, hors passé simple et subjonctif imparfait (justification chiffrée plus bas), vérifiées par Hunspell : 8 505 formes, fréquence 1.
- `ajouts.txt` : abréviations de messagerie (`mdr`, `stp`, `jsp`…) et élisions familières (`j'suis`) avec une fréquence minimale estimée.
- Échelle des fréquences recalée sur celle des dictionnaires AOSP : 221 au maximum (au lieu de 254), ce qui laisse aux bigrammes leur marge jusqu'à 255.

Résultat : 295 451 mots (dont 8 505 formes complétées), 538 014 bigrammes, `main_fr.dict` de 4,1 Mo ; ≈ 85 s sur 16 threads, 5 Go de RAM au maximum. Le dictionnaire couvre 131 702 des 190 425 mots du dictionnaire français d'HeliBoard (122 416 avant cette étape).

## Morphalou (`morphalou.rs`)

Morphalou 3.1 (ATILF, 976 570 formes, fusion de Morphalou 2, DELA, Dicollecte, Lefff et LGLex) évalué sur le dictionnaire précédent :

- **Validation** : rien à gagner. Parmi les 3 000 premiers rejets, les 64 formes connues de Morphalou sont presque toutes de l'anglais (`happy`, `dog`, `sorry`, `because`, issues de DELA ou du Lefff) ou des fautes (`débarasser`).
- **Noms et adjectifs** : les formes absentes, même acceptées par Hunspell, sont surtout des unités préfixées de Dicollecte (`zeptohenrys`, `picoseconde`) ; non retenu.
- **Verbes** : le Wiktionnaire laisse des trous dans les conjugaisons (`espérerai`, `souviendrions`, `générerions`, `accélérerions`). Morphalou sert donc de seconde source de paradigmes pour la complétion, limitée aux verbes et aux formes d'origine `morphalou2` (DELA et Lefff ajoutent `disez` ou `sortissait`), avec les mêmes exclusions de temps (passé simple, subjonctif imparfait) et la même vérification Hunspell.

Résultat : 8 893 formes complétées (dont 345 de Morphalou seul), 295 839 mots, 538 014 bigrammes, `main_fr.dict` de 4,1 Mo ; aucun mot retiré.

## Complétion : pourquoi exclure le passé simple et le subjonctif imparfait

Mesure faite en construisant le dictionnaire sans cette exclusion (Wiktionnaire et Morphalou) et en comparant au dictionnaire normal. L'exclusion ne concerne que la complétion : un passé simple attesté au moins 5 fois dans les corpus entre normalement (`fut`, `dit`, `influença`).

- **Volume** : 15 012 formes de plus (8 728 au subjonctif imparfait, 6 248 au passé simple), soit 23 905 complétions au lieu de 8 893 et 92 Ko de plus pour `main_fr.dict` (+2,3 %) ; aucun mot retiré.
- **Usage** : 86 % de ces formes n'apparaissent jamais dans les corpus (≈ 1 milliard de mots), aucune n'y dépasse 4 occurrences (`aimai`, `boucla`, `bouffa`, `cherchas`). Pour les complétions retenues, la proportion jamais vue est de 40 %.
- **Gêne à la frappe** :
  - 312 formes ne diffèrent d'un mot du dictionnaire que par les accents (`finît` / `finit`, `interdît` / `interdit`, `contrât` / `contrat`, `formât` / `format`, `unîtes` / `unités`, `habitât` / `habitat`) ;
  - 944 sont à une faute de frappe (une lettre ajoutée, supprimée, remplacée ou deux lettres inversées) d'un mot de fréquence ≥ 100 (`jouas` / `jours`, `prias` / `pris`, `souvînt` / `souvent`, `permît` / `permet`, `fuisse` / `puisse`, `lût` / `lit`).

Conclusion : l'exclusion est maintenue. Ces formes n'apportent presque rien et créent des concurrents inutiles aux mots courants pour la correction et les suggestions.
