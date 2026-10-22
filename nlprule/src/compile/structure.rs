use fs_err::File;
use serde::Deserialize;
use std::io::BufReader;
use xml::reader::EventReader;

mod preprocess {
    use std::{borrow::Cow, str::FromStr};

    use lazy_static::lazy_static;
    use xml::{attribute::OwnedAttribute, reader::EventReader};
    use xml::{name::OwnedName, writer::EmitterConfig};

    use super::Category;

    pub fn sanitize(input: impl std::io::Read, whitespace_sensitive_tags: &[&str]) -> String {
        let mut sanitized = Vec::new();

        let mut writer = EmitterConfig::new()
            .perform_indent(true)
            .create_writer(&mut sanitized);

        let parser = EventReader::new(input);

        let events = parser
            .into_iter()
            .map(|x| x.expect("error reading XML"))
            .filter(|x| {
                // processing instructions break the writer and are useless to us
                !matches!(x, xml::reader::XmlEvent::ProcessingInstruction { .. })
            })
            .collect::<Vec<_>>();

        let mut out_events: Vec<xml::writer::XmlEvent> = Vec::new();
        let mut parents: Vec<(&OwnedName, &Vec<OwnedAttribute>)> = Vec::new();

        for event in &events {
            match event {
                xml::reader::XmlEvent::StartElement {
                    name,
                    attributes,
                    namespace,
                } => {
                    let unify_index = parents
                        .iter()
                        .position(|(name, _)| name.local_name.as_str() == "unify");
                    let ignore_index = parents
                        .iter()
                        .position(|(name, _)| name.local_name.as_str() == "unify-ignore");

                    parents.push((name, attributes));

                    let unify = unify_index.map_or(false, |i| {
                        ignore_index.map_or(true, |ignore_i| i > ignore_i)
                    });

                    if unify && name.local_name == "token" {
                        lazy_static! {
                            static ref UNIFY_ATTRIBUTE: OwnedAttribute =
                                OwnedAttribute::new(OwnedName::from_str("unify").unwrap(), "yes",);
                        }
                        lazy_static! {
                            static ref UNIFY_NEGATE_ATTRIBUTE: OwnedAttribute = OwnedAttribute::new(
                                OwnedName::from_str("unify").unwrap(),
                                "negate",
                            );
                        }

                        // we push to parents after index is computed but it doesn't matter because the index stays the same
                        let negate = parents[unify_index.unwrap()]
                            .1
                            .iter()
                            .any(|x| x.name.local_name == "negate" && x.value == "yes");

                        out_events.push(xml::writer::XmlEvent::StartElement {
                            name: name.borrow(),
                            attributes: attributes
                                .iter()
                                .chain(if negate {
                                    vec![&*UNIFY_NEGATE_ATTRIBUTE]
                                } else {
                                    vec![&*UNIFY_ATTRIBUTE]
                                })
                                .map(|a| a.borrow())
                                .collect(),
                            namespace: Cow::Borrowed(namespace),
                        });
                        continue;
                    }

                    if ["unify", "unify-ignore"].contains(&name.local_name.as_str()) {
                        continue;
                    }
                }
                xml::reader::XmlEvent::EndElement { name, .. } => {
                    parents.pop();

                    if ["unify", "unify-ignore"].contains(&name.local_name.as_str()) {
                        continue;
                    }
                }
                xml::reader::XmlEvent::Characters(chars) => {
                    out_events.push(
                        xml::writer::XmlEvent::start_element("text")
                            .attr("text", chars)
                            .into(),
                    );
                    out_events.push(xml::writer::XmlEvent::end_element().into());
                    continue;
                }
                xml::reader::XmlEvent::Whitespace(whitespace) => {
                    let whitespace_sensitive = parents.iter().any(|(name, _)| {
                        whitespace_sensitive_tags.contains(&name.local_name.as_str())
                    });
                    if whitespace_sensitive {
                        out_events.push(
                            xml::writer::XmlEvent::start_element("text")
                                .attr("text", whitespace)
                                .into(),
                        );
                        out_events.push(xml::writer::XmlEvent::end_element().into());
                        continue;
                    }
                }
                _ => {}
            }

            if let Some(writer_event) = event.as_writer_event() {
                out_events.push(writer_event);
            }
        }

        for event in out_events {
            writer.write(event).expect("error writing to output XML");
        }

        std::str::from_utf8(&sanitized)
            .expect("invalid UTF-8")
            .to_string()
    }

