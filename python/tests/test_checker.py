"""Tests for the writemorebetter Python package.

They need the LGPL data files, which are not in the wheel. Point WMB_DATA_DIR at a directory
holding en_tokenizer.bin and en_rules.bin (the unpacked release data zip); by default the tests
look in the checkout's storage/ and dist/data/.
"""

import os
import threading
import time
from pathlib import Path

import pytest

import writemorebetter
from writemorebetter import Checker, StyleError, Suggestion, WriteMoreBetterError

ROOT = Path(__file__).resolve().parents[2]


def _data_dir() -> Path:
    candidates = [os.environ.get("WMB_DATA_DIR"), ROOT / "storage", ROOT / "dist" / "data"]
    for candidate in candidates:
        if candidate and (Path(candidate) / writemorebetter.RULES_FILE).is_file():
            return Path(candidate)
    pytest.skip("no data files; set WMB_DATA_DIR to the unpacked release data zip")


@pytest.fixture(scope="module")
def data_dir() -> Path:
    return _data_dir()


@pytest.fixture(scope="module")
def checker(data_dir) -> Checker:
    return Checker.from_dir(data_dir, spell_variant="en_US")


def sources(suggestions):
    return [s.source for s in suggestions]


def test_check_flags_a_grammar_error(checker):
    text = "She was went to the store."
    suggestions = checker.check(text)
    assert suggestions, "expected a grammar suggestion"
    first = suggestions[0]
    assert isinstance(first, Suggestion)
    assert isinstance(first.replacements, tuple)
    assert text[first.start : first.end] in text
    assert first.message


def test_clean_text_has_no_suggestions(checker):
    assert checker.check("She went to the store.") == []


# Each prefix is non-ASCII in a way that makes byte, UTF-16 and code-point offsets disagree.
# The misspelling "recieve" must come back at its Python string index.
PREFIXES = [
    "“Fine,” she said. ",  # curly double quotes (3 UTF-8 bytes each)
    "It’s late. ",  # curly apostrophe
    "He paused—then left. ",  # em dash
    "Wait… ",  # ellipsis
    "One line. ",  # line separator
    "🙂 Smile. ",  # astral: 1 code point, 2 UTF-16 units, 4 UTF-8 bytes
    "👩‍👩‍👧 Family. 🎉🎉 ",  # ZWJ emoji sequence and repeated astral characters
]


@pytest.mark.parametrize("prefix", PREFIXES)
def test_offsets_are_code_points(checker, prefix):
    text = prefix + "I will recieve it tomorrow."
    spelling = [s for s in checker.check(text) if s.source.startswith("SPELLCHECK")]
    assert len(spelling) == 1, checker.check(text)
    s = spelling[0]
    assert text[s.start : s.end] == "recieve"
    assert "receive" in s.replacements
    assert s.start == text.index("recieve")


def test_offsets_after_many_astral_characters(checker):
    text = "🙂" * 50 + " She was went home."
    suggestions = checker.check(text)
    assert suggestions
    for s in suggestions:
        assert 0 <= s.start <= s.end <= len(text)
        assert text[s.start : s.end].strip()
        # Code points, not UTF-16: the span lies after the 50 emoji, not 50 units later.
        assert s.start > 50 and s.end <= len(text)


def test_user_dictionary(data_dir):
    checker = Checker.from_dir(data_dir, spell_variant="en_US")
    text = "Zorbath’s sword gleamed. Zorbath smiled."
    assert any(text[s.start : s.end].startswith("Zorbath") for s in checker.check(text))
    assert checker.add_words(["Zorbath", "Port Royal"]) == 2
    assert checker.add_words(["Zorbath"]) == 0
    assert checker.words() == ["Port Royal", "Zorbath"]
    assert not any("Zorbath" in text[s.start : s.end] for s in checker.check(text))
    assert checker.remove_words(["Zorbath"]) == 1
    assert checker.words() == ["Port Royal"]
    assert checker.set_words(["Quillon"]) == 1
    assert checker.words() == ["Quillon"]
    checker.clear_words()
    assert checker.words() == []
    with pytest.raises(TypeError):
        checker.add_words("Zorbath")


