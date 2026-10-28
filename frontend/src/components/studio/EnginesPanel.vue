<script setup lang="ts">
import { t } from '../../i18n';
import { useStudio } from '../../composables/useStudio';
import DisclosurePanel from '../ui/DisclosurePanel.vue';
import EngineCard from './EngineCard.vue';
import FeedbackNotice from '../FeedbackNotice.vue';
const { providers, capabilities, diagnostics, errors } = useStudio();
</script>
<template>
  <p class="panel-lead">{{ t('Enable an engine to download its models. The first download may take a few minutes.') }}</p>
  <FeedbackNotice :feedback="errors.providers ? { kind: 'error', message: errors.providers } : null"/>
  <div class="provider-list"><EngineCard v-for="provider in providers" :key="provider.provider_id" :provider="provider" :capabilities="capabilities.find(cap => cap.provider_id === provider.provider_id)"/></div>
  <DisclosurePanel :label="t('Advanced inference diagnostics')"><pre id="inferenceDiagnostics">{{ JSON.stringify(diagnostics, null, 2) }}</pre></DisclosurePanel>
</template>
