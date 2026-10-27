export interface Span { start: number; end: number; }
export type Mode = "narration" | "dialogue";
export interface Split<T> { all: T; narration: T; dialogue: T; }
export interface SentenceRange { start: number; end: number; }
export interface PacingParagraphRange { start: number; end: number; }

export type ClicheKind = "cliche" | "redundancy";
export type DialogueTagPosition = "before" | "after" | "split";
export type DialogueTagVerbClass = "speech" | "nonSpeech" | "unlisted";
export type SensoryModality = "sight" | "sound" | "touch" | "smell" | "taste" | "interoception";
export type SentenceStartCategory = "subject" | "adverb" | "gerund" | "infinitive" | "subordinatingConjunction" | "coordinatingConjunction" | "pastParticiple" | "inversion" | "introductory" | "other";
export interface Echo {
  key: string;
  label: string;
  earlier: Span;
  later: Span;
  distance: number;
}

export interface EchoReport {
  echoes: Array<Echo>;
  words: number;
  perThousandWords: number;
}

export interface RepeatedPhrase {
  keys: Array<string>;
  labels: Array<string>;
  count: number;
  spans: Array<Span>;
}

export interface RepeatedPhraseReport {
  words: number;
  phrases: Array<RepeatedPhrase>;
}

export interface OverusedWord {
  lemma: string;
  count: number;
  perTenThousandWords: number;
  referencePerTenThousandWords: number;
  ratio: number;
  spans: Array<Span>;
}

export interface OverusedWordsFiller {
  word: string;
  count: number;
  perTenThousandWords: number;
  referencePerTenThousandWords: number | null;
  ratio: number | null;
  spans: Array<Span>;
}

export interface OverusedWordsReport {
  words: number;
  overused: Array<OverusedWord>;
  fillers: Array<OverusedWordsFiller>;
}

export interface ClicheMatch {
  kind: ClicheKind;
  mode: Mode;
  phrase: string;
  source: string;
  span: Span;
}

export interface ClicheCounts {
  cliches: number;
  redundancies: number;
  words: number;
}

export interface ClicheReport {
  matches: Array<ClicheMatch>;
  groups: Split<ClicheCounts>;
}

export interface DialogueTag {
  span: Span;
  clause: Span;
  verb: Span;
  verbText: string;
  lemma: string;
  class: DialogueTagVerbClass;
  showy: boolean;
  position: DialogueTagPosition;
  adverb: Span | null;
  quotes: Array<Span>;
}

export interface DialogueTagActionBeat {
  span: Span;
  quotes: Array<Span>;
}

export interface DialogueTagUntaggedRun {
  span: Span;
  quotes: Array<Span>;
  exchanges: number;
}

export interface DialogueTagVerbCount {
  lemma: string;
  class: DialogueTagVerbClass;
  count: number;
}

export interface DialogueTagReport {
  words: Split<number>;
  tags: Array<DialogueTag>;
  beats: Array<DialogueTagActionBeat>;
  untaggedRuns: Array<DialogueTagUntaggedRun>;
  otherVerbs: Array<DialogueTagVerbCount>;
  neutralVerbs: Array<DialogueTagVerbCount>;
  showyVerbs: Array<DialogueTagVerbCount>;
  lowConfidence: boolean;
  dialogueSpans: number;
  speechSpans: number;
  taggedSpans: number;
  exchanges: number;
  untaggedExchanges: number;
  saidAsked: Share;
  withAdverb: Share;
  nonSpeech: Share;
}

export interface SentenceLengthBucket {
  words: number;
  sentences: number;
}

export interface SentenceLengthPercentile {
  percentile: number;
  value: number | null;
  difference: number | null;
  highlighted: boolean;
}

export interface SentenceLengthGroup {
  sentences: number;
  zeroWordSentences: number;
  histogram: Array<SentenceLengthBucket>;
  mean: number | null;
  meanDifference: number | null;
  meanHighlighted: boolean;
  lowConfidence: boolean;
  percentiles: Array<SentenceLengthPercentile>;
  longSentences: Array<number>;
  monotonyRuns: Array<SentenceRun>;
}

export interface SentenceLengthReport {
  sentences: Array<SentenceRow>;
  words: Split<number>;
  groups: Split<SentenceLengthGroup>;
}

export interface SentenceStart {
  sentence: number;
  opener: Span | null;
  word: string | null;
  category: SentenceStartCategory;
  ambiguous: boolean;
  universalCandidates: Array<string>;
}

export interface SentenceStartCategoryShare {
  category: SentenceStartCategory;
  count: number;
  percent: number | null;
  difference: number | null;
  highlighted: boolean;
}

