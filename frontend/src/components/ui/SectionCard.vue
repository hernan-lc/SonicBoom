<script setup lang="ts">
defineProps<{ title?: string; subtitle?: string }>();
</script>
<template>
  <section class="section-card">
    <header v-if="title || $slots.actions" class="section-card-head">
      <div class="section-card-titles">
        <h2 v-if="title" class="section-card-title">{{ title }}</h2>
        <p v-if="subtitle" class="section-card-subtitle">{{ subtitle }}</p>
      </div>
      <div v-if="$slots.actions" class="section-card-actions"><slot name="actions"/></div>
    </header>
    <slot/>
  </section>
</template>
