import { computed, ref, watch, type Ref } from 'vue';
import type { NormalizationMode, ServerInfo, SpeechPronunciation } from '../services/contracts';
import { effectiveLocale, localeChoices, showLocaleChoice } from '../features/speech/pronunciation';

/**
 * Pronunciation (text normalization) state. The speech language is the form's
 * language, never the UI language, and every locale comes from /api/info.
 */
export function usePronunciation(info: Ref<ServerInfo | null>, language: Ref<string>) {
  const normalizationInfo = computed(() => info.value?.normalization);
  const normalizationAvailable = computed(() => !!normalizationInfo.value);
  const normalizationChoice = ref<NormalizationMode | null>(null);
  const normalizationMode = computed<NormalizationMode>({
    get: () =>
      normalizationInfo.value?.enabled === false
        ? 'off'
        : (normalizationChoice.value ?? normalizationInfo.value?.default_mode ?? 'auto'),
    set: (value) => {
      normalizationChoice.value = value;
    },
  });
  const pronunciationLocale = ref('');
  const normalizationLanguage = computed(() =>
    normalizationInfo.value?.languages.find((entry) => entry.language === language.value),
  );
  const pronunciationLocales = computed(() => localeChoices(normalizationLanguage.value));
  const showPronunciationLocale = computed(() =>
    showLocaleChoice(normalizationLanguage.value, normalizationMode.value),
  );
  /** Query fields for synthesis; undefined when the server predates normalization (legacy behavior). */
  const pronunciation = computed<SpeechPronunciation | undefined>(() =>
    normalizationInfo.value
      ? {
          normalization: normalizationMode.value,
          locale: effectiveLocale(normalizationLanguage.value, pronunciationLocale.value, normalizationMode.value),
        }
      : undefined,
  );
  watch(pronunciationLocales, (options) => {
    if (
      pronunciationLocale.value &&
      pronunciationLocale.value !== 'auto' &&
      !options.includes(pronunciationLocale.value)
    )
      pronunciationLocale.value = '';
  });
  return {
    normalizationAvailable,
    normalizationMode,
    pronunciationLocale,
    normalizationLanguage,
    pronunciationLocales,
    showPronunciationLocale,
    pronunciation,
  };
}
