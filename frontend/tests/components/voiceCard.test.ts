import { beforeEach, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { ref } from 'vue';
import VoiceCard from '../../src/components/studio/VoiceCard.vue';

const mock = vi.hoisted(() => ({ providers: null as ReturnType<typeof ref> | null }));
vi.mock('../../src/composables/useStudio', () => ({ useStudio: () => ({ providers: mock.providers }) }));

const voice = {
  id: 'v1',
  name: 'Old voice',
  provider: 'reverb-tts',
  model: 'reverb-tts',
  language: 'en',
  clone_mode: 'icl',
  reference_duration_secs: 5,
};

function render(provider = voice.provider) {
  return mount(VoiceCard, {
    props: {
      voice: { ...voice, provider },
      disabled: false,
      usable: true,
      compatible: true,
      modelReady: true,
    },
  });
}

beforeEach(() => {
  mock.providers = ref([{ provider_id: 'reverb-tts' }]);
});

it('marks retired providers and disables use and preview, but keeps delete', () => {
  for (const provider of ['qwen3-gguf', 'qwen3-candle', 'pocket-tts', 'gpt-sovits']) {
    const card = render(provider);
    expect(card.get('.status-dot').text()).toBe('Retired engine');
    expect(card.get('[data-guide="voice-preview"]').attributes('disabled')).toBeDefined();
    expect(card.findAll('button')[0].attributes('disabled')).toBeDefined();
    expect(card.get('[data-guide="voice-delete"]').attributes('disabled')).toBeUndefined();
    card.unmount();
  }
});

it('marks a profile whose engine is missing from the provider list', () => {
  const card = render('removed-engine');
  expect(card.get('.status-dot').text()).toBe('Retired engine');
  expect(card.get('[data-guide="voice-preview"]').attributes('disabled')).toBeDefined();
  card.unmount();
});

it('keeps voices from active providers usable', () => {
  const card = render();
  expect(card.get('.status-dot').text()).toBe('Ready');
  expect(card.get('[data-guide="voice-preview"]').attributes('disabled')).toBeUndefined();
  card.unmount();
});
