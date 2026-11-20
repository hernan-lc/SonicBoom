import type { Guide } from './types';

export const guides: Guide[] = [
  {
    id: 'first-audio',
    route: '#/create',
    titleKey: 'Your first audio',
    summaryKey: 'Write text, pick a voice and language, then generate and play your audio.',
    seconds: 60,
    steps: [
      {
        target: '[data-guide="first-audio-text"]',
        titleKey: 'Write your text',
        textKey: 'Type what you want SonicBoom to say in this box.',
        side: 'bottom',
      },
      {
        target: '[data-guide="first-audio-voice"]',
        titleKey: 'Pick a voice',
        textKey: 'Choose the model and voice that fit your recording.',
        side: 'bottom',
      },
      {
        target: '[data-guide="first-audio-language"]',
        titleKey: 'Choose a language',
        textKey: 'Select the language that matches your text.',
        side: 'bottom',
      },
      {
        target: '[data-guide="first-audio-generate"]',
        titleKey: 'Generate speech',
        textKey: 'Press Generate speech to hear your result.',
        side: 'top',
      },
      {
        target: '[data-guide="first-audio-result"]',
        titleKey: 'Play or download',
        textKey: 'Use the player to listen, then download the audio file.',
        side: 'left',
      },
    ],
  },
  {
    id: 'tokens-api',
    route: '#/settings',
    titleKey: 'API tokens',
    summaryKey: 'Find your API token, create one in admin, and paste it into the studio.',
    seconds: 60,
    steps: [
      {
        target: '[data-guide="tokens-api-access"]',
        titleKey: 'Find API access',
        textKey: 'Open the admin panel to manage API tokens.',
        side: 'bottom',
      },
      {
        target: '[data-guide="tokens-api-create"]',
        titleKey: 'Create a token',
        textKey: 'Generate a token there and copy it right away.',
        side: 'bottom',
      },
      {
        target: '[data-guide="tokens-api-token"]',
        titleKey: 'Use the token',
        textKey: 'Paste the token here to authenticate your requests.',
        side: 'bottom',
      },
    ],
  },
  {
    id: 'key-settings',
    route: '#/settings',
    titleKey: 'Key settings',
    summaryKey: "Search settings, reveal internal keys, and read each setting's apply badge.",
    seconds: 60,
    steps: [
      {
        target: '[data-guide="key-settings-search"]',
        titleKey: 'Search settings',
        textKey: 'Type a name here to find the setting you need.',
        side: 'bottom',
      },
      {
        target: '[data-guide="key-settings-category"]',
        titleKey: 'Browse categories',
        textKey: 'Use the category filter when you do not know the name.',
        side: 'bottom',
      },
      {
        target: '[data-guide="key-settings-keys"]',
        titleKey: 'Reveal keys',
        textKey: 'Show internal keys to search by exact config path.',
        side: 'bottom',
      },
      {
        target: '[data-guide="key-settings-badge"]',
        titleKey: 'Read the badge',
        textKey: 'The badge shows how and when a change takes effect.',
        side: 'right',
      },
      {
        target: '[data-guide="key-settings-save"]',
        titleKey: 'Save a change',
        textKey: 'Edit a value, then press Save to apply it.',
        side: 'top',
      },
    ],
  },
];
