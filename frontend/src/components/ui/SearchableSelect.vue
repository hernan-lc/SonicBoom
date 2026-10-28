<script setup lang="ts" generic="T extends { id: string }">
import { computed, ref } from 'vue';
import {
  ComboboxRoot,
  ComboboxAnchor,
  ComboboxInput,
  ComboboxTrigger,
  ComboboxPortal,
  ComboboxContent,
  ComboboxItem,
  ComboboxVirtualizer,
} from 'reka-ui';
import { t } from '../../i18n';

const selected = defineModel<string>({ required: true });
const query = defineModel<string>('query', { default: '' });
const open = defineModel<boolean>('open', { default: false });

const props = withDefaults(
  defineProps<{
    options: T[];
    label: string;
    inputId: string;
    placeholder?: string;
    disabled?: boolean;
    emptyText?: string;
    describedBy?: string;
    itemText: (option: T) => string;
    itemSearchText?: (option: T) => string;
    virtualizeAfter?: number;
    estimateSize?: number;
    triggerLabel?: string;
  }>(),
  { virtualizeAfter: 100, estimateSize: 64 },
);

const combobox = ref<{ highlightedElement?: HTMLElement } | null>(null);
const activeId = computed(() => (open.value ? combobox.value?.highlightedElement?.id : undefined));
const filtered = computed(() => {
  const needle = query.value.toLocaleLowerCase();
  const search = props.itemSearchText ?? props.itemText;
  return props.options.filter((option) => search(option).toLocaleLowerCase().includes(needle));
});
const current = computed({
  get: () => props.options.find((option) => option.id === selected.value),
  set: (value: T | undefined) => {
    if (value) selected.value = value.id;
  },
});
</script>
<template>
  <div class="searchable-select">
    <label :for="inputId">{{ label }}</label>
    <ComboboxRoot
      ref="combobox"
      v-model="current"
      v-model:open="open"
      :disabled="disabled"
      ignore-filter
      open-on-click
      by="id"
      @update:open="(value) => { if (value) query = ''; }"
    >
      <ComboboxAnchor class="voice-combobox-anchor">
        <ComboboxInput
          :id="inputId"
          :aria-activedescendant="activeId"
          v-model="query"
          :display-value="(value: T) => (value ? itemText(value) : '')"
          :placeholder="placeholder ?? t('Search…')"
          :aria-describedby="describedBy"
        />
        <ComboboxTrigger type="button" :aria-label="triggerLabel ?? t('Show options')">⌄</ComboboxTrigger>
      </ComboboxAnchor>
      <ComboboxPortal>
        <ComboboxContent class="voice-combobox-content searchable-select-content" position="popper" :side-offset="4">
          <div class="voice-combobox-viewport" role="presentation">
            <p v-if="!filtered.length && (emptyText ?? t('No results.'))" class="muted">
              {{ emptyText ?? t('No results.') }}
            </p>
            <ComboboxVirtualizer
              v-if="filtered.length > virtualizeAfter"
              v-slot="{ option }"
              :options="filtered"
              :text-content="(option: T) => itemText(option)"
              :estimate-size="estimateSize"
            >
              <ComboboxItem :value="option" class="voice-combobox-item"><slot name="item" :option="option" /></ComboboxItem>
            </ComboboxVirtualizer>
            <template v-else>
              <ComboboxItem v-for="option in filtered" :key="option.id" :value="option" class="voice-combobox-item">
                <slot name="item" :option="option" />
              </ComboboxItem>
            </template>
            <slot name="footer" :query="query" />
          </div>
        </ComboboxContent>
      </ComboboxPortal>
    </ComboboxRoot>
    <slot name="help" :filtered="filtered" />
  </div>
</template>
