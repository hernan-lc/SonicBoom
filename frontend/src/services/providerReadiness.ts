import type { ProviderStatus } from './contracts';

/** An engine can generate when the server reports it ready. */
export function isProviderReady(providers: ProviderStatus[], providerId: string): boolean {
  return providers.some((provider) => provider.provider_id === providerId && provider.state === 'ready');
}
