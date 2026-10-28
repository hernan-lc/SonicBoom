<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef, useId, watch } from 'vue';
import { t, renderMessage, errorMessage, type Message } from '../../i18n';
import { inspectWave, analyzeWave, cropWave, type WaveFile, type WaveAnalysis } from '../../features/audio/wav';

const props = withDefaults(
  defineProps<{
    file: File;
    maxBytes: number;
    analyzing?: boolean;
    status?: { kind: 'ok' | 'warn' | 'bad'; text: string } | null;
    canRestore?: boolean;
    disabled?: boolean;
  }>(),
  { analyzing: false, status: null, canRestore: false, disabled: false },
);
const emit = defineEmits<{ apply: [file: File]; restore: [] }>();

const MIN_TRIM = 0.01;
const wave = shallowRef<WaveFile | null>(null);
const health = shallowRef<WaveAnalysis | null>(null);
const pending = ref(true);
const error = shallowRef<Message | null>(null);
const start = ref<string | number>(0);
const end = ref<string | number>(0);
const title = useId();
const track = ref<HTMLElement | null>(null);
const audio = ref<HTMLAudioElement | null>(null);
const audioUrl = ref('');
const playing = ref(false);
const position = ref(0);
const playingRange = ref<[number, number] | null>(null);
let controller: AbortController | null = null;
let stopDrag: (() => void) | null = null;

const duration = computed(() => wave.value?.duration ?? 0);
const startNumber = computed(() => Number(start.value));
const endNumber = computed(() => Number(end.value));
const rangeError = computed(() =>
  !wave.value ||
  start.value === '' ||
  end.value === '' ||
  !Number.isFinite(startNumber.value) ||
  !Number.isFinite(endNumber.value) ||
  startNumber.value < 0 ||
  endNumber.value > wave.value.duration ||
  endNumber.value <= startNumber.value
    ? t('Choose a valid start and end time.')
    : '',
);
const selectionDuration = computed(() =>
  rangeError.value ? 0 : endNumber.value - startNumber.value,
);
const trimmed = computed(
  () => !!wave.value && (startNumber.value > 0 || endNumber.value < duration.value - 1e-9),
);
const busy = computed(() => pending.value || props.analyzing);
const startPercent = computed(() => percent(startNumber.value));
const endPercent = computed(() => percent(endNumber.value));
const positionPercent = computed(() => percent(position.value));
const rangeErrorId = `${title}-range-error`;
const startShadeStyle = computed(() => ({ width: `${startPercent.value}%` }));
const endShadeStyle = computed(() => ({ width: `${100 - endPercent.value}%` }));
const selectionStyle = computed(() => ({
  left: `${startPercent.value}%`,
  width: `${Math.max(0, endPercent.value - startPercent.value)}%`,
}));
const playheadStyle = computed(() => ({ left: `${positionPercent.value}%` }));
const startHandleStyle = computed(() => ({ left: `${startPercent.value}%` }));
const endHandleStyle = computed(() => ({ left: `${endPercent.value}%` }));
const path = computed(
  () =>
    health.value?.peaks
      .map(
        (value, index) =>
          `M${index},${50 - Math.min(1, value) * 48}v${Math.min(1, value) * 96}`,
      )
      .join(' ') || '',
);

