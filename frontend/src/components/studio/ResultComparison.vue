<script setup lang="ts">
import { t } from '../../i18n';
import type { GenerationResult } from '../../composables/useSynthesis';
import type { PlayerHandle } from '../audio/playerTypes';
import DisclosurePanel from '../ui/DisclosurePanel.vue';
import WaveformPlayer from '../audio/WaveformPlayer.vue';
defineProps<{
  results: GenerationResult[];
  register: (id: string, handle: PlayerHandle | null) => void;
}>();
const emit = defineEmits<{ play: [id: string]; remove: [index: number] }>();
</script>
<template>
  <DisclosurePanel v-if="results.length > 1" id="resultComparison" :label="t('Compare generated versions')">
    <ul class="result-list">
      <li v-for="(result, index) in results" :key="result.jobId">
        <WaveformPlayer
          :ref="(handle) => register(result.jobId, handle as PlayerHandle | null)"
          :blob="result.blob"
          :label="t('Generated version {version}', { version: index + 1 })"
          compact
          @play="emit('play', result.jobId)"
        />
        <span class="muted">
          {{ result.text.slice(0, 80) }} · {{ result.voice }} · {{ t(`language.${result.language}`) }} ·
          {{ result.model }} · {{ t('{seconds}s', { seconds: result.durationSecs.toFixed(2) }) }}
          <template v-if="result.preset"> · {{ t('Preset: {preset}', { preset: result.preset }) }}</template>
          <template v-if="result.seed"> · {{ t('Seed: {seed}', { seed: result.seed }) }}</template>
          <template v-if="result.quality"> · {{ t('Quality: {status}', { status: result.quality.quality_status }) }}</template>
        </span>
        <a :href="result.url" :download="result.filename">{{ t('Download') }}</a>
        <button type="button" class="btn-inline" @click="emit('remove', index)">{{ t('Remove') }}</button>
      </li>
    </ul>
  </DisclosurePanel>
</template>
