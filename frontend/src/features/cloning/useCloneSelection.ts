import { computed, ref, watch, type Ref } from 'vue';
import { modelLanguages } from '../../services/providerLanguages';
import type { Capabilities, ProviderStatus, VoiceConstraints } from '../../services/contracts';

/**
 * Engine/model/language/mode selection for the clone wizard. Keeps every
 * choice valid as capabilities and provider readiness change.
 */
export function useCloneSelection(
  capabilities: Ref<Capabilities[]>,
  providers: Ref<ProviderStatus[]>,
  limits: Ref<VoiceConstraints | null>,
) {
  const step = ref(0);
  const provider = ref('');
  const model = ref('');
  const language = ref('en');
  const mode = ref('icl');

  const choices = computed(() => capabilities.value.filter((cap) => cap.requires_reference_audio));
  const cap = computed(() => choices.value.find((item) => item.provider_id === provider.value));
  const languages = computed(() => modelLanguages(cap.value, model.value));

  watch(languages, (value) => {
    if (!value.includes(language.value)) language.value = value[0] || 'en';
  });

  const needsTranscript = computed(() => {
    const constraint = limits.value?.providers[provider.value];
    return constraint
      ? constraint.transcript_required_for.includes(mode.value)
      : mode.value === 'icl' || !!cap.value?.requires_reference_transcript;
  });

  const engineReady = computed(() =>
    providers.value.some((item) => item.provider_id === provider.value && item.state === 'ready'),
  );

  watch(
    choices,
    (value) => {
      if (!value.some((item) => item.provider_id === provider.value)) provider.value = value[0]?.provider_id || '';
    },
    { immediate: true },
  );

  watch(
    cap,
    (value) => {
      if (!value?.models.includes(model.value)) model.value = value?.models[0] || '';
      if (!value?.languages.includes(language.value)) language.value = value?.languages[0] || 'en';
      if (!value?.clone_modes.includes(mode.value)) mode.value = value?.clone_modes[0] || 'icl';
    },
    { immediate: true },
  );

  return { step, provider, model, language, mode, choices, cap, languages, needsTranscript, engineReady };
}
