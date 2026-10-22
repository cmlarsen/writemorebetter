import Foundation

public struct Span: Codable, Equatable, Sendable {
    public let start: Int
    public let end: Int
    public var nsRange: NSRange { NSRange(location: start, length: end - start) }
}

public enum Mode: String, Codable, Sendable { case narration, dialogue }

public struct Split<T: Codable & Equatable & Sendable>: Codable, Equatable, Sendable {
    public let all: T
    public let narration: T
    public let dialogue: T
}

public struct SentenceRange: Codable, Equatable, Sendable {
    public let start: Int
    public let end: Int
}

public struct PacingParagraphRange: Codable, Equatable, Sendable {
    public let start: Int
    public let end: Int
}

public enum ClicheKind: String, Codable, Sendable {
    case cliche
    case redundancy
}

public enum DialogueTagPosition: String, Codable, Sendable {
    case before
    case after
    case split
}

public enum DialogueTagVerbClass: String, Codable, Sendable {
    case speech
    case nonSpeech
    case unlisted
}

public enum SentenceStartCategory: String, Codable, Sendable {
    case subject
    case adverb
    case gerund
    case infinitive
    case subordinatingConjunction
    case coordinatingConjunction
    case pastParticiple
    case inversion
    case introductory
    case other
}

public enum SensoryModality: String, Codable, Sendable {
    case sight
    case sound
    case touch
    case smell
    case taste
    case interoception
}

public struct Echo: Codable, Equatable, Sendable {
    public let key: String
    public let label: String
    public let earlier: Span
    public let later: Span
    public let distance: Int
}

public struct EchoReport: Codable, Equatable, Sendable {
    public let echoes: [Echo]
    public let words: Int
    public let perThousandWords: Double
}

public struct RepeatedPhrase: Codable, Equatable, Sendable {
    public let keys: [String]
    public let labels: [String]
    public let count: Int
    public let spans: [Span]
}

public struct RepeatedPhraseReport: Codable, Equatable, Sendable {
    public let words: Int
    public let phrases: [RepeatedPhrase]
}

public struct OverusedWord: Codable, Equatable, Sendable {
    public let lemma: String
    public let count: Int
    public let perTenThousandWords: Double
    public let referencePerTenThousandWords: Double
    public let ratio: Double
    public let spans: [Span]
}

public struct OverusedWordsFiller: Codable, Equatable, Sendable {
    public let word: String
    public let count: Int
    public let perTenThousandWords: Double
    public let referencePerTenThousandWords: Double?
    public let ratio: Double?
    public let spans: [Span]
}

public struct OverusedWordsReport: Codable, Equatable, Sendable {
    public let words: Int
    public let overused: [OverusedWord]
    public let fillers: [OverusedWordsFiller]
}

public struct ClicheMatch: Codable, Equatable, Sendable {
    public let kind: ClicheKind
    public let mode: Mode
    public let phrase: String
    public let source: String
    public let span: Span
}

public struct ClicheCounts: Codable, Equatable, Sendable {
    public let cliches: Int
    public let redundancies: Int
    public let words: Int
}

public struct ClicheReport: Codable, Equatable, Sendable {
    public let matches: [ClicheMatch]
    public let groups: Split<ClicheCounts>
}

public struct DialogueTag: Codable, Equatable, Sendable {
    public let span: Span
    public let clause: Span
    public let verb: Span
    public let verbText: String
    public let lemma: String
    public let `class`: DialogueTagVerbClass
    public let showy: Bool
    public let position: DialogueTagPosition
    public let adverb: Span?
    public let quotes: [Span]
}

public struct DialogueTagActionBeat: Codable, Equatable, Sendable {
    public let span: Span
    public let quotes: [Span]
}

public struct DialogueTagUntaggedRun: Codable, Equatable, Sendable {
    public let span: Span
    public let quotes: [Span]
    public let exchanges: Int
}

public struct DialogueTagVerbCount: Codable, Equatable, Sendable {
    public let lemma: String
    public let `class`: DialogueTagVerbClass
    public let count: Int
}

