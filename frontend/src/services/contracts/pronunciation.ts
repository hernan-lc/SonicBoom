/** Pronunciation (text normalization) mode: `auto` converts numerals, dates, etc.; `off` speaks the text as typed. */
export type NormalizationMode = 'auto' | 'off';
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
  enabled?: boolean;
  languages: NormalizationLanguage[];
}
/** Optional synthesis query fields. Absent fields are never sent, so legacy requests are unchanged. */
export interface SpeechPronunciation {
  normalization?: NormalizationMode;
  locale?: string;
}
export interface NormalizeRequest {
  text: string;
  language: string;
  locale?: string;
  mode?: NormalizationMode;
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
  normalizer_version: string;
  changes: NormalizationChange[];
  warnings: NormalizationWarning[];
  coverage: Record<string, CoverageLevel>;
}
