export interface Voice {
  model_revision?: string;
  id: string;
  name: string;
  provider: string;
  model: string;
  language: string;
  clone_mode: string;
  created_at?: string;
  updated_at?: string;
  reference_duration_secs?: number;
  reference_sample_rate?: number;
  warnings?: string[];
  defects?: string[];
  guidance?: string[];
}
export interface ReferenceHealth {
  duration_secs: number;
  source_channels: number;
  source_sample_rate: number;
  peak: number;
  rms: number;
  clipping_fraction: number;
  low_amplitude_fraction: number;
  non_finite_samples: number;
}
export interface VoiceCreateResult extends Voice {
  revision?: number;
  reference_health: ReferenceHealth;
  warnings: string[];
  defects: string[];
  guidance: string[];
}
export interface BuiltinVoice {
  id: string;
  name: string;
}

export interface CloneInput {
  expected_revision?: number;
  name: string;
  provider: string;
  model: string;
  language: string;
  mode: string;
  consent_basis: string;
  transcript?: string;
}

export interface VoiceConstraints {
  revision?: number;
  version: number;
  max_upload_bytes: number;
  min_sample_rate: number;
  max_sample_rate: number;
  max_channels: number;
  name_max_chars: number;
  transcript_max_chars: number;
  providers: Record<
    string,
    {
      duration_seconds: [number, number];
      clone_modes: string[];
      models: string[];
      languages: string[];
      transcript_required_for: string[];
    }
  >;
}
