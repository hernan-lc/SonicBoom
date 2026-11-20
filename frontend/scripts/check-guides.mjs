import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const { guides } = await import(new URL('../src/guides/registry.ts', import.meta.url).href);

const english = JSON.parse(readFileSync(path.join(root, 'src/i18n/en.json'), 'utf8'));
const spanish = JSON.parse(readFileSync(path.join(root, 'src/i18n/es.json'), 'utf8'));

const guideKeys = guides.flatMap((guide) => [
  guide.titleKey,
  guide.summaryKey,
  ...guide.steps.flatMap((step) => [step.titleKey, step.textKey]),
]);
const uiKeys = [
  'Guides',
  'Short interactive walkthroughs.',
  'Pick a short guide and follow the highlights.',
  'Start',
  'Done',
  'Pending',
  '~{count} min',
  'Start guide: {title}',
  'Next',
  'Previous',
  'Close',
  '{current} of {total}',
];
const keys = [...new Set([...guideKeys, ...uiKeys])];

const source = path.join(root, 'src');
function vueFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) return vueFiles(full);
    return entry.name.endsWith('.vue') ? [full] : [];
  });
}
const markup = vueFiles(source)
  .map((file) => readFileSync(file, 'utf8'))
  .join('\n');
const declared = new Set([...markup.matchAll(/data-guide="([^"]+)"/g)].map((match) => match[1]));

const errors = [];
const guideIds = new Set();
const stepIds = new Set();
const words = (text) => text.trim().split(/\s+/).filter(Boolean).length;

for (const guide of guides) {
  if (guideIds.has(guide.id)) errors.push(`duplicate guide id: ${guide.id}`);
  guideIds.add(guide.id);
  for (const [index, step] of guide.steps.entries()) {
    const id = step.target.match(/^\[data-guide="([^"]+)"\]$/)?.[1];
    if (!id) errors.push(`${guide.id} step ${index}: invalid target ${step.target}`);
    else {
      if (stepIds.has(id)) errors.push(`duplicate step id: ${id}`);
      stepIds.add(id);
      if (!declared.has(id)) errors.push(`${guide.id}: no .vue declares data-guide="${id}"`);
    }
  }
}

for (const key of keys) {
  for (const [name, catalog] of [
    ['en.json', english],
    ['es.json', spanish],
  ]) {
    if (typeof catalog[key] !== 'string' || !catalog[key]) errors.push(`${name}: missing guide message: ${key}`);
  }
}

for (const guide of guides) {
  for (const [name, catalog] of [
    ['en.json', english],
    ['es.json', spanish],
  ]) {
    const summary = catalog[guide.summaryKey];
    if (typeof summary === 'string' && words(summary) > 14)
      errors.push(`${name}: summary over 14 words: ${guide.summaryKey}`);
    for (const step of guide.steps) {
      const title = catalog[step.titleKey];
      const text = catalog[step.textKey];
      if (typeof title === 'string' && words(title) > 4)
        errors.push(`${name}: step title over 4 words: ${step.titleKey}`);
      if (typeof text === 'string' && words(text) > 12)
        errors.push(`${name}: step text over 12 words: ${step.textKey}`);
    }
  }
}

if (errors.length) {
  console.error(errors.join('\n'));
  process.exit(1);
}
console.log(`${guides.length} guides: targets, messages and word budgets OK.`);
