<script setup lang="ts">
import { computed, ref } from 'vue';
import { t } from '../../i18n';
import SearchableSelect from '../ui/SearchableSelect.vue';

interface VoiceOption {
  id: string;
  name: string;
  detail: string;
  cloned: boolean;
}

const selected = defineModel<string>({ required: true });
const props = defineProps<{ voices: VoiceOption[]; disabled?: boolean }>();
const open = ref(false);
const current = computed(() => props.voices.find((voice) => voice.id === selected.value));
const itemText = (voice: VoiceOption) => voice.name;
const itemSearchText = (voice: VoiceOption) => `${voice.name} ${voice.detail}`;
</script>
<template>
  <div>
    <SearchableSelect
      v-model="selected"
      v-model:open="open"
      class="voice-picker"
      input-id="voice-search"
      :label="t('Voice')"
      :options="voices"
      :disabled="disabled"
      :placeholder="t('Search voices')"
      :empty-text="t('No voices match this search.')"
      :item-text="itemText"
      :item-search-text="itemSearchText"
      described-by="voice-count voice-selected"
      :trigger-label="t('Show voices')"
    >
      <template #item="{ option }">
        <strong>{{ option.name }}</strong>
        <small>{{ option.cloned ? option.detail : t(option.detail) }}</small>
      </template>
      <template #help="{ filtered }">
        <p v-if="open" id="voice-count" role="status" class="muted">{{ t('{count} voices', { count: filtered.length }) }}</p>
        <p id="voice-selected" class="muted">
          {{ current ? t('Selected voice: {name}', { name: current.name }) : t('No voices match this search.') }}
        </p>
      </template>
    </SearchableSelect>
  </div>
</template>
