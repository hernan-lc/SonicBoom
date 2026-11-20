<script setup lang="ts">
import { computed, ref } from 'vue';
import { t } from '../i18n';
import { completedGuides } from '../guides/progress';
import { guides } from '../guides/registry';
import { startGuide } from '../guides/runner';

const done = ref(new Set(completedGuides()));
const cards = computed(() =>
  guides.map((guide) => ({
    id: guide.id,
    title: t(guide.titleKey),
    summary: t(guide.summaryKey),
    minutes: Math.max(1, Math.round(guide.seconds / 60)),
    complete: done.value.has(guide.id),
  })),
);
async function launch(id: string) {
  await startGuide(id);
  done.value = new Set(completedGuides());
}
</script>
<template>
  <div class="guides-page">
    <p class="guides-intro">{{ t('Pick a short guide and follow the highlights.') }}</p>
    <ul class="guides-grid">
      <li v-for="card in cards" :key="card.id" class="guide-card">
        <h2>{{ card.title }}</h2>
        <p class="guide-summary">{{ card.summary }}</p>
        <div class="guide-meta">
          <span>{{ t('~{count} min', { count: card.minutes }) }}</span>
          <span :class="{ 'guide-done': card.complete }">{{ t(card.complete ? 'Done' : 'Pending') }}</span>
        </div>
        <button
          type="button"
          class="primary"
          :aria-label="t('Start guide: {title}', { title: card.title })"
          @click="launch(card.id)"
        >{{ t('Start') }}</button>
      </li>
    </ul>
  </div>
</template>
