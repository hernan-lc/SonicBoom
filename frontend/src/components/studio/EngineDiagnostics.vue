<script setup lang="ts">
import { computed } from 'vue';
import { t } from '../../i18n';
import type { ProviderStatus } from '../../services/contracts';
import DisclosurePanel from '../ui/DisclosurePanel.vue';

const props = defineProps<{ provider: ProviderStatus }>();
const vulkanError = computed(() => props.provider.backend_diagnostics?.vulkan_initialization_error || '');
const cpuFallback = computed(() => props.provider.backend_diagnostics?.cpu_fallback_reason || '');
const visible = computed(() => !!(props.provider.backend_diagnostics || vulkanError.value || cpuFallback.value));
</script>
<template>
  <DisclosurePanel v-if="visible" class="engine-solution" :label="t('View solution')">
    <dl v-if="provider.backend_diagnostics" class="help-note">
      <dt>{{ provider.backend_diagnostics.status }}</dt>
      <dd>{{ provider.backend_diagnostics.vulkan_gpu_name }}</dd>
      <dt>{{ t('Compiled / available') }}</dt>
      <dd>
        {{ provider.backend_diagnostics.compiled_backends.join(', ') }} /
        {{ provider.backend_diagnostics.available_runtime_backends.join(', ') }}
      </dd>
      <dt>{{ t('Requested / effective') }}</dt>
      <dd>
        {{ provider.backend_diagnostics.requested_device }} /
        {{ provider.backend_diagnostics.effective_device || '—' }}
      </dd>
      <dt>{{ t('Model') }}</dt>
      <dd>{{ provider.backend_diagnostics.model_loading_status }}</dd>
      <dd v-if="cpuFallback">{{ cpuFallback }}</dd>
      <dd v-else-if="vulkanError">{{ vulkanError }}</dd>
    </dl>
    <p v-if="vulkanError" class="help-note">
      {{
        t(
          'This build cannot start with Vulkan on this machine. Enable the engine only if CPU inference is supported for this model.',
        )
      }}
    </p>
  </DisclosurePanel>
</template>
