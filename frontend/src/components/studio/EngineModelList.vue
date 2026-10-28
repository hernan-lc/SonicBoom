<script setup lang="ts">
import { t } from '../../i18n';
import DisclosurePanel from '../ui/DisclosurePanel.vue';
import StatusChip from '../ui/StatusChip.vue';

defineProps<{ models: string[]; activeModel?: string | null }>();
const isActive = (model: string, activeModel?: string | null) => activeModel === model;
</script>
<template>
  <DisclosurePanel class="engine-models-details" :badge="models.length">
    <template #label>{{ t('Models') }}</template>
    <ul class="engine-models">
      <li v-for="model in models" :key="model">
        <code>{{ model }}</code>
        <StatusChip v-if="isActive(model, activeModel)" tone="ok" :label="t('Active')" />
      </li>
    </ul>
  </DisclosurePanel>
</template>
