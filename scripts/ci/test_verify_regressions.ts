/** Regression checks for the security-regression manifest guard. */
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { REQUIRED, check, parseListing } from './verify_regressions.ts';

test('counts leaf names ignoring module path', () => {
  const text = [
    'Running unittests src/lib.rs',
    'api::queue::tests::cancellation_does_not_release_running_permit: test',
    'api::gate::tests::enqueue_rejects_beyond_bound_without_dropping: test',
    'benches::perf: bench',
    'Doc-tests sonicboom',
  ].join('\n');
  const counts = parseListing(text);
  assert.equal(counts.get('cancellation_does_not_release_running_permit'), 1);
  assert.equal(counts.get('enqueue_rejects_beyond_bound_without_dropping'), 1);
  assert.equal(counts.get('perf'), 1);
});

test('ignores non-test lines', () => {
  assert.equal(parseListing('foo: ok\nbar\n').size, 0);
});

test('all present passes', () => {
  const text = REQUIRED.map((name) => `tests::${name}: test`).join('\n');
  assert.equal(check(text), REQUIRED.length);
});

test('missing test fails', () => {
  const text = REQUIRED.slice(0, -1).map((name) => `tests::${name}: test`).join('\n');
  assert.throws(() => check(text), /missing regression tests/);
});

test('duplicate test fails', () => {
  const text = REQUIRED.map((name) => `tests::${name}: test`).join('\n')
    + `\nother::module::${REQUIRED[0]}: test`;
  assert.throws(() => check(text), /matched more than once/);
});

test('renamed test is reported missing', () => {
  const text = REQUIRED.map((name) => `tests::${name}: test`).join('\n')
    .replace('malformed_integers_fail_instead_of_defaulting', 'malformed_integers_fail');
  assert.throws(() => check(text), /malformed_integers_fail_instead_of_defaulting/);
});
