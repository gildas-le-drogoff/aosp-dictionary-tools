use std::sync::LazyLock;
use std::time::Instant;

static START: LazyLock<Instant> = LazyLock::new(Instant::now);

macro_rules! log {
    ($($arg:tt)*) => {
        eprintln!("[{:>4.0}s] {}", $crate::START.elapsed().as_secs_f64(), format_args!($($arg)*))
    };
}

mod corpus;
mod hunspell;
mod lang;
mod makedict;
mod morphalou;
mod wiktionary;
mod wordlist;

use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rustc_hash::{FxHashMap, FxHashSet};

use lang::{Download, Lang};
use wordlist::{to_bigram_freq, to_freq};

const BIGRAM_HEADS: usize = 60_000;
const BIGRAMS_PER_WORD: usize = 12;
const MIN_BIGRAM_COUNT: u64 = 5;
const MAX_REJECTS: usize = 3000;
const DICT_VERSION: u32 = 2;
const USAGE: &str = "Usage :
  dictfr build <langue> [<racine>]                             construit <dossier>/build-rs/main_<langue>.dict
  dictfr sources <langue>                                      liste les sources à télécharger
  dictfr makedict [--table] <entrée.combined> <sortie.dict>    compile une liste combined
  dictfr dump <entrée.dict> [<sortie.combined>]                décompile un dictionnaire";

fn main() {
    LazyLock::force(&START);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["makedict", input, output] => makedict_file(Path::new(input), Path::new(output), false),
        ["makedict", "--table", input, output] => {
            makedict_file(Path::new(input), Path::new(output), true)
        }
        ["dump", input] => dump_file(Path::new(input), None),
        ["dump", input, output] => dump_file(Path::new(input), Some(Path::new(output))),
        ["build", code] => language(code).and_then(|l| run(l, Path::new("."))),
        ["build", code, root] => language(code).and_then(|l| run(l, Path::new(root))),
        ["sources", code] => language(code).map(print_sources),
        _ => {
            eprintln!("{USAGE}\n\nLangues : {}", lang::codes());
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("Erreur : {e}");
        std::process::exit(1);
    }
}

fn language(code: &str) -> Result<&'static Lang, Box<dyn Error>> {
    lang::find(code)
        .ok_or_else(|| format!("langue inconnue : {code} (connues : {})", lang::codes()).into())
}

fn print_sources(lang: &Lang) {
    let dir = Path::new(lang.dir).join("sources");
    for d in lang.downloads {
        let (url, file, extra) = match d {
            Download::Url { url, file, member } => (*url, *file, member.unwrap_or("-")),
            Download::Manual { file, how } => ("-", *file, *how),
        };
        println!("{url}\t{}\t{extra}", dir.join(file).display());
    }
}

fn makedict_file(input: &Path, output: &Path, table: bool) -> Result<(), Box<dyn Error>> {
    let text = fs::read_to_string(input).map_err(|e| format!("{} : {e}", input.display()))?;
    let (dict, shortcuts) = makedict::read_combined(&text)?;
    if shortcuts > 0 {
        log!("{shortcuts} raccourcis (shortcut=) ignorés");
    }
    write_dict(&dict, output, table)
}

fn write_dict(dict: &makedict::Dict, output: &Path, table: bool) -> Result<(), Box<dyn Error>> {
    let bytes = makedict::encode(dict, table)?;
    fs::write(output, &bytes)?;
    log!(
        "{} : {} mots, {} octets",
        output.display(),
        dict.words.len(),
        bytes.len()
    );
    Ok(())
}

fn dump_file(input: &Path, output: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(input).map_err(|e| format!("{} : {e}", input.display()))?;
    let mut dict = makedict::decode(&bytes)?;
    dict.words
        .sort_by(|a, b| b.freq.cmp(&a.freq).then_with(|| a.word.cmp(&b.word)));
    let text = makedict::write_combined(&dict);
    match output {
        Some(path) => fs::write(path, text)?,
        None => std::io::stdout().lock().write_all(text.as_bytes())?,
    }
    Ok(())
}

