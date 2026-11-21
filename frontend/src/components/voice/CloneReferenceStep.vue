<script setup lang="ts">
import { computed, defineAsyncComponent, ref } from 'vue';
import { t } from '../../i18n';
import { useStudio } from '../../composables/useStudio';
import AppIcon from '../ui/AppIcon.vue';
import BaseTextarea from '../ui/BaseTextarea.vue';
import DisclosurePanel from '../ui/DisclosurePanel.vue';
import FileDropzone from '../ui/FileDropzone.vue';
import type { VoiceConstraints } from '../../services/contracts';
import type { ReferenceMetadata } from '../../features/cloning/validateReference';
const ReferenceEditor = defineAsyncComponent(() => import('../audio/ReferenceEditor.vue'));
const props = defineProps<{ disabled: boolean; analyzing?: boolean; provider: string; modes: string[]; file: File | null; originalFile: File | null; metadata: ReferenceMetadata | null; limits: VoiceConstraints | null; errors: Record<string, string>; needsTranscript: boolean }>();
const emit = defineEmits<{ select: [file: File | null]; trim: [file: File]; restore: [] }>();
const transcript = defineModel<string>('transcript', { required: true }), mode = defineModel<string>('mode', { required: true });
const picker = ref<{ open: () => void } | null>(null);
const studio = useStudio();
const asrPhase = ref<'idle' | 'preparing' | 'transcribing' | 'error'>('idle');
const asrError = ref('');
const asrBusy = computed(() => asrPhase.value === 'preparing' || asrPhase.value === 'transcribing');
async function transcribeReference() {
  if (!props.file || asrBusy.value) return;
  asrError.value = '';
  asrPhase.value = 'preparing';
  try {
    await studio.api.prepareAsr();
    asrPhase.value = 'transcribing';
    transcript.value = (await studio.api.transcribe(props.file)).text;
    asrPhase.value = 'idle';
  } catch (error) {
    asrPhase.value = 'error';
    asrError.value = error instanceof Error ? error.message : t('Transcription failed.');
  }
}
function select(file: File | null) { emit('select', file); }
function trim(file: File) { emit('trim', file); }
function modeLabel(value: string) { const key = `mode.${value}`; const label = t(key); return label === key ? value : label; }
const modeOptions = computed(() => props.modes.map(value => ({ value, label: modeLabel(value) })));
const metaText = computed(() => (props.metadata ? `${props.metadata.duration.toFixed(2)} s` : ''));
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
  <FileDropzone data-guide="clone-file" id="cloneFile" ref="picker" :label="t('Reference recording (WAV, 3–10 s recommended)')" :help="uploadHelp" :error="errors.cloneFile" :disabled="disabled" :filled="!!file" @select="select">
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

  <DisclosurePanel v-if="file && limits" class="reference-trim" :label="t('Trim recording')">
    <ReferenceEditor
      :file="file"
      :max-bytes="limits.max_upload_bytes"
      :analyzing="analyzing"
      :status="durationState"
      :can-restore="!!originalFile && file !== originalFile"
      :disabled="disabled"
      @apply="trim"
      @restore="emit('restore')"
    />
  </DisclosurePanel>

  <fieldset v-if="modeOptions.length > 1" id="cloneMode" class="mode-picker" tabindex="-1">
    <legend>{{ t('Mode') }}</legend>
    <label v-for="option in modeOptions" :key="option.value" class="mode-option" :class="{ selected: mode === option.value }">
      <input type="radio" v-model="mode" :value="option.value" :name="'cloneMode-' + provider" :disabled="disabled" :aria-invalid="errors.cloneMode ? 'true' : undefined" :aria-describedby="errors.cloneMode ? 'cloneMode-error' : undefined">
      <span class="mode-text"><span class="mode-label">{{ option.label }}</span></span>
    </label>
    <p v-if="errors.cloneMode" id="cloneMode-error" class="field-error" role="alert">{{ errors.cloneMode }}</p>
  </fieldset>

  <p v-if="provider === 'reverb-tts'" class="callout"><span class="callout-icon" aria-hidden="true">i</span><span>{{ t('Reverb weights are CC BY-NC 4.0: non-commercial use only.') }}</span></p>

  <div v-if="needsTranscript && file" class="transcript-actions">
    <button type="button" class="btn-inline" :disabled="disabled || asrBusy" @click="transcribeReference">{{ t(asrPhase === 'preparing' ? 'Preparing model…' : asrPhase === 'transcribing' ? 'Transcribing…' : 'Transcribe') }}</button>
    <span v-if="asrPhase === 'error'" class="field-error" role="alert">{{ asrError }}</span>
  </div>
  <BaseTextarea v-if="needsTranscript" data-guide="clone-transcript" id="cloneTranscript" v-model="transcript" :label="t('Exact transcript')" :error="errors.cloneTranscript" :help="`${[...transcript].length} / ${limits?.transcript_max_chars ?? '—'}`" required/>
  <p v-if="needsTranscript && originalFile && file !== originalFile" class="callout warn"><span class="callout-icon" aria-hidden="true">!</span><span>{{ t('Review the transcript to match the trimmed recording. The transcript has not been rewritten.') }}</span></p>
</fieldset></template>
