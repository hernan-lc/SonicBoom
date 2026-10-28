<script setup lang="ts">
import { ref, useId, watch } from 'vue';
import AppIcon from './AppIcon.vue';

const props = withDefaults(
  defineProps<{ label?: string; badge?: string | number; defaultOpen?: boolean; contentId?: string }>(),
  { defaultOpen: false },
);
const emit = defineEmits<{ opened: [] }>();
const open = ref(props.defaultOpen);
const panelId = props.contentId ?? `disclosure-${useId()}`;
const triggerId = `${panelId}-trigger`;
function toggle() {
  open.value = !open.value;
  if (open.value) emit('opened');
}
watch(
  () => props.defaultOpen,
  (value) => {
    if (value && !open.value) toggle();
  },
);
defineExpose({ open });
</script>
<template>
  <div class="disclosure" :class="{ 'disclosure-open': open }">
    <button
      :id="triggerId"
      type="button"
      class="disclosure-trigger"
      :aria-expanded="open"
      :aria-controls="panelId"
      @click="toggle"
    >
      <AppIcon name="chevron-down" class="disclosure-chevron" />
      <span class="disclosure-label"><slot name="label">{{ label }}</slot></span>
      <span v-if="badge || $slots.badge" class="disclosure-badge"><slot name="badge">{{ badge }}</slot></span>
    </button>
    <section :id="panelId" class="disclosure-panel" :aria-labelledby="triggerId">
      <div class="disclosure-panel-inner"><slot /></div>
    </section>
  </div>
</template>
