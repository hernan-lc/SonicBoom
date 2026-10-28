<script setup lang="ts">
import { t } from '../../i18n';
import type { Voice } from '../../services/contracts';
import DisclosurePanel from '../ui/DisclosurePanel.vue';
import StatusDot from '../ui/StatusDot.vue';

defineProps<{ voice: Voice; disabled: boolean; usable: boolean; compatible: boolean; modelReady: boolean }>();
const emit = defineEmits<{ use: []; preview: []; delete: []; engines: [] }>();
</script>
<template>
  <article class="voice-row voice-card">
    <div class="voice-card-main">
      <h2 class="voice-name">{{ voice.name }}</h2>
      <p class="voice-meta">
        <StatusDot :state="usable ? 'ok' : 'warn'" :label="t(usable ? 'Ready' : 'Unavailable')" /><span
          class="voice-tags"
          >{{ voice.provider }} · {{ t(`language.${voice.language}`) }} · {{ voice.model }}</span
        >
      </p>
    </div>
    <div class="voice-actions">
      <button type="button" :disabled="disabled || !usable" @click="emit('use')">{{ t('Use') }}</button>
      <button type="button" :disabled="disabled || !usable" @click="emit('preview')">{{ t('Preview') }}</button>
      <button type="button" class="danger ghost" :disabled="disabled" @click="emit('delete')">
        {{ t('Delete') }}
      </button>
    </div>
    <DisclosurePanel class="voice-details" :label="t('Details')">
      <p class="voice-meta">
        {{ voice.clone_mode
        }}<span v-if="voice.reference_duration_secs">
          · {{ voice.reference_duration_secs.toFixed(1) }}s {{ t('reference') }}</span
        >
      </p>
      <p v-if="!compatible" class="help-note">
        {{ t('This voice belongs to an incompatible model revision or mode. Recreate it from a supported reference.') }}
      </p>
      <p v-else-if="!modelReady" class="help-note">
        {{ t('Model {model} is not ready — enable its engine first.', { model: voice.model }) }}
        <button type="button" class="btn-inline" @click="emit('engines')">{{ t('Manage engines') }}</button>
      </p>
    </DisclosurePanel>
  </article>
</template>
