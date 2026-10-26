"""English grammar, spelling and fiction style checking without a JVM.

writemorebetter runs LanguageTool's open English rule set in Rust. LanguageTool is the source of
the rules; this project is not affiliated with LanguageTool or LanguageTooler GmbH.

The wheel holds engine code only (MIT OR Apache-2.0). The compiled rule and dictionary data,
``en_tokenizer.bin`` and ``en_rules.bin``, are LGPL-2.1 and ship separately in
``writemorebetter-data-<version>.zip`` on the GitHub release; pass their paths (or the directory
holding them) when constructing a :class:`Checker`.

Every offset is a Python string index (a code point), so ``text[s.start:s.end]`` is the flagged
span even after emoji, curly quotes or other non-ASCII text.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from typing import Any, Iterable, List, Mapping, Optional, Tuple, Union

from . import _native
from ._native import StyleError, WriteMoreBetterError

__all__ = [
    "Checker",
    "DocumentResolution",
    "ParagraphCheck",
    "StyleDocument",
    "StyleError",
    "Suggestion",
    "WriteMoreBetterError",
    "TOKENIZER_FILE",
    "RULES_FILE",
    "load_style_preset",
    "parse_style_preset",
    "style_preset_names",
    "title_case",
]

#: The tokenizer data file's name in the release data zip.
TOKENIZER_FILE = "en_tokenizer.bin"
#: The rules data file's name in the release data zip.
RULES_FILE = "en_rules.bin"

PathLike = Union[str, "os.PathLike[str]"]
#: Report options: a mapping (encoded as JSON) or a JSON string.
Options = Union[Mapping[str, Any], str, None]
#: A report baseline: ``None`` for the bundled one, a selector string such as ``"none"`` or
#: ``"publicStandIn"``, a JSON string, or a mapping.
Baseline = Union[Mapping[str, Any], str, None]


@dataclass(frozen=True)
class Suggestion:
    """One flagged span. ``start`` and ``end`` (exclusive) are code-point indices into the text."""

    start: int
    end: int
    #: The rule that fired, e.g. ``"GRAMMAR/WAS_BEEN/1"`` or ``"SPELLCHECK/SINGLE"``.
    source: str
    #: Human-readable explanation.
    message: str
    #: Suggested replacements, possibly empty.
    replacements: Tuple[str, ...]


def _json_arg(value: Optional[Union[Mapping[str, Any], str]]) -> Optional[str]:
    if value is None or isinstance(value, str):
        return value
    return json.dumps(value)


def _words(words: Iterable[str]) -> List[str]:
    if isinstance(words, str):
        raise TypeError("pass an iterable of words, not a single string")
    return list(words)


class Checker:
    """A grammar and spelling checker over the loaded data files.

    Loading deserializes two multi-megabyte binaries, so construct one checker and reuse it.
    The checker is thread-safe: checks and analyses release the GIL, and the user-dictionary
    and rule methods apply to the next check.

    :param tokenizer_path: path to ``en_tokenizer.bin``.
    :param rules_path: path to ``en_rules.bin``.
    :param spell_variant: a speller dictionary such as ``"en_US"`` or ``"en_GB"``; ``None`` (the
        default) or ``""`` leaves spelling off.
    :param profile: a rule profile. ``"fiction"`` (the default, as in Swift) turns off rules that
        misfire on fiction voice; ``None``, ``""`` or ``"default"`` disables nothing (WASM's
        default). An unknown name raises :class:`ValueError`.
    :raises OSError: a data file cannot be read.
    :raises WriteMoreBetterError: a data file is corrupt or from another version.
    """

    def __init__(
        self,
        tokenizer_path: PathLike,
        rules_path: PathLike,
        *,
        spell_variant: Optional[str] = None,
        profile: Optional[str] = "fiction",
    ) -> None:
        self._native = _native.Checker(
            os.fspath(tokenizer_path), os.fspath(rules_path), spell_variant, profile
        )

    @classmethod
    def from_dir(
        cls,
        data_dir: PathLike,
        *,
        spell_variant: Optional[str] = None,
        profile: Optional[str] = "fiction",
    ) -> "Checker":
        """Loads ``en_tokenizer.bin`` and ``en_rules.bin`` from one directory, such as the
        unpacked release data zip."""
        data_dir = os.fspath(data_dir)
        return cls(
            os.path.join(data_dir, TOKENIZER_FILE),
            os.path.join(data_dir, RULES_FILE),
            spell_variant=spell_variant,
            profile=profile,
        )

    def check(self, text: str) -> List[Suggestion]:
        """Checks ``text`` and returns every flagged span, in document order.

        Pass one paragraph per call where you can: a heading or a line without terminal
        punctuation otherwise runs into the next line's sentence.
        """
        return [
            Suggestion(start, end, source, message, tuple(replacements))
            for start, end, source, message, replacements in self._native.check(text)
        ]

    def analyze(self, text: str) -> List[dict]:
        """Splits ``text`` into sentences of tagged tokens (lemma/POS readings, chunks), without
        grammar or spelling checks. Tokens keep their surface text and include whitespace;
        ``start``/``end`` are code-point offsets into ``text``."""
        return json.loads(self._native.analyze_json(text))

    def analyze_document(self, text: str, options: Options = None) -> dict:
        """Analyzes a whole document into paragraphs, sentences and tokens, with dialogue spans
        and a lemma index. ``options`` are ``DocumentOptions`` (``skip_names``,
        ``include_token_details``, ``hard_wrapped``). Offsets are code points."""
        return json.loads(self._native.analyze_document_json(text, _json_arg(options)))

    def style_document(self, text: str, options: Options = None) -> "StyleDocument":
        """Analyzes ``text`` once and returns a :class:`StyleDocument` whose reports reuse that
        analysis. ``options`` are the same ``DocumentOptions`` as :meth:`analyze_document`."""
        return StyleDocument(self._native.style_document(text, _json_arg(options)))

    # User dictionary (#86)

    def add_words(self, words: Iterable[str]) -> int:
        """Adds words to the user dictionary and returns how many were new.

        Use it for character names, invented places and coinages. Entries may be single words,
        whitespace-separated phrases (``"Port Royal"``) or words with an internal apostrophe;
        curly and straight apostrophes match each other. Matching is case-sensitive except that
        a lowercase entry also accepts its sentence-initial capitalized form, and every entry
        accepts a possessive. The caller owns persistence.
        """
        return self._native.add_words(_words(words))

    def remove_words(self, words: Iterable[str]) -> int:
        """Removes words from the user dictionary and returns how many were present."""
        return self._native.remove_words(_words(words))

    def set_words(self, words: Iterable[str]) -> int:
        """Replaces the whole user dictionary and returns the number of entries now set."""
        return self._native.set_words(_words(words))

    def clear_words(self) -> None:
        """Removes every word from the user dictionary."""
        self._native.clear_words()

    def words(self) -> List[str]:
        """Lists the user dictionary, canonicalized and sorted."""
        return self._native.words()

    # Rules

    def set_mother_tongue(self, mother_tongue: Optional[str]) -> None:
        """Selects the writer's mother tongue (e.g. ``"de"``) so the false-friend rules for
        speakers of that language run; ``None`` or ``""`` turns them off (the default)."""
        self._native.set_mother_tongue(mother_tongue)

    def mother_tongues(self) -> List[str]:
        """Lists the mother tongues the rules have false-friend rules for."""
        return self._native.mother_tongues()

    def enable_rules(self, selectors: Iterable[str]) -> int:
        """Enables rules that are off by default by the selector syntax profiles use
        (``"PLAIN_ENGLISH"``, ``"STYLE/PASSIVE_VOICE"``, ``"BUILTIN/EN_REDUNDANCY_REPLACE"``).
        Returns how many rules changed from off to on. A selector that matches no rule raises
        :class:`ValueError` and nothing is enabled."""
        return self._native.enable_rules(_words(selectors))

    def apply_preset(self, preset: Union[str, Mapping[str, Any]]) -> int:
        """Applies a style preset's grammar settings (profile, rules off and on, speller
        variant). ``preset`` is a built-in preset's name, a preset file's JSON text, or a
        mapping. Returns how many rules went from off to on. Apply a preset to a new checker;
        profiles cannot be undone."""
        return self._native.apply_preset(_json_arg(preset))

    def set_too_often_used_percent(self, percent: int) -> None:
        """Sets the minimum share (1 to 100, default 5) of all the adjectives, nouns or verbs of
        the document from which the ``TOO_OFTEN_USED_*`` rules flag a word. It does not enable
        the rules, which are off by default. Out of range raises :class:`ValueError`, or
        :class:`OverflowError` for a negative ``percent``."""
        self._native.set_too_often_used_percent(percent)

    def set_readability_level(self, level: int) -> None:
        """Sets the level (0 very difficult to 6 very easy, default 3) the
        ``READABILITY_RULE_SIMPLE`` and ``READABILITY_RULE_DIFFICULT`` rules compare each
        paragraph's Flesch reading level with: SIMPLE flags a paragraph above it, DIFFICULT one
        below it. It does not enable the rules, which are off by default. Above 6 raises
        :class:`ValueError`, a negative ``level`` :class:`OverflowError`; nothing changes."""
        self._native.set_readability_level(level)

    # Incremental checking (#329)

    def check_paragraph(self, text: str, *, ends_paragraph: bool = True) -> "ParagraphCheck":
        """Checks one paragraph of a larger document on its own, for an editor that rechecks
        only the paragraphs that changed.

        Returns the paragraph's suggestions, without those of the rules that need the whole
        document (``EN_CONSISTENT_APOS``, ``EN_WORD_COHERENCY``, ``EN_REPEATEDWORDS``,
        ``TOO_OFTEN_USED_*``), and its digest: a few bytes per word that those rules need, a
        pure function of ``text``. Keep the digest with the paragraph, replace both when the
        paragraph changes, and pass the digests of all paragraphs to :meth:`resolve_document`.
        ``ends_paragraph`` (the default) says the paragraph ends at the end of ``text``, for
        paragraphs stored without the blank line after them; pass ``False`` for slices of a
        document that keep their line breaks. Offsets are code points into ``text``.
        """
        suggestions, digest = self._native.check_paragraph(text, ends_paragraph)
        return ParagraphCheck(
            [Suggestion(s, e, src, msg, tuple(r)) for s, e, src, msg, r in suggestions], digest
        )

    def resolve_document(self, digests: Iterable[bytes]) -> "DocumentResolution":
        """Runs the whole-document rules over the digests of every paragraph, in document
        order. Run it again after any paragraph changes: one edit can add or remove matches in
        paragraphs that did not change. Digests from another engine version, or bytes that are
        not a digest, are listed in :attr:`DocumentResolution.stale`."""
        return DocumentResolution(self._native.resolve_document(list(digests)))


@dataclass(frozen=True)
class ParagraphCheck:
    """One paragraph checked by :meth:`Checker.check_paragraph`."""

    #: The paragraph's suggestions without the whole-document rules', code points into it.
    suggestions: List[Suggestion]
    #: What the whole-document rules need from the paragraph; store it with the paragraph.
    digest: bytes


class DocumentResolution:
    """The whole-document rules' matches over a document's digests, from
    :meth:`Checker.resolve_document`. It replaces the previous resolution of the document."""

    def __init__(self, native: "_native.DocumentResolution") -> None:
        self._native = native

    @property
    def stale(self) -> List[int]:
        """The indices of the digests that are from another engine version or not digests:
        check those paragraphs again and resolve again. While any are listed, matches can be
        missing in every paragraph."""
        return self._native.stale()

    @property
    def paragraphs(self) -> List[int]:
        """The indices of the paragraphs that have whole-document matches, ascending."""
        return self._native.paragraphs()

    def suggestions(
        self, paragraph: int, text: str, checked: Iterable[Suggestion]
    ) -> List[Suggestion]:
        """The suggestions of paragraph ``paragraph``, whose text is ``text``: ``checked``, the
        suggestions :meth:`Checker.check_paragraph` returned for it, merged with its
        whole-document matches. A match whose span no longer fits ``text`` is left out, while a
        ``checked`` suggestion whose span does not fit it raises :class:`ValueError` (the Swift
        binding leaves such a suggestion out instead). Offsets are code points into ``text``;
        the result equals what :meth:`Checker.check` reports for the whole-document rules on the
        joined document."""
        native = [(s.start, s.end, s.source, s.message, list(s.replacements)) for s in checked]
        return [
            Suggestion(s, e, src, msg, tuple(r))
            for s, e, src, msg, r in self._native.suggestions(paragraph, text, native)
        ]


class StyleDocument:
    """One retained analysis that every style report reads; build it with
    :meth:`Checker.style_document`.

    Each report returns a ``dict`` (the same records as the WASM and Swift bindings, documented
    in ``wasm/style-results.d.ts``) whose spans are ``{"start", "end"}`` code-point ranges into
    the analyzed text. Options default when omitted; any bad report input (options, baseline,
    table, rules, or a second input the report does not take) raises :class:`StyleError`, whose
    ``kind``, ``report`` (``None`` when no report is named) and ``message`` are always set.
    """

    def __init__(self, native: "_native.StyleDocument") -> None:
        self._native = native

    def _report(self, name: str, options: Options = None, extra: Any = None) -> dict:
        return json.loads(self._native.report_json(name, _json_arg(options), _json_arg(extra)))

    def stats(self) -> dict:
        """Counts for the retained analysis (``DocumentStats``)."""
        return json.loads(self._native.stats_json())

    def echoes(self, options: Options = None) -> dict:
        return self._report("echoes", options)

    def repeated_phrases(self, options: Options = None) -> dict:
        return self._report("repeated_phrases", options)

    def cliches(self, options: Options = None) -> dict:
        return self._report("cliches", options)

    def dialogue_tags(self, options: Options = None, baseline: Baseline = None) -> dict:
        """``baseline``: ``None`` for the bundled one, ``"none"`` to skip comparisons, or a
        baseline object."""
        return self._report("dialogue_tags", options, baseline)

    def sentence_length(self, options: Options = None, baseline: Baseline = None) -> dict:
        return self._report("sentence_length", options, baseline)

    def sentence_starts(self, options: Options = None, baseline: Baseline = None) -> dict:
        return self._report("sentence_starts", options, baseline)

    def transitions(self, options: Options = None) -> dict:
        return self._report("transitions", options)

    def pronoun_starts(self, options: Options = None) -> dict:
        return self._report("pronoun_starts", options)

    def adverbs(self, options: Options = None) -> dict:
        return self._report("adverbs", options)

    def sensory_balance(self, options: Options = None) -> dict:
        return self._report("sensory_balance", options)

    def pacing(self, options: Options = None) -> dict:
        return self._report("pacing", options)

    def diction(
        self, options: Options = None, table: Union[Mapping[str, float], str, None] = None
    ) -> dict:
        """``table``: a word-to-concreteness mapping, JSON or TSV text; ``None`` uses the
        bundled table."""
        return self._report("diction", options, table)

    def overused_words(self, options: Options = None, baseline: Baseline = None) -> dict:
        return self._report("overused_words", options, baseline)

    def consistency(self, options: Options = None) -> dict:
        return self._report("consistency", options)

    def readability(self, options: Options = None) -> dict:
        return self._report("readability", options)

    def passive(self, options: Options = None, baseline: Baseline = None) -> dict:
        """``baseline``: ``None``, ``"bundled"`` and ``"none"`` skip comparisons (no published
        band yet); ``"publicStandIn"`` selects the provisional stand-in band."""
        return self._report("passive", options, baseline)

    def clarity(self, options: Options = None) -> dict:
        return self._report("clarity", options)

    def house_style(
        self, options: Options = None, rules: Union[Mapping[str, Any], str, None] = None
    ) -> dict:
        """Runs caller-supplied house-style rules (Vale-format YAML or JSON text, or a mapping).
        Omitting the rules raises :class:`StyleError` (``invalidRules``)."""
        return self._report("house_style", options, rules)

    def regional_spelling(self, options: Options = None) -> dict:
        return self._report("regional_spelling", options)

    def name_spelling(self, options: Options = None) -> dict:
        return self._report("name_spelling", options)

    def emotion_tells(self, options: Options = None) -> dict:
        return self._report("emotion_tells", options)

    def preset_checks(self, options: Options = None) -> dict:
        return self._report("preset_checks", options)

    def preset(self, preset: Union[str, Mapping[str, Any], None] = None) -> dict:
        """Runs every report a style preset drives. ``preset`` is a built-in preset's name, a
        preset file's JSON text or a mapping; ``None`` selects ``"chicago-fiction"``."""
        return self._report("preset", None, preset)

    def alliteration(self, options: Options = None) -> dict:
        return self._report("alliteration", options)

    def vague_words(self, options: Options = None) -> dict:
        return self._report("vague_words", options)

    def hidden_verbs(self, options: Options = None) -> dict:
        return self._report("hidden_verbs", options)


def style_preset_names() -> List[str]:
    """The built-in style presets' names; the first, ``"chicago-fiction"``, is the default."""
    return _native.style_preset_names()


def load_style_preset(name: str) -> dict:
    """A built-in style preset by name, resolved to a self-contained ``StylePreset``."""
    return json.loads(_native.load_style_preset_json(name))


def parse_style_preset(preset: Union[str, Mapping[str, Any]]) -> dict:
    """Validates a preset file (JSON text or a mapping), resolves its ``extends`` and returns
    the self-contained ``StylePreset``. Invalid presets raise :class:`StyleError`."""
    return json.loads(_native.parse_style_preset_json(_json_arg(preset)))


def title_case(headings: Iterable[str], options: Options = None) -> dict:
    """Checks headings such as chapter titles against a title-case style (Chicago headline case
    by default). Spans are code-point offsets into each heading."""
    return json.loads(_native.title_case_json(_words(headings), _json_arg(options)))
