# Journal — Dictionnaires anglais (`en`, `en_US`, `en_GB`)

Même chaîne que le français (`fr/JOURNAL.md`), réglages dans `dictfr/src/lang/en.rs`.
Les trois variantes partagent les sources (`en/sources/`) et les listes (`en/listes/`).

## Sources

| Source        | Fichier                                 | Poids | Volume                |
| ------------- | --------------------------------------- | ----- | --------------------- |
| Sous-titres   | OpenSubtitles 2024 `en` (8,9 Go gz)     | 0,50  | 4,5 G occurrences     |
| Presse        | Leipzig `eng_news_2023_1M`, `2024_1M`   | 0,35  | 39 M occurrences      |
| Wikipédia     | Leipzig `eng_wikipedia_2016_1M`         | 0,15  | 20 M occurrences      |
| Wiktionnaire  | kaikki.org, Wiktionary anglais, entrées `en` (1,5 M) | — | 1 084 795 formes |

Leipzig ne propose pas `eng_news_2021_1M`/`2022_1M` ni de Wikipédia anglaise plus récente que 2016.

## Différences avec le français

- **Wiktionnaire anglais** : `categories` y est une liste d'objets (`{"name": …}`) et non de chaînes ; sans adaptation, chaque entrée échouait à la lecture et le Wiktionnaire était vide sans erreur. Le préfiltre sur `lang_code` dans les 400 premiers octets est désactivé : le fichier ne contient que de l'anglais et `lang_code` apparaît d'abord dans `descendants`.
- **Pseudo-formes** : `no-table-tags` (`table-tags`) et `glossary` (`inflection-template`) sont ignorées (aucun effet sur le français, vérifié).
- **Fautes** : pas de catégorie « Fautes » ; une entrée dont tous les sens sont `alt-of` + `misspelling` est écartée (`recieve`, `definately`, `dont`, `teh`).
- **Complétion** : la règle française du passé simple (`indicative` + `past`) aurait écarté tous les prétérits anglais. En anglais, on écarte les formes `alternative` (les `forms` mêlent flexions et variantes : `colour` sous `color`), `archaic`, `rare`, `obsolete`, `nonstandard`, `dialectal`, `misspelling`, `pronunciation-spelling`. Résultat : ≈ 1 000 formes complétées seulement (morphologie pauvre, formes déjà présentes dans les corpus).
- **Repérage de la langue** : pas de langue parasite (liste vide), le filtre est inactif.
- **OCR des sous-titres** : confusion I/l mesurée sur 3 Go d'OpenSubtitles (`l` 118 k, `l'm` 23 k, `l'll` 17 k, `lt` 17 k, `lf` 16 k, `ls` 14 k, `I'II` 13 k, `Iike` 6,7 k…) ; les 30 formes les plus fréquentes sont corrigées, dans les sous-titres seulement (`l`, `lt` peuvent être corrects ailleurs).
- **Mots d'une lettre** : `a` et `I` (clé `i`).

## Variantes US / GB

Les entrées du Wiktionnaire marquent mal la variante (`colour` : « Commonwealth and Ireland standard spelling of color », sans tag ; `realise` : aucune catégorie britannique). On utilise donc Hunspell :

- `en` : valide si `en_US` **ou** `en_GB` accepte la forme ;
- `en_US` : Hunspell `en_US` ; une forme que `en_US` refuse et que `en_GB` accepte (4 970 formes : `colour`, `centre`, `realise`…) n'est admise ni par le Wiktionnaire ni comme nom propre ;
- `en_GB` : symétrique (5 167 formes : `color`, `center`, `analyze`…). `en_GB-large` accepte l'orthographe d'Oxford : `realize` et `realise` sont tous deux présents.

`en_US` et `en_GB` diffèrent de ≈ 2 550 mots chacun. Les listes `ajouts_en_US.txt` / `exclusions_en_GB.txt` (etc.) permettent d'ajuster une seule variante.

## Résultat

| Variante | Mots    | Bigrammes | `main_*.dict` |
| -------- | ------- | --------- | ------------- |
| `en_US`  | 301 006 | 631 478   | 4,4 Mo        |
| `en_GB`  | 301 023 | 630 836   | 4,4 Mo        |
| `en`     | 301 047 | 632 539   | 4,4 Mo        |

≈ 4 à 5 min par variante sur 16 threads, 10 Go de RAM au maximum (le comptage des sous-titres domine). Le plafond de 300 000 mots est atteint.

## Points ouverts

- **Noms propres du Wiktionnaire** : 103 000 des 300 000 mots ne sont connus que du Wiktionnaire, dont ≈ 50 000 noms propres purs (`Finke`, `Lundgren`, `Gatlinburg`), admis sans la règle des noms propres (≥ 50 occurrences dans la presse et Wikipédia). Le Wiktionary anglais a bien plus d'entrées `name` que le Wiktionnaire français. Piste : soumettre les entrées `name` à la règle des noms propres.
- **Possessifs** : Hunspell accepte `John's`, `today's` ; ≈ 15 000 formes en `'s` (contractions comprises : `it's`, `that's`, `let's`) occupent 5 % du dictionnaire. À comparer avec les dictionnaires anglais d'AOSP et d'HeliBoard.
- **Rejets** : prénoms coréens des sous-titres (`seok`, `joon`), élisions familières (`somethin`, `tryin`, `sayin`) à ajouter éventuellement à `ajouts.txt`.
