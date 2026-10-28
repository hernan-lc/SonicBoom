<script setup lang="ts">
import { computed, ref } from 'vue';
import ConfirmDialog from '../ui/ConfirmDialog.vue';
import DisclosurePanel from '../ui/DisclosurePanel.vue';
import EmptyState from '../ui/EmptyState.vue';
import { t, uiError } from '../../i18n';
import type { Voice } from '../../services/contracts';
import { useStudio } from '../../composables/useStudio';
import { useTask } from '../../composables/useTask';
import VoiceCard from './VoiceCard.vue';
import VoiceCreatorDialog from './VoiceCreatorDialog.vue';
import FeedbackNotice from '../FeedbackNotice.vue';
const emit = defineEmits<{ compose: []; engines: [] }>();
const studio = useStudio(); const { voices, errors } = studio;
const task = useTask(); const { pending, feedback } = task;
async function select(voice: Voice, preview: boolean) {
  await task.run(async () => {
    if (!studio.profileCompatible(voice)) throw uiError('This voice belongs to an incompatible model revision or mode. Recreate it from a supported reference.');
    if (!studio.models.value.some(model => model.id === voice.model && model.ready)) throw uiError('Model {model} is not ready — enable its engine first.', { model: voice.model });
    studio.selectProfile(voice, preview); emit('compose');
  });
}
const creatorOpen = ref(false);
function created() { creatorOpen.value = false; emit('compose'); }
function openEngines() { creatorOpen.value = false; emit('engines'); }
const deleting = ref<Voice | null>(null);
const dialogOpen = computed({ get: () => !!deleting.value, set: value => { if (!value) deleting.value = null; } });
const query = ref(''), provider = ref(''), language = ref(''), model = ref(''), sort = ref('newest');
const providerOptions = computed(() => [...new Set(voices.value.map(voice => voice.provider))].sort());
const languageOptions = computed(() => [...new Set(voices.value.map(voice => voice.language))].sort());
const modelOptions = computed(() => [...new Set(voices.value.map(voice => voice.model))].sort());
const filtered = computed(() => voices.value.filter(voice =>
  (!provider.value || voice.provider === provider.value) && (!language.value || voice.language === language.value) &&
  (!model.value || voice.model === model.value) && `${voice.name} ${voice.model} ${voice.provider}`.toLocaleLowerCase().includes(query.value.toLocaleLowerCase())
).sort((a, b) => sort.value === 'name' ? a.name.localeCompare(b.name) : (sort.value === 'oldest' ? 1 : -1) * (a.created_at || '').localeCompare(b.created_at || '')));
const activeFilters = computed(() => [provider.value, language.value, model.value].filter(Boolean).length + (sort.value === 'newest' ? 0 : 1));
function usable(voice: Voice) { return ready(voice) && studio.profileCompatible(voice); }
function ready(voice: Voice) { return studio.models.value.some(model => model.id === voice.model && model.ready); }
async function remove() {
  const voice = deleting.value;
  if (!voice) return;
  await task.run(async () => {
    await studio.api.deleteVoice(voice.id); deleting.value = null;
    await studio.refreshVoices();
  });
}
</script>
<template>
  <FeedbackNotice :feedback="errors.voices ? { kind: 'error', message: errors.voices } : null"/>
  <div class="voice-toolbar"><p role="status">{{ t('{count} voices', { count: filtered.length }) }}</p><button v-if="voices.length" type="button" class="primary" @click="creatorOpen = true">{{ t('New voice') }}</button></div>
  <div class="voice-search"><label class="sr-only" for="voiceSearch">{{ t('Search voices') }}</label><input id="voiceSearch" v-model="query" type="search" :placeholder="t('Search voices')"></div>
  <DisclosurePanel class="voice-filters" :badge="activeFilters || undefined">
    <template #label>{{ t('Filters') }}</template>
    <div class="voice-filters-grid">
      <label>{{ t('Engine') }}<select v-model="provider"><option value="">{{ t('All providers') }}</option><option v-for="item in providerOptions" :key="item">{{ item }}</option></select></label>
      <label>{{ t('Language') }}<select v-model="language"><option value="">{{ t('All languages') }}</option><option v-for="item in languageOptions" :key="item">{{ t(`language.${item}`) }}</option></select></label>
      <label>{{ t('Model') }}<select v-model="model"><option value="">{{ t('All models') }}</option><option v-for="item in modelOptions" :key="item">{{ item }}</option></select></label>
      <label>{{ t('Sort') }}<select v-model="sort"><option value="newest">{{ t('Newest first') }}</option><option value="oldest">{{ t('Oldest first') }}</option><option value="name">{{ t('Name') }}</option></select></label>
    </div>
  </DisclosurePanel>
  <div id="voicesList" class="voice-list voice-grid"><EmptyState v-if="!voices.length && !errors.voices" :title="t('No cloned voices yet')"><button type="button" class="primary" @click="creatorOpen = true">{{ t('New voice') }}</button></EmptyState><VoiceCard v-for="voice in filtered" :key="voice.id" :voice="voice" :disabled="pending" :usable="usable(voice)" :compatible="studio.profileCompatible(voice)" :model-ready="ready(voice)" @use="select(voice, false)" @preview="select(voice, true)" @delete="deleting = voice" @engines="emit('engines')"/></div>
  <FeedbackNotice :feedback="feedback"/>
  <ConfirmDialog v-model="dialogOpen" :title="t('Delete {name}?', { name: deleting?.name || '' })" :description="t('Voice deletion cannot be undone. Its stored reference audio is deleted. Existing generated files remain available; future requests using this profile will fail.')" :pending="pending" @confirm="remove"/>
  <VoiceCreatorDialog v-model="creatorOpen" @created="created" @engines="openEngines"/>
</template>
