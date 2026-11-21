<script setup lang="ts">
import { computed } from 'vue';
import { t } from '../../i18n';
import type { ProviderStatus, Capabilities } from '../../services/contracts';
import { useStudio } from '../../composables/useStudio';
import { useTask } from '../../composables/useTask';
import FeedbackNotice from '../FeedbackNotice.vue';
import ModelLoadingNotice from '../ModelLoadingNotice.vue';
import StatusChip from '../ui/StatusChip.vue';
import EngineModelList from './EngineModelList.vue';

const props = defineProps<{ provider: ProviderStatus; capabilities?: Capabilities }>();
const studio = useStudio();
const task = useTask();
const { pending, feedback } = task;
const togglePath = computed(() => `providers.${props.provider.provider_id.replaceAll('-', '_')}.enabled`);
const models = computed(() => props.capabilities?.models || []);
const stateTone = computed(() =>
  props.provider.state === 'ready'
    ? ('ok' as const)
    : props.provider.state === 'loading'
      ? ('info' as const)
      : props.provider.state === 'disabled'
        ? ('neutral' as const)
        : ('warn' as const),
);
const readiness = computed(() =>
  [
    props.provider.enabled ? t('Engine enabled') : t('Engine disabled'),
    props.provider.active_model ? t('Model loaded') : t('No model loaded'),
    props.provider.state === 'ready' ? t('Ready to generate') : t('Not ready'),
  ].join(' · '),
);
</script>
<template>
  <article class="provider" :aria-busy="pending">
    <div class="provider-top">
      <h2 class="provider-name">{{ provider.provider_id }}</h2>
      <StatusChip :tone="stateTone" :label="t(provider.state)" /><span
        v-if="provider.effective_device"
        class="device-chip"
        >{{ provider.effective_device }}</span
      >
    </div>
    <p data-guide="engine-status" class="engine-readiness" :title="readiness">{{ readiness }}</p>
    <p v-if="provider.active_model" data-guide="engine-active-model" class="engine-active-model">
      <code>{{ provider.active_model }}</code>
    </p>
    <EngineModelList v-if="models.length" :models="models" :active-model="provider.active_model" />
    <ModelLoadingNotice mode="inline" :engine="provider" />
    <div v-if="studio.schema.value[togglePath]" class="provider-actions">
      <span v-if="capabilities?.languages.length" class="muted">{{
        t('{count} audio languages', { count: capabilities.languages.length })
      }}</span
      ><button data-guide="engine-toggle" type="button" :disabled="pending" @click="task.run(() => studio.toggleProvider(provider))">
        {{ t(pending ? (provider.enabled ? 'Stopping…' : 'Starting…') : provider.enabled ? 'Disable' : 'Enable') }}
      </button>
    </div>
    <FeedbackNotice :feedback="feedback" />
  </article>
</template>