fn run(lang: &Lang, root: &Path) -> Result<(), Box<dyn Error>> {
    let dir = root.join(lang.dir);
    let (sources, lists, out) = (
        dir.join("sources"),
        dir.join("listes"),
        dir.join("build-rs"),
    );
    let code = lang.code;
    for dict in lang.hunspell.iter().chain(lang.rival_hunspell) {
        for ext in ["aff", "dic"] {
            if !Path::new(&format!("{dict}.{ext}")).exists() {
                return Err(format!("dictionnaire Hunspell introuvable : {dict}.{ext}").into());
            }
        }
    }
    let files = lang
        .sources
        .iter()
        .flat_map(|s| s.files.iter())
        .chain([&lang.wiktionary.file])
        .chain(lang.lexicon.as_ref().map(|l| &l.file));
    for file in files {
        let path = sources.join(file);
        if !path.exists() {
            return Err(format!("source introuvable : {}", path.display()).into());
        }
    }
    fs::create_dir_all(&out)?;
    let mut additions = wordlist::read_additions(&lists.join("ajouts.txt"))?;
    let mut exclusions = wordlist::read_list(&lists.join("exclusions.txt"))?;
    if code != lang.dir {
        additions.extend(wordlist::read_additions(
            &lists.join(format!("ajouts_{code}.txt")),
        )?);
        exclusions.extend(wordlist::read_list(
            &lists.join(format!("exclusions_{code}.txt")),
        )?);
    }
    log!(
        "{} threads ; {} ajouts, {} exclusions",
        rayon::current_num_threads(),
        additions.len(),
        exclusions.len()
    );

    log!("Lecture du Wiktionnaire…");
    let wikt = wiktionary::load(&sources.join(lang.wiktionary.file), &lang.wiktionary)?;
    log!(
        "  {} formes, {} lemmes fléchis",
        wikt.forms.len(),
        wikt.paradigms.len()
    );
    let lexicon = match &lang.lexicon {
        Some(lex) => {
            log!("Lecture de {}…", lex.name);
            let paradigms = (lex.load)(&sources.join(lex.file))?;
            log!("  {} lemmes fléchis", paradigms.len());
            paradigms
        }
        None => Vec::new(),
    };

    let mut unigrams = Vec::new();
    for src in lang.sources {
        log!("Comptage des mots : {}…", src.name);
        let uni = corpus::count_unigrams(&sources, src, lang)?;
        let total: u64 = uni.values().map(|n| n[corpus::TOTAL]).sum();
        log!("  {} : {total} occurrences, {} formes", src.name, uni.len());
        unigrams.push(uni);
    }
    let mut keys = wordlist::merge_sources(lang.sources, unigrams, &additions);
    log!("{} clés (formes en minuscules)", keys.len());

    let validation = {
        let candidates = wordlist::candidates(&keys, &additions, &exclusions, lang.one_letter);
        let mut to_check = FxHashSet::default();
        for &key in &candidates {
            to_check.extend(wordlist::checked_forms(key, &keys[key]));
        }
        let mut to_check: Vec<&str> = to_check.into_iter().collect();
        to_check.sort_unstable();
        log!("Hunspell : vérification de {} formes…", to_check.len());
        let accepted = hunspell::check_any(lang.hunspell, &to_check);
        let hun: FxHashSet<&str> = to_check
            .iter()
            .zip(accepted)
            .filter_map(|(w, ok)| ok.then_some(*w))
            .collect();
        log!("  {} formes acceptées par Hunspell", hun.len());
        let others: Vec<&str> = to_check
            .iter()
            .filter(|w| !hun.contains(**w))
            .copied()
            .collect();
        let rival: FxHashSet<&str> = if lang.rival_hunspell.is_empty() {
            FxHashSet::default()
        } else {
            let accepted = hunspell::check_any(lang.rival_hunspell, &others);
            let rival: FxHashSet<&str> = others
                .iter()
                .zip(accepted)
                .filter_map(|(w, ok)| ok.then_some(*w))
                .collect();
            log!("  {} formes propres à la variante concurrente", rival.len());
            rival
        };
        wordlist::validate(
            &keys,
            &candidates,
            &hun,
            &rival,
            &wikt.forms,
            lang.sources,
            &additions,
            &exclusions,
        )
    };
    let by_reason = |r: &str| validation.accepted.iter().filter(|a| a.1 == r).count();
    log!(
        "  {} formes valides dont {} du Wiktionnaire et {} noms propres ; {} rattachements",
        validation.valid.len(),
        by_reason("wiktionnaire"),
        by_reason("nom propre"),
        validation.folds.len()
    );
    let mut report = BufWriter::new(File::create(out.join(format!("{code}_rattachements.tsv")))?);
    writeln!(report, "graphie\tcible\tppm")?;
    for (src, dst) in &validation.folds {
        writeln!(report, "{src}\t{dst}\t{:.3}", keys[src].score)?;
    }
    report.flush()?;
    wordlist::apply_folds(&mut keys, &validation.folds);
    let mut accepted = validation.accepted.clone();
    accepted.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    let mut report = BufWriter::new(File::create(out.join(format!("{code}_admis.tsv")))?);
    writeln!(report, "forme\traison\tppm")?;
    for (form, reason, ppm) in &accepted {
        writeln!(report, "{form}\t{reason}\t{ppm:.3}")?;
    }
    report.flush()?;

    let candidates = wordlist::candidates(&keys, &additions, &exclusions, lang.one_letter);
    let valid: FxHashSet<&str> = validation.valid.iter().map(String::as_str).collect();
    let (entries, rejected) =
        wordlist::select(&keys, &candidates, &valid, lang.sources, &additions);
    let (Some(first), Some(last)) = (entries.first(), entries.last()) else {
        return Err("aucun mot retenu".into());
    };
    let (lmax, lmin) = (first.ppm.ln(), last.ppm.max(wordlist::FREQ_FLOOR_PPM).ln());
    let floor = entries
        .iter()
        .filter(|e| e.ppm <= wordlist::FREQ_FLOOR_PPM)
        .count();
    log!(
        "{} mots retenus (dont {floor} à la fréquence minimale), {} clés rejetées",
        entries.len(),
        rejected.len()
    );
    let completions =
        wordlist::complete(&entries, wikt.paradigms.iter().chain(&lexicon), &exclusions);
    let checked = hunspell::check_any(lang.hunspell, &completions);
    let completions: Vec<&str> = completions
        .into_iter()
        .zip(checked)
        .filter_map(|(f, ok)| ok.then_some(f))
        .collect();
    let from_wiktionary: FxHashSet<&str> = wikt
        .paradigms
        .iter()
        .flat_map(|(_, forms)| forms.iter().map(String::as_str))
        .collect();
    match &lang.lexicon {
        Some(lex) => log!(
            "{} formes fléchies ajoutées depuis le Wiktionnaire et {} (dont {} de {} seul)",
            completions.len(),
            lex.name,
            completions
                .iter()
                .filter(|f| !from_wiktionary.contains(**f))
                .count(),
            lex.name
        ),
        None => log!(
            "{} formes fléchies ajoutées depuis le Wiktionnaire",
            completions.len()
        ),
    }

    let mut rejects = BufWriter::new(File::create(out.join(format!("{code}_rejets.tsv")))?);
    let names: Vec<&str> = lang.sources.iter().map(|s| s.name).collect();
    writeln!(
        rejects,
        "clé\tppm\tformes\t{}\t{}\t{}",
        names.join("\t"),
        lang.markers.code,
        lang.foreign.code
    )?;
    for &key in rejected.iter().take(MAX_REJECTS) {
        let stats = &keys[key];
        let forms: Vec<&str> = stats
            .variants
            .iter()
            .take(3)
            .map(|(f, _)| f.as_str())
            .collect();
        let counts: Vec<String> = stats.per_source.iter().map(u64::to_string).collect();
        writeln!(
            rejects,
            "{key}\t{:.3}\t{}\t{}\t{}\t{}",
            stats.score,
            forms.join(","),
            counts.join("\t"),
            stats.lang[0],
            stats.lang[1]
        )?;
    }
    rejects.flush()?;

    let mut primary: FxHashMap<&str, &str> = FxHashMap::default();
    let mut order = Vec::new();
    for e in &entries {
        primary.entry(e.key).or_insert_with(|| {
            order.push(e.key);
            e.form
        });
    }
    let mut ids: FxHashMap<&str, u32> = order
        .iter()
        .enumerate()
        .map(|(i, &k)| (k, i as u32))
        .collect();

    for (src, dst) in &validation.folds {
        if let Some(&id) = ids.get(dst.as_str()) {
            ids.insert(src, id);
        }
    }
    let heads = order.len().min(BIGRAM_HEADS) as u32;

    let mut pairs: FxHashMap<u64, (f64, u64)> = FxHashMap::default();
    for (i, src) in lang.sources.iter().enumerate() {
        log!("Comptage des bigrammes : {}…", src.name);
        let counts = corpus::count_bigrams(&sources, src, lang, &ids, heads)?;
        log!("  {} : {} bigrammes distincts", src.name, counts.len());
        for (k, n) in counts {
            let a = order[(k >> 32) as usize];
            let pair = pairs.entry(k).or_default();
            pair.0 += src.weight * n as f64 / keys[a].per_source[i] as f64;
            pair.1 += n;
        }
    }
    let mut by_head: Vec<Vec<(f64, u32)>> = vec![Vec::new(); heads as usize];
    for (k, (p, n)) in pairs {
        let b = k as u32;

        if n >= MIN_BIGRAM_COUNT && p * 1e6 > keys[order[b as usize]].score {
            by_head[(k >> 32) as usize].push((p, b));
        }
    }
    for list in &mut by_head {
        list.sort_unstable_by(|x, y| {
            y.0.total_cmp(&x.0)
                .then_with(|| order[y.1 as usize].cmp(order[x.1 as usize]))
        });
        list.truncate(BIGRAMS_PER_WORD);
    }

    let date = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let mut dict = makedict::Dict {
        attributes: [
            ("dictionary", format!("main:{}", code.to_lowercase())),
            ("locale", code.to_owned()),
            ("description", lang.description.to_owned()),
            ("date", date.to_string()),
            ("version", DICT_VERSION.to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect(),
        words: Vec::with_capacity(entries.len() + completions.len()),
    };
    let mut bigram_total = 0;
    for e in &entries {
        let mut word = makedict::Word {
            word: e.form.to_owned(),
            freq: to_freq(e.ppm, lmin, lmax),
            ..Default::default()
        };
        let id = ids[e.key];
        if id < heads && primary[e.key] == e.form {
            for &(p, b) in &by_head[id as usize] {
                let next = primary[order[b as usize]];
                word.bigrams
                    .push((next.to_owned(), to_bigram_freq(p * 1e6, lmin, lmax)));
                bigram_total += 1;
            }
        }
        dict.words.push(word);
    }
    for form in completions {
        dict.words.push(makedict::Word {
            word: form.to_owned(),
            freq: 1,
            ..Default::default()
        });
    }
    let path = out.join(format!("{code}_wordlist.combined"));
    fs::write(&path, makedict::write_combined(&dict))?;
    log!(
        "{} mots et {bigram_total} bigrammes écrits dans {}",
        dict.words.len(),
        path.display()
    );
    write_dict(&dict, &out.join(format!("main_{code}.dict")), false)
}
