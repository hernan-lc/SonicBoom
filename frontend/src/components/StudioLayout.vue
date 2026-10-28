<script setup lang="ts">
import { computed, nextTick } from 'vue';
import { t } from '../i18n';
import AppIcon, { type IconName } from './ui/AppIcon.vue';
export type StudioTab = 'create' | 'voices' | 'engines' | 'settings';
const active = defineModel<StudioTab>({ required: true });
const sections: { id: StudioTab; label: string; description: string; icon: IconName }[] = [
  { id: 'create', label: 'Create speech', description: 'Turn your ideas into natural speech.', icon: 'create' },
  { id: 'voices', label: 'Voice library', description: 'Create a voice that sounds like you.', icon: 'voices' },
  { id: 'engines', label: 'Engines', description: 'Choose what powers your voice.', icon: 'engines' },
  { id: 'settings', label: 'Settings', description: 'Make SonicBoom work your way.', icon: 'settings' },
];
const current = computed(() => sections.find(section => section.id === active.value)!);
async function navigate(event: KeyboardEvent) {
  const index = sections.findIndex(section => section.id === active.value);
  let next: number;
  if (event.key === 'ArrowDown' || event.key === 'ArrowRight') next = (index + 1) % sections.length;
  else if (event.key === 'ArrowUp' || event.key === 'ArrowLeft') next = (index + sections.length - 1) % sections.length;
  else if (event.key === 'Home') next = 0;
  else if (event.key === 'End') next = sections.length - 1;
  else return;
  event.preventDefault(); active.value = sections[next].id;
  await nextTick();
  document.getElementById(`tab-${active.value}`)?.focus();
}
</script>
<template>
  <div class="studio-layout">
    <aside class="workspace-nav">
      <div role="tablist" :aria-label="t('Workspace')" @keydown="navigate">
        <button v-for="section in sections" :id="`tab-${section.id}`" :key="section.id" role="tab" :aria-selected="active === section.id" :aria-controls="`panel-${section.id}`" :tabindex="active === section.id ? 0 : -1" :class="{ selected: active === section.id }" :title="t(section.description)" @click="active = section.id"><AppIcon :name="section.icon" class="nav-icon" /><span class="nav-label">{{ t(section.label) }}</span></button>
      </div>
    </aside>
    <main id="main-content" class="workspace-content" tabindex="-1">
      <header class="workspace-heading"><div><h1>{{ t(current.label) }}</h1><p>{{ t(current.description) }}</p></div><slot name="actions"/></header>
      <slot name="notice"/>
      <section v-for="section in sections" :id="`panel-${section.id}`" v-show="active === section.id" :key="section.id" role="tabpanel" :aria-labelledby="`tab-${section.id}`" class="studio-panel"><slot :name="section.id"/></section>
    </main>
  </div>
</template>
