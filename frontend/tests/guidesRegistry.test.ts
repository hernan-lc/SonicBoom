import { test } from 'node:test';
import assert from 'node:assert/strict';
import { guides } from '../src/guides/registry.ts';
import en from '../src/i18n/en.json' with { type: 'json' };
import es from '../src/i18n/es.json' with { type: 'json' };

const catalogs = [
  ['en', en],
  ['es', es],
] as const;
const words = (text: string) => text.trim().split(/\s+/).filter(Boolean).length;

test('guide ids and step targets are unique', () => {
  const guideIds = guides.map((guide) => guide.id);
  assert.equal(new Set(guideIds).size, guideIds.length);
  const targets = guides.flatMap((guide) => guide.steps.map((step) => step.target));
  assert.equal(new Set(targets).size, targets.length);
  for (const target of targets) assert.match(target, /^\[data-guide="[a-z0-9-]+"\]$/);
});

test('each guide has three to five steps on a hash route', () => {
  assert.equal(guides.length, 3);
  for (const guide of guides) {
    assert.match(guide.route, /^#\//);
    assert.ok(guide.steps.length >= 3 && guide.steps.length <= 5, `${guide.id} has ${guide.steps.length} steps`);
    assert.ok(guide.seconds > 0);
  }
});

test('step titles, step texts and summaries fit the word budget', () => {
  for (const guide of guides) {
    for (const [name, catalog] of catalogs) {
      assert.ok(words(catalog[guide.summaryKey as keyof typeof catalog] ?? '') <= 14, `${name}: ${guide.summaryKey}`);
      for (const step of guide.steps) {
        assert.ok(words(catalog[step.titleKey as keyof typeof catalog] ?? '') <= 4, `${name}: ${step.titleKey}`);
        assert.ok(words(catalog[step.textKey as keyof typeof catalog] ?? '') <= 12, `${name}: ${step.textKey}`);
      }
    }
  }
});

test('every message used by the guides exists in both catalogs', () => {
  const keys = guides.flatMap((guide) => [
    guide.titleKey,
    guide.summaryKey,
    ...guide.steps.flatMap((step) => [step.titleKey, step.textKey]),
  ]);
  for (const key of keys) {
    for (const [name, catalog] of catalogs) {
      const value = catalog[key as keyof typeof catalog];
      assert.equal(typeof value, 'string', `${name} is missing ${key}`);
      assert.ok(value, `${name} has an empty ${key}`);
    }
  }
});
