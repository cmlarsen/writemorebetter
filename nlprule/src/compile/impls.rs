use bimap::BiMap;
use fs_err::File;
use fst::Map;
use log::warn;
use serde::{Deserialize, Serialize};
use std::{
    cmp,
    collections::{BTreeSet, HashMap, HashSet},
    hash::{Hash, Hasher},
    io::{self, BufRead, BufReader},
    iter::once,
    path::{Path, PathBuf},
};

use crate::{
    builtin::BuiltinRule,
    rule::{
        disambiguation::PosFilter,
        engine::{
            composition::{GraphId, Matcher, MatcherKind, PartInterner, PosMatcher, TextMatcher},
            Engine,
        },
        id::Category,
        DisambiguationRule, Repetition, Rule,
    },
    rules::{Rules, RulesLangOptions},
    spell::{
        multitoken::{normalize_key, MultitokenSpeller},
        Spell, SpellInt, SpellLangOptions,
    },
    tokenizer::{
        chunk,
        multiword::{MultiwordTagger, MultiwordTaggerFields},
        tag::{Tagger, TaggerFields, TaggerLangOptions},
        Tokenizer, TokenizerLangOptions,
    },
    types::*,
    utils::{
        apply_to_first, parallelism::MaybeParallelIterator, regex::Regex,
        uppercase_first_letter_or_digit,
    },
};

use super::{parse_structure::BuildInfo, Error};

impl Spell {
    fn new(
        fst: Vec<u8>,
        multiwords: DefaultHashMap<String, Vec<(Vec<String>, SpellInt)>>,
        phrases: Vec<u8>,
        max_freq: usize,
        map: DefaultHashMap<String, String>,
        lang_options: SpellLangOptions,
    ) -> Self {
        let mut spell = Spell {
            fst,
            multiwords,
            phrases,
            max_freq,
            map,
            lang_options,
            ..Default::default()
        };
        spell.ingest_options();
        spell
    }

    fn add_line(
        word: &str,
        freq: usize,
        words: &mut DefaultHashMap<String, SpellInt>,
        multiwords: &mut DefaultHashMap<String, Vec<(Vec<String>, SpellInt)>>,
        variant_index: usize,
        // in some LT lists an underline denotes a prefix such that e.g. "hin_reiten" also adds "hingeritten"
        underline_denotes_prefix: bool,
        tokenizer: &Tokenizer,
    ) {
        let tokens = tokenizer.get_token_strs(word);

        if tokens.len() > 1 {
            assert!(!(underline_denotes_prefix && word.contains('_'))); // not supported in entries spanning multiple tokens

            let mut int = SpellInt::default();
            int.add_variant(variant_index);

            // we do not add the frequency for multiwords - they are not used for suggestions, just to check validity

            multiwords
                .entry(tokens[0].to_owned())
                .or_insert_with(Vec::new)
                .push((tokens[1..].iter().map(|x| (*x).to_owned()).collect(), int));
        } else if word.contains('_') && underline_denotes_prefix {
            assert!(!word.contains('\\')); // escaped underlines are not supported
            let mut parts = word.split('_');

            let prefix = parts.next().unwrap();
            let suffix = parts.next().unwrap();

            // TODO: add the inflected forms of the suffix. Upstream did this with the `PosReplacer`, which is unimplemented on this branch.
            warn!(
                "Only adding the base form of prefixed spelling entry \"{}\".",
                word
            );

            let value = words.entry(format!("{}{}", prefix, suffix)).or_default();
            value.add_variant(variant_index);
            value.update_freq(freq);
        } else {
            let value = words.entry((*word).to_owned()).or_default();

            value.update_freq(freq);
            value.add_variant(variant_index);
        }
    }

    fn casing_variants(entry: &str) -> Vec<String> {
        let is_camel_case = {
            let rest = entry.trim_start_matches(|c: char| c.is_ascii_lowercase());
            let mut chars = rest.chars();
            rest.len() < entry.len()
                && chars.next().is_some_and(|c| c.is_ascii_uppercase())
                && !chars.as_str().is_empty()
                && chars.all(|c| c.is_ascii_alphabetic())
        };

        let mut variants = vec![entry.to_owned()];
        let mut push = |variant: String| {
            if !variants.contains(&variant) {
                variants.push(variant);
            }
        };
        if !is_camel_case {
            push(entry.to_uppercase());
        }
        push(apply_to_first(entry, |c| c.to_uppercase().collect()));
        variants
    }

    fn global_entries(path: &Path) -> io::Result<Vec<String>> {
        let mut entries = Vec::new();

        let reader = BufReader::new(File::open(path)?);
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let entry = line[..line.find('#').unwrap_or(line.len())].trim();
            if !entry.is_empty() {
                entries.push(entry.to_owned());
            }
        }

        Ok(entries)
    }

    fn global_phrases(entries: &[String], tokenizer: &Tokenizer) -> Vec<u8> {
        let mut phrases = BTreeSet::new();

        for entry in entries {
            for variant in Spell::casing_variants(entry) {
                let tokens = tokenizer.get_token_strs(&variant);
                if !tokens.is_empty() {
                    phrases.insert(tokens.join(" "));
                }
            }
        }

        fst::Set::from_iter(phrases)
            .expect("phrases are lexicographically sorted.")
            .into_fst()
            .to_vec()
    }

    pub(in crate::compile) fn from_dumps(
        spell_dir_path: impl AsRef<Path>,
        map_path: impl AsRef<Path>,
        global_word_path: impl AsRef<Path>,
        global_phrase_path: impl AsRef<Path>,
        lang_options: SpellLangOptions,
        tokenizer: &Tokenizer,
    ) -> io::Result<Self> {
        let mut words: DefaultHashMap<String, SpellInt> = DefaultHashMap::default();
        let mut multiwords: DefaultHashMap<String, Vec<(Vec<String>, SpellInt)>> =
            DefaultHashMap::default();
        let mut max_freq = 0;

        if let Some(default) = &lang_options.default_variant {
            if !lang_options.variants.contains(default) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "default spell variant \"{}\" is not one of the variants {:?}",
                        default.as_str(),
                        lang_options.variants
                    ),
                ));
            }
        }

        let global_entries = if global_phrase_path.as_ref().exists() {
            Some(Spell::global_entries(global_phrase_path.as_ref())?)
        } else {
            None
        };
        let global_words: Vec<&str> = global_entries
            .iter()
            .flatten()
            .map(String::as_str)
            .filter(|entry| tokenizer.get_token_strs(entry).len() == 1)
            .collect();

        for (i, variant) in lang_options.variants.iter().enumerate() {
            let spell_path = spell_dir_path
                .as_ref()
                .join(variant.as_str())
                .with_extension("dump");

            let reader = BufReader::new(File::open(&spell_path)?);
            for line in reader.lines() {
                match line?
                    .trim()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    [freq, word] => {
                        // frequency is denoted as letters from A to Z in LanguageTool where A is the least frequent.
                        let freq = freq.chars().next().expect("freq must have one char - would not have been yielded by split_whitespace otherwise.") as usize - 'A' as usize + 1;

                        Spell::add_line(
                            word,
                            freq,
                            &mut words,
                            &mut multiwords,
                            i,
                            false,
                            tokenizer,
                        );
                        max_freq = cmp::max(max_freq, freq);
                    }
                    _ => continue,
                }
            }

            let global_word_reader = BufReader::new(File::open(global_word_path.as_ref())?);

            let extra_word_path = spell_path.with_extension("txt");
            let reader = BufReader::new(File::open(&extra_word_path)?);
            for line in reader.lines().chain(global_word_reader.lines()) {
                let line = line?;
                let word = line.trim();

                Spell::add_line(word, 0, &mut words, &mut multiwords, i, true, tokenizer);
            }

            for word in &global_words {
                Spell::add_line(word, 0, &mut words, &mut multiwords, i, false, tokenizer);
            }
        }
        let mut words: Vec<_> = words
            .into_iter()
            .map(|(key, value)| (key, value.as_u64()))
            .collect();
        words.sort_by(|(a, _), (b, _)| a.cmp(b));

        let fst =
            fst::Map::from_iter(words.into_iter()).expect("words are lexicographically sorted.");

        let mut map = DefaultHashMap::default();
        let reader = BufReader::new(File::open(map_path.as_ref())?);
        for line in reader.lines() {
            let line = line?;

            let mut parts = line.split('=');
            let wrong = parts
                .next()
                .expect("spell map line must have part before =")
                .to_owned();
            let right = parts
                .next()
                .expect("spell map line must have part after =")
                .to_owned();

            // map lookup happens on token level, so the key has to be exactly one token
            assert_eq!(tokenizer.get_token_strs(&wrong).len(), 1);

            map.insert(wrong, right);
            assert!(parts.next().is_none());
        }

        let phrases = global_entries
            .map(|entries| Spell::global_phrases(&entries, tokenizer))
            .unwrap_or_default();

        Ok(Spell::new(
            fst.into_fst().to_vec(),
            multiwords,
            phrases,
            max_freq,
            map,
            lang_options,
        ))
    }
}

impl MultitokenSpeller {
    /// Reads the phrase list (one spelling per line), as `MultitokenSpeller.initMultitokenSpeller`
    /// does after `prepareLineForSpeller`. Single-word keys are skipped.
    pub(in crate::compile) fn from_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut by_first_char: DefaultHashMap<char, Vec<(String, Vec<String>)>> =
            DefaultHashMap::default();
        let mut key_indices: DefaultHashMap<String, usize> = DefaultHashMap::default();
        let mut by_key_without_spaces: DefaultHashMap<String, Vec<String>> =
            DefaultHashMap::default();

        let push_unique = |spellings: &mut Vec<String>, line: &str| {
            if !spellings.iter().any(|x| x == line) {
                spellings.push(line.to_owned());
            }
        };

        for line in BufReader::new(File::open(path.as_ref())?).lines() {
            let line = line?;
            let key = normalize_key(&line);
            if !key.contains(' ') {
                continue;
            }
            let first = key.chars().next().expect("key contains a space");
            let entries = by_first_char.entry(first).or_default();
            let index = *key_indices.entry(key.clone()).or_insert_with(|| {
                entries.push((key.clone(), Vec::new()));
                entries.len() - 1
            });
            push_unique(&mut entries[index].1, &line);
            push_unique(
                by_key_without_spaces
                    .entry(key.replace(' ', ""))
                    .or_default(),
                &line,
            );
        }

        for entries in by_first_char.values_mut() {
            let mask = java_hash_map_capacity(entries.len()) - 1;
            entries.sort_by_key(|(key, _)| java_hash_map_hash(key) & mask);
        }

        Ok(MultitokenSpeller {
            by_first_char,
            by_key_without_spaces,
        })
    }
}

/// `HashMap.hash(key)` for a Java `String` key: `String.hashCode` with its high half folded in.
fn java_hash_map_hash(key: &str) -> u32 {
    let hash = key
        .encode_utf16()
        .fold(0u32, |hash, c| hash.wrapping_mul(31).wrapping_add(c as u32));
    hash ^ (hash >> 16)
}

/// The table size of a Java `HashMap` after `len` insertions at the default load factor.
fn java_hash_map_capacity(len: usize) -> u32 {
    let mut capacity = 16;
    while len > capacity * 3 / 4 {
        capacity *= 2;
    }
    capacity as u32
}

