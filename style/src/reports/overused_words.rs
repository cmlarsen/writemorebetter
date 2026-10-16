use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::echoes::{fold, listed};
use crate::document::compound_head;
use crate::error::StyleError;
use crate::{Document, Span, Token};

pub(crate) const REPORT: &str = "overusedWords";

pub const FILLER_WORDS: [&str; 9] = [
    "actually", "just", "quite", "really", "simply", "somehow", "suddenly", "that", "very",
];

const CONTENT_TAGS: [&str; 4] = ["NN", "VB", "JJ", "RB"];

pub const EXCLUDED_LEMMAS: [&str; 9] = [
    "around", "away", "back", "down", "like", "off", "out", "round", "up",
];

pub const SPELLING_VARIANTS: [(&str, &str); 248] = [
    ("acknowledgement", "acknowledgment"),
    ("aeon", "eon"),
    ("aeroplane", "airplane"),
    ("ageing", "aging"),
    ("agonise", "agonize"),
    ("agonising", "agonizing"),
    ("amphitheatre", "amphitheater"),
    ("anaesthesia", "anesthesia"),
    ("anaesthetic", "anesthetic"),
    ("analogue", "analog"),
    ("analyse", "analyze"),
    ("antagonise", "antagonize"),
    ("apologise", "apologize"),
    ("appal", "appall"),
    ("appetiser", "appetizer"),
    ("arbour", "arbor"),
    ("archaeological", "archeological"),
    ("archaeologist", "archeologist"),
    ("archaeology", "archeology"),
    ("ardour", "ardor"),
    ("armour", "armor"),
    ("armoured", "armored"),
    ("armoury", "armory"),
    ("artefact", "artifact"),
    ("authorisation", "authorization"),
    ("authorise", "authorize"),
    ("axe", "ax"),
    ("baptise", "baptize"),
    ("behaviour", "behavior"),
    ("behavioural", "behavioral"),
    ("brutalise", "brutalize"),
    ("calibre", "caliber"),
    ("candour", "candor"),
    ("capitalise", "capitalize"),
    ("catalogue", "catalog"),
    ("categorise", "categorize"),
    ("centimetre", "centimeter"),
    ("centre", "center"),
    ("characterisation", "characterization"),
    ("characterise", "characterize"),
    ("chimaera", "chimera"),
    ("civilisation", "civilization"),
    ("civilise", "civilize"),
    ("civilised", "civilized"),
    ("clamour", "clamor"),
    ("colonisation", "colonization"),
    ("colonise", "colonize"),
    ("colour", "color"),
    ("coloured", "colored"),
    ("colourful", "colorful"),
    ("colouring", "coloring"),
    ("colourless", "colorless"),
    ("cosy", "cozy"),
    ("criticise", "criticize"),
    ("crystallise", "crystallize"),
    ("customise", "customize"),
    ("defence", "defense"),
    ("demeanour", "demeanor"),
    ("demoralise", "demoralize"),
    ("diarrhoea", "diarrhea"),
    ("discolour", "discolor"),
    ("dishonour", "dishonor"),
    ("dishonourable", "dishonorable"),
    ("disorganised", "disorganized"),
    ("distil", "distill"),
    ("doughnut", "donut"),
    ("dramatise", "dramatize"),
    ("empathise", "empathize"),
    ("emphasise", "emphasize"),
    ("enamour", "enamor"),
    ("encyclopaedia", "encyclopedia"),
    ("endeavour", "endeavor"),
    ("energise", "energize"),
    ("enrol", "enroll"),
    ("enrolment", "enrollment"),
    ("enthral", "enthrall"),
    ("faeces", "feces"),
    ("familiarise", "familiarize"),
    ("fantasise", "fantasize"),
    ("favour", "favor"),
    ("favourable", "favorable"),
    ("favourably", "favorably"),
    ("favoured", "favored"),
    ("favourite", "favorite"),
    ("fertilise", "fertilize"),
    ("fertiliser", "fertilizer"),
    ("fervour", "fervor"),
    ("fibre", "fiber"),
    ("finalise", "finalize"),
    ("flavour", "flavor"),
    ("flavoured", "flavored"),
    ("foetal", "fetal"),
    ("foetid", "fetid"),
    ("foetus", "fetus"),
    ("fulfil", "fulfill"),
    ("fulfilment", "fulfillment"),
    ("galvanise", "galvanize"),
    ("generalisation", "generalization"),
    ("grey", "gray"),
    ("greyish", "grayish"),
    ("greyness", "grayness"),
    ("gruelling", "grueling"),
    ("haemorrhage", "hemorrhage"),
    ("harbour", "harbor"),
    ("harmonise", "harmonize"),
    ("honour", "honor"),
    ("honourable", "honorable"),
    ("honoured", "honored"),
    ("hospitalise", "hospitalize"),
    ("humour", "humor"),
    ("humourless", "humorless"),
    ("hypnotise", "hypnotize"),
    ("idealise", "idealize"),
    ("idolise", "idolize"),
    ("immobilise", "immobilize"),
    ("immortalise", "immortalize"),
    ("instal", "install"),
    ("instalment", "installment"),
    ("instil", "instill"),
    ("institutionalise", "institutionalize"),
    ("jeopardise", "jeopardize"),
    ("jeweller", "jeweler"),
    ("jewellery", "jewelry"),
    ("judgement", "judgment"),
    ("kilometre", "kilometer"),
    ("labour", "labor"),
    ("laboured", "labored"),
    ("labourer", "laborer"),
    ("leukaemia", "leukemia"),
    ("licence", "license"),
    ("litre", "liter"),
    ("lustre", "luster"),
    ("manoeuvre", "maneuver"),
    ("materialise", "materialize"),
    ("maximise", "maximize"),
    ("meagre", "meager"),
    ("mediaeval", "medieval"),
    ("memorise", "memorize"),
    ("mesmerise", "mesmerize"),
    ("metre", "meter"),
    ("millimetre", "millimeter"),
    ("minimise", "minimize"),
    ("misdemeanour", "misdemeanor"),
    ("mobilise", "mobilize"),
    ("modernise", "modernize"),
    ("monopolise", "monopolize"),
    ("motorise", "motorize"),
    ("mould", "mold"),
    ("mouldy", "moldy"),
    ("moustache", "mustache"),
    ("multicoloured", "multicolored"),
    ("neighbour", "neighbor"),
    ("neighbourhood", "neighborhood"),
    ("neighbouring", "neighboring"),
    ("neutralise", "neutralize"),
    ("odour", "odor"),
    ("offence", "offense"),
    ("organisation", "organization"),
    ("organise", "organize"),
    ("organised", "organized"),
    ("organiser", "organizer"),
    ("ostracise", "ostracize"),
    ("paediatric", "pediatric"),
    ("paediatrician", "pediatrician"),
    ("paedophile", "pedophile"),
    ("panelling", "paneling"),
    ("paralyse", "paralyze"),
    ("parlour", "parlor"),
    ("patronise", "patronize"),
    ("patronising", "patronizing"),
    ("personalise", "personalize"),
    ("plough", "plow"),
    ("practise", "practice"),
    ("pretence", "pretense"),
    ("primaeval", "primeval"),
    ("programme", "program"),
    ("publicise", "publicize"),
    ("pulverise", "pulverize"),
    ("pyjamas", "pajamas"),
    ("rancour", "rancor"),
    ("rationalisation", "rationalization"),
    ("rationalise", "rationalize"),
    ("realisation", "realization"),
    ("realise", "realize"),
    ("recognise", "recognize"),
    ("reconnoitre", "reconnoiter"),
    ("reorganisation", "reorganization"),
    ("reorganise", "reorganize"),
    ("reveller", "reveler"),
    ("revitalise", "revitalize"),
    ("rigour", "rigor"),
    ("rumour", "rumor"),
    ("sabre", "saber"),
    ("saviour", "savior"),
    ("savour", "savor"),
    ("savoury", "savory"),
    ("scandalise", "scandalize"),
    ("sceptic", "skeptic"),
    ("sceptical", "skeptical"),
    ("scepticism", "skepticism"),
    ("sceptre", "scepter"),
    ("scrutinise", "scrutinize"),
    ("sepulchre", "sepulcher"),
    ("skilful", "skillful"),
    ("smoulder", "smolder"),
    ("socialise", "socialize"),
    ("sombre", "somber"),
    ("specialisation", "specialization"),
    ("specialise", "specialize"),
    ("specialised", "specialized"),
    ("spectre", "specter"),
    ("splendour", "splendor"),
    ("stabilise", "stabilize"),
    ("sterilise", "sterilize"),
    ("succour", "succor"),
    ("sulphur", "sulfur"),
    ("summarise", "summarize"),
    ("symbolise", "symbolize"),
    ("sympathise", "sympathize"),
    ("sympathiser", "sympathizer"),
    ("synchronise", "synchronize"),
    ("synthesise", "synthesize"),
    ("tantalise", "tantalize"),
    ("tantalising", "tantalizing"),
    ("terrorise", "terrorize"),
    ("theatre", "theater"),
    ("theorise", "theorize"),
    ("traumatise", "traumatize"),
    ("traveller", "traveler"),
    ("tumour", "tumor"),
    ("unauthorised", "unauthorized"),
    ("uncivilised", "uncivilized"),
    ("unfavourable", "unfavorable"),
    ("unsavoury", "unsavory"),
    ("utilisation", "utilization"),
    ("utilise", "utilize"),
    ("valour", "valor"),
    ("vandalise", "vandalize"),
    ("vaporise", "vaporize"),
    ("vapour", "vapor"),
    ("verbalise", "verbalize"),
    ("victimise", "victimize"),
    ("vigour", "vigor"),
    ("visualise", "visualize"),
    ("watercolour", "watercolor"),
    ("whisky", "whiskey"),
    ("wilful", "willful"),
    ("womaniser", "womanizer"),
];