export interface SentenceStartUniversalShare {
  tag: string;
  weight: number;
  percent: number | null;
  difference: number | null;
  highlighted: boolean;
}

export interface SentenceStartsGroup {
  sentences: number;
  lowConfidence: boolean;
  excludedSentences: number;
  ambiguousStarts: number;
  unknownStarts: number;
  categories: Array<SentenceStartCategoryShare>;
  universal: Array<SentenceStartUniversalShare>;
  sameWordRuns: Array<SentenceRun>;
}

export interface SentenceStartsReport {
  sentences: Array<SentenceRow>;
  words: Split<number>;
  starts: Array<SentenceStart>;
  groups: Split<SentenceStartsGroup>;
}

export interface Share {
  count: number;
  percent: number | null;
  difference: number | null;
  highlighted: boolean;
}

export interface SentenceRow {
  span: Span;
  mode: Mode;
  words: number;
}

export interface SentenceRun {
  span: Span;
  sentences: SentenceRange;
}

export interface DocumentStats {
  paragraphs: number;
  hardWrapped: boolean;
  tokenDetails: boolean;
  words: Split<number>;
  sentences: Split<number>;
  dialogueSpans: number;
  speechSpans: number;
}

export interface SenseWord {
  span: Span;
  lemma: string;
  modality: SensoryModality;
  strength: number;
  mode: Mode;
}

export interface SensoryShare {
  modality: SensoryModality;
  count: number;
  percent: number | null;
}

export interface SensoryGroup {
  words: number;
  senseWords: number;
  modalities: Array<SensoryShare>;
}

export interface SensoryUnit {
  span: Span;
  chapter: number;
  groups: Split<SensoryGroup>;
}

export interface SensoryBalanceReport {
  words: Split<number>;
  senseWords: Array<SenseWord>;
  groups: Split<SensoryGroup>;
  chapters: Array<SensoryUnit>;
  scenes: Array<SensoryUnit>;
}

export interface DictionSentence {
  span: Span;
  mode: Mode;
  words: number;
  ratedWords: number;
  mean: number | null;
  abstract: boolean;
}

export interface DictionParagraph {
  span: Span;
  sentences: SentenceRange;
  ratedWords: number;
  mean: number | null;
}

export interface DictionGroup {
  sentences: number;
  ratedWords: number;
  mean: number | null;
  abstractSentences: number;
}

export interface DictionReport {
  words: Split<number>;
  tableWords: number;
  sentences: Array<DictionSentence>;
  paragraphs: Array<DictionParagraph>;
  groups: Split<DictionGroup>;
  abstractRuns: Array<SentenceRun>;
}

export type PacingSignal = "sentenceLength" | "pastPerfect" | "cognition";
export interface PacingSignals {
  words: Split<number>;
  sentences: number;
  verbPhrases: number;
  pastPerfect: number;
  cognition: number;
  dialoguePercent: number | null;
  meanSentenceLength: number | null;
  pastPerfectPercent: number | null;
  cognitionPercent: number | null;
}
export interface PacingParagraph {
  span: Span;
  sceneBreakBefore: boolean;
  signals: PacingSignals;
  hasSpeech: boolean;
  triggeredSignals: Array<PacingSignal>;
}
export interface PacingRun {
  span: Span;
  paragraphs: PacingParagraphRange;
  signals: PacingSignals;
  triggeredSignals: Array<PacingSignal>;
}
export interface PacingReport {
  words: Split<number>;
  paragraphs: Array<PacingParagraph>;
  slowRuns: Array<PacingRun>;
}

export type TransitionType = "contrast" | "cause" | "time" | "addition" | "example" | "condition";

export interface TransitionMatch {
  span: Span;
  sentence: number;
  phrase: string;
  kind: TransitionType;
  opening: boolean;
  mode: Mode;
}

export interface TransitionRepeat {
  span: Span;
  phrase: string;
  spans: Array<Span>;
  sentences: Array<number>;
  mode: Mode;
}

export interface TransitionTypeShare {
  kind: TransitionType;
  containing: Share;
  opening: Share;
}

export interface TransitionsGroup {
  sentences: number;
  lowConfidence: boolean;
  containing: Share;
  opening: Share;
  types: Array<TransitionTypeShare>;
}

export interface TransitionsReport {
  matches: Array<TransitionMatch>;
  repeats: Array<TransitionRepeat>;
  groups: Split<TransitionsGroup>;
}

export interface PronounStart {
  span: Span;
  sentence: number;
  pronoun: string;
  mode: Mode;
}

export interface PronounStartsRun {
  span: Span;
  pronoun: string;
  sentences: SentenceRange;
  mode: Mode;
}

