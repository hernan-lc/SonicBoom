<script setup lang="ts">
import { computed, ref } from 'vue';
import { getPath, useStudio } from '../../composables/useStudio';
import { t } from '../../i18n';

const props = defineProps<{ providerId: string; activeModel?: string | null; models: string[]; enabled: boolean; state: string; switching: boolean; pending: boolean }>();
const emit = defineEmits<{ activate: [model: string] }>();
const studio = useStudio();
const selected = computed(() => {
  const value = getPath(studio.config.value?.config, 'providers.pocket_tts.model_id') || props.activeModel;
  return typeof value === 'string' ? value : '';
});
const switching = computed(() => props.switching);
const isActive = (model: string) =>
  props.enabled && props.state === 'ready' && props.activeModel === model && selected.value === model;
const draft = ref<string | null>(null);
const choice = computed({
  get: () => draft.value ?? selected.value ?? props.activeModel ?? '',
  set: (value: string) => {
    draft.value = value;
  },
});
function activate() {
  if (!choice.value || isActive(choice.value)) return;
  emit('activate', choice.value);
  draft.value = null;
}
const schemaReady = computed(() => !!studio.schema.value['providers.pocket_tts.model_id']);
</script>
<template>
  <div class="engine-model-picker">
    <label :for="`engine-model-${providerId}`"
      >{{ t('Model')
      }}<select
        :id="`engine-model-${providerId}`"
        v-model="choice"
        :disabled="pending || switching || !schemaReady"
      >
        <option v-for="model in models" :key="model" :value="model">{{ model }}</option>
      </select></label
    ><button
      type="button"
      class="engine-activate"
      :disabled="pending || switching || !choice || isActive(choice) || !schemaReady"
      @click="activate"
    >
      {{ t(switching ? 'Loading…' : 'Activate') }}
    </button>
  </div>
</template>