pub fn canonical_spelling(lemma: &str) -> &str {
    SPELLING_VARIANTS
        .binary_search_by(|(british, _)| (*british).cmp(lemma))
        .map_or(lemma, |index| SPELLING_VARIANTS[index].1)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct OverusedWordsOptions {
    #[serde(alias = "minimum_ratio")]
    pub minimum_ratio: f64,
    #[serde(alias = "minimum_count")]
    pub minimum_count: usize,
    #[serde(alias = "minimum_reference_per_million")]
    pub minimum_reference_per_million: f64,
    #[serde(alias = "include_nouns")]
    pub include_nouns: bool,
    #[serde(alias = "count_scale_words")]
    pub count_scale_words: usize,
}

impl Default for OverusedWordsOptions {
    fn default() -> Self {
        Self {
            minimum_ratio: 4.0,
            minimum_count: 5,
            minimum_reference_per_million: 10.0,
            include_nouns: false,
            count_scale_words: 10_000,
        }
    }
}

impl OverusedWordsOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_ratio(mut self, ratio: f64) -> Self {
        self.minimum_ratio = ratio;
        self
    }
    pub fn with_minimum_count(mut self, count: usize) -> Self {
        self.minimum_count = count;
        self
    }
    pub fn with_minimum_reference_per_million(mut self, rate: f64) -> Self {
        self.minimum_reference_per_million = rate;
        self
    }
    pub fn with_include_nouns(mut self, include: bool) -> Self {
        self.include_nouns = include;
        self
    }
    pub fn with_count_scale_words(mut self, words: usize) -> Self {
        self.count_scale_words = words;
        self
    }

    pub fn minimum_count_for(&self, words: usize) -> usize {
        let minimum = self.minimum_count.max(1);
        match self.count_scale_words {
            0 => minimum,
            scale => {
                minimum.max((minimum as f64 * (words as f64 / scale as f64).sqrt()).ceil() as usize)
            }
        }
    }

    fn validate(&self) -> Result<(), StyleError> {
        if !(self.minimum_ratio.is_finite() && self.minimum_ratio > 0.0) {
            return Err(StyleError::options(
                "minimumRatio must be a positive number",
            ));
        }
        if !(self.minimum_reference_per_million.is_finite()
            && self.minimum_reference_per_million >= 0.0)
        {
            return Err(StyleError::options(
                "minimumReferencePerMillion must be a finite, non-negative rate",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct OverusedWordsBaseline {
    pub lemmas: BTreeMap<String, f64>,
    pub fillers: BTreeMap<String, f64>,
}

impl OverusedWordsBaseline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_lemma(mut self, lemma: &str, per_million: f64) -> Self {
        self.lemmas.insert(lemma.to_owned(), per_million);
        self
    }

    pub fn with_filler(mut self, word: &str, per_million: f64) -> Self {
        self.fillers.insert(word.to_owned(), per_million);
        self
    }

    pub fn rate(&self, lemma: &str) -> Option<f64> {
        self.lemmas.get(canonical_spelling(lemma)).copied()
    }

    pub fn bundled() -> Option<&'static Self> {
        static BUNDLED: OnceLock<OverusedWordsBaseline> = OnceLock::new();
        Some(BUNDLED.get_or_init(|| {
            super::bundled_baseline(include_str!("../../data/baselines/overused-words.json"))
        }))
    }

    pub fn validate(&self) -> Result<(), StyleError> {
        for key in self.lemmas.keys() {
            if fold(key) != *key {
                return Err(StyleError::baseline(
                    REPORT,
                    format!(
                        "{key} must be lowercase with straight apostrophes, as {:?}",
                        fold(key)
                    ),
                ));
            }
            let canonical = canonical_spelling(key);
            if canonical != key {
                return Err(StyleError::baseline(
                    REPORT,
                    format!("{key} is a spelling variant of {canonical}; give one rate under {canonical}"),
                ));
            }
        }
        for (key, rate) in self.lemmas.iter().chain(&self.fillers) {
            if !(rate.is_finite() && *rate >= 0.0) {
                return Err(StyleError::baseline(
                    REPORT,
                    format!("{key} must have a finite, non-negative rate per million, got {rate}"),
                ));
            }
        }
        if let Some(word) = self
            .fillers
            .keys()
            .find(|w| !FILLER_WORDS.contains(&w.as_str()))
        {
            return Err(StyleError::baseline(
                REPORT,
                format!("{word} is not a filler word; use one of {FILLER_WORDS:?}"),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OverusedWord {
    pub lemma: String,
    pub count: usize,
    pub per_ten_thousand_words: f64,
    pub reference_per_ten_thousand_words: f64,
    pub ratio: f64,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OverusedWordsFiller {
    pub word: String,
    pub count: usize,
    pub per_ten_thousand_words: f64,
    pub reference_per_ten_thousand_words: Option<f64>,
    pub ratio: Option<f64>,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OverusedWordsReport {
    pub words: usize,
    pub overused: Vec<OverusedWord>,
    pub fillers: Vec<OverusedWordsFiller>,
}

struct Word<'a> {
    token: &'a Token,
    initial: bool,
    next: Option<&'a Token>,
}

fn filler(word: &Word) -> Option<&'static str> {
    let token = word.token;
    let text = fold(&token.text);
    let filler = FILLER_WORDS.iter().find(|filler| **filler == text)?;
    let qualifies = if *filler == "that" {
        token.chunks.iter().any(|chunk| chunk == "B-SBAR") && word.next.is_some_and(|t| t.is_word)
    } else {
        token.readings.iter().any(|r| r.pos.starts_with("RB"))
    };
    qualifies.then_some(*filler)
}

fn capitalized(token: &Token) -> bool {
    token.text.chars().next().is_some_and(char::is_uppercase)
}

fn proper(token: &Token, initial: bool, names: &HashSet<&str>) -> bool {
    capitalized(token)
        && (!initial
            || names.contains(token.text.as_str())
            || token
                .readings
                .iter()
                .any(|r| matches!(r.pos.as_str(), "NNP" | "NNPS")))
}

fn words_with_position(document: &Document) -> impl Iterator<Item = Word<'_>> {
    document.sentences().flat_map(|sentence| {
        let tokens: Vec<&Token> = sentence
            .tokens
            .iter()
            .filter(|t| !t.is_whitespace)
            .collect();
        let mut initial = true;
        let mut words = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            let opens = initial;
            initial =
                token.is_punctuation && (initial || token.text.contains(['“', '‘', '"', ':', '(']));
            if token.is_word && !token.is_skipped {
                words.push(Word {
                    token,
                    initial: opens,
                    next: tokens.get(index + 1).copied(),
                });
            }
        }
        words
    })
}

fn excluded(lemma: &str) -> bool {
    !lemma.chars().any(char::is_alphabetic)
        || lemma.contains(char::is_whitespace)
        || listed(lemma)
        || FILLER_WORDS.contains(&lemma)
        || EXCLUDED_LEMMAS.contains(&lemma)
}

fn candidates<'a>(
    readings: impl Iterator<Item = (&'a str, &'a str)>,
) -> Vec<(String, &'static str)> {
    readings
        .filter_map(|(lemma, pos)| {
            if matches!(pos, "NNP" | "NNPS") {
                return None;
            }
            let tag = CONTENT_TAGS.iter().find(|tag| pos.starts_with(**tag))?;
            Some((fold(lemma), *tag))
        })
        .filter(|(lemma, _)| !excluded(lemma))
        .collect()
}

fn content_lemma(
    word: &Word,
    document: &Document,
    baseline: &OverusedWordsBaseline,
) -> Option<(String, bool)> {
    let token = word.token;
    if token.readings.iter().any(|r| r.pos.starts_with("CD")) {
        return None;
    }
    let mut content = candidates(
        token
            .readings
            .iter()
            .map(|r| (r.lemma.as_str(), r.pos.as_str())),
    );
    if !content
        .iter()
        .any(|(lemma, _)| baseline.rate(lemma).is_some())
    {
        if let Some(readings) =
            compound_head(&token.text).and_then(|head| document.compound_heads.get(&head))
        {
            content = candidates(readings.iter().map(|(l, p)| (l.as_str(), p.as_str())));
        }
    }
    let chunk = |kind: &str| token.chunks.iter().any(|c| c.contains(kind));
    let family = |tag: &str| content.iter().any(|(_, t)| *t == tag);
    let content: Vec<(String, &str)> = if chunk("-VP") && family("VB") {
        content.into_iter().filter(|(_, t)| *t == "VB").collect()
    } else if chunk("E-NP") && family("NN") {
        content.into_iter().filter(|(_, t)| *t == "NN").collect()
    } else {
        content
    };
    let rate = |lemma: &str| baseline.rate(lemma).unwrap_or(0.0);
    let lemma = content
        .iter()
        .map(|(lemma, _)| lemma)
        .max_by(|a, b| rate(a).total_cmp(&rate(b)).then_with(|| b.cmp(a)))?
        .clone();
    let noun = content
        .iter()
        .filter(|(other, _)| *other == lemma)
        .all(|(_, tag)| *tag == "NN");
    Some((lemma, noun))
}

fn per_ten_thousand(count: usize, words: usize) -> f64 {
    if words == 0 {
        0.0
    } else {
        count as f64 * 10_000.0 / words as f64
    }
}

pub fn report(
    document: &Document,
    options: &OverusedWordsOptions,
    baseline: Option<&OverusedWordsBaseline>,
) -> Result<OverusedWordsReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    options.validate()?;
    if let Some(baseline) = baseline {
        baseline.validate()?;
    }
    let words = document.word_count;
    let mut lemmas: HashMap<String, (Vec<Span>, BTreeMap<String, usize>)> = HashMap::new();
    let mut fillers: BTreeMap<&str, Vec<Span>> = BTreeMap::new();
    let names: HashSet<&str> = words_with_position(document)
        .filter(|word| !word.initial && capitalized(word.token))
        .map(|word| word.token.text.as_str())
        .collect();
    for word in words_with_position(document) {
        let token = word.token;
        if let Some(filler) = filler(&word) {
            fillers.entry(filler).or_default().push(token.span.clone());
        } else if let Some(baseline) = baseline.filter(|_| !proper(token, word.initial, &names)) {
            match content_lemma(&word, document, baseline) {
                Some((lemma, noun)) if options.include_nouns || !noun => {
                    let key = canonical_spelling(&lemma).to_owned();
                    let (spans, spellings) = lemmas.entry(key).or_default();
                    spans.push(token.span.clone());
                    *spellings.entry(lemma).or_default() += 1;
                }
                _ => {}
            }
        }
    }
    let minimum_count = options.minimum_count_for(words);
    let mut overused: Vec<OverusedWord> = baseline
        .map(|baseline| {
            lemmas.into_iter().filter_map(|(key, (spans, spellings))| {
                let reference = *baseline.lemmas.get(&key)?;
                let lemma = spellings
                    .iter()
                    .max_by_key(|(spelling, count)| {
                        (**count, **spelling == key, Reverse(*spelling))
                    })
                    .map(|(spelling, _)| spelling.clone())?;
                if spans.len() < minimum_count
                    || reference <= 0.0
                    || reference < options.minimum_reference_per_million
                {
                    return None;
                }
                let rate = per_ten_thousand(spans.len(), words);
                let reference = reference / 100.0;
                let ratio = rate / reference;
                (ratio >= options.minimum_ratio).then_some(OverusedWord {
                    lemma,
                    count: spans.len(),
                    per_ten_thousand_words: rate,
                    reference_per_ten_thousand_words: reference,
                    ratio,
                    spans,
                })
            })
        })
        .into_iter()
        .flatten()
        .collect();
    overused.sort_by(|a, b| {
        b.ratio
            .total_cmp(&a.ratio)
            .then(b.count.cmp(&a.count))
            .then_with(|| a.lemma.cmp(&b.lemma))
    });
    let mut fillers: Vec<OverusedWordsFiller> = fillers
        .into_iter()
        .map(|(word, spans)| {
            let rate = per_ten_thousand(spans.len(), words);
            let reference = baseline
                .and_then(|b| b.fillers.get(word))
                .map(|per_million| per_million / 100.0);
            OverusedWordsFiller {
                word: word.to_owned(),
                count: spans.len(),
                per_ten_thousand_words: rate,
                reference_per_ten_thousand_words: reference,
                ratio: reference.filter(|r| *r > 0.0).map(|r| rate / r),
                spans,
            }
        })
        .collect();
    fillers.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word)));
    Ok(OverusedWordsReport {
        words,
        overused,
        fillers,
    })
}