impl Tagger {
    fn get_lines<S1: AsRef<Path>, S2: AsRef<Path>>(
        paths: &[S1],
        remove_paths: &[S2],
    ) -> std::io::Result<Vec<(String, String, String)>> {
        let mut output = Vec::new();
        let mut disallowed: Vec<String> = Vec::new();

        for path in remove_paths {
            let file = File::open(path.as_ref())?;
            let reader = std::io::BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                if line.starts_with('#') {
                    continue;
                }

                disallowed.push(line.to_string());
            }
        }

        for path in paths {
            let file = File::open(path.as_ref())?;
            let reader = std::io::BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                if line.starts_with('#') {
                    continue;
                }

                if disallowed.contains(&line) {
                    continue;
                }

                let parts: Vec<_> = line.split('\t').collect();

                let word = parts[0].to_string();
                let inflection = parts[1].to_string();
                let tag = parts[2].to_string();

                output.push((word, inflection, tag))
            }
        }

        Ok(output)
    }

    /// Creates a tagger from raw files.
    ///
    /// # Arguments
    /// * `paths`: Paths to files where each line contains the word, lemma and tag, respectively,
    /// separated by tabs, to be added to the tagger.
    /// * `remove_paths`: Paths to files where each line contains the word, lemma and tag, respectively,
    /// separated by tabs, to be removed from the tagger if present in the files from `paths`.
    pub(in crate::compile) fn from_dumps<S1: AsRef<Path>, S2: AsRef<Path>>(
        paths: &[S1],
        remove_paths: &[S2],
        common_words: &HashSet<String>,
        lang_options: TaggerLangOptions,
    ) -> std::io::Result<Self> {
        let mut tag_store = HashSet::new();
        let mut word_store = HashSet::new();

        // add language specific special tags
        tag_store.extend(lang_options.extra_tags.iter().map(|x| x.as_str()));

        let lines = Tagger::get_lines(paths, remove_paths)?;

        let punct = "!\"#$%&\\'()*+,-./:;<=>?@[\\]^_`{|}~";
        for i in 0..punct.len() {
            word_store.insert(&punct[i..(i + 1)]);
        }

        word_store.extend(common_words.iter().map(|x| x.as_str()));

        for (word, inflection, tag) in lines.iter() {
            word_store.insert(word);
            word_store.insert(inflection);
            tag_store.insert(tag);
        }

        // the empty string must not be part of any wordlist
        assert!(!word_store.contains(""));

        // word store ids should be consistent across runs
        let mut word_store: Vec<_> = word_store.into_iter().collect();
        word_store.sort_unstable();

        // add special empty string to wordlist, must be the first element to have id 0
        word_store.insert(0, "");

        // tag store ids should be consistent across runs
        let mut tag_store: Vec<_> = tag_store.into_iter().collect();
        tag_store.sort_unstable();

        // add special part of speech tags, they must have ids starting from zero
        for (i, special_pos) in SpecialPos::iter().enumerate() {
            tag_store.insert(i, special_pos);
        }

        let word_store: BiMap<_, _> = word_store
            .iter()
            .enumerate()
            .map(|(i, x)| (x.to_string(), WordIdInt::from_value_unchecked(i as u32)))
            .collect();
        let tag_store: DefaultBiMap<_, _> = tag_store
            .iter()
            .enumerate()
            .map(|(i, x)| (x.to_string(), PosIdInt::from_value_unchecked(i as u16)))
            .collect();

        let mut tags: Vec<Option<Vec<(WordIdInt, PosIdInt)>>> = vec![None; word_store.len()];

        for (word, inflection, tag) in lines.iter() {
            let word_id = word_store.get_by_left(word).unwrap();
            let lemma_id = word_store.get_by_left(inflection).unwrap();
            let pos_id = tag_store.get_by_left(tag).unwrap();

            match &mut tags[word_id.value() as usize] {
                Some(vec) => {
                    vec.push((*lemma_id, *pos_id));
                }
                None => {
                    tags[word_id.value() as usize] = Some(vec![(*lemma_id, *pos_id)]);
                }
            }
        }

        // Build the tag FST directly from the intermediate maps: word bytes + one index byte
        // -> packed (lemma id, pos id). This is the same layout the loaded tagger queries.
        let mut tag_fst_items: Vec<(Vec<u8>, u64)> = Vec::new();

        for (word_id, map) in tags.iter().enumerate() {
            if let Some(map) = map {
                let word = word_store
                    .get_by_right(&WordIdInt::from_value_unchecked(word_id as u32))
                    .expect("word id exists");

                for (i, (inflect_id, pos_id)) in map.iter().enumerate() {
                    assert!(i < 255);

                    let key: Vec<u8> = word
                        .as_bytes()
                        .iter()
                        .chain(once(&(i as u8)))
                        .copied()
                        .collect();
                    let pos_bytes = pos_id.value().to_be_bytes();
                    let inflect_bytes = inflect_id.value().to_be_bytes();

                    let value = u64::from_be_bytes([
                        inflect_bytes[0],
                        inflect_bytes[1],
                        inflect_bytes[2],
                        inflect_bytes[3],
                        0,
                        0,
                        pos_bytes[0],
                        pos_bytes[1],
                    ]);
                    tag_fst_items.push((key, value));
                }
            }
        }

        tag_fst_items.sort_by(|(a, _), (b, _)| a.cmp(b));
        let tag_fst = Map::from_iter(tag_fst_items)
            .unwrap()
            .into_fst()
            .as_bytes()
            .to_vec();

        // Build the reverse synth FST: lemma bytes + one index byte -> packed (word id, pos id).
        // Grouping the triples by lemma lets a synthesis lookup for a lemma enumerate every word
        // form carrying it, and the POS tag lets the caller filter by target tag.
        let mut synth: Vec<Option<Vec<(WordIdInt, PosIdInt)>>> = vec![None; word_store.len()];

        for (word, inflection, tag) in lines.iter() {
            let word_id = *word_store.get_by_left(word).unwrap();
            let lemma_id = *word_store.get_by_left(inflection).unwrap();
            let pos_id = *tag_store.get_by_left(tag).unwrap();

            match &mut synth[lemma_id.value() as usize] {
                Some(vec) => vec.push((word_id, pos_id)),
                None => synth[lemma_id.value() as usize] = Some(vec![(word_id, pos_id)]),
            }
        }

        let mut synth_fst_items: Vec<(Vec<u8>, u64)> = Vec::new();

        for (lemma_id, map) in synth.iter().enumerate() {
            if let Some(map) = map {
                let lemma = word_store
                    .get_by_right(&WordIdInt::from_value_unchecked(lemma_id as u32))
                    .expect("word id exists");

                for (i, (word_id, pos_id)) in map.iter().enumerate() {
                    assert!(i < 255);

                    let key: Vec<u8> = lemma
                        .as_bytes()
                        .iter()
                        .chain(once(&(i as u8)))
                        .copied()
                        .collect();
                    let pos_bytes = pos_id.value().to_be_bytes();
                    let word_bytes = word_id.value().to_be_bytes();

                    let value = u64::from_be_bytes([
                        word_bytes[0],
                        word_bytes[1],
                        word_bytes[2],
                        word_bytes[3],
                        0,
                        0,
                        pos_bytes[0],
                        pos_bytes[1],
                    ]);
                    synth_fst_items.push((key, value));
                }
            }
        }

        synth_fst_items.sort_by(|(a, _), (b, _)| a.cmp(b));
        let synth_fst = Map::from_iter(synth_fst_items)
            .unwrap()
            .into_fst()
            .as_bytes()
            .to_vec();

        let mut word_store_items: Vec<(String, u64)> = word_store
            .iter()
            .map(|(key, value)| (key.clone(), value.value() as u64))
            .collect();
        word_store_items.sort_by(|(a, _), (b, _)| a.cmp(b));
        let word_store_fst = Map::from_iter(word_store_items)
            .unwrap()
            .into_fst()
            .as_bytes()
            .to_vec();

        Ok(TaggerFields {
            tag_fst,
            synth_fst,
            word_store_fst,
            tag_store,
            lang_options,
            anchors: Vec::new(),
        }
        .into())
    }
}

fn is_camel_case(text: &str) -> bool {
    let lower = text.chars().take_while(|c| c.is_ascii_lowercase()).count();
    let rest: Vec<char> = text.chars().skip(lower).collect();
    lower > 0
        && rest.len() >= 2
        && rest[0].is_ascii_uppercase()
        && rest[1..].iter().all(|c| c.is_ascii_alphabetic())
}

fn casing_variants(
    phrase: &str,
    known: &DefaultHashMap<String, usize>,
    options: &TokenizerLangOptions,
) -> Vec<String> {
    let mut variants = Vec::new();
    if options.multiword_all_uppercase && !is_camel_case(phrase) {
        let upper = phrase.to_uppercase();
        if !known.contains_key(&upper) && upper != phrase {
            variants.push(upper);
        }
    }
    if options.multiword_first_capitalized {
        let first_capitalized = uppercase_first_letter_or_digit(phrase);
        if !known.contains_key(&first_capitalized) && first_capitalized != phrase {
            variants.push(first_capitalized);
        }
    }
    variants
}

impl MultiwordTagger {
    pub(in crate::compile) fn from_dump<P: AsRef<Path>>(
        dump: P,
        info: &BuildInfo,
        options: &TokenizerLangOptions,
    ) -> Result<Self, io::Error> {
        let reader = BufReader::new(File::open(dump.as_ref())?);
        let mut multiwords: Vec<(String, String, owned::PosId)> = Vec::new();
        let mut indices: DefaultHashMap<String, usize> = DefaultHashMap::default();

        for line in reader.lines() {
            let line = line?;

            // strip comments
            let line = &line[..line.find('#').unwrap_or_else(|| line.len())].trim();
            if line.is_empty() {
                continue;
            }
            let tab_split: Vec<_> = line.split('\t').collect();

            let word = tab_split[0].to_owned();
            let pos = info.tagger().id_tag(tab_split[1]).to_owned_id();

            let variants = casing_variants(&word, &indices, options);
            for phrase in std::iter::once(word.clone()).chain(variants) {
                let entry = (phrase.clone(), word.clone(), pos.clone());
                match indices.get(&phrase) {
                    Some(&i) => multiwords[i] = entry,
                    None => {
                        indices.insert(phrase, multiwords.len());
                        multiwords.push(entry);
                    }
                }
            }
        }

        Ok((MultiwordTaggerFields { multiwords }).into())
    }
}

