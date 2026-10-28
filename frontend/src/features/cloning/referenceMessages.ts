import { t } from '../../i18n';

const messages: Record<string, string> = {
  reference_nearly_silent: 'The reference recording is nearly silent. Choose a clearer recording.',
  reference_heavily_clipped: 'The reference recording has heavy clipping. Try recording at a lower input level.',
  reference_large_dc_offset: 'The reference recording has a large DC offset. Check the recording source.',
  reference_non_finite_samples: 'The reference recording contains invalid (non-finite) samples and must be replaced.',
  reference_unusually_quiet: 'The reference recording is unusually quiet. Increase the recording level.',
  reference_ends_during_speech: 'The recording appears to end during speech. A little trailing silence may help cloning.',
  reference_possible_background_noise: 'The reference may contain background noise. A quieter recording is recommended.',
  reference_short_for_cloning: 'A 5-10 second recording usually clones better than this short clip.',
  reference_leading_silence: 'The recording starts with a long silence.',
  reference_trailing_silence: 'The recording ends with a long silence.',
};

/** Localize a reference-quality warning code, falling back to the raw code. */
export function referenceMessage(code: string): string {
  return t(messages[code] || code);
}
