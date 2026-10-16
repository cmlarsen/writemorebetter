use nlprule::Tokenizer;
use writemorebetter_style::reports::{echoes, run, StyleRequest, StyleResources};
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
        let result = run(&document, &StyleRequest::new(), &resources, &tokenizer).unwrap();
        let tags = result.dialogue_tags.unwrap();
        println!(
            "{}",
            serde_json::json!({"file":path,"words":result.stats.words.all,"sentences":result.stats.sentences,"speechSpans":result.stats.speech_spans,"echoes":echoes::report(&document,&echoes::EchoOptions::new()).unwrap().echoes.len(),"phrases":result.repeated_phrases.unwrap().phrases.len(),"cliches":result.cliches.unwrap().matches.len(),"long":result.sentence_length.as_ref().unwrap().groups.all.long_sentences.len(),"monotony":result.sentence_length.unwrap().groups.all.monotony_runs.len(),"openers":result.sentence_starts.unwrap().groups.all.same_word_runs.len(),"verbFlags":tags.showy_verbs.iter().map(|v|v.count).sum::<usize>(),"adverbs":tags.with_adverb.count,"nonSpeech":tags.non_speech.count,"untagged":tags.untagged_runs.len()})
        );
    }
}
