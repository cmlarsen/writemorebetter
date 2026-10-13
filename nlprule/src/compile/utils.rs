use crate::{rules::RulesLangOptions, spell::SpellLangOptions, tokenizer::TokenizerLangOptions};
use crate::{tokenizer::tag::TaggerLangOptions, types::*};
use lazy_static::lazy_static;

lazy_static! {
    static ref TOKENIZER_LANG_OPTIONS: DefaultHashMap<String, TokenizerLangOptions> = {
        serde_json::from_slice(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/",
            "tokenizer_configs.json"
        )))
        .expect("tokenizer configs must be valid JSON")
    };
}

lazy_static! {
    static ref RULES_LANG_OPTIONS: DefaultHashMap<String, RulesLangOptions> = {
        serde_json::from_slice(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/",
            "rules_configs.json"
        )))
        .expect("rules configs must be valid JSON")
    };
}

lazy_static! {
    static ref TAGGER_LANG_OPTIONS: DefaultHashMap<String, TaggerLangOptions> = {
        serde_json::from_slice(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/",
            "tagger_configs.json"
        )))
        .expect("tagger configs must be valid JSON")
    };
}

lazy_static! {
    static ref SPELLCHECKER_LANG_OPTIONS: DefaultHashMap<String, SpellLangOptions> = {
        serde_json::from_slice(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/",
            "spellchecker_configs.json"
        )))
        .expect("tagger configs must be valid JSON")
    };
}

/// Gets the tokenizer language options for the language code
pub(crate) fn tokenizer_lang_options(lang_code: &str) -> Option<TokenizerLangOptions> {
    TOKENIZER_LANG_OPTIONS.get(lang_code).cloned()
}

/// Gets the rules language options for the language code
pub(crate) fn rules_lang_options(lang_code: &str) -> Option<RulesLangOptions> {
    RULES_LANG_OPTIONS.get(lang_code).cloned()
}

/// Gets the tagger language options for the language code
pub(crate) fn tagger_lang_options(lang_code: &str) -> Option<TaggerLangOptions> {
    TAGGER_LANG_OPTIONS.get(lang_code).cloned()
}

/// Gets the spellchecker language options for the language code
pub(crate) fn spellchecker_lang_options(lang_code: &str) -> Option<SpellLangOptions> {
    SPELLCHECKER_LANG_OPTIONS.get(lang_code).cloned()
}

pub(crate) use regex::{from_java_regex, java_char_class_ranges, replace_spaces_in_regex};

mod regex {
    use std::collections::HashMap;

    use lazy_static::lazy_static;
    use regex_syntax::ast::{
        print::Printer, Alternation, Assertion, AssertionKind, Ast, CaptureName, Class, ClassAscii,
        ClassAsciiKind, ClassBracketed, ClassPerlKind, ClassSet, ClassSetItem, ClassSetUnion,
        Concat, ErrorKind, Flag, Flags, FlagsItemKind, Group, GroupKind, Literal, LiteralKind,
        Position, Repetition, RepetitionKind, RepetitionOp, RepetitionRange, Span,
    };
    use regex_syntax::hir::{self, HirKind};

    use crate::compile::Error;

    fn zero_span() -> Span {
        Span {
            start: Position::new(0, 0, 0),
            end: Position::new(0, 0, 0),
        }
    }

    fn to_lower_literal(literal: &Literal) -> Option<Literal> {
        let chars: Vec<_> = literal.c.to_lowercase().collect();

        match &chars[..] {
            [c] => {
                let mut out = literal.clone();
                out.c = *c;

                Some(out)
            }
            _ => None,
        }
    }

    fn to_upper_literal(literal: &Literal) -> Option<Literal> {
        let chars: Vec<_> = literal.c.to_uppercase().collect();

        match &chars[..] {
            [c] => {
                let mut out = literal.clone();
                out.c = *c;

                Some(out)
            }
            _ => None,
        }
    }

    fn literal_to_union(literal: &Literal) -> Option<ClassSetUnion> {
        let lower = to_lower_literal(literal)?;
        let upper = to_upper_literal(literal)?;

        Some(ClassSetUnion {
            span: zero_span(),
            items: if lower == upper {
                vec![ClassSetItem::Literal(lower)]
            } else {
                vec![ClassSetItem::Literal(lower), ClassSetItem::Literal(upper)]
            },
        })
    }