public struct DialogueTagReport: Codable, Equatable, Sendable {
    public let words: Split<Int>
    public let tags: [DialogueTag]
    public let beats: [DialogueTagActionBeat]
    public let untaggedRuns: [DialogueTagUntaggedRun]
    public let otherVerbs: [DialogueTagVerbCount]
    public let neutralVerbs: [DialogueTagVerbCount]
    public let showyVerbs: [DialogueTagVerbCount]
    public let lowConfidence: Bool
    public let dialogueSpans: Int
    public let speechSpans: Int
    public let taggedSpans: Int
    public let exchanges: Int
    public let untaggedExchanges: Int
    public let saidAsked: Share
    public let withAdverb: Share
    public let nonSpeech: Share
}

public struct SentenceLengthBucket: Codable, Equatable, Sendable {
    public let words: Int
    public let sentences: Int
}

public struct SentenceLengthPercentile: Codable, Equatable, Sendable {
    public let percentile: Int
    public let value: Double?
    public let difference: Double?
    public let highlighted: Bool
}

public struct SentenceLengthGroup: Codable, Equatable, Sendable {
    public let sentences: Int
    public let zeroWordSentences: Int
    public let histogram: [SentenceLengthBucket]
    public let mean: Double?
    public let meanDifference: Double?
    public let meanHighlighted: Bool
    public let lowConfidence: Bool
    public let percentiles: [SentenceLengthPercentile]
    public let longSentences: [Int]
    public let monotonyRuns: [SentenceRun]
}

public struct SentenceLengthReport: Codable, Equatable, Sendable {
    public let sentences: [SentenceRow]
    public let words: Split<Int>
    public let groups: Split<SentenceLengthGroup>
}

public struct SentenceStart: Codable, Equatable, Sendable {
    public let sentence: Int
    public let opener: Span?
    public let word: String?
    public let category: SentenceStartCategory
    public let ambiguous: Bool
    public let universalCandidates: [String]
}

public struct SentenceStartCategoryShare: Codable, Equatable, Sendable {
    public let category: SentenceStartCategory
    public let count: Int
    public let percent: Double?
    public let difference: Double?
    public let highlighted: Bool
}

public struct SentenceStartUniversalShare: Codable, Equatable, Sendable {
    public let tag: String
    public let weight: Double
    public let percent: Double?
    public let difference: Double?
    public let highlighted: Bool
}

public struct SentenceStartsGroup: Codable, Equatable, Sendable {
    public let sentences: Int
    public let lowConfidence: Bool
    public let excludedSentences: Int
    public let ambiguousStarts: Int
    public let unknownStarts: Int
    public let categories: [SentenceStartCategoryShare]
    public let universal: [SentenceStartUniversalShare]
    public let sameWordRuns: [SentenceRun]
}

public struct SentenceStartsReport: Codable, Equatable, Sendable {
    public let sentences: [SentenceRow]
    public let words: Split<Int>
    public let starts: [SentenceStart]
    public let groups: Split<SentenceStartsGroup>
}

public struct Share: Codable, Equatable, Sendable {
    public let count: Int
    public let percent: Double?
    public let difference: Double?
    public let highlighted: Bool
}

public struct SentenceRow: Codable, Equatable, Sendable {
    public let span: Span
    public let mode: Mode
    public let words: Int
}

public struct SentenceRun: Codable, Equatable, Sendable {
    public let span: Span
    public let sentences: SentenceRange
}

public struct DocumentStats: Codable, Equatable, Sendable {
    public let paragraphs: Int
    public let hardWrapped: Bool
    public let tokenDetails: Bool
    public let words: Split<Int>
    public let sentences: Split<Int>
    public let dialogueSpans: Int
    public let speechSpans: Int
}

public struct SenseWord: Codable, Equatable, Sendable {
    public let span: Span
    public let lemma: String
    public let modality: SensoryModality
    public let strength: Double
    public let mode: Mode
}

public struct SensoryShare: Codable, Equatable, Sendable {
    public let modality: SensoryModality
    public let count: Int
    public let percent: Double?
}

