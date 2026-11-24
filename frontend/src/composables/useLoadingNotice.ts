import { computed, type Ref, ref } from 'vue';
import { t } from '../i18n';
import type { AsrEngineStatus, ProviderStatus, ServerInfo } from '../services/contracts';

export interface LoadingItem {
  key: string;
  name: string;
  /** Localized for the default model, raw server diagnostic for an engine. */
  detail: string;
  /** Rounded percentage, when the server reports one. */
  progress: number | null;
}

function progressFrom(detail: string | null | undefined): number | null {
  const match = /(\d+(?:\.\d+)?)\s*%/.exec(detail ?? '');
  if (!match) return null;
  const value = Number(match[1]);
  return Number.isFinite(value) ? Math.min(100, Math.max(0, Math.round(value))) : null;
}

/** Engines that are still preparing a model, plus the default model itself. */
export function loadingItems(
  providers: ProviderStatus[],
  info: ServerInfo | null,
  asr?: AsrEngineStatus | null,
): LoadingItem[] {
  const items: LoadingItem[] = [];
  const modelStatus = info?.model.status;
  if (modelStatus === 'downloading' || modelStatus === 'loading') {
    items.push({
      key: 'model',
      name: t('Model'),
      detail: modelStatus === 'downloading' ? t('Downloading model…') : t('Loading model...'),
      progress: null,
    });
  }
  for (const provider of providers) {
    if (!provider.enabled || provider.state !== 'loading') continue;
    items.push({
      key: provider.provider_id,
      name: provider.provider_id,
      detail: provider.detail ?? '',
      progress: progressFrom(provider.detail),
    });
  }
  if (asr && (asr.state === 'downloading' || asr.state === 'loading')) {
    items.push({
      key: asr.id,
      name: t('Transcription'),
      detail: asr.detail ?? '',
      progress: asr.progress === undefined ? null : Math.round(asr.progress * 100),
    });
  }
  return items;
}

/** Short status label; unrecognized server diagnostics stay as text. */
export function loadingDetail(item: LoadingItem): string {
  if (item.key === 'model') return item.detail;
  const detail = item.detail;
  if (/\bdownload/i.test(detail)) {
    return item.progress === null
      ? t('Downloading files…')
      : t('Downloading files: {progress}%', { progress: item.progress });
  }
  if (/verif/i.test(detail)) {
    return item.progress === null
      ? t('Verifying files…')
      : t('Verifying files: {progress}%', { progress: item.progress });
  }
  if (/loading/i.test(detail)) return t('Loading model...');
  return detail ? t(detail) : t('Preparing model…');
}

/** Compact header label for the default model. Empty when nothing is pending. */
export function modelStatusLabel(status: string | null | undefined): string {
  if (!status || status === 'ready') return '';
  if (status === 'failed') return t('Model load failed. Check server logs.');
  if (status === 'downloading') return t('Downloading model…');
  if (status === 'loading') return t('Loading model...');
  return t('Preparing model…');
}

/** Single-line status for a voice or model that cannot generate yet. */
export function notReadyDetail(model: string): string {
  return t('Model {model} is not ready — enable its engine first.', { model });
}

/**
 * Drives the global notice for the whole studio. Dismissal is scoped to the
 * current set of busy engines: progress ticks do not reopen it, but a newly
 * loading engine does.
 */
export function useLoadingNotice(
  providers: Ref<ProviderStatus[]>,
  info: Ref<ServerInfo | null>,
  asr?: Ref<AsrEngineStatus | null>,
) {
  const items = computed(() => loadingItems(providers.value, info.value, asr?.value));
  const signature = computed(() =>
    items.value
      .map((item) => item.key)
      .sort()
      .join(','),
  );
  const dismissed = ref<string | null>(null);
  const visible = computed(() => items.value.length > 0 && dismissed.value !== signature.value);
  function dismiss() {
    dismissed.value = signature.value;
  }
  return { items, visible, dismiss };
}
