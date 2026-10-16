use std::sync::OnceLock;

use nlprule::Tokenizer;
use writemorebetter_style::reports::sensory_balance::{
    self, PerceptualNorms, SensoryBalanceOptions, SensoryBalanceReport, SensoryGroup,
    SensoryModality,
};
use writemorebetter_style::reports::to_utf16_json;
use writemorebetter_style::{analyze_document, Document, DocumentOptions, Mode, StyleError};

const INVENTED: &str = "# invented for tests\n\
glimmer\tv\t45\t40\n\
lamp\tv\t46\t32\n\
hum\ta\t44\t30\n\
velvet\th\t42\t20\n\
smoke\to\t41\t25\n\
salt\tg\t43\t30\n\
ache\ti\t42\t30\n\
see\tv\t50\t45\n\
story\ta\t44\t21\n\
door\tv\t46\t15\n\
murmur\ta\t36\t12\n";

fn tokenizer() -> &'static Tokenizer {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap())
}

fn norms() -> PerceptualNorms {
    PerceptualNorms::from_tsv(INVENTED).unwrap()
}

fn document(text: &str) -> Document {
    analyze_document(tokenizer(), text, &DocumentOptions::new())
}

fn options() -> SensoryBalanceOptions {
    SensoryBalanceOptions::new().with_minimum_sense_words(1)
}

fn run(text: &str, options: &SensoryBalanceOptions) -> SensoryBalanceReport {
    sensory_balance::report(&document(text), &norms(), options).unwrap()
}

fn counts(group: &SensoryGroup) -> Vec<(SensoryModality, usize)> {
    group
        .modalities
        .iter()
        .filter(|share| share.count > 0)
        .map(|share| (share.modality, share.count))
        .collect()
}

fn percent(group: &SensoryGroup, modality: SensoryModality) -> Option<f64> {
    group
        .modalities
        .iter()
        .find(|share| share.modality == modality)
        .unwrap()
        .percent
}

fn words<'a>(text: &'a str, report: &SensoryBalanceReport) -> Vec<&'a str> {
    report
        .sense_words
        .iter()
        .map(|word| &text[word.span.byte.clone()])
        .collect()
}

#[test]
fn shares_follow_the_invented_table_per_scene_chapter_and_mode() {
    let text = "Chapter 1\nThe lamp glimmered. A hum rose and the smoke drifted.\n\n\nShe tasted salt. “The velvet hums,” she said.\nChapter 2\nAn ache.";
    let r = run(text, &options());
    use SensoryModality::*;
    assert_eq!(
        words(text, &r),
        [
            "lamp",
            "glimmered",
            "hum",
            "smoke",
            "salt",
            "velvet",
            "hums",
            "ache"
        ]
    );
    assert_eq!(r.scenes.len(), 3);
    assert_eq!(r.chapters.len(), 2);
    assert_eq!(
        counts(&r.scenes[0].groups.narration),
        [(Sight, 2), (Sound, 1), (Smell, 1)]
    );
    assert_eq!(percent(&r.scenes[0].groups.narration, Sight), Some(50.0));
    assert_eq!(percent(&r.scenes[0].groups.narration, Taste), Some(0.0));
    assert_eq!(counts(&r.scenes[1].groups.narration), [(Taste, 1)]);
    assert_eq!(
        counts(&r.scenes[1].groups.dialogue),
        [(Sound, 1), (Touch, 1)]
    );
    assert_eq!(counts(&r.scenes[2].groups.all), [(Interoception, 1)]);
    assert_eq!(
        counts(&r.chapters[0].groups.all),
        [(Sight, 2), (Sound, 2), (Touch, 1), (Smell, 1), (Taste, 1)]
    );
    assert_eq!(
        (
            r.scenes[0].chapter,
            r.scenes[1].chapter,
            r.scenes[2].chapter
        ),
        (0, 0, 1)
    );
    assert_eq!(r.chapters[1].chapter, 1);
    assert_eq!(
        &text[r.scenes[1].span.byte.clone()],
        "She tasted salt. “The velvet hums,” she said."
    );
    assert!(text[r.chapters[0].span.byte.clone()].starts_with("The lamp"));
    assert_eq!(r.groups.all.sense_words, 8);
    assert_eq!(r.groups.all.words, r.words.all);
    assert_eq!(
        r.groups.narration.words + r.groups.dialogue.words,
        r.words.all
    );
    let modes: Vec<_> = r.sense_words.iter().map(|w| w.mode).collect();
    assert_eq!(modes[5..7], [Mode::Dialogue, Mode::Dialogue]);
    assert_eq!(r.sense_words[1].lemma, "glimmer");
    assert_eq!(r.sense_words[6].lemma, "hum");
    assert_eq!(r.sense_words[0].strength, 4.6);
}