    pub fn extract_rules(mut xml: impl std::io::Read) -> Vec<(String, Option<Category>)> {
        let mut string = String::new();
        xml.read_to_string(&mut string)
            .expect("error writing to string.");

        let document = roxmltree::Document::parse(&string).expect("error parsing XML");

        document
            .descendants()
            .filter(|x| {
                let name = x.tag_name().name();

                name == "unification"
                    || name == "rulegroup"
                    || (name == "rule"
                        && x.parent_element()
                            .expect("must have parent")
                            .tag_name()
                            .name()
                            != "rulegroup")
            })
            .map(|x| {
                let xml = string[x.range()].to_string();
                let parent = x.parent_element().expect("must have parent");

                let category = if parent.tag_name().name() == "category" {
                    Some(Category {
                        id: parent.attribute("id").unwrap().to_owned(),
                        name: parent.attribute("name").unwrap().to_owned(),
                        kind: parent.attribute("type").map(|x| x.to_owned()),
                        default: parent.attribute("default").map(|x| x.to_owned()),
                    })
                } else {
                    None
                };

                (xml, category)
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub default: Option<String>,
    pub tags: Option<String>,
    pub min_prev_matches: Option<String>,
    pub distance_tokens: Option<String>,
    pub n: usize,
}

#[derive(Debug, Clone)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub kind: Option<String>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct XmlString {
    pub text: String,
}

impl std::ops::Deref for XmlString {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

impl From<XmlString> for String {
    fn from(data: XmlString) -> String {
        data.text
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct XmlText {
    pub text: XmlString,
}

impl std::ops::Deref for XmlText {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

impl From<XmlText> for String {
    fn from(data: XmlText) -> String {
        data.text.into()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Match {
    pub no: String,
    pub postag: Option<String>,
    #[serde(rename = "postag_regexp")]
    pub postag_regex: Option<String>,
    pub postag_replace: Option<String>,
    /// `setpos="yes"` would write the matched POS back onto the token. It only
    /// occurs together with `postag` in the English rules, which is already
    /// rejected as unimplemented, so it is parsed and otherwise ignored.
    pub setpos: Option<String>,
    pub text: Option<XmlString>,
    pub include_skipped: Option<String>,
    pub case_conversion: Option<String>,
    pub regexp_match: Option<String>,
    pub regexp_replace: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum SuggestionPart {
    Match(Match),
    Text(XmlString),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Suggestion {
    pub suppress_misspelled: Option<String>,
    /// An empty `<suggestion/>` is valid in LanguageTool and means "delete the
    /// match", so the parts list may be absent.
    #[serde(rename = "$value", default)]
    pub parts: Vec<SuggestionPart>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum MessagePart {
    Suggestion(Suggestion),
    Text(XmlString),
    Match(Match),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    #[serde(rename = "$value")]
    pub parts: Vec<MessagePart>,
    pub suppress_misspelled: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExampleMarker {
    /// LanguageTool allows an empty `<marker></marker>` in examples to mark a
    /// zero-width span (usually a deletion); the text is then absent.
    pub text: Option<XmlString>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum ExamplePart {
    Marker(ExampleMarker),
    Text(XmlString),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Example {
    pub reason: Option<String>,
    pub correction: Option<String>,
    #[serde(rename = "$value")]
    pub parts: Vec<ExamplePart>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exception {
    pub case_sensitive: Option<String>,
    pub inflected: Option<String>,
    pub postag: Option<String>,
    pub postag_regexp: Option<String>,
    pub chunk: Option<String>,
    pub chunk_re: Option<String>,
    pub regexp: Option<String>,
    pub spacebefore: Option<String>,
    pub negate: Option<String>,
    pub negate_pos: Option<String>,
    pub scope: Option<String>,
    pub text: Option<XmlString>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
#[allow(clippy::large_enum_variant)]
pub enum TokenPart {
    Text(XmlString),
    Exception(Exception),
    #[serde(rename = "match")]
    Sub(Sub),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sub {
    pub no: String,
    /// `<match .../>` inside a `<token>` can carry the full attribute set of a
    /// match reference (see LanguageTool's `pattern.xsd`). `no`,
    /// `case_conversion` and the `postag` synthesis are implemented;
    /// `regexp_match`/`regexp_replace` and `include_skipped` are rejected
    /// explicitly in `parse_token`.
    pub postag: Option<String>,
    pub postag_regexp: Option<String>,
    pub postag_replace: Option<String>,
    pub case_conversion: Option<String>,
    pub regexp_match: Option<String>,
    pub regexp_replace: Option<String>,
    pub include_skipped: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Token {
    pub min: Option<String>,
    pub max: Option<String>,
    pub skip: Option<String>,
    pub unify: Option<String>,
    pub case_sensitive: Option<String>,
    pub inflected: Option<String>,
    pub postag: Option<String>,
    pub postag_regexp: Option<String>,
    pub chunk: Option<String>,
    pub chunk_re: Option<String>,
    pub regexp: Option<String>,
    pub spacebefore: Option<String>,
    pub negate: Option<String>,
    pub negate_pos: Option<String>,
    #[serde(rename = "$value")]
    pub parts: Option<Vec<TokenPart>>,
}

// NB: needlessly verbose, would be nicer with #[serde(flatten)] but blocked by https://github.com/RReverser/serde-xml-rs/issues/83
pub trait MatchAttributes {
    fn case_sensitive(&self) -> &Option<String>;
    fn inflected(&self) -> &Option<String>;
    fn postag(&self) -> &Option<String>;
    fn postag_regexp(&self) -> &Option<String>;
    fn chunk(&self) -> &Option<String>;
    fn chunk_re(&self) -> &Option<String>;
    fn regexp(&self) -> &Option<String>;
    fn spacebefore(&self) -> &Option<String>;
    fn negate(&self) -> &Option<String>;
    fn negate_pos(&self) -> &Option<String>;
}

macro_rules! impl_match_attributes {
    ($e:ty) => {
        impl MatchAttributes for $e {
            fn case_sensitive(&self) -> &Option<String> {
                &self.case_sensitive
            }

            fn inflected(&self) -> &Option<String> {
                &self.inflected
            }

            fn postag(&self) -> &Option<String> {
                &self.postag
            }

            fn postag_regexp(&self) -> &Option<String> {
                &self.postag_regexp
            }

            fn chunk(&self) -> &Option<String> {
                &self.chunk
            }

            fn chunk_re(&self) -> &Option<String> {
                &self.chunk_re
            }

            fn regexp(&self) -> &Option<String> {
                &self.regexp
            }

            fn spacebefore(&self) -> &Option<String> {
                &self.spacebefore
            }

            fn negate(&self) -> &Option<String> {
                &self.negate
            }

            fn negate_pos(&self) -> &Option<String> {
                &self.negate_pos
            }
        }
    };
}

impl_match_attributes!(&Exception);
impl_match_attributes!(&Token);

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenVector {
    #[serde(rename = "token")]
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
pub enum TokenCombination {
    Token(Token),
    Or(TokenVector),
    And(TokenVector),
    Feature(Feature),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternMarker {
    #[serde(rename = "$value")]
    pub tokens: Vec<TokenCombination>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhraseRef {
    pub idref: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
pub enum PatternPart {
    Token(Token),
    Marker(PatternMarker),
    Or(TokenVector),
    And(TokenVector),
    Feature(Feature),
    /// LanguageTool allows `<example>` elements inside `<antipattern>` as
    /// documentation (rules.xsd: "this element should appear at the end").
    /// They do not take part in matching, so the value is parsed and ignored.
    #[allow(dead_code)]
    Example(Example),
    /// A reference to a `<phrase id="…">` defined in a `<phrases>` block.
    /// nlprule does not expand phrases, so this is rejected explicitly.
    Phraseref(PhraseRef),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pattern {
    pub case_sensitive: Option<String>,
    /// `raw_pos="yes"` makes the token's POS tags refer to the tagger output
    /// *before* disambiguation. A rule that reads POS tags, lemmas or chunks
    /// then matches against [`Sentence::raw`][crate::types::Sentence::raw];
    /// a text-only pattern matches the same either way.
    pub raw_pos: Option<String>,
    #[serde(rename = "$value")]
    pub parts: Vec<PatternPart>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regex {
    pub text: XmlString,
    pub case_sensitive: Option<String>,
    /// `type="exact"` keeps the regexp as written; the default `smart` replaces
    /// literal spaces outside character classes with a whitespace matcher (see
    /// `replace_spaces_in_regex`).
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub mark: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub pattern: Option<Pattern>,
    #[serde(rename = "regexp")]
    pub regex: Option<Regex>,
    #[serde(rename = "antipattern")]
    pub antipatterns: Option<Vec<Pattern>>,
    pub message: Message,
    #[serde(rename = "suggestion")]
    pub suggestions: Option<Vec<Suggestion>>,
    #[serde(rename = "example", default)]
    pub examples: Vec<Example>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub short: Option<XmlText>,
    pub url: Option<XmlText>,
    pub default: Option<String>,
    /// ITS 2.0 localization quality issue type (`type="grammar"`, …). A label
    /// for consumers; it has no effect on matching or suggestions, so it is
    /// parsed and ignored.
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub kind: Option<String>,
    /// `tags="picky"` marks the rule as opt-in (LanguageTool's Level.PICKY).
    /// The value may be a space separated list.
    pub tags: Option<String>,
    /// LanguageTool's repetition experiment: fire only after `min_prev_matches`
    /// earlier matches within `distance_tokens` tokens. The state spans one
    /// [`Rules::suggest`](crate::Rules::suggest) call.
    pub min_prev_matches: Option<String>,
    pub distance_tokens: Option<String>,
    pub filter: Option<Filter>,
    #[serde(rename = "__unused_unifications")]
    pub unifications: Option<Vec<Unification>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleGroup {
    pub id: String,
    #[serde(rename = "antipattern")]
    pub antipatterns: Option<Vec<Pattern>>,
    pub default: Option<String>,
    pub name: String,
    pub short: Option<XmlText>,
    pub url: Option<XmlText>,
    /// See [`Rule::kind`] — parsed and ignored.
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub kind: Option<String>,
    /// See [`Rule::tags`].
    pub tags: Option<String>,
    /// See [`Rule::min_prev_matches`].
    pub min_prev_matches: Option<String>,
    pub distance_tokens: Option<String>,
    #[serde(rename = "rule")]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
pub enum RuleContainer {
    Rule(Rule),
    RuleGroup(RuleGroup),
    Unification(Unification),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisambiguationExample {
    #[serde(rename = "type")]
    pub kind: String,
    pub inputform: Option<String>,
    pub outputform: Option<String>,
    #[serde(rename = "$value")]
    pub parts: Vec<ExamplePart>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordData {
    pub pos: Option<String>,
    pub text: Option<XmlString>,
    pub lemma: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub args: String,
    pub class: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisambiguationMatch {
    pub no: usize,
    pub postag: Option<String>,
    pub postag_regexp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DisambiguationPart {
    #[serde(rename = "wd")]
    WordData(WordData),
    #[serde(rename = "match")]
    Match(DisambiguationMatch),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Disambiguation {
    pub postag: Option<String>,
    pub action: Option<String>,
    #[serde(rename = "$value")]
    pub word_datas: Option<Vec<DisambiguationPart>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisambiguationRule {
    pub pattern: Pattern,
    #[serde(rename = "antipattern")]
    pub antipatterns: Option<Vec<Pattern>>,
    #[serde(rename = "suggestion")]
    pub suggestions: Option<Vec<Suggestion>>,
    pub disambig: Disambiguation,
    #[serde(rename = "example")]
    pub examples: Option<Vec<DisambiguationExample>>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub filter: Option<Filter>,
    #[serde(rename = "__unused_unifications")]
    pub unifications: Option<Vec<Unification>>,
    pub default: Option<String>,
}

impl DisambiguationRule {
    fn inherit_antipattern_case(&mut self, preceding: Option<&[Pattern]>) {
        let last = self
            .antipatterns
            .as_deref()
            .and_then(<[_]>::last)
            .or_else(|| preceding.and_then(<[_]>::last));
        if last.is_some_and(|pattern| pattern.case_sensitive.as_deref() == Some("yes")) {
            self.pattern.case_sensitive = Some("yes".to_owned());
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisambiguationRuleGroup {
    pub id: String,
    #[serde(rename = "antipattern")]
    pub antipatterns: Option<Vec<Pattern>>,
    pub name: String,
    #[serde(rename = "rule")]
    pub rules: Vec<DisambiguationRule>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquivalenceToken {
    pub postag: String,
    pub postag_regexp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Equivalence {
    pub token: EquivalenceToken,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unification {
    #[serde(rename = "equivalence")]
    pub equivalences: Vec<Equivalence>,
    pub feature: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
pub enum DisambiguationRuleContainer {
    Rule(DisambiguationRule),
    RuleGroup(DisambiguationRuleGroup),
    Unification(Unification),
}

macro_rules! flatten_group {
    ($rulegroup:expr, $category:expr, $tags:expr, $min_prev_matches:expr, $distance_tokens:expr) => {{
        let group_antipatterns = if let Some(antipatterns) = $rulegroup.antipatterns {
            antipatterns
        } else {
            Vec::new()
        };

        let group = Group {
            id: $rulegroup.id,
            default: $rulegroup.default,
            tags: $tags,
            min_prev_matches: $min_prev_matches,
            distance_tokens: $distance_tokens,
            name: $rulegroup.name,
            n: 0,
        };

        $rulegroup
            .rules
            .into_iter()
            .enumerate()
            .map(|(i, mut rule)| {
                if let Some(antipatterns) = &mut rule.antipatterns {
                    antipatterns.extend(group_antipatterns.clone());
                } else {
                    rule.antipatterns = Some(group_antipatterns.clone());
                }

                let mut group = group.clone();
                group.n = i;
                (rule, Some(group), $category.clone())
            })
            .collect::<Vec<_>>()
    }};
}

const GRAMMAR_WHITESPACE_SENSITIVE_TAGS: &[&str] = &["suggestion", "example"];

type GrammarRuleReading = (Rule, Option<Group>, Option<Category>);
type DisambiguationRuleReading = (DisambiguationRule, Option<Group>, Option<Category>);

pub fn read_rules<P: AsRef<std::path::Path>>(
    path: P,
) -> Vec<Result<GrammarRuleReading, serde_xml_rs::Error>> {
    let file = File::open(path.as_ref()).unwrap();
    let file = BufReader::new(file);

    let sanitized = preprocess::sanitize(file, GRAMMAR_WHITESPACE_SENSITIVE_TAGS);
    let rules = preprocess::extract_rules(sanitized.as_bytes());

    let mut unifications = Vec::new();

    let rules: Vec<_> = rules
        .into_iter()
        .map(|(xml, category)| {
            let mut out = Vec::new();

            let deseralized = RuleContainer::deserialize(&mut serde_xml_rs::Deserializer::new(
                EventReader::new(xml.as_bytes()),
            ));

            out.extend(match deseralized {
                Ok(rule_container) => match rule_container {
                    RuleContainer::Rule(rule) => {
                        vec![Ok((rule, None, category))]
                    }
                    RuleContainer::RuleGroup(rule_group) => {
                        let tags = rule_group.tags.clone();
                        let min_prev_matches = rule_group.min_prev_matches.clone();
                        let distance_tokens = rule_group.distance_tokens.clone();
                        flatten_group!(
                            rule_group,
                            category,
                            tags,
                            min_prev_matches,
                            distance_tokens
                        )
                        .into_iter()
                        .map(Ok)
                        .collect()
                    }
                    RuleContainer::Unification(unification) => {
                        unifications.push(unification);

                        vec![]
                    }
                },
                Err(err) => vec![Err(err)],
            });
            out
        })
        .flatten()
        .collect();

    rules
        .into_iter()
        .map(|result| match result {
            Ok(mut x) => {
                x.0.unifications = Some(unifications.clone());

                Ok(x)
            }
            Err(x) => Err(x),
        })
        .collect()
}

pub fn read_disambiguation_rules<P: AsRef<std::path::Path>>(
    path: P,
) -> Vec<Result<DisambiguationRuleReading, serde_xml_rs::Error>> {
    let file = File::open(path.as_ref()).unwrap();
    let file = BufReader::new(file);

    let sanitized = preprocess::sanitize(file, &[]);
    let rules = preprocess::extract_rules(sanitized.as_bytes());

    let mut unifications = Vec::new();

    let rules: Vec<_> = rules
        .into_iter()
        .map(|(xml, _)| {
            let mut out = Vec::new();

            let deseralized = DisambiguationRuleContainer::deserialize(
                &mut serde_xml_rs::Deserializer::new(EventReader::new(xml.as_bytes())),
            );

            let category: Option<Category> = None;

            out.extend(match deseralized {
                Ok(rule_container) => match rule_container {
                    DisambiguationRuleContainer::Rule(mut rule) => {
                        rule.inherit_antipattern_case(None);
                        vec![Ok((rule, None, category))]
                    }
                    DisambiguationRuleContainer::RuleGroup(mut rule_group) => {
                        for (i, rule) in rule_group.rules.iter_mut().enumerate() {
                            let preceding = rule_group.antipatterns.as_deref().filter(|_| i == 0);
                            rule.inherit_antipattern_case(preceding);
                        }
                        flatten_group!(rule_group, category, None, None, None)
                            .into_iter()
                            .map(Ok)
                            .collect()
                    }
                    DisambiguationRuleContainer::Unification(unification) => {
                        unifications.push(unification);

                        vec![]
                    }
                },
                Err(err) => vec![Err(err)],
            });
            out
        })
        .flatten()
        .collect();

    rules
        .into_iter()
        .map(|result| match result {
            Ok(mut x) => {
                x.0.unifications = Some(unifications.clone());

                Ok(x)
            }
            Err(x) => Err(x),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_texts(xml: &str) -> Vec<String> {
        let sanitized = preprocess::sanitize(xml.as_bytes(), GRAMMAR_WHITESPACE_SENSITIVE_TAGS);
        let document = roxmltree::Document::parse(&sanitized).unwrap();
        document
            .descendants()
            .filter(|x| x.tag_name().name() == "text")
            .map(|x| x.attribute("text").unwrap().to_owned())
            .collect()
    }

    #[test]
    fn whitespace_only_example_markers_are_kept() {
        assert_eq!(
            example_texts("<example>This is a<marker>  </marker>sentence.</example>"),
            ["This is a", "  ", "sentence."]
        );
        assert_eq!(
            example_texts(
                "<example>\u{201c}It\u{2019}s\u{2026}<marker>\u{a0} </marker>\u{2014}\u{2028}</example>"
            ),
            ["\u{201c}It\u{2019}s\u{2026}", "\u{a0} ", "\u{2014}\u{2028}"]
        );
    }
}
