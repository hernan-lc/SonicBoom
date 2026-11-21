<script setup lang="ts">
import { loadingDetail, useLoadingNotice } from '../composables/useLoadingNotice';
import { useStudio } from '../composables/useStudio';
import { t } from '../i18n';
import AppIcon from './ui/AppIcon.vue';

const { providers, info } = useStudio();
const { items, visible, dismiss } = useLoadingNotice(providers, info);
</script>
<template>
  <Transition name="notice">
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
</template>
