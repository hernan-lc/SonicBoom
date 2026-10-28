<script setup lang="ts">
import { computed, ref } from 'vue';
import { t } from '../../i18n';
import AppIcon from '../ui/AppIcon.vue';
import SearchableSelect from '../ui/SearchableSelect.vue';

export interface ModelOption {
  id: string;
  provider: string;
  cloned: boolean;
  languages: string[];
  ready: boolean;
}

const selected = defineModel<string>({ required: true });
const props = defineProps<{ models: ModelOption[]; disabled?: boolean }>();
const emit = defineEmits<{ enable: [provider: string] }>();

const query = ref('');
const showUnavailable = ref(false);

const ready = computed(() => props.models.filter((model) => model.ready));
const currentModel = computed(() => props.models.find((model) => model.id === selected.value));
/** Always keep the current choice visible, even when its engine went away. */
const selectable = computed(() =>
  currentModel.value && !currentModel.value.ready ? [currentModel.value, ...ready.value] : ready.value,
);
const unavailable = computed(() => {
  const needle = query.value.toLocaleLowerCase();
  return props.models
    .filter((model) => !model.ready && model.id !== selected.value)
    .filter((model) => `${model.id} ${model.provider}`.toLocaleLowerCase().includes(needle));
});
const unavailableTotal = computed(() => props.models.filter((model) => !model.ready).length);
/** Reveal matching disabled models while searching so they stay findable. */
const revealUnavailable = computed(() => showUnavailable.value || (query.value !== '' && unavailable.value.length > 0));
const stateLabel = (model: ModelOption) => t(model.ready ? 'ready' : 'Not ready');
const itemText = (model: ModelOption) => `${model.id} · ${stateLabel(model)}`;
const itemSearchText = (model: ModelOption) => `${model.id} ${model.provider}`;
</script>
<template>
  <SearchableSelect
    v-model="selected"
    v-model:query="query"
    input-id="model"
    :label="t('Model')"
    :options="selectable"
    :disabled="disabled"
    :placeholder="t('Search models')"
    :empty-text="unavailable.length ? '' : t('No models match this search.')"
    :item-text="itemText"
    :item-search-text="itemSearchText"
  >
    <template #item="{ option }">
      <strong>{{ option.id }}</strong>
      <small>{{ option.provider }} · {{ stateLabel(option) }}</small>
    </template>
    <template #footer>
      <div v-if="unavailableTotal" class="choice-unavailable-section">
        <button
          v-if="!revealUnavailable"
          type="button"
          class="choice-unavailable-toggle"
          @click.stop="showUnavailable = true"
        >
          {{ t('Show unavailable models ({count})', { count: unavailableTotal }) }}
        </button>
        <template v-else>
          <p class="muted choice-unavailable-title">{{ t('Unavailable models') }}</p>
          <button
            v-for="model in unavailable"
            :key="model.id"
            type="button"
            class="choice-unavailable"
            @click.stop="emit('enable', model.provider)"
          >
            <span>
              <strong>{{ model.id }}</strong>
              <small>{{ model.provider }} · {{ t('Not ready') }}</small>
            </span>
            <span class="choice-action">{{ t('Enable') }} <AppIcon name="arrow-right" /></span>
          </button>
        </template>
      </div>
    </template>
  </SearchableSelect>
</template>
