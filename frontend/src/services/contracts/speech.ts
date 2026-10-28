import type { NumericBounds } from './json';

export interface SynthesisConstraints {
  max_text_chars: number;
  max_body_bytes: number;
  sampling: {
    provider: string;
    seed_max: string;
    temperature: NumericBounds;
    top_k: NumericBounds;
    top_p: NumericBounds;
    repetition_penalty: NumericBounds;
  };
}

export interface QualityIssue {
  kind: string;
  severity: 'info' | 'warning' | 'error';
  detail: string;
  confidence?: number | null;
}
export interface AudioHealth {
  sample_count: number;
  sample_rate: number;
  duration_secs: number;
  non_finite_samples: number;
  peak: number;
  rms: number;
  dc_offset: number;
  clipped_samples: number;
  clipping_fraction: number;
  near_silent_fraction: number;
  max_discontinuity: number;
  impulsive_samples: number;
  chunk_boundary_peaks: number;
}
export interface EchoAnalysis {
  detected: boolean;
  asr_required: boolean;
  matched_words: number;
  candidate_words: number;
  at_start: boolean;
  confidence?: number | null;
  evidence: string[];
}
export interface QualityReport {
  quality_status: 'ok' | 'warning' | 'failed';
  issues: QualityIssue[];
  audio?: AudioHealth;
  echo?: EchoAnalysis;
  retries: number;
  mode: string;
}
export interface Job {
  id?: string;
  stage: string;
  elapsed_secs?: number;
  audio_secs?: number;
  rtf?: number;
  frames?: number;
  fragments_total?: number;
  fragments_done?: number;
  cancellation_requested?: boolean;
  deadline_exceeded?: boolean;
  error?: string;
  quality?: QualityReport;
  context?: { provider: string; model: string; voice: string; effective_device: string; seed?: string };
}
