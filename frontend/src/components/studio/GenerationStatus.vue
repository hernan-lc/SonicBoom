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
  <div id="status" role="status" aria-live="polite">
    <progress v-if="pending" class="generate-progress" :aria-label="t('Generating…')" />
    <template v-if="pending && snapshot">
      <span v-if="snapshot.context">{{ snapshot.context.model }} · {{ snapshot.context.effective_device }} · </span>
      {{ t(snapshot.stage) }} ·
      {{ t('{seconds}s elapsed', { seconds: Math.floor(snapshot.elapsed_secs || 0) }) }}
      <span v-if="snapshot.frames"> · {{ t('{count} generated frames', { count: snapshot.frames }) }}</span>
      <span v-if="snapshot.fragments_total">
        · {{ t('{done} of {total} fragments', { done: snapshot.fragments_done ?? 0, total: snapshot.fragments_total }) }}
      </span>
      <p v-if="snapshot.context?.provider === 'qwen3-gguf' && snapshot.context.effective_device === 'cpu'" class="muted">
        {{ t('CPU inference can be slow.') }}
      </p>
    </template>
    <span v-if="complete">{{ t('Completed. Audio is ready to play or download.') }}</span>
  </div>
  <details v-if="snapshot?.context" id="generationDiagnostics">
    <summary>{{ t('Generation diagnostics') }}</summary>
    <p>
      {{ snapshot.context.provider }} · {{ snapshot.context.model }} · {{ snapshot.context.effective_device }} ·
      {{ snapshot.context.voice }}
    </p>
    <p v-if="snapshot.audio_secs">
      {{ t('Generated duration: {seconds}s', { seconds: snapshot.audio_secs.toFixed(2) }) }} · RTF
      {{ snapshot.rtf?.toFixed(2) ?? '—' }}
    </p>
    <p v-if="snapshot.context.seed">{{ t('Seed: {seed}', { seed: snapshot.context.seed }) }}</p>
    <p v-else>{{ t('Seed: not exposed by this API') }}</p>
    <p v-if="quality">
      {{ t('Sampling preset: {preset}', { preset: quality.mode }) }} ·
      {{ t('Quality: {status}', { status: quality.quality_status }) }} ·
      {{ t('Warnings: {count}', { count: qualityIssues.length }) }} ·
      {{ t('Retries: {count}', { count: quality.retries }) }}
    </p>
    <ul v-if="qualityIssues.length" class="quality-issues">
      <li v-for="issue in qualityIssues" :key="issue.kind">
        {{ t('{kind}: {detail}', { kind: issue.kind, detail: issue.detail }) }}
      </li>
    </ul>
    <p v-if="quality?.echo?.asr_required" class="muted">
      {{ t('Reference-echo detection needs an offline ASR transcript; none is claimed here.') }}
    </p>
  </details>
  <p v-if="error" role="alert" class="feedback error">
    {{ renderMessage(error) }}
    <span v-if="jobId">{{ t('Use Resume monitoring or Cancel generation.') }}</span>
  </p>
</template>
