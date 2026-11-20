<script setup lang="ts">
import { computed } from 'vue';
import { t } from '../../i18n';
import { useStudio } from '../../composables/useStudio';
import GuideHint from '../GuideHint.vue';
defineProps<{ disabled?: boolean }>();
const { info, normalizationAvailable, normalizationMode, readingStyle, pronunciationLocale, pronunciationLocales, showPronunciationLocale, normalizationLanguage } = useStudio();
// Locales and the default come from /api/info; nothing is hard-coded here.
const defaultLocale = computed(() => {
  const configured = info.value?.normalization?.default_locale;
  return configured && pronunciationLocales.value.some(tag => tag.toLowerCase() === configured.toLowerCase()) ? configured : normalizationLanguage.value?.default_locale ?? '';
});
</script>
<template>
  <div v-if="normalizationAvailable" class="row pronunciation-settings">
    <div class="field">
      <div class="label-with-hint"><label for="pronunciation">{{ t('Pronunciation') }}</label><GuideHint guide-id="pronunciation"/></div>
      <select id="pronunciation" data-guide="pronunciation-mode" v-model="normalizationMode" :disabled="disabled || info?.normalization?.enabled === false">
        <option value="auto">{{ t('Automatic (recommended)') }}</option>
        <option value="off">{{ t('Original text') }}</option>
      </select>
    </div>
    <div v-if="showPronunciationLocale" class="field">
      <label for="pronunciationLocale">{{ t('Regional pronunciation') }}</label>
      <select id="pronunciationLocale" data-guide="pronunciation-region" v-model="pronunciationLocale" :disabled="disabled">
        <option value="">{{ defaultLocale ? `Server default (${defaultLocale})` : 'Server default' }}</option>
        <option value="auto">{{ normalizationLanguage?.default_locale ? `Language default (${normalizationLanguage.default_locale})` : 'Language default' }}</option>
        <option v-for="tag in pronunciationLocales" :key="tag" :value="tag">{{ tag }}</option>
      </select>
    </div>
    <div v-if="normalizationMode === 'auto'" class="field">
      <label for="readingStyle">{{ t('Reading style') }}</label>
      <select id="readingStyle" data-guide="pronunciation-style" v-model="readingStyle" :disabled="disabled">
        <option value="precise">{{ t('Precise') }}</option>
        <option value="natural">{{ t('Natural') }}</option>
        <option value="digits">{{ t('Digit by digit') }}</option>
      </select>
    </div>
  </div>
</template>