#[test]
fn shares_are_null_below_the_minimum_and_flags_need_one_dominant_sense() {
    let text = "The lamp glimmered. The lamp glimmered again. A lamp.\n\n\nThe lamp glimmered. A hum. The smoke.\n\n\nA lamp.";
    let options = SensoryBalanceOptions::new().with_minimum_sense_words(3);
    let r = run(text, &options);
    assert_eq!(r.scenes[0].single_sense, Some(SensoryModality::Sight));
    assert_eq!(r.scenes[1].single_sense, None);
    assert_eq!(r.scenes[2].single_sense, None);
    assert_eq!(
        percent(&r.scenes[0].groups.all, SensoryModality::Sight),
        Some(100.0)
    );
    assert_eq!(
        percent(&r.scenes[2].groups.all, SensoryModality::Sight),
        None
    );
    let loose = run(text, &options.clone().with_single_sense_percent(50.0));
    assert_eq!(loose.scenes[1].single_sense, Some(SensoryModality::Sight));
    let dialogue = run(
        "“The lamp glimmered. The lamp glimmered. A lamp,” she said.",
        &options,
    );
    assert_eq!(dialogue.scenes[0].single_sense, None);
    assert_eq!(dialogue.scenes[0].groups.dialogue.sense_words, 5);
}

#[test]
fn thresholds_exclusions_names_and_figurative_uses_are_respected() {
    let text = "She saw that it mattered. She saw the door. A story. Then Lamp came in. Smoke rose. A murmur.";
    let r = run(text, &options());
    assert_eq!(words(text, &r), ["saw", "Smoke"]);
    let strict = run(text, &options().with_minimum_strength(5.0));
    assert_eq!(words(text, &strict), ["saw"]);
    let sight = run(text, &options().with_sight_margin(1.0));
    assert_eq!(words(text, &sight), ["saw", "door", "Smoke"]);
    let margin = run(text, &options().with_dominance_margin(0.0));
    assert_eq!(words(text, &margin), ["saw", "Smoke"]);
    let low = run(
        text,
        &options()
            .with_minimum_strength(3.5)
            .with_dominance_margin(1.0),
    );
    assert_eq!(words(text, &low), ["saw", "Smoke", "murmur"]);
    let named = sensory_balance::report(
        &analyze_document(
            tokenizer(),
            text,
            &DocumentOptions::new().with_skip_names(vec!["Smoke".into()]),
        ),
        &norms(),
        &options(),
    )
    .unwrap();
    assert_eq!(words(text, &named), ["saw"]);
    assert_eq!(named.words.all + 1, r.words.all);
}

#[test]
fn spans_survive_curly_quotes_apostrophes_dashes_ellipses_and_line_separators() {
    let text = "The lamp’s glimmer—a hum… \u{2028}“Smoke,” she said, ‘velvet’s salt.’";
    let r = run(text, &options());
    assert_eq!(
        words(text, &r),
        ["lamp", "glimmer", "hum", "Smoke", "velvet", "salt"]
    );
    let utf16: Vec<u16> = text.encode_utf16().collect();
    for word in &r.sense_words {
        assert_eq!(
            String::from_utf16(&utf16[word.span.utf16.clone()]).unwrap(),
            &text[word.span.byte.clone()]
        );
        assert_eq!(
            text.chars()
                .skip(word.span.char.start)
                .take(word.span.char.len())
                .collect::<String>(),
            &text[word.span.byte.clone()]
        );
    }
    let json: serde_json::Value = serde_json::from_str(&to_utf16_json(&r)).unwrap();
    assert_eq!(json["senseWords"][3]["modality"], "smell");
    assert_eq!(json["senseWords"][3]["mode"], "dialogue");
    assert!(json["scenes"][0]["singleSense"].is_null());
}