    /// Returns a case insensitive version of the given `ClassSetItem`.
    fn to_i_item(root: &ClassSetItem) -> ClassSetItem {
        match root {
            ClassSetItem::Literal(literal) => {
                if let Some(union) = literal_to_union(literal) {
                    ClassSetItem::Union(union)
                } else {
                    ClassSetItem::Literal(literal.clone())
                }
            }
            ClassSetItem::Range(range) => {
                assert!(range.is_valid()); // would have returned an error otherwise

                match (to_upper_literal(&range.start), to_lower_literal(&range.end)) {
                    (Some(start), Some(end)) => {
                        let mut out = range.clone();
                        out.start = start;
                        out.end = end;
                        ClassSetItem::Range(out)
                    }
                    _ => ClassSetItem::Range(range.clone()),
                }
            }
            ClassSetItem::Union(union) => {
                let mut union = union.clone();

                union.items = union.items.iter().map(to_i_item).collect();
                ClassSetItem::Union(union)
            }
            ClassSetItem::Bracketed(bracketed) => {
                let mut bracketed = bracketed.clone();
                bracketed.kind = to_i_class_set(&bracketed.kind);
                ClassSetItem::Bracketed(bracketed)
            }
            ClassSetItem::Empty(_)
            | ClassSetItem::Ascii(_)
            | ClassSetItem::Unicode(_)
            | ClassSetItem::Perl(_) => root.clone(),
        }
    }

    /// Returns a case insensitive version of the given `ClassSet`.
    fn to_i_class_set(root: &ClassSet) -> ClassSet {
        match root {
            ClassSet::Item(item) => ClassSet::Item(to_i_item(item)),
            ClassSet::BinaryOp(op) => {
                let mut op = op.clone();
                op.rhs = to_i_class_set(&op.rhs).into();
                op.lhs = to_i_class_set(&op.lhs).into();
                ClassSet::BinaryOp(op)
            }
        }
    }

