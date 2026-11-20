export interface SynthesisConstraints {
  max_text_chars: number;
  max_body_bytes: number;
  seed_max: string;
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
  context?: { provider: string; model: string; voice: string; effective_device: string; seed?: string };
}
