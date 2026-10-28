<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import AppIcon from './ui/AppIcon.vue';
import LanguageSwitcher from './LanguageSwitcher.vue';
import { t } from '../i18n';
defineProps<{ page: string }>();
type Theme = 'system' | 'light' | 'dark';
const theme = ref<Theme>('system');
function apply(value: Theme) {
  theme.value = value;
  const root = document.documentElement;
  if (value === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', value);
  try { localStorage.setItem('sonicboom.theme', value); } catch { /* session-only */ }
}
function cycle() { apply(theme.value === 'system' ? 'light' : theme.value === 'light' ? 'dark' : 'system'); }
const themeName = computed(() => theme.value === 'system' ? t('System') : theme.value === 'light' ? t('Light') : t('Dark'));
onMounted(() => {
  try {
    const saved = localStorage.getItem('sonicboom.theme');
    if (saved === 'light' || saved === 'dark' || saved === 'system') apply(saved);
  } catch { /* session-only */ }
});
</script>
<template>
  <div class="app-shell">
    <a class="skip-link" href="#main-content">{{ t('Skip to content') }}</a>
    <header class="app-bar">
      <a href="/" class="brand" :aria-label="t('SonicBoom home')"><span class="brand-mark" aria-hidden="true"><AppIcon name="brand" /></span><span>SonicBoom</span></a>
      <div class="app-actions"><button type="button" class="theme-toggle" :aria-label="t('Color theme: {theme}', { theme: themeName })" :title="t('Toggle color theme')" @click="cycle"><AppIcon :name="theme === 'light' ? 'sun' : theme === 'dark' ? 'moon' : 'monitor'" /></button><LanguageSwitcher/><a v-if="page !== 'setup'" class="admin-link" :href="page === 'home' ? '/admin' : '/'">{{ t(page === 'home' ? 'API access' : 'Back to studio') }}</a></div>
    </header>
    <slot/>
    <footer class="app-footer"><span>{{ t('Your words. Your voice.') }}</span></footer>
  </div>
</template>
