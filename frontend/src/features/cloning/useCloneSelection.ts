import { computed, ref, watch, type Ref } from 'vue';
import { modelLanguages } from '../../services/providerLanguages';
import { isModelReady, isProviderReady } from '../../services/providerReadiness';
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
  const readyChoices = computed(() =>
    choices.value.filter((choice) => isProviderReady(providers.value, choice.provider_id)),
  );
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

  const engineReady = computed(() => isProviderReady(providers.value, provider.value));

  watch(
    choices,
    (value) => {
      if (!value.some((item) => item.provider_id === provider.value)) {
        const fallback = readyChoices.value[0] ?? value[0];
        provider.value = fallback?.provider_id || '';
      }
    },
    { immediate: true },
  );

  watch(
    cap,
    (value) => {
      if (!value?.models.includes(model.value)) {
        const readyModel = value?.models.find((id) => isModelReady(providers.value, value.provider_id, id));
        model.value = readyModel ?? value?.models[0] ?? '';
      }
      if (!value?.languages.includes(language.value)) language.value = value?.languages[0] || 'en';
      if (!value?.clone_modes.includes(mode.value)) mode.value = value?.clone_modes[0] || 'icl';
    },
    { immediate: true },
  );

  return { step, provider, model, language, mode, choices, cap, languages, needsTranscript, engineReady };
}
