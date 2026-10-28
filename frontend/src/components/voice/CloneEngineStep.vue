<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import SearchableSelect from '../ui/SearchableSelect.vue';
import type { Capabilities, ProviderStatus } from '../../services/contracts';
import { isModelReady, isProviderReady } from '../../services/providerReadiness';
import { t } from '../../i18n';

const props = defineProps<{
  choices: Capabilities[];
  providers: ProviderStatus[];
  cap?: Capabilities;
  languages: string[];
  ready: boolean;
  disabled: boolean;
}>();

const provider = defineModel<string>('provider', { required: true });
const model = defineModel<string>('model', { required: true });
const language = defineModel<string>('language', { required: true });

interface Option {
  id: string;
  ready: boolean;
}

const engineQuery = ref('');
const modelQuery = ref('');
const engineOpen = ref(false);
const modelOpen = ref(false);
const showEngineUnavailable = ref(false);
const showModelUnavailable = ref(false);

const engines = computed<Option[]>(() =>
  props.choices.map((choice) => ({
    id: choice.provider_id,
    ready: isProviderReady(props.providers, choice.provider_id),
  })),
);
const models = computed<Option[]>(() =>
  (props.cap?.models ?? []).map((id) => ({
    id,
    ready: isModelReady(props.providers, props.cap?.provider_id ?? '', id),
  })),
);
const languageOptions = computed<Option[]>(() => props.languages.map((id) => ({ id, ready: true })));

const currentEngine = computed(() => engines.value.find((engine) => engine.id === provider.value));
const currentModel = computed(() => models.value.find((item) => item.id === model.value));
const availableEngines = computed(() => engines.value.filter((engine) => engine.ready));
const availableModels = computed(() => models.value.filter((item) => item.ready));
const hiddenEngines = computed(() =>
  engines.value.filter((engine) => !engine.ready && engine.id !== provider.value),
);
const hiddenModels = computed(() =>
  models.value.filter((item) => !item.ready && item.id !== model.value),
);
const unavailableEngines = computed(() => {
  const needle = engineQuery.value.toLocaleLowerCase();
  return hiddenEngines.value.filter((engine) => engine.id.toLocaleLowerCase().includes(needle));
});
const unavailableModels = computed(() => {
  const needle = modelQuery.value.toLocaleLowerCase();
  return hiddenModels.value.filter((item) => item.id.toLocaleLowerCase().includes(needle));
});
const unavailableEngineTotal = computed(() => hiddenEngines.value.length);
const unavailableModelTotal = computed(() => hiddenModels.value.length);
/** Reveal matching disabled choices while searching so they stay findable. */
const revealEngineUnavailable = computed(
  () => showEngineUnavailable.value || (engineQuery.value !== '' && unavailableEngines.value.length > 0),
);
const revealModelUnavailable = computed(
  () => showModelUnavailable.value || (modelQuery.value !== '' && unavailableModels.value.length > 0),
);
/** Keep the current choice visible so the input never goes blank. */
const engineOptions = computed(() => {
  const options = availableEngines.value.slice();
  if (currentEngine.value && !currentEngine.value.ready) options.unshift(currentEngine.value);
  if (revealEngineUnavailable.value) options.push(...hiddenEngines.value);
  return options;
});
const modelOptions = computed(() => {
  const options = availableModels.value.slice();
  if (currentModel.value && !currentModel.value.ready) options.unshift(currentModel.value);
  if (revealModelUnavailable.value) options.push(...hiddenModels.value);
  return options;
});
const modelReady = computed(() =>
  props.cap ? isModelReady(props.providers, props.cap.provider_id, model.value) : false,
);
const engineEmptyText = computed(() => {
  if (!props.choices.length) return t('No cloning engines available');
  if (engineQuery.value && !unavailableEngines.value.length) return t('No results.');
  return '';
});
const modelEmptyText = computed(() => {
  if (modelQuery.value && !unavailableModels.value.length) return t('No models match this search.');
  return !props.cap?.models.length ? t('No models match this search.') : '';
});

const optionText = (option: Option) => option.id;
const stateText = (option: Option) => t(option.ready ? 'ready' : 'Not ready');
const languageText = (option: Option) => t(`language.${option.id}`);
const languageSearch = (option: Option) => `${option.id} ${languageText(option)}`;

watch(engineOpen, (open) => {
  if (!open) showEngineUnavailable.value = false;
});
watch(modelOpen, (open) => {
  if (!open) showModelUnavailable.value = false;
});
</script>
<template>
  <fieldset :disabled="disabled">
    <legend class="sr-only">{{ t('Choose an engine') }}</legend>
    <div class="row">
      <div class="field">
        <SearchableSelect
          v-model="provider"
          v-model:query="engineQuery"
          v-model:open="engineOpen"
          input-id="cloneProvider"
          :label="t('Engine')"
          :options="engineOptions"
          :disabled="disabled"
          :placeholder="t('Search engines')"
          :empty-text="engineEmptyText"
          :item-text="optionText"
          :item-search-text="optionText"
          :trigger-label="t('Show options')"
        >
          <template #item="{ option }">
            <strong>{{ option.id }}</strong>
            <small>{{ stateText(option) }}</small>
          </template>
          <template #footer>
            <div
              v-if="unavailableEngineTotal && !revealEngineUnavailable && !engineQuery"
              class="choice-unavailable-section"
            >
              <button
                type="button"
                class="choice-unavailable-toggle"
                :disabled="disabled"
                @click.stop="showEngineUnavailable = true"
              >
                {{ t('Show unavailable engines ({count})', { count: unavailableEngineTotal }) }}
              </button>
            </div>
          </template>
        </SearchableSelect>
      </div>
      <div class="field">
        <SearchableSelect
          v-model="model"
          v-model:query="modelQuery"
          v-model:open="modelOpen"
          input-id="cloneModel"
          :label="t('Model')"
          :options="modelOptions"
          :disabled="disabled || !cap"
          :placeholder="t('Search models')"
          :empty-text="modelEmptyText"
          :item-text="optionText"
          :trigger-label="t('Show options')"
        >
          <template #item="{ option }">
            <strong>{{ option.id }}</strong>
            <small>{{ stateText(option) }}</small>
          </template>
          <template #footer>
            <div
              v-if="unavailableModelTotal && !revealModelUnavailable && !modelQuery"
              class="choice-unavailable-section"
            >
              <button
                type="button"
                class="choice-unavailable-toggle"
                :disabled="disabled"
                @click.stop="showModelUnavailable = true"
              >
                {{ t('Show unavailable models ({count})', { count: unavailableModelTotal }) }}
              </button>
            </div>
          </template>
        </SearchableSelect>
      </div>
    </div>
    <div class="form-group">
      <SearchableSelect
        v-model="language"
        input-id="cloneLang"
        :label="t('Language')"
        :options="languageOptions"
        :disabled="disabled"
        :placeholder="t('Search languages')"
        :empty-text="t('No results.')"
        :item-text="languageText"
        :item-search-text="languageSearch"
        :trigger-label="t('Show options')"
      >
        <template #item="{ option }">
          <strong>{{ languageText(option) }}</strong>
          <small>{{ option.id }}</small>
        </template>
      </SearchableSelect>
    </div>
    <p v-if="!ready" class="help-note">
      {{ t('You can save a reference now. Enable its engine before previewing or generating audio.') }}
    </p>
    <p v-else-if="!modelReady" class="help-note">
      {{ t('You can save a reference now. Switch to an active model before previewing or generating audio.') }}
    </p>
  </fieldset>
</template>