public struct SensoryGroup: Codable, Equatable, Sendable {
    public let words: Int
    public let senseWords: Int
    public let modalities: [SensoryShare]
}

public struct SensoryUnit: Codable, Equatable, Sendable {
    public let span: Span
    public let chapter: Int
    public let groups: Split<SensoryGroup>
}

public struct SensoryBalanceReport: Codable, Equatable, Sendable {
    public let words: Split<Int>
    public let senseWords: [SenseWord]
    public let groups: Split<SensoryGroup>
    public let chapters: [SensoryUnit]
    public let scenes: [SensoryUnit]
}

public struct DictionSentence: Codable, Equatable, Sendable {
    public let span: Span
    public let mode: Mode
    public let words: Int
    public let ratedWords: Int
    public let mean: Double?
    public let abstract: Bool
}

public struct DictionParagraph: Codable, Equatable, Sendable {
    public let span: Span
    public let sentences: SentenceRange
    public let ratedWords: Int
    public let mean: Double?
}

public struct DictionGroup: Codable, Equatable, Sendable {
    public let sentences: Int
    public let ratedWords: Int
    public let mean: Double?
    public let abstractSentences: Int
}

public struct DictionReport: Codable, Equatable, Sendable {
    public let words: Split<Int>
    public let tableWords: Int
    public let sentences: [DictionSentence]
    public let paragraphs: [DictionParagraph]
    public let groups: Split<DictionGroup>
    public let abstractRuns: [SentenceRun]
}

public enum PacingSignal: String, Codable, Equatable, Sendable {
    case sentenceLength, pastPerfect, cognition
}
public struct PacingSignals: Codable, Equatable, Sendable {
    public let words: Split<Int>
    public let sentences: Int
    public let verbPhrases: Int
    public let pastPerfect: Int
    public let cognition: Int
    public let dialoguePercent: Double?
    public let meanSentenceLength: Double?
    public let pastPerfectPercent: Double?
    public let cognitionPercent: Double?
}
public struct PacingParagraph: Codable, Equatable, Sendable {
    public let span: Span
    public let sceneBreakBefore: Bool
    public let signals: PacingSignals
    public let hasSpeech: Bool
    public let triggeredSignals: [PacingSignal]
}
public struct PacingRun: Codable, Equatable, Sendable {
    public let span: Span
    public let paragraphs: PacingParagraphRange
    public let signals: PacingSignals
    public let triggeredSignals: [PacingSignal]
}
public struct PacingReport: Codable, Equatable, Sendable {
    public let words: Split<Int>
    public let paragraphs: [PacingParagraph]
    public let slowRuns: [PacingRun]
}

/// Explicit baseline selection for a single report call. Custom input is a baseline JSON object.
public enum StyleBaseline: Equatable, Sendable {
    case bundled
    case none
    case custom(String)

    var selector: String {
        switch self {
        case .bundled: return "bundled"
        case .none: return "none"
        case .custom(let json): return json
        }
    }
}

