<script setup lang="ts">
import { computed } from 'vue';

export type IconName =
  | 'create'
  | 'voices'
  | 'engines'
  | 'settings'
  | 'chevron-down'
  | 'info'
  | 'arrow-right'
  | 'play'
  | 'pause'
  | 'skip-forward'
  | 'close'
  | 'refresh'
  | 'restore'
  | 'plus'
  | 'trash'
  | 'download'
  | 'upload'
  | 'mic'
  | 'check'
  | 'alert'
  | 'sun'
  | 'moon'
  | 'monitor'
  | 'brand';

const paths: Record<IconName, string> = {
  create: 'M12 3v10m0 0 3-3m-3 3-3-3M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4',
  voices: 'M4 10v4m4-7v10m4-13v16m4-12v8m4-9v10',
  engines: 'M12 2 3 7v10l9 5 9-5V7l-9-5Zm0 5v10M3 7l9 5 9-5',
  settings:
    'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm7-3a7 7 0 0 0-.1-1.2l2-1.5-2-3.4-2.3 1a7 7 0 0 0-2-1.2L14.2 3h-4l-.4 2.7a7 7 0 0 0-2 1.2l-2.3-1-2 3.4 2 1.5A7 7 0 0 0 5 12c0 .4 0 .8.1 1.2l-2 1.5 2 3.4 2.3-1a7 7 0 0 0 2 1.2l.4 2.7h4l.4-2.7a7 7 0 0 0 2-1.2l2.3 1 2-3.4-2-1.5c.1-.4.1-.8.1-1.2Z',
  'chevron-down': 'm6 9 6 6 6-6',
  info: 'M12 16v-4m0-4h.01M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z',
  'arrow-right': 'M5 12h14m-6-6 6 6-6 6',
  play: 'M8 5.5v13a1 1 0 0 0 1.5.87l11-6.5a1 1 0 0 0 0-1.74l-11-6.5A1 1 0 0 0 8 5.5Z',
  pause: 'M7 5h3.5v14H7zM13.5 5H17v14h-3.5z',
  close: 'm6 6 12 12M18 6 6 18',
  refresh: 'M21 12a9 9 0 1 1-2.64-6.36M21 3v6h-6',
  plus: 'M12 5v14M5 12h14',
  trash: 'M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2m3 0-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6',
  download: 'M12 3v12m0 0 4-4m-4 4-4-4M4 21h16',
  check: 'm4 12.5 5 5L20 6.5',
  alert: 'M12 9v4m0 4h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z',
  'skip-forward': 'M4 5v14M20 5l-11 7 11 7V5Z',
  restore: 'M4 9V5h4M4.6 13a8 8 0 1 0 2-7.5L4 9',
  upload: 'M12 15V4m-4.5 4.5L12 4l4.5 4.5M4 15v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3',
  mic: 'M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3ZM5 11a7 7 0 0 0 14 0M12 18v3',
  sun: 'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM12 2v2m0 16v2M4.9 4.9l1.4 1.4m11.4 11.4 1.4 1.4M2 12h2m16 0h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4',
  moon: 'M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z',
  monitor: 'M4 3h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2ZM8 21h8m-4-4v4',
  brand: 'M3 9v6m5-10v14m4-11v8m4-13v18m5-12v6',
};

const props = withDefaults(defineProps<{ name: IconName; filled?: boolean }>(), { filled: false });
const path = computed(() => paths[props.name]);
</script>
<template>
  <svg
    class="app-icon"
    width="18"
    height="18"
    viewBox="0 0 24 24"
    :fill="filled ? 'currentColor' : 'none'"
    :stroke="filled ? 'none' : 'currentColor'"
    stroke-width="1.8"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path :d="path" :fill="filled ? 'currentColor' : 'none'" />
  </svg>
</template>
