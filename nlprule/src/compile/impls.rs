use bimap::BiMap;
use fs_err::File;
use fst::Map;
use half::bf16;
use log::warn;
use serde::{Deserialize, Serialize};
use std::{
    cmp,
    collections::{BTreeSet, HashMap, HashSet},
    hash::{Hash, Hasher},
    io::{self, BufRead, BufReader},
    iter::once,
    path::Path,
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
        DisambiguationRule, Rule,
    },
    rules::{Rules, RulesLangOptions},
    spell::{Spell, SpellInt, SpellLangOptions},
    tokenizer::{
        chunk,
        multiword::{MultiwordTagger, MultiwordTaggerFields},
        tag::{Tagger, TaggerFields, TaggerLangOptions},
        Tokenizer, TokenizerLangOptions,
    },
    types::*,
    utils::{apply_to_first, parallelism::MaybeParallelIterator, regex::Regex},
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

    fn global_phrases(path: &Path, tokenizer: &Tokenizer) -> io::Result<Vec<u8>> {
        let mut phrases = BTreeSet::new();

        let reader = BufReader::new(File::open(path)?);
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let entry = line[..line.find('#').unwrap_or(line.len())].trim();
            if entry.is_empty() {
                continue;
            }

            for variant in Spell::casing_variants(entry) {
                let tokens = tokenizer.get_token_strs(&variant);
                if !tokens.is_empty() {
                    phrases.insert(tokens.join(" "));
                }
            }
        }

        Ok(fst::Set::from_iter(phrases)
            .expect("phrases are lexicographically sorted.")
            .into_fst()
            .to_vec())
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
                        let freq = freq.chars().next().expect("freq must have one char - would not have been yielded by split_whitespace otherwise.") as usize - 'A' as usize;

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

        let phrases = if global_phrase_path.as_ref().exists() {
            Spell::global_phrases(global_phrase_path.as_ref(), tokenizer)?
        } else {
            Vec::new()
        };

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

impl MultiwordTagger {
    pub(in crate::compile) fn from_dump<P: AsRef<Path>>(
        dump: P,
        info: &BuildInfo,
    ) -> Result<Self, io::Error> {
        let reader = BufReader::new(File::open(dump.as_ref())?);
        let mut multiwords = Vec::new();

        for line in reader.lines() {
            let line = line?;

            // strip comments
            let line = &line[..line.find('#').unwrap_or_else(|| line.len())].trim();
            if line.is_empty() {
                continue;
            }
            let tab_split: Vec<_> = line.split('\t').collect();

            let word: String = tab_split[0]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let pos = info.tagger().id_tag(tab_split[1]).to_owned_id();
            multiwords.push((word, pos));
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

impl Rules {
    pub(in crate::compile) fn from_xml<P: AsRef<Path>>(
        path: P,
        build_info: &mut BuildInfo,
        spell: Spell,
        options: RulesLangOptions,
        builtins: Vec<BuiltinRule>,
    ) -> Self {
        let rules = super::parse_structure::read_rules(path);
        let mut errors: HashMap<String, usize> = HashMap::new();
        let mut pool = PartInterner::default();

        let rules: Vec<_> = rules
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

                    // LanguageTool's repetition experiment (`min_prev_matches` /
                    // `distance_tokens`) needs cross-match state, so reject it
                    // explicitly instead of compiling a rule that ignores the
                    // constraint.
                    if rule_structure.min_prev_matches.is_some()
                        || rule_structure.distance_tokens.is_some()
                        || group.as_ref().map_or(false, |g| {
                            g.min_prev_matches.is_some() || g.distance_tokens.is_some()
                        })
                    {
                        *errors
                            .entry(
                                "[Rule] feature not implemented: repetition attributes \
                                 (min_prev_matches / distance_tokens) are not implemented."
                                    .into(),
                            )
                            .or_insert(0) += 1;
                        return None;
                    }

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

                    match Rule::from_rule_structure(rule_structure, build_info, &mut pool) {
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
        let mut parameters: Vec<bf16> = Vec::new();

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
            parameters.extend(value.parameters.into_iter().map(bf16::from_f32));

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
            AndAtom, Atom, Composition, FalseAtom, GraphId, NotAtom, OffsetAtom, OrAtom, Part,
            PartInterner, Quantifier, TrueAtom,
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

        pub(in crate::compile) fn graph_id(&self) -> Option<GraphId> {
            match &self.matcher {
                MatcherKind::Graph(id) => Some(*id),
                MatcherKind::Synthesis(reference, _) => Some(reference.id),
                _ => None,
            }
        }

        pub(in crate::compile) fn mut_graph_id(&mut self) -> Option<&mut GraphId> {
            match &mut self.matcher {
                MatcherKind::Graph(id) => Some(id),
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

            let can_stop_mask = (0..parts.len())
                .map(|i| parts[i..].iter().all(|x| x.quantifier.min == 0))
                .collect();

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
                can_stop_mask,
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
/// (LanguageTool's multitoken-speller data) or that query the speller and are
/// not ported yet stay compile errors that name the class, so a rule can never
/// silently ignore its filter.
pub(in crate::compile) mod rule_filters {
    use std::{collections::HashMap, path::Path};

    use log::warn;

    use super::Error;
    use crate::rule::filter::{
        Adverb, ApostropheType, ArgValue, DateCheck, DateCheckKind, DateRange, FutureDate,
        NewYearDate, RegexAntiPattern, RuleFilter, UnderlineSpaces, WordFrom, YmdDateCheck,
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
            "ApostropheTypeFilter" => {
                let word_from = required(&args, "wordFrom")?;
                let word_from = if word_from == "marker" {
                    WordFrom::Marker
                } else {
                    WordFrom::Index(word_from.parse()?)
                };
                RuleFilter::ApostropheType(ApostropheType {
                    word_from,
                    has_typographical_apostrophe: required(&args, "hasTypographicalApostrophe")?
                        .eq_ignore_ascii_case("true"),
                })
            }
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
                return Err(Error::Unimplemented(format!(
                    "filter class '{}' needs LanguageTool's multitoken-speller data, \
                     which is not available to the on-device engine.",
                    class
                )))
            }
            "EnglishSuppressMisspelledSuggestionsFilter"
            | "FindSuggestionsFilter"
            | "EnglishNumberInWordFilter" => {
                return Err(Error::Unimplemented(format!(
                    "filter class '{}' needs the speller at match time and is not ported yet.",
                    class
                )))
            }
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
