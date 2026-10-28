<script setup lang="ts">
import { computed, defineAsyncComponent, ref } from 'vue';
import { t } from '../../i18n';
import AppIcon from '../ui/AppIcon.vue';
import BaseTextarea from '../ui/BaseTextarea.vue';
import FileDropzone from '../ui/FileDropzone.vue';
import type { VoiceConstraints } from '../../services/contracts';
import type { ReferenceMetadata } from '../../features/cloning/validateReference';
const ReferenceEditor = defineAsyncComponent(() => import('../audio/ReferenceEditor.vue'));
const props = defineProps<{ disabled: boolean; analyzing?: boolean; provider: string; modes: string[]; file: File | null; originalFile: File | null; metadata: ReferenceMetadata | null; limits: VoiceConstraints | null; errors: Record<string, string>; needsTranscript: boolean }>();
const emit = defineEmits<{ select: [file: File | null]; trim: [file: File]; restore: [] }>();
const transcript = defineModel<string>('transcript', { required: true }), mode = defineModel<string>('mode', { required: true });
const picker = ref<{ open: () => void } | null>(null);
function select(file: File | null) { emit('select', file); }
function trim(file: File) { emit('trim', file); }
function modeLabel(value: string) { const key = `mode.${value}`; const label = t(key); return label === key ? value : label; }
function modeHint(value: string) { const key = `mode.${value}.hint`; const hint = t(key); return hint === key ? '' : hint; }
const modeOptions = computed(() => props.modes.map(value => ({ value, label: modeLabel(value), hint: modeHint(value) })));
const metaText = computed(() => {
  if (!props.file) return '';
  if (!props.metadata) return `${props.file.size} bytes`;
  const meta = props.metadata;
  return `${meta.duration.toFixed(2)} s · ${meta.sampleRate} Hz · ${meta.channels} ${meta.channels === 1 ? t('channel') : t('channels')} · ${meta.encoding}`;
});
const durationState = computed<{ kind: 'ok' | 'warn' | 'bad'; text: string } | null>(() => {
  const duration = props.metadata?.duration;
  const range = props.limits?.providers?.[props.provider]?.duration_seconds;
  if (duration === undefined || !range) return null;
  const [min, max] = range;
  if (duration < min) return { kind: 'bad', text: t('Too short: the engine needs at least {min} s.', { min }) };
  if (duration > max) return { kind: 'bad', text: t('Too long: the engine accepts at most {max} s.', { max }) };
  if (duration < 3 || duration > 10) return { kind: 'warn', text: t('Valid length, but 3–10 s clones best.') };
  return { kind: 'ok', text: t('Good length for cloning.') };
});
const uploadLimit = computed(() => {
  const bytes = props.limits?.max_upload_bytes;
  if (!bytes) return '';
  return bytes >= 1048576 ? `${Math.round(bytes / 1048576)} MB` : `${Math.round(bytes / 1024)} KB`;
});
const uploadHelp = computed(() => uploadLimit.value ? t('WAV only · up to {size}', { size: uploadLimit.value }) : t('WAV only'));
</script>
<template><fieldset class="reference-step" :disabled="disabled"><legend class="sr-only">{{ t('Reference recording') }}</legend>
  <p class="step-intro">{{ t('Upload 3–10 seconds of clear speech. Only use a voice you own or have permission to use.') }}</p>

  <FileDropzone id="cloneFile" ref="picker" :label="t('Reference recording (WAV, 3–10 s recommended)')" :help="uploadHelp" :error="errors.cloneFile" :disabled="disabled" :filled="!!file" @select="select">
    <div v-if="!file" class="dropzone-empty">
      <AppIcon name="upload" class="dropzone-icon" />
      <p class="dropzone-title">{{ t('Drop a WAV file here') }}</p>
      <p class="dropzone-hint">{{ t('or choose a file from your computer') }}</p>
    </div>
    <div v-else class="dropzone-filled">
      <AppIcon name="mic" class="file-icon" />
      <div class="file-details">
        <p class="file-name muted" :title="file.name">{{ file.name }}</p>
        <p class="file-meta" role="status">{{ metaText }}</p>
      </div>
      <div class="file-actions">
        <button type="button" class="btn-inline" :disabled="disabled" @click="picker?.open()">{{ t('Replace file') }}</button>
        <button type="button" class="btn-inline danger" :disabled="disabled" @click="select(null)">{{ t('Remove file') }}</button>
      </div>
    </div>
  </FileDropzone>

  <ReferenceEditor
    v-if="file && limits"
    :file="file"
    :max-bytes="limits.max_upload_bytes"
    :analyzing="analyzing"
    :status="durationState"
    :can-restore="!!originalFile && file !== originalFile"
    :disabled="disabled"
    @apply="trim"
    @restore="emit('restore')"
  />

  <fieldset id="cloneMode" class="mode-picker" tabindex="-1">
    <legend>{{ t('Mode') }}</legend>
    <p class="field-intro">{{ t('How the engine uses your recording') }}</p>
    <label v-for="option in modeOptions" :key="option.value" class="mode-option" :class="{ selected: mode === option.value }">
      <input type="radio" v-model="mode" :value="option.value" :name="'cloneMode-' + provider" :disabled="disabled" :aria-invalid="errors.cloneMode ? 'true' : undefined" :aria-describedby="errors.cloneMode ? 'cloneMode-error' : undefined">
      <span class="mode-text"><span class="mode-label">{{ option.label }}</span><span v-if="option.hint" class="mode-hint">{{ option.hint }}</span></span>
    </label>
    <p v-if="errors.cloneMode" id="cloneMode-error" class="field-error" role="alert">{{ errors.cloneMode }}</p>
  </fieldset>

  <p v-if="provider === 'qwen3-gguf'" class="callout"><span class="callout-icon" aria-hidden="true">i</span><span>{{ t('Qwen ICL needs the exact words, punctuation and language of the recording. A mismatched transcript can cause reference words to be repeated. Speaker-only mode is not validated.') }}</span></p>
  <p v-if="provider === 'pocket-tts'" class="callout"><span class="callout-icon" aria-hidden="true">i</span><span>{{ t('Pocket uses the recording without a transcript. Each language needs its own model; select a matching model and language.') }}</span></p>

  <BaseTextarea v-if="needsTranscript" id="cloneTranscript" v-model="transcript" :label="t('Exact transcript')" :error="errors.cloneTranscript" :help="`${[...transcript].length} / ${limits?.transcript_max_chars ?? '—'}`" required>
    <template #hint><p class="field-hint-text">{{ t('Type exactly what the reference audio says') }}</p></template>
  </BaseTextarea>
  <p v-if="needsTranscript && originalFile && file !== originalFile" class="callout warn"><span class="callout-icon" aria-hidden="true">!</span><span>{{ t('Review the transcript to match the trimmed recording. The transcript has not been rewritten.') }}</span></p>
</fieldset></template>
