<script setup lang="ts">
import { nextTick, onMounted, provide, ref, watch, defineAsyncComponent } from 'vue';
import { t } from '../i18n';
import type { HomeData } from '../types';
import { createStudio, studioKey } from '../composables/useStudio';
import StudioLayout, { type StudioTab } from '../components/StudioLayout.vue';
import SpeechGenerator from '../components/studio/SpeechGenerator.vue';
const VoiceLibrary = defineAsyncComponent(() => import('../components/studio/VoiceLibrary.vue'));
const EnginesPanel = defineAsyncComponent(() => import('../components/studio/EnginesPanel.vue'));
const SettingsPanel = defineAsyncComponent(() => import('../components/studio/SettingsPanel.vue'));
const GuidesPage = defineAsyncComponent(() => import('./GuidesPage.vue'));
import FeedbackNotice from '../components/FeedbackNotice.vue';
import GuideHint from '../components/GuideHint.vue';
import GuideLauncher from '../components/GuideLauncher.vue';
import ModelLoadingNotice from '../components/ModelLoadingNotice.vue';
const props = defineProps<{ data: HomeData }>();
const studio = createStudio(props.data); provide(studioKey, studio);
const { token, info, loading, errors } = studio;
const tabs: StudioTab[] = ['create', 'voices', 'engines', 'settings', 'guides'];
function tabFromHash(): StudioTab | null {
  const hash = window.location.hash.replace('#/', '').replace('#', '') as StudioTab;
  return tabs.includes(hash) ? hash : null;
}
const active = ref<StudioTab>(tabFromHash() ?? 'create');
onMounted(() => {
  const sync = () => { const tab = tabFromHash(); if (tab) active.value = tab; };
  window.addEventListener('hashchange', sync);
});
watch(active, value => {
  const hash = `#/${value}`;
  if (window.location.hash !== hash) history.replaceState(null, '', hash);
});
async function compose() { active.value = 'create'; await nextTick(); document.getElementById('text')?.focus(); }
</script>
<template>
  <ModelLoadingNotice />
  <StudioLayout v-model="active">
    <template #actions><ModelLoadingNotice mode="badge" /><button :disabled="loading" @click="studio.refresh()">{{ t('Refresh') }}</button><GuideLauncher v-if="active === 'create'" guide-id="first-audio"/><GuideHint v-else-if="active === 'engines'" guide-id="engines"/><GuideLauncher v-else-if="active === 'settings'" guide-id="key-settings"/></template>
    <template #notice><div v-if="info?.auth.required" id="tokenGroup" class="token-bar"><label for="apiToken">{{ t('API token') }}</label><input id="apiToken" data-guide="tokens-api-token" v-model.lazy="token" type="password" :placeholder="t('Paste your API token here')" autocomplete="off" spellcheck="false"><a href="/admin" data-guide="tokens-api-create">{{ t('create one in the admin panel') }}</a></div><FeedbackNotice :feedback="errors.info ? { kind: 'error', message: errors.info } : null"/></template>
    <template #create><KeepAlive><SpeechGenerator v-if="active === 'create'" @navigate="active = $event"/></KeepAlive></template>
    <template #voices><KeepAlive><VoiceLibrary v-if="active === 'voices'" @compose="compose" @engines="active = 'engines'"/></KeepAlive></template>
    <template #engines><KeepAlive><EnginesPanel v-if="active === 'engines'"/></KeepAlive></template>
    <template #settings><KeepAlive><SettingsPanel v-if="active === 'settings'"/></KeepAlive></template>
    <template #guides><GuidesPage v-if="active === 'guides'"/></template>
  </StudioLayout>
</template>
