<script setup lang="ts">
import { t, renderMessage, type Message } from '../../i18n';
import type { Job, QualityReport } from '../../services/contracts';
defineProps<{
  pending: boolean;
  snapshot: Job | null;
  complete: boolean;
  quality?: QualityReport;
  qualityIssues: QualityReport['issues'];
  error: Message | null;
  jobId: string | null;
}>();
</script>
<template>
  <div id="status" role="status" aria-live="polite" class="gen-status">
    <progress v-if="pending" class="generate-progress" :aria-label="t('Generating…')" />
    <p v-if="pending && snapshot" class="gen-line">
      <span v-if="snapshot.context" class="gen-context">{{ snapshot.context.model }} · {{ snapshot.context.effective_device }}</span>
      {{ t(snapshot.stage) }} · {{ t('{seconds}s elapsed', { seconds: Math.floor(snapshot.elapsed_secs || 0) }) }}<span v-if="snapshot.fragments_total"> · {{ t('{done} of {total} fragments', { done: snapshot.fragments_done ?? 0, total: snapshot.fragments_total }) }}</span><span v-else-if="snapshot.frames"> · {{ t('{count} generated frames', { count: snapshot.frames }) }}</span>
    </p>
    <p v-else-if="pending" class="gen-line">{{ t('Generating…') }}</p>
    <p v-if="complete" class="gen-done">{{ t('Completed. Audio is ready to play or download.') }}</p>
  </div>
  <details v-if="snapshot?.context" id="generationDiagnostics" class="gen-diagnostics">
    <summary>{{ t('Generation diagnostics') }}</summary>
    <dl class="gen-facts">
      <div><dt>{{ t('Model') }}</dt><dd>{{ snapshot.context.provider }} · {{ snapshot.context.model }} · {{ snapshot.context.effective_device }}</dd></div>
      <div><dt>{{ t('Voice') }}</dt><dd>{{ snapshot.context.voice }}</dd></div>
      <div v-if="snapshot.audio_secs"><dt>{{ t('Generated duration: {seconds}s', { seconds: snapshot.audio_secs.toFixed(2) }) }}</dt><dd>RTF {{ snapshot.rtf?.toFixed(2) ?? '—' }}</dd></div>
      <div v-if="snapshot.context.seed"><dt>{{ t('Seed: {seed}', { seed: snapshot.context.seed }) }}</dt></div>
      <div v-if="quality"><dt>{{ t('Sampling preset: {preset}', { preset: quality.mode }) }}</dt><dd>{{ t('Quality: {status}', { status: quality.quality_status }) }} · {{ t('Warnings: {count}', { count: qualityIssues.length }) }} · {{ t('Retries: {count}', { count: quality.retries }) }}</dd></div>
    </dl>
    <ul v-if="qualityIssues.length" class="quality-issues">
      <li v-for="issue in qualityIssues" :key="issue.kind">
        {{ t('{kind}: {detail}', { kind: issue.kind, detail: issue.detail }) }}
      </li>
    </ul>
  </details>
  <p v-if="error" role="alert" class="feedback error">
    {{ renderMessage(error) }}
    <span v-if="jobId">{{ t('Use Resume monitoring or Cancel generation.') }}</span>
  </p>
</template>