#[test]
fn bundled_table_loads_and_validation_rejects_bad_input() {
    let bundled = PerceptualNorms::bundled();
    assert!(bundled.len() > 4_000);
    assert!(bundled.get("the").is_none());
    let text = "The lamp glimmered.";
    for options in [
        options().with_minimum_strength(5.5),
        options().with_dominance_margin(-1.0),
        options().with_sight_margin(f64::NAN),
        options().with_single_sense_percent(0.0),
        options().with_single_sense_percent(101.0),
    ] {
        assert!(matches!(
            sensory_balance::report(&document(text), &norms(), &options),
            Err(StyleError::InvalidOptions { .. })
        ));
    }
    let compact = analyze_document(
        tokenizer(),
        text,
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        sensory_balance::report(&compact, &norms(), &options()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
    let bom = PerceptualNorms::from_tsv("\u{feff}lamp\tv\t46\t32\n").unwrap();
    assert_eq!(bom.get("lamp").unwrap().modality, SensoryModality::Sight);
    for bad in [
        "lamp\tv\t46",
        "lamp\tq\t46\t32",
        "lamp\tv\t51\t2",
        "lamp\tv\tx\t2",
    ] {
        assert!(matches!(
            PerceptualNorms::from_tsv(bad),
            Err(StyleError::InvalidTable { .. })
        ));
    }
}

fn bundled(text: &str, options: &SensoryBalanceOptions) -> Vec<(String, String, SensoryModality)> {
    let report =
        sensory_balance::report(&document(text), PerceptualNorms::bundled(), options).unwrap();
    report
        .sense_words
        .iter()
        .map(|word| {
            (
                text[word.span.byte.clone()].to_owned(),
                word.lemma.clone(),
                word.modality,
            )
        })
        .collect()
}

fn surfaces(found: &[(String, String, SensoryModality)]) -> Vec<&str> {
    found
        .iter()
        .map(|(surface, _, _)| surface.as_str())
        .collect()
}

#[test]
fn bundled_past_tense_verbs_resolve_to_the_verb_not_the_noun() {
    use SensoryModality::*;
    let text = "I felt ashamed. I felt like a thief. He felt obliged to stay. She felt the bark. \
                The felt was green. He saw the ship. They heard the bells. She rose and left.";
    let found = bundled(text, &options());
    let lemmas: Vec<_> = found
        .iter()
        .map(|(surface, lemma, modality)| (surface.as_str(), lemma.as_str(), *modality))
        .collect();
    assert_eq!(
        lemmas,
        [
            ("felt", "feel", Touch),
            ("bark", "bark", Sound),
            ("felt", "felt", Touch),
            ("green", "green", Sight),
            ("saw", "see", Sight),
            ("heard", "hear", Sound),
            ("bells", "bell", Sound),
        ]
    );
}

#[test]
fn bundled_figurative_rules_skip_whitespace_but_not_punctuation() {
    let text =
        "I feel  ashamed. I see  that it matters. Instantly I felt  a shock. I felt, ashamed.";
    assert_eq!(surfaces(&bundled(text, &options())), ["felt", "felt"]);
    let wrapped = "Then I felt\r\nashamed of it, and I heard\r\nthat she left.\r\n";
    let report = sensory_balance::report(
        &analyze_document(
            tokenizer(),
            wrapped,
            &DocumentOptions::new().with_hard_wrapped(true),
        ),
        PerceptualNorms::bundled(),
        &options(),
    )
    .unwrap();
    assert!(report.sense_words.is_empty());
}

#[test]
fn bundled_appearance_hearsay_reading_and_flat_adverbs_are_not_imagery() {
    let text = "It looked as if it would rain. It looked as though he slept. She looked like her \
                mother. It sounded as if they wept. He heard that the war was over. They heard of \
                it. She read the letter. They told tales. Pretty soon we left. It was pretty cold. \
                He tried hard. I s’pose so.";
    assert_eq!(surfaces(&bundled(text, &options())), ["cold"]);
    let literal = "She looked at the red sky. He heard the bells. A pretty girl smiled. \
                   The ground was hard.";
    assert_eq!(
        surfaces(&bundled(literal, &options())),
        ["looked", "red", "sky", "heard", "bells", "pretty", "hard"]
    );
}

#[test]
fn bundled_compounds_fall_back_to_their_parts_with_the_whole_span() {
    let text = "It was red-hot and ice-cold, sweet-smelling, with dance-music and well-known men.";
    let found = bundled(text, &options());
    assert_eq!(
        found
            .iter()
            .map(|(surface, lemma, _)| (surface.as_str(), lemma.as_str()))
            .collect::<Vec<_>>(),
        [
            ("red-hot", "hot"),
            ("ice-cold", "cold"),
            ("sweet-smelling", "smelling"),
            ("dance-music", "music"),
        ]
    );
}

#[test]
fn bundled_table_keeps_temperature_and_colour_words_for_looser_options() {
    use SensoryModality::*;
    let bundled_norms = PerceptualNorms::bundled();
    for (word, modality) in [
        ("cold", Touch),
        ("warm", Touch),
        ("heat", Touch),
        ("crimson", Sight),
        ("scarlet", Sight),
    ] {
        assert_eq!(
            bundled_norms.get(word).unwrap().modality,
            modality,
            "{word}"
        );
    }
    let text = "The cold wind. A warm crimson glow.";
    assert_eq!(surfaces(&bundled(text, &options())), ["cold", "glow"]);
    assert_eq!(
        surfaces(&bundled(text, &options().with_minimum_strength(3.5))),
        ["cold", "warm", "crimson", "glow"]
    );
}
