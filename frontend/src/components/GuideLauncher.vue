<script setup lang="ts">
import { computed } from 'vue';
import { t } from '../i18n';
import { guides } from '../guides/registry';
import { startGuide } from '../guides/runner';

const props = defineProps<{ guideId: string }>();
const label = computed(() => {
  const guide = guides.find((item) => item.id === props.guideId);
  return t('Start guide: {title}', { title: guide ? t(guide.titleKey) : t('Guides') });
});
</script>
<template>
  <button type="button" class="guide-launcher" :aria-label="label" :title="label" @click="startGuide(guideId)">?</button>
</template>
