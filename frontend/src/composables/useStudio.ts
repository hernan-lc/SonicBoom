import { computed, inject, onMounted, onUnmounted, ref, shallowRef, watch, type InjectionKey } from 'vue';
import { samplingQuery, samplingErrors } from '../features/speech/generationSchemas';
import { isCancelledError } from '@tanstack/vue-query';
import { createResourceCache } from '../app/queryClient';
import { createHttpClient } from '../services/http';
import { createApi, type Api } from '../services/api';
import type { HomeData } from '../types';
import type {
  ServerInfo,
  ProviderStatus,
  Capabilities,
  Voice,
  BuiltinVoice,
  ConfigSnapshot,
  ConfigStatus,
  Schema,
  AudioDevice,
} from '../services/contracts';
import { errorMessage, type Message } from '../i18n';
import {
  DEFAULT_SAMPLING,
  loadSampling,
  persistSampling,
  type SamplingState,
} from '../features/speech/samplingStorage';
import { STUDIO_FAST_MS, slowRefreshDue } from './studioRefresh';
import { useModelCatalog } from './useModelCatalog';
import { useSettingsActions } from './useSettingsActions';
import { usePronunciation } from './usePronunciation';

// Re-exported so existing consumers keep a single studio entry point.
export { getPath, nestedChange } from '../services/configPaths';
export { STUDIO_FAST_MS, STUDIO_SLOW_EVERY, slowRefreshDue } from './studioRefresh';
export { DEFAULT_SAMPLING, type SamplingState } from '../features/speech/samplingStorage';
export { samplingQuery } from '../features/speech/generationSchemas';