def test_spelling_is_off_by_default(data_dir):
    checker = Checker.from_dir(data_dir)
    assert not any(s.source.startswith("SPELLCHECK") for s in checker.check("I recieve it."))


def test_profiles(data_dir):
    # The fiction profile turns off dialogue informality such as "gonna".
    text = "“I’m gonna go,” she said."
    fiction = Checker.from_dir(data_dir, profile="fiction")
    default = Checker.from_dir(data_dir, profile="default")
    assert len(default.check(text)) > len(fiction.check(text))
    with pytest.raises(ValueError, match="unknown profile"):
        Checker.from_dir(data_dir, profile="no-such-profile")
    with pytest.raises(ValueError, match="spell variant"):
        Checker.from_dir(data_dir, spell_variant="xx_XX")


def test_enable_rules_and_mother_tongue(data_dir):
    checker = Checker.from_dir(data_dir)
    assert checker.enable_rules(["PLAIN_ENGLISH"]) > 0
    with pytest.raises(ValueError):
        checker.enable_rules(["NO_SUCH_RULE_SELECTOR"])
    tongues = checker.mother_tongues()
    assert "de" in tongues
    checker.set_mother_tongue("de")
    checker.set_mother_tongue(None)


def test_load_errors(tmp_path, data_dir):
    with pytest.raises(FileNotFoundError):
        Checker(tmp_path / "missing.bin", data_dir / writemorebetter.RULES_FILE)
    bad = tmp_path / "bad.bin"
    bad.write_bytes(b"not a tokenizer")
    with pytest.raises(WriteMoreBetterError):
        Checker(bad, data_dir / writemorebetter.RULES_FILE)


def test_analyze_offsets(checker):
    text = "🙂 “It’s late—” she said… He left. Done."
    sentences = checker.analyze(text)
    assert sentences
    assert "".join(s["text"] for s in sentences) == text
    for sentence in sentences:
        assert text[sentence["start"] : sentence["end"]] == sentence["text"]
        for token in sentence["tokens"]:
            assert text[token["start"] : token["end"]] == token["text"]
    words = [t for s in sentences for t in s["tokens"] if t["text"] == "said"]
    assert words and any(r["lemma"] == "say" for r in words[0]["readings"])


def test_analyze_document_offsets(checker):
    text = "🙂 “Run,” she said—quietly.\n\nHe ran… and fell. The end."
    document = checker.analyze_document(text)
    assert document["word_count"] > 0
    for paragraph in document["paragraphs"]:
        assert text[paragraph["start"] : paragraph["end"]] == paragraph["text"]
        for sentence in paragraph["sentences"]:
            assert text[sentence["start"] : sentence["end"]] == sentence["text"]
            for token in sentence["tokens"]:
                assert text[token["start"] : token["end"]] == token["text"]
    assert any(text[d["start"] : d["end"]].startswith("“Run") for d in document["dialogue"])
    compact = checker.analyze_document(text, {"include_token_details": False})
    token = compact["paragraphs"][0]["sentences"][0]["tokens"][0]
    assert "readings" not in token
    with pytest.raises(StyleError) as error:
        checker.analyze_document(text, {"no_such_option": True})
    assert error.value.kind == "invalidOptions"


def test_style_reports_use_code_point_spans(checker):
    text = (
        "🙂🙂 She walked slowly—quietly… “Go,” he said softly. "
        "“I’m ready,” she whispered gently. It was what it was."
    )
    document = checker.style_document(text)
    adverbs = document.adverbs()
    lemmas = {m["lemma"]: m for m in adverbs["matches"]}
    assert "slowly" in lemmas and "quietly" in lemmas
    for match in adverbs["matches"]:
        span = match["span"]
        assert text[span["start"] : span["end"]].lower() == match["lemma"]
    stats = document.stats()
    assert stats["paragraphs"] == 1 and stats["words"]["all"] > 0
    # Every report runs, with defaults, on the same analysis.
    for name in [
        "echoes", "repeated_phrases", "cliches", "dialogue_tags", "sentence_length",
        "sentence_starts", "transitions", "pronoun_starts", "sensory_balance", "pacing",
        "diction", "overused_words", "consistency", "readability", "passive", "clarity",
        "regional_spelling", "emotion_tells", "preset_checks", "preset", "alliteration",
        "vague_words", "hidden_verbs",
    ]:
        assert isinstance(getattr(document, name)(), dict), name
    assert isinstance(document.dialogue_tags(baseline="none"), dict)
    with pytest.raises(StyleError) as error:
        document.house_style()
    assert error.value.kind == "invalidRules"
    with pytest.raises(StyleError):
        document.echoes({"no_such_option": 1})


