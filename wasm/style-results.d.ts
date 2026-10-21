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

export type TransitionType = "contrast" | "cause" | "time" | "addition" | "example";

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

export interface ConsistencyCheck {
  mixed: boolean;
  variants: Array<ConsistencyVariant>;
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
