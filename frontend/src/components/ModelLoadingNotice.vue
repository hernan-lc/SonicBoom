<script setup lang="ts">
import { computed } from 'vue';
import {
  type LoadingItem,
  loadingDetail,
  loadingItems,
  modelStatusLabel,
  notReadyDetail,
  useLoadingNotice,
} from '../composables/useLoadingNotice';
import { useStudio } from '../composables/useStudio';
import { t } from '../i18n';
import type { ProviderStatus } from '../services/contracts';
import AppIcon from './ui/AppIcon.vue';

const props = withDefaults(
  defineProps<{
    /** toast: global overlay; badge: header status; inline: one engine or model. */
    mode?: 'toast' | 'badge' | 'inline';
    engine?: ProviderStatus | null;
    model?: string | null;
  }>(),
  { mode: 'toast', engine: null, model: null },
);

const emit = defineEmits<{ manage: [] }>();

const studio = useStudio();
const { providers, info, asr } = studio;
const { items, visible, dismiss } = useLoadingNotice(providers, info, asr);

const badgeLabel = computed(() => modelStatusLabel(info?.value?.model.status));
const target = computed<ProviderStatus | null>(() => {
  if (props.engine) return props.engine;
  if (props.model) {
    const match = studio.models?.value?.find((entry) => entry.id === props.model);
    return providers?.value.find((provider) => provider.provider_id === match?.provider) ?? null;
  }
  return null;
});
const targetItem = computed<LoadingItem | null>(() => {
  const provider = target.value;
  return provider?.enabled && provider.state === 'loading'
    ? (loadingItems([provider], null)[0] ?? null)
    : null;
});
</script>
<template>
  <Transition v-if="mode === 'toast'" name="notice">
    <aside v-if="visible" class="loading-notice" role="status" aria-live="polite" :aria-label="t('Model loading')">
      <span class="button-spinner" aria-hidden="true"></span>
      <div class="loading-notice-body">
        <strong class="loading-notice-title">{{ t('Model loading') }}</strong>
        <ul class="loading-notice-list">
          <li v-for="item in items" :key="item.key" class="loading-notice-item">
            <span class="loading-notice-name">{{ item.name }}</span>
            <span class="loading-notice-detail">{{ loadingDetail(item) }}</span>
            <progress v-if="item.progress !== null" class="loading-notice-bar" :value="item.progress" max="100" :aria-label="t('Model progress')">{{ item.progress }}%</progress>
          </li>
        </ul>
        <a class="btn-inline" href="#/engines">{{ t('Engines') }}</a>
      </div>
      <button type="button" class="icon-button loading-notice-close" :aria-label="t('Close')" @click="dismiss"><AppIcon name="close" /></button>
    </aside>
  </Transition>
  <span v-else-if="mode === 'badge' && badgeLabel" class="model-status">{{ badgeLabel }}</span>
  <template v-else-if="mode === 'inline'">
    <p v-if="targetItem" role="status" class="help-note model-loading-inline">
      <span class="button-spinner" aria-hidden="true"></span>{{ loadingDetail(targetItem) }}
      <progress v-if="targetItem.progress !== null" class="loading-notice-bar" :value="targetItem.progress" max="100" :aria-label="t('Model progress')">{{ targetItem.progress }}%</progress>
    </p>
    <p v-else-if="model" class="help-note">
      {{ notReadyDetail(model) }}
      <button type="button" class="btn-inline" @click="emit('manage')">{{ t('Manage engines') }}</button>
    </p>
    <p v-else-if="engine?.detail" class="engine-detail">{{ t(engine.detail) }}</p>
  </template>
</template>
