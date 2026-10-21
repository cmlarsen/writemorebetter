import Foundation
import XCTest
import WriteMoreBetter

final class CheckerTests: XCTestCase {
    private static var cached: Checker?

    private static func dataDirectory() -> URL {
        if let directory = ProcessInfo.processInfo.environment["WMB_DATA_DIR"] {
            return URL(fileURLWithPath: directory, isDirectory: true)
        }
        return repositoryRoot.appendingPathComponent("storage/66", isDirectory: true)
    }

    private static let repositoryRoot = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()

    private static func dataPath(_ name: String) throws -> String {
        let url = dataDirectory().appendingPathComponent(name)
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw XCTSkip("missing \(url.path); run the compile step of the CI 6.6 job first")
        }
        return url.path
    }

    private func checker() throws -> Checker {
        if let cached = Self.cached {
            return cached
        }
        let checker = try Checker(
            tokenizerPath: Self.dataPath("en_tokenizer.bin"),
            rulesPath: Self.dataPath("en_rules.bin"),
            spellVariant: "en_US"
        )
        Self.cached = checker
        return checker
    }

    func testFindsKnownGrammarError() throws {
        let text = "She was not been here since Monday."
        let suggestions = try checker().check(text: text)
        guard let hit = suggestions.first(where: { $0.replacements.contains("has not been") }) else {
            return XCTFail("BEEN_PART_AGREEMENT did not fire: \(suggestions.map(\.source))")
        }
        XCTAssertEqual(hit.source, "GRAMMAR/BEEN_PART_AGREEMENT/5")
        XCTAssertEqual((text as NSString).substring(with: hit.nsRange), "was not been")
        XCTAssertFalse(hit.message.isEmpty)
    }

    func testSpellingUsesTheRequestedVariant() throws {
        let suggestions = try checker().check(text: "She walkd home through the rain.")
        XCTAssertTrue(
            suggestions.contains { $0.replacements.contains("walked") },
            "expected a spelling suggestion for walkd: \(suggestions)"
        )
    }

    func testAnalysisPreservesSurfacesWithUTF16Offsets() throws {
        let checker = try checker()
        let text = "😀 “She’s goin’—again…”\u{2028}\n\nHe stayed."
        let sentences = checker.analyze(text: text)
        XCTAssertFalse(sentences.isEmpty)
        for sentence in sentences {
            let range = NSRange(location: Int(sentence.start), length: Int(sentence.end - sentence.start))
            XCTAssertEqual((text as NSString).substring(with: range), sentence.text)
            XCTAssertEqual(sentence.tokens.map(\.text).joined(), sentence.text)
            var cursor = sentence.start
            for token in sentence.tokens {
                XCTAssertEqual(token.start, cursor)
                XCTAssertGreaterThan(token.end, token.start)
                let range = NSRange(location: Int(token.start), length: Int(token.end - token.start))
                XCTAssertEqual((text as NSString).substring(with: range), token.text)
                cursor = token.end
            }
            XCTAssertEqual(cursor, sentence.end)
        }
        let tokens = sentences.flatMap(\.tokens)
        XCTAssertTrue(tokens.contains { $0.isWhitespace })
        XCTAssertTrue(tokens.first { $0.text == "—" }!.isPunctuation)
        let dropped = try XCTUnwrap(tokens.first { $0.text == "goin’" })
        XCTAssertEqual(dropped.readings.map(\.lemma), ["go"])
        XCTAssertEqual(dropped.readings.map(\.pos), ["VBG"])
        XCTAssertTrue(checker.analyze(text: " \n\u{2028}").isEmpty)
        XCTAssertFalse(checker.check(text: "The the dog barked.").isEmpty)
        XCTAssertEqual(checker.analyze(text: text), sentences)
    }

    func testDocumentAnalysisPreservesRecordsAndReusesTokenizer() throws {
        let checker = try checker()
        let text = "😀 “Zoë’s goin’—again…”\u{2028}She stayed.\r\n\r\nZoë walked with O’Brien."
        let options = DocumentOptions(skipNames: ["Zoë", "O'Brien"])
        let analysis = checker.analyze(text: text)
        let document = checker.analyzeDocument(text: text, options: options)
        let unskipped = checker.analyzeDocument(text: text, options: nil)
        let substring: (UInt32, UInt32) -> String = { start, end in
            (text as NSString).substring(with: NSRange(location: Int(start), length: Int(end - start)))
        }
        XCTAssertEqual(document.paragraphs.count, 2)
        XCTAssertEqual(document.dialogue.map { substring($0.start, $0.end) }, ["“Zoë’s goin’—again…”"])
        let tokens = document.paragraphs.flatMap(\.sentences).flatMap(\.tokens)
        for paragraph in document.paragraphs {
            XCTAssertEqual(substring(paragraph.start, paragraph.end), paragraph.text)
            let projected = paragraph.sentences.map { sentence in
                AnalysisSentence(
                    text: sentence.text, start: sentence.start - paragraph.start, end: sentence.end - paragraph.start,
                    tokens: sentence.tokens.map { token in
                        AnalysisToken(
                            text: token.text, start: token.start - paragraph.start, end: token.end - paragraph.start,
                            readings: token.readings, chunks: token.chunks,
                            isWhitespace: token.isWhitespace, isPunctuation: token.isPunctuation
                        )
                    }
                )
            }
            if paragraph.text.contains("\u{2028}") {
                XCTAssertEqual(paragraph.sentences.count, 2)
            } else {
                XCTAssertEqual(projected, checker.analyze(text: paragraph.text))
            }
            for sentence in paragraph.sentences {
                XCTAssertEqual(substring(sentence.start, sentence.end), sentence.text)
                XCTAssertEqual(sentence.tokens.map(\.text).joined(), sentence.text)
                for token in sentence.tokens {
                    XCTAssertEqual(substring(token.start, token.end), token.text)
                    XCTAssertEqual(token.isWord, token.text.unicodeScalars.contains { CharacterSet.alphanumerics.contains($0) })
                }
            }
        }
        let skipped = tokens.filter(\.isSkipped)
        XCTAssertTrue(skipped.contains { $0.text == "Zoë" })
        XCTAssertTrue(skipped.contains { $0.text == "O’Brien" })
        XCTAssertEqual(Int(document.wordCount), tokens.filter { $0.isWord && !$0.isSkipped }.count)
        XCTAssertEqual(Int(unskipped.wordCount - document.wordCount), skipped.filter(\.isWord).count)
        XCTAssertTrue(tokens.contains { $0.isWhitespace })
        XCTAssertTrue(try XCTUnwrap(tokens.first { $0.text == "—" }).isPunctuation)
        let dropped = try XCTUnwrap(tokens.first { $0.text == "goin’" })
        XCTAssertTrue(dropped.readings.contains { $0.lemma == "go" && $0.pos == "VBG" })
        XCTAssertFalse(document.lemmaIndex.isEmpty)
        for (lemma, positions) in document.lemmaIndex {
            for position in positions {
                let token = document.paragraphs[Int(position.paragraph)].sentences[Int(position.sentence)].tokens[Int(position.token)]
                XCTAssertTrue(token.isWord && !token.isSkipped)
                XCTAssertTrue(token.readings.contains { $0.lemma == lemma } || (token.readings.isEmpty && lemma == token.text.lowercased()))
            }
        }
        for (paragraphIndex, paragraph) in document.paragraphs.enumerated() {
            for (sentenceIndex, sentence) in paragraph.sentences.enumerated() {
                for (tokenIndex, token) in sentence.tokens.enumerated() where token.isWord && !token.isSkipped {
                    for lemma in Set(token.readings.map(\.lemma)) {
                        let positions = try XCTUnwrap(document.lemmaIndex[lemma])
                        XCTAssertEqual(positions.filter {
                            Int($0.paragraph) == paragraphIndex && Int($0.sentence) == sentenceIndex && Int($0.token) == tokenIndex
                        }.count, 1)
                    }
                }
            }
        }
        let empty = checker.analyzeDocument(text: " \n\u{2028}", options: nil)
        XCTAssertTrue(empty.paragraphs.isEmpty)
        XCTAssertTrue(empty.dialogue.isEmpty)
        XCTAssertTrue(empty.lemmaIndex.isEmpty)
        XCTAssertEqual(empty.wordCount, 0)
        let phrase = checker.analyzeDocument(text: "Port Royal met Port.", options: DocumentOptions(skipNames: ["Port Royal"]))
        let phraseTokens = phrase.paragraphs.flatMap(\.sentences).flatMap(\.tokens)
        XCTAssertEqual(phraseTokens.filter { $0.isWord && $0.isSkipped }.map(\.text), ["Port", "Royal"])
        XCTAssertEqual(phrase.wordCount, 2)
        XCTAssertFalse(checker.check(text: "The the dog barked.").isEmpty)
        XCTAssertEqual(checker.analyze(text: text), analysis)
        XCTAssertEqual(checker.analyzeDocument(text: text, options: options), document)
    }

    func testDocumentOptionsDefaultsNamesAndCompactProjection() throws {
        let checker = try checker()
        let text = "Will said he will go. Anna's hat. Anna’s coat. Charles' car."
        let defaults = DocumentOptions()
        XCTAssertTrue(defaults.includeTokenDetails)
        XCTAssertFalse(defaults.hardWrapped)
        let options = DocumentOptions(skipNames: ["Will", "Anna", "Charles"])
        let full = checker.analyzeDocument(text: text, options: options)
        let tokens = full.paragraphs.flatMap(\.sentences).flatMap(\.tokens)
        XCTAssertTrue(try XCTUnwrap(tokens.first { $0.text == "Will" }).isSkipped)
        XCTAssertFalse(try XCTUnwrap(tokens.first { $0.text == "will" }).isSkipped)
        XCTAssertTrue(tokens.filter { ["'s", "’s", "'", "’"].contains($0.text) }.allSatisfy(\.isSkipped))
        let detached = checker.analyzeDocument(text: "Anna 's hat.", options: DocumentOptions(skipNames: ["Anna"]))
        XCTAssertEqual(detached.wordCount, 2)
        XCTAssertTrue(detached.paragraphs.flatMap(\.sentences).flatMap(\.tokens).filter { $0.isWhitespace || $0.text == "'s" }.allSatisfy { !$0.isSkipped })
        let compact = checker.analyzeDocument(text: text, options: DocumentOptions(skipNames: options.skipNames, includeTokenDetails: false))
        XCTAssertEqual(compact.wordCount, full.wordCount)
        XCTAssertEqual(compact.lemmaIndex, full.lemmaIndex)
        XCTAssertTrue(compact.paragraphs.flatMap(\.sentences).flatMap(\.tokens).allSatisfy { $0.readings.isEmpty && $0.chunks.isEmpty })
    }

    func testOffsetsAreUTF16CodeUnits() throws {
        let prefix = "😀 “I’m fine” — e\u{301}\u{2028}"
        let text = prefix + "She was not been here since Monday."
        let suggestions = try checker().check(text: text)
        guard let hit = suggestions.first(where: { $0.replacements.contains("has not been") }) else {
            return XCTFail("BEEN_PART_AGREEMENT did not fire: \(suggestions.map(\.source))")
        }

        let expected = (text as NSString).range(of: "was not been")
        XCTAssertGreaterThan(prefix.utf16.count, prefix.count, "prefix must mix scalar and UTF-16 counts")
        XCTAssertEqual(Int(hit.start), expected.location)
        XCTAssertEqual(Int(hit.end), expected.location + expected.length)
        XCTAssertEqual(hit.nsRange, expected)
        XCTAssertEqual((text as NSString).substring(with: hit.nsRange), "was not been")
    }

    func testCheckFromSeveralThreads() throws {
        let checker = try checker()
        let text = "She was not been here since Monday. The the dog barked 😀"
        let expected = checker.check(text: text)
        XCTAssertFalse(expected.isEmpty)

        let iterations = 8
        let lock = NSLock()
        var results: [[Suggestion]] = []
        DispatchQueue.concurrentPerform(iterations: iterations) { _ in
            let actual = checker.check(text: text)
            lock.lock()
            results.append(actual)
            lock.unlock()
        }
        XCTAssertEqual(results.count, iterations)
        for actual in results {
            XCTAssertEqual(actual, expected)
        }
    }

    func testDictionaryEditsWhileChecking() throws {
        let checker = try Checker(
            tokenizerPath: Self.dataPath("en_tokenizer.bin"),
            rulesPath: Self.dataPath("en_rules.bin"),
            spellVariant: "en_US"
        )
        let text = "Zxqvarn and the the dog barked."
        XCTAssertFalse(checker.check(text: text).isEmpty, "the spellchecker should flag Zxqvarn")

        XCTAssertEqual(checker.addWords(words: ["Zxqvarn"]), 1)
        XCTAssertEqual(checker.addWords(words: ["Zxqvarn"]), 0)
        XCTAssertEqual(checker.words(), ["Zxqvarn"])
        XCTAssertFalse(
            checker.check(text: "Zxqvarn").contains { $0.source.contains("SPELL") },
            "an added word should stop being flagged: \(checker.check(text: "Zxqvarn"))"
        )

        let group = DispatchGroup()
        let rounds = 12
        for _ in 0..<4 {
            DispatchQueue.global().async(group: group) {
                for _ in 0..<rounds {
                    _ = checker.check(text: text)
                    _ = checker.words()
                }
            }
        }
        for round in 0..<rounds {
            if round.isMultiple(of: 2) {
                _ = checker.removeWords(words: ["Zxqvarn"])
            } else {
                _ = checker.addWords(words: ["Zxqvarn"])
            }
        }
        group.wait()

        XCTAssertEqual(checker.setWords(words: ["Zxqvarn", "Port Royal"]), 2)
        XCTAssertEqual(checker.words(), ["Port Royal", "Zxqvarn"])

        checker.clearWords()
        XCTAssertTrue(checker.words().isEmpty)
        XCTAssertFalse(checker.check(text: "Zxqvarn").isEmpty)
    }

    func testMotherTongueSelectsFalseFriends() throws {
        let checker = try Checker(
            tokenizerPath: Self.dataPath("en_tokenizer.bin"),
            rulesPath: Self.dataPath("en_rules.bin")
        )
        let text = "\u{201C}My handy is broken\u{2014}again\u{2026}\u{201D}\u{2028}It\u{2019}s the actual problem."
        let falseFriends = {
            checker.check(text: text).filter { $0.source.hasPrefix("FALSE_FRIENDS/") }
        }

        XCTAssertTrue(checker.motherTongues().contains("de"), "\(checker.motherTongues())")
        XCTAssertTrue(falseFriends().isEmpty, "no mother tongue is selected by default")

        checker.setMotherTongue(motherTongue: "de")
        guard let handy = falseFriends().first(where: { $0.source == "FALSE_FRIENDS/HANDY/0" }) else {
            return XCTFail("HANDY did not fire for a German speaker: \(falseFriends().map(\.source))")
        }
        XCTAssertEqual((text as NSString).substring(with: handy.nsRange), "handy")
        XCTAssertTrue(handy.replacements.contains("cell phone"))

        checker.setMotherTongue(motherTongue: nil)
        XCTAssertTrue(falseFriends().isEmpty)
    }

    func testEnableRulesAfterProfile() throws {
        let checker = try Checker(
            tokenizerPath: Self.dataPath("en_tokenizer.bin"),
            rulesPath: Self.dataPath("en_rules.bin")
        )
        let text = "\u{201C}It\u{2019}s, in my opinion, wrong\u{2026}\u{201D} she said\u{2014}quietly.\u{2028}It was a free gift."
        let fired = { (prefix: String) in
            checker.check(text: text).filter { $0.source.hasPrefix(prefix) }
        }
        XCTAssertTrue(fired("PLAIN_ENGLISH/").isEmpty, "plain-English rules are off by default")

        let enabled = try checker.enableRules(selectors: ["PLAIN_ENGLISH/IN_MY_OPINION", "REDUNDANCY"])
        XCTAssertGreaterThan(enabled, 1)
        guard let opinion = fired("PLAIN_ENGLISH/IN_MY_OPINION").first else {
            return XCTFail("IN_MY_OPINION did not fire once enabled")
        }
        XCTAssertTrue((text as NSString).substring(with: opinion.nsRange).contains("in my opinion"))
        XCTAssertFalse(fired("REDUNDANCY/").isEmpty, "REDUNDANCY is back on after the fiction profile")

        XCTAssertThrowsError(try checker.enableRules(selectors: ["NO_SUCH_CATEGORY"])) { error in
            guard case let CheckError.UnknownSelectors(selectors) = error else {
                return XCTFail("unexpected error: \(error)")
            }
            XCTAssertEqual(selectors, ["NO_SUCH_CATEGORY"])
        }
    }

    func testUnknownProfileThrows() throws {
        let tokenizer = try Self.dataPath("en_tokenizer.bin")
        let rules = try Self.dataPath("en_rules.bin")
        XCTAssertThrowsError(
            try Checker(tokenizerPath: tokenizer, rulesPath: rules, profile: "no-such-profile")
        ) { error in
            guard case let CheckError.UnknownProfile(name) = error else {
                return XCTFail("unexpected error: \(error)")
            }
            XCTAssertEqual(name, "no-such-profile")
        }
    }

    func testMissingTokenizerThrows() {
        XCTAssertThrowsError(
            try Checker(tokenizerPath: "/does/not/exist.bin", rulesPath: "/does/not/exist.bin")
        ) { error in
            guard case CheckError.TokenizerFile = error else {
                return XCTFail("unexpected error: \(error)")
            }
        }
    }
    func testIndependentStyleReports() throws {
        let checker = try checker()
        let text = "😀.\u{2028}\n\n“Fine,” he said. She kicked the bucket. She waited. She waited. She waited."
        let document = checker.styleDocument(text: text, options: nil)
        let stats = try document.stats()
        let length = try document.sentenceLength(optionsJson: "{\"minimumSentences\":1}")
        let starts = try document.sentenceStarts(optionsJson: "{\"minimumSentences\":1}")
        XCTAssertEqual(length.sentences, starts.sentences)
        XCTAssertEqual(length.words, starts.words)
        XCTAssertEqual(length.sentences.count, stats.sentences.all)
        XCTAssertNotNil(length.groups.all.meanDifference)
        XCTAssertNil(try document.sentenceLength(optionsJson: "{\"minimumSentences\":1}", baseline: .none).groups.all.meanDifference)
        let custom = try document.sentenceLength(optionsJson: "{\"minimumSentences\":1}", baseline: .custom("{\"mean\":1}"))
        XCTAssertEqual(custom.groups.all.meanDifference, custom.groups.all.mean.map { $0 - 1 })
        let cliches = try document.cliches()
        let match = try XCTUnwrap(cliches.matches.first)
        XCTAssertEqual((text as NSString).substring(with: match.span.nsRange), "kicked the bucket")
        let tags = try document.dialogueTags()
        let quote = try XCTUnwrap(tags.tags.first?.quotes.first)
        XCTAssertEqual((text as NSString).substring(with: quote.nsRange), "“Fine,”")
        XCTAssertEqual(try document.echoes().words, stats.words.all)
        XCTAssertEqual(try document.repeatedPhrases().words, stats.words.all)
        let fresh = { checker.styleDocument(text: text, options: nil) }
        XCTAssertEqual(try document.stats(), try fresh().stats())
        XCTAssertEqual(try document.echoes(), try fresh().echoes())
        XCTAssertEqual(try document.repeatedPhrases(), try fresh().repeatedPhrases())
        XCTAssertEqual(try document.cliches(), try fresh().cliches())
        XCTAssertEqual(try document.dialogueTags(), try fresh().dialogueTags())
        XCTAssertEqual(try document.sentenceLength(), try fresh().sentenceLength())
        XCTAssertEqual(try document.sentenceStarts(), try fresh().sentenceStarts())
        XCTAssertEqual(try document.overusedWords(), try fresh().overusedWords())
        XCTAssertThrowsError(try document.sentenceLength(optionsJson: "{\"typo\":1}"))
        XCTAssertThrowsError(try document.sentenceStarts(baseline: .custom("{\"universalPercent\":{\"Noun\":20}}")))
        let compact = checker.styleDocument(text: text, options: DocumentOptions(includeTokenDetails: false))
        XCTAssertEqual(try compact.cliches(), cliches)
        XCTAssertEqual(try compact.sentenceLength(), try document.sentenceLength())
        XCTAssertThrowsError(try compact.echoes())
        XCTAssertThrowsError(try compact.repeatedPhrases())
        XCTAssertThrowsError(try compact.dialogueTags())
        XCTAssertThrowsError(try compact.sentenceStarts())
        XCTAssertThrowsError(try compact.overusedWords())
        let spanText = "😀.\u{2028}He walked. She was walking. The old lamp glowed. The old lamp broke. The old lamp faded."
        let spanDocument = checker.styleDocument(text: spanText, options: nil)
        let echo = try XCTUnwrap(try spanDocument.echoes().echoes.first { $0.label == "walk" })
        XCTAssertEqual((spanText as NSString).substring(with: echo.earlier.nsRange), "walked")
        XCTAssertEqual((spanText as NSString).substring(with: echo.later.nsRange), "walking")
        let phrase = try XCTUnwrap(try spanDocument.repeatedPhrases().phrases.first)
        for span in phrase.spans {
            XCTAssertEqual((spanText as NSString).substring(with: span.nsRange), "The old lamp")
        }
        XCTAssertEqual(try document.sentenceLength(optionsJson: "{\"minimumSentences\":1}", baseline: .bundled), length)
        XCTAssertEqual(try document.sentenceLengthJson(optionsJson: nil, baselineJson: nil), try document.sentenceLengthJson(optionsJson: nil, baselineJson: "bundled"))
        XCTAssertEqual(try document.sentenceStartsJson(optionsJson: nil, baselineJson: nil), try document.sentenceStartsJson(optionsJson: nil, baselineJson: "bundled"))
        XCTAssertEqual(try document.dialogueTagsJson(optionsJson: nil, baselineJson: nil), try document.dialogueTagsJson(optionsJson: nil, baselineJson: "bundled"))
        XCTAssertEqual(try document.overusedWordsJson(optionsJson: nil, baselineJson: nil), try document.overusedWordsJson(optionsJson: nil, baselineJson: "bundled"))
        XCTAssertEqual(try document.overusedWords(baseline: .none).overused, [])
        let waited = try document.overusedWords(optionsJson: "{\"minimumCount\":3,\"minimumRatio\":1,\"minimumReferencePerMillion\":0}", baseline: .custom("{\"lemmas\":{\"wait\":0.01}}"))
        XCTAssertEqual(waited.overused.map(\.lemma), ["wait"])
        XCTAssertEqual(waited.overused.first?.spans.map { (text as NSString).substring(with: $0.nsRange) }, ["waited", "waited", "waited"])
        XCTAssertTrue(try document.sentenceStarts(optionsJson: "{\"minimumSentences\":1}", baseline: .none).groups.all.universal.allSatisfy { $0.difference == nil })
        XCTAssertNil(try document.dialogueTags(optionsJson: "{\"minimumTags\":1}", baseline: .none).saidAsked.difference)
        XCTAssertEqual(try document.dialogueTags(optionsJson: "{\"minimumTags\":1}", baseline: .custom("{\"saidAskedPercent\":50}")).saidAsked.difference, 50)
        for method in [document.sentenceLengthJson, document.sentenceStartsJson, document.dialogueTagsJson, document.overusedWordsJson] {
            for bad in ["null", "{", "{\"typo\":1}"] {
                XCTAssertThrowsError(try method(nil, bad)) { error in
                    guard case CheckError.ReportInput(let message) = error else { return XCTFail("unexpected error: \(error)") }
                    XCTAssertTrue(message.hasPrefix("invalid "))
                    XCTAssertTrue(message.contains(" baseline:"))
                }
            }
        }
        let long = checker.styleDocument(text: Array(repeating: "word", count: 50).joined(separator: " ") + ".", options: nil)
        XCTAssertEqual(try long.sentenceLength().groups.all.longSentences, [])
        XCTAssertEqual(try long.sentenceLength(optionsJson: "{\"includeLongSentences\":true}").groups.all.longSentences, [0])
    }

    func testStyleCodableRecordsDoNotLoseJSONFields() throws {
        let text = try String(contentsOf: Self.repositoryRoot.appendingPathComponent("style/tests/fixtures/binding-records.txt"), encoding: .utf8)
        let document = try checker().styleDocument(text: text)
        let lengthOptions = "{\"minimumSentences\":1,\"includeLongSentences\":true}"
        let startOptions = "{\"minimumSentences\":1}"
        let tagOptions = "{\"minimumTags\":1,\"maxUntaggedExchanges\":2}"
        let tagBaseline = "{\"saidAskedPercent\":50,\"adverbPercent\":10,\"nonSpeechPercent\":5}"
        func strippedNulls(_ value: Any) -> Any {
            if let object = value as? [String: Any] { return object.filter { !($0.value is NSNull) }.mapValues(strippedNulls) }
            if let array = value as? [Any] { return array.map(strippedNulls) }
            return value
        }
        func roundTrip<T: Codable>(_ type: T.Type, _ json: String, file: StaticString = #filePath, line: UInt = #line) throws {
            let data = Data(json.utf8)
            let value = try JSONDecoder().decode(type, from: data)
            let encoded = try JSONEncoder().encode(value)
            let original = strippedNulls(try JSONSerialization.jsonObject(with: data)) as! NSDictionary
            let actual = strippedNulls(try JSONSerialization.jsonObject(with: encoded)) as! NSDictionary
            XCTAssertEqual(actual, original, file: file, line: line)
        }
        let pacingText = try String(contentsOf: Self.repositoryRoot.appendingPathComponent("style/tests/fixtures/pacing.txt"), encoding: .utf8)
        let pacingDocument = try checker().styleDocument(text: pacingText)
        try roundTrip(PacingReport.self, pacingDocument.pacingJson())
        let clarityText = try String(contentsOf: Self.repositoryRoot.appendingPathComponent("style/tests/fixtures/clarity.txt"), encoding: .utf8)
        let clarityDocument = try checker().styleDocument(text: clarityText)
        try roundTrip(ClarityReport.self, clarityDocument.clarityJson(optionsJson: "{\"minimumSentences\":1}"))
        XCTAssertEqual(try clarityDocument.clarity().longClauses.first?.introducer, "who")
        try roundTrip(DocumentStats.self, document.statsJson())
        try roundTrip(EchoReport.self, document.echoesJson(optionsJson: nil))
        try roundTrip(RepeatedPhraseReport.self, document.repeatedPhrasesJson(optionsJson: nil))
        try roundTrip(ClicheReport.self, document.clichesJson(optionsJson: nil))
        try roundTrip(DialogueTagReport.self, document.dialogueTagsJson(optionsJson: tagOptions, baselineJson: tagBaseline))
        try roundTrip(SentenceLengthReport.self, document.sentenceLengthJson(optionsJson: lengthOptions, baselineJson: nil))
        try roundTrip(SentenceStartsReport.self, document.sentenceStartsJson(optionsJson: startOptions, baselineJson: nil))
        let transitionText = try String(contentsOf: Self.repositoryRoot.appendingPathComponent("style/tests/fixtures/transition-records.txt"), encoding: .utf8)
        let transitionDocument = try checker().styleDocument(text: transitionText)
        try roundTrip(TransitionsReport.self, transitionDocument.transitionsJson(optionsJson: "{\"minimumSentences\":1}"))
        try roundTrip(PronounStartsReport.self, transitionDocument.pronounStartsJson(optionsJson: "{\"minimumSentences\":1}"))
        try roundTrip(ReadabilityReport.self, transitionDocument.readabilityJson(optionsJson: "{\"minimumSentences\":1}"))
        try roundTrip(AdverbsReport.self, document.adverbsJson(optionsJson: nil))
        XCTAssertEqual(try transitionDocument.transitions().repeats.count, 1)
        XCTAssertEqual(try transitionDocument.pronounStarts().groups.narration.runs.count, 1)
        XCTAssertEqual(try transitionDocument.pronounStarts().groups.dialogue.runs.count, 1)
        let consistencyText = try String(contentsOf: Self.repositoryRoot.appendingPathComponent("style/tests/fixtures/consistency-records.txt"), encoding: .utf8)
        let consistencyDocument = try checker().styleDocument(text: consistencyText)
        try roundTrip(ConsistencyReport.self, consistencyDocument.consistencyJson(optionsJson: nil))
        XCTAssertTrue(try consistencyDocument.consistency().doubleQuotes.mixed)
        XCTAssertEqual(try consistencyDocument.consistency().hyphenation.first?.word, "email")
        let passiveText = try String(contentsOf: Self.repositoryRoot.appendingPathComponent("style/tests/fixtures/passive-records.txt"), encoding: .utf8)
        let passiveDocument = try checker().styleDocument(text: passiveText)
        try roundTrip(PassiveReport.self, passiveDocument.passiveJson(optionsJson: "{\"minimumSentences\":1}", baselineJson: nil))
        let passive = try passiveDocument.passive(optionsJson: "{\"minimumSentences\":1}")
        XCTAssertEqual(passive.groups.narration.passives, 2)
        XCTAssertEqual(passive.groups.dialogue.passives, 2)
        XCTAssertNil(passive.groups.all.band)
        XCTAssertNotNil(try passiveDocument.passive(baseline: .passivePublicStandIn).groups.all.band)
        let spellingText = try String(contentsOf: Self.repositoryRoot.appendingPathComponent("style/tests/fixtures/regional-spelling-records.txt"), encoding: .utf8)
        let spellingDocument = try checker().styleDocument(text: spellingText)
        try roundTrip(RegionalSpellingReport.self, spellingDocument.regionalSpellingJson(optionsJson: nil))
        let spelling = try spellingDocument.regionalSpelling()
        XCTAssertEqual(spelling.flags.map(\.form), ["focused", "color"])
        XCTAssertEqual(spelling.flags.map(\.reason), [.form, .system])
        XCTAssertEqual(spelling.dominant, [.british, .oxford])
        let normText = try String(contentsOf: Self.repositoryRoot.appendingPathComponent("style/tests/fixtures/norm-records.txt"), encoding: .utf8)
        let normDocument = try checker().styleDocument(text: normText)
        let table = "{\"hope\": 1.5, \"idea\": 1.6, \"freedom\": 1.3, \"lamp\": 4.8}"
        try roundTrip(SensoryBalanceReport.self, normDocument.sensoryBalanceJson(optionsJson: "{\"minimumSenseWords\":1}"))
        try roundTrip(DictionReport.self, normDocument.dictionJson(optionsJson: nil, table: table))
        let sensory = try normDocument.sensoryBalance(optionsJson: "{\"minimumSenseWords\":1}")
        XCTAssertFalse(sensory.senseWords.isEmpty)
        XCTAssertEqual(sensory.groups.all.words, try normDocument.stats().words.all)
        let diction = try normDocument.diction(table: table)
        XCTAssertFalse(diction.abstractRuns.isEmpty)
        XCTAssertEqual(diction.tableWords, 4)
        try roundTrip(DictionReport.self, normDocument.dictionJson(optionsJson: nil, table: nil))
        let bundled = try normDocument.diction()
        XCTAssertGreaterThan(bundled.tableWords, 14_000)
        XCTAssertThrowsError(try normDocument.diction(table: "lamp")) { error in
            guard case CheckError.ReportInput(let message) = error else { return XCTFail("unexpected error: \(error)") }
            XCTAssertTrue(message.hasPrefix("invalid diction table"))
        }
        let overusedOptions = "{\"minimumCount\":1,\"minimumRatio\":1,\"minimumReferencePerMillion\":0,\"includeNouns\":true}"
        try roundTrip(OverusedWordsReport.self, document.overusedWordsJson(optionsJson: overusedOptions, baselineJson: nil))
        let overused = try document.overusedWords(optionsJson: overusedOptions)
        XCTAssertFalse(overused.overused.isEmpty)
        XCTAssertFalse(overused.fillers.isEmpty)
        let length = try document.sentenceLength(optionsJson: lengthOptions)
        let tags = try document.dialogueTags(optionsJson: tagOptions, baseline: .custom(tagBaseline))
        XCTAssertFalse(length.groups.all.longSentences.isEmpty)
        XCTAssertFalse(length.groups.all.monotonyRuns.isEmpty)
        XCTAssertFalse(tags.beats.isEmpty)
        XCTAssertFalse(tags.untaggedRuns.isEmpty)
        XCTAssertTrue(tags.tags.contains { $0.adverb != nil })
        XCTAssertFalse(try document.sentenceStarts(optionsJson: startOptions).groups.all.sameWordRuns.isEmpty)
    }

    func testPacingLens() throws {
        let text = "😀 She remembered that he had gone… She knew that he had been alone.\nShe wondered why he had forgotten. She had never understood why he had gone.\nShe recalled how he had known. She had imagined that he had remembered."
        let document = try checker().styleDocument(text: text)
        let pacing = try document.pacing()
        XCTAssertEqual(pacing.slowRuns.count, 1)
        let run = try XCTUnwrap(pacing.slowRuns.first)
        XCTAssertEqual(run.paragraphs.start, 0)
        XCTAssertEqual(run.paragraphs.end, 3)
        XCTAssertEqual((text as NSString).substring(with: run.span.nsRange), text)
        XCTAssertTrue(run.triggeredSignals.contains(.pastPerfect))
        XCTAssertEqual(pacing.words, try document.stats().words)
        let roundTrip = try JSONDecoder().decode(PacingReport.self, from: Data(document.pacingJson().utf8))
        XCTAssertEqual(roundTrip, pacing)
        XCTAssertThrowsError(try document.pacing(optionsJson: "{\"minimumParagraphs\":0}"))
    }


    func testHouseStyleRules() throws {
        let rules = try String(contentsOf: Self.repositoryRoot.appendingPathComponent("style/tests/fixtures/house-style/bundle.yml"), encoding: .utf8)
        let text = "“Towards the door,” she said, walking towards it — sort of."
        let document = try checker().styleDocument(text: text)
        let report = try document.houseStyle(rules: rules)
        XCTAssertEqual(report.rules, 4)
        XCTAssertEqual(report.unsupported, [])
        XCTAssertEqual(report.matches.map(\.rule), ["House.Toward", "House.Toward", "House.Hedging"])
        XCTAssertEqual(report.matches[0].mode, .dialogue)
        XCTAssertEqual(report.matches[0].replacement, "toward")
        XCTAssertEqual((text as NSString).substring(with: report.matches[2].span.nsRange), "sort of")
        let narration = try document.houseStyle(rules: rules, optionsJson: "{\"skipDialogue\":true}")
        XCTAssertEqual(narration.matches.count, 2)
        XCTAssertThrowsError(try document.houseStyleJson(optionsJson: nil, rules: nil)) { error in
            guard case CheckError.ReportInput(let message) = error else { return XCTFail("unexpected error: \(error)") }
            XCTAssertTrue(message.hasPrefix("invalid houseStyle rules"))
        }
    }

    func testEmotionTellsLens() throws {
        let text = "😀 “I’m afraid of heights,” he said. She was afraid—then angry.\u{2028}Fear washed over her."
        let document = try checker().styleDocument(text: text)
        let tells = try document.emotionTells()
        XCTAssertEqual(tells.tells.map { (text as NSString).substring(with: $0.span.nsRange) }, ["was afraid", "Fear washed"])
        XCTAssertEqual(tells.tells.map(\.pattern), [.predicate, .subject])
        XCTAssertEqual(tells.groups.dialogue.tells, 0)
        let withDialogue = try document.emotionTells(optionsJson: "{\"includeDialogue\":true}")
        XCTAssertEqual(withDialogue.groups.dialogue.tells, 1)
        let roundTrip = try JSONDecoder().decode(EmotionTellsReport.self, from: Data(document.emotionTellsJson().utf8))
        XCTAssertEqual(roundTrip, tells)
        XCTAssertThrowsError(try document.emotionTells(optionsJson: "{\"typo\":true}"))
    }

    func testStylePresets() throws {
        XCTAssertEqual(StylePresets.names, ["chicago-fiction", "ap", "uk"])
        let uk = try StylePresets.summary(StylePresets.load("uk"))
        XCTAssertEqual(uk.name, "uk")
        XCTAssertEqual(uk.version, "1.0.0")
        let mine = try StylePresets.parse("{\"format\": 1, \"name\": \"mine\", \"extends\": \"ap\", \"checks\": {\"numbers\": null}}")
        XCTAssertEqual(try StylePresets.summary(mine).name, "mine")
        XCTAssertThrowsError(try StylePresets.parse("{\"format\": 1, \"name\": \"x\", \"colour\": 1}")) { error in
            guard case CheckError.ReportInput(let message) = error else { return XCTFail("unexpected error: \(error)") }
            XCTAssertTrue(message.hasPrefix("invalid style preset"))
        }

        let checker = try Checker(
            tokenizerPath: Self.dataPath("en_tokenizer.bin"),
            rulesPath: Self.dataPath("en_rules.bin"),
            profile: ""
        )
        XCTAssertGreaterThan(try checker.applyPreset(preset: "ap"), 0)
        let serial = checker.check(text: "We bought apples, pears, and plums.").map(\.source)
        XCTAssertTrue(serial.contains { $0.hasPrefix("STYLE/SERIAL_COMMA_OFF") }, "\(serial)")

        let text = "\u{201C}Mom, it\u{2019}s 7\u{2026}\u{201D} I said\u{2014}then James\u{2019} dad left.\u{2028}He called it \u{201C}art\u{201D}."
        let document = try checker.styleDocument(text: text)
        let report = try document.preset()
        XCTAssertEqual(report.preset, "chicago-fiction")
        XCTAssertEqual(report.checks.matches.map(\.check), [.numbers, .possessives, .quotePunctuation])
        XCTAssertEqual((text as NSString).substring(with: report.checks.matches[0].span.nsRange), "7")
        XCTAssertEqual(report.checks.matches[0].mode, .dialogue)
        XCTAssertEqual(report.consistency.dashes.preferred, "emClosed")
        let checks = try document.presetChecks(optionsJson: "{\"familyTitles\": true}")
        XCTAssertEqual(checks.matches.count, 0)
    }
}