impl TextMatcher {
    pub(in crate::compile) fn new(matcher: Matcher, info: &mut BuildInfo) -> Self {
        // can not cache a matcher that depends on the graph
        let set = if matcher.graph_id().is_some() {
            None
        } else if let MatcherKind::Regex(regex) = &matcher.matcher {
            let mut hasher = DefaultHasher::default();
            regex.hash(&mut hasher);
            matcher.negate.hash(&mut hasher);
            matcher.empty_always_false.hash(&mut hasher);
            let matcher_hash = hasher.finish();

            if let Some(set) = info.mut_regex_cache().get(&matcher_hash) {
                set.clone()
            } else {
                let data: Vec<_> = info.tagger().word_store().collect();

                let set: DefaultHashSet<_> = data
                    .into_maybe_par_iter()
                    .filter_map(|(word, id)| {
                        if matcher.is_match(word.as_str(), None, None) {
                            Some(id)
                        } else {
                            None
                        }
                    })
                    .collect();

                // there are some regexes which match lots of strings
                // this cutoff is pretty arbitrary but without any threshold the size of some sets blows up
                // the vast majority of regexes matches less than 100 strings from manual inspection
                let set = if set.len() > 100 { None } else { Some(set) };
                info.mut_regex_cache().insert(matcher_hash, set.clone());
                set
            }
        } else {
            None
        };

        TextMatcher {
            matcher,
            set: set.map(|set| {
                let mut ids: Vec<WordIdInt> = set.into_iter().collect();
                ids.sort_unstable();
                ids.into_boxed_slice()
            }),
        }
    }
}

impl PosMatcher {
    pub(in crate::compile) fn new(matcher: Matcher, info: &mut BuildInfo) -> Self {
        let mut mask = vec![false; info.tagger().tag_store().len()];

        for (word, id) in info.tagger().tag_store().iter() {
            mask[id.value() as usize] = matcher.is_match(word.as_str(), None, None);
        }

        PosMatcher { mask }
    }
}

/// Whether a LanguageTool `tags` attribute marks the rule as opt-in
/// (`tags="picky"`, i. e. Level.PICKY). The value is a space separated list.
fn has_picky_tag(tags: Option<&str>) -> bool {
    tags.map_or(false, |tags| tags.split_whitespace().any(|x| x == "picky"))
}

const DEFAULT_REPETITION_DISTANCE_TOKENS: usize = 60;

fn repetition(
    min_prev_matches: Option<&str>,
    distance_tokens: Option<&str>,
) -> Result<Option<Repetition>, std::num::ParseIntError> {
    let parse = |value: Option<&str>| value.map_or(Ok(0), |x| x.trim().parse::<usize>());

    let min_prev_matches = parse(min_prev_matches)?;
    if min_prev_matches == 0 {
        return Ok(None);
    }

    let distance_tokens = match parse(distance_tokens)? {
        0 => DEFAULT_REPETITION_DISTANCE_TOKENS * min_prev_matches,
        x => x,
    };

    Ok(Some(Repetition {
        min_prev_matches,
        distance_tokens,
    }))
}

fn rules_from_xml<P: AsRef<Path>>(
    path: P,
    build_info: &mut BuildInfo,
    options: &RulesLangOptions,
    pool: &mut PartInterner,
    errors: &mut HashMap<String, usize>,
) -> Vec<Rule> {
    super::parse_structure::read_rules(path)
        .into_iter()
        .filter_map(|x| match x {
            Ok((rule_structure, group, category)) => {
                let category = category.expect("grammar rules must have category");
                let id = Category::new(category.id.as_str());

                let id = if let Some(group) = &group {
                    id.join(group.id.as_str()).join(group.n)
                } else {
                    id.join(
                        rule_structure
                            .id
                            .as_ref()
                            .expect("ID must be set if not in group."),
                    )
                    .join(0)
                };

                let repetition = match repetition(
                    rule_structure
                        .min_prev_matches
                        .as_deref()
                        .or_else(|| group.as_ref().and_then(|g| g.min_prev_matches.as_deref())),
                    rule_structure
                        .distance_tokens
                        .as_deref()
                        .or_else(|| group.as_ref().and_then(|g| g.distance_tokens.as_deref())),
                ) {
                    Ok(repetition) => repetition,
                    Err(x) => {
                        *errors
                            .entry(format!("[Rule] repetition attribute: {}", x))
                            .or_insert(0) += 1;
                        return None;
                    }
                };

                let rule_on = match rule_structure.default.as_deref() {
                    Some("off") | Some("temp_off") => false,
                    Some("on") | None => true,
                    Some(x) => panic!("unknown `default` value: {}", x),
                };

                let group_on = match group.as_ref().and_then(|x| x.default.as_deref()) {
                    Some("off") | Some("temp_off") => false,
                    Some("on") | None => true,
                    Some(x) => panic!("unknown `default` value: {}", x),
                };

                let category_on = match category.default.as_deref() {
                    Some("off") | Some("temp_off") => false,
                    Some("on") | None => true,
                    Some(x) => panic!("unknown `default` value: {}", x),
                };

                // `tags="picky"` rules are LanguageTool's Level.PICKY and are
                // off by default. Rulegroup tags apply to every rule inside.
                let picky = has_picky_tag(rule_structure.tags.as_deref())
                    || group
                        .as_ref()
                        .map_or(false, |g| has_picky_tag(g.tags.as_deref()));

                let name = rule_structure.name.as_ref().map_or_else(
                    || {
                        let group = group.as_ref().expect("must have group if name not set");
                        group.name.clone()
                    },
                    |x| x.clone(),
                );

                match Rule::from_rule_structure(rule_structure, build_info, pool) {
                    Ok(mut rule) => {
                        if options.ignore_ids.iter().any(|x| x.is_match(&id)) {
                            warn!("Skipping rule {}: it is listed in `ignore_ids`.", id);
                            None
                        } else if options.ids.is_empty()
                            || options.ids.iter().any(|x| x.is_match(&id))
                        {
                            rule.id = id;
                            rule.name = name;
                            rule.category_name = category.name;
                            rule.category_type = category.kind;
                            rule.enabled = category_on && group_on && rule_on && !picky;
                            rule.repetition = repetition;
                            Some(rule)
                        } else {
                            None
                        }
                    }
                    Err(x) => {
                        *errors.entry(format!("[Rule] {}", x)).or_insert(0) += 1;
                        None
                    }
                }
            }
            Err(x) => {
                *errors.entry(format!("[Structure] {}", x)).or_insert(0) += 1;
                None
            }
        })
        .collect()
}

impl Rules {
    pub(in crate::compile) fn from_xml<P: AsRef<Path>>(
        path: P,
        mother_tongue_paths: &[(String, PathBuf)],
        build_info: &mut BuildInfo,
        spell: Spell,
        options: RulesLangOptions,
        builtins: Vec<BuiltinRule>,
    ) -> Self {
        let mut errors: HashMap<String, usize> = HashMap::new();
        let mut pool = PartInterner::default();

        let mut rules = rules_from_xml(path, build_info, &options, &mut pool, &mut errors);
        for (mother_tongue, path) in mother_tongue_paths {
            rules.extend(
                rules_from_xml(path, build_info, &options, &mut pool, &mut errors)
                    .into_iter()
                    .map(|mut rule| {
                        rule.mother_tongue = Some(mother_tongue.clone());
                        rule
                    }),
            );
        }

        if !errors.is_empty() {
            let mut errors: Vec<(String, usize)> = errors.into_iter().collect();
            errors.sort_by_key(|x| -(x.1 as i32));

            warn!(
                "Errors constructing Rules: {:#?}",
                &errors
                    .iter()
                    .map(|(message, number)| format!("{} (n={})", message, number))
                    .collect::<Vec<_>>()
            );
        }

        // The article word lists for `+DT`/`+INDT` synthesis are identical for every rule that
        // uses them; load them once here instead of once per rule.
        let articles = if rules.iter().any(|rule| rule.uses_articles) {
            Some(super::builtin::article_data(build_info.build_dir()))
        } else {
            None
        };

        Rules {
            rules,
            spell,
            builtins,
            articles,
            parts: pool.into_pool(),
            batch_size: None,
            mother_tongue: None,
        }
    }
}

impl Tokenizer {
    pub(in crate::compile) fn from_xml<P: AsRef<Path>>(
        path: P,
        build_info: &mut BuildInfo,
        chunker: Option<chunk::Chunker>,
        multiword_tagger: Option<MultiwordTagger>,
        sentencizer: srx::Rules,
        lang_options: TokenizerLangOptions,
    ) -> Result<Self, Error> {
        let rules = super::parse_structure::read_disambiguation_rules(path);
        let mut errors: HashMap<String, usize> = HashMap::new();
        let mut pool = PartInterner::default();

        let rules: Vec<_> = rules
            .into_iter()
            .filter_map(|x| match x {
                Ok((rule_structure, group, _)) => {
                    let id = Category::new("DISAMBIGUATION");

                    let id = if let Some(group) = &group {
                        id.join(group.id.as_str()).join(group.n)
                    } else {
                        id.join(
                            rule_structure
                                .id
                                .as_ref()
                                .expect("ID must be set if not in group."),
                        )
                        .join(0)
                    };

                    match DisambiguationRule::from_rule_structure(
                        rule_structure,
                        build_info,
                        &mut pool,
                    ) {
                        Ok(mut rule) => {
                            if (lang_options.ids.is_empty()
                                || lang_options.ids.iter().any(|x| x.is_match(&id)))
                                && !lang_options.ignore_ids.iter().any(|x| x.is_match(&id))
                            {
                                rule.id = id;

                                Some(rule)
                            } else {
                                None
                            }
                        }
                        Err(x) => {
                            *errors.entry(format!("[Rule] {}", x)).or_insert(0) += 1;
                            None
                        }
                    }
                }
                Err(x) => {
                    *errors.entry(format!("[Structure] {}", x)).or_insert(0) += 1;
                    None
                }
            })
            .collect();

        if !errors.is_empty() {
            let mut errors: Vec<(String, usize)> = errors.into_iter().collect();
            errors.sort_by_key(|x| -(x.1 as i32));

            let summary: Vec<String> = errors
                .iter()
                .map(|(message, number)| format!("{} (n={})", message, number))
                .collect();

            if lang_options.allow_errors {
                warn!("Errors constructing Disambiguator: {:#?}", summary);
            } else {
                return Err(Error::Unexpected(format!(
                    "Errors constructing Disambiguator: {:#?}",
                    summary
                )));
            }
        }

        Ok(Tokenizer {
            tagger: build_info.tagger().clone(),
            sentencizer,
            chunker,
            multiword_tagger,
            rules,
            lang_options,
            parts: pool.into_pool(),
        })
    }
}

#[derive(Deserialize)]
struct ModelData {
    outcome_labels: Vec<String>,
    pmap: DefaultHashMap<String, ContextData>,
}

#[derive(Serialize, Deserialize)]
pub(in crate::compile) struct ContextData {
    parameters: Vec<f32>,
    outcomes: Vec<usize>,
}

impl From<ModelData> for chunk::Model {
    fn from(data: ModelData) -> Self {
        let mut outcomes: Vec<u8> = Vec::new();
        let mut parameters: Vec<f32> = Vec::new();

        let mut contexts: Vec<_> = data.pmap.into_iter().collect();
        contexts.sort_unstable_by(|a, b| a.0.cmp(&b.0));

        let pmap = contexts.into_iter().map(|(key, value)| {
            assert_eq!(value.outcomes.len(), value.parameters.len());

            let offset = outcomes.len();
            let length = value.outcomes.len();

            outcomes.extend(value.outcomes.into_iter().map(|outcome| {
                assert!(outcome <= u8::MAX as usize);
                outcome as u8
            }));
            parameters.extend(value.parameters);

            (chunk::hash::hash_str(&key), (offset, length))
        });

        let pmap = chunk::Pmap::from_pairs(pmap);

        chunk::Model {
            outcome_labels: data.outcome_labels,
            outcomes,
            parameters,
            pmap,
        }
    }
}

