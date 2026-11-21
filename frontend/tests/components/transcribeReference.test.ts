import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import CloneReferenceStep from '../../src/components/voice/CloneReferenceStep.vue';

const mock = vi.hoisted(() => ({ studio: null as any }));
vi.mock('../../src/composables/useStudio', () => ({ useStudio: () => mock.studio }));

const file = new File([new Uint8Array(44)], 'ref.wav', { type: 'audio/wav' });
const props = {
  disabled: false,
  provider: 'reverb-tts',
  modes: ['icl'],
  file,
  originalFile: null,
  metadata: null,
  limits: null,
  errors: {},
  needsTranscript: true,
  transcript: '',
  mode: 'icl',
};

beforeEach(() => {
  mock.studio = {
    api: {
      prepareAsr: vi.fn(async () => ({ state: 'ready' })),
      transcribe: vi.fn(async () => ({
        text: 'Hola mundo.',
        language: 'es',
        duration: 1,
        model: 'fluid-speech-pro',
      })),
    },
  };
});

function mountStep() {
  return mount(CloneReferenceStep, { props: { ...props } });
}

describe('reference transcription', () => {
  it('sends the WAV, fills the transcript and keeps the field editable', async () => {
    const wrapper = mountStep();
    await wrapper.get('.transcript-actions button').trigger('click');
    await flushPromises();
    expect(mock.studio.api.prepareAsr).toHaveBeenCalledOnce();
    expect(mock.studio.api.transcribe).toHaveBeenCalledWith(file);
    expect(wrapper.emitted('update:transcript')?.[0]).toEqual(['Hola mundo.']);
    expect(wrapper.get('#cloneTranscript').attributes('readonly')).toBeUndefined();
  });

  it('shows a short error and stays usable when transcription fails', async () => {
    mock.studio.api.transcribe.mockRejectedValueOnce(new Error('Audio too long'));
    const wrapper = mountStep();
    await wrapper.get('.transcript-actions button').trigger('click');
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain('Audio too long');
    expect(wrapper.get('.transcript-actions button').attributes('disabled')).toBeUndefined();
  });

  it('shows the preparing label while the model downloads', async () => {
    mock.studio.api.prepareAsr.mockReturnValueOnce(new Promise(() => {}));
    const wrapper = mountStep();
    await wrapper.get('.transcript-actions button').trigger('click');
    await flushPromises();
    expect(wrapper.get('.transcript-actions button').text()).toBe('Preparing model…');
    expect(wrapper.get('.transcript-actions button').attributes('disabled')).toBeDefined();
    expect(mock.studio.api.transcribe).not.toHaveBeenCalled();
  });
});
