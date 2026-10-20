use nlprule::Tokenizer;
use writemorebetter_style::reports::diction::{self, ConcretenessTable};
use writemorebetter_style::reports::sensory_balance::{self, PerceptualNorms, SensoryGroup};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn shares(group: &SensoryGroup) -> String {
    group
        .modalities
        .iter()
        .map(|share| format!("{:?}:{}", share.modality, share.count))
        .collect::<Vec<_>>()
        .join(" ")
}

fn context(text: &str, span: &std::ops::Range<usize>) -> String {
    let start = text.floor_char_boundary(span.start.saturating_sub(50));
    let end = text.ceil_char_boundary((span.end + 30).min(text.len()));
    format!(
        "{}[[{}]]{}",
        &text[start..span.start],
        &text[span.clone()],
        &text[span.end..end]
    )
    .replace(['\r', '\n'], " ")
}

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    let custom = std::env::var("WMB_CONCRETENESS")
        .ok()
        .map(|path| ConcretenessTable::parse(&std::fs::read_to_string(path).unwrap()).unwrap());
    let table = custom.as_ref().unwrap_or(ConcretenessTable::bundled());
    let sensory_options =
        serde_json::from_str(&std::env::var("WMB_SENSORY_OPTIONS").unwrap_or_else(|_| "{}".into()))
            .unwrap();
    let diction_options =
        serde_json::from_str(&std::env::var("WMB_DICTION_OPTIONS").unwrap_or_else(|_| "{}".into()))
            .unwrap();
    let words = std::env::var_os("WMB_SENSE_WORDS").is_some();
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap();
        let document = analyze_document(
            &tokenizer,
            &text,
            &DocumentOptions::new().with_hard_wrapped(path.contains("moby")),
        );
        let sensory =
            sensory_balance::report(&document, PerceptualNorms::bundled(), &sensory_options)
                .unwrap();
        if let Ok(directory) = std::env::var("WMB_NORM_JSON") {
            let diction = diction::report(&document, table, &diction_options).unwrap();
            std::fs::create_dir_all(&directory).unwrap();
            let name = std::path::Path::new(&path).file_name().unwrap();
            std::fs::write(
                std::path::Path::new(&directory)
                    .join(name)
                    .with_extension("json"),
                serde_json::to_vec(&serde_json::json!({"sensory": sensory, "diction": diction}))
                    .unwrap(),
            )
            .unwrap();
        }
        println!(
            "== {path} words={} senseWords={} scenes={} chapters={} narration[{}] dialogue[{}]",
            sensory.words.all,
            sensory.sense_words.len(),
            sensory.scenes.len(),
            sensory.chapters.len(),
            shares(&sensory.groups.narration),
            shares(&sensory.groups.dialogue)
        );
        for (index, scene) in sensory.scenes.iter().enumerate() {
            if scene.groups.narration.sense_words >= 20 {
                println!(
                    "scene {index} narrationSenseWords={} [{}] {}",
                    scene.groups.narration.sense_words,
                    shares(&scene.groups.narration),
                    text[scene.span.byte.clone()]
                        .chars()
                        .take(60)
                        .collect::<String>()
                        .replace(['\r', '\n'], " ")
                );
            }
        }
        if words {
            for word in &sensory.sense_words {
                println!(
                    "word {:?} {:?} {} | {}",
                    word.modality,
                    word.mode,
                    word.lemma,
                    context(&text, &word.span.byte)
                );
            }
        }
        let diction = diction::report(&document, table, &diction_options).unwrap();
        println!(
            "diction sentences={} abstract={} runs={} mean={:?}",
            diction.groups.all.sentences,
            diction.groups.all.abstract_sentences,
            diction.abstract_runs.len(),
            diction.groups.all.mean
        );
        for sentence in diction.sentences.iter().filter(|s| s.is_abstract) {
            println!(
                "abstract {:.2} {:?} | {}",
                sentence.mean.unwrap(),
                sentence.mode,
                text[sentence.span.byte.clone()].replace(['\r', '\n'], " ")
            );
        }
        for run in &diction.abstract_runs {
            println!(
                "run {} | {}",
                run.sentences.len(),
                text[run.span.byte.clone()].replace(['\r', '\n'], " ")
            );
        }
    }
}
