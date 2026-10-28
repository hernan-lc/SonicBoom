import type { ProviderStatus } from './contracts';

/** An engine can generate when the server reports it ready. */
export function isProviderReady(providers: ProviderStatus[], providerId: string): boolean {
  return providers.some((provider) => provider.provider_id === providerId && provider.state === 'ready');
}

/**
 * Pocket TTS loads one model at a time, so only the active model is usable.
 * Other engines expose every model in their capability list once ready.
 */
export function isModelReady(providers: ProviderStatus[], providerId: string, modelId: string): boolean {
  return providers.some(
    (provider) =>
      provider.provider_id === providerId &&
      provider.state === 'ready' &&
      (providerId !== 'pocket-tts' || provider.active_model === modelId),
  );
}