function percent(value: number): number {
  if (!duration.value || !Number.isFinite(value)) return 0;
  return Math.min(100, Math.max(0, (value / duration.value) * 100));
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function round(value: number): number {
  return Math.round(value * 1000) / 1000;
}

function format(seconds: number): string {
  const value = Math.max(0, Math.floor(seconds));
  return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, '0')}`;
}

function createAudioUrl(file: File): string {
  if (typeof URL === 'undefined' || typeof URL.createObjectURL !== 'function') return '';
  return URL.createObjectURL(file);
}

function releaseAudioUrl() {
  if (!audioUrl.value || typeof URL === 'undefined' || typeof URL.revokeObjectURL !== 'function') return;
  URL.revokeObjectURL(audioUrl.value);
  audioUrl.value = '';
}

async function load(file: File) {
  controller?.abort();
  const current = new AbortController();
  controller = current;
  stopPlayback();
  position.value = 0;
  playingRange.value = null;
  pending.value = true;
  error.value = null;
  wave.value = null;
  health.value = null;
  try {
    if (file.size > props.maxBytes || !file.size)
      throw new Error(t('Reference must be 1–{max} bytes.', { max: props.maxBytes }));
    const buffer = await file.arrayBuffer();
    current.signal.throwIfAborted();
    const parsed = inspectWave(buffer);
    wave.value = parsed;
    start.value = 0;
    end.value = parsed.duration;
    health.value = await analyzeWave(parsed, current.signal);
  } catch (failure) {
    if (!current.signal.aborted) error.value = errorMessage(failure);
  } finally {
    if (controller === current) pending.value = false;
  }
}

watch(
  () => props.file,
  (file) => {
    releaseAudioUrl();
    audioUrl.value = createAudioUrl(file);
    void load(file);
  },
  { immediate: true },
);
watch(
  () => props.disabled,
  (value) => {
    if (value) stopPlayback();
  },
);
watch([start, end], () => {
  if (playing.value) stopPlayback();
});

function stopPlayback() {
  if (playbackAvailable()) audio.value?.pause();
  playing.value = false;
  playingRange.value = null;
}

function playbackAvailable(): boolean {
  return typeof window !== 'undefined' && typeof window.AudioContext === 'function';
}

function playRange(from: number, to: number) {
  const element = audio.value;
  if (!element || !playbackAvailable() || rangeError.value) return;
  try {
    element.currentTime = clamp(from, 0, duration.value);
    position.value = element.currentTime;
    playingRange.value = [from, clamp(to, from, duration.value)];
    void element.play().catch(() => {
      playing.value = false;
    });
  } catch {
    playing.value = false;
  }
}

function togglePlayback() {
  if (playing.value) {
    stopPlayback();
    return;
  }
  playRange(startNumber.value, endNumber.value);
}

function playFullRecording() {
  stopPlayback();
  playRange(0, duration.value);
}

function onTimeUpdate() {
  const element = audio.value;
  if (!element) return;
  position.value = element.currentTime;
  const range = playingRange.value;
  if (range && element.currentTime >= range[1]) {
    element.pause();
    try {
      element.currentTime = range[0];
    } catch {
      // Some browsers refuse seeks until metadata is ready; the playhead simply stays.
    }
    position.value = range[0];
    playingRange.value = null;
  }
}

function timeFromClientX(clientX: number): number {
  const rect = track.value?.getBoundingClientRect();
  if (!rect?.width || !duration.value) return 0;
  return round(clamp((clientX - rect.left) / rect.width, 0, 1) * duration.value);
}

function beginDrag(kind: 'start' | 'end' | 'move', event: PointerEvent) {
  if (props.disabled || !wave.value || rangeError.value || event.button !== 0) return;
  event.preventDefault();
  stopPlayback();
  const originX = event.clientX;
  const initialStart = startNumber.value;
  const initialEnd = endNumber.value;
  const width = initialEnd - initialStart;
  const move = (moveEvent: PointerEvent) => {
    if (kind === 'start') {
      start.value = clamp(
        timeFromClientX(moveEvent.clientX),
        0,
        round(endNumber.value - MIN_TRIM),
      );
      return;
    }
    if (kind === 'end') {
      end.value = clamp(
        timeFromClientX(moveEvent.clientX),
        round(startNumber.value + MIN_TRIM),
        duration.value,
      );
      return;
    }
    const rect = track.value?.getBoundingClientRect();
    if (!rect?.width) return;
    const delta = ((moveEvent.clientX - originX) / rect.width) * duration.value;
    const nextStart = round(clamp(initialStart + delta, 0, Math.max(0, duration.value - width)));
    start.value = nextStart;
    end.value = round(nextStart + width);
  };
  const finish = () => {
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', finish);
    window.removeEventListener('pointercancel', finish);
    stopDrag = null;
    if (!rangeError.value) position.value = startNumber.value;
  };
  window.addEventListener('pointermove', move);
  window.addEventListener('pointerup', finish);
  window.addEventListener('pointercancel', finish);
  stopDrag = finish;
}

function onTrackPointerDown(event: PointerEvent) {
  if (props.disabled || !wave.value || rangeError.value || event.button !== 0) return;
  const time = timeFromClientX(event.clientX);
  if (time >= endNumber.value) {
    end.value = clamp(time, round(startNumber.value + MIN_TRIM), duration.value);
    beginDrag('end', event);
  } else if (time <= startNumber.value) {
    start.value = clamp(time, 0, round(endNumber.value - MIN_TRIM));
    beginDrag('start', event);
  }
}

function nudge(kind: 'start' | 'end', delta: number) {
  if (props.disabled || !wave.value) return;
  if (kind === 'start')
    start.value = clamp(round(startNumber.value + delta), 0, round(endNumber.value - MIN_TRIM));
  else
    end.value = clamp(
      round(endNumber.value + delta),
      round(startNumber.value + MIN_TRIM),
      duration.value,
    );
}

function onHandleKeydown(kind: 'start' | 'end', event: KeyboardEvent) {
  if (props.disabled || !wave.value || rangeError.value) return;
  const step = event.shiftKey ? 0.1 : 0.01;
  let delta = 0;
  let target: number | null = null;
  if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') delta = -step;
  else if (event.key === 'ArrowRight' || event.key === 'ArrowUp') delta = step;
  else if (event.key === 'PageDown') delta = -1;
  else if (event.key === 'PageUp') delta = 1;
  else if (event.key === 'Home') target = 0;
  else if (event.key === 'End') target = duration.value;
  else return;
  event.preventDefault();
  stopPlayback();
  if (target !== null) {
    if (kind === 'start') start.value = clamp(target, 0, round(endNumber.value - MIN_TRIM));
    else end.value = clamp(target, round(startNumber.value + MIN_TRIM), duration.value);
    return;
  }
  nudge(kind, delta);
}

function croppedFile(): File | undefined {
  if (!wave.value || rangeError.value) return;
  return new File(
    [cropWave(wave.value, startNumber.value, endNumber.value)],
    `${props.file.name.replace(/\.wav$/i, '')}-trim.wav`,
    { type: 'audio/wav' },
  );
}

function apply() {
  const file = croppedFile();
  if (file) emit('apply', file);
}

function exportSelection() {
  const file = croppedFile();
  if (!file) return;
  const url = URL.createObjectURL(file);
  const link = document.createElement('a');
  link.href = url;
  link.download = file.name;
  link.click();
  // The download owns this URL until the browser has taken the file.
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

function valueText(value: number): string {
  return `${round(value).toFixed(3)} s`;
}

onBeforeUnmount(() => {
  stopDrag?.();
  controller?.abort();
  stopPlayback();
  releaseAudioUrl();
});
</script>
<template>
  <section
    class="reference-editor"
    :aria-labelledby="title"
    :aria-busy="busy"
    :data-disabled="disabled || undefined"
  >
    <header class="reference-editor-head">
      <div>
        <h3 :id="title">{{ t('Reference editor') }}</h3>
        <p class="reference-hint">
          {{ t('Drag the handles, or click the waveform to move the nearest edge.') }}
        </p>
      </div>
      <span v-if="wave" class="reference-length">{{ wave.duration.toFixed(2) }} s</span>
    </header>

    <p v-if="busy" class="reference-busy" role="status">{{ t('Analyzing reference…') }}</p>
    <p v-if="error" role="alert" class="field-error">{{ renderMessage(error) }}</p>

    <template v-if="wave && health">
      <div class="reference-transport">
        <button
          type="button"
          class="player-toggle"
          :aria-label="playing ? t('Pause') : t('Preview selection')"
          :title="playing ? t('Pause') : t('Preview selection')"
          :aria-pressed="playing"
          :disabled="disabled || !!rangeError"
          @click="togglePlayback"
        >
          <svg v-if="playing" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <rect x="7" y="5" width="3.5" height="14" rx="1" />
            <rect x="13.5" y="5" width="3.5" height="14" rx="1" />
          </svg>
          <svg v-else viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M8 5.5v13a1 1 0 0 0 1.5.87l11-6.5a1 1 0 0 0 0-1.74l-11-6.5A1 1 0 0 0 8 5.5Z" />
          </svg>
        </button>
        <p class="reference-time" aria-hidden="true">
          <strong>{{ format(position) }}</strong> / {{ format(duration) }}
        </p>
        <button
          v-if="trimmed"
          type="button"
          class="icon-button"
          :aria-label="t('Play full recording')"
          :title="t('Play full recording')"
          :disabled="disabled"
          @click="playFullRecording"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M4 5v14" />
            <path d="m20 5-11 7 11 7V5Z" />
          </svg>
        </button>
        <span v-if="!busy && status" class="status-pill" :class="status.kind" role="status">
          {{ status.text }}
        </span>
      </div>

      <div ref="track" class="reference-track" @pointerdown="onTrackPointerDown">
        <div class="reference-track-fill">
          <svg
            viewBox="0 0 512 100"
            preserveAspectRatio="none"
            role="img"
            :aria-label="t('Waveform of the original recording')"
            class="audio-waveform"
          >
            <title>{{ t('Waveform of the original recording') }}</title>
            <path :d="path" fill="none" stroke="currentColor" stroke-width="1" />
          </svg>
          <span class="trim-shade" :style="startShadeStyle" />
          <span class="trim-shade" :style="endShadeStyle" />
          <span
            class="trim-selection"
            :style="selectionStyle"
            :title="t('Drag to move the selection')"
            @pointerdown.stop="beginDrag('move', $event)"
          />
          <span class="trim-playhead" :style="playheadStyle" />
        </div>
        <span
          class="trim-handle start"
          role="slider"
          :tabindex="disabled ? -1 : 0"
          :aria-label="t('Trim start handle')"
          aria-valuemin="0"
          :aria-valuemax="wave.duration"
          :aria-valuenow="Number.isFinite(startNumber) ? round(startNumber) : 0"
          :aria-valuetext="valueText(startNumber)"
          :title="valueText(startNumber)"
          :style="startHandleStyle"
          @pointerdown.stop="beginDrag('start', $event)"
          @keydown="onHandleKeydown('start', $event)"
        />
        <span
          class="trim-handle end"
          role="slider"
          :tabindex="disabled ? -1 : 0"
          :aria-label="t('Trim end handle')"
          aria-valuemin="0"
          :aria-valuemax="wave.duration"
          :aria-valuenow="Number.isFinite(endNumber) ? round(endNumber) : 0"
          :aria-valuetext="valueText(endNumber)"
          :title="valueText(endNumber)"
          :style="endHandleStyle"
          @pointerdown.stop="beginDrag('end', $event)"
          @keydown="onHandleKeydown('end', $event)"
        />
      </div>

      <p class="reference-selection-readout">
        {{ t('Selected duration: {seconds} s', { seconds: rangeError ? '—' : selectionDuration.toFixed(3) }) }}
      </p>

      <div class="reference-range">
        <label>
          {{ t('Trim start (seconds)') }}
          <input
            v-model.number="start"
            type="number"
            min="0"
            :max="wave.duration"
            step="0.001"
            :disabled="disabled"
            :aria-invalid="!!rangeError || undefined"
            :aria-describedby="rangeError ? rangeErrorId : undefined"
            @keydown.enter.prevent
          />
        </label>
        <label>
          {{ t('Trim end (seconds)') }}
          <input
            v-model.number="end"
            type="number"
            min="0"
            :max="wave.duration"
            step="0.001"
            :disabled="disabled"
            :aria-invalid="!!rangeError || undefined"
            :aria-describedby="rangeError ? rangeErrorId : undefined"
            @keydown.enter.prevent
          />
        </label>
      </div>
      <p v-if="rangeError" :id="rangeErrorId" role="alert" class="field-error">
        {{ rangeError }}
      </p>

      <dl class="stat-grid">
        <div><dt>{{ t('Peak') }}</dt><dd>{{ health.peak.toFixed(4) }}</dd></div>
        <div><dt>{{ t('RMS') }}</dt><dd>{{ health.rms.toFixed(4) }}</dd></div>
        <div><dt>{{ t('Clipping') }}</dt><dd>{{ (health.clipping * 100).toFixed(2) }}%</dd></div>
        <div><dt>{{ t('Near silence') }}</dt><dd>{{ (health.silence * 100).toFixed(2) }}%</dd></div>
      </dl>

      <details class="health-notes">
        <summary>{{ t('Recording health notes') }}</summary>
        <p class="help-note">
          {{ t('Select original sample frames without resampling or changing encoding. Your original file is kept until you apply a trim. Export omits ancillary WAV metadata.') }}
        </p>
        <p class="help-note">{{ t('Local measurements cover original channels. Server measurements use the mono mix.') }}</p>
        <p v-if="health.silence > 0.95" role="status" class="help-note">
          {{ t('The reference recording is nearly silent. Choose a clearer recording.') }}
        </p>
        <p v-if="health.clipping > 0.1" role="status" class="help-note">
          {{ t('The reference recording has heavy clipping. Try recording at a lower input level.') }}
        </p>
        <p v-if="health.rms < 0.01 && health.silence <= 0.95" class="help-note">
          {{ t('The reference recording is unusually quiet. Increase the recording level.') }}
        </p>
        <p class="help-note">{{ t('Quality advice does not impose an additional upload restriction.') }}</p>
      </details>

      <div class="reference-actions">
        <button
          type="button"
          class="primary btn-with-icon"
          :title="t('Use trimmed recording')"
          :disabled="disabled || !!rangeError"
          @click="apply"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="m5 12 4 4L19 6" />
          </svg>
          {{ t('Use trimmed recording') }}
        </button>
        <button
          type="button"
          class="btn-with-icon"
          :title="t('Export selected WAV')"
          :disabled="disabled || !!rangeError"
          @click="exportSelection"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M12 4v11" />
            <path d="m7.5 10.5 4.5 4.5 4.5-4.5" />
            <path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2" />
          </svg>
          {{ t('Export selected WAV') }}
        </button>
        <button
          v-if="canRestore"
          type="button"
          class="icon-button"
          :aria-label="t('Restore original recording')"
          :title="t('Restore original recording')"
          :disabled="disabled"
          @click="emit('restore')"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M4 9V5h4" />
            <path d="M4.6 13a8 8 0 1 0 2-7.5L4 9" />
          </svg>
        </button>
      </div>
    </template>

    <audio
      ref="audio"
      :src="audioUrl"
      preload="metadata"
      hidden
      @play="playing = true"
      @pause="playing = false"
      @ended="playing = false"
      @timeupdate="onTimeUpdate"
    />
  </section>
</template>