impl chunk::Chunker {
    pub(in crate::compile) fn from_json<R: std::io::Read>(
        reader: R,
    ) -> Result<chunk::Chunker, serde_json::Error> {
        #[derive(Deserialize)]
        struct ChunkData {
            token_model: ModelData,
            pos_model: ModelData,
            pos_tagdict: DefaultHashMap<String, Vec<String>>,
            chunk_model: ModelData,
        }

        let chunk_data: ChunkData = serde_json::from_reader(reader)?;
        Ok(chunk::Chunker {
            token_model: chunk::MaxentTokenizer {
                model: chunk_data.token_model.into(),
            },
            pos_model: chunk::MaxentPosTagger {
                model: chunk_data.pos_model.into(),
                tagdict: chunk_data.pos_tagdict.into(),
            },
            chunk_model: chunk::MaxentChunker {
                model: chunk_data.chunk_model.into(),
            },
        })
    }
}

impl PosFilter {
    pub(in crate::compile) fn new(matcher: PosMatcher) -> Self {
        PosFilter { matcher }
    }
}

impl Regex {
    pub(in crate::compile) fn from_java_regex(
        java_regex_str: &str,
        full_match: bool,
        case_sensitive: bool,
    ) -> Result<Self, Error> {
        let regex_string =
            super::utils::from_java_regex(java_regex_str, case_sensitive, full_match)?;

        let regex = Regex::new(regex_string);
        if let Err(error) = regex.try_compile() {
            return Err(Error::Regex(error));
        }

        Ok(regex)
    }
}

impl Engine {
    pub(in crate::compile) fn to_graph_id(&self, id: usize) -> Result<GraphId, Error> {
        let mut id = GraphId(id);

        match &self {
            Engine::Token(engine) => {
                let map = &engine.composition.id_to_idx;
                if map.is_empty() {
                    return Err(Error::Unexpected("graph is empty".into()));
                }
                let max_id = GraphId(map.len() - 1);
                if id.0 >= map.len() {
                    id = max_id;
                }
            }
            Engine::Text(_, map) => {
                let max_id = *map
                    .keys()
                    .max()
                    .ok_or_else(|| Error::Unexpected("graph is empty".into()))?;

                // ideally this should throw an error but LT is more lenient than nlprule
                if !map.contains_key(&id) {
                    id = max_id;
                }
            }
        }

        Ok(id)
    }
}

mod composition {
    use super::*;
    use crate::{
        rule::engine::composition::{
            AffixedGraph, AndAtom, Atom, Composition, FalseAtom, GraphId, NotAtom, OffsetAtom,
            OrAtom, Part, PartInterner, Quantifier, TrueAtom,
        },
        utils::regex::Regex,
    };

    impl Atom {
        fn iter_mut<'a>(&'a mut self) -> Box<dyn Iterator<Item = &'a mut Atom> + 'a> {
            match self {
                Atom::ChunkAtom(_)
                | Atom::SpaceBeforeAtom(_)
                | Atom::TextAtom(_)
                | Atom::WordDataAtom(_)
                | Atom::FalseAtom(_)
                | Atom::TrueAtom(_) => Box::new(std::iter::once(self)),
                Atom::AndAtom(x) => Box::new(x.atoms.iter_mut()),
                Atom::OrAtom(x) => Box::new(x.atoms.iter_mut()),
                Atom::NotAtom(x) => x.atom.iter_mut(),
                Atom::OffsetAtom(x) => x.atom.iter_mut(),
            }
        }

        pub(in crate::compile) fn mut_graph_ids(&mut self) -> Vec<&mut GraphId> {
            let mut ids = Vec::new();

            for atom in self.iter_mut() {
                let id = match atom {
                    Atom::ChunkAtom(atom) => atom.matcher.mut_graph_id(),
                    Atom::TextAtom(atom) => atom.matcher.matcher.mut_graph_id(),
                    Atom::WordDataAtom(atom) => atom
                        .matcher
                        .inflect_matcher
                        .as_mut()
                        .and_then(|x| x.matcher.mut_graph_id()),
                    _ => {
                        continue;
                    }
                };

                if let Some(id) = id {
                    ids.push(id);
                }
            }

            ids
        }
    }

    impl Matcher {
        pub(in crate::compile) fn new_regex(
            regex: Regex,
            negate: bool,
            empty_always_false: bool,
        ) -> Self {
            Matcher {
                matcher: MatcherKind::Regex(Box::new(regex)),
                negate,
                case_sensitive: true, // handled by regex, should maybe be an option
                empty_always_false,
                reference_conversion: None,
            }
        }

        pub(in crate::compile) fn new_string(
            string_or_idx: either::Either<String, GraphId>,
            negate: bool,
            case_sensitive: bool,
            empty_always_false: bool,
        ) -> Self {
            Matcher {
                matcher: match string_or_idx {
                    either::Left(string) => MatcherKind::Text(string.into_boxed_str()),
                    either::Right(id) => MatcherKind::Graph(id),
                },
                negate,
                case_sensitive,
                empty_always_false,
                reference_conversion: None,
            }
        }

        /// Applies LanguageTool's `case_conversion` to a reference matcher
        /// (`<match no="…"/>` inside a `<token>`).
        pub(in crate::compile) fn with_reference_conversion(
            mut self,
            conversion: crate::rule::grammar::Conversion,
        ) -> Self {
            self.reference_conversion = Some(conversion);
            self
        }

        pub(in crate::compile) fn with_reference_affix(
            mut self,
            prefix: &str,
            suffix: &str,
        ) -> Self {
            if let MatcherKind::Graph(id) = self.matcher {
                if !prefix.is_empty() || !suffix.is_empty() {
                    self.matcher = MatcherKind::AffixedGraph(Box::new(AffixedGraph {
                        id,
                        prefix: prefix.into(),
                        suffix: suffix.into(),
                    }));
                }
            }
            self
        }

        pub(in crate::compile) fn graph_id(&self) -> Option<GraphId> {
            match &self.matcher {
                MatcherKind::Graph(id) => Some(*id),
                MatcherKind::AffixedGraph(graph) => Some(graph.id),
                MatcherKind::Synthesis(reference, _) => Some(reference.id),
                _ => None,
            }
        }

        pub(in crate::compile) fn mut_graph_id(&mut self) -> Option<&mut GraphId> {
            match &mut self.matcher {
                MatcherKind::Graph(id) => Some(id),
                MatcherKind::AffixedGraph(graph) => Some(&mut graph.id),
                MatcherKind::Synthesis(reference, _) => Some(&mut reference.id),
                _ => None,
            }
        }

        pub(in crate::compile) fn with_synthesis(
            mut self,
            mut reference: crate::rule::grammar::Match,
            is_regex: bool,
        ) -> Self {
            if let Some(id) = self.graph_id() {
                reference.id = id;
                self.matcher = MatcherKind::Synthesis(Box::new(reference), is_regex);
            }
            self
        }
    }

    impl Quantifier {
        pub(in crate::compile) fn new(min: usize, max: usize) -> Self {
            assert!(max >= min);
            Quantifier {
                min: min as u32,
                max: max as u32,
            }
        }
    }

    impl AndAtom {
        pub(in crate::compile) fn and(atoms: Vec<Atom>) -> Atom {
            let mut atoms: Vec<_> = atoms
                .into_iter()
                .filter(|x| !matches!(x, Atom::TrueAtom { .. }))
                .collect();

            if atoms.is_empty() {
                (TrueAtom {}).into()
            } else if atoms.len() == 1 {
                atoms.remove(0)
            } else {
                (AndAtom { atoms }).into()
            }
        }
    }

    impl OrAtom {
        pub(in crate::compile) fn or(atoms: Vec<Atom>) -> Atom {
            let mut atoms: Vec<_> = atoms
                .into_iter()
                .filter(|x| !matches!(x, Atom::FalseAtom { .. }))
                .collect();

            if atoms.is_empty() {
                (FalseAtom {}).into()
            } else if atoms.len() == 1 {
                atoms.remove(0)
            } else {
                (OrAtom { atoms }).into()
            }
        }
    }

    impl NotAtom {
        pub(in crate::compile) fn not(atom: Atom) -> Atom {
            match atom {
                Atom::TrueAtom { .. } => FalseAtom::default().into(),
                Atom::FalseAtom { .. } => TrueAtom::default().into(),
                x => (NotAtom { atom: Box::new(x) }).into(),
            }
        }
    }

    impl OffsetAtom {
        pub(in crate::compile) fn new(atom: Atom, offset: isize) -> Self {
            OffsetAtom {
                atom: Box::new(atom),
                offset,
            }
        }
    }

    impl Composition {
        pub(in crate::compile) fn new(
            mut parts: Vec<Part>,
            pool: &mut PartInterner,
        ) -> Result<Self, Error> {
            // A required literal first part lets the matcher jump straight to the token
            // positions that can start a match (see `Composition::start_keys`).
            let start_keys = if parts[0].quantifier.min > 0 {
                let mut keys = Vec::new();
                if parts[0].atom.start_keys(&mut keys) && !keys.is_empty() {
                    Some(keys.into_boxed_slice())
                } else {
                    None
                }
            } else {
                None
            };

            // `GraphId(k) -> group index`; keys are the visible parts in order, so the vector is
            // indexed by `k` and holds `part index + 1` (see `GraphIndex`).
            let mut id_to_idx: Vec<u32> = vec![0];

            let required_key = {
                let skip_first = usize::from(start_keys.is_some());
                parts
                    .iter()
                    .skip(skip_first)
                    .filter(|part| part.quantifier.min > 0)
                    .filter_map(|part| {
                        let mut keys = Vec::new();
                        if part.atom.start_keys(&mut keys)
                            && keys.len() == 1
                            && !keys[0].text.is_empty()
                        {
                            keys.pop()
                        } else {
                            None
                        }
                    })
                    .max_by_key(|key| key.text.len())
                    .map(Box::new)
            };

            for (i, part) in parts.iter().enumerate() {
                if part.visible {
                    id_to_idx.push((i + 1) as u32);
                }
            }

            let optional_count = parts
                .iter()
                .filter(|part| part.visible && part.quantifier.min == 0)
                .count() as u32;

            for (i, part) in parts.iter_mut().enumerate() {
                for id in part.atom.mut_graph_ids() {
                    loop {
                        let index = id_to_idx
                            .get(id.0)
                            .copied()
                            .filter(|&index| index != u32::MAX)
                            .ok_or_else(|| {
                                Error::Unexpected(format!("id must exist in graph: {:?}", id))
                            })?;

                        // ideally this should throw an error but LT is more lenient than nlprule
                        if index > i as u32 {
                            *id = GraphId(id.0 - 1);
                        } else {
                            break;
                        }
                    }
                }
            }

            Ok(Composition {
                parts: parts.into_iter().map(|part| pool.intern(part)).collect(),
                id_to_idx,
                optional_count,
                start_keys,
                required_key,
            })
        }
    }
}

pub(in crate::compile) mod filters {
    use super::Error;
    use std::collections::HashMap;

    use crate::{filter::*, rule::engine::Engine, utils::regex::Regex};