public extension StyleDocument {
    func stats() throws -> DocumentStats { try decodeStyle(statsJson()) }
    func echoes(optionsJson: String? = nil) throws -> EchoReport {
        try decodeStyle(echoesJson(optionsJson: optionsJson))
    }
    func repeatedPhrases(optionsJson: String? = nil) throws -> RepeatedPhraseReport {
        try decodeStyle(repeatedPhrasesJson(optionsJson: optionsJson))
    }
    func cliches(optionsJson: String? = nil) throws -> ClicheReport {
        try decodeStyle(clichesJson(optionsJson: optionsJson))
    }
    func dialogueTags(optionsJson: String? = nil, baseline: StyleBaseline = .bundled) throws -> DialogueTagReport {
        try decodeStyle(dialogueTagsJson(optionsJson: optionsJson, baselineJson: baseline.selector))
    }
    func sentenceLength(optionsJson: String? = nil, baseline: StyleBaseline = .bundled) throws -> SentenceLengthReport {
        try decodeStyle(sentenceLengthJson(optionsJson: optionsJson, baselineJson: baseline.selector))
    }
    func sentenceStarts(optionsJson: String? = nil, baseline: StyleBaseline = .bundled) throws -> SentenceStartsReport {
        try decodeStyle(sentenceStartsJson(optionsJson: optionsJson, baselineJson: baseline.selector))
    }
    func pacing(optionsJson: String? = nil) throws -> PacingReport {
        try decodeStyle(pacingJson(optionsJson: optionsJson))
    }
    func sensoryBalance(optionsJson: String? = nil) throws -> SensoryBalanceReport {
        try decodeStyle(sensoryBalanceJson(optionsJson: optionsJson))
    }
    /// `table` is a word-to-concreteness JSON object or TSV text; nil uses the bundled table.
    func diction(table: String? = nil, optionsJson: String? = nil) throws -> DictionReport {
        try decodeStyle(dictionJson(optionsJson: optionsJson, table: table))
    }
    func overusedWords(optionsJson: String? = nil, baseline: StyleBaseline = .bundled) throws -> OverusedWordsReport {
        try decodeStyle(overusedWordsJson(optionsJson: optionsJson, baselineJson: baseline.selector))
    }
}

private func decodeStyle<T: Decodable>(_ json: String) throws -> T {
    try JSONDecoder().decode(T.self, from: Data(json.utf8))
}

public enum TransitionType: String, Codable, Sendable { case contrast, cause, time, addition, example, condition }

public struct TransitionMatch: Codable, Equatable, Sendable {
    public let span: Span
    public let sentence: Int
    public let phrase: String
    public let kind: TransitionType
    public let opening: Bool
    public let mode: Mode
}

public struct TransitionRepeat: Codable, Equatable, Sendable {
    public let span: Span
    public let phrase: String
    public let spans: [Span]
    public let sentences: [Int]
    public let mode: Mode
}

public struct TransitionTypeShare: Codable, Equatable, Sendable {
    public let kind: TransitionType
    public let containing: Share
    public let opening: Share
}

public struct TransitionsGroup: Codable, Equatable, Sendable {
    public let sentences: Int
    public let lowConfidence: Bool
    public let containing: Share
    public let opening: Share
    public let types: [TransitionTypeShare]
}

public struct TransitionsReport: Codable, Equatable, Sendable {
    public let matches: [TransitionMatch]
    public let repeats: [TransitionRepeat]
    public let groups: Split<TransitionsGroup>
}

public struct PronounStart: Codable, Equatable, Sendable {
    public let span: Span
    public let sentence: Int
    public let pronoun: String
    public let mode: Mode
}

public struct PronounStartsRun: Codable, Equatable, Sendable {
    public let span: Span
    public let pronoun: String
    public let sentences: SentenceRange
    public let mode: Mode
}

public struct PronounStartsShare: Codable, Equatable, Sendable {
    public let pronoun: String
    public let count: Int
    public let percent: Double?
    public let difference: Double?
    public let highlighted: Bool
}

public struct PronounStartsGroup: Codable, Equatable, Sendable {
    public let sentences: Int
    public let excludedSentences: Int
    public let lowConfidence: Bool
    public let pronounStarts: Share
    public let pronouns: [PronounStartsShare]
    public let runs: [PronounStartsRun]
    public let words: Int
    public let pronounWords: Share
    public let wordPronouns: [PronounStartsShare]
}

public struct PronounStartsReport: Codable, Equatable, Sendable {
    public let starts: [PronounStart]
    public let groups: Split<PronounStartsGroup>
}

public extension StyleDocument {
    func transitions(optionsJson: String? = nil) throws -> TransitionsReport {
        try decodeStyle(transitionsJson(optionsJson: optionsJson))
    }
    func pronounStarts(optionsJson: String? = nil) throws -> PronounStartsReport {
        try decodeStyle(pronounStartsJson(optionsJson: optionsJson))
    }
}

public struct ConsistencyVariant: Codable, Equatable, Sendable {
    public let form: String
    public let counts: Split<Int>
    public let spans: [Span]
}

public enum SpellingSystem: String, Codable, Sendable { case american, british, oxford }