export interface PronounStartsShare {
  pronoun: string;
  count: number;
  percent: number | null;
  difference: number | null;
  highlighted: boolean;
}

export interface PronounStartsGroup {
  sentences: number;
  excludedSentences: number;
  lowConfidence: boolean;
  pronounStarts: Share;
  pronouns: Array<PronounStartsShare>;
  runs: Array<PronounStartsRun>;
  words: number;
  pronounWords: Share;
  wordPronouns: Array<PronounStartsShare>;
}

export interface PronounStartsReport {
  starts: Array<PronounStart>;
  groups: Split<PronounStartsGroup>;
}

export interface ConsistencyVariant {
  form: string;
  counts: Split<number>;
  spans: Array<Span>;
}

export type SpellingSystem = "american" | "british" | "oxford";
export type RegionalSpellingReason = "system" | "form";

export interface SpellingSystemCount {
  system: SpellingSystem;
  count: number;
}

export interface RegionalSpellingForm {
  form: string;
  systems: Array<SpellingSystem>;
  counts: Split<number>;
  spans: Array<Span>;
}

export interface ConsistencyCheck {
  mixed: boolean;
  variants: Array<ConsistencyVariant>;
  /** The preferred form from the options (a style preset), or null without one. */
  preferred: string | null;
  /** Occurrences of every form other than `preferred`; 0 without a preference. */
  nonPreferred: number;
}

export interface ConsistencyCount {
  form: string;
  counts: Split<number>;
}

export interface ConsistencyWord {
  word: string;
  variants: Array<ConsistencyVariant>;
}

export interface ConsistencyReport {
  doubleQuotes: ConsistencyCheck;
  singleQuotes: ConsistencyCheck;
  apostrophes: ConsistencyCheck;
  ellipses: ConsistencyCheck;
  dashes: ConsistencyCheck;
  dashCharacters: Array<ConsistencyCount>;
  capitalization: Array<ConsistencyWord>;
  hyphenation: Array<ConsistencyWord>;
}

export interface ReadabilityScores {
  sentences: number;
  words: number;
  syllables: number;
  characters: number;
  polysyllables: number;
  lowConfidence: boolean;
  fleschReadingEase: number | null;
  fleschKincaidGrade: number | null;
  colemanLiau: number | null;
  automatedReadability: number | null;
  readingMinutes: number;
}

export interface ReadabilityParagraph {
  span: Span;
  dialogueOnly: boolean;
  excluded: boolean;
  scores: ReadabilityScores;
}

export interface ReadabilityReport {
  groups: Split<ReadabilityScores>;
  paragraphs: Array<ReadabilityParagraph>;
}

export interface AdverbMatch {
  span: Span;
  lemma: string;
  mode: Mode;
  tagAdverb: boolean;
}

export interface AdverbLemma {
  lemma: string;
  count: number;
  perThousandWords: number;
}

export interface AdverbGroup {
  words: number;
  count: number;
  perThousandWords: number;
  lemmas: Array<AdverbLemma>;
}

export interface AdverbsReport {
  matches: Array<AdverbMatch>;
  groups: Split<AdverbGroup>;
}

export type PassiveAuxiliary = "be" | "get";

export interface PercentileBand {
  p5: number;
  p95: number;
}

export interface PassiveMatch {
  span: Span;
  participle: Span;
  sentence: number;
  auxiliary: PassiveAuxiliary;
  progressive: boolean;
  agent: boolean;
  mode: Mode;
}

export interface PassiveGroup {
  sentences: number;
  lowConfidence: boolean;
  passives: number;
  perHundredSentences: number | null;
  band: PercentileBand | null;
  highlighted: boolean;
}

export interface PassiveReport {
  passives: Array<PassiveMatch>;
  groups: Split<PassiveGroup>;
}

export type ClarityClauseIntroducer = "subordinator" | "relative";

export interface ClarityStickySentence {
  span: Span;
  sentence: number;
  mode: Mode;
  words: number;
  glueWords: number;
  gluePercent: number;
  glue: Array<Span>;
}

export interface ClarityLongClause {
  span: Span;
  sentence: number;
  mode: Mode;
  introducer: string;
  introducerSpan: Span;
  kind: ClarityClauseIntroducer;
  words: number;
  sentenceWords: number;
}

export interface ClarityGroup {
  sentences: number;
  eligibleSentences: number;
  lowConfidence: boolean;
  glue: Share;
  sticky: Share;
  subordinateClauses: number;
  longClauses: number;
  longClauseSentences: Share;
}

