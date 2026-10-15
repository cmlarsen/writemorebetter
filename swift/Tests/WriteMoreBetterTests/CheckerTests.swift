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
            XCTAssertEqual(projected, checker.analyze(text: paragraph.text))
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
}