public enum RegionalSpellingReason: String, Codable, Sendable { case system, form }

public struct SpellingSystemCount: Codable, Equatable, Sendable {
    public let system: SpellingSystem
    public let count: Int
}

public struct RegionalSpellingForm: Codable, Equatable, Sendable {
    public let form: String
    public let systems: [SpellingSystem]
    public let counts: Split<Int>
    public let spans: [Span]
}

public struct ConsistencyCheck: Codable, Equatable, Sendable {
    public let mixed: Bool
    public let variants: [ConsistencyVariant]
    /// The preferred form from the options (a style preset), or nil without one.
    public let preferred: String?
    /// Occurrences of every form other than `preferred`; 0 without a preference.
    public let nonPreferred: Int
}

public struct ConsistencyCount: Codable, Equatable, Sendable {
    public let form: String
    public let counts: Split<Int>
}

public struct ConsistencyWord: Codable, Equatable, Sendable {
    public let word: String
    public let variants: [ConsistencyVariant]
}

public struct ConsistencyReport: Codable, Equatable, Sendable {
    public let doubleQuotes: ConsistencyCheck
    public let singleQuotes: ConsistencyCheck
    public let apostrophes: ConsistencyCheck
    public let ellipses: ConsistencyCheck
    public let dashes: ConsistencyCheck
    public let dashCharacters: [ConsistencyCount]
    public let capitalization: [ConsistencyWord]
    public let hyphenation: [ConsistencyWord]
}

public extension StyleDocument {
    func consistency(optionsJson: String? = nil) throws -> ConsistencyReport {
        try decodeStyle(consistencyJson(optionsJson: optionsJson))
    }
}

public struct ReadabilityScores: Codable, Equatable, Sendable {
    public let sentences: Int
    public let words: Int
    public let syllables: Int
    public let characters: Int
    public let polysyllables: Int
    public let lowConfidence: Bool
    public let fleschReadingEase: Double?
    public let fleschKincaidGrade: Double?
    public let colemanLiau: Double?
    public let automatedReadability: Double?
    public let readingMinutes: Double
}

public struct ReadabilityParagraph: Codable, Equatable, Sendable {
    public let span: Span
    public let dialogueOnly: Bool
    public let excluded: Bool
    public let scores: ReadabilityScores
}

public struct ReadabilityReport: Codable, Equatable, Sendable {
    public let groups: Split<ReadabilityScores>
    public let paragraphs: [ReadabilityParagraph]
}

public extension StyleDocument {
    func readability(optionsJson: String? = nil) throws -> ReadabilityReport {
        try decodeStyle(readabilityJson(optionsJson: optionsJson))
    }
}

public struct AdverbMatch: Codable, Equatable, Sendable {
    public let span: Span
    public let lemma: String
    public let mode: Mode
    public let tagAdverb: Bool
}

public struct AdverbLemma: Codable, Equatable, Sendable {
    public let lemma: String
    public let count: Int
    public let perThousandWords: Double
}

public struct AdverbGroup: Codable, Equatable, Sendable {
    public let words: Int
    public let count: Int
    public let perThousandWords: Double
    public let lemmas: [AdverbLemma]
}

public struct AdverbsReport: Codable, Equatable, Sendable {
    public let matches: [AdverbMatch]
    public let groups: Split<AdverbGroup>
}

public extension StyleDocument {
    func adverbs(optionsJson: String? = nil) throws -> AdverbsReport {
        try decodeStyle(adverbsJson(optionsJson: optionsJson))
    }
}

public enum PassiveAuxiliary: String, Codable, Sendable { case be, get }

public struct PercentileBand: Codable, Equatable, Sendable {
    public let p5: Double
    public let p95: Double
}

public struct PassiveMatch: Codable, Equatable, Sendable {
    public let span: Span
    public let participle: Span
    public let sentence: Int
    public let auxiliary: PassiveAuxiliary
    public let progressive: Bool
    public let agent: Bool
    public let mode: Mode
}