    trait FromArgs: Sized {
        fn from_args(args: HashMap<String, String>, engine: &Engine) -> Result<Self, Error>;
    }

    impl FromArgs for NoDisambiguationEnglishPartialPosTagFilter {
        fn from_args(args: HashMap<String, String>, engine: &Engine) -> Result<Self, Error> {
            if args.contains_key("negate_postag") {
                panic!("negate_postag not supported in NoDisambiguationEnglishPartialPosTagFilter");
            }

            Ok(NoDisambiguationEnglishPartialPosTagFilter {
                id: engine.to_graph_id(args
                    .get("no")
                    .ok_or_else(|| {
                        Error::Unexpected(
                            "NoDisambiguationEnglishPartialPosTagFilter must have `no` argument"
                                .into(),
                        )
                    })?
                    .parse::<usize>()?)?,
                regexp: Regex::from_java_regex(
                    &args.get("regexp").ok_or_else(|| {
                        Error::Unexpected(
                        "NoDisambiguationEnglishPartialPosTagFilter must have `regexp` argument"
                            .into(),
                    )
                    })?,
                    true,
                    true,
                )?,
                postag_regexp: Regex::from_java_regex(
                    &args.get("postag_regexp").ok_or_else(|| {
                        Error::Unexpected(
                        "NoDisambiguationEnglishPartialPosTagFilter must have `postag_regexp` argument"
                            .into(),
                    )
                    })?,
                    true,
                    true,
                )?,
                negate_postag: args.get("negate_postag").map_or(false, |x| x == "yes"),
            })
        }
    }

    pub(in crate::compile) fn get_filter(
        name: &str,
        args: HashMap<String, String>,
        engine: &Engine,
    ) -> Result<Filter, Error> {
        match name {
            "NoDisambiguationEnglishPartialPosTagFilter" => {
                Ok(NoDisambiguationEnglishPartialPosTagFilter::from_args(args, engine)?.into())
            }
            _ => Err(Error::Unexpected(format!("unsupported filter {}", name))),
        }
    }
}

/// Build-time construction of the grammar-rule filters
/// ([`crate::rule::filter::RuleFilter`]).
///
/// Filters whose class needs resources the on-device engine does not have
/// (LanguageTool's multitoken-speller data) stay compile errors that name the
/// class, so a rule can never silently ignore its filter.
pub(in crate::compile) mod rule_filters {
    use std::{collections::HashMap, path::Path};

    use log::warn;

    use super::Error;
    use crate::rule::filter::{
        Adverb, ApostropheType, ArgValue, DateCheck, DateCheckKind, DateRange, FindSuggestions,
        FutureDate, NewYearDate, NumberInWord, RegexAntiPattern, RuleFilter,
        SuppressMisspelledSuggestions, UnderlineSpaces, WordFrom, YmdDateCheck,
    };
    use crate::types::DefaultHashMap;
    use crate::utils::regex::Regex;

    /// Splits LanguageTool's `args` attribute (`key:value` pairs, whitespace
    /// separated) into a map.
    fn split_args(args: &str) -> HashMap<String, String> {
        args.split_whitespace()
            .filter(|x| !x.is_empty())
            .map(|arg| {
                let idx = arg.find(':').unwrap_or_else(|| {
                    panic!("invalid filter argument (expected 'key:value'): {}", arg)
                });
                (arg[..idx].to_string(), arg[idx + 1..].to_string())
            })
            .collect()
    }

    /// Turns one argument value into a [`ArgValue`]. Token references (`\1`)
    /// only exist for `<pattern>` rules; `<regexp>` rules receive their
    /// arguments verbatim (LanguageTool's `RegexRuleFilterEvaluator`).
    fn to_arg_value(value: &str, is_regex_rule: bool) -> ArgValue {
        if !is_regex_rule {
            if let Some(rest) = value.strip_prefix('\\') {
                if let Ok(index) = rest.parse::<usize>() {
                    return ArgValue::Token(index);
                }
            }
        }
        ArgValue::Static(value.to_string())
    }

