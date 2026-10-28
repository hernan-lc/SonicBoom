<script setup lang="ts">
import { computed } from 'vue';
import { t } from '../../i18n';
import type { ProviderStatus, Capabilities } from '../../services/contracts';
import { getPath, useStudio } from '../../composables/useStudio';
import { useTask } from '../../composables/useTask';
import FeedbackNotice from '../FeedbackNotice.vue';
import StatusChip from '../ui/StatusChip.vue';
import EngineDiagnostics from './EngineDiagnostics.vue';
import EngineModelList from './EngineModelList.vue';
import EngineModelPicker from './EngineModelPicker.vue';

const props = defineProps<{ provider: ProviderStatus; capabilities?: Capabilities }>();
const studio = useStudio();
const task = useTask();
const { pending, feedback } = task;
const togglePath = computed(() => `providers.${props.provider.provider_id.replaceAll('-', '_')}.enabled`);
const isPocket = computed(() => props.provider.provider_id === 'pocket-tts');
const models = computed(() => props.capabilities?.models || []);
const selectedPocketModel = computed(() => {
  const value = getPath(studio.config.value?.config, 'providers.pocket_tts.model_id') || props.provider.active_model;
  return typeof value === 'string' ? value : '';
});
const switching = computed(
  () =>
    Boolean(isPocket.value) &&
    Boolean(props.provider.enabled) &&
    props.provider.state !== 'error' &&
    (props.provider.state === 'loading' ||
      Boolean(selectedPocketModel.value && props.provider.active_model !== selectedPocketModel.value)),
);
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
    <p class="engine-readiness" :title="readiness">{{ readiness }}</p>
    <p v-if="provider.active_model && !isPocket" class="engine-active-model">
      <code>{{ provider.active_model }}</code>
    </p>
    <EngineModelPicker
      v-if="isPocket"
      :provider-id="provider.provider_id"
      :active-model="provider.active_model"
      :models="models"
      :enabled="provider.enabled"
      :state="provider.state"
      :switching="switching"
      :pending="pending"
      @activate="(model) => task.run(() => studio.activatePocketModel(model))"
    />
    <EngineModelList v-else-if="models.length" :models="models" :active-model="provider.active_model" />
    <p v-if="switching" role="status" class="help-note">
      {{ t('Preparing the selected model. This page updates automatically; the first download may take a few minutes.') }}
    </p>
    <p v-if="provider.detail" class="engine-detail">{{ t(provider.detail) }}</p>
    <EngineDiagnostics :provider="provider" />
    <div v-if="studio.schema.value[togglePath]" class="provider-actions">
      <span v-if="capabilities?.languages.length" class="muted">{{
        t('{count} audio languages', { count: capabilities.languages.length })
      }}</span
      ><button type="button" :disabled="pending" @click="task.run(() => studio.toggleProvider(provider))">
        {{ t(pending ? (provider.enabled ? 'Stopping…' : 'Starting…') : provider.enabled ? 'Disable' : 'Enable') }}
      </button>
    </div>
    <FeedbackNotice :feedback="feedback" />
  </article>
</template>
