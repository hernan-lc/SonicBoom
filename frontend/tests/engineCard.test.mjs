import { test, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'vite';
import { createSSRApp, h, ref } from 'vue';
import { renderToString } from 'vue/server-renderer';

Object.defineProperty(globalThis, 'localStorage', { value: { getItem: () => null, setItem: () => {} }, configurable: true });
globalThis.document = { documentElement: { lang: '' }, title: '' };
const server = await createServer({ server: { middlewareMode: true }, appType: 'custom' });
after(() => server.close());
const { default: EngineCard } = await server.ssrLoadModule('/src/components/studio/EngineCard.vue');
const { studioKey } = await server.ssrLoadModule('/src/composables/useStudio.ts');
const { setLocale } = await server.ssrLoadModule('/src/i18n/index.ts');
// Locale is auto-detected from `navigator.language`; force English so these
// assertions are deterministic on non-English hosts (the Spanish case below
// switches explicitly and restores English).
setLocale('en');
const models = ['reverb-tts'];
const capabilities = { provider_id: 'reverb-tts', models, languages: ['de', 'en', 'es', 'fr', 'it', 'pl', 'pt'] };
async function render(provider) {
  const app = createSSRApp({ render: () => h(EngineCard, { provider, capabilities }) });
  app.provide(studioKey, {
    schema: ref({ 'providers.reverb_tts.enabled': {} }),
    config: ref({ config: { providers: { reverb_tts: { model_id: 'reverb-tts' } } } }),
  });
  return renderToString(app);
}

test('ready engine lists its model once, marks it active and offers Disable', async () => {
  const html = await render({ provider_id: 'reverb-tts', enabled: true, state: 'ready', active_model: models[0] });
  assert.equal((html.match(/<code>reverb-tts<\/code>/g) || []).length, 2, 'model shown in the header and in the list');
  assert.ok(html.includes('Active'));
  assert.ok(html.includes('>Disable</button>'));
  assert.ok(html.includes('7 audio languages'));
  assert.ok(!html.includes('Preparing model…'));
});
test('disabled engine offers Enable and shows no active model', async () => {
  const html = await render({ provider_id: 'reverb-tts', enabled: false, state: 'disabled' });
  assert.ok(html.includes('>Enable</button>'));
  assert.ok(html.includes('value="reverb-tts"') === false, 'no model picker remains');
  assert.ok(!html.includes('>Active</span>'));
  assert.ok(!html.includes('Preparing model…'));
});
test('loading engine reports that the model is being prepared', async () => {
  const html = await render({ provider_id: 'reverb-tts', enabled: true, state: 'loading', active_model: models[0] });
  assert.ok(html.includes('Preparing model…'));
});
test('Spanish loading help is localized', async () => {
  setLocale('es');
  try {
    const html = await render({ provider_id: 'reverb-tts', enabled: true, state: 'loading', active_model: models[0] });
    assert.ok(html.includes('Preparando el modelo…'));
  } finally { setLocale('en'); }
});