export interface ClarityReport {
  words: Split<number>;
  stickySentences: Array<ClarityStickySentence>;
  longClauses: Array<ClarityLongClause>;
  groups: Split<ClarityGroup>;
}

export type HouseStyleExtends = "existence" | "substitution" | "consistency" | "occurrence";
export type HouseStyleLevel = "suggestion" | "warning" | "error";
export interface HouseStyleMatch {
  rule: string;
  extends: HouseStyleExtends;
  level: HouseStyleLevel;
  message: string;
  span: Span;
  mode: Mode;
  replacement: string | null;
  count: number | null;
  link: string | null;
}

export interface HouseStyleCounts {
  suggestions: number;
  warnings: number;
  errors: number;
  words: number;
}

export interface HouseStyleRuleError {
  rule: string;
  message: string;
}

export interface HouseStyleReport {
  rules: number;
  unsupported: Array<HouseStyleRuleError>;
  matches: Array<HouseStyleMatch>;
  groups: Split<HouseStyleCounts>;
}

export interface RegionalSpellingWord {
  word: string;
  mixed: boolean;
  forms: Array<RegionalSpellingForm>;
}

export interface RegionalSpellingFlag {
  span: Span;
  sentence: number;
  form: string;
  word: string;
  systems: Array<SpellingSystem>;
  reason: RegionalSpellingReason;
  dominantForm: string | null;
  mode: Mode;
}

export interface RegionalSpellingReport {
  markers: number;
  systems: Array<SpellingSystemCount>;
  dominant: Array<SpellingSystem>;
  words: Array<RegionalSpellingWord>;
  flags: Array<RegionalSpellingFlag>;
}

export type NameSpellingReason = "edit" | "phonetic" | "diacritics" | "case" | "spacing";

export interface NameSpellingFlag {
  span: Span;
  sentence: number;
  form: string;
  name: string;
  alternatives: Array<string>;
  reason: NameSpellingReason;
  distance: number | null;
  mode: Mode;
}

export interface NameSpellingVariant {
  form: string;
  reason: NameSpellingReason;
  counts: Split<number>;
  spans: Array<Span>;
}

export interface NameSpellingName {
  name: string;
  counts: Split<number>;
  variants: Array<NameSpellingVariant>;
  possibleCharacters: Array<NameSpellingVariant>;
}

export interface NameSpellingReport {
  flags: Array<NameSpellingFlag>;
  names: Array<NameSpellingName>;
}

export type EmotionTellPattern = "predicate" | "object" | "subject";

export interface EmotionTell {
  span: Span;
  emotion: Span;
  verb: Span;
  word: string;
  pattern: EmotionTellPattern;
  sentence: number;
  mode: Mode;
}

export interface EmotionTellsGroup {
  tells: number;
  words: number;
  perThousandWords: number | null;
}

export interface EmotionTellsReport {
  tells: Array<EmotionTell>;
  groups: Split<EmotionTellsGroup>;
}

export type QuotePunctuation = "inside" | "outside";
export type Possessives = "apostropheS" | "apostropheOnly";
export type TimeAbbreviations = "dotted" | "undotted";
// Inputs are type aliases rather than interfaces: style-records-test.mjs checks every interface
// against a result record.
export type NumberCheck = {
  spellOutThrough?: number;
  agesAsNumerals?: boolean;
  matchLarger?: boolean;
};
/** Options for `presetChecks`, and a preset's `checks`. Every check is off by default. */
export type PresetChecks = {
  numbers?: NumberCheck | null;
  quotePunctuation?: QuotePunctuation | null;
  possessives?: Possessives | null;
  timeAbbreviations?: TimeAbbreviations | null;
  familyTitles?: boolean;
};

export type PresetCheck = "numbers" | "quotePunctuation" | "possessives" | "timeAbbreviations" | "familyTitles";
export interface PresetCheckMatch {
  check: PresetCheck;
  message: string;
  span: Span;
  mode: Mode;
  replacement: string | null;
}

export interface PresetChecksReport {
  matches: Array<PresetCheckMatch>;
  counts: Split<number>;
}

export type ConsistencyPreferred = {
  doubleQuotes?: "curly" | "straight";
  singleQuotes?: "curly" | "straight";
  apostrophes?: "curly" | "straight";
  ellipses?: "character" | "periods" | "spacedPeriods";
  dashes?: "emClosed" | "emSpaced" | "enSpaced" | "doubleHyphen" | "spacedHyphen";
};

