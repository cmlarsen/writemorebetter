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