    /// "Fixes" the AST by:
    /// * removing case insensitive and unicode flags since their behavior is not consistent
    ///     e. g. (?i)\p{Lu} is equivalent to \p{L} in `fancy_regex` and to \p{Lu} in Java / Oniguruma
    /// * manually making the case insensitive parts case insensitive instead. This is done by:
    ///     * for each literal which has a single-char uppercase and lowercase variant, replace the literal
    ///         by a set of the uppercase and lowercase variant of the union e. g. "a" to "[aA]".
    ///     * uppercasing range start and lowercasing range end e.g. [A-Z] to [A-z].
    ///         This is also only done for chars with single-char upper- / lowercase variants.
    /// * disallowing nested quantifiers
    fn fix_ast(root: &Ast, mut case_sensitive: bool) -> Result<(Ast, bool), Error> {
        let ast = match root {
            Ast::Alternation(alternation) => {
                let mut alternation = alternation.clone();

                alternation.asts = alternation
                    .asts
                    .iter()
                    .map(|x| {
                        let (ast, i) = fix_ast(x, case_sensitive)?;
                        case_sensitive = i;
                        Ok(ast)
                    })
                    .collect::<Result<_, Error>>()?;
                Ast::Alternation(alternation)
            }
            Ast::Concat(concat) => {
                let mut concat = concat.clone();

                concat.asts = concat
                    .asts
                    .iter()
                    .map(|x| {
                        let (ast, i) = fix_ast(x, case_sensitive)?;
                        case_sensitive = i;
                        Ok(ast)
                    })
                    .collect::<Result<_, Error>>()?;
                Ast::Concat(concat)
            }
            Ast::Class(class) => match &class {
                Class::Bracketed(bracketed) => {
                    let mut bracketed = bracketed.clone();
                    if !case_sensitive {
                        bracketed.kind = to_i_class_set(&bracketed.kind);
                    }
                    Ast::Class(Class::Bracketed(bracketed))
                }
                Class::Perl(perl) => {
                    if matches!(perl.kind, ClassPerlKind::Space) {
                        Ast::Literal(Literal {
                            span: zero_span(),
                            kind: LiteralKind::Verbatim,
                            c: ' ',
                        })
                    } else {
                        Ast::Class(class.clone())
                    }
                }
                Class::Unicode(_) => Ast::Class(class.clone()),
            },
            Ast::Group(group) => {
                let mut group = group.clone();
                group.ast = fix_ast(&group.ast, case_sensitive)?.0.into();
                expand_lookbehind(&group)?.unwrap_or(Ast::Group(group))
            }
            Ast::Repetition(repetition) => {
                let mut repetition = repetition.clone();
                repetition.ast = fix_ast(&repetition.ast, case_sensitive)?.0.into();
                match (&repetition.op.kind, &*repetition.ast) {
                    (
                        RepetitionKind::ZeroOrOne
                        | RepetitionKind::ZeroOrMore
                        | RepetitionKind::Range(RepetitionRange::Exactly(0))
                        | RepetitionKind::Range(RepetitionRange::AtLeast(0))
                        | RepetitionKind::Range(RepetitionRange::Bounded(0, _)),
                        Ast::Assertion(_),
                    ) => Ast::Empty(zero_span()),
                    (_, Ast::Assertion(_)) => (*repetition.ast).clone(),
                    (RepetitionKind::Range(_), Ast::Repetition(_)) => (*repetition.ast).clone(),
                    (RepetitionKind::OneOrMore, Ast::Repetition(_)) if repetition.greedy => {
                        Ast::Group(Group {
                            span: zero_span(),
                            kind: GroupKind::CaptureName(CaptureName {
                                span: zero_span(),
                                name: ATOMIC_PLACEHOLDER.into(),
                                index: 0,
                            }),
                            ast: repetition.ast,
                        })
                    }
                    // disallow nested quantifiers because of inconsistent behavior
                    (_, Ast::Repetition(_)) => {
                        return Err(Error::Unexpected(
                            "nested quantifiers in regex are not allowed.".into(),
                        ))
                    }
                    _ => Ast::Repetition(repetition),
                }
            }
            Ast::Literal(literal) if !case_sensitive => {
                if let Some(union) = literal_to_union(literal) {
                    Ast::Class(Class::Bracketed(ClassBracketed {
                        span: zero_span(),
                        negated: false,
                        kind: ClassSet::Item(ClassSetItem::Union(union)),
                    }))
                } else {
                    Ast::Literal(literal.clone())
                }
            }
            Ast::Literal(_) => root.clone(),
            Ast::Flags(flags) => {
                let mut flags = flags.clone();

                if let Some(i) = flags.flags.flag_state(Flag::CaseInsensitive) {
                    case_sensitive = !i;
                }

                flags.flags.items = flags
                    .flags
                    .items
                    .into_iter()
                    .filter(|flag| {
                        !matches!(
                            flag.kind,
                            FlagsItemKind::Flag(Flag::CaseInsensitive)
                            // we completely ignore the unicode flag, might not be sound
                                | FlagsItemKind::Flag(Flag::Unicode)
                                | FlagsItemKind::Flag(Flag::SwapGreed)
                        )
                    })
                    .collect();

                if !flags.flags.items.is_empty()
                    && matches!(
                        flags.flags.items[flags.flags.items.len() - 1].kind,
                        FlagsItemKind::Negation
                    )
                {
                    flags.flags.items.pop();
                }

                if flags.flags.items.is_empty() {
                    Ast::Empty(zero_span())
                } else {
                    Ast::Flags(flags)
                }
            }
            Ast::Dot(_) | Ast::Assertion(_) | Ast::Empty(_) => root.clone(),
        };
        Ok((ast, case_sensitive))
    }

    const ATOMIC_PLACEHOLDER: &str = "__ATOMIC";
    const NEGATIVE_LOOKBEHIND_PREFIX: &str = "__NEGATIVE_LOOKBEHIND_";
    const POSITIVE_LOOKBEHIND_PREFIX: &str = "__POSITIVE_LOOKBEHIND_";
    const MAX_LOOKBEHIND_VARIANTS: usize = 256;

    fn lookaround_name(group: &Group) -> Option<&str> {
        match &group.kind {
            GroupKind::CaptureName(name)
                if name.name.starts_with("__NEGATIVE_") || name.name.starts_with("__POSITIVE_") =>
            {
                Some(&name.name)
            }
            _ => None,
        }
    }

