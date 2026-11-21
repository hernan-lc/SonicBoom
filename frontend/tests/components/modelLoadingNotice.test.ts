import { beforeEach, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { nextTick, ref, shallowRef } from 'vue';
import ModelLoadingNotice from '../../src/components/ModelLoadingNotice.vue';
import { setLocale } from '../../src/i18n';

const mock = vi.hoisted(() => ({
  providers: null as ReturnType<typeof ref> | null,
  info: null as ReturnType<typeof shallowRef> | null,
  models: null as ReturnType<typeof ref> | null,
}));
vi.mock('../../src/composables/useStudio', () => ({
  useStudio: () => ({ providers: mock.providers, info: mock.info, models: mock.models }),
}));

function set(providers: unknown[], modelStatus: string | null = null) {
  mock.providers = ref(providers);
  mock.info = shallowRef(modelStatus ? { model: { status: modelStatus } } : null);
  mock.models = ref([]);
}

const loading = {
  provider_id: 'reverb-tts',
  enabled: true,
  state: 'loading',
  detail: 'reverb-tts files verifying (79% complete)',
};

beforeEach(() => setLocale('en'));

it('shows a localized toast with progress for a loading engine', () => {
  set([loading]);
  const notice = mount(ModelLoadingNotice);
  expect(notice.find('.loading-notice').exists()).toBe(true);
  expect(notice.text()).toContain('Model loading');
  expect(notice.text()).toContain('reverb-tts');
  expect(notice.text()).toContain('Verifying files: 79%');
  expect(notice.get('progress').attributes('value')).toBe('79');
  expect(notice.get('a.btn-inline').attributes('href')).toBe('#/engines');
});

it('hides the toast once every engine is ready', async () => {
  set([loading]);
  const notice = mount(ModelLoadingNotice);
  (mock.providers as ReturnType<typeof ref>).value = [{ ...loading, state: 'ready', detail: null }];
  await nextTick();
  expect(notice.find('.loading-notice').exists()).toBe(false);
  notice.unmount();
});

it('includes the default model in the toast while it downloads', () => {
  set([{ ...loading, state: 'ready' }], 'downloading');
  const notice = mount(ModelLoadingNotice);
  expect(notice.text()).toContain('Downloading model…');
  expect(notice.findAll('.loading-notice-item')).toHaveLength(1);
});

it('localizes the toast in Spanish', () => {
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

it('renders the header badge from the model status', async () => {
  set([], 'loading');
  const notice = mount(ModelLoadingNotice, { props: { mode: 'badge' } });
  expect(notice.get('.model-status').text()).toBe('Loading model...');
  (mock.info as ReturnType<typeof shallowRef>).value = { model: { status: 'ready' } };
  await nextTick();
  expect(notice.find('.model-status').exists()).toBe(false);
  notice.unmount();
});

it('renders one engine inline without a raw diagnostic string', () => {
  set([loading]);
  const notice = mount(ModelLoadingNotice, { props: { mode: 'inline', engine: loading } });
  expect(notice.text()).toContain('Verifying files: 79%');
  expect(notice.text()).not.toContain('reverb-tts files verifying');
  expect(notice.get('progress').attributes('value')).toBe('79');
  notice.unmount();
});

it('keeps non-loading diagnostics as plain text inline', () => {
  const failed = { ...loading, state: 'error', detail: 'model unavailable' };
  const notice = mount(ModelLoadingNotice, { props: { mode: 'inline', engine: failed } });
  expect(notice.get('.engine-detail').text()).toBe('model unavailable');
  notice.unmount();
});

it('offers Manage engines for a model that is not ready', async () => {
  set([]);
  const notice = mount(ModelLoadingNotice, { props: { mode: 'inline', model: 'reverb-tts' } });
  expect(notice.text()).toContain('Model reverb-tts is not ready — enable its engine first.');
  await notice.get('button').trigger('click');
  expect(notice.emitted('manage')).toHaveLength(1);
  notice.unmount();
});

it('shows loading progress instead of not-ready text when that model is loading', () => {
  set([loading], null);
  (mock.models as ReturnType<typeof ref>).value = [{ id: 'reverb-tts', provider: 'reverb-tts' }];
  const notice = mount(ModelLoadingNotice, { props: { mode: 'inline', model: 'reverb-tts' } });
  expect(notice.text()).toContain('Verifying files: 79%');
  expect(notice.text()).not.toContain('is not ready');
  notice.unmount();
});