export function createStudio(data: HomeData, overrides?: { api?: Api }) {
  const token = ref('');
  const scopeEpoch = ref(0);
  const info = shallowRef<ServerInfo | null>(null);
  const providers = shallowRef<ProviderStatus[]>(data.providers.map((entry) => entry.status));
  const capabilities = shallowRef<Capabilities[]>([]);
  const diagnostics = shallowRef<unknown>(null);
  const builtinVoices = shallowRef<BuiltinVoice[]>(data.voices.map((id) => ({ id, name: id })));
  const voices = shallowRef<Voice[]>([]);
  const config = shallowRef<ConfigSnapshot | null>(null);
  const status = shallowRef<ConfigStatus | null>(null);
  const schema = shallowRef<Schema>({});
  const devices = shallowRef<AudioDevice[]>([]);
  const errors = ref<Record<string, Message | undefined>>({});
  const loading = ref(true);
  const model = ref('supertonic-3');
  const voice = ref(data.voices[0] || '');
  const language = ref('en');
  const text = ref('');
  const preview = ref(0);
  const sampling = ref<SamplingState>(loadSampling());
  watch(sampling, (value) => persistSampling({ ...value }), { deep: true });
  const samplingIssues = computed(() => samplingErrors(sampling.value, info.value?.synthesis?.sampling));
  const samplingParams = computed(() => samplingQuery(sampling.value, info.value?.synthesis?.sampling));
  function resetSampling() {
    sampling.value = { ...DEFAULT_SAMPLING };
  }
  const {
    normalizationAvailable,
    normalizationMode,
    pronunciationLocale,
    readingStyle,
    normalizationLanguage,
    pronunciationLocales,
    showPronunciationLocale,
    pronunciation,
  } = usePronunciation(info, language);
  let session = new AbortController();
  const cache = createResourceCache();
  const api =
    overrides?.api ??
    createApi(
      createHttpClient(
        () => token.value,
        () => info.value?.auth.required ?? true,
        () => session.signal,
      ),
    );
  let disposed = false;
  let refreshing = false;
  let queued = false;
  let slowQueued = false;
  let tick = 0;
  // Generation guard: responses from an older token (or a disposed studio)
  // never overwrite newer state. Bumped on token change and disposal.
  let generation = 0;
  let authScope: string | null = null;
  // Last model status that triggered a built-in voices fetch. Immutable
  // voices are re-fetched only when readiness changes, not every tick.
  let lastVoicesStatus: string | null = null;
  async function load(area: string, action: () => Promise<void>) {
    const gen = generation;
    try {
      await action();
      if (gen !== generation || disposed) return;
      errors.value[area] = undefined;
    } catch (error) {
      if (gen !== generation || disposed || isCancelledError(error)) return;
      errors.value[area] = errorMessage(error);
    }
  }
  async function refreshInfo() {
    const gen = generation;
    await load('info', async () => {
      const result = await cache.read(gen, 'info', (signal) => api.info(signal));
      if (gen !== generation || disposed) return;
      const nextAuth = `${result.auth.mode}:${result.auth.required}`;
      if (authScope !== null && authScope !== nextAuth) {
        authScope = nextAuth;
        info.value = result;
        invalidateSession();
        void refresh();
        return;
      }
      authScope = nextAuth;
      info.value = result;
      // Built-in voices are immutable for a model state: refetch only when
      // readiness changes (first load, or a ready transition), never on
      // every ready-state tick.
      if (result.model.status === 'ready' && lastVoicesStatus !== 'ready') {
        const voices = await cache.read(gen, `builtin-${result.model.status}`, (signal) => api.builtinVoices(signal));
        if (gen !== generation || disposed) return;
        builtinVoices.value = voices;
      }
      lastVoicesStatus = result.model.status;
    });
  }
  async function refreshProviders() {
    const gen = generation;
    await load('providers', async () => {
      const result = await cache.read(gen, 'providers', (signal) => api.providers(signal));
      if (gen !== generation || disposed) return;
      providers.value = result.providers;
      capabilities.value = result.capabilities;
      diagnostics.value = result.diagnostics;
    });
  }
  async function refreshVoices(force = true) {
    const gen = generation;
    await load('voices', async () => {
      if (force) await cache.invalidate('voices', true);
      const result = await cache.read(gen, 'voices', (signal) => api.voices(signal));
      if (gen !== generation || disposed) return;
      voices.value = result;
    });
  }
  async function refreshDevices() {
    const gen = generation;
    await load('devices', async () => {
      const value = await cache.read(gen, 'devices', (signal) => api.audioDevices(signal), 30000);
      if (gen === generation && !disposed) devices.value = value;
    });
  }
  async function refreshConfig(force = false) {
    const gen = generation;
    await load('settings', async () => {
      const result = await cache.read(gen, 'config-status', (signal) => api.configStatus(signal));
      if (gen !== generation || disposed) return;
      status.value = result;
      if (force || !config.value || result.revision !== config.value.revision || !Object.keys(schema.value).length) {
        if (force || result.revision !== config.value?.revision) await cache.invalidate('config');
        const [snapshot, definitions] = await Promise.all([
          cache.read(gen, 'config', (signal) => api.config(signal), force ? 0 : 1000),
          cache.read(gen, 'schema', (signal) => api.schema(signal), Infinity),
        ]);
        if (gen !== generation || disposed) return;
        config.value = snapshot;
        schema.value = definitions;
      }
    });
  }
  async function refreshFast() {
    // Frequently changing provider/job status only.
    await Promise.all([refreshInfo(), refreshProviders(), refreshConfig()]);
  }
  async function refreshSlow() {
    // Low-frequency metadata: voice list plus the conditional snapshot.
    await Promise.all([refreshVoices(false), refreshDevices()]);
  }
  async function refresh(forceSlow = true) {
    if (refreshing) {
      queued = true;
      if (forceSlow) slowQueued = true;
      return;
    }
    refreshing = true;
    loading.value = true;
    const slow = forceSlow || slowQueued;
    slowQueued = false;
    try {
      await Promise.all(slow ? [refreshFast(), refreshSlow()] : [refreshFast()]);
    } finally {
      refreshing = false;
      loading.value = false;
      if (queued && !disposed) {
        queued = false;
        void refresh(slowQueued);
      }
    }
  }
  const { saveSetting, toggleProvider, activatePocketModel } = useSettingsActions({
    api,
    config,
    capabilities,
    errors,
    refreshConfig,
    refreshProviders,
  });

  const { models, selectedModel, availableVoices, profileCompatible } = useModelCatalog({
    model,
    voice,
    language,
    voices,
    builtinVoices,
    providers,
    capabilities,
    info,
    modelReady: data.modelReady,
  });

  function invalidateSession() {
    generation++;
    scopeEpoch.value = generation;
    session.abort();
    session = new AbortController();
    cache.clear();
    voices.value = [];
    config.value = null;
    status.value = null;
    schema.value = {};
    providers.value = [];
    capabilities.value = [];
    devices.value = [];
    errors.value = {};
    lastVoicesStatus = null;
  }
  watch(
    token,
    () => {
      invalidateSession();
      void refresh();
    },
    { flush: 'sync' },
  );
  function dispose() {
    disposed = true;
    generation++;
    session.abort();
    cache.clear();
    clearInterval(timer);
  }
  function selectProfile(profile: Voice, previewAudio = false) {
    model.value = profile.model;
    voice.value = profile.id;
    language.value = profile.language;
    if (previewAudio) preview.value++;
  }
  onMounted(() => {
    void refresh();
  });
  // Fast status ticks every 5s while visible; slow metadata every 6th tick.
  // In-flight refreshes are deduped, stale token generations are dropped,
  // and unmount clears the timer so no requests or rejections outlive it.
  const timer = setInterval(() => {
    if (disposed || (typeof document !== 'undefined' && document.hidden)) return;
    tick++;
    void refresh(slowRefreshDue(tick));
  }, STUDIO_FAST_MS);
  onUnmounted(dispose);
  return {
    api,
    token,
    scopeEpoch,
    info,
    providers,
    capabilities,
    diagnostics,
    builtinVoices,
    voices,
    config,
    status,
    schema,
    devices,
    errors,
    loading,
    model,
    voice,
    language,
    text,
    preview,
    sampling,
    samplingIssues,
    samplingParams,
    resetSampling,
    normalizationAvailable,
    normalizationMode,
    pronunciationLocale,
    readingStyle,
    normalizationLanguage,
    pronunciationLocales,
    showPronunciationLocale,
    pronunciation,
    models,
    selectedModel,
    availableVoices,
    refresh,
    refreshConfig,
    refreshProviders,
    refreshVoices,
    saveSetting,
    toggleProvider,
    activatePocketModel,
    profileCompatible,
    selectProfile,
    dispose,
  };
}
export type Studio = ReturnType<typeof createStudio>;
export const studioKey: InjectionKey<Studio> = Symbol('SonicBoom studio');
export function useStudio() {
  const studio = inject(studioKey);
  if (!studio) throw new Error('Studio context is missing');
  return studio;
}
