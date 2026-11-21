import { beforeEach, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { nextTick, ref, shallowRef } from 'vue';
import ModelLoadingNotice from '../../src/components/ModelLoadingNotice.vue';
import { setLocale } from '../../src/i18n';

const mock = vi.hoisted(() => ({
  providers: null as ReturnType<typeof ref> | null,
  info: null as ReturnType<typeof shallowRef> | null,
}));
vi.mock('../../src/composables/useStudio', () => ({
  useStudio: () => ({ providers: mock.providers, info: mock.info }),
}));

function set(providers: unknown[], modelStatus: string | null = null) {
  mock.providers = ref(providers);
  mock.info = shallowRef(modelStatus ? { model: { status: modelStatus } } : null);
}

const loading = {
  provider_id: 'reverb-tts',
  enabled: true,
  state: 'loading',
  detail: 'reverb-tts files verifying (79% complete)',
};

beforeEach(() => setLocale('en'));

it('shows a localized notice with progress for a loading engine', () => {
  set([loading]);
  const notice = mount(ModelLoadingNotice);
  expect(notice.find('.loading-notice').exists()).toBe(true);
  expect(notice.text()).toContain('Model loading');
  expect(notice.text()).toContain('reverb-tts');
  expect(notice.text()).toContain('Verifying files: 79%');
  expect(notice.get('progress').attributes('value')).toBe('79');
  expect(notice.get('a.btn-inline').attributes('href')).toBe('#/engines');
});

it('hides once every engine is ready', async () => {
  set([loading]);
  const notice = mount(ModelLoadingNotice);
  (mock.providers as ReturnType<typeof ref>).value = [{ ...loading, state: 'ready', detail: null }];
  await nextTick();
  expect(notice.find('.loading-notice').exists()).toBe(false);
  notice.unmount();
});

it('includes the default model while it downloads', () => {
  set([{ ...loading, state: 'ready' }], 'downloading');
  const notice = mount(ModelLoadingNotice);
  expect(notice.text()).toContain('Downloading model…');
  expect(notice.findAll('.loading-notice-item')).toHaveLength(1);
});

it('localizes labels and details in Spanish', () => {
  setLocale('es');
  set([loading]);
  const notice = mount(ModelLoadingNotice);
  expect(notice.text()).toContain('Cargando modelo');
  expect(notice.text()).toContain('Verificando archivos: 79%');
  notice.unmount();
});

it('stays dismissed during progress ticks and reopens for a new engine', async () => {
  set([loading]);
  const notice = mount(ModelLoadingNotice);
  await notice.get('.loading-notice-close').trigger('click');
  expect(notice.find('.loading-notice').exists()).toBe(false);
  (mock.providers as ReturnType<typeof ref>).value = [{ ...loading, detail: 'reverb-tts files verifying (95% complete)' }];
  await nextTick();
  expect(notice.find('.loading-notice').exists()).toBe(false);
  (mock.providers as ReturnType<typeof ref>).value = [loading, { ...loading, provider_id: 'other-engine' }];
  await nextTick();
  expect(notice.find('.loading-notice').exists()).toBe(true);
  notice.unmount();
});