    fn fixed_len(ast: &Ast) -> Option<usize> {
        match ast {
            Ast::Empty(_) | Ast::Flags(_) | Ast::Assertion(_) => Some(0),
            Ast::Literal(_) | Ast::Dot(_) | Ast::Class(_) => Some(1),
            Ast::Group(group) if lookaround_name(group).is_some() => Some(0),
            Ast::Group(group) => fixed_len(&group.ast),
            Ast::Concat(concat) => concat.asts.iter().map(fixed_len).sum(),
            Ast::Alternation(alternation) => {
                let mut lens = alternation.asts.iter().map(fixed_len);
                let first = lens.next()??;
                lens.all(|len| len == Some(first)).then_some(first)
            }
            Ast::Repetition(repetition) => {
                let inner = fixed_len(&repetition.ast)?;
                match &repetition.op.kind {
                    RepetitionKind::Range(RepetitionRange::Exactly(n)) => Some(inner * *n as usize),
                    _ if inner == 0 => Some(0),
                    _ => None,
                }
            }
        }
    }

    fn strip_captures(ast: &Ast, captures: &mut Vec<GroupKind>) -> Ast {
        match ast {
            Ast::Group(group) if lookaround_name(group).is_none() => {
                let mut group = group.clone();
                if !matches!(group.kind, GroupKind::NonCapturing(_)) {
                    captures.push(group.kind.clone());
                    group.kind = GroupKind::NonCapturing(Flags {
                        span: zero_span(),
                        items: Vec::new(),
                    });
                }
                group.ast = strip_captures(&group.ast, captures).into();
                Ast::Group(group)
            }
            Ast::Concat(concat) => {
                let mut concat = concat.clone();
                concat.asts = concat
                    .asts
                    .iter()
                    .map(|x| strip_captures(x, captures))
                    .collect();
                Ast::Concat(concat)
            }
            Ast::Alternation(alternation) => {
                let mut alternation = alternation.clone();
                alternation.asts = alternation
                    .asts
                    .iter()
                    .map(|x| strip_captures(x, captures))
                    .collect();
                Ast::Alternation(alternation)
            }
            Ast::Repetition(repetition) => {
                let mut repetition = repetition.clone();
                repetition.ast = strip_captures(&repetition.ast, captures).into();
                Ast::Repetition(repetition)
            }
            _ => ast.clone(),
        }
    }

    fn concat(asts: Vec<Ast>) -> Ast {
        Ast::Concat(Concat {
            span: zero_span(),
            asts,
        })
    }

    fn alternation(asts: Vec<Ast>) -> Ast {
        Ast::Alternation(Alternation {
            span: zero_span(),
            asts,
        })
    }

    fn non_capturing(ast: Ast) -> Ast {
        Ast::Group(Group {
            span: zero_span(),
            kind: GroupKind::NonCapturing(Flags {
                span: zero_span(),
                items: Vec::new(),
            }),
            ast: Box::new(ast),
        })
    }

    fn product(left: Vec<Ast>, right: &[Ast]) -> Option<Vec<Ast>> {
        if left.len() * right.len() > MAX_LOOKBEHIND_VARIANTS {
            return None;
        }

        Some(
            left.iter()
                .flat_map(|l| {
                    right
                        .iter()
                        .map(move |r| concat(vec![l.clone(), r.clone()]))
                })
                .collect(),
        )
    }

