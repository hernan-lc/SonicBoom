import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const catalog = JSON.parse(readFileSync(path.join(root, 'src/i18n/en.json'), 'utf8'));
const allowlist = new Set(JSON.parse(readFileSync(path.join(root, 'scripts/ui-text-allowlist.json'), 'utf8')));
const limit = 90;

const longKeys = Object.keys(catalog).filter((key) => key.length > limit);
const offenders = longKeys.filter((key) => !allowlist.has(key));
if (offenders.length) {
  console.error('Long UI text keys are not allowed:');
  for (const key of offenders) console.error(`  (${key.length} chars) ${key}`);
  console.error(
    'Prefer an interactive guide (src/guides/registry.ts); allow this key only if it is an error or diagnostic message.',
  );
  process.exit(1);
}
console.log(`${longKeys.length} long UI messages checked; all are allowlisted.`);