/** A resolved style preset, as `loadStylePreset` and `parseStylePreset` return it. */
export type StylePreset = {
  format: 1;
  name: string;
  version?: string;
  description?: string;
  grammar: {
    profile?: string;
    /** Selector → on (true) or off (false); every false applies before every true. */
    rules: Record<string, boolean>;
    spellVariant?: string;
  };
  consistency: { minimumMinority: number; preferred: ConsistencyPreferred };
  houseStyle: {
    options: { skipDialogue: boolean };
    /** Rule name → Vale rule. */
    rules: Record<string, Record<string, unknown>>;
    /** A Vale bundle as YAML or JSON text. */
    vale?: string;
  };
  checks: PresetChecks;
  /** How headings are capitalized; unset means Chicago-style headline case. */
  titleCase?: TitleCaseStyle;
};

export interface PresetReport {
  preset: string;
  version: string | null;
  consistency: ConsistencyReport;
  houseStyle: HouseStyleReport;
  checks: PresetChecksReport;
}

/** Options for `alliteration`. */
export type AlliterationOptions = {
  minimumWords?: number;
  maximumGap?: number;
  skipDialogue?: boolean;
};

/** `sound` is the shared first sound, a lowercase ARPAbet consonant label (`k`, `sh`, `jh`, …). */
export interface AlliterationRun {
  span: Span;
  sound: string;
  spans: Array<Span>;
  words: Array<string>;
  sentence: number;
  mode: Mode;
}

export interface AlliterationGroup {
  words: number;
  runs: number;
  perThousandWords: number;
}

export interface AlliterationReport {
  runs: Array<AlliterationRun>;
  groups: Split<AlliterationGroup>;
}

export type TitleCaseStyle = "chicago" | "ap" | "sentence";
/** Options for `titleCase`. */
export type TitleCaseOptions = {
  style?: TitleCaseStyle;
  skipNames?: Array<string>;
};
export type TitleCaseRule = "firstWord" | "lastWord" | "afterColon" | "majorWord" | "minorWord" | "sentenceCase";

export interface TitleCaseMatch {
  heading: number;
  span: Span;
  word: string;
  replacement: string;
  rule: TitleCaseRule;
}

export interface TitleCaseReport {
  style: TitleCaseStyle;
  headings: number;
  flaggedHeadings: number;
  matches: Array<TitleCaseMatch>;
}

export type VagueWordCategory = "filler" | "hedge" | "quantity" | "noun" | "adjective";

export interface VagueWord {
  span: Span;
  sentence: number;
  term: string;
  category: VagueWordCategory;
  mode: Mode;
}

export interface VagueWordTerm {
  term: string;
  category: VagueWordCategory;
  count: number;
  narration: number;
  dialogue: number;
}

export interface VagueWordsGroup {
  matches: number;
  words: number;
  perThousandWords: number | null;
}

export interface VagueWordsReport {
  matches: Array<VagueWord>;
  terms: Array<VagueWordTerm>;
  groups: Split<VagueWordsGroup>;
}

export type HiddenVerbPattern = "lightVerb" | "ofPhrase";

export interface HiddenVerb {
  span: Span;
  noun: Span;
  lightVerb: Span | null;
  nominalization: string;
  term: string;
  pattern: HiddenVerbPattern;
  sentence: number;
  mode: Mode;
}

export interface HiddenVerbTerm {
  term: string;
  nominalization: string;
  count: number;
  narration: number;
  dialogue: number;
}

export interface HiddenVerbsGroup {
  matches: number;
  words: number;
  perThousandWords: number | null;
}

export interface HiddenVerbsReport {
  matches: Array<HiddenVerb>;
  terms: Array<HiddenVerbTerm>;
  groups: Split<HiddenVerbsGroup>;
}

/**
 * Options for `drafting`. Edits are offered only when `dictated` is true (the host knows the text
 * came from speech-to-text); typed text is flag only. `spokenCommands` follows `dictated` when
 * unset, and `true` without `dictated` is `invalidOptions`.
 */
export type DraftingOptions = {
  placeholders?: boolean;
  dictated?: boolean;
  spokenCommands?: boolean;
  filledPauses?: boolean;
  er?: boolean;
  includeDialogue?: boolean;
};
export type DraftingCategory = "placeholder" | "spokenCommand" | "filledPause" | "restart" | "selfCorrection";
export interface DraftingEdit { span: Span; replacement: string; }
export interface DraftingMatch { span: Span; sentence: number; category: DraftingCategory; term: string; mode: Mode; edits: Array<DraftingEdit>; }
export interface DraftingTerm { term: string; category: DraftingCategory; count: number; narration: number; dialogue: number; }
export interface DraftingGroup { matches: number; edits: number; words: number; perThousandWords: number | null; }
export interface DraftingReport { matches: Array<DraftingMatch>; terms: Array<DraftingTerm>; groups: Split<DraftingGroup>; }