    fn fixed_variants(ast: &Ast) -> Option<Vec<Ast>> {
        if fixed_len(ast).is_some() {
            return Some(vec![ast.clone()]);
        }

        match ast {
            Ast::Group(group) => Some(
                fixed_variants(&group.ast)?
                    .into_iter()
                    .map(|variant| {
                        let mut group = group.clone();
                        group.ast = Box::new(variant);
                        Ast::Group(group)
                    })
                    .collect(),
            ),
            Ast::Concat(concat) => concat
                .asts
                .iter()
                .try_fold(vec![Ast::Empty(zero_span())], |acc, x| {
                    product(acc, &fixed_variants(x)?)
                }),
            Ast::Alternation(alternation) => {
                let mut out = Vec::new();
                for x in &alternation.asts {
                    out.extend(fixed_variants(x)?);
                    if out.len() > MAX_LOOKBEHIND_VARIANTS {
                        return None;
                    }
                }
                Some(out)
            }
            Ast::Repetition(repetition) => {
                let (min, max) = match &repetition.op.kind {
                    RepetitionKind::ZeroOrOne => (0, 1),
                    RepetitionKind::Range(RepetitionRange::Exactly(n)) => (*n, *n),
                    RepetitionKind::Range(RepetitionRange::Bounded(m, n)) => (*m, *n),
                    RepetitionKind::ZeroOrMore
                    | RepetitionKind::OneOrMore
                    | RepetitionKind::Range(RepetitionRange::AtLeast(_)) => return None,
                };
                let inner = fixed_variants(&repetition.ast)?;
                let mut out = Vec::new();
                let mut current = vec![Ast::Empty(zero_span())];
                for count in 0..=max {
                    if count >= min {
                        out.extend(current.iter().cloned());
                    }
                    if count < max {
                        current = product(current, &inner)?;
                    }
                    if out.len() > MAX_LOOKBEHIND_VARIANTS {
                        return None;
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }

    fn expand_lookbehind(group: &Group) -> Result<Option<Ast>, Error> {
        let name = match lookaround_name(group) {
            Some(name) => name,
            None => return Ok(None),
        };
        let negative = if name.starts_with(NEGATIVE_LOOKBEHIND_PREFIX) {
            true
        } else if name.starts_with(POSITIVE_LOOKBEHIND_PREFIX) {
            false
        } else {
            return Ok(None);
        };

        if fixed_len(&group.ast).is_some() {
            return Ok(None);
        }

        let mut captures = Vec::new();
        let body = strip_captures(&group.ast, &mut captures);

        if !negative && !captures.is_empty() {
            return Err(Error::Unexpected(
                "variable-length positive lookbehind with capture groups is not supported.".into(),
            ));
        }

        let variants = fixed_variants(&body).ok_or_else(|| {
            Error::Unexpected("lookbehind of unbounded length is not supported.".into())
        })?;

        let mut by_len: Vec<(usize, Vec<Ast>)> = Vec::new();
        for variant in variants {
            let len = fixed_len(&variant).expect("variants have a fixed length");
            match by_len.iter_mut().find(|(l, _)| *l == len) {
                Some((_, asts)) => asts.push(variant),
                None => by_len.push((len, vec![variant])),
            }
        }

        let lookbehinds: Vec<Ast> = by_len
            .into_iter()
            .map(|(_, asts)| {
                let mut lookbehind = group.clone();
                lookbehind.ast = Box::new(if asts.len() == 1 {
                    asts.into_iter().next().unwrap()
                } else {
                    alternation(asts)
                });
                Ast::Group(lookbehind)
            })
            .collect();

        let mut out = if negative {
            lookbehinds
        } else {
            vec![non_capturing(alternation(lookbehinds))]
        };

        if !captures.is_empty() {
            let mut never = vec![
                Ast::Assertion(Assertion {
                    span: zero_span(),
                    kind: AssertionKind::WordBoundary,
                }),
                Ast::Assertion(Assertion {
                    span: zero_span(),
                    kind: AssertionKind::NotWordBoundary,
                }),
            ];
            never.extend(captures.into_iter().map(|kind| {
                Ast::Group(Group {
                    span: zero_span(),
                    kind,
                    ast: Box::new(Ast::Empty(zero_span())),
                })
            }));
            out.push(Ast::Repetition(Repetition {
                span: zero_span(),
                op: RepetitionOp {
                    span: zero_span(),
                    kind: RepetitionKind::ZeroOrOne,
                },
                greedy: true,
                ast: Box::new(non_capturing(concat(never))),
            }));
        }

        Ok(Some(non_capturing(concat(out))))
    }

    lazy_static! {
        static ref LOOKAROUND_MAP: HashMap<&'static str, &'static str> = {
            let mut map = HashMap::new();
            map.insert("(?!", r"(?P<__NEGATIVE_LOOKAHEAD_$1>");
            map.insert("(?<!", r"(?P<__NEGATIVE_LOOKBEHIND_$1>");
            map.insert("(?=", r"(?P<__POSITIVE_LOOKAHEAD_$1>");
            map.insert("(?<=", r"(?P<__POSITIVE_LOOKBEHIND_$1>");
            map
        };
    }

    /// Mirrors LanguageTool's `PatternRuleHandler.replaceSpacesInRegex`: for a
    /// `smart` `<regexp>` (the default), every literal space outside a character
    /// class matches a run of whitespace (including non-breaking and narrow
    /// non-breaking space). `type="exact"` skips this.
    pub fn replace_spaces_in_regex(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut in_bracket = false;

        for c in s.chars() {
            match c {
                '[' => {
                    in_bracket = true;
                    out.push(c);
                }
                ']' => {
                    in_bracket = false;
                    out.push(c);
                }
                ' ' if !in_bracket => out.push_str("(?:[\\s\u{00A0}\u{202F}]+)"),
                _ => out.push(c),
            }
        }

        out
    }

    /// Does a good effort of converting Java Regexes to regular expressions
    /// usable by `oniguruma` / `fancy-regex`.
    pub fn from_java_regex(
        in_regex: &str,
        case_sensitive: bool,
        full_match: bool,
    ) -> Result<String, Error> {
        let mut regex = in_regex.to_owned();
        let mut prev_error_start = None;

        let mut ast = loop {
            let mut ast_parser = regex_syntax::ast::parse::Parser::new();
            match ast_parser.parse(&regex) {
                Err(error) => {
                    let start = error.span().start.offset;
                    let end = error.span().end.offset;

                    // exit if an error already occured at the same position
                    // to prevent infinitely looping
                    if let Some(prev_error_start) = prev_error_start {
                        if prev_error_start == start {
                            break Err(error);
                        }
                    }

                    match error.kind() {
                        ErrorKind::EscapeUnrecognized => {
                            // remove the backslash
                            regex =
                                format!("{}{}", &regex[..start], &regex[start + '\\'.len_utf8()..]);
                        }
                        ErrorKind::UnsupportedLookAround => {
                            // replace unsupported lookaround syntax with a placeholder named group
                            if let Some(placeholder) = LOOKAROUND_MAP.get(&regex[start..end]) {
                                regex = format!(
                                    "{}{}{}",
                                    &regex[..start],
                                    placeholder.replace(r"$1", &start.to_string()),
                                    &regex[end..]
                                );
                            } else {
                                break Err(error);
                            }
                        }
                        // Java accepts `(?)` as an empty inline-flag group (a
                        // no-op); LanguageTool uses it e.g. in
                        // `regexp_match="(?)id"`. `regex-syntax` reports it as a
                        // repetition without an expression, so drop the group.
                        ErrorKind::RepetitionMissing
                            if start >= 1
                                && regex.as_bytes()[start - 1] == b'('
                                && regex[start..].starts_with("?)") =>
                        {
                            regex = format!("{}{}", &regex[..start - 1], &regex[start + 2..]);
                        }
                        _ => break Err(error),
                    }

                    prev_error_start = Some(start);
                }
                Ok(ast) => break Ok(ast),
            }
        }?;

        ast = fix_ast(&ast, case_sensitive)?.0;

        let mut printer = Printer::new();
        let mut out = String::new();
        printer.print(&ast, &mut out).unwrap();

        out = out.replace(&format!("(?P<{}>", ATOMIC_PLACEHOLDER), "(?>");

        // undo the placeholder named group replacement for lookaround
        for (original, placeholder) in LOOKAROUND_MAP.iter() {
            if let Some(index) = prev_error_start {
                for i in 0..(index + 1) {
                    out = out.replace(&placeholder.replace("$1", &i.to_string()), original);
                }
            }
        }

        if full_match {
            out = format!("^(?:{})$", out);
        }

        Ok(out)
    }

    fn ascii_digit_item(item: &mut ClassSetItem) {
        match item {
            ClassSetItem::Perl(perl) if matches!(perl.kind, ClassPerlKind::Digit) => {
                *item = ClassSetItem::Ascii(ClassAscii {
                    span: perl.span,
                    kind: ClassAsciiKind::Digit,
                    negated: perl.negated,
                });
            }
            ClassSetItem::Union(union) => union.items.iter_mut().for_each(ascii_digit_item),
            ClassSetItem::Bracketed(bracketed) => ascii_digit_set(&mut bracketed.kind),
            _ => {}
        }
    }

    fn ascii_digit_set(set: &mut ClassSet) {
        match set {
            ClassSet::Item(item) => ascii_digit_item(item),
            ClassSet::BinaryOp(op) => {
                ascii_digit_set(&mut op.lhs);
                ascii_digit_set(&mut op.rhs);
            }
        }
    }

    /// Expands the body of a Java character class (the text between `[` and
    /// `]`) into sorted, non-overlapping inclusive char ranges. Java's `\d` only
    /// matches ASCII digits unless `UNICODE_CHARACTER_CLASS` is set, so it is
    /// read as `[0-9]`.
    pub fn java_char_class_ranges(class: &str) -> Result<Vec<(char, char)>, String> {
        let pattern = format!("[{}]", class);
        let mut ast = regex_syntax::ast::parse::Parser::new()
            .parse(&pattern)
            .map_err(|error| error.to_string())?;
        match &mut ast {
            Ast::Class(Class::Bracketed(bracketed)) => ascii_digit_set(&mut bracketed.kind),
            _ => return Err(format!("{:?} is not a character class", class)),
        }

        let hir = regex_syntax::hir::translate::Translator::new()
            .translate(&pattern, &ast)
            .map_err(|error| error.to_string())?;

        match hir.into_kind() {
            HirKind::Class(hir::Class::Unicode(class)) => Ok(class
                .iter()
                .map(|range| (range.start(), range.end()))
                .collect()),
            _ => Err(format!("{:?} is not a Unicode character class", class)),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn java_char_class_digits_are_ascii() {
            let ranges = java_char_class_ranges(r"\d\-’").unwrap();
            assert_eq!(ranges, vec![('-', '-'), ('0', '9'), ('’', '’')]);
        }

        #[test]
        fn java_char_class_expands_properties_and_ranges() {
            let ranges = java_char_class_ranges("\\p{L}\u{300}-\u{36F}°").unwrap();
            let contains = |c: char| ranges.iter().any(|&(start, end)| start <= c && c <= end);
            assert!(contains('é'));
            assert!(contains('\u{301}'));
            assert!(contains('°'));
            assert!(!contains('—'));
            assert!(!contains('٣'));
            assert!(ranges.windows(2).all(|pair| pair[0].1 < pair[1].0));
        }

        #[test]
        fn i_flag_removed() {
            assert_eq!(
                from_java_regex(r"(?iu)\p{Lu}\p{Ll}+", false, false).unwrap(),
                r"\p{Lu}\p{Ll}+"
            )
        }

        #[test]
        fn i_flag_used() {
            assert_eq!(
                from_java_regex(r"(?i)a(?-i)b", false, false).unwrap(),
                r"[aA]b"
            )
        }

        #[test]
        fn positive_lookbehind() {
            assert_eq!(
                from_java_regex(r"(?i)(?<=x)(?-i)s", false, false).unwrap(),
                r"(?<=[xX])s"
            )
        }

        #[test]
        fn nested_quantifiers() {
            assert!(from_java_regex(r"[0-9,.]{2}*", false, false).is_err())
        }

        fn compiled(java: &str) -> crate::utils::regex::Regex {
            let regex =
                crate::utils::regex::Regex::new(from_java_regex(java, true, false).unwrap());
            regex.try_compile().unwrap();
            regex
        }

        fn matches(regex: &crate::utils::regex::Regex, text: &str) -> Vec<String> {
            regex
                .find_iter(text)
                .map(|m| m.as_str().to_owned())
                .collect()
        }

        #[test]
        fn quantified_assertion() {
            assert_eq!(from_java_regex(r"PRP$?", true, true).unwrap(), r"^(?:PRP)$");
            assert_eq!(from_java_regex(r"a\b{0,2}", true, false).unwrap(), r"a");
            assert_eq!(from_java_regex(r"a\b+", true, false).unwrap(), r"a\b");

            let regex = compiled(r"^(?:PRP$?)$");
            assert!(regex.is_match("PRP"));
            assert!(!regex.is_match("PRP$"));
        }

        #[test]
        fn interval_after_quantifier_is_ignored() {
            assert_eq!(
                from_java_regex(r"[0-9,.]*{1,30}", true, false).unwrap(),
                r"[0-9,.]*"
            );
            assert_eq!(from_java_regex(r"a{2}{3}", true, false).unwrap(), r"a{2}");
        }

        #[test]
        fn possessive_quantifier_becomes_atomic_group() {
            assert_eq!(from_java_regex(r"a*+b", true, false).unwrap(), r"(?>a*)b");
            assert_eq!(
                from_java_regex(r"a{1,3}+a", true, false).unwrap(),
                r"(?>a{1,3})a"
            );

            let regex = compiled(r"\d++5");
            assert!(!regex.is_match("12345"));
            assert_eq!(matches(&compiled(r"\d{1,2}+5"), "12345"), vec!["345"]);
        }

        #[test]
        fn unicode_character_class_flag_removed() {
            assert_eq!(from_java_regex(r"(?U)a+b", true, false).unwrap(), r"a+b");
            assert_eq!(matches(&compiled(r"(?U)a+?"), "aaa"), vec!["a", "a", "a"]);
        }

        #[test]
        fn variable_length_negative_lookbehind() {
            assert_eq!(
                from_java_regex(r"(?<!a-?)b", true, false).unwrap(),
                r"(?:(?<!a)(?<!a-))b"
            );

            let regex = compiled(r"(?<![a-c]\d{0,2})x");
            assert_eq!(matches(&regex, "a1x b12x 9x c123x x"), vec!["x", "x", "x"]);
        }

        #[test]
        fn variable_length_positive_lookbehind() {
            assert_eq!(
                from_java_regex(r"(?<=a|bc)d", true, false).unwrap(),
                r"(?:(?:(?<=a)|(?<=bc)))d"
            );

            let regex = compiled(r"(?<=a|bc)d");
            assert_eq!(matches(&regex, "ad bd bcd d"), vec!["d", "d"]);
        }

        #[test]
        fn lookbehind_expansion_keeps_group_numbers() {
            let regex = compiled(r"(?<!(x|y(z)))(\d)(\w)");
            assert_eq!(regex.captures_len(), 5);

            let captures = regex.captures("yz1a 2b").unwrap();
            assert!(captures.get(1).is_none());
            assert!(captures.get(2).is_none());
            assert_eq!(captures.get(3).unwrap().as_str(), "2");
            assert_eq!(captures.get(4).unwrap().as_str(), "b");
        }

        #[test]
        fn lookbehind_expansion_is_case_insensitive() {
            let regex = crate::utils::regex::Regex::new(
                from_java_regex(r"(?<!a-?)b", false, false).unwrap(),
            );
            assert_eq!(regex.find_iter("A-b cB").count(), 1);
        }

        #[test]
        fn unbounded_lookbehind_is_an_error() {
            assert!(from_java_regex(r"(?<!a+)b", true, false).is_err());
            assert!(from_java_regex(r"(?<=(a|bc))d", true, false).is_err());
        }

        #[test]
        fn fixed_length_lookbehind_unchanged() {
            assert_eq!(
                from_java_regex(r"(?<!ab|cd)e", true, false).unwrap(),
                r"(?<!ab|cd)e"
            );
        }

        #[test]
        fn lookbehind_expansion_utf8() {
            let regex = compiled("(?<![“’—…\u{2028}] ?)x");
            assert_eq!(
                matches(&regex, "“x ’ x —x … x \u{2028}x \u{2028} x ax"),
                vec!["x"]
            );

            let regex = compiled(r"(?<=[”‘]|—…)x");
            assert_eq!(matches(&regex, "”x ‘x —…x …x —x"), vec!["x", "x", "x"]);
        }

        #[test]
        fn empty_inline_flag_group_removed() {
            // Java accepts `(?)` as an empty inline-flag group; LanguageTool
            // uses it in `regexp_match="(?)id"` / `"(?)ife"`.
            assert_eq!(from_java_regex(r"(?)id", true, false).unwrap(), "id");
            assert_eq!(from_java_regex(r"(?)ife", true, false).unwrap(), "ife");
        }

        #[test]
        fn smart_regexp_space_replacement() {
            assert_eq!(
                replace_spaces_in_regex("a b"),
                "a(?:[\\s\u{00A0}\u{202F}]+)b"
            );
            // spaces inside a character class are left alone
            assert_eq!(replace_spaces_in_regex("[ ]{2}"), "[ ]{2}");
        }
    }
}