    fn required<'a>(args: &'a HashMap<String, String>, key: &str) -> Result<&'a str, Error> {
        args.get(key).map(String::as_str).ok_or_else(|| {
            Error::Unexpected(format!("filter is missing required argument '{}'", key))
        })
    }

    fn optional_usize(args: &HashMap<String, String>, key: &str) -> Result<Option<usize>, Error> {
        args.get(key)
            .map(|x| x.parse())
            .transpose()
            .map_err(Error::from)
    }

    /// A `wordFrom` argument: `marker` or a 1-based pattern token index.
    fn word_from(value: &str) -> Result<WordFrom, std::num::ParseIntError> {
        Ok(if value == "marker" {
            WordFrom::Marker
        } else {
            WordFrom::Index(value.parse()?)
        })
    }

    /// An optional regular expression argument, matched against the whole
    /// string like Java's `Matcher.matches`.
    fn optional_regex(
        args: &HashMap<String, String>,
        key: &str,
    ) -> Result<Option<Regex>, Box<Error>> {
        match args.get(key) {
            Some(pattern) => Ok(Some(Regex::from_java_regex(pattern, true, true)?)),
            None => Ok(None),
        }
    }

    /// Reads the LanguageTool adverb -> adjective table used by `AdverbFilter`
    /// (LGPL data, loaded from the build directory like the builtin lists).
    fn adverb_table(build_dir: &Path) -> DefaultHashMap<String, String> {
        let path = build_dir.join("builtin").join("adverb_adjective.txt");
        match fs_err::read_to_string(&path) {
            Ok(content) => content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .filter_map(|line| {
                    let mut parts = line.split('\t');
                    let adverb = parts.next()?.trim();
                    let adjective = parts.next()?.trim();
                    Some((adverb.to_string(), adjective.to_string()))
                })
                .collect(),
            Err(_) => {
                warn!(
                    "Builtin data file {} does not exist. AdverbFilter will use an empty table.",
                    path.display()
                );
                DefaultHashMap::default()
            }
        }
    }

    /// Builds the filter for a rule's `<filter>` element.
    ///
    /// `class` is the fully qualified LanguageTool class name; `args` is the raw
    /// `args` attribute. Unknown classes and classes whose resources are not
    /// available are reported as errors that name the class.
    pub(in crate::compile) fn get_rule_filter(
        class: &str,
        args: &str,
        is_regex_rule: bool,
        build_dir: &Path,
    ) -> Result<RuleFilter, Error> {
        let name = class.rsplit('.').next().unwrap_or(class);
        let args = split_args(args);

        Ok(match name {
            "UnderlineSpacesFilter" => {
                let spaces = required(&args, "underlineSpaces")?;
                RuleFilter::UnderlineSpaces(UnderlineSpaces {
                    before: spaces == "before" || spaces == "both",
                    after: spaces == "after" || spaces == "both",
                })
            }
            "OrdinalSuffixFilter" => RuleFilter::OrdinalSuffix,
            "ApostropheTypeFilter" => RuleFilter::ApostropheType(ApostropheType {
                word_from: word_from(required(&args, "wordFrom")?)?,
                has_typographical_apostrophe: required(&args, "hasTypographicalApostrophe")?
                    .eq_ignore_ascii_case("true"),
            }),
            "RegexAntiPatternFilter" => {
                let antipatterns = required(&args, "antipatterns")?;
                let regexes = antipatterns
                    .split('|')
                    .map(|pattern| Regex::from_java_regex(pattern, false, true))
                    .collect::<Result<Vec<_>, _>>()?;
                RuleFilter::RegexAntiPattern(RegexAntiPattern { regexes })
            }
            "DateRangeChecker" => RuleFilter::DateRangeChecker(DateRange {
                x: to_arg_value(required(&args, "x")?, is_regex_rule),
                y: to_arg_value(required(&args, "y")?, is_regex_rule),
            }),
            "AdverbFilter" => RuleFilter::Adverb(Adverb {
                adverb_to_adjective: adverb_table(build_dir),
                adverb: to_arg_value(required(&args, "adverb")?, is_regex_rule),
                noun: to_arg_value(required(&args, "noun")?, is_regex_rule),
            }),
            "FutureDateFilter" => RuleFilter::FutureDate(FutureDate {
                year: to_arg_value(required(&args, "year")?, is_regex_rule),
                month: to_arg_value(required(&args, "month")?, is_regex_rule),
                day: to_arg_value(required(&args, "day")?, is_regex_rule),
            }),
            "DateCheckFilter" => {
                let kind = if args.get("weekDay").map_or(false, |x| x.starts_with('\\')) {
                    // LanguageTool 5.2: the arguments are the matched token texts.
                    DateCheckKind::Text {
                        week_day: to_arg_value(required(&args, "weekDay")?, is_regex_rule),
                        day: to_arg_value(required(&args, "day")?, is_regex_rule),
                        month: to_arg_value(required(&args, "month")?, is_regex_rule),
                        year: args.get("year").map(|v| to_arg_value(v, is_regex_rule)),
                    }
                } else {
                    // LanguageTool 6.6: the arguments are pattern-token indices.
                    DateCheckKind::Index {
                        week_day: required(&args, "weekDay")?.parse()?,
                        day: optional_usize(&args, "day")?,
                        month: optional_usize(&args, "month")?,
                        year: optional_usize(&args, "year")?,
                        date: optional_usize(&args, "date")?,
                    }
                };
                RuleFilter::DateCheck(DateCheck { kind })
            }
            "YMDDateCheckFilter" => RuleFilter::YmdDateCheck(YmdDateCheck {
                date: to_arg_value(required(&args, "date")?, is_regex_rule),
                week_day: to_arg_value(required(&args, "weekDay")?, is_regex_rule),
            }),
            "NewYearDateFilter" => RuleFilter::NewYearDate(NewYearDate {
                year: to_arg_value(required(&args, "year")?, is_regex_rule),
                month: to_arg_value(required(&args, "month")?, is_regex_rule),
                day: to_arg_value(required(&args, "day")?, is_regex_rule),
                date: None,
            }),
            "YMDNewYearDateFilter" => RuleFilter::NewYearDate(NewYearDate {
                year: ArgValue::Static(String::new()),
                month: ArgValue::Static(String::new()),
                day: ArgValue::Static(String::new()),
                date: Some(to_arg_value(required(&args, "date")?, is_regex_rule)),
            }),
            "MultitokenSpellerFilter" => {
                if !build_dir.join("spell").join("multitoken.txt").exists() {
                    return Err(Error::Unimplemented(format!(
                        "filter class '{}' needs the multitoken spelling list \
                         (spell/multitoken.txt), which this build directory does not have.",
                        class
                    )));
                }
                RuleFilter::MultitokenSpeller
            }
            "FindSuggestionsFilter" => {
                let from = required(&args, "wordFrom")?;
                if from == "inmarker" || args.contains_key("Mode") {
                    return Err(Error::Unimplemented(format!(
                        "filter class '{}' does not support 'wordFrom:inmarker' or 'Mode' yet.",
                        class
                    )));
                }
                RuleFilter::FindSuggestions(FindSuggestions {
                    word_from: word_from(from)?,
                    desired_postag: Regex::from_java_regex(
                        required(&args, "desiredPostag")?,
                        true,
                        true,
                    )?,
                    priority_postag: optional_regex(&args, "priorityPostag").map_err(|e| *e)?,
                    remove_suggestions: optional_regex(&args, "removeSuggestionsRegexp")
                        .map_err(|e| *e)?,
                    suppress_match: args
                        .get("suppressMatch")
                        .is_some_and(|x| x.eq_ignore_ascii_case("true")),
                })
            }
            "EnglishSuppressMisspelledSuggestionsFilter" => {
                RuleFilter::SuppressMisspelledSuggestions(SuppressMisspelledSuggestions {
                    suppress_match: !required(&args, "suppressMatch")?
                        .eq_ignore_ascii_case("false"),
                    suppress_postag: optional_regex(&args, "SuppressPostag").map_err(|e| *e)?,
                })
            }
            "EnglishNumberInWordFilter" => RuleFilter::NumberInWord(NumberInWord {
                word: to_arg_value(required(&args, "word")?, is_regex_rule),
            }),
            _ => {
                return Err(Error::Unexpected(format!(
                    "unsupported filter class '{}'",
                    class
                )))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::parse_structure::RegexCache;
    use crate::rule::engine::composition::{MatchSentence, PartPool};

    const MARKER_ANTIPATTERNS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="en">
<category id="PROBE" name="Probe">
  <rule id="M_TRAIL" name="m">
    <antipattern><token>aaa</token><marker><token skip="-1">bbb</token></marker><token>ddd</token></antipattern>
    <pattern><token>ccc</token></pattern>
    <message>m</message>
  </rule>
  <rule id="M_LEAD" name="m">
    <antipattern><token skip="-1">bbb</token><marker><token>ddd</token></marker></antipattern>
    <pattern><token>ccc</token></pattern>
    <message>m</message>
  </rule>
  <rule id="M_INNER" name="m">
    <antipattern><marker><token skip="-1">bbb</token><token>ddd</token></marker></antipattern>
    <pattern><token>ccc</token></pattern>
    <message>m</message>
  </rule>
  <rule id="M_AFTER" name="m">
    <antipattern><marker><token>bbb</token></marker><token>ccc</token></antipattern>
    <pattern><token>ccc</token></pattern>
    <message>m</message>
  </rule>
  <rule id="M_TWO" name="m">
    <antipattern><token skip="-1">www</token><marker><token skip="-1">iii</token></marker><token>qqq</token></antipattern>
    <pattern><token>iii</token></pattern>
    <message>m</message>
  </rule>
</category>
</rules>
"#;

    fn assert_flag_starts(name: &str, xml: &str, cases: Vec<(&str, Vec<(&str, usize)>)>) {
        let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
        let path = std::env::temp_dir().join(format!("wmb-{}-{}.xml", name, std::process::id()));
        std::fs::write(&path, xml).unwrap();
        let mut info = BuildInfo::new(
            tokenizer.tagger().clone(),
            RegexCache::new(0),
            PathBuf::new(),
        );
        let mut pool = PartInterner::default();
        let rules = rules_from_xml(
            &path,
            &mut info,
            &RulesLangOptions::default(),
            &mut pool,
            &mut HashMap::new(),
        );
        std::fs::remove_file(&path).unwrap();
        let pool = pool.into_pool();
        assert_eq!(rules.len(), xml.matches("<rule ").count());

        for (text, expected) in cases {
            let mut flagged = Vec::new();
            for sentence in tokenizer.pipe(text) {
                let sentence = MatchSentence::new(&sentence);
                for rule in &rules {
                    let id = rule.id.to_string();
                    let name = id.split('/').nth(1).unwrap().to_string();
                    flagged.extend(
                        rule.apply(&pool, &sentence, None, None, None)
                            .map(|suggestion| (name.clone(), suggestion.span().char().start)),
                    );
                }
            }
            flagged.sort();
            let expected: Vec<_> = expected
                .into_iter()
                .map(|(name, start)| (name.to_string(), start))
                .collect();
            assert_eq!(flagged, expected, "{:?}", text);
        }
    }

    const DISAMBIGUATION_ANTIPATTERNS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="en">
  <rule id="D_SKIP" name="d">
    <antipattern><token>bbb</token></antipattern>
    <pattern><marker><token skip="-1">aaa</token></marker><token regexp="yes">bbb|ccc</token></pattern>
    <disambig action="replace"><wd lemma="zz" pos="ZZ"/></disambig>
  </rule>
  <rule id="D_GAP" name="d">
    <antipattern><token>ggg</token></antipattern>
    <pattern><marker><token skip="-1">fff</token></marker><token>hhh</token></pattern>
    <disambig action="replace"><wd lemma="zz" pos="ZZ"/></disambig>
  </rule>
  <rule id="D_TOUCH" name="d">
    <antipattern><token>“</token></antipattern>
    <antipattern><token>—</token></antipattern>
    <antipattern><token>…</token></antipattern>
    <pattern><token>ddd</token></pattern>
    <disambig action="replace"><wd lemma="zz" pos="ZZ"/></disambig>
  </rule>
</rules>
"#;

    fn disambiguation_rules(xml: &str) -> (Tokenizer, Vec<(String, DisambiguationRule)>, PartPool) {
        let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
        let path = std::env::temp_dir().join(format!(
            "wmb-disambiguation-{}-{}.xml",
            std::process::id(),
            xml.len()
        ));
        std::fs::write(&path, xml).unwrap();
        let mut info = BuildInfo::new(
            tokenizer.tagger().clone(),
            RegexCache::new(0),
            PathBuf::new(),
        );
        let mut pool = PartInterner::default();
        let rules = crate::compile::parse_structure::read_disambiguation_rules(&path)
            .into_iter()
            .map(|result| {
                let (rule, group, _) = result.unwrap();
                let id = match group {
                    Some(group) => format!("{}/{}", group.id, group.n),
                    None => rule.id.clone().unwrap(),
                };
                let rule =
                    DisambiguationRule::from_rule_structure(rule, &mut info, &mut pool).unwrap();
                (id, rule)
            })
            .collect();
        std::fs::remove_file(&path).unwrap();
        (tokenizer, rules, pool.into_pool())
    }

    fn disambiguated(
        tokenizer: &Tokenizer,
        rules: &[(String, DisambiguationRule)],
        pool: &PartPool,
        text: &str,
    ) -> Vec<String> {
        let mut fired = Vec::new();
        for sentence in tokenizer.pipe(text) {
            let sentence = MatchSentence::new(&sentence);
            for (id, rule) in rules {
                if !rule.apply(pool, &sentence).is_empty() {
                    fired.push(id.clone());
                }
            }
        }
        fired
    }

    #[test]
    fn disambiguation_antipatterns_block_the_whole_match_span_like_languagetool() {
        let (tokenizer, rules, pool) = disambiguation_rules(DISAMBIGUATION_ANTIPATTERNS);
        for (text, expected) in [
            ("Then aaa bbb ccc.", vec![]),
            ("Then aaa eee ccc.", vec!["D_SKIP"]),
            ("Then fff ggg hhh.", vec![]),
            ("Then fff eee hhh.", vec!["D_GAP"]),
            ("\u{201c}ddd,\u{201d} she said.", vec![]),
            ("Then eee\u{2014}ddd.", vec![]),
            ("Then ddd\u{2026} eee.", vec![]),
            ("\u{201c}Then eee ddd,\u{201d} she said.", vec!["D_TOUCH"]),
            ("Then eee \u{2014} ddd.", vec!["D_TOUCH"]),
            ("She left.\u{2028}ddd eee.", vec!["D_TOUCH"]),
            ("Then eee\u{2019}s ddd.", vec!["D_TOUCH"]),
        ] {
            assert_eq!(
                disambiguated(&tokenizer, &rules, &pool, text),
                expected,
                "{:?}",
                text
            );
        }
    }

    const DISAMBIGUATION_ANTIPATTERN_CASE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="en">
  <rulegroup id="D_CASE_GROUP" name="d">
    <antipattern case_sensitive="yes"><token>Kkk</token><token>Lll</token></antipattern>
    <rule>
      <pattern><token>kkk</token></pattern>
      <disambig action="replace"><wd lemma="zz" pos="ZZ"/></disambig>
    </rule>
    <rule>
      <pattern><token>mmm</token></pattern>
      <disambig action="replace"><wd lemma="zz" pos="ZZ"/></disambig>
    </rule>
  </rulegroup>
  <rule id="D_CASE" name="d">
    <antipattern case_sensitive="yes"><token>Nnn</token></antipattern>
    <pattern><token>ppp</token></pattern>
    <disambig action="replace"><wd lemma="zz" pos="ZZ"/></disambig>
  </rule>
  <rule id="D_CASE_RESET" name="d">
    <antipattern case_sensitive="yes"><token>Nnn</token></antipattern>
    <antipattern><token>qqq</token></antipattern>
    <pattern><token>sss</token></pattern>
    <disambig action="replace"><wd lemma="zz" pos="ZZ"/></disambig>
  </rule>
  <rulegroup id="D_CASE_OWN" name="d">
    <antipattern case_sensitive="yes"><token>Ttt</token></antipattern>
    <rule>
      <antipattern><token>uuu</token></antipattern>
      <pattern><token>vvv</token></pattern>
      <disambig action="replace"><wd lemma="zz" pos="ZZ"/></disambig>
    </rule>
    <rule>
      <antipattern case_sensitive="yes"><token>Xxx</token></antipattern>
      <pattern><token>yyy</token></pattern>
      <disambig action="replace"><wd lemma="zz" pos="ZZ"/></disambig>
    </rule>
  </rulegroup>
</rules>
"#;

    #[test]
    fn case_sensitive_disambiguation_antipattern_makes_the_pattern_case_sensitive() {
        let (tokenizer, rules, pool) = disambiguation_rules(DISAMBIGUATION_ANTIPATTERN_CASE);
        for (text, expected) in [
            (
                "Then kkk mmm ppp sss.",
                vec!["D_CASE_GROUP/0", "D_CASE_GROUP/1", "D_CASE", "D_CASE_RESET"],
            ),
            (
                "Then Kkk Mmm Ppp Sss.",
                vec!["D_CASE_GROUP/1", "D_CASE_RESET"],
            ),
            (
                "THEN KKK MMM PPP SSS.",
                vec!["D_CASE_GROUP/1", "D_CASE_RESET"],
            ),
            (
                "\u{201c}Kkk\u{2014}mmm\u{2026}\u{201d} ppp\u{2019}s Sss.",
                vec!["D_CASE_GROUP/1", "D_CASE", "D_CASE_RESET"],
            ),
            ("She left.\u{2028}Ppp sss.", vec!["D_CASE_RESET"]),
            ("Then vvv yyy.", vec!["D_CASE_OWN/0", "D_CASE_OWN/1"]),
            ("Then Vvv Yyy.", vec!["D_CASE_OWN/0"]),
            ("THEN VVV YYY.", vec!["D_CASE_OWN/0"]),
            (
                "\u{201c}Vvv\u{2014}yyy\u{2026}\u{201d} she said.",
                vec!["D_CASE_OWN/0", "D_CASE_OWN/1"],
            ),
        ] {
            assert_eq!(
                disambiguated(&tokenizer, &rules, &pool, text),
                expected,
                "{:?}",
                text
            );
        }
    }

    #[test]
    fn antipattern_marker_limits_the_immunized_tokens() {
        assert_flag_starts(
            "antipattern-marker",
            MARKER_ANTIPATTERNS,
            vec![
                (
                    "\u{201c}bbb ccc\u{2026}\u{201d} she said.",
                    vec![
                        ("M_AFTER", 5),
                        ("M_INNER", 5),
                        ("M_LEAD", 5),
                        ("M_TRAIL", 5),
                    ],
                ),
                (
                    "\u{201c}www iii\u{2014}iii qqq,\u{201d} he said.",
                    vec![("M_TWO", 9)],
                ),
                (
                    "She left.\u{2028}aaa bbb ccc ddd.",
                    vec![("M_AFTER", 18), ("M_LEAD", 18), ("M_TRAIL", 18)],
                ),
                (
                    "\u{201c}aaa bbb ccc ddd\u{2019}\u{201d}",
                    vec![("M_AFTER", 9), ("M_LEAD", 9), ("M_TRAIL", 9)],
                ),
            ],
        );
    }

    const OPTIONAL_TOKENS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="en">
<category id="PROBE" name="Probe">
  <rule id="LAZY" name="m">
    <pattern><token>we</token><token min="0" regexp="yes">really|truly</token><token regexp="yes">truly|left</token><token>early</token></pattern>
    <message>m</message>
  </rule>
  <rule id="COMPOUND" name="m">
    <pattern><token>and</token><token min="0" postag="RB"/><token postag="NNPS?|PRP_S.*" postag_regexp="yes"/><token inflected="yes" min="0" regexp="yes">be|do|have</token><token min="0" postag="RB|TO" postag_regexp="yes"/><token min="0" postag="VB[DPZ]?" postag_regexp="yes"><exception regexp="yes">are|be|do|have</exception></token><token min="0" postag="RB"/><token postag="SENT_END"/></pattern>
    <message>m</message>
  </rule>
  <rule id="SKIPPED" name="m">
    <pattern><token skip="-1">was<exception scope="next">did</exception></token><token min="0" skip="-1">not<exception scope="next">so</exception></token><token>it</token></pattern>
    <message>m</message>
  </rule>
</category>
</rules>
"#;

    #[test]
    fn optional_token_is_absent_when_the_next_token_matches_too() {
        assert_flag_starts(
            "optional-tokens",
            OPTIONAL_TOKENS,
            vec![
                ("We left and it has to stop.", vec![]),
                (
                    "\u{201c}We left and it has to stop,\u{201d} she said.",
                    vec![],
                ),
                ("We left and it is to stop.", vec![]),
                ("We left \u{2014} and it has not.", vec![("COMPOUND", 10)]),
                ("We left\u{2026} and it stops.", vec![("COMPOUND", 9)]),
                ("We truly left early.", vec![]),
                (
                    "We\u{2019}d left.\u{2028}We truly left early\u{2026}",
                    vec![],
                ),
                (
                    "\u{201c}We really left early,\u{201d} she said.",
                    vec![("LAZY", 1)],
                ),
                ("We truly early \u{2014} or so.", vec![("LAZY", 0)]),
            ],
        );
    }

    const NEXT_EXCEPTIONS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="en">
<category id="PROBE" name="Probe">
  <rule id="AFTER_SKIP" name="m">
    <pattern><token skip="2">it</token><token min="0">really</token><token>was<exception postag="VBN" scope="next"/></token></pattern>
    <message>m</message>
  </rule>
  <rule id="AFTER_OPTIONAL_SKIP" name="m">
    <pattern><token>it</token><token min="0" skip="1">really</token><token>was<exception postag="VBN" scope="next"/></token></pattern>
    <message>m</message>
  </rule>
</category>
</rules>
"#;

    #[test]
    fn scope_next_exception_depends_on_the_last_matched_skip() {
        assert_flag_starts(
            "next-exceptions",
            NEXT_EXCEPTIONS,
            vec![
                ("It was eaten today.", vec![("AFTER_SKIP", 0)]),
                ("It was eaten.\u{2028}Fine.", vec![("AFTER_SKIP", 0)]),
                ("It\u{2019}s\u{2026} it was eaten.", vec![("AFTER_SKIP", 6)]),
                ("It then was eaten.", vec![("AFTER_SKIP", 0)]),
                (
                    "\u{201c}It really was eaten today\u{2026}\u{201d}",
                    vec![("AFTER_OPTIONAL_SKIP", 1)],
                ),
                ("It really was eaten.", vec![("AFTER_OPTIONAL_SKIP", 0)]),
                (
                    "It really then was eaten.",
                    vec![("AFTER_OPTIONAL_SKIP", 0)],
                ),
                ("It\u{2014} really was eaten.", vec![]),
            ],
        );
    }

    #[test]
    fn absent_optional_token_uses_the_previous_skip_window() {
        assert_flag_starts(
            "optional-skip",
            OPTIONAL_TOKENS,
            vec![
                ("It was then did it go\u{2026}", vec![]),
                ("\u{201c}It was then so it went.\u{201d}", vec![]),
                ("It was then it went.", vec![("SKIPPED", 3)]),
                ("It was not then it went \u{2014} oh.", vec![("SKIPPED", 3)]),
                ("It was so it went.\u{2028}Done.", vec![]),
                ("It was not so it went.", vec![]),
                ("It was not did it go.", vec![("SKIPPED", 3)]),
            ],
        );
    }

    const IMMUNIZED_AND_REPEATED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="en">
<category id="PROBE" name="Probe">
  <rule id="IMMUNE_EXCEPTION" name="m">
    <antipattern><token>was</token><token>then</token></antipattern>
    <pattern><token skip="4">it<exception scope="next">then</exception></token><token>was</token></pattern>
    <message>m</message>
  </rule>
  <rule id="MIN_TWO" name="m">
    <pattern><token>saw</token><token case_sensitive="yes" min="2" regexp="yes">[A-Z].*</token></pattern>
    <message>m</message>
  </rule>
  <rule id="MIN_TWO_NEXT" name="m">
    <pattern><token>saw</token><token case_sensitive="yes" min="2" regexp="yes">[A-Z].*</token><token>plant</token></pattern>
    <message>m</message>
  </rule>
  <rule id="MAX_ANY" name="m">
    <pattern><token>said</token><token max="-1">no</token><token>more</token></pattern>
    <message>m</message>
  </rule>
</category>
</rules>
"#;

    #[test]
    fn immunized_tokens_skip_next_exceptions_and_repeats_follow_min_and_max() {
        let many_noes = format!("She said {} more\u{2014}again.", vec!["no"; 25].join(" "));
        assert_flag_starts(
            "immunized-repeated",
            IMMUNIZED_AND_REPEATED,
            vec![
                ("It soon was then was gone.", vec![("IMMUNE_EXCEPTION", 0)]),
                (
                    "\u{201c}It soon was then was gone\u{2026}\u{201d}",
                    vec![("IMMUNE_EXCEPTION", 1)],
                ),
                (
                    "It soon was\u{2014}then was gone.",
                    vec![("IMMUNE_EXCEPTION", 0)],
                ),
                ("It soon then was gone.", vec![]),
                (
                    "It\u{2019}s odd.\u{2028}It soon was then was gone.",
                    vec![("IMMUNE_EXCEPTION", 10)],
                ),
                (
                    "\u{201c}We saw Nine Mile Nuclear Power plant\u{2026}\u{201d}",
                    vec![("MIN_TWO", 4)],
                ),
                (
                    "We saw Nine Mile plant\u{2014}today.",
                    vec![("MIN_TWO", 3), ("MIN_TWO_NEXT", 3)],
                ),
                (
                    "We\u{2019}d saw Nine Mile Nuclear today.",
                    vec![("MIN_TWO", 5)],
                ),
                (&many_noes, vec![("MAX_ANY", 4)]),
                (
                    "\u{201c}She said no no more,\u{201d} he said.\u{2028}Fine.",
                    vec![("MAX_ANY", 5)],
                ),
            ],
        );
    }

    const VETOING_FILTER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="en">
<category id="PROBE" name="Probe">
  <rule id="PAIR" name="m">
    <pattern><token regexp="yes">[a-z]{3}</token><token regexp="yes">[a-z]{3}</token></pattern>
    <filter class="org.languagetool.rules.patterns.RegexAntiPatternFilter" args="antipatterns:aaa"/>
    <message>m</message>
  </rule>
</category>
</rules>
"#;

    #[test]
    fn a_vetoed_match_does_not_hide_a_later_overlapping_match() {
        let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
        let path =
            std::env::temp_dir().join(format!("wmb-vetoing-filter-{}.xml", std::process::id()));
        std::fs::write(&path, VETOING_FILTER).unwrap();
        let mut info = BuildInfo::new(
            tokenizer.tagger().clone(),
            RegexCache::new(0),
            PathBuf::new(),
        );
        let mut pool = PartInterner::default();
        let rules = rules_from_xml(
            &path,
            &mut info,
            &RulesLangOptions::default(),
            &mut pool,
            &mut HashMap::new(),
        );
        std::fs::remove_file(&path).unwrap();
        let pool = pool.into_pool();
        assert_eq!(rules.len(), 1);

        for (text, expected) in [
            ("\u{201c}aaa bbb ccc\u{2026}\u{201d} she said.", vec![5..12]),
            ("aaa bbb\u{2014}ccc ddd eee.", vec![8..15]),
            ("She left.\u{2028}aaa bbb ccc ddd.", vec![14..21]),
            ("\u{2018}ccc ddd aaa bbb\u{2019}", vec![1..8]),
        ] {
            let mut flagged = Vec::new();
            for sentence in tokenizer.pipe(text) {
                let sentence = MatchSentence::new(&sentence);
                flagged.extend(
                    rules[0]
                        .apply(&pool, &sentence, None, None, None)
                        .map(|suggestion| suggestion.span().char().clone()),
                );
            }
            assert_eq!(flagged, expected, "{:?}", text);
        }
    }

    const SUGGESTION_CASE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="en">
<category id="PROBE" name="Probe">
  <rule id="LIT" name="m">
    <pattern><token>zork</token><token>blat</token></pattern>
    <message>m <suggestion>quux frob</suggestion>.</message>
  </rule>
  <rule id="REF" name="m">
    <pattern><token>zirk</token><token>blat</token></pattern>
    <message>m <suggestion>\1 frob</suggestion>.</message>
  </rule>
  <rule id="PUNCT" name="m">
    <pattern><token>zark</token><token>blat</token></pattern>
    <message>m <suggestion>(the) frob</suggestion> <suggestion>‘tis frob</suggestion> <suggestion>—…</suggestion> <suggestion>ßa</suggestion> <suggestion>ß</suggestion> <suggestion>"½ frob</suggestion>.</message>
  </rule>
  <rule id="CONV" name="m">
    <pattern><token>zerk</token><token>blat</token></pattern>
    <message>m <suggestion><match no="1" case_conversion="alllower"/> frob</suggestion>.</message>
  </rule>
  <rule id="LATE_CONV" name="m">
    <pattern><token>qerk</token><token>blat</token></pattern>
    <message>m <suggestion>\2 <match no="1" case_conversion="startlower"/></suggestion>.</message>
  </rule>
  <rule id="SECOND_CONV" name="m">
    <pattern><token>zwerk</token><token>blat</token></pattern>
    <message>m <suggestion>quux</suggestion> <suggestion><match no="1" case_conversion="alllower"/> frob</suggestion>.</message>
  </rule>
  <rule id="START_CONV" name="m">
    <pattern><token postag="SENT_START"/><token>zurk</token><token>blat</token></pattern>
    <message>m <suggestion><match no="2" case_conversion="alllower"/> frob</suggestion>.</message>
  </rule>
  <rule id="START_LIT" name="m">
    <pattern><token postag="SENT_START"/><token>zyrk</token><token>blat</token></pattern>
    <message>m <suggestion>quux frob</suggestion>.</message>
  </rule>
  <rule id="REGEX" name="m">
    <regexp>zork blot</regexp>
    <message>m <suggestion>quux frob</suggestion>.</message>
  </rule>
  <rule id="REGEX_CONV" name="m">
    <regexp>(Zeek) (blot)</regexp>
    <message>m <suggestion>\2 \1</suggestion> <suggestion><match no="1" case_conversion="alllower"/></suggestion>.</message>
  </rule>
</category>
</rules>
"#;

    #[test]
    fn suggestion_case_follows_the_matched_text() {
        // Every expectation was checked against LanguageTool 6.6 with these rules as
        // a `--rulefile` (#158, #191).
        let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
        let path =
            std::env::temp_dir().join(format!("wmb-suggestion-case-{}.xml", std::process::id()));
        std::fs::write(&path, SUGGESTION_CASE).unwrap();
        let mut info = BuildInfo::new(
            tokenizer.tagger().clone(),
            RegexCache::new(0),
            PathBuf::new(),
        );
        let mut pool = PartInterner::default();
        let rules = rules_from_xml(
            &path,
            &mut info,
            &RulesLangOptions::default(),
            &mut pool,
            &mut HashMap::new(),
        );
        std::fs::remove_file(&path).unwrap();
        let pool = pool.into_pool();
        assert_eq!(rules.len(), SUGGESTION_CASE.matches("<rule ").count());

        let quux = |first: &str| vec![format!("{} frob", first)];
        for (text, rule, expected) in [
            ("zork blat is here.", "LIT", quux("quux")),
            ("Zork blat is here.", "LIT", quux("Quux")),
            ("It is zork blat here.", "LIT", quux("quux")),
            (
                "\u{201c}Zork blat,\u{201d} she said\u{2026}",
                "LIT",
                quux("Quux"),
            ),
            ("Fine.\u{2028}zork blat\u{2014}here.", "LIT", quux("quux")),
            ("zirk blat is here.", "REF", quux("zirk")),
            ("Zirk blat is here.", "REF", quux("Zirk")),
            (
                "zark blat is here.",
                "PUNCT",
                vec!["(the) frob", "‘tis frob", "—…", "ßa", "ß", "\"½ frob"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            ),
            (
                "Zark blat is here.",
                "PUNCT",
                vec!["(The) frob", "‘Tis frob", "—…", "ßa", "SS", "\"½ Frob"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            ),
            ("Zerk blat is here.", "CONV", quux("zerk")),
            // A conversion anywhere in the suggestions counts once the first
            // suggestion starts with a match reference, and only then.
            (
                "Qerk blat is here.",
                "LATE_CONV",
                vec!["blat qerk".to_string()],
            ),
            (
                "Zwerk blat is here.",
                "SECOND_CONV",
                vec!["Quux".to_string(), "Zwerk frob".to_string()],
            ),
            (
                "It was zwerk blat.",
                "SECOND_CONV",
                vec!["quux".to_string(), "zwerk frob".to_string()],
            ),
            // From `SENT_START`, LanguageTool skips the case conversion check.
            ("Zurk blat is here.", "START_CONV", quux("Zurk")),
            ("zurk blat is here.", "START_CONV", quux("zurk")),
            ("zyrk blat is here.", "START_LIT", quux("quux")),
            ("Zyrk blat is here.", "START_LIT", quux("Quux")),
            // A regex rule uppercases only a match at the start of the sentence.
            ("zork blot is here.", "REGEX", quux("quux")),
            ("Zork blot is here.", "REGEX", quux("Quux")),
            ("It was Zork blot.", "REGEX", quux("quux")),
            (
                "\u{201c}Zork blot,\u{201d} she said.",
                "REGEX",
                quux("quux"),
            ),
            // `RegexPatternRule` never applies the case conversion check.
            (
                "Zeek blot is here.",
                "REGEX_CONV",
                vec!["Blot Zeek".to_string(), "Zeek".to_string()],
            ),
        ] {
            let mut flagged = Vec::new();
            for sentence in tokenizer.pipe(text) {
                let sentence = MatchSentence::new(&sentence);
                for compiled in &rules {
                    let id = compiled.id.to_string();
                    let name = id.split('/').nth(1).unwrap().to_string();
                    flagged.extend(
                        compiled
                            .apply(&pool, &sentence, None, None, None)
                            .map(|suggestion| (name.clone(), suggestion.replacements().to_vec())),
                    );
                }
            }
            assert_eq!(flagged, vec![(rule.to_string(), expected)], "{:?}", text);
        }
    }

    #[test]
    fn multitoken_phrases_keep_the_java_hash_map_order() {
        assert_eq!(java_hash_map_hash("hello"), 99162322 ^ (99162322 >> 16));
        assert_eq!(java_hash_map_capacity(12), 16);
        assert_eq!(java_hash_map_capacity(13), 32);

        let path = std::env::temp_dir().join(format!("wmb-multitoken-{}.txt", std::process::id()));
        std::fs::write(
            &path,
            "New York\nNoah Harari\nNotre Dame\nNorth Sea\nNova Scotia\nnoah harari\nNoah Harari\n\
             Harari\nZo\u{eb} Quill\nZoe-Quill\nMara\u{2014}Venn\nO\u{2019}Brien Hall\n",
        )
        .unwrap();
        let speller = MultitokenSpeller::from_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        let keys: Vec<&str> = speller.by_first_char[&'n']
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();
        assert_eq!(
            keys,
            [
                "noah harari",
                "nova scotia",
                "north sea",
                "notre dame",
                "new york"
            ]
        );
        assert_eq!(
            speller.by_first_char[&'n'][0].1,
            ["Noah Harari", "noah harari"]
        );
        assert_eq!(
            speller.by_first_char[&'z'],
            [(
                "zoe quill".to_owned(),
                vec!["Zo\u{eb} Quill".to_owned(), "Zoe-Quill".to_owned()]
            )]
        );
        assert_eq!(
            speller.by_key_without_spaces["o\u{2019}brienhall"],
            ["O\u{2019}Brien Hall"]
        );
        assert!(!speller.by_first_char.contains_key(&'h'));
        assert!(!speller.by_first_char.contains_key(&'m'));
    }

    #[test]
    fn multiword_casing_variants_follow_the_chunker() {
        let options = TokenizerLangOptions {
            multiword_first_capitalized: true,
            multiword_all_uppercase: true,
            ..TokenizerLangOptions::default()
        };
        let none = DefaultHashMap::default();
        assert_eq!(casing_variants("O'Brien", &none, &options), vec!["O'BRIEN"]);
        assert_eq!(casing_variants("O’Brien", &none, &options), vec!["O’BRIEN"]);
        assert_eq!(casing_variants("L'Oréal", &none, &options), vec!["L'ORÉAL"]);
        assert_eq!(
            casing_variants("in a while", &none, &options),
            vec!["IN A WHILE", "In a while"]
        );
        assert_eq!(casing_variants("iPad", &none, &options), vec!["IPad"]);
        assert_eq!(
            casing_variants("“élan”", &none, &options),
            vec!["“ÉLAN”", "“Élan”"]
        );
        assert!(casing_variants("U.S.", &none, &options).is_empty());

        let mut known = DefaultHashMap::default();
        known.insert("O'BRIEN".to_owned(), 0);
        assert!(casing_variants("O'Brien", &known, &options).is_empty());

        let off = TokenizerLangOptions::default();
        assert!(casing_variants("O'Brien", &none, &off).is_empty());
    }

    #[test]
    fn is_camel_case_needs_lowercase_then_two_more_chars() {
        assert!(is_camel_case("iPad"));
        assert!(!is_camel_case("iP"));
        assert!(!is_camel_case("O'Brien"));
    }

    fn model_data(keys: &[String], capacity: usize) -> ModelData {
        let mut pmap = DefaultHashMap::with_capacity_and_hasher(capacity, Default::default());
        for key in keys {
            let length = key.chars().count() % 3 + 1;
            pmap.insert(
                key.clone(),
                ContextData {
                    parameters: (0..length).map(|j| (key.len() + j) as f32).collect(),
                    outcomes: (0..length).map(|j| (j + key.len()) % 4).collect(),
                },
            );
        }
        ModelData {
            outcome_labels: vec!["B-NP".into(), "I-NP".into(), "B-VP".into(), "O".into()],
            pmap,
        }
    }

    #[test]
    fn spell_rejects_a_default_variant_outside_its_variants() {
        let lang_options: SpellLangOptions = serde_json::from_str(
            r#"{"variants": ["en_GB", "en_US"], "split_hyphens": true, "default_variant": "en_’NZ"}"#,
        )
        .unwrap();
        let error = Spell::from_dumps(
            "missing",
            "missing",
            "missing",
            "missing",
            lang_options,
            &Tokenizer::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("en_\u{2019}NZ"));
    }

    #[test]
    fn global_phrase_casing_variants_follow_the_multiword_chunker() {
        assert_eq!(
            Spell::casing_variants("McCarthy"),
            vec!["McCarthy", "MCCARTHY"]
        );
        assert_eq!(Spell::casing_variants("eBay"), vec!["eBay", "EBay"]);
        assert_eq!(
            Spell::casing_variants("iPhone4"),
            vec!["iPhone4", "IPHONE4", "IPhone4"]
        );
        assert_eq!(
            Spell::casing_variants("McDonald\u{2019}s"),
            vec!["McDonald\u{2019}s", "MCDONALD\u{2019}S"]
        );
        assert_eq!(
            Spell::casing_variants("\u{e9}clair"),
            vec!["\u{e9}clair", "\u{c9}CLAIR", "\u{c9}clair"]
        );
        assert_eq!(Spell::casing_variants("NASA"), vec!["NASA"]);
    }

    #[test]
    fn chunk_model_serializes_independently_of_pmap_layout() {
        let mut keys: Vec<String> = (0..500).map(|i| format!("w={}", i)).collect();
        keys.extend(
            ["\u{201C}", "\u{2019}s", "\u{2014}", "\u{2026}", "\u{2028}"]
                .iter()
                .map(|x| format!("w={}", x)),
        );
        let reversed: Vec<String> = keys.iter().rev().cloned().collect();

        let a = model_data(&keys, 0);
        let b = model_data(&reversed, 4096);
        assert_ne!(
            a.pmap.keys().collect::<Vec<_>>(),
            b.pmap.keys().collect::<Vec<_>>()
        );

        let a = chunk::Model::from(a);
        let b = chunk::Model::from(b);
        assert_eq!(
            bincode::serialize(&a).unwrap(),
            bincode::serialize(&b).unwrap()
        );
    }
}