public struct PassiveGroup: Codable, Equatable, Sendable {
    public let sentences: Int
    public let lowConfidence: Bool
    public let passives: Int
    public let perHundredSentences: Double?
    public let band: PercentileBand?
    public let highlighted: Bool
}

public struct PassiveReport: Codable, Equatable, Sendable {
    public let passives: [PassiveMatch]
    public let groups: Split<PassiveGroup>
}

public extension StyleBaseline {
    /// Passive voice only: the provisional band from public text, until a published-fiction band exists.
    static let passivePublicStandIn = StyleBaseline.custom("publicStandIn")
}

public extension StyleDocument {
    /// The bundled baseline is no comparison until a published-fiction band exists.
    func passive(optionsJson: String? = nil, baseline: StyleBaseline = .bundled) throws -> PassiveReport {
        try decodeStyle(passiveJson(optionsJson: optionsJson, baselineJson: baseline.selector))
    }
}

public enum ClarityClauseIntroducer: String, Codable, Sendable { case subordinator, relative }

public struct ClarityStickySentence: Codable, Equatable, Sendable {
    public let span: Span
    public let sentence: Int
    public let mode: Mode
    public let words: Int
    public let glueWords: Int
    public let gluePercent: Double
    public let glue: [Span]
}

public struct ClarityLongClause: Codable, Equatable, Sendable {
    public let span: Span
    public let sentence: Int
    public let mode: Mode
    public let introducer: String
    public let introducerSpan: Span
    public let kind: ClarityClauseIntroducer
    public let words: Int
    public let sentenceWords: Int
}

public struct ClarityGroup: Codable, Equatable, Sendable {
    public let sentences: Int
    public let eligibleSentences: Int
    public let lowConfidence: Bool
    public let glue: Share
    public let sticky: Share
    public let subordinateClauses: Int
    public let longClauses: Int
    public let longClauseSentences: Share
}

public struct ClarityReport: Codable, Equatable, Sendable {
    public let words: Split<Int>
    public let stickySentences: [ClarityStickySentence]
    public let longClauses: [ClarityLongClause]
    public let groups: Split<ClarityGroup>
}

public extension StyleDocument {
    func clarity(optionsJson: String? = nil) throws -> ClarityReport {
        try decodeStyle(clarityJson(optionsJson: optionsJson))
    }
}

public enum HouseStyleExtends: String, Codable, Sendable { case existence, substitution, consistency, occurrence }
public enum HouseStyleLevel: String, Codable, Sendable { case suggestion, warning, error }

public struct HouseStyleMatch: Codable, Equatable, Sendable {
    public let rule: String
    public let extends: HouseStyleExtends
    public let level: HouseStyleLevel
    public let message: String
    public let span: Span
    public let mode: Mode
    public let replacement: String?
    public let count: Int?
    public let link: String?
}

public struct HouseStyleCounts: Codable, Equatable, Sendable {
    public let suggestions: Int
    public let warnings: Int
    public let errors: Int
    public let words: Int
}

public struct HouseStyleRuleError: Codable, Equatable, Sendable {
    public let rule: String
    public let message: String
}

public struct HouseStyleReport: Codable, Equatable, Sendable {
    public let rules: Int
    public let unsupported: [HouseStyleRuleError]
    public let matches: [HouseStyleMatch]
    public let groups: Split<HouseStyleCounts>
}

public extension StyleDocument {
    /// `rules` is Vale-format YAML or JSON text supplied by the caller.
    func houseStyle(rules: String, optionsJson: String? = nil) throws -> HouseStyleReport {
        try decodeStyle(houseStyleJson(optionsJson: optionsJson, rules: rules))
    }
}

public struct RegionalSpellingWord: Codable, Equatable, Sendable {
    public let word: String
    public let mixed: Bool
    public let forms: [RegionalSpellingForm]
}

public struct RegionalSpellingFlag: Codable, Equatable, Sendable {
    public let span: Span
    public let sentence: Int
    public let form: String
    public let word: String
    public let systems: [SpellingSystem]
    public let reason: RegionalSpellingReason
    public let dominantForm: String?
    public let mode: Mode
}