def test_house_style_rules(checker):
    text = "🙂 “Very well,” she said. It was very cold."
    rules = {
        "name": "T.Exist",
        "extends": "existence",
        "message": "Avoid '%s'.",
        "ignorecase": True,
        "tokens": ["very"],
    }
    report = checker.style_document(text).house_style(None, rules)
    found = [text[m["span"]["start"] : m["span"]["end"]] for m in report["matches"]]
    assert found == ["Very", "very"]


def test_presets_and_title_case(data_dir):
    names = writemorebetter.style_preset_names()
    assert names[0] == "chicago-fiction"
    preset = writemorebetter.load_style_preset(names[0])
    assert isinstance(preset, dict)
    with pytest.raises(StyleError):
        writemorebetter.load_style_preset("no-such-preset")
    with pytest.raises(StyleError):
        writemorebetter.parse_style_preset("{\"no_such_key\": 1}")
    checker = Checker.from_dir(data_dir, profile="")
    assert checker.apply_preset(names[0]) >= 0
    report = writemorebetter.title_case(["🙂 the house of the rising sun"])
    assert isinstance(report, dict)


def test_threads_share_one_checker(checker):
    text = "🙂 She was went to the store."
    expected = checker.check(text)
    results = []

    def run():
        for _ in range(5):
            results.append(checker.check(text))

    threads = [threading.Thread(target=run) for _ in range(4)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert len(results) == 20 and all(r == expected for r in results)


def test_profile_none_applies_no_profile(data_dir):
    text = "“I’m gonna go,” she said."
    none = Checker.from_dir(data_dir, profile=None)
    default = Checker.from_dir(data_dir, profile="default")
    assert sources(none.check(text)) == sources(default.check(text))


def test_style_error_attributes(checker):
    document = checker.style_document("🙂 She walked slowly.")
    with pytest.raises(StyleError) as error:
        document.echoes({"no_such_option": 1})
    assert error.value.kind == "invalidOptions"
    assert error.value.report is None or isinstance(error.value.report, str)
    assert isinstance(error.value.message, str) and error.value.message
    # Inputs the binding rejects itself are StyleErrors too, with the same attributes.
    with pytest.raises(StyleError) as error:
        document._report("echoes", None, "none")
    assert (error.value.kind, error.value.report) == ("invalidOptions", "echoes")
    with pytest.raises(StyleError) as error:
        document._report("house_style", None, None)
    assert (error.value.kind, error.value.report) == ("invalidRules", "houseStyle")
    with pytest.raises(StyleError) as error:
        document._report("preset", {"window": 1}, None)
    assert error.value.report == "preset"
    with pytest.raises(StyleError) as error:
        document._report("no_such_report")
    assert (error.value.kind, error.value.report) == ("unknownReport", None)


def test_dictionary_edits_do_not_hold_the_gil(data_dir):
    """add_words waits for a running check's read lock; other Python threads must keep running
    meanwhile, so the heartbeat's longest gap stays well below the wait."""
    checker = Checker.from_dir(data_dir)
    text = "She was went home. " * 1500
    started = threading.Event()

    def long_check():
        started.set()
        checker.check(text)

    worker = threading.Thread(target=long_check)
    worker.start()
    started.wait()
    time.sleep(0.05)
    beats = []
    stop = threading.Event()

    def heartbeat():
        while not stop.is_set():
            beats.append(time.perf_counter())
            time.sleep(0.005)

    beater = threading.Thread(target=heartbeat)
    beater.start()
    time.sleep(0.05)
    before = time.perf_counter()
    checker.add_words(["Zorbath"])
    waited = time.perf_counter() - before
    time.sleep(0.05)
    stop.set()
    beater.join()
    worker.join()
    gaps = [b - a for a, b in zip(beats, beats[1:])]
    assert waited < 0.1 or max(gaps) < 0.5 * waited, (waited, max(gaps))
