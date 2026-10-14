use std::{ops::Range, path::PathBuf, sync::Arc};

use super::{structure, Error};
use crate::{tokenizer::tag::Tagger, types::*};
use crate::{utils, utils::regex::Regex};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

pub use structure::{read_disambiguation_rules, read_rules};

use crate::rule::disambiguation::*;
use crate::rule::engine::composition::concrete::*;
use crate::rule::engine::composition::*;
use crate::rule::engine::*;
use crate::rule::grammar::*;
use crate::rule::{id::Index, DisambiguationRule, Rule, Unification};

// `max="-1"`: LanguageTool sets no limit
#[inline]
fn max_matches() -> usize {
    u32::MAX as usize
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct RegexCache {
    #[serde(serialize_with = "serialize_cache")]
    cache: DefaultHashMap<u64, Option<DefaultHashSet<WordIdInt>>>,
    // this is compared with the hash of the word store of the tagger
    word_hash: u64,
}

fn serialize_cache<S>(
    cache: &DefaultHashMap<u64, Option<DefaultHashSet<WordIdInt>>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut entries: Vec<(u64, Option<Vec<WordIdInt>>)> = cache
        .iter()
        .map(|(key, set)| {
            let set = set.as_ref().map(|set| {
                let mut ids: Vec<WordIdInt> = set.iter().copied().collect();
                ids.sort_unstable();
                ids
            });
            (*key, set)
        })
        .collect();
    entries.sort_unstable_by_key(|(key, _)| *key);
    serializer.collect_map(entries)
}

impl RegexCache {
    pub fn new(word_hash: u64) -> Self {
        RegexCache {
            cache: DefaultHashMap::default(),
            word_hash,
        }
    }

    pub fn word_hash(&self) -> &u64 {
        &self.word_hash
    }

    pub(crate) fn get(&self, key: &u64) -> Option<&Option<DefaultHashSet<WordIdInt>>> {
        self.cache.get(key)
    }

    pub(crate) fn insert(&mut self, key: u64, value: Option<DefaultHashSet<WordIdInt>>) {
        self.cache.insert(key, value);
    }
}

pub(crate) struct BuildInfo {
    tagger: Arc<Tagger>,
    regex_cache: RegexCache,
    build_dir: PathBuf,
}

impl BuildInfo {
    pub fn new(tagger: Arc<Tagger>, regex_cache: RegexCache, build_dir: PathBuf) -> Self {
        BuildInfo {
            tagger,
            regex_cache,
            build_dir,
        }
    }

    pub fn tagger(&self) -> &Arc<Tagger> {
        &self.tagger
    }

    pub fn mut_regex_cache(&mut self) -> &mut RegexCache {
        &mut self.regex_cache
    }

    /// The directory the rules are compiled from; filter data tables live under
    /// its `builtin/` directory.
    pub fn build_dir(&self) -> &std::path::Path {
        &self.build_dir
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_match_attribs(
    attribs: impl structure::MatchAttributes,
    text: Option<&str>,
    case_sensitive: bool,
    text_match_idx: Option<usize>,
    reference_conversion: Option<Conversion>,
    reference_synthesis: Option<Match>,
    reference_affix: Option<(String, String)>,
    info: &mut BuildInfo,
) -> Result<Atom, Error> {
    let mut atoms: Vec<Atom> = Vec::new();

    let case_sensitive = match attribs.case_sensitive().as_deref() {
        Some("yes") => true,
        Some("no") => false,
        None => case_sensitive,
        x => panic!("unknown case_sensitive value {:?}", x),
    };

    let inflected = match attribs.inflected().as_deref() {
        Some("yes") => true,
        Some("no") => false,
        None => false,
        x => panic!("unknown inflected value {:?}", x),
    };

    let is_regex = match attribs.regexp().as_deref() {
        Some("yes") => true,
        None => false,
        x => panic!("unknown regexp value {:?}", x),
    };

    let is_postag_regexp = match attribs.postag_regexp().as_deref() {
        Some("yes") => true,
        None => false,
        x => panic!("unknown postag_regexp value {:?}", x),
    };

    let negate = match attribs.negate().as_deref() {
        Some("yes") => true,
        None => false,
        x => panic!("unknown negate value {:?}", x),
    };

    let negate_pos = match attribs.negate_pos().as_deref() {
        Some("yes") => true,
        None => false,
        x => panic!("unknown negate_pos value {:?}", x),
    };

    let mut inflect_matcher = None;
    let mut pos_matcher = None;

    if text.is_some() || text_match_idx.is_some() {
        let matcher = if let (Some(id), Some(reference)) = (text_match_idx, reference_synthesis) {
            Matcher::new_string(
                either::Right(GraphId(id)),
                negate,
                case_sensitive,
                inflected,
            )
            .with_synthesis(reference, is_regex)
        } else if is_regex {
            if let Some(text) = text {
                let regex = Regex::from_java_regex(text.trim(), true, case_sensitive);
                Matcher::new_regex(regex?, negate, inflected)
            } else {
                return Err(Error::Unexpected("`text` must be set if regex".into()));
            }
        } else {
            let matcher = Matcher::new_string(
                text_match_idx.map_or_else(
                    || {
                        either::Left(
                            text.expect("either `text_match_idx` or `text` are set.")
                                .trim()
                                .to_string(),
                        )
                    },
                    // this is validated in Composition::new, otherwise creating a graph id is not valid!
                    |id| either::Right(GraphId(id)),
                ),
                negate,
                case_sensitive,
                inflected,
            );
            let matcher = match (text_match_idx, reference_conversion) {
                (Some(_), Some(conversion)) => matcher.with_reference_conversion(conversion),
                _ => matcher,
            };
            match (text_match_idx, reference_affix) {
                (Some(_), Some((prefix, suffix))) => matcher.with_reference_affix(&prefix, &suffix),
                _ => matcher,
            }
        };

        if inflected {
            inflect_matcher = Some(matcher);
        } else {
            atoms.push(
                (TextAtom {
                    matcher: TextMatcher::new(matcher, info),
                })
                .into(),
            );
        }
    }

    if let Some(postag) = attribs.postag() {
        let raw_matcher = if is_postag_regexp {
            let regex = Regex::from_java_regex(&postag.trim(), true, true);
            Matcher::new_regex(regex?, negate_pos, true)
        } else {
            Matcher::new_string(
                either::Left(postag.trim().to_string()),
                negate_pos,
                true,
                true,
            )
        };
        pos_matcher = Some(PosMatcher::new(raw_matcher, info));
    }

    if pos_matcher.is_some() || inflect_matcher.is_some() {
        let matcher = WordDataMatcher {
            pos_matcher,
            inflect_matcher: inflect_matcher.map(|x| TextMatcher::new(x, info)),
        };
        atoms.push(
            (WordDataAtom {
                matcher,
                case_sensitive,
                first_reading: false,
            })
            .into(),
        );
    }

    match (attribs.chunk(), attribs.chunk_re()) {
        (Some(chunk), None) => {
            let chunk_atom = ChunkAtom {
                matcher: Matcher::new_string(
                    either::Left(chunk.trim().to_string()),
                    false,
                    true,
                    true,
                ),
            };
            atoms.push(chunk_atom.into());
        }
        (None, Some(chunk_re)) => {
            let regex = Regex::from_java_regex(chunk_re.trim(), true, true)?;
            let chunk_atom = ChunkAtom {
                matcher: Matcher::new_regex(regex, false, true),
            };
            atoms.push(chunk_atom.into());
        }
        (None, None) => {}
        _ => panic!("unexpected combination of chunk / chunk_re values."),
    }

    if let Some(chunk) = attribs.chunk() {
        let chunk_atom = ChunkAtom {
            matcher: Matcher::new_string(either::Left(chunk.trim().to_string()), false, true, true),
        };

        atoms.push(chunk_atom.into());
    }

    if let Some(space_before) = attribs.spacebefore() {
        let value = match space_before.as_str() {
            "yes" => true,
            "no" => false,
            _ => panic!("unknown spacebefore value {}", space_before),
        };

        atoms.push((SpaceBeforeAtom { value }).into());
    }

    Ok(AndAtom::and(atoms))
}

fn first_reading_only(atom: &mut Atom) {
    match atom {
        Atom::WordDataAtom(x) => x.first_reading = true,
        Atom::AndAtom(x) => x.atoms.iter_mut().for_each(first_reading_only),
        Atom::OrAtom(x) => x.atoms.iter_mut().for_each(first_reading_only),
        Atom::NotAtom(x) => first_reading_only(&mut x.atom),
        Atom::OffsetAtom(x) => first_reading_only(&mut x.atom),
        _ => {}
    }
}

#[allow(clippy::result_large_err)]
fn get_next_exceptions(
    token: &structure::Token,
    case_sensitive: bool,
    info: &mut BuildInfo,
) -> Result<Option<NextExceptions>, Error> {
    let exceptions = token
        .parts
        .iter()
        .flatten()
        .filter_map(|x| match x {
            structure::TokenPart::Exception(x) if x.scope.as_deref() == Some("next") => Some(x),
            _ => None,
        })
        .map(|x| {
            parse_match_attribs(
                x,
                x.text.as_ref().map(|text| text.as_str()),
                case_sensitive,
                None,
                None,
                None,
                info,
            )
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok((!exceptions.is_empty()).then(|| {
        let all = OrAtom::or(exceptions);
        let mut first = all.clone();
        first_reading_only(&mut first);
        NextExceptions { all, first }
    }))
}

fn get_exceptions(
    token: &structure::Token,
    case_sensitive: bool,
    info: &mut BuildInfo,
) -> Result<Atom, Error> {
    if let Some(parts) = &token.parts {
        let exceptions: Vec<Atom> = parts
            .iter()
            .filter_map(|x| match x {
                structure::TokenPart::Exception(x) => Some(x),
                _ => None,
            })
            .filter_map(|x| {
                let exception_text = if let Some(exception_text) = &x.text {
                    Some(exception_text.as_str())
                } else {
                    None
                };
                let mut atom = match parse_match_attribs(
                    x,
                    exception_text,
                    case_sensitive,
                    None,
                    None,
                    None,
                    None,
                    info,
                ) {
                    Ok(atom) => atom,
                    Err(err) => return Some(Err(err)),
                };

                let offset = if let Some(scope) = &x.scope {
                    match scope.as_str() {
                        "next" => return None,
                        "current" => 0,
                        "previous" => -1,
                        _ => panic!("unknown scope value {}", scope),
                    }
                } else {
                    0
                };

                if offset != 0 {
                    atom = OffsetAtom::new(atom, offset).into();
                }

                Some(Ok(atom))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(NotAtom::not(OrAtom::or(exceptions)))
    } else {
        Ok((TrueAtom {}).into())
    }
}

fn reference_affix(parts: &[structure::TokenPart]) -> (String, String) {
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut seen_reference = false;
    for part in parts {
        match part {
            structure::TokenPart::Text(text) if seen_reference => suffix.push_str(text),
            structure::TokenPart::Text(text) => prefix.push_str(text),
            structure::TokenPart::Sub(_) => seen_reference = true,
            structure::TokenPart::Exception(_) => {}
        }
    }
    (
        prefix.trim_start().to_string(),
        suffix.trim_end().to_string(),
    )
}

fn parse_token(
    token: &structure::Token,
    case_sensitive: bool,
    info: &mut BuildInfo,
) -> Result<Vec<Part>, Error> {
    let mut parts = Vec::new();
    let text = if let Some(parts) = &token.parts {
        parts.iter().find_map(|x| match x {
            structure::TokenPart::Text(text) => Some(text.as_str()),
            _ => None,
        })
    } else {
        None
    };

    let text_match = if let Some(parts) = &token.parts {
        match parts.iter().find_map(|x| match x {
            structure::TokenPart::Sub(sub) => Some(sub),
            _ => None,
        }) {
            None => None,
            Some(sub) => {
                // `<token><match no="…" regexp_match="…"/></token>` and
                // `include_skipped` are not implemented. `no`,
                // `case_conversion` and the `postag` synthesis are.
                if sub.regexp_match.is_some()
                    || sub.regexp_replace.is_some()
                    || sub.include_skipped.is_some()
                {
                    return Err(Error::Unimplemented(
                        "regexp_match/include_skipped on <match> inside a <token> are not implemented."
                            .into(),
                    ));
                }

                let conversion = match sub.case_conversion.as_deref() {
                    None => None,
                    Some("alllower") => Some(Conversion::AllLower),
                    Some("startlower") => Some(Conversion::StartLower),
                    Some("allupper") => Some(Conversion::AllUpper),
                    Some("startupper") => Some(Conversion::StartUpper),
                    Some("preserve") => Some(Conversion::Preserve),
                    Some(x) => {
                        return Err(Error::Unimplemented(format!(
                            "case conversion {} not supported.",
                            x
                        )))
                    }
                };

                let synthesis = match &sub.postag {
                    None => None,
                    Some(postag) => Some(Match {
                        id: GraphId(0),
                        conversion: conversion.clone().unwrap_or(Conversion::Nop),
                        pos_replacer: Some(parse_pos_replacer(
                            postag,
                            matches!(sub.postag_regexp.as_deref(), Some("yes")),
                            sub.postag_replace.clone(),
                            false,
                        )?),
                        regex_replacer: None,
                        static_lemma: None,
                        include_skipped: None,
                    }),
                };

                let (prefix, suffix) = reference_affix(parts);
                if (!prefix.is_empty() || !suffix.is_empty())
                    && (synthesis.is_some() || token.regexp.as_deref() == Some("yes"))
                {
                    return Err(Error::Unimplemented(
                        "literal text around a <match> with postag or regexp inside a <token> is not implemented."
                            .into(),
                    ));
                }

                Some((
                    sub.no.parse::<usize>()? + 1,
                    conversion,
                    synthesis,
                    (prefix, suffix),
                ))
            }
        }
    } else {
        None
    };
    let (text_match_idx, reference_conversion, reference_synthesis, reference_affix) =
        match text_match {
            Some((idx, conversion, synthesis, affix)) => {
                (Some(idx), conversion, synthesis, Some(affix))
            }
            None => (None, None, None, None),
        };

    let min = token
        .min
        .clone()
        .map(|x| {
            if x == "-1" {
                1
            } else {
                x.parse().expect("can't parse min as usize")
            }
        })
        .unwrap_or(1usize);
    let mut max = token
        .max
        .clone()
        .map(|x| {
            if x == "-1" {
                max_matches()
            } else {
                x.parse().expect("can't parse max as usize")
            }
        })
        .unwrap_or(1usize);
    if min > 1 && max == 1 {
        max = min;
    }

    let quantifier = Quantifier::new(min, max);
    let mut atom = parse_match_attribs(
        token,
        text,
        case_sensitive,
        text_match_idx,
        reference_conversion,
        reference_synthesis,
        reference_affix,
        info,
    )?;
    atom = AndAtom::and(vec![atom, get_exceptions(token, case_sensitive, info)?]);

    parts.push(Part {
        atom,
        quantifier,
        visible: true,
        unify: token.unify.as_ref().map(|x| x == "yes"),
        next_exception: get_next_exceptions(token, case_sensitive, info)?.map(Box::new),
    });

    if let Some(to_skip) = token.skip.clone() {
        let to_skip = if to_skip == "-1" {
            u32::MAX as usize
        } else {
            to_skip.parse().expect("can't parse skip as usize or -1")
        };

        parts.push(Part {
            atom: (TrueAtom {}).into(),
            quantifier: Quantifier::new(0, to_skip),
            visible: false,
            unify: None,
            next_exception: None,
        });
    }

    Ok(parts)
}

fn parse_pos_replacer(
    postag: &str,
    postag_regexp: bool,
    postag_replace: Option<String>,
    has_static_lemma: bool,
) -> Result<PosReplacer, Error> {
    let postag = postag.trim();

    if postag == "_spell_number_" {
        return Ok(PosReplacer {
            postag: postag.to_string(),
            regex: None,
            postag_replace: None,
            determiner: None,
            spell_number: true,
        });
    } else if postag.starts_with("_spell_number_") {
        return Err(Error::Unimplemented(format!(
            "the number spelling postag '{}' is not supported.",
            postag
        )));
    }

    // LanguageTool's English synthesizer extends synthesis with two special
    // postags: `+INDT` adds "a"/"an" and `+DT` adds "a"/"an" or "the". In the
    // regexp form the tag ends with `\+INDT`/`\+DT` (a `\`-escaped `+` in the
    // XML); the suffix is removed before synthesizing the base tag.
    let (determiner, base_postag) = if postag == "+INDT" {
        (Some(Determiner::Indefinite), None)
    } else if postag == "+DT" {
        (Some(Determiner::Definite), None)
    } else if postag_regexp && postag.ends_with("\\+INDT") {
        (
            Some(Determiner::Indefinite),
            Some(postag[..postag.len() - "\\+INDT".len()].trim()),
        )
    } else if postag_regexp && postag.ends_with("\\+DT") {
        (
            Some(Determiner::Definite),
            Some(postag[..postag.len() - "\\+DT".len()].trim()),
        )
    } else if postag.contains("+DT") || postag.contains("+INDT") {
        return Err(Error::Unimplemented(format!(
            "the determiner postag '{}' is not supported here.",
            postag
        )));
    } else {
        (None, None)
    };

    // A determiner with an exact postag formats the matched token itself; a
    // regexp determiner synthesizes the stripped base tag.
    let synthesis_postag = base_postag.unwrap_or(postag);

    // LanguageTool's `setLemmaString` treats a static lemma's `postag` as a regexp even when
    // `postag_regexp` is not set, so a static lemma always uses the regexp synthesis branch.
    let regex = if determiner.is_some() && base_postag.is_none() {
        None
    } else if postag_regexp || has_static_lemma {
        Some(Regex::from_java_regex(synthesis_postag, true, true)?)
    } else {
        None
    };

    Ok(PosReplacer {
        postag: synthesis_postag.to_string(),
        regex,
        postag_replace,
        determiner,
        spell_number: false,
    })
}

fn parse_match(
    m: structure::Match,
    engine: &Engine,
    _info: &mut BuildInfo,
    pool: &PartInterner,
) -> Result<Match, Error> {
    let id =
        m.no.parse::<usize>()
            .expect("no must be parsable as usize.");
    let graph_id = engine.to_graph_id(id)?;

    // `<match include_skipped="...">` includes the tokens skipped by the
    // referenced token's `skip` attribute. In the compiled pattern those tokens
    // form an invisible part directly after the referenced element, so remember
    // its graph index; without a `skip` attribute the attribute has no effect.
    let include_skipped = match m.include_skipped.as_deref() {
        None => None,
        Some(value) => {
            let range = match value {
                "all" => IncludeRange::All,
                "following" => IncludeRange::Following,
                other => {
                    return Err(Error::Unimplemented(format!(
                        "include_skipped value '{}' is not supported.",
                        other
                    )))
                }
            };

            match engine {
                Engine::Token(engine) => {
                    // `id_to_idx` maps a graph id to `part_index + 1`; the skip
                    // part, if any, is the part right after the referenced one.
                    let group = engine
                        .composition
                        .id_to_idx
                        .get(graph_id.0)
                        .copied()
                        .filter(|&index| index != u32::MAX)
                        .ok_or_else(|| {
                            Error::Unexpected("match reference is not in the pattern".into())
                        })? as usize;
                    let has_skip = engine
                        .composition
                        .parts
                        .get(group)
                        .map_or(false, |id| !pool.get(*id).visible);

                    has_skip.then_some(IncludeSkipped {
                        range,
                        group: group + 1,
                    })
                }
                Engine::Text(..) => {
                    return Err(Error::Unimplemented(
                        "include_skipped in `match` is not implemented for regexp rules.".into(),
                    ))
                }
            }
        }
    };

    let case_conversion = if let Some(conversion) = &m.case_conversion {
        Some(conversion.as_str())
    } else {
        None
    };

    let static_lemma = m
        .text
        .as_ref()
        .map(|text| text.text.trim().to_string())
        .filter(|text| !text.is_empty());

    let postag_regexp = matches!(m.postag_regex.as_deref(), Some("yes"));

    let pos_replacer = match m.postag.as_deref() {
        Some(postag) => Some(parse_pos_replacer(
            postag,
            postag_regexp,
            m.postag_replace.clone(),
            static_lemma.is_some(),
        )?),
        None => None,
    };

    let regex_replacer = match (m.regexp_match, m.regexp_replace) {
        (Some(regex_match), Some(regex_replace)) => Some((
            Regex::from_java_regex(&regex_match, false, true)?,
            regex_replace,
        )),
        _ => None,
    };

    Ok(Match {
        id: graph_id,
        conversion: match case_conversion {
            Some("alllower") => Conversion::AllLower,
            Some("startlower") => Conversion::StartLower,
            Some("startupper") => Conversion::StartUpper,
            Some("allupper") => Conversion::AllUpper,
            // mirrors LanguageTool's CaseConversionHelper: uppercase the
            // replacement if the matched text was uppercase (all-uppercase if
            // the match was all-uppercase, else just the first char).
            Some("preserve") => Conversion::Preserve,
            Some(x) => {
                return Err(Error::Unimplemented(format!(
                    "case conversion {} not supported.",
                    x
                )))
            }
            None => Conversion::Nop,
        },
        pos_replacer,
        regex_replacer,
        static_lemma,
        include_skipped,
    })
}

fn parse_synthesizer_text(text: &str, engine: &Engine) -> Result<Vec<SynthesizerPart>, Error> {
    // LanguageTool resolves a backreference as `\` followed by *all* consecutive
    // digits: `PatternRuleMatcher.formatMatches` measures `numLen` while the next
    // character is a digit and then parses the whole number, so `\13` refers to
    // token 13, not to `\1` followed by a literal `3`. Match that here — a
    // reference may have more than one digit.
    lazy_static! {
        static ref MATCH_REGEX: Regex = Regex::new(r"\\(\d+)".into());
    }

    let mut parts = Vec::new();
    let mut end_index = 0;

    for capture in MATCH_REGEX.captures_iter(&text) {
        let mat = capture.get(0).expect("0th regex group exists");

        if end_index != mat.start() {
            parts.push(SynthesizerPart::Text(
                (&text[end_index..mat.start()]).to_string(),
            ))
        }

        let id = capture
            .get(1)
            .expect("1st regex group exists")
            .as_str()
            .parse::<usize>()
            .expect("match regex capture must be parsable as usize.");

        parts.push(SynthesizerPart::Match(
            Match {
                id: engine.to_graph_id(id)?,
                conversion: Conversion::Nop,
                pos_replacer: None,
                regex_replacer: None,
                static_lemma: None,
                include_skipped: None,
            }
            .into(),
        ));
        end_index = mat.end();
    }

    if end_index < text.len() {
        parts.push(SynthesizerPart::Text((&text[end_index..]).to_string()))
    }
    Ok(parts)
}

fn parse_suggestion(
    data: structure::Suggestion,
    engine: &Engine,
    rule_suppress_misspelled: bool,
    info: &mut BuildInfo,
    pool: &PartInterner,
) -> Result<Synthesizer, Error> {
    let suppress_misspelled =
        rule_suppress_misspelled || data.suppress_misspelled.as_deref() == Some("yes");
    let mut parts = Vec::new();
    for part in data.parts {
        match part {
            structure::SuggestionPart::Text(text) => {
                parts.extend(parse_synthesizer_text(text.as_str(), engine)?);
            }
            structure::SuggestionPart::Match(m) => {
                parts.push(SynthesizerPart::Match(
                    parse_match(m, engine, info, pool)?.into(),
                ));
            }
        }
    }

    Ok(Synthesizer {
        parts,
        // use titlecase adjustment (i. e. make replacement title case if match is title case) if token rule
        use_titlecase_adjust: matches!(engine, Engine::Token(_)),
        suppress_misspelled,
    })
}

#[allow(clippy::result_large_err)]
fn parse_antipattern(
    pattern: structure::Pattern,
    info: &mut BuildInfo,
    pool: &mut PartInterner,
) -> Result<(Composition, Option<(GraphId, GraphId)>), Error> {
    let has_marker = pattern
        .parts
        .iter()
        .any(|part| matches!(part, structure::PatternPart::Marker(_)));
    let (composition, start, end) = parse_pattern(pattern, info, pool)?;
    let max_id = composition.id_to_idx.len() - 1;
    let marker = has_marker.then(|| (GraphId(start.min(max_id)), GraphId(end.min(max_id))));
    Ok((composition, marker))
}

fn get_last_id(parts: &[Part]) -> isize {
    parts.iter().fold(1, |a, x| a + x.visible as isize)
}

fn parse_parallel_tokens(
    tokens: &[structure::Token],
    combine: fn(Vec<Atom>) -> Atom,
    case_sensitive: bool,
    info: &mut BuildInfo,
) -> Result<Vec<Part>, Error> {
    let mut parts = parse_token(&tokens[0], case_sensitive, info)?;
    let mut atoms = vec![std::mem::replace(&mut parts[0].atom, (TrueAtom {}).into())];

    for token in &tokens[1..] {
        let mut parsed = parse_token(token, case_sensitive, info)?;

        if parsed.len() != 1 || parsed[0].quantifier.min != 1 || parsed[0].quantifier.max != 1 {
            return Err(Error::Unimplemented(
                "control flow on a non-first parallel token is not implemented.".into(),
            ));
        }

        atoms.push(parsed.remove(0).atom);
    }

    parts[0].atom = combine(atoms);
    Ok(parts)
}

fn parse_tokens(
    tokens: &[structure::TokenCombination],
    case_sensitive: bool,
    info: &mut BuildInfo,
) -> Result<Vec<Part>, Error> {
    let mut out = Vec::new();

    for token_combination in tokens {
        out.extend(match token_combination {
            structure::TokenCombination::Token(token) => parse_token(token, case_sensitive, info)?,
            structure::TokenCombination::And(tokens) => {
                parse_parallel_tokens(&tokens.tokens, AndAtom::and, case_sensitive, info)?
            }
            structure::TokenCombination::Or(tokens) => {
                parse_parallel_tokens(&tokens.tokens, OrAtom::or, case_sensitive, info)?
            }
            structure::TokenCombination::Feature(_) => Vec::new(),
        });
    }

    Ok(out)
}

fn require_last_token_match(parts: &mut [Part]) {
    if let Some(last) = parts.iter_mut().rev().find(|part| part.visible) {
        if last.quantifier.min == 0 {
            last.quantifier.min = 1;
        }
    }
}

fn token_uses_readings(token: &structure::Token) -> bool {
    token.postag.is_some()
        || token.inflected.as_deref() == Some("yes")
        || token.chunk.is_some()
        || token.chunk_re.is_some()
        || token.unify.is_some()
        || token.parts.iter().flatten().any(|part| match part {
            structure::TokenPart::Exception(exception) => {
                exception.postag.is_some()
                    || exception.inflected.as_deref() == Some("yes")
                    || exception.chunk.is_some()
                    || exception.chunk_re.is_some()
            }
            structure::TokenPart::Sub(sub) => sub.postag.is_some(),
            structure::TokenPart::Text(_) => false,
        })
}

fn pattern_uses_readings(pattern: &structure::Pattern) -> bool {
    pattern.parts.iter().any(|part| match part {
        structure::PatternPart::Token(token) => token_uses_readings(token),
        structure::PatternPart::Marker(marker) => {
            marker.tokens.iter().any(|combination| match combination {
                structure::TokenCombination::Token(token) => token_uses_readings(token),
                structure::TokenCombination::Or(tokens)
                | structure::TokenCombination::And(tokens) => {
                    tokens.tokens.iter().any(token_uses_readings)
                }
                structure::TokenCombination::Feature(_) => true,
            })
        }
        structure::PatternPart::Or(tokens) | structure::PatternPart::And(tokens) => {
            tokens.tokens.iter().any(token_uses_readings)
        }
        structure::PatternPart::Feature(_) => true,
        structure::PatternPart::Example(_) | structure::PatternPart::Phraseref(_) => false,
    })
}

fn rule_reads_readings_outside_pattern(rule: &structure::Rule) -> bool {
    let synthesizes = |m: &structure::Match| m.postag.is_some();
    let suggestion_synthesizes = |suggestion: &structure::Suggestion| {
        suggestion.parts.iter().any(|part| match part {
            structure::SuggestionPart::Match(m) => synthesizes(m),
            structure::SuggestionPart::Text(_) => false,
        })
    };

    rule.filter.is_some()
        || rule.message.parts.iter().any(|part| match part {
            structure::MessagePart::Suggestion(suggestion) => suggestion_synthesizes(suggestion),
            structure::MessagePart::Match(m) => synthesizes(m),
            structure::MessagePart::Text(_) => false,
        })
        || rule
            .suggestions
            .iter()
            .flatten()
            .any(suggestion_synthesizes)
}

fn reads_raw_readings(rule: &structure::Rule) -> bool {
    rule.pattern.as_ref().is_some_and(|pattern| {
        pattern.raw_pos.as_deref() == Some("yes")
            && (pattern_uses_readings(pattern) || rule_reads_readings_outside_pattern(rule))
    })
}

fn parse_pattern(
    pattern: structure::Pattern,
    info: &mut BuildInfo,
    pool: &mut PartInterner,
) -> Result<(Composition, usize, usize), Error> {
    let mut start = None;
    let mut end = None;

    let mut composition_parts = Vec::new();
    let case_sensitive = match &pattern.case_sensitive {
        Some(string) => string == "yes",
        None => false,
    };

    for part in pattern.parts.iter() {
        match part {
            structure::PatternPart::Token(token) => {
                composition_parts.extend(parse_token(token, case_sensitive, info)?)
            }
            structure::PatternPart::Marker(marker) => {
                start = Some(get_last_id(&composition_parts));

                composition_parts.extend(parse_tokens(&marker.tokens, case_sensitive, info)?);

                end = Some(get_last_id(&composition_parts));
            }
            structure::PatternPart::And(tokens) => composition_parts.extend(parse_parallel_tokens(
                &tokens.tokens,
                AndAtom::and,
                case_sensitive,
                info,
            )?),
            structure::PatternPart::Or(tokens) => composition_parts.extend(parse_parallel_tokens(
                &tokens.tokens,
                OrAtom::or,
                case_sensitive,
                info,
            )?),
            structure::PatternPart::Feature(_) => {}
            // documentation-only `<example>` inside `<antipattern>`
            structure::PatternPart::Example(_) => {}
            structure::PatternPart::Phraseref(_) => {
                // `<phraseref>` expands a `<phrase>` that nlprule does not
                // parse; dropping it would silently change the pattern.
                return Err(Error::Unimplemented(
                    "phraseref in <pattern> (phrase expansion) is not implemented.".into(),
                ));
            }
        }
    }

    require_last_token_match(&mut composition_parts);

    let start = start.unwrap_or(1) as usize;
    let end = end.unwrap_or_else(|| get_last_id(&composition_parts)) as usize - 1;

    let composition = Composition::new(composition_parts, pool)?;

    Ok((composition, start, end))
}

fn parse_features(
    pattern: &structure::Pattern,
    unifications: &Option<Vec<structure::Unification>>,
    info: &mut BuildInfo,
) -> Vec<Vec<PosFilter>> {
    let mut filters = Vec::new();
    let mut parse_feature = |id: &str| -> Vec<PosFilter> {
        let unification = unifications
            .as_ref()
            .unwrap()
            .iter()
            .find(|x| x.feature == id)
            .unwrap();

        unification
            .equivalences
            .iter()
            .map(|equiv| {
                parse_pos_filter(
                    &equiv.token.postag,
                    equiv.token.postag_regexp.as_deref(),
                    info,
                )
            })
            .collect()
    };

    for part in &pattern.parts {
        match part {
            structure::PatternPart::Feature(feature) => filters.push(parse_feature(&feature.id)),
            structure::PatternPart::Marker(marker) => {
                for token_combination in &marker.tokens {
                    if let structure::TokenCombination::Feature(feature) = token_combination {
                        filters.push(parse_feature(&feature.id));
                    }
                }
            }
            _ => {}
        }
    }

    filters
}

impl Rule {
    pub(crate) fn from_rule_structure(
        data: structure::Rule,
        info: &mut BuildInfo,
        pool: &mut PartInterner,
    ) -> Result<Rule, Error> {
        let is_regex_rule = data.regex.is_some();

        let raw_pos = reads_raw_readings(&data);

        let (engine, start, end) = match (&data.pattern, data.regex) {
            (Some(_), Some(_)) => Err(Error::Unexpected(
                "must not contain both `pattern` and `regexp`.".into(),
            )),
            (None, None) => Err(Error::Unexpected(
                "either `pattern` or `regexp` must be supplied.".into(),
            )),
            (Some(pattern), None) => {
                let (composition, start, end) = parse_pattern(pattern.clone(), info, pool)?;
                let antipatterns = if let Some(antipatterns) = data.antipatterns {
                    antipatterns
                        .into_iter()
                        .map(|pattern| parse_antipattern(pattern, info, pool))
                        .collect::<Result<Vec<_>, Error>>()?
                } else {
                    Vec::new()
                };

                if antipatterns.iter().any(|(pattern, _)| {
                    pattern.parts.iter().any(|id| pool.get(*id).unify.is_some())
                }) {
                    return Err(Error::Unimplemented(
                        "`unify` in antipattern is not supported.".into(),
                    ));
                }

                Ok((
                    Engine::Token(TokenEngine {
                        composition,
                        antipatterns: antipatterns
                            .into_iter()
                            .map(|(pattern, marker)| pool.intern_antipattern(pattern, marker))
                            .collect(),
                    }),
                    start,
                    end,
                ))
            }
            (None, Some(regex)) => {
                let case_sensitive = match regex.case_sensitive.as_deref() {
                    Some("yes") => true,
                    None => false,
                    x => panic!("unknown case_sensitive value {:?}", x),
                };
                let mark = regex.mark.map_or(Ok(0), |x| x.parse())?;
                let regex_text = match regex.kind.as_deref() {
                    // `smart` (the default) lets a space in the regexp match a
                    // run of whitespace, like LanguageTool's
                    // `PatternRuleHandler.replaceSpacesInRegex`.
                    Some("exact") => regex.text.to_string(),
                    Some("smart") | None => super::utils::replace_spaces_in_regex(&regex.text),
                    Some(x) => panic!("unknown regexp type {:?}", x),
                };
                let regex = Regex::from_java_regex(&regex_text, false, case_sensitive)?;
                let id_to_idx: DefaultHashMap<GraphId, usize> = (0..regex.captures_len() + 1)
                    .enumerate()
                    // the IDs in a regex rule are just the same as indices
                    .map(|(key, value)| (GraphId(key), value))
                    .collect();
                Ok((Engine::Text(regex.into(), id_to_idx), mark, mark))
            }
        }?;

        let maybe_composition = if let Engine::Token(engine) = &engine {
            Some(&engine.composition)
        } else {
            None
        };

        let unify_data = if let Some(pattern) = &data.pattern {
            let unify_filters = parse_features(&pattern, &data.unifications, info);
            let unify_mask: Vec<_> = maybe_composition
                .unwrap()
                .parts
                .iter()
                .map(|id| pool.get(*id).unify)
                .collect();
            Some((unify_filters, unify_mask))
        } else {
            None
        };

        let filter = if let Some(filter_data) = &data.filter {
            Some(super::impls::rule_filters::get_rule_filter(
                &filter_data.class,
                &filter_data.args,
                is_regex_rule,
                info.build_dir(),
            )?)
        } else {
            None
        };

        let mut message_parts = Vec::new();
        let mut suggesters = Vec::new();
        let suppress_misspelled = data.message.suppress_misspelled.as_deref() == Some("yes");

        for part in data.message.parts {
            match part {
                structure::MessagePart::Suggestion(suggestion) => {
                    let suggester = parse_suggestion(
                        suggestion.clone(),
                        &engine,
                        suppress_misspelled,
                        info,
                        pool,
                    )?;
                    // simpler to just parse a second time than cloning the result
                    message_parts.extend(
                        parse_suggestion(suggestion, &engine, suppress_misspelled, info, pool)?
                            .parts,
                    );
                    suggesters.push(suggester);
                }
                structure::MessagePart::Text(text) => {
                    message_parts.extend(parse_synthesizer_text(text.as_str(), &engine)?);
                }
                structure::MessagePart::Match(m) => {
                    message_parts.push(SynthesizerPart::Match(
                        parse_match(m, &engine, info, pool)?.into(),
                    ));
                }
            }
        }

        if let Some(suggestions) = data.suggestions {
            for suggestion in suggestions {
                suggesters.push(parse_suggestion(
                    suggestion,
                    &engine,
                    suppress_misspelled,
                    info,
                    pool,
                )?);
            }
        }

        // A rule with no <suggestion> (neither embedded in <message> nor
        // standalone) still reports a match with its <message> and no
        // replacement, so it is compiled normally (see `Suggestions`).

        assert!(!message_parts.is_empty(), "Rules must have a message.");

        let mut examples = Vec::new();
        for example in &data.examples {
            let mut texts = Vec::new();
            let mut suggestion: Option<Suggestion> = None;
            let mut marker_span: Option<Span> = None;

            for part in &example.parts {
                match part {
                    structure::ExamplePart::Text(text) => {
                        texts.push(text.as_str());
                    }
                    structure::ExamplePart::Marker(marker) => {
                        let (bytes_before, chars_before) =
                            texts.iter().fold((0, 0), |acc, text| {
                                (acc.0 + text.len(), acc.1 + text.chars().count())
                            });

                        if suggestion.is_some() {
                            return Err(Error::Unexpected(
                                "example must have one or zero markers".into(),
                            ));
                        }

                        let marker_text = marker.text.as_ref().map_or("", |x| x.as_str());
                        texts.push(marker_text);

                        let span = Span::new(
                            bytes_before..bytes_before + marker_text.len(),
                            chars_before..chars_before + marker_text.chars().count(),
                        );

                        if let Some(correction_text) = &example.correction {
                            let mut replacements: Vec<_> =
                                correction_text.split('|').map(|x| x.to_string()).collect();

                            replacements = if chars_before == 0 {
                                // title case if at start
                                replacements
                                    .into_iter()
                                    .map(|x| {
                                        utils::apply_to_first(&x, |c| c.to_uppercase().collect())
                                    })
                                    .collect()
                            } else {
                                replacements
                            };

                            suggestion = Some(Suggestion::new(
                                "_Test".into(),
                                "_Test".into(),
                                span.clone(),
                                replacements,
                            ));
                        }

                        // LanguageTool checks an incorrect example against its
                        // first marker; a correct example's markers are only
                        // documentation.
                        marker_span.get_or_insert(span);
                    }
                }
            }

            // LanguageTool's `PatternRuleHandler` classifies an example by its
            // explicit `type` when present and by whether it carries a
            // `correction` otherwise; a `<marker>` alone does not make an
            // example incorrect (`PatternRuleHandler.java`, LT 6.6, classifies
            // as incorrect when `type="incorrect"` or a `correction` attribute
            // is present). `PatternRuleTest.testCorrectSentences` then asserts
            // that a correct example does not match, markers or not.
            let kind = match example.kind.as_deref() {
                Some("correct") => ExampleKind::Correct,
                Some("incorrect") => ExampleKind::Incorrect,
                Some("triggers_error") => ExampleKind::TriggersError,
                Some(other) => {
                    return Err(Error::Unimplemented(format!(
                        "unknown example type '{}'",
                        other
                    )))
                }
                None => {
                    if example.correction.is_some() {
                        ExampleKind::Incorrect
                    } else {
                        ExampleKind::Correct
                    }
                }
            };

            examples.push(Example {
                text: texts.join(""),
                suggestion,
                kind,
                marker: marker_span,
            });
        }

        let unification = if let Some((unify_filters, unify_mask)) = unify_data {
            if unify_filters.is_empty() {
                None
            } else {
                Some(Unification {
                    filters: unify_filters,
                    mask: unify_mask,
                })
            }
        } else {
            None
        };

        let message = Synthesizer {
            parts: message_parts,
            use_titlecase_adjust: true,
            suppress_misspelled: false,
        };

        // LanguageTool's `+DT`/`+INDT` synthesis chooses the article from the
        // build directory's word lists; record only whether this rule needs them.
        // The lists are stored once per `Rules` (see `Rules::from_xml`).
        let uses_articles = message.uses_determiner()
            || suggesters
                .iter()
                .any(|suggester| suggester.uses_determiner());

        Ok(Rule {
            start: engine.to_graph_id(start)?,
            end: engine.to_graph_id(end)?,
            engine,
            unification,
            filter,
            examples: crate::rule::Examples::from_vec(examples),
            suggesters,
            message,
            suppress_misspelled,
            uses_articles,
            raw_pos,
            url: data.url.map(|x| x.to_string()),
            short: data.short.map(|x| x.to_string()),
            // fields below need information from rule group / category, so are set later
            id: Index::default(),
            name: String::new(),
            category_name: String::new(),
            category_type: None,
            mother_tongue: None,
            enabled: true,
        })
    }
}

fn parse_tag_form(form: &str, info: &mut BuildInfo) -> Result<owned::Word, Error> {
    lazy_static! {
        static ref REGEX: Regex = Regex::new(r"(.+?)\[(.+?)\]".into());
    }

    let captures = REGEX
        .captures(form)
        .ok_or_else(|| Error::Unexpected(format!("tag form must match regex, found '{}'", form)))?;
    let text = captures.get(1).expect("1st regex group exists").as_str();
    let tags = captures.get(2).expect("2nd regex group exists").as_str();

    let tags = tags
        .split(',')
        .filter_map(|x| {
            if x == "</S>" {
                // special symbol, presumably for SENT_END, can be ignored
                return None;
            }

            let parts: Vec<_> = x.split('/').collect();
            if parts.len() < 2 {
                None
            } else {
                Some(owned::WordData::new(
                    info.tagger.id_word(parts[0].into()).to_owned_id(),
                    info.tagger.id_tag(parts[1]).to_owned_id(),
                ))
            }
        })
        .collect();

    Ok(owned::Word {
        text: info.tagger.id_word(text.into()).to_owned_id(),
        tags,
    })
}

impl owned::WordData {
    fn from_structure(data: structure::WordData, info: &mut BuildInfo) -> Self {
        owned::WordData::new(
            info.tagger
                .id_word(data.lemma.unwrap_or_else(String::new).into())
                .to_owned_id(),
            info.tagger
                .id_tag(data.pos.as_ref().map_or("", |x| x.as_str().trim()))
                .to_owned_id(),
        )
    }
}

fn parse_pos_filter(postag: &str, postag_regexp: Option<&str>, info: &mut BuildInfo) -> PosFilter {
    match postag_regexp.as_deref() {
        Some("yes") => PosFilter::new(PosMatcher::new(
            Matcher::new_regex(
                Regex::from_java_regex(&postag, true, true).unwrap(),
                false,
                true,
            ),
            info,
        )),
        Some(_) | None => PosFilter::new(PosMatcher::new(
            Matcher::new_string(either::Left(postag.into()), false, false, true),
            info,
        )),
    }
}

impl DisambiguationRule {
    pub(crate) fn from_rule_structure(
        data: structure::DisambiguationRule,
        info: &mut BuildInfo,
        pool: &mut PartInterner,
    ) -> Result<DisambiguationRule, Error> {
        // might need the pattern later so clone it here
        let (composition, start, end) = parse_pattern(data.pattern.clone(), info, pool)?;

        let unify_filters = parse_features(&data.pattern, &data.unifications, info);
        let unify_mask: Vec<_> = composition
            .parts
            .iter()
            .map(|id| pool.get(*id).unify)
            .collect();

        let antipatterns = if let Some(antipatterns) = data.antipatterns {
            antipatterns
                .into_iter()
                .map(|pattern| parse_antipattern(pattern, info, pool))
                .collect::<Result<Vec<_>, Error>>()?
        } else {
            Vec::new()
        };

        if antipatterns
            .iter()
            .any(|(pattern, _)| pattern.parts.iter().any(|id| pool.get(*id).unify.is_some()))
        {
            return Err(Error::Unimplemented(
                "`unify` in antipattern is not supported.".into(),
            ));
        }

        let engine = Engine::Token(TokenEngine {
            composition,
            antipatterns: antipatterns
                .into_iter()
                .map(|(pattern, marker)| pool.intern_antipattern(pattern, marker))
                .collect(),
        });

        let word_datas: Vec<_> = if let Some(wds) = data.disambig.word_datas {
            wds.into_iter()
                .map(|part| match part {
                    structure::DisambiguationPart::WordData(x) => {
                        either::Left(owned::WordData::from_structure(x, info))
                    }
                    structure::DisambiguationPart::Match(x) => either::Right(parse_pos_filter(
                        &x.postag.unwrap(),
                        x.postag_regexp.as_deref(),
                        info,
                    )),
                })
                .collect()
        } else {
            Vec::new()
        };

        let disambiguations = match data.disambig.action.as_deref() {
            Some("remove") => {
                if let Some(postag) = data.disambig.postag.as_ref() {
                    Ok(Disambiguation::Remove(vec![either::Right(
                        parse_pos_filter(postag, Some("yes"), info),
                    )]))
                } else {
                    Ok(Disambiguation::Remove(word_datas.into_iter().collect()))
                }
            }
            Some("add") => {
                if let Some(postag) = data.disambig.postag.as_ref() {
                    // `<disambig action="add" postag="…">`: add a reading with the
                    // given POS tag. An empty lemma means the matched token's text
                    // is used as the lemma at runtime, like LT does when the
                    // reading has no lemma (Disambiguation::Add).
                    Ok(Disambiguation::Add(vec![owned::WordData::new(
                        info.tagger.id_word("".into()).to_owned_id(),
                        info.tagger.id_tag(postag).to_owned_id(),
                    )]))
                } else {
                    Ok(Disambiguation::Add(
                        word_datas
                            .into_iter()
                            .map(|x| x.left().expect("match not supported for `add`"))
                            .collect(),
                    ))
                }
            }
            Some("replace") => Ok(Disambiguation::Replace(
                word_datas
                    .into_iter()
                    .map(|x| {
                        x.left()
                            .expect("match not supported for `replace` disambiguation")
                    })
                    .collect(),
            )),
            Some("ignore_spelling") => Ok(Disambiguation::IgnoreSpelling),
            Some("immunize") => Ok(Disambiguation::Nop), // immunize can probably not be ignored
            Some("filterall") => {
                let mut disambig = Vec::new();
                let mut marker_disambig = Vec::new();
                let mut has_marker = false;

                for part in &data.pattern.parts {
                    match part {
                        structure::PatternPart::Marker(marker) => {
                            has_marker = true;
                            for token in &marker.tokens {
                                let token = match token {
                                    structure::TokenCombination::Token(token) => token,
                                    structure::TokenCombination::And(tokens)
                                    | structure::TokenCombination::Or(tokens) => &tokens.tokens[0],
                                    structure::TokenCombination::Feature(_) => continue,
                                };

                                marker_disambig.push(token.postag.as_ref().map(|x| {
                                    either::Right(parse_pos_filter(
                                        x,
                                        token.postag_regexp.as_deref(),
                                        info,
                                    ))
                                }));
                            }
                        }
                        structure::PatternPart::Token(token) => {
                            disambig.push(token.postag.as_ref().map(|x| {
                                either::Right(parse_pos_filter(
                                    x,
                                    token.postag_regexp.as_deref(),
                                    info,
                                ))
                            }))
                        }
                        structure::PatternPart::And(tokens)
                        | structure::PatternPart::Or(tokens) => {
                            disambig.push(tokens.tokens[0].postag.as_ref().map(|x| {
                                either::Right(parse_pos_filter(
                                    x,
                                    tokens.tokens[0].postag_regexp.as_deref(),
                                    info,
                                ))
                            }))
                        }
                        structure::PatternPart::Feature(_)
                        | structure::PatternPart::Example(_)
                        | structure::PatternPart::Phraseref(_) => {}
                    }
                }

                let disambiguations = if has_marker {
                    marker_disambig
                } else {
                    disambig
                };

                Ok(Disambiguation::Filter(
                    disambiguations.into_iter().collect(),
                    info.tagger().lang_options().retain_last,
                ))
            }
            Some("filter") => {
                if let Some(postag) = data.disambig.postag.as_ref() {
                    Ok(Disambiguation::Filter(
                        vec![Some(either::Right(parse_pos_filter(
                            postag,
                            Some("yes"),
                            info,
                        )))],
                        info.tagger().lang_options().retain_last,
                    ))
                } else {
                    Ok(Disambiguation::Filter(
                        word_datas.into_iter().map(Some).collect(),
                        info.tagger().lang_options().retain_last,
                    ))
                }
            }
            Some("unify") => {
                let mut mask = Vec::new();
                let mut marker_mask = Vec::new();

                let mut disambig = Vec::new();
                let mut marker_disambig = Vec::new();
                let mut has_marker = false;

                for part in &data.pattern.parts {
                    match part {
                        structure::PatternPart::Marker(marker) => {
                            has_marker = true;
                            for token in &marker.tokens {
                                let token = match token {
                                    structure::TokenCombination::Token(token) => token,
                                    structure::TokenCombination::And(tokens)
                                    | structure::TokenCombination::Or(tokens) => &tokens.tokens[0],
                                    structure::TokenCombination::Feature(_) => continue,
                                };

                                marker_disambig.push(token.postag.as_ref().map(|x| {
                                    parse_pos_filter(x, token.postag_regexp.as_deref(), info)
                                }));
                                marker_mask.push(token.unify.is_some())
                            }
                        }
                        structure::PatternPart::Token(token) => {
                            disambig.push(token.postag.as_ref().map(|x| {
                                parse_pos_filter(x, token.postag_regexp.as_deref(), info)
                            }));
                            mask.push(token.unify.is_some());
                        }
                        structure::PatternPart::And(tokens)
                        | structure::PatternPart::Or(tokens) => {
                            disambig.push(tokens.tokens[0].postag.as_ref().map(|x| {
                                parse_pos_filter(x, tokens.tokens[0].postag_regexp.as_deref(), info)
                            }));
                            mask.push(tokens.tokens[0].unify.is_some());
                        }
                        structure::PatternPart::Feature(_)
                        | structure::PatternPart::Example(_)
                        | structure::PatternPart::Phraseref(_) => {}
                    }
                }

                let (disambig, mask) = if has_marker {
                    (marker_disambig, marker_mask)
                } else {
                    (disambig, mask)
                };

                Ok(Disambiguation::Unify(unify_filters.clone(), disambig, mask))
            }
            None => {
                if let Some(postag) = data.disambig.postag.as_ref() {
                    Ok(Disambiguation::Filter(
                        vec![Some(either::Left(owned::WordData::new(
                            info.tagger.id_word("".into()).to_owned_id(),
                            info.tagger.id_tag(postag).to_owned_id(),
                        )))],
                        info.tagger().lang_options().retain_last,
                    ))
                } else {
                    Ok(Disambiguation::Filter(
                        word_datas.into_iter().map(Some).collect(),
                        info.tagger().lang_options().retain_last,
                    ))
                }
            }
            Some(x) => Err(Error::Unimplemented(format!("action {}", x))),
        }?;

        let filter = if let Some(filter_data) = data.filter {
            let args = filter_data
                .args
                .split(' ')
                .map(|x| {
                    let idx = x.find(':').unwrap();
                    (
                        x[..idx].to_string(),
                        x[(idx + ':'.len_utf8())..].to_string(),
                    )
                })
                .collect();

            Some(super::impls::filters::get_filter(
                filter_data.class.split('.').next_back().unwrap(),
                args,
                &engine,
            )?)
        } else {
            None
        };

        let mut examples = Vec::new();

        if let Some(examples_structure) = data.examples.as_ref() {
            for example in examples_structure {
                let mut texts = Vec::new();
                let mut char_span: Option<Range<usize>> = None;
                let mut char_length = 0;

                for part in &example.parts {
                    match part {
                        structure::ExamplePart::Text(text) => {
                            texts.push(text.as_str());
                            char_length += text.chars().count();
                        }
                        structure::ExamplePart::Marker(marker) => {
                            if char_span.is_some() {
                                return Err(Error::Unexpected(
                                    "example must have one or zero markers".into(),
                                ));
                            }

                            let marker_text = marker.text.as_ref().map_or("", |x| x.as_str());
                            texts.push(marker_text);
                            let length = marker_text.chars().count();

                            char_span = Some(char_length..char_length + length);
                            char_length += marker_text.chars().count();
                        }
                    }
                }

                let text = texts.join("");

                let test = match example.kind.as_str() {
                    "untouched" => DisambiguationExample::Unchanged(text),
                    "ambiguous" => DisambiguationExample::Changed(DisambiguationChange {
                        text,
                        before: parse_tag_form(
                            example
                                .inputform
                                .as_ref()
                                .expect("must have inputform when ambiguous example"),
                            info,
                        )?,
                        after: parse_tag_form(
                            &example
                                .outputform
                                .as_ref()
                                .expect("must have inputform when ambiguous example"),
                            info,
                        )?,
                        char_span: char_span.expect("must have marker when ambiguous example"),
                    }),
                    x => panic!("unknown disambiguation example type {}", x),
                };

                examples.push(test);
            }
        }

        Ok(DisambiguationRule {
            start: engine.to_graph_id(start)?,
            end: engine.to_graph_id(end)?,
            engine,
            unification: if unify_filters.is_empty() {
                None
            } else {
                Some(Unification {
                    filters: unify_filters,
                    mask: unify_mask,
                })
            },
            filter,
            disambiguations,
            examples,
            id: Index::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a text engine whose graph ids `0..=n` all exist; that is what
    /// `Engine::to_graph_id` needs to accept a reference without clamping it to
    /// the last id. The regex itself is never compiled here.
    fn text_engine(n: usize) -> Engine {
        let id_to_idx: DefaultHashMap<GraphId, usize> = (0..=n).map(|i| (GraphId(i), i)).collect();
        Engine::Text(Box::new(Regex::new("".into())), id_to_idx)
    }

    fn describe(parts: &[SynthesizerPart]) -> Vec<String> {
        parts
            .iter()
            .map(|part| match part {
                SynthesizerPart::Match(m) => format!("<{}>", m.id.0),
                SynthesizerPart::Text(t) => t.clone(),
            })
            .collect()
    }

    #[test]
    fn two_digit_backreferences_are_a_single_reference() {
        // LanguageTool parses `\` followed by all consecutive digits as one
        // token reference, so `\13` is token 13, not `\1` plus a literal "3".
        let engine = text_engine(20);
        let parts = parse_synthesizer_text(r"\13 \14", &engine).unwrap();
        assert_eq!(describe(&parts), vec!["<13>", " ", "<14>"]);
    }

    #[test]
    fn single_digit_backreferences_still_work() {
        let engine = text_engine(3);
        let parts = parse_synthesizer_text(r"a \1 b \2", &engine).unwrap();
        assert_eq!(describe(&parts), vec!["a ", "<1>", " b ", "<2>"]);
    }

    #[test]
    fn text_without_backreferences_is_a_single_part() {
        let engine = text_engine(1);
        let parts = parse_synthesizer_text("plain text", &engine).unwrap();
        assert_eq!(describe(&parts), vec!["plain text"]);
    }

    fn part(min: usize, max: usize, visible: bool) -> Part {
        Part {
            atom: Atom::TrueAtom(TrueAtom {}),
            quantifier: Quantifier::new(min, max),
            visible,
            unify: None,
            next_exception: None,
        }
    }

    fn mins(parts: &[Part]) -> Vec<u32> {
        parts.iter().map(|part| part.quantifier.min).collect()
    }

    #[test]
    fn trailing_optional_token_must_match() {
        let mut parts = vec![part(1, 1, true), part(0, 1, true), part(0, 1, true)];
        require_last_token_match(&mut parts);
        assert_eq!(mins(&parts), vec![1, 0, 1]);
    }

    #[test]
    fn trailing_skip_does_not_count_as_the_last_token() {
        let mut parts = vec![part(1, 1, true), part(0, 3, true), part(0, 2, false)];
        require_last_token_match(&mut parts);
        assert_eq!(mins(&parts), vec![1, 1, 0]);
    }

    #[test]
    fn required_last_token_is_unchanged() {
        let mut parts = vec![part(0, 1, true), part(2, 5, true)];
        require_last_token_match(&mut parts);
        assert_eq!(mins(&parts), vec![0, 2]);
    }

    fn try_compiled_mins(xml: &str) -> Result<Vec<u32>, Error> {
        let pattern: structure::Pattern = serde_xml_rs::from_str(xml).unwrap();
        let mut info = BuildInfo::new(
            Arc::new(Tagger::default()),
            RegexCache::new(0),
            PathBuf::new(),
        );
        let mut pool = PartInterner::default();
        let (composition, _, _) = parse_pattern(pattern, &mut info, &mut pool)?;
        Ok(composition
            .parts
            .iter()
            .map(|id| pool.get(*id).quantifier.min)
            .collect())
    }

    fn compiled_mins(xml: &str) -> Vec<u32> {
        try_compiled_mins(xml).unwrap()
    }

    fn is_unimplemented(result: Result<Vec<u32>, Error>) -> bool {
        matches!(result, Err(Error::Unimplemented(_)))
    }

    #[test]
    fn or_group_takes_its_first_tokens_quantifier() {
        assert_eq!(
            compiled_mins(
                r#"<pattern><token><text text="“Please"/></token><or><token min="0"><text text="multiple"/></token><token><text text="—"/></token></or><token><text text="by…"/></token></pattern>"#
            ),
            vec![1, 0, 1]
        );
        assert_eq!(
            compiled_mins(
                "<pattern><and><token skip=\"2\"><text text=\"it’s\"/></token><token><text text=\"it’s\"/></token></and><token><text text=\"\u{2028}\"/></token></pattern>"
            ),
            vec![1, 0, 1]
        );
    }

    #[test]
    fn quantifier_on_a_later_parallel_token_is_rejected() {
        assert!(is_unimplemented(try_compiled_mins(
            r#"<pattern><token><text text="“Well"/></token><or><token><text text="it’s"/></token><token min="0"><text text="—"/></token></or></pattern>"#
        )));
    }

    fn next_exception_readings(atom: &Atom, next: bool, out: &mut Vec<bool>) {
        match atom {
            Atom::WordDataAtom(x) if next => out.push(x.first_reading),
            Atom::AndAtom(x) => x
                .atoms
                .iter()
                .for_each(|x| next_exception_readings(x, next, out)),
            Atom::OrAtom(x) => x
                .atoms
                .iter()
                .for_each(|x| next_exception_readings(x, next, out)),
            Atom::NotAtom(x) => next_exception_readings(&x.atom, next, out),
            Atom::OffsetAtom(x) => next_exception_readings(&x.atom, x.offset == 1, out),
            _ => {}
        }
    }

    fn compiled_next_exceptions(xml: &str) -> Vec<(Vec<bool>, Vec<bool>, Vec<bool>)> {
        let pattern: structure::Pattern = serde_xml_rs::from_str(xml).unwrap();
        let mut info = BuildInfo::new(
            Arc::new(Tagger::default()),
            RegexCache::new(0),
            PathBuf::new(),
        );
        let mut pool = PartInterner::default();
        let (composition, _, _) = parse_pattern(pattern, &mut info, &mut pool).unwrap();
        composition
            .parts
            .iter()
            .map(|id| {
                let part = pool.get(*id);
                let (mut shifted, mut all, mut first) = (Vec::new(), Vec::new(), Vec::new());
                next_exception_readings(&part.atom, false, &mut shifted);
                if let Some(exceptions) = &part.next_exception {
                    next_exception_readings(&exceptions.all, true, &mut all);
                    next_exception_readings(&exceptions.first, true, &mut first);
                }
                (shifted, all, first)
            })
            .collect()
    }

    #[test]
    fn scope_next_exceptions_are_kept_apart_from_the_token() {
        let none = (vec![], vec![], vec![]);
        let vbn = (vec![], vec![false], vec![true]);
        for (xml, expected) in [
            (
                r#"<pattern><token><text text="get"/></token><token spacebefore="no"><text text="’s"/><exception postag="VBN" scope="next"/></token></pattern>"#,
                vec![none.clone(), vbn.clone()],
            ),
            (
                r#"<pattern><token skip="-1"><text text="“It’s"/></token><token><text text="…"/><exception postag="VBN" scope="next"/></token></pattern>"#,
                vec![none.clone(), none.clone(), vbn.clone()],
            ),
            (
                r#"<pattern><token skip="2"><text text="“It’s"/><exception postag="VBN" scope="next"/></token><token><text text="—"/></token></pattern>"#,
                vec![vbn.clone(), none.clone(), none.clone()],
            ),
            (
                "<pattern><token skip=\"1\"><text text=\"—so\"/></token><marker><token skip=\"2\"><text text=\"\u{2028}it…\"/><exception postag=\"VBN\" scope=\"next\"/><exception postag=\"VBG\"/></token></marker></pattern>",
                vec![none.clone(), none.clone(), vbn, none],
            ),
        ] {
            assert_eq!(compiled_next_exceptions(xml), expected, "{}", xml);
        }
    }

    fn rule(xml: &str) -> structure::Rule {
        serde_xml_rs::from_str(&xml.replace(
            "</rule>",
            r#"<example><text text="“Maybe "/><marker><text text="I’Ve"/></marker><text text=" seen it…”"/></example></rule>"#,
        ))
        .unwrap()
    }

    #[test]
    fn literal_text_around_a_token_reference_is_its_affix() {
        let affix = |xml: &str| {
            let token: structure::Token = serde_xml_rs::from_str(xml).unwrap();
            reference_affix(token.parts.as_deref().unwrap())
        };

        assert_eq!(
            affix(r#"<token><match no="0"/><text text="s"/></token>"#),
            ("".to_string(), "s".to_string())
        );
        assert_eq!(
            affix(
                "<token><text text=\"“\"/><match no=\"1\"/><text text=\"’s—…\u{2028}\"/><exception postag=\"VBN\"/></token>"
            ),
            ("“".to_string(), "’s—…".to_string())
        );
        assert_eq!(
            affix(r#"<token><match no="2" case_conversion="startupper"/></token>"#),
            (String::new(), String::new())
        );
    }

    #[test]
    fn literal_text_around_a_reference_with_postag_or_regexp_is_unimplemented() {
        let pattern = |second: &str| {
            format!(
                "<pattern><token><text text=\"“active\"/></token>{}</pattern>",
                second
            )
        };

        assert!(is_unimplemented(try_compiled_mins(&pattern(
            r#"<token><match no="0" postag="NNS"/><text text="’s—…"/></token>"#
        ))));
        assert!(is_unimplemented(try_compiled_mins(&pattern(
            "<token regexp=\"yes\"><match no=\"0\"/><text text=\"s?\u{2028}x\"/></token>"
        ))));

        assert_eq!(
            compiled_mins(&pattern(
                r#"<token><match no="0"/><text text="’s—…"/></token>"#
            )),
            vec![1, 1]
        );
        assert_eq!(
            compiled_mins(&pattern(r#"<token><match no="0" postag="NNS"/></token>"#)),
            vec![1, 1]
        );
    }

    #[test]
    fn raw_pos_applies_only_to_rules_that_read_readings() {
        let text_only = r#"<pattern case_sensitive="yes" raw_pos="yes"><token><text text="“I"/></token><token spacebefore="no"><text text="’"/></token><token><text text="Ve…"/></token></pattern>"#;
        let message = r#"<message><text text="Use "/><suggestion><match no="3" case_conversion="alllower"/></suggestion><text text="—check it."/></message>"#;

        assert!(!reads_raw_readings(&rule(&format!(
            "<rule>{}{}</rule>",
            text_only, message
        ))));
        assert!(!reads_raw_readings(&rule(&format!(
            "<rule>{}{}</rule>",
            r#"<pattern><token postag="VB"><text text="—"/></token></pattern>"#, message
        ))));

        for pattern in [
            r#"<pattern raw_pos="yes"><token postag="VB"><text text="—"/></token></pattern>"#,
            r#"<pattern raw_pos="yes"><token inflected="yes"><text text="it’s"/></token></pattern>"#,
            r#"<pattern raw_pos="yes"><token chunk_re="B-NP"><text text="…"/></token></pattern>"#,
            r#"<pattern raw_pos="yes"><token><text text="“I"/><exception postag="PRP"/></token></pattern>"#,
            r#"<pattern raw_pos="yes"><or><token><text text="’"/></token><token postag="POS"><text text="'"/></token></or></pattern>"#,
            "<pattern raw_pos=\"yes\"><marker><token chunk=\"I-VP\"><text text=\"\u{2028}\"/></token></marker></pattern>",
        ] {
            assert!(
                reads_raw_readings(&rule(&format!("<rule>{}{}</rule>", pattern, message))),
                "{}",
                pattern
            );
        }

        for tail in [
            r#"<message><text text="Use "/><suggestion><match no="3" postag="VBP"/></suggestion><text text="—check it."/></message>"#,
            r#"<message><text text="Is "/><match no="1" postag="PRP"/><text text=" right…"/></message>"#,
            r#"<message><text text="Check it."/></message><suggestion><match no="3" postag="VBP"/></suggestion>"#,
            r#"<message><text text="Check it."/></message><filter class="org.languagetool.rules.en.AdverbFilter" args="adverb:1"/>"#,
        ] {
            assert!(
                reads_raw_readings(&rule(&format!("<rule>{}{}</rule>", text_only, tail))),
                "{}",
                tail
            );
        }
    }

    #[test]
    fn spell_number_postag_compiles_to_a_number_speller() {
        for postag_regexp in [false, true] {
            let replacer = parse_pos_replacer(" _spell_number_ ", postag_regexp, None, false)
                .expect("_spell_number_ compiles");
            assert!(replacer.spell_number);
            assert!(replacer.regex.is_none());
            assert!(replacer.determiner.is_none());
        }

        for postag in ["_spell_number_:feminine", "_spell_number_:Roman"] {
            assert!(
                matches!(
                    parse_pos_replacer(postag, false, None, false),
                    Err(Error::Unimplemented(_))
                ),
                "{}",
                postag
            );
        }

        assert!(
            !parse_pos_replacer("NNS", false, None, false)
                .unwrap()
                .spell_number
        );
    }

    #[test]
    fn parsed_pattern_requires_its_trailing_optional_token() {
        assert_eq!(
            compiled_mins(
                r#"<pattern><token><text text="“He’s"/></token><token min="0"><text text="—…"/></token></pattern>"#
            ),
            vec![1, 1]
        );
        assert_eq!(
            compiled_mins(
                "<pattern><marker><token><text text=\"it’s\"/></token></marker><token min=\"0\" max=\"-1\"><text text=\"\u{2028}\"/></token></pattern>"
            ),
            vec![1, 1]
        );
        assert_eq!(
            compiled_mins(
                r#"<pattern><token min="0"><text text="zqa"/></token><token><text text="zqb"/></token></pattern>"#
            ),
            vec![0, 1]
        );
    }
}
