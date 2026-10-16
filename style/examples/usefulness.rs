use nlprule::Tokenizer;
use writemorebetter_style::reports::{
    cliches, dialogue_tags, echoes, repeated_phrases, sentence_length, sentence_starts,
    DocumentStats, StyleResources,
};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    let resources = StyleResources::new();
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap();
        let document = analyze_document(
            &tokenizer,
            &text,
            &DocumentOptions::new().with_hard_wrapped(path.contains("moby")),
        );
        if std::env::var_os("WMB_AUDIT_SPEECH").is_some() {
            for (index, span) in document.dialogue.iter().enumerate() {
                if !document.is_speech(index) {
                    eprintln!("{}: {}", path, text.get(span.byte.clone()).unwrap());
                }
            }
        }
        let stats = DocumentStats::new(&document);
        let cliches = cliches::report(
            &document,
            resources.cliches(&tokenizer),
            &Default::default(),
        )
        .unwrap();
        let length = sentence_length::report(&document, &Default::default(), None).unwrap();
        let starts = sentence_starts::report(&document, &Default::default(), None).unwrap();
        let tags = dialogue_tags::report(&document, &Default::default(), None).unwrap();
        let phrases = repeated_phrases::report(&document, &Default::default()).unwrap();
        let long = sentence_length::report(
            &document,
            &sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
            None,
        )
        .unwrap();
        let default_flags = cliches.matches.len()
            + length.groups.all.monotony_runs.len()
            + starts.groups.all.same_word_runs.len()
            + tags.showy_verbs.iter().map(|v| v.count).sum::<usize>()
            + tags.non_speech.count
            + tags.with_adverb.count
            + tags.untagged_runs.len();
        println!(
            "{}",
            serde_json::json!({"file":path,"words":stats.words.all,"sentences":stats.sentences,"speechSpans":stats.speech_spans,"echoes":echoes::report(&document,&echoes::EchoOptions::new()).unwrap().echoes.len(),"phrases":phrases.phrases.len(),"cliches":cliches.matches.len(),"long":long.groups.all.long_sentences.len(),"defaultFlags":default_flags,"monotony":length.groups.all.monotony_runs.len(),"openers":starts.groups.all.same_word_runs.len(),"verbFlags":tags.showy_verbs.iter().map(|v|v.count).sum::<usize>(),"adverbs":tags.with_adverb.count,"nonSpeech":tags.non_speech.count,"untagged":tags.untagged_runs.len()})
        );
    }
}
