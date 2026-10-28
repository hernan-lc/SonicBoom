import type { SamplingDraft } from './generationSchemas';

/** Qwen3-TTS advanced generation settings (all optional, string-backed). */
export type SamplingState = SamplingDraft;

export const DEFAULT_SAMPLING: SamplingState = {
  preset: 'balanced',
  seed: '',
  quality: '',
  talkerTemperature: '',
  talkerTopK: '',
  talkerTopP: '',
  talkerRepetitionPenalty: '',
  predictorTemperature: '',
  predictorTopK: '',
  predictorTopP: '',
  predictorRepetitionPenalty: '',
};

const samplingStorageKey = 'sonicboom.sampling';

/** Restore persisted sampling settings, ignoring unknown keys and bad JSON. */
export function loadSampling(): SamplingState {
  try {
    const saved = localStorage.getItem(samplingStorageKey);
    if (saved) {
      const value: unknown = JSON.parse(saved);
      if (value && typeof value === 'object')
        return Object.fromEntries(
          Object.entries(DEFAULT_SAMPLING).map(([key, fallback]) => [
            key,
            typeof (value as Record<string, unknown>)[key] === 'string'
              ? (value as Record<string, string>)[key]
              : fallback,
          ]),
        ) as unknown as SamplingState;
    }
  } catch {
    /* Storage can be unavailable in restricted contexts. */
  }
  return { ...DEFAULT_SAMPLING };
}

export function persistSampling(sampling: SamplingState) {
  try {
    localStorage.setItem(samplingStorageKey, JSON.stringify(sampling));
  } catch {
    /* session-only */
  }
}
