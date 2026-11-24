import type { SynthesisConstraints } from './speech';
import type { NormalizationInfo } from './pronunciation';

export interface ServerInfo {
  synthesis?: SynthesisConstraints;
  normalization?: NormalizationInfo;
  version: string;
  mode: string;
  setup_complete: boolean;
  auth: { mode: string; required: boolean };
  features: { playback: boolean; audio_devices: boolean; hot_reload: boolean };
  config_revision: number;
  model: { status: string };
}
export interface ProviderStatus {
  model_revision?: string;
  provider_id: string;
  state: string;
  enabled: boolean;
  effective_device?: string | null;
  active_model?: string | null;
  detail?: string | null;
}
export interface Capabilities {
  provider_id: string;
  models: string[];
  languages: string[];
  devices: string[];
  clone_modes: string[];
  requires_reference_audio: boolean;
  requires_reference_transcript: boolean;
  sample_rate?: number;
}
/** Transcription engine: same lifecycle as a provider, exposed by /api/providers. */
export interface AsrEngineStatus {
  id: string;
  kind: 'asr';
  state: 'absent' | 'downloading' | 'loading' | 'ready' | 'failed';
  enabled: boolean;
  model: string;
  active_model: string | null;
  progress?: number;
  detail?: string;
  license: string;
  prepare_path: string;
}
export interface ProvidersResponse {
  providers: ProviderStatus[];
  capabilities: Capabilities[];
  asr?: AsrEngineStatus;
  diagnostics?: unknown;
}

export interface AudioDevice {
  id: string;
  name: string;
}
