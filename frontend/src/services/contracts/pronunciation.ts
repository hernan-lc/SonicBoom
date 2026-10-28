/** Pronunciation (text normalization) mode: `auto` converts numerals, dates, etc.; `off` speaks the text as typed. */
export type NormalizationMode = 'auto' | 'off';
/** How written numbers and times are read: compatibility, conversational or digit by digit. */
export type ReadingStyle = 'precise' | 'natural' | 'digits';
export type CoverageLevel = 'full' | 'partial' | 'none';
/** One speech language in the `normalization` block of `GET /api/info`. */
export interface NormalizationLanguage {
  language: string;
  locales: string[];
  default_locale: string;
  rules: CoverageLevel;
  features: Record<string, CoverageLevel>;
}
export interface NormalizationInfo {
  ruleset_version: string;
  default_mode: NormalizationMode;
  default_locale?: string;
  default_reading_style?: ReadingStyle;
  enabled?: boolean;
  languages: NormalizationLanguage[];
}
/** Optional synthesis query fields. Absent fields are never sent, so legacy requests are unchanged. */
export interface SpeechPronunciation {
  normalization?: NormalizationMode;
  locale?: string;
  reading_style?: ReadingStyle;
}
export interface NormalizeRequest {
  text: string;
  language: string;
  locale?: string;
  mode?: NormalizationMode;
  reading_style?: ReadingStyle;
  model?: string;
}
/** `start`/`end` are UTF-8 byte offsets into the original text: display `before`/`after`, never slice with them. */
export interface NormalizationChange {
  kind: string;
  before: string;
  after: string;
  start: number;
  end: number;
  confidence: string;
}
export interface NormalizationWarning {
  code: string;
  before: string;
  start: number;
  end: number;
}
export interface NormalizePreview {
  original: string;
  spoken: string;
  language: string;
  locale: string;
  mode: NormalizationMode;
  reading_style: ReadingStyle;
  normalizer_version: string;
  changes: NormalizationChange[];
  warnings: NormalizationWarning[];
  coverage: Record<string, CoverageLevel>;
}
