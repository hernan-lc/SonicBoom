<script setup lang="ts">
defineProps<{ tone?: 'ok' | 'warn' | 'bad' | 'info' | 'neutral'; label?: string }>();
</script>
<template>
  <span class="status-chip" :class="`status-chip-${tone || 'neutral'}`"><span class="status-chip-dot" aria-hidden="true"/>{{ label }}<slot/></span>
</template>
