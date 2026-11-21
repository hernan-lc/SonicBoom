<script setup lang="ts">
import { computed } from 'vue';
import { t } from '../../i18n';
import { useStudio } from '../../composables/useStudio';
import DisclosurePanel from '../ui/DisclosurePanel.vue';
import EngineCard from './EngineCard.vue';
import FeedbackNotice from '../FeedbackNotice.vue';
const { providers, capabilities, diagnostics, errors } = useStudio();
const hasReverb = computed(() => providers.value.some((provider) => provider.provider_id === 'reverb-tts'));
</script>
<template>
  <p v-if="hasReverb" data-guide="engine-license" class="help-note">{{ t('Reverb weights are CC BY-NC 4.0: non-commercial use only.') }}</p>
  <FeedbackNotice :feedback="errors.providers ? { kind: 'error', message: errors.providers } : null"/>
  <div class="provider-list"><EngineCard v-for="provider in providers" :key="provider.provider_id" :provider="provider" :capabilities="capabilities.find(cap => cap.provider_id === provider.provider_id)"/></div>
  <DisclosurePanel :label="t('Advanced inference diagnostics')"><pre id="inferenceDiagnostics">{{ JSON.stringify(diagnostics, null, 2) }}</pre></DisclosurePanel>
</template>
