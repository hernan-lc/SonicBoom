import { test } from 'node:test';
import assert from 'node:assert/strict';
import { startGuide } from '../src/guides/runner.ts';
import { guides } from '../src/guides/registry.ts';
import { completedGuides } from '../src/guides/progress.ts';
import { setLocale } from '../src/i18n/index.ts';

const doneKey = 'sonicboom.guides.done';

type Config = {
  steps: { element: string; popover: Record<string, unknown> }[];
  animate: boolean;
  nextBtnText: string;
  prevBtnText: string;
  doneBtnText: string;
  closeBtnLabel: string;
  onDestroyed?: (element: unknown, step: unknown, options: { reason: string }) => void;
};

interface HarnessOptions {
  available?: string[];
  reason?: string;
  hash?: string;
  reduced?: boolean;
  waitMs?: number;
}

function harness(options: HarnessOptions = {}) {
  const available = options.available ?? guides.flatMap((guide) => guide.steps.map((step) => step.target));
  const elements = new Map(available.map((target) => [target, { target }]));
  const captured: Config[] = [];
  const drives: number[] = [];
  const doc = { querySelector: (selector: string) => elements.get(selector) ?? null };
  const location = { hash: options.hash ?? '#/guides' };
  const loadDriver = async () => ({
    driver(config: Config) {
      captured.push(config);
      return {
        drive() {
          drives.push(1);
          config.onDestroyed?.(undefined, undefined, { reason: options.reason ?? 'done' });
        },
      };
    },
  });
  return {
    captured,
    drives,
    location,
    deps: {
      doc,
      location,
      loadDriver,
      isVisible: () => true,
      sleep: async () => {},
      matchMedia: () => ({ matches: options.reduced ?? false }),
      waitMs: options.waitMs ?? 100,
    },
  };
}

test('unknown guide ids do not start a tour', async () => {
  const h = harness();
  assert.equal(await startGuide('missing', h.deps), false);
  assert.equal(h.drives.length, 0);
});

test('navigates to the guide route and starts only with a visible first target', async () => {
  localStorage.removeItem(doneKey);
  const h = harness();
  assert.equal(await startGuide('first-audio', h.deps), true);
  assert.equal(h.location.hash, '#/create');
  assert.equal(h.drives.length, 1);
  assert.equal(h.captured[0].steps.length, 5);
  assert.deepEqual(completedGuides(), ['first-audio']);
});

test('a missing first target ends the guide without loading driver.js', async () => {
  const h = harness({ available: [], waitMs: 0 });
  assert.equal(await startGuide('first-audio', h.deps), false);
  assert.equal(h.location.hash, '#/create');
  assert.equal(h.drives.length, 0);
  assert.equal(h.captured.length, 0);
});

test('steps without a target are ignored and the rest still run', async () => {
  localStorage.removeItem(doneKey);
  const h = harness({ available: ['[data-guide="first-audio-text"]', '[data-guide="first-audio-generate"]'] });
  assert.equal(await startGuide('first-audio', h.deps), true);
  assert.deepEqual(
    h.captured[0].steps.map((step) => step.element),
    ['[data-guide="first-audio-text"]', '[data-guide="first-audio-generate"]'],
  );
  assert.deepEqual(completedGuides(), ['first-audio']);
});

test('reduced motion disables animation and buttons use the active locale', async () => {
  localStorage.removeItem(doneKey);
  const h = harness({ reduced: true });
  setLocale('es');
  try {
    assert.equal(await startGuide('key-settings', h.deps), true);
    const config = h.captured[0];
    assert.equal(config.animate, false);
    assert.equal(config.nextBtnText, 'Siguiente');
    assert.equal(config.prevBtnText, 'Anterior');
    assert.equal(config.doneBtnText, 'Hecho');
    assert.equal(config.closeBtnLabel, 'Cerrar');
  } finally {
    setLocale('en');
  }
  assert.equal(h.captured[0].animate, false);
});

test('closing early does not mark the guide as completed', async () => {
  localStorage.removeItem(doneKey);
  const h = harness({ reason: 'close' });
  assert.equal(await startGuide('tokens-api', h.deps), false);
  assert.deepEqual(completedGuides(), []);
});

test('a failing driver import is handled without rejecting', async () => {
  const h = harness();
  const deps = { ...h.deps, loadDriver: async () => { throw new Error('offline'); } };
  assert.equal(await startGuide('first-audio', deps), false);
  assert.equal(h.drives.length, 0);
});
