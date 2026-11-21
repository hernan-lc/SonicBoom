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

test('steps that are not on screen yet stay in the tour and Driver skips them if they never appear', async () => {
  localStorage.removeItem(doneKey);
  const h = harness({ available: ['[data-guide="first-audio-text"]', '[data-guide="first-audio-generate"]'] });
  assert.equal(await startGuide('first-audio', h.deps), true);
  const guide = guides.find((item) => item.id === 'first-audio')!;
  // The result player only exists after generating, so every step is kept for Driver to resolve.
  assert.deepEqual(h.captured[0].steps.map((step) => step.element), guide.steps.map((step) => step.target));
  assert.equal(h.captured[0].skipMissingElement, true);
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

interface OpensOptions {
  /** `visible`: the target is already on screen; `hidden`: pressing the opener reveals it; `never`: it never appears. */
  state: 'visible' | 'hidden' | 'never';
  step?: number;
}

/** A page where `voice-new` opens the dialog that holds the later steps, with a mocked Driver. */
function opensHarness({ state, step }: OpensOptions) {
  const opener = '[data-guide="voice-new"]';
  const modalSelectors = new Set(['[data-guide="clone-file"]', '[data-guide="clone-transcript"]', '[data-guide="clone-submit"]']);
  const elements = new Map<string, object>();
  for (const guide of guides) for (const item of guide.steps) elements.set(item.target, {});
  if (state !== 'visible') for (const selector of modalSelectors) elements.delete(selector);
  const clicks: string[] = [];
  const moves: number[] = [];
  const captured: Config[] = [];
  const location = { hash: '#/guides' };
  const doc = {
    querySelector: (selector: string) =>
      selector === opener
        ? { click: () => { clicks.push(opener); if (state === 'hidden') for (const m of modalSelectors) elements.set(m, {}); } }
        : (elements.get(selector) ?? null),
  };
  const loadDriver = async () => ({
    driver(config: Config) {
      captured.push(config);
      return { drive() {}, moveNext() { moves.push(1); } };
    },
  });
  const deps = {
    doc, location, loadDriver, isVisible: () => true, sleep: async () => {}, matchMedia: () => ({ matches: false }), waitMs: 100, step,
  };
  const guide = guides.find((item) => item.id === 'clone-voice')!;
  let result: Promise<boolean> = Promise.resolve(false);
  /** Starts the guide and returns once Driver has been created (the guide itself stays open). */
  async function begin() {
    result = startGuide('clone-voice', deps);
    while (!captured.length) await new Promise((resolve) => setTimeout(resolve, 0));
  }
  async function pressNext(index: number) {
    const hook = captured[0].steps![index].popover!.onNextClick as (e: undefined, s: object, o: { driver: { moveNext(): void } }) => Promise<void>;
    await hook(undefined, {}, { driver: { moveNext: () => moves.push(1) } });
  }
  return { clicks, moves, captured, location, begin, pressNext, guide, finish: async () => { captured[0].onDestroyed?.(undefined, undefined, { reason: 'done' } as never); await result; } };
}

test('nothing is pressed when the guide starts, so the first step still shows the opener', async () => {
  localStorage.removeItem(doneKey);
  const h = opensHarness({ state: 'hidden' });
  await h.begin();
  assert.deepEqual(h.clicks, [], 'the dialog must stay closed under step 1');
  assert.equal(h.location.hash, '#/voices');
  assert.deepEqual(h.captured[0].steps!.map((step) => step.element), h.guide.steps.map((step) => step.target));
  assert.equal(typeof h.captured[0].steps![0].popover!.onNextClick, 'function', 'the step before an opener presses it');
  assert.equal(h.captured[0].steps![1].popover!.onNextClick, undefined, 'only steps before an opener are intercepted');
  await h.finish();
});

test('moving to a step with opens presses the control, then advances', async () => {
  const h = opensHarness({ state: 'hidden' });
  await h.begin();
  await h.pressNext(0);
  assert.deepEqual(h.clicks, ['[data-guide="voice-new"]']);
  assert.equal(h.moves.length, 1);
  await h.finish();
});

test('an opener is not pressed when the target is already visible (no toggling a panel shut)', async () => {
  const h = opensHarness({ state: 'visible' });
  await h.begin();
  await h.pressNext(0);
  assert.deepEqual(h.clicks, []);
  assert.equal(h.moves.length, 1);
  await h.finish();
});

test('a target that never appears still lets the guide move on (Driver skips the missing step)', async () => {
  const h = opensHarness({ state: 'never' });
  await h.begin();
  await h.pressNext(0);
  assert.deepEqual(h.clicks, ['[data-guide="voice-new"]']);
  assert.equal(h.moves.length, 1);
  assert.equal(h.captured[0].skipMissingElement, true);
  await h.finish();
});

test('starting inside a dialog presses the closest earlier opener first', async () => {
  const h = opensHarness({ state: 'hidden', step: 2 });
  await h.begin();
  assert.deepEqual(h.clicks, ['[data-guide="voice-new"]']);
  assert.deepEqual(h.captured[0].steps!.map((step) => step.element), ['[data-guide="clone-transcript"]', '[data-guide="clone-submit"]']);
  await h.finish();
});

test('a collapsed panel is opened by the pronunciation guide and left alone when already open', async () => {
  const toggle = '[data-guide="pronunciation-toggle"]';
  const first = '[data-guide="pronunciation-mode"]';
  for (const open of [false, true]) {
    const clicks: string[] = [];
    const elements = new Map<string, object>();
    for (const guide of guides) for (const item of guide.steps) if (open || item.target !== first) elements.set(item.target, {});
    const doc = {
      querySelector: (selector: string) =>
        selector === toggle ? { click: () => { clicks.push(toggle); elements.set(first, {}); } } : (elements.get(selector) ?? null),
    };
    const captured: Config[] = [];
    const loadDriver = async () => ({ driver(config: Config) { captured.push(config); return { drive() { config.onDestroyed?.(undefined, undefined, { reason: 'done' } as never); }, moveNext() {} }; } });
    const result = await startGuide('pronunciation', { doc, location: { hash: '' }, loadDriver, isVisible: () => true, sleep: async () => {}, matchMedia: () => ({ matches: false }), waitMs: 100 });
    assert.equal(result, true);
    assert.deepEqual(clicks, open ? [] : [toggle]);
  }
});
