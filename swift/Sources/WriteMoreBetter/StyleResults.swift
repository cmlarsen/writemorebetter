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
    func dialogueTags(optionsJson: String? = nil, baselineJson: String? = nil) throws -> DialogueTagReport {
        try decodeStyle(dialogueTagsJson(optionsJson: optionsJson, baselineJson: baselineJson))
    }
    func sentenceLength(optionsJson: String? = nil, baselineJson: String? = nil) throws -> SentenceLengthReport {
        try decodeStyle(sentenceLengthJson(optionsJson: optionsJson, baselineJson: baselineJson))
    }
    func sentenceStarts(optionsJson: String? = nil, baselineJson: String? = nil) throws -> SentenceStartsReport {
        try decodeStyle(sentenceStartsJson(optionsJson: optionsJson, baselineJson: baselineJson))
    }
}

private func decodeStyle<T: Decodable>(_ json: String) throws -> T {
    try JSONDecoder().decode(T.self, from: Data(json.utf8))
}