public struct RegionalSpellingReport: Codable, Equatable, Sendable {
    public let markers: Int
    public let systems: [SpellingSystemCount]
    public let dominant: [SpellingSystem]
    public let words: [RegionalSpellingWord]
    public let flags: [RegionalSpellingFlag]
}

public extension StyleDocument {
    func regionalSpelling(optionsJson: String? = nil) throws -> RegionalSpellingReport {
        try decodeStyle(regionalSpellingJson(optionsJson: optionsJson))
    }
}

public enum EmotionTellPattern: String, Codable, Sendable { case predicate, object, subject }

public struct EmotionTell: Codable, Equatable, Sendable {
    public let span: Span
    public let emotion: Span
    public let verb: Span
    public let word: String
    public let pattern: EmotionTellPattern
    public let sentence: Int
    public let mode: Mode
}

public struct EmotionTellsGroup: Codable, Equatable, Sendable {
    public let tells: Int
    public let words: Int
    public let perThousandWords: Double?
}

public struct EmotionTellsReport: Codable, Equatable, Sendable {
    public let tells: [EmotionTell]
    public let groups: Split<EmotionTellsGroup>
}

public extension StyleDocument {
    /// Narration only unless options set `includeDialogue`.
    func emotionTells(optionsJson: String? = nil) throws -> EmotionTellsReport {
        try decodeStyle(emotionTellsJson(optionsJson: optionsJson))
    }
}

public enum PresetCheck: String, Codable, Sendable {
    case numbers, quotePunctuation, possessives, timeAbbreviations, familyTitles
}

public struct PresetCheckMatch: Codable, Equatable, Sendable {
    public let check: PresetCheck
    public let message: String
    public let span: Span
    public let mode: Mode
    /// A fixed term from the check's table, when the check has one.
    public let replacement: String?
}

public struct PresetChecksReport: Codable, Equatable, Sendable {
    public let matches: [PresetCheckMatch]
    public let counts: Split<Int>
}

public struct PresetReport: Codable, Equatable, Sendable {
    public let preset: String
    public let version: String?
    public let consistency: ConsistencyReport
    public let houseStyle: HouseStyleReport
    public let checks: PresetChecksReport
}

/// The identifying fields of a resolved style preset; the full JSON passes through unchanged.
public struct StylePresetSummary: Codable, Equatable, Sendable {
    public let format: Int
    public let name: String
    public let version: String?
    public let description: String?
    /// The heading style to pass as `TitleCaseOptions.style`; `nil` means Chicago.
    public let titleCase: TitleCaseStyle?
}

public enum StylePresets {
    /// The built-in presets' names; the first is the default.
    public static var names: [String] { stylePresetNames() }

    /// A built-in preset by name, as self-contained preset JSON.
    public static func load(_ name: String) throws -> String { try loadStylePresetJson(name: name) }

    /// Validates a preset file and resolves its `extends`, returning self-contained preset JSON.
    public static func parse(_ json: String) throws -> String { try parseStylePresetJson(text: json) }

    public static func summary(_ json: String) throws -> StylePresetSummary { try decodeStyle(json) }
}

public extension StyleDocument {
    /// `optionsJson` is a `PresetChecks` object; every check is off by default.
    func presetChecks(optionsJson: String? = nil) throws -> PresetChecksReport {
        try decodeStyle(presetChecksJson(optionsJson: optionsJson))
    }

    /// `preset` is a built-in preset's name or a preset file (JSON); nil selects "chicago-fiction".
    func preset(_ preset: String? = nil) throws -> PresetReport {
        try decodeStyle(presetJson(preset: preset))
    }
}

public struct AlliterationRun: Codable, Equatable, Sendable {
    public let span: Span
    /// The shared first sound, as a lowercase ARPAbet consonant label (`k`, `sh`, `jh`, …).
    public let sound: String
    public let spans: [Span]
    public let words: [String]
    public let sentence: Int
    public let mode: Mode
}

public struct AlliterationGroup: Codable, Equatable, Sendable {
    public let words: Int
    public let runs: Int
    public let perThousandWords: Double
}

