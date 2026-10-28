<script setup lang="ts">
import { computed, nextTick } from 'vue';
import { t } from '../i18n';
export type StudioTab = 'create' | 'voices' | 'engines' | 'settings';
const active = defineModel<StudioTab>({ required: true });
const sections: { id: StudioTab; label: string; description: string; icon: string }[] = [
  { id: 'create', label: 'Create speech', description: 'Turn your ideas into natural speech.', icon: 'M12 3v10m0 0 3-3m-3 3-3-3M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4' },
  { id: 'voices', label: 'Voice library', description: 'Create a voice that sounds like you.', icon: 'M4 10v4m4-7v10m4-13v16m4-12v8m4-9v10' },
  { id: 'engines', label: 'Engines', description: 'Choose what powers your voice.', icon: 'M12 2 3 7v10l9 5 9-5V7l-9-5Zm0 5v10M3 7l9 5 9-5' },
  { id: 'settings', label: 'Settings', description: 'Make SonicBoom work your way.', icon: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm7-3a7 7 0 0 0-.1-1.2l2-1.5-2-3.4-2.3 1a7 7 0 0 0-2-1.2L14.2 3h-4l-.4 2.7a7 7 0 0 0-2 1.2l-2.3-1-2 3.4 2 1.5A7 7 0 0 0 5 12c0 .4 0 .8.1 1.2l-2 1.5 2 3.4 2.3-1a7 7 0 0 0 2 1.2l.4 2.7h4l.4-2.7a7 7 0 0 0 2-1.2l2.3 1 2-3.4-2-1.5c.1-.4.1-.8.1-1.2Z' },
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
        <button v-for="section in sections" :id="`tab-${section.id}`" :key="section.id" role="tab" :aria-selected="active === section.id" :aria-controls="`panel-${section.id}`" :tabindex="active === section.id ? 0 : -1" :class="{ selected: active === section.id }" :title="t(section.description)" @click="active = section.id"><svg class="nav-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path :d="section.icon"/></svg><span class="nav-label">{{ t(section.label) }}</span></button>
      </div>
    </aside>
    <main id="main-content" class="workspace-content" tabindex="-1">
      <header class="workspace-heading"><div><h1>{{ t(current.label) }}</h1><p>{{ t(current.description) }}</p></div><slot name="actions"/></header>
      <slot name="notice"/>
      <section v-for="section in sections" :id="`panel-${section.id}`" v-show="active === section.id" :key="section.id" role="tabpanel" :aria-labelledby="`tab-${section.id}`" class="studio-panel"><slot :name="section.id"/></section>
    </main>
  </div>
</template>
