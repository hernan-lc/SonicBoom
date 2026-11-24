<script setup lang="ts">
import { computed } from 'vue';
import { t } from '../../i18n';
import { useStudio } from '../../composables/useStudio';
import { useTask } from '../../composables/useTask';
import type { AsrEngineStatus } from '../../services/contracts';
import FeedbackNotice from '../FeedbackNotice.vue';
import StatusChip from '../ui/StatusChip.vue';

const props = defineProps<{ engine: AsrEngineStatus }>();
const studio = useStudio();
const task = useTask();
const { pending, feedback } = task;
const busy = computed(
  () => pending.value || props.engine.state === 'downloading' || props.engine.state === 'loading',
);
const tone = computed<'ok' | 'bad' | 'info' | 'neutral'>(() =>
  props.engine.state === 'ready'
    ? 'ok'
    : props.engine.state === 'failed'
      ? 'bad'
      : busy.value
        ? 'info'
        : 'neutral',
);
const readiness = computed(() =>
  [
    props.engine.active_model ? t('Model loaded') : t('No model loaded'),
    props.engine.state === 'ready' ? t('Ready to generate') : t('Not ready'),
  ].join(' · '),
);
async function prepare() {
  await task.run(async () => {
    await studio.api.prepareAsr();
    await studio.refreshProviders();
  });
}
</script>
<template>
  <article class="provider" :aria-busy="busy">
    <div class="provider-top">
      <h2 class="provider-name">{{ t('Transcription') }}</h2>
      <StatusChip :tone="tone" :label="t(engine.state)" />
    </div>
    <p data-guide="engine-status" class="engine-readiness" :title="readiness">{{ readiness }}</p>
    <p v-if="engine.active_model" data-guide="engine-active-model" class="engine-active-model">
      <code>{{ engine.active_model }}</code>
    </p>
    <progress
      v-if="engine.progress !== undefined"
      class="loading-notice-bar"
      :value="Math.round(engine.progress * 100)"
      max="100"
      :aria-label="t('Model progress')"
    ></progress>
    <p v-if="engine.detail" class="engine-detail">{{ engine.detail }}</p>
    <p class="engine-about"><span class="muted">{{ engine.license }}</span></p>
    <div class="provider-actions">
      <button data-guide="engine-toggle" type="button" :disabled="busy" @click="prepare">
        {{ t(busy ? 'Preparing model…' : engine.state === 'ready' ? 'Model ready' : 'Prepare model') }}
      </button>
    </div>
    <FeedbackNotice :feedback="feedback" />
  </article>
</template>
