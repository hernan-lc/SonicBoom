<script setup lang="ts">
import { computed, ref } from 'vue';
import { t } from '../../i18n';
import type { ProviderStatus, Capabilities } from '../../services/contracts';
import { getPath, useStudio } from '../../composables/useStudio';
import { useTask } from '../../composables/useTask';
import FeedbackNotice from '../FeedbackNotice.vue';
import StatusChip from '../ui/StatusChip.vue';
const props = defineProps<{ provider: ProviderStatus; capabilities?: Capabilities }>();
const studio = useStudio();
const task = useTask();
const { pending, feedback } = task;
const togglePath = computed(() => `providers.${props.provider.provider_id.replaceAll('-', '_')}.enabled`);
const isPocket = computed(() => props.provider.provider_id === 'pocket-tts');
const selectedPocketModel = computed(() => {
  const value = getPath(studio.config.value?.config, 'providers.pocket_tts.model_id') || props.provider.active_model;
  return typeof value === 'string' ? value : '';
});
const switching = computed(() => isPocket.value && props.provider.enabled && props.provider.state !== 'error' && (
  props.provider.state === 'loading' || (selectedPocketModel.value && props.provider.active_model !== selectedPocketModel.value)
));
function active(model: string) {
  if (!isPocket.value) return props.provider.active_model === model;
  return props.provider.enabled && props.provider.state === 'ready' && props.provider.active_model === model && selectedPocketModel.value === model;
}
// Draft selection for the pocket model picker; applying is always explicit
// through the Activate button, and the draft resets once config catches up.
const pocketDraft = ref<string | null>(null);
const pocketChoice = computed({
  get: () => pocketDraft.value ?? selectedPocketModel.value ?? props.provider.active_model ?? '',
  set: value => { pocketDraft.value = value; },
});
async function activateChoice() {
  const model = pocketChoice.value;
  if (!model || active(model)) return;
  await task.run(() => studio.activatePocketModel(model));
  pocketDraft.value = null;
}
const vulkanError = computed(() => props.provider.backend_diagnostics?.vulkan_initialization_error || '');
const cpuFallback = computed(() => props.provider.backend_diagnostics?.cpu_fallback_reason || '');
const stateTone = computed(() => props.provider.state === 'ready' ? 'ok' as const : props.provider.state === 'loading' ? 'info' as const : props.provider.state === 'disabled' ? 'neutral' as const : 'warn' as const);
const readiness = computed(() => [
  props.provider.enabled ? t('Engine enabled') : t('Engine disabled'),
  props.provider.active_model ? t('Model loaded') : t('No model loaded'),
  props.provider.state === 'ready' ? t('Ready to generate') : t('Not ready'),
].join(' · '));
const modelCount = computed(() => props.capabilities?.models.length ?? 0);
</script>
<template>
  <article class="provider" :aria-busy="pending">
    <div class="provider-top"><h2 class="provider-name">{{ provider.provider_id }}</h2><StatusChip :tone="stateTone" :label="t(provider.state)" /><span v-if="provider.effective_device" class="device-chip">{{ provider.effective_device }}</span></div>
    <p class="engine-readiness" :title="readiness">{{ readiness }}</p>
    <p v-if="provider.active_model && !isPocket" class="engine-active-model"><code>{{ provider.active_model }}</code></p>
    <div v-if="isPocket" class="engine-model-picker"><label :for="`engine-model-${provider.provider_id}`">{{ t('Model') }}<select :id="`engine-model-${provider.provider_id}`" v-model="pocketChoice" :disabled="pending || switching || !studio.schema.value['providers.pocket_tts.model_id']"><option v-for="model in capabilities?.models || []" :key="model" :value="model">{{ model }}</option></select></label><button type="button" class="engine-activate" :disabled="pending || switching || !pocketChoice || active(pocketChoice) || !studio.schema.value['providers.pocket_tts.model_id']" @click="activateChoice">{{ t(switching ? 'Loading…' : 'Activate') }}</button></div>
    <details v-else-if="modelCount" class="engine-models-details"><summary>{{ t('Models') }} ({{ modelCount }})</summary>
      <ul class="engine-models"><li v-for="model in capabilities?.models || []" :key="model">
        <code>{{ model }}</code>
        <span v-if="active(model)" class="badge badge-ready">{{ t('Active') }}</span>
      </li></ul>
    </details>
    <p v-if="switching" role="status" class="help-note">{{ t('Preparing the selected model. This page updates automatically; the first download may take a few minutes.') }}</p>
    <p v-if="provider.detail" class="engine-detail">{{ t(provider.detail) }}</p>
    <details v-if="provider.backend_diagnostics || vulkanError || cpuFallback" class="engine-solution"><summary>{{ t('View solution') }}</summary>
    <dl v-if="provider.backend_diagnostics" class="help-note">
      <dt>{{ provider.backend_diagnostics.status }}</dt>
      <dd>{{ provider.backend_diagnostics.vulkan_gpu_name }}</dd>
      <dt>{{ t('Compiled / available') }}</dt><dd>{{ provider.backend_diagnostics.compiled_backends.join(', ') }} / {{ provider.backend_diagnostics.available_runtime_backends.join(', ') }}</dd>
      <dt>{{ t('Requested / effective') }}</dt><dd>{{ provider.backend_diagnostics.requested_device }} / {{ provider.backend_diagnostics.effective_device || '—' }}</dd>
      <dt>{{ t('Model') }}</dt><dd>{{ provider.backend_diagnostics.model_loading_status }}</dd>
      <dd v-if="cpuFallback">{{ cpuFallback }}</dd>
      <dd v-else-if="vulkanError">{{ vulkanError }}</dd>
    </dl>
    <p v-if="vulkanError" class="help-note">{{ t('This build cannot start with Vulkan on this machine. Enable the engine only if CPU inference is supported for this model.') }}</p>
    </details>
    <div v-if="studio.schema.value[togglePath]" class="provider-actions"><span v-if="capabilities?.languages.length" class="muted">{{ t('{count} audio languages', { count: capabilities.languages.length }) }}</span><button :disabled="pending" @click="task.run(() => studio.toggleProvider(provider))">{{ t(pending ? provider.enabled ? 'Stopping…' : 'Starting…' : provider.enabled ? 'Disable' : 'Enable') }}</button></div>
    <FeedbackNotice :feedback="feedback"/>
  </article>
</template>
