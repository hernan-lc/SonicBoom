import type { Ref } from 'vue';
import { ApiError } from '../services/http';
import { nestedChange } from '../services/configPaths';
import type { Api } from '../services/api';
import { activatePocketModel as activatePocketBundle } from '../services/pocketActivation';
import type { Capabilities, ConfigSnapshot, Json, ProviderStatus } from '../services/contracts';
import type { Message } from '../i18n';

type SettingsActionsInput = {
  api: Api;
  config: Ref<ConfigSnapshot | null>;
  capabilities: Ref<Capabilities[]>;
  errors: Ref<Record<string, Message | undefined>>;
  refreshConfig: (force?: boolean) => Promise<void>;
  refreshProviders: () => Promise<void>;
};

/** Config mutations with revision-conflict recovery and area-scoped errors. */
export function useSettingsActions(input: SettingsActionsInput) {
  const { api, config, capabilities, errors, refreshConfig, refreshProviders } = input;

  async function saveSetting(path: string, value: Json) {
    try {
      await api.patchConfig(nestedChange(path, value), config.value?.revision);
    } catch (error) {
      if (error instanceof ApiError && error.status === 409) {
        await refreshConfig(true);
        throw Object.assign(error, {
          localizedMessage: {
            key: 'Settings changed on the server. Your draft is preserved. Review the current value before saving again.',
          },
        });
      }
      throw error;
    }
    await refreshConfig(true);
    if (errors.value.settings) throw Object.assign(new Error(), { localizedMessage: errors.value.settings });
  }

  async function toggleProvider(provider: ProviderStatus) {
    const path = `providers.${provider.provider_id.replaceAll('-', '_')}.enabled`;
    const snapshot = await api.config();
    try {
      await api.patchConfig(nestedChange(path, !provider.enabled), snapshot.revision);
    } catch (error) {
      if (!(error instanceof ApiError) || error.status !== 409) throw error;
      const latest = await api.config();
      await api.patchConfig(nestedChange(path, !provider.enabled), latest.revision);
    }
    await Promise.all([refreshConfig(true), refreshProviders()]);
  }

  async function activatePocketModel(model: string) {
    const supported = capabilities.value.find((cap) => cap.provider_id === 'pocket-tts')?.models || [];
    await activatePocketBundle(api, model, supported);
    await Promise.all([refreshConfig(true), refreshProviders()]);
    if (errors.value.settings) throw Object.assign(new Error(), { localizedMessage: errors.value.settings });
    if (errors.value.providers) throw Object.assign(new Error(), { localizedMessage: errors.value.providers });
  }

  return { saveSetting, toggleProvider, activatePocketModel };
}