public struct AlliterationReport: Codable, Equatable, Sendable {
    public let runs: [AlliterationRun]
    public let groups: Split<AlliterationGroup>
}

public extension StyleDocument {
    /// Runs of stressed words in a sentence that share a first sound; informational only.
    func alliteration(optionsJson: String? = nil) throws -> AlliterationReport {
        try decodeStyle(alliterationJson(optionsJson: optionsJson))
    }
}

public enum TitleCaseStyle: String, Codable, Sendable { case chicago, ap, sentence }

public enum TitleCaseRule: String, Codable, Sendable {
    case firstWord, lastWord, afterColon, majorWord, minorWord, sentenceCase
}

public struct TitleCaseMatch: Codable, Equatable, Sendable {
    /// Index of the heading in the input.
    public let heading: Int
    /// UTF-16 offsets into that heading.
    public let span: Span
    public let word: String
    /// The same word recased.
    public let replacement: String
    public let rule: TitleCaseRule
}

public struct TitleCaseReport: Codable, Equatable, Sendable {
    public let style: TitleCaseStyle
    public let headings: Int
    public let flaggedHeadings: Int
    public let matches: [TitleCaseMatch]
}

public enum TitleCase {
    /// Checks headings such as chapter titles; `optionsJson` is a `TitleCaseOptions` object
    /// (`{"style":"ap","skipNames":["Mara"]}`), Chicago-style headline case when nil.
    public static func check(_ headings: [String], optionsJson: String? = nil) throws -> TitleCaseReport {
        try decodeStyle(titleCaseJson(headings: headings, optionsJson: optionsJson))
    }
}

public enum VagueWordCategory: String, Codable, Sendable { case filler, hedge, quantity, noun, adjective }

public struct VagueWord: Codable, Equatable, Sendable {
    public let span: Span
    public let sentence: Int
    public let term: String
    public let category: VagueWordCategory
    public let mode: Mode
}

public struct VagueWordTerm: Codable, Equatable, Sendable {
    public let term: String
    public let category: VagueWordCategory
    public let count: Int
    public let narration: Int
    public let dialogue: Int
}

public struct VagueWordsGroup: Codable, Equatable, Sendable {
    public let matches: Int
    public let words: Int
    public let perThousandWords: Double?
}

public struct VagueWordsReport: Codable, Equatable, Sendable {
    public let matches: [VagueWord]
    public let terms: [VagueWordTerm]
    public let groups: Split<VagueWordsGroup>
}

public enum HiddenVerbPattern: String, Codable, Sendable { case lightVerb, ofPhrase }

public struct HiddenVerb: Codable, Equatable, Sendable {
    public let span: Span
    public let noun: Span
    public let lightVerb: Span?
    public let nominalization: String
    /// The related verb, named for the writer; the report never rewrites the text.
    public let term: String
    public let pattern: HiddenVerbPattern
    public let sentence: Int
    public let mode: Mode
}

public struct HiddenVerbTerm: Codable, Equatable, Sendable {
    public let term: String
    public let nominalization: String
    public let count: Int
    public let narration: Int
    public let dialogue: Int
}

public struct HiddenVerbsGroup: Codable, Equatable, Sendable {
    public let matches: Int
    public let words: Int
    public let perThousandWords: Double?
}

public struct HiddenVerbsReport: Codable, Equatable, Sendable {
    public let matches: [HiddenVerb]
    public let terms: [HiddenVerbTerm]
    public let groups: Split<HiddenVerbsGroup>
}

public extension StyleDocument {
    /// Narration and dialogue are counted separately; options may set `fillers` to false.
    func vagueWords(optionsJson: String? = nil) throws -> VagueWordsReport {
        try decodeStyle(vagueWordsJson(optionsJson: optionsJson))
    }

    /// Options may set `ofPhrases` to false to keep only light-verb matches.
    func hiddenVerbs(optionsJson: String? = nil) throws -> HiddenVerbsReport {
        try decodeStyle(hiddenVerbsJson(optionsJson: optionsJson))
    }
}
