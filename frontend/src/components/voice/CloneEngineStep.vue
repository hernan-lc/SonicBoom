<script setup lang="ts">
import { computed } from 'vue';
import SearchableSelect from '../ui/SearchableSelect.vue';
import type { Capabilities, ProviderStatus } from '../../services/contracts';
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
  ready?: boolean;
}

const engines = computed<Option[]>(() =>
  props.choices.map((choice) => ({
    id: choice.provider_id,
    ready: props.providers.some(
      (status) => status.provider_id === choice.provider_id && status.state === 'ready',
    ),
  })),
);
const models = computed<Option[]>(() => (props.cap?.models ?? []).map((id) => ({ id })));
const languageOptions = computed<Option[]>(() => props.languages.map((id) => ({ id })));

const optionText = (option: Option) => option.id;
const engineState = (option: Option) => t(option.ready ? 'ready' : 'Not ready');
const languageText = (option: Option) => t(`language.${option.id}`);
const languageSearch = (option: Option) => `${option.id} ${languageText(option)}`;
</script>
<template>
  <fieldset :disabled="disabled">
    <legend class="sr-only">{{ t('Choose an engine') }}</legend>
    <div class="row">
      <div class="field">
        <SearchableSelect
          v-model="provider"
          input-id="cloneProvider"
          :label="t('Engine')"
          :options="engines"
          :disabled="disabled"
          :placeholder="t('Search engines')"
          :empty-text="t('No cloning engines available')"
          :item-text="optionText"
          :item-search-text="optionText"
          :trigger-label="t('Show options')"
        >
          <template #item="{ option }">
            <strong>{{ option.id }}</strong>
            <small>{{ engineState(option) }}</small>
          </template>
        </SearchableSelect>
      </div>
      <div class="field">
        <SearchableSelect
          v-model="model"
          input-id="cloneModel"
          :label="t('Model')"
          :options="models"
          :disabled="disabled || !cap"
          :placeholder="t('Search models')"
          :empty-text="t('No models match this search.')"
          :item-text="optionText"
          :trigger-label="t('Show options')"
        >
          <template #item="{ option }">
            <strong>{{ option.id }}</strong>
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
  </fieldset>
</template>
